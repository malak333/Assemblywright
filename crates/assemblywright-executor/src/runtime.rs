use crate::{
    ExecutorAuthoritySnapshot, ExecutorIdentity, ExecutorPolicy, OwnedExecution,
    UnprivilegedProcessOperation,
};
use assemblywright_protocol::{
    ExecutionActionEnvelope, ExecutionHostPlatform, ExecutionTerminationMode,
    ExecutionTerminationReceipt, ProtectedControlPlanePathManifest,
};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;

pub const RUNTIME_SCHEMA_VERSION: u16 = 1;
#[cfg(not(windows))]
const MAX_FRAME_BYTES: usize = 1024 * 1024;
const MAX_TERMINATION_WINDOW_MS: u64 = 60_000;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RuntimeError {
    #[error("runtime configuration is invalid")]
    InvalidConfig,
    #[error("runtime request is unauthenticated or stale")]
    InvalidRequest,
    #[error("runtime is quarantined")]
    Quarantined,
    #[error("runtime framing failed")]
    Io,
    #[error("executor rejected the request")]
    Executor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutorRuntimeConfig {
    pub schema_version: u16,
    pub runtime_id: Uuid,
    pub runtime_revision: u64,
    pub platform: ExecutionHostPlatform,
    pub owner_uid: Option<u32>,
    pub next_request_sequence: u64,
    pub restart_quarantined: bool,
    pub executor_id: Uuid,
    pub executor_revision: u64,
    pub executor_executable_sha256: [u8; 32],
    pub broker_id: Uuid,
    pub broker_revision: u64,
    pub broker_executable_sha256: [u8; 32],
    pub protected_control_plane_sha256: [u8; 32],
    pub authority_key_id: String,
    pub authority_verifying_key: [u8; 32],
    pub receipt_key_id: String,
    pub bound_child_epoch_id: Uuid,
    pub bound_session_id: Uuid,
    pub bound_session_revision: u64,
    pub bound_child_epoch_revision: u64,
    pub bound_feature_lifecycle_revision: u64,
    pub bound_authority_revision: u64,
    pub bound_authority_snapshot_sha256: [u8; 32],
    pub next_action_sequence: u64,
    pub protected_manifest: ProtectedControlPlanePathManifest,
    pub authority_snapshot: ExecutorAuthoritySnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "intent", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExecutorRuntimeIntent {
    Dispatch {
        envelope: Box<ExecutionActionEnvelope>,
        operation: UnprivilegedProcessOperation,
    },
    AuthorityUpdate {
        snapshot: ExecutorAuthoritySnapshot,
    },
    Stop {
        last_checkpoint_sha256: [u8; 32],
        graceful_window_ms: u64,
        forced_window_ms: u64,
    },
    EmergencyTerminate {
        snapshot: ExecutorAuthoritySnapshot,
        last_checkpoint_sha256: [u8; 32],
        forced_window_ms: u64,
    },
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutorRuntimeRequest {
    pub schema_version: u16,
    pub runtime_id: Uuid,
    pub runtime_revision: u64,
    pub request_id: Uuid,
    pub request_sequence: u64,
    pub nonce: Uuid,
    pub session_id: Uuid,
    pub session_revision: u64,
    pub child_epoch_id: Uuid,
    pub child_epoch_revision: u64,
    pub feature_lifecycle_revision: u64,
    pub authority_revision: u64,
    pub signer_key_id: String,
    pub intent: ExecutorRuntimeIntent,
    pub signature: Vec<u8>,
}

impl ExecutorRuntimeRequest {
    pub fn sign(&mut self, key: &SigningKey) -> Result<(), RuntimeError> {
        if !self.signature.is_empty() || !self.valid_shape(false) {
            return Err(RuntimeError::InvalidRequest);
        }
        self.signature = key.sign(&self.signing_bytes()?).to_bytes().to_vec();
        Ok(())
    }

    fn verify(&self, key: &VerifyingKey) -> Result<(), RuntimeError> {
        if !self.valid_shape(true) {
            return Err(RuntimeError::InvalidRequest);
        }
        let signature =
            Signature::from_slice(&self.signature).map_err(|_| RuntimeError::InvalidRequest)?;
        key.verify(&self.signing_bytes()?, &signature)
            .map_err(|_| RuntimeError::InvalidRequest)
    }

    fn valid_shape(&self, signed: bool) -> bool {
        self.schema_version == RUNTIME_SCHEMA_VERSION
            && !self.runtime_id.is_nil()
            && self.runtime_revision > 0
            && !self.request_id.is_nil()
            && self.request_sequence > 0
            && !self.nonce.is_nil()
            && !self.session_id.is_nil()
            && self.session_revision > 0
            && !self.child_epoch_id.is_nil()
            && self.child_epoch_revision > 0
            && self.feature_lifecycle_revision > 0
            && self.authority_revision > 0
            && !self.signer_key_id.is_empty()
            && if signed {
                self.signature.len() == 64
            } else {
                self.signature.is_empty()
            }
    }

    fn signing_bytes(&self) -> Result<Vec<u8>, RuntimeError> {
        let mut unsigned = self.clone();
        unsigned.signature.clear();
        serde_json::to_vec(&unsigned).map_err(|_| RuntimeError::InvalidRequest)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExecutorRuntimeResult {
    Accepted,
    AuthorityUpdated,
    Terminated {
        receipt: ExecutionTerminationReceipt,
    },
    Shutdown,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutorRuntimeResponse {
    pub schema_version: u16,
    pub request_id: Uuid,
    pub result: ExecutorRuntimeResult,
}

pub struct ExecutorRuntime {
    config: ExecutorRuntimeConfig,
    policy: ExecutorPolicy,
    verifying_key: VerifyingKey,
    next_request_sequence: u64,
    seen_requests: HashSet<Uuid>,
    seen_nonces: HashSet<Uuid>,
    current_authority_revision: u64,
    active: Option<OwnedExecution>,
    quarantined: bool,
}

impl ExecutorRuntime {
    /// Constructs an active runtime from a protected, out-of-band receipt key.
    ///
    /// The receipt seed is deliberately not serializable as part of
    /// [`ExecutorRuntimeConfig`]. Windows production wiring must inject it only
    /// after establishing authenticated broker IPC and isolating payload
    /// processes from the executor service process.
    pub fn new(
        config: ExecutorRuntimeConfig,
        receipt_signing_seed: [u8; 32],
    ) -> Result<Self, RuntimeError> {
        validate_config(&config)?;
        if receipt_signing_seed == [0; 32] {
            return Err(RuntimeError::InvalidConfig);
        }
        let verifying_key = VerifyingKey::from_bytes(&config.authority_verifying_key)
            .map_err(|_| RuntimeError::InvalidConfig)?;
        let receipt_key = SigningKey::from_bytes(&receipt_signing_seed);
        let policy = ExecutorPolicy::new(
            ExecutorIdentity {
                platform: config.platform,
                executor_id: config.executor_id,
                executor_revision: config.executor_revision,
                executor_executable_sha256: config.executor_executable_sha256,
                broker_id: config.broker_id,
                broker_revision: config.broker_revision,
                broker_executable_sha256: config.broker_executable_sha256,
                protected_control_plane_sha256: config.protected_control_plane_sha256,
                authority_key_id: config.authority_key_id.clone(),
                authority_verifying_key: verifying_key,
                receipt_key_id: config.receipt_key_id.clone(),
                receipt_signing_key: receipt_key,
                bound_child_epoch_id: config.bound_child_epoch_id,
                bound_session_id: config.bound_session_id,
                bound_session_revision: config.bound_session_revision,
                bound_child_epoch_revision: config.bound_child_epoch_revision,
                bound_feature_lifecycle_revision: config.bound_feature_lifecycle_revision,
                bound_authority_revision: config.bound_authority_revision,
                bound_authority_snapshot_sha256: config.bound_authority_snapshot_sha256,
                next_action_sequence: config.next_action_sequence,
            },
            config.protected_manifest.clone(),
            config.authority_snapshot.clone(),
        )
        .map_err(|_| RuntimeError::InvalidConfig)?;
        Ok(Self {
            next_request_sequence: config.next_request_sequence,
            current_authority_revision: config.authority_snapshot.authority_revision,
            quarantined: config.restart_quarantined,
            config,
            policy,
            verifying_key,
            seen_requests: HashSet::new(),
            seen_nonces: HashSet::new(),
            active: None,
        })
    }

    pub fn handle(
        &mut self,
        request: ExecutorRuntimeRequest,
    ) -> Result<ExecutorRuntimeResponse, RuntimeError> {
        if self.quarantined {
            return Err(RuntimeError::Quarantined);
        }
        if let Err(error) = self.authenticate(&request) {
            self.quarantined = true;
            return Err(error);
        }
        let request_id = request.request_id;
        let result = match request.intent {
            ExecutorRuntimeIntent::Dispatch {
                envelope,
                operation,
            } => {
                if self.active.is_some()
                    || envelope.authority_revision != request.authority_revision
                {
                    return self.quarantine(RuntimeError::InvalidRequest);
                }
                let execution = match self
                    .policy
                    .admit(&envelope, &operation)
                    .and_then(|admission| admission.spawn())
                {
                    Ok(execution) => execution,
                    Err(_) => return self.quarantine(RuntimeError::Executor),
                };
                self.active = Some(execution);
                ExecutorRuntimeResult::Accepted
            }
            ExecutorRuntimeIntent::AuthorityUpdate { snapshot } => {
                if self.active.is_some()
                    || snapshot.authority_revision <= self.current_authority_revision
                    || snapshot.emergency_paused
                    || snapshot.revoked
                {
                    return self.quarantine(RuntimeError::InvalidRequest);
                }
                if self
                    .policy
                    .update_authority_snapshot(snapshot.clone())
                    .is_err()
                {
                    return self.quarantine(RuntimeError::Executor);
                }
                self.current_authority_revision = snapshot.authority_revision;
                ExecutorRuntimeResult::AuthorityUpdated
            }
            ExecutorRuntimeIntent::Stop {
                last_checkpoint_sha256,
                graceful_window_ms,
                forced_window_ms,
            } => {
                if validate_windows(graceful_window_ms, forced_window_ms).is_err() {
                    return self.quarantine(RuntimeError::InvalidRequest);
                }
                let Some(execution) = self.active.take() else {
                    return self.quarantine(RuntimeError::InvalidRequest);
                };
                let receipt = match execution.terminate(
                    ExecutionTerminationMode::Stop,
                    last_checkpoint_sha256,
                    Duration::from_millis(graceful_window_ms),
                    Duration::from_millis(forced_window_ms),
                ) {
                    Ok(receipt) => receipt,
                    Err(_) => return self.quarantine(RuntimeError::Executor),
                };
                ExecutorRuntimeResult::Terminated { receipt }
            }
            ExecutorRuntimeIntent::EmergencyTerminate {
                snapshot,
                last_checkpoint_sha256,
                forced_window_ms,
            } => {
                if validate_windows(0, forced_window_ms).is_err() {
                    return self.quarantine(RuntimeError::InvalidRequest);
                }
                if snapshot.authority_revision <= self.current_authority_revision
                    || (!snapshot.emergency_paused && !snapshot.revoked)
                    || self
                        .policy
                        .update_authority_snapshot(snapshot.clone())
                        .is_err()
                {
                    return self.quarantine(RuntimeError::InvalidRequest);
                }
                self.current_authority_revision = snapshot.authority_revision;
                let Some(execution) = self.active.take() else {
                    return self.quarantine(RuntimeError::InvalidRequest);
                };
                let receipt = match execution.terminate(
                    ExecutionTerminationMode::EmergencyPause,
                    last_checkpoint_sha256,
                    Duration::ZERO,
                    Duration::from_millis(forced_window_ms),
                ) {
                    Ok(receipt) => receipt,
                    Err(_) => return self.quarantine(RuntimeError::Executor),
                };
                ExecutorRuntimeResult::Terminated { receipt }
            }
            ExecutorRuntimeIntent::Shutdown => {
                if self.active.is_some() {
                    return self.quarantine(RuntimeError::InvalidRequest);
                }
                ExecutorRuntimeResult::Shutdown
            }
        };
        if self.commit_request(request_id, request.nonce).is_err() {
            return self.quarantine(RuntimeError::Quarantined);
        }
        Ok(ExecutorRuntimeResponse {
            schema_version: RUNTIME_SCHEMA_VERSION,
            request_id,
            result,
        })
    }

    fn authenticate(&self, request: &ExecutorRuntimeRequest) -> Result<(), RuntimeError> {
        request.verify(&self.verifying_key)?;
        if request.runtime_id != self.config.runtime_id
            || request.runtime_revision != self.config.runtime_revision
            || request.signer_key_id != self.config.authority_key_id
            || request.request_sequence != self.next_request_sequence
            || self.seen_requests.contains(&request.request_id)
            || self.seen_nonces.contains(&request.nonce)
            || request.session_id != self.config.bound_session_id
            || request.session_revision != self.config.bound_session_revision
            || request.child_epoch_id != self.config.bound_child_epoch_id
            || request.child_epoch_revision != self.config.bound_child_epoch_revision
            || request.feature_lifecycle_revision != self.config.bound_feature_lifecycle_revision
            || request.authority_revision != self.current_authority_revision
        {
            return Err(RuntimeError::InvalidRequest);
        }
        Ok(())
    }

    fn commit_request(&mut self, request_id: Uuid, nonce: Uuid) -> Result<(), RuntimeError> {
        self.next_request_sequence = self
            .next_request_sequence
            .checked_add(1)
            .ok_or(RuntimeError::Quarantined)?;
        self.seen_requests.insert(request_id);
        self.seen_nonces.insert(nonce);
        Ok(())
    }

    fn quarantine<T>(&mut self, error: RuntimeError) -> Result<T, RuntimeError> {
        self.quarantined = true;
        Err(error)
    }
}

fn validate_windows(graceful: u64, forced: u64) -> Result<(), RuntimeError> {
    if graceful > MAX_TERMINATION_WINDOW_MS || forced == 0 || forced > MAX_TERMINATION_WINDOW_MS {
        Err(RuntimeError::InvalidRequest)
    } else {
        Ok(())
    }
}

fn validate_config(config: &ExecutorRuntimeConfig) -> Result<(), RuntimeError> {
    if config.schema_version != RUNTIME_SCHEMA_VERSION
        || config.runtime_id.is_nil()
        || config.runtime_revision == 0
        || config.next_request_sequence == 0
        || config.owner_uid.is_none() && cfg!(unix)
        || config.bound_authority_revision != config.authority_snapshot.authority_revision
        || config.bound_authority_snapshot_sha256
            != config
                .authority_snapshot
                .sha256()
                .map_err(|_| RuntimeError::InvalidConfig)?
    {
        return Err(RuntimeError::InvalidConfig);
    }
    #[cfg(unix)]
    if config.owner_uid != Some(unsafe { libc::geteuid() }) {
        return Err(RuntimeError::InvalidConfig);
    }
    #[cfg(target_os = "macos")]
    if config.platform != ExecutionHostPlatform::Macos {
        return Err(RuntimeError::InvalidConfig);
    }
    #[cfg(windows)]
    if config.platform != ExecutionHostPlatform::Windows || config.owner_uid.is_some() {
        return Err(RuntimeError::InvalidConfig);
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    return Err(RuntimeError::InvalidConfig);
    Ok(())
}

pub fn load_config(
    path: &Path,
    expected_sha256: [u8; 32],
) -> Result<ExecutorRuntimeConfig, RuntimeError> {
    let mut file = open_config(path)?;
    let metadata = file.metadata().map_err(|_| RuntimeError::InvalidConfig)?;
    if !metadata.is_file() || is_windows_reparse(&metadata) || link_count(path, &metadata) != 1 {
        return Err(RuntimeError::InvalidConfig);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.uid() != unsafe { libc::geteuid() }
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err(RuntimeError::InvalidConfig);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_READONLY: u32 = 0x1;
        if metadata.file_attributes() & FILE_ATTRIBUTE_READONLY == 0 {
            return Err(RuntimeError::InvalidConfig);
        }
    }
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .map_err(|_| RuntimeError::InvalidConfig)?;
    let actual_sha256: [u8; 32] = Sha256::digest(&bytes).into();
    if actual_sha256 != expected_sha256 {
        return Err(RuntimeError::InvalidConfig);
    }
    let config: ExecutorRuntimeConfig =
        serde_json::from_slice(&bytes).map_err(|_| RuntimeError::InvalidConfig)?;
    if current_executable_sha256()? != config.executor_executable_sha256 {
        return Err(RuntimeError::InvalidConfig);
    }
    Ok(config)
}

/// Validates the complete effect-disabled Windows service bootstrap without
/// materializing any receipt-signing secret or active execution runtime.
pub fn validate_service_bootstrap(config: ExecutorRuntimeConfig) -> Result<(), RuntimeError> {
    validate_config(&config)?;
    let verifying_key = VerifyingKey::from_bytes(&config.authority_verifying_key)
        .map_err(|_| RuntimeError::InvalidConfig)?;
    let bootstrap_only_key = SigningKey::from_bytes(&[1; 32]);
    ExecutorPolicy::new(
        ExecutorIdentity {
            platform: config.platform,
            executor_id: config.executor_id,
            executor_revision: config.executor_revision,
            executor_executable_sha256: config.executor_executable_sha256,
            broker_id: config.broker_id,
            broker_revision: config.broker_revision,
            broker_executable_sha256: config.broker_executable_sha256,
            protected_control_plane_sha256: config.protected_control_plane_sha256,
            authority_key_id: config.authority_key_id,
            authority_verifying_key: verifying_key,
            receipt_key_id: config.receipt_key_id,
            receipt_signing_key: bootstrap_only_key,
            bound_child_epoch_id: config.bound_child_epoch_id,
            bound_session_id: config.bound_session_id,
            bound_session_revision: config.bound_session_revision,
            bound_child_epoch_revision: config.bound_child_epoch_revision,
            bound_feature_lifecycle_revision: config.bound_feature_lifecycle_revision,
            bound_authority_revision: config.bound_authority_revision,
            bound_authority_snapshot_sha256: config.bound_authority_snapshot_sha256,
            next_action_sequence: config.next_action_sequence,
        },
        config.protected_manifest,
        config.authority_snapshot,
    )
    .map(|_| ())
    .map_err(|_| RuntimeError::InvalidConfig)
}

fn current_executable_sha256() -> Result<[u8; 32], RuntimeError> {
    let path = std::env::current_exe().map_err(|_| RuntimeError::InvalidConfig)?;
    let mut file = open_config(&path)?;
    let metadata = file.metadata().map_err(|_| RuntimeError::InvalidConfig)?;
    if !metadata.is_file() || is_windows_reparse(&metadata) || link_count(&path, &metadata) != 1 {
        return Err(RuntimeError::InvalidConfig);
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|_| RuntimeError::InvalidConfig)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hasher.finalize().into())
}

#[cfg(unix)]
fn open_config(path: &Path) -> Result<File, RuntimeError> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
        .map_err(|_| RuntimeError::InvalidConfig)
}

#[cfg(windows)]
fn open_config(path: &Path) -> Result<File, RuntimeError> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_READ, FILE_SHARE_READ,
    };
    OpenOptions::new()
        .access_mode(FILE_GENERIC_READ)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| RuntimeError::InvalidConfig)
}

#[cfg(not(any(unix, windows)))]
fn open_config(_path: &Path) -> Result<File, RuntimeError> {
    Err(RuntimeError::InvalidConfig)
}

#[cfg(windows)]
fn is_windows_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_windows_reparse(_metadata: &fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
fn link_count(_path: &Path, metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.nlink()
}

#[cfg(windows)]
fn link_count(path: &Path, _metadata: &fs::Metadata) -> u64 {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };
    let Ok(file) = open_config(path) else {
        return 0;
    };
    let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) } == 0 {
        0
    } else {
        u64::from(information.nNumberOfLinks)
    }
}

#[cfg(not(any(unix, windows)))]
fn link_count(_path: &Path, _metadata: &fs::Metadata) -> u64 {
    0
}

#[cfg(windows)]
pub fn run_stdio(
    _config: ExecutorRuntimeConfig,
    _input: impl Read,
    _output: impl Write,
) -> Result<(), RuntimeError> {
    Err(RuntimeError::InvalidConfig)
}

#[cfg(not(windows))]
pub fn run_stdio(
    config: ExecutorRuntimeConfig,
    mut input: impl Read,
    mut output: impl Write,
) -> Result<(), RuntimeError> {
    let mut receipt_signing_seed = [0_u8; 32];
    input
        .read_exact(&mut receipt_signing_seed)
        .map_err(|_| RuntimeError::InvalidConfig)?;
    let mut runtime = ExecutorRuntime::new(config, receipt_signing_seed)?;
    loop {
        let Some(frame) = read_frame(&mut input)? else {
            return if runtime.active.is_some() {
                Err(RuntimeError::Quarantined)
            } else {
                Ok(())
            };
        };
        let request: ExecutorRuntimeRequest =
            serde_json::from_slice(&frame).map_err(|_| RuntimeError::InvalidRequest)?;
        let shutdown = matches!(request.intent, ExecutorRuntimeIntent::Shutdown);
        let response = runtime.handle(request)?;
        write_frame(
            &mut output,
            &serde_json::to_vec(&response).map_err(|_| RuntimeError::Io)?,
        )?;
        if shutdown {
            return Ok(());
        }
    }
}

#[cfg(not(windows))]
fn read_frame(reader: &mut impl Read) -> Result<Option<Vec<u8>>, RuntimeError> {
    let mut length = [0_u8; 4];
    match reader.read_exact(&mut length) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(_) => return Err(RuntimeError::Io),
    }
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_FRAME_BYTES {
        return Err(RuntimeError::InvalidRequest);
    }
    let mut frame = vec![0; length];
    reader
        .read_exact(&mut frame)
        .map_err(|_| RuntimeError::Io)?;
    Ok(Some(frame))
}

#[cfg(not(windows))]
fn write_frame(writer: &mut impl Write, frame: &[u8]) -> Result<(), RuntimeError> {
    let length = u32::try_from(frame.len()).map_err(|_| RuntimeError::Io)?;
    writer
        .write_all(&length.to_be_bytes())
        .map_err(|_| RuntimeError::Io)?;
    writer.write_all(frame).map_err(|_| RuntimeError::Io)?;
    writer.flush().map_err(|_| RuntimeError::Io)
}

pub fn parse_sha256(value: &str) -> Result<[u8; 32], RuntimeError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(RuntimeError::InvalidConfig);
    }
    let mut output = [0_u8; 32];
    for (index, byte) in output.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| RuntimeError::InvalidConfig)?;
    }
    Ok(output)
}

pub fn config_path_from_args() -> Result<(PathBuf, [u8; 32]), RuntimeError> {
    let mut arguments = std::env::args_os().skip(1);
    if arguments.next().as_deref() != Some(std::ffi::OsStr::new("--config")) {
        return Err(RuntimeError::InvalidConfig);
    }
    let path = arguments
        .next()
        .map(PathBuf::from)
        .ok_or(RuntimeError::InvalidConfig)?;
    if arguments.next().as_deref() != Some(std::ffi::OsStr::new("--config-sha256")) {
        return Err(RuntimeError::InvalidConfig);
    }
    let digest = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .ok_or(RuntimeError::InvalidConfig)?;
    if arguments.next().is_some() || !path.is_absolute() {
        return Err(RuntimeError::InvalidConfig);
    }
    Ok((path, parse_sha256(&digest)?))
}
