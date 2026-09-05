use crate::{BrokerIdentity, BrokerOperation, BrokerPolicy};
use assemblywright_protocol::{
    ExecutionActionEnvelope, ExecutionHostPlatform, ProtectedControlPlanePathManifest,
};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub const RUNTIME_SCHEMA_VERSION: u16 = 1;
const MAX_FRAME_BYTES: usize = 1024 * 1024;

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
    #[error("broker rejected the request")]
    Broker,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerRuntimeConfig {
    pub schema_version: u16,
    pub runtime_id: Uuid,
    pub runtime_revision: u64,
    pub platform: ExecutionHostPlatform,
    pub owner_uid: Option<u32>,
    pub next_request_sequence: u64,
    pub restart_quarantined: bool,
    pub broker_id: Uuid,
    pub broker_revision: u64,
    pub broker_executable_sha256: [u8; 32],
    pub executor_id: Uuid,
    pub executor_revision: u64,
    pub executor_executable_sha256: [u8; 32],
    pub protected_control_plane_sha256: [u8; 32],
    pub authority_key_id: String,
    pub authority_verifying_key: [u8; 32],
    pub bound_child_epoch_id: Uuid,
    pub bound_session_id: Uuid,
    pub bound_session_revision: u64,
    pub bound_child_epoch_revision: u64,
    pub bound_feature_lifecycle_revision: u64,
    pub bound_authority_revision: u64,
    pub next_action_sequence: u64,
    pub protected_manifest: ProtectedControlPlanePathManifest,
    #[serde(default)]
    pub ipc: Option<BrokerIpcBootstrap>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerIpcBootstrap {
    pub pipe_name: String,
    pub broker_service_sid: String,
    pub expected_master_service_sid: String,
    pub executor_pipe_name: String,
    pub expected_executor_service_sid: String,
    pub durable_state_path: PathBuf,
    pub ack_seed_path: PathBuf,
    pub ack_key_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "intent", rename_all = "snake_case", deny_unknown_fields)]
pub enum BrokerRuntimeIntent {
    Dispatch {
        envelope: Box<ExecutionActionEnvelope>,
        operation: BrokerOperation,
    },
    Stop,
    EmergencyTerminate,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerRuntimeRequest {
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
    pub intent: BrokerRuntimeIntent,
    pub signature: Vec<u8>,
}

impl BrokerRuntimeRequest {
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

#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub enum BrokerRuntimeResult {
    ValidatedEffectDisabled,
    TerminationIntentAccepted { active_effects: u32 },
    Shutdown,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerRuntimeResponse {
    pub schema_version: u16,
    pub request_id: Uuid,
    pub result: BrokerRuntimeResult,
}

pub struct BrokerRuntime {
    config: BrokerRuntimeConfig,
    policy: BrokerPolicy,
    verifying_key: VerifyingKey,
    next_request_sequence: u64,
    seen_requests: HashSet<Uuid>,
    seen_nonces: HashSet<Uuid>,
    quarantined: bool,
}

impl BrokerRuntime {
    pub fn new(config: BrokerRuntimeConfig) -> Result<Self, RuntimeError> {
        validate_config(&config)?;
        let verifying_key = VerifyingKey::from_bytes(&config.authority_verifying_key)
            .map_err(|_| RuntimeError::InvalidConfig)?;
        let policy = BrokerPolicy::new(
            BrokerIdentity {
                platform: config.platform,
                broker_id: config.broker_id,
                broker_revision: config.broker_revision,
                broker_executable_sha256: config.broker_executable_sha256,
                executor_id: config.executor_id,
                executor_revision: config.executor_revision,
                executor_executable_sha256: config.executor_executable_sha256,
                protected_control_plane_sha256: config.protected_control_plane_sha256,
                signer_key_id: config.authority_key_id.clone(),
                verifying_key,
                bound_child_epoch_id: config.bound_child_epoch_id,
                bound_session_id: config.bound_session_id,
                bound_session_revision: config.bound_session_revision,
                bound_child_epoch_revision: config.bound_child_epoch_revision,
                bound_feature_lifecycle_revision: config.bound_feature_lifecycle_revision,
                bound_authority_revision: config.bound_authority_revision,
                next_action_sequence: config.next_action_sequence,
            },
            config.protected_manifest.clone(),
        )
        .map_err(|_| RuntimeError::InvalidConfig)?;
        Ok(Self {
            next_request_sequence: config.next_request_sequence,
            quarantined: config.restart_quarantined,
            config,
            policy,
            verifying_key,
            seen_requests: HashSet::new(),
            seen_nonces: HashSet::new(),
        })
    }

    pub fn handle(
        &mut self,
        request: BrokerRuntimeRequest,
    ) -> Result<BrokerRuntimeResponse, RuntimeError> {
        if self.quarantined {
            return Err(RuntimeError::Quarantined);
        }
        if let Err(error) = self.authenticate(&request) {
            self.quarantined = true;
            return Err(error);
        }
        let request_id = request.request_id;
        let terminal = matches!(
            request.intent,
            BrokerRuntimeIntent::Stop | BrokerRuntimeIntent::EmergencyTerminate
        );
        let result = match request.intent {
            BrokerRuntimeIntent::Dispatch {
                envelope,
                operation,
            } => {
                if envelope.authority_revision != request.authority_revision {
                    return self.quarantine(RuntimeError::Broker);
                }
                let admission = match self.policy.admit(&envelope, &operation) {
                    Ok(admission) => admission,
                    Err(_) => return self.quarantine(RuntimeError::Broker),
                };
                // Dispatch remains product-unavailable: this synchronous
                // runtime cannot truthfully service Stop/EmergencyTerminate
                // while an effect is active. Native CreateDirectory is exposed
                // only through the dedicated one-shot proof seam.
                let _ = admission;
                BrokerRuntimeResult::ValidatedEffectDisabled
            }
            BrokerRuntimeIntent::Stop | BrokerRuntimeIntent::EmergencyTerminate => {
                BrokerRuntimeResult::TerminationIntentAccepted { active_effects: 0 }
            }
            BrokerRuntimeIntent::Shutdown => BrokerRuntimeResult::Shutdown,
        };
        if self.commit_request(request_id, request.nonce).is_err() {
            return self.quarantine(RuntimeError::Quarantined);
        }
        if terminal {
            self.quarantined = true;
        }
        Ok(BrokerRuntimeResponse {
            schema_version: RUNTIME_SCHEMA_VERSION,
            request_id,
            result,
        })
    }

    fn authenticate(&self, request: &BrokerRuntimeRequest) -> Result<(), RuntimeError> {
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
            || request.authority_revision != self.config.bound_authority_revision
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

fn validate_config(config: &BrokerRuntimeConfig) -> Result<(), RuntimeError> {
    if config.schema_version != RUNTIME_SCHEMA_VERSION
        || config.runtime_id.is_nil()
        || config.runtime_revision == 0
        || config.next_request_sequence == 0
        || config.bound_authority_revision == 0
        || config.owner_uid.is_none() && cfg!(unix)
    {
        return Err(RuntimeError::InvalidConfig);
    }
    if let Some(ipc) = &config.ipc {
        if ipc.pipe_name.len() > 128
            || ipc.executor_pipe_name.len() > 128
            || ipc.broker_service_sid.is_empty()
            || ipc.broker_service_sid.len() > 192
            || ipc.expected_master_service_sid.len() > 192
            || ipc.expected_executor_service_sid.len() > 192
            || ipc.ack_key_id.is_empty()
            || ipc.durable_state_path == ipc.ack_seed_path
            || !is_protected_ipc_leaf(
                &ipc.durable_state_path,
                Path::new(&config.protected_manifest.ipc_and_enforcement_state),
            )
            || !is_protected_ipc_leaf(
                &ipc.ack_seed_path,
                Path::new(&config.protected_manifest.ipc_and_enforcement_state),
            )
        {
            return Err(RuntimeError::InvalidConfig);
        }
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

fn is_protected_ipc_leaf(path: &Path, protected_root: &Path) -> bool {
    if !path.is_absolute()
        || !protected_root.is_absolute()
        || path == protected_root
        || path.file_name().is_none()
        || path.parent() != Some(protected_root)
        || path.components().any(|component| {
            matches!(
                component,
                std::path::Component::CurDir | std::path::Component::ParentDir
            )
        })
        || protected_root.components().any(|component| {
            matches!(
                component,
                std::path::Component::CurDir | std::path::Component::ParentDir
            )
        })
    {
        return false;
    }
    #[cfg(windows)]
    if !has_safe_windows_path_spelling(path) || !has_safe_windows_path_spelling(protected_root) {
        return false;
    }
    true
}

#[cfg(windows)]
fn has_safe_windows_path_spelling(path: &Path) -> bool {
    use std::path::{Component, Prefix};

    let mut components = path.components();
    if !matches!(
        components.next(),
        Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_))
    ) || !matches!(components.next(), Some(Component::RootDir))
    {
        return false;
    }
    components.all(|component| match component {
        Component::Normal(value) => {
            let value = value.to_string_lossy();
            let trimmed = value.trim_end_matches(['.', ' ']);
            let stem = trimmed.split('.').next().unwrap_or_default();
            !value.contains(':')
                && trimmed.len() == value.len()
                && !matches!(
                    stem.to_ascii_uppercase().as_str(),
                    "CON"
                        | "PRN"
                        | "AUX"
                        | "NUL"
                        | "COM1"
                        | "COM2"
                        | "COM3"
                        | "COM4"
                        | "COM5"
                        | "COM6"
                        | "COM7"
                        | "COM8"
                        | "COM9"
                        | "LPT1"
                        | "LPT2"
                        | "LPT3"
                        | "LPT4"
                        | "LPT5"
                        | "LPT6"
                        | "LPT7"
                        | "LPT8"
                        | "LPT9"
                )
        }
        _ => false,
    })
}

#[cfg(all(test, windows))]
mod ipc_path_tests {
    use super::{has_safe_windows_path_spelling, is_protected_ipc_leaf};
    use std::path::Path;

    #[test]
    fn rejects_windows_device_ads_and_escape_spellings() {
        let root = Path::new(r"C:\ProgramData\Assemblywright\ipc");
        assert!(is_protected_ipc_leaf(
            Path::new(r"C:\ProgramData\Assemblywright\ipc\broker.journal"),
            root
        ));
        for hostile in [
            r"C:\ProgramData\Assemblywright\ipc\..\outside",
            r"C:\ProgramData\Assemblywright\ipc\seed:ads",
            r"C:\ProgramData\Assemblywright\ipc\CON",
            r"\\?\C:\ProgramData\Assemblywright\ipc\seed",
            r"\\.\C:\ProgramData\Assemblywright\ipc\seed",
        ] {
            assert!(
                !is_protected_ipc_leaf(Path::new(hostile), root),
                "{hostile}"
            );
        }
        assert!(!has_safe_windows_path_spelling(Path::new(
            r"C:\ProgramData\Assemblywright\ipc\seed."
        )));
    }
}

pub fn load_config(
    path: &Path,
    expected_sha256: [u8; 32],
) -> Result<BrokerRuntimeConfig, RuntimeError> {
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
    let config: BrokerRuntimeConfig =
        serde_json::from_slice(&bytes).map_err(|_| RuntimeError::InvalidConfig)?;
    if current_executable_sha256()? != config.broker_executable_sha256 {
        return Err(RuntimeError::InvalidConfig);
    }
    Ok(config)
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

pub fn run_stdio(
    config: BrokerRuntimeConfig,
    mut input: impl Read,
    mut output: impl Write,
) -> Result<(), RuntimeError> {
    let mut runtime = BrokerRuntime::new(config)?;
    loop {
        let Some(frame) = read_frame(&mut input)? else {
            return Ok(());
        };
        let request: BrokerRuntimeRequest =
            serde_json::from_slice(&frame).map_err(|_| RuntimeError::InvalidRequest)?;
        let terminal = matches!(
            request.intent,
            BrokerRuntimeIntent::Stop
                | BrokerRuntimeIntent::EmergencyTerminate
                | BrokerRuntimeIntent::Shutdown
        );
        let response = runtime.handle(request)?;
        let response = serde_json::to_vec(&response).map_err(|_| RuntimeError::Io)?;
        write_frame(&mut output, &response)?;
        if terminal {
            return Ok(());
        }
    }
}

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
