use assemblywright_protocol::{
    execution_path_sha256, ExecutionActionEnvelope, ExecutionActionType, ExecutionHostPlatform,
    ExecutionTargetIdentity, ExecutionTerminationMode, ExecutionTerminationOutcome,
    ExecutionTerminationReceipt, ProtectedControlPlanePathManifest,
    FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub mod ipc;
pub mod runtime;
pub mod startup;
#[cfg(windows)]
pub mod windows_execution_ipc;
#[cfg(windows)]
pub mod windows_service_host;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ExecutorError {
    #[error("executor identity or signed envelope is invalid")]
    InvalidIdentity,
    #[error("action is expired, future-dated, or bound to another host")]
    InvalidDeadline,
    #[error("action operation does not match its signed digest")]
    InvalidOperation,
    #[error("action is replayed or non-contiguous")]
    Replay,
    #[error("execution path is ambiguous, linked, or protected")]
    UnsafePath,
    #[error("process could not be atomically contained")]
    ContainmentFailed,
    #[error("process termination could not be proved")]
    IncompleteTermination,
    #[error("executor state is unavailable")]
    StateUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnprivilegedProcessOperation {
    pub executable: String,
    pub arguments: Vec<String>,
    pub working_directory: String,
    pub environment: BTreeMap<String, String>,
}

impl UnprivilegedProcessOperation {
    pub fn sha256(&self) -> Result<[u8; 32], ExecutorError> {
        if self.arguments.len() > 256
            || self.executable.contains('\0')
            || self.working_directory.contains('\0')
            || self.arguments.iter().any(|value| value.contains('\0'))
            || self.arguments.iter().any(|value| value.len() > 16 * 1024)
            || self.environment.len() > 64
            || self.environment.keys().any(|key| {
                key.is_empty()
                    || key.len() > 128
                    || !key
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            })
            || self
                .environment
                .values()
                .any(|value| value.len() > 64 * 1024 || value.contains('\0'))
        {
            return Err(ExecutorError::InvalidOperation);
        }
        let bytes = serde_json::to_vec(self).map_err(|_| ExecutorError::InvalidOperation)?;
        Ok(Sha256::digest(bytes).into())
    }
}

pub struct ExecutorIdentity {
    pub platform: ExecutionHostPlatform,
    pub executor_id: Uuid,
    pub executor_revision: u64,
    pub executor_executable_sha256: [u8; 32],
    pub broker_id: Uuid,
    pub broker_revision: u64,
    pub broker_executable_sha256: [u8; 32],
    pub protected_control_plane_sha256: [u8; 32],
    pub authority_key_id: String,
    pub authority_verifying_key: VerifyingKey,
    pub receipt_key_id: String,
    pub receipt_signing_key: SigningKey,
    /// Restored from the Windows-authoritative durable action ledger.
    pub bound_child_epoch_id: Uuid,
    pub bound_session_id: Uuid,
    pub bound_session_revision: u64,
    pub bound_child_epoch_revision: u64,
    pub bound_feature_lifecycle_revision: u64,
    /// Exact durable authority checkpoint restored from the master ledger.
    pub bound_authority_revision: u64,
    pub bound_authority_snapshot_sha256: [u8; 32],
    pub next_action_sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecutorAuthoritySnapshot {
    pub authority_revision: u64,
    pub session_id: Uuid,
    pub session_revision: u64,
    pub child_epoch_id: Uuid,
    pub child_epoch_revision: u64,
    pub feature_lifecycle_revision: u64,
    pub emergency_paused: bool,
    pub revoked: bool,
    pub signer_key_id: String,
    pub signature: Vec<u8>,
}

impl ExecutorAuthoritySnapshot {
    pub fn sign(&mut self, key: &SigningKey) -> Result<(), ExecutorError> {
        if !self.signature.is_empty() {
            return Err(ExecutorError::InvalidIdentity);
        }
        self.validate_shape(false)?;
        self.signature = key.sign(&self.signing_bytes()?).to_bytes().to_vec();
        self.validate_shape(true)
    }

    pub fn sha256(&self) -> Result<[u8; 32], ExecutorError> {
        self.validate_shape(true)?;
        let bytes = serde_json::to_vec(self).map_err(|_| ExecutorError::InvalidIdentity)?;
        Ok(Sha256::digest(bytes).into())
    }

    fn verify(&self, key: &VerifyingKey) -> Result<(), ExecutorError> {
        self.validate_shape(true)?;
        let signature =
            Signature::from_slice(&self.signature).map_err(|_| ExecutorError::InvalidIdentity)?;
        key.verify(&self.signing_bytes()?, &signature)
            .map_err(|_| ExecutorError::InvalidIdentity)
    }

    fn validate_shape(&self, require_signature: bool) -> Result<(), ExecutorError> {
        if self.authority_revision == 0
            || self.session_id.is_nil()
            || self.session_revision == 0
            || self.child_epoch_id.is_nil()
            || self.child_epoch_revision == 0
            || self.feature_lifecycle_revision == 0
            || self.signer_key_id.is_empty()
            || (require_signature && self.signature.len() != 64)
            || (!require_signature && !self.signature.is_empty())
        {
            return Err(ExecutorError::InvalidIdentity);
        }
        Ok(())
    }

    fn signing_bytes(&self) -> Result<Vec<u8>, ExecutorError> {
        let mut unsigned = self.clone();
        unsigned.signature.clear();
        serde_json::to_vec(&unsigned).map_err(|_| ExecutorError::InvalidIdentity)
    }
}

pub struct ExecutorPolicy {
    identity: ExecutorIdentity,
    protected_roots: Vec<String>,
    replay: Mutex<ReplayState>,
    live_authority: Mutex<ExecutorAuthoritySnapshot>,
}

#[derive(Default)]
struct ReplayState {
    next_sequence: HashMap<Uuid, u64>,
    next_execution: HashMap<Uuid, u64>,
    active_execution: HashMap<Uuid, u64>,
    failed_epochs: HashSet<Uuid>,
    actions: HashSet<Uuid>,
    nonces: HashSet<Uuid>,
}

pub struct ExecutorAdmission<'a> {
    policy: &'a ExecutorPolicy,
    envelope: &'a ExecutionActionEnvelope,
    operation: &'a UnprivilegedProcessOperation,
    authority_revision: u64,
    prepared: platform::PreparedProcess,
}

pub struct OwnedExecution {
    child_epoch_id: Uuid,
    platform: ExecutionHostPlatform,
    receipt_key_id: String,
    receipt_signing_key: SigningKey,
    process: platform::ContainedProcess,
}

impl ExecutorPolicy {
    pub fn new(
        identity: ExecutorIdentity,
        protected_manifest: ProtectedControlPlanePathManifest,
        authority_snapshot: ExecutorAuthoritySnapshot,
    ) -> Result<Self, ExecutorError> {
        if identity.executor_id.is_nil()
            || identity.broker_id.is_nil()
            || identity.executor_id == identity.broker_id
            || identity.executor_revision == 0
            || identity.broker_revision == 0
            || identity.executor_executable_sha256 == [0; 32]
            || identity.broker_executable_sha256 == [0; 32]
            || identity.protected_control_plane_sha256 == [0; 32]
            || identity.authority_key_id.is_empty()
            || identity.receipt_key_id.is_empty()
            || identity.bound_child_epoch_id.is_nil()
            || identity.bound_session_id.is_nil()
            || identity.bound_session_revision == 0
            || identity.bound_child_epoch_revision == 0
            || identity.bound_feature_lifecycle_revision == 0
            || identity.bound_authority_revision == 0
            || identity.bound_authority_snapshot_sha256 == [0; 32]
            || identity.next_action_sequence == 0
        {
            return Err(ExecutorError::InvalidIdentity);
        }
        if protected_manifest.platform != identity.platform
            || protected_manifest
                .canonical_sha256()
                .map_err(|_| ExecutorError::InvalidIdentity)?
                != identity.protected_control_plane_sha256
        {
            return Err(ExecutorError::InvalidIdentity);
        }
        Self::validate_authority_snapshot(&identity, &authority_snapshot)?;
        if authority_snapshot.authority_revision != identity.bound_authority_revision
            || authority_snapshot.sha256()? != identity.bound_authority_snapshot_sha256
        {
            return Err(ExecutorError::InvalidIdentity);
        }
        let mut roots = Vec::new();
        for root in protected_manifest.paths() {
            #[cfg(windows)]
            let compared = compare_path(identity.platform, Path::new(root))?;
            #[cfg(not(windows))]
            let compared = {
                let canonical = canonical_ordinary(Path::new(root))?;
                compare_path(identity.platform, &canonical)?
            };
            roots.push(compared);
        }
        if roots.is_empty() {
            return Err(ExecutorError::InvalidIdentity);
        }
        roots.sort();
        roots.dedup();
        let replay_seed = (identity.bound_child_epoch_id, identity.next_action_sequence);
        Ok(Self {
            live_authority: Mutex::new(authority_snapshot),
            identity,
            protected_roots: roots,
            replay: Mutex::new(ReplayState {
                next_sequence: HashMap::from([replay_seed]),
                next_execution: HashMap::from([replay_seed]),
                ..ReplayState::default()
            }),
        })
    }

    /// Updates the in-process authority view supplied by the trusted master/broker
    /// integration. Product wiring remains disabled; untrusted children never
    /// receive a reference to this policy object.
    pub fn update_authority_snapshot(
        &self,
        snapshot: ExecutorAuthoritySnapshot,
    ) -> Result<(), ExecutorError> {
        Self::validate_authority_snapshot(&self.identity, &snapshot)?;
        let mut live = self
            .live_authority
            .lock()
            .map_err(|_| ExecutorError::StateUnavailable)?;
        if snapshot.authority_revision <= live.authority_revision {
            return Err(ExecutorError::Replay);
        }
        if live.revoked && !snapshot.revoked {
            return Err(ExecutorError::InvalidIdentity);
        }
        *live = snapshot;
        Ok(())
    }

    fn validate_authority_snapshot(
        identity: &ExecutorIdentity,
        snapshot: &ExecutorAuthoritySnapshot,
    ) -> Result<(), ExecutorError> {
        snapshot.verify(&identity.authority_verifying_key)?;
        if snapshot.signer_key_id != identity.authority_key_id
            || snapshot.session_id != identity.bound_session_id
            || snapshot.session_revision != identity.bound_session_revision
            || snapshot.child_epoch_id != identity.bound_child_epoch_id
            || snapshot.child_epoch_revision != identity.bound_child_epoch_revision
            || snapshot.feature_lifecycle_revision != identity.bound_feature_lifecycle_revision
        {
            return Err(ExecutorError::InvalidIdentity);
        }
        Ok(())
    }

    pub fn admit<'a>(
        &'a self,
        envelope: &'a ExecutionActionEnvelope,
        operation: &'a UnprivilegedProcessOperation,
    ) -> Result<ExecutorAdmission<'a>, ExecutorError> {
        let authority_revision = self.validate_live_authority(envelope)?;
        let now_ms = now_ms()?;
        envelope
            .verify_signature(&self.identity.authority_verifying_key)
            .map_err(|_| ExecutorError::InvalidIdentity)?;
        if envelope.host_platform != self.identity.platform {
            return Err(ExecutorError::InvalidDeadline);
        }
        if envelope.child_epoch_id != self.identity.bound_child_epoch_id
            || envelope.child_epoch_revision != self.identity.bound_child_epoch_revision
            || envelope.session_id != self.identity.bound_session_id
            || envelope.session_revision != self.identity.bound_session_revision
            || envelope.feature_lifecycle_revision != self.identity.bound_feature_lifecycle_revision
        {
            return Err(ExecutorError::Replay);
        }
        if envelope.signer_key_id != self.identity.authority_key_id
            || envelope.executor_id != self.identity.executor_id
            || envelope.executor_revision != self.identity.executor_revision
            || envelope.executor_executable_sha256 != self.identity.executor_executable_sha256
            || envelope.broker_id != self.identity.broker_id
            || envelope.broker_revision != self.identity.broker_revision
            || envelope.broker_executable_sha256 != self.identity.broker_executable_sha256
            || envelope.protected_control_plane_sha256
                != self.identity.protected_control_plane_sha256
        {
            return Err(ExecutorError::InvalidIdentity);
        }
        if now_ms < envelope.issued_at_ms || now_ms > envelope.deadline_ms {
            return Err(ExecutorError::InvalidDeadline);
        }
        if envelope.action_type != ExecutionActionType::RunUnprivilegedProcess
            || envelope.operation_sha256 != operation.sha256()?
        {
            return Err(ExecutorError::InvalidOperation);
        }
        let keys = operation.environment.keys().cloned().collect::<Vec<_>>();
        if keys != envelope.environment_keys {
            return Err(ExecutorError::InvalidOperation);
        }
        self.validate_path(Path::new(&operation.executable), true)?;
        let cwd = self.validate_path(Path::new(&operation.working_directory), false)?;
        if execution_path_sha256(self.identity.platform, &operation.working_directory)
            .map_err(|_| ExecutorError::UnsafePath)?
            != envelope.working_directory_sha256
            || !cwd.is_dir()
        {
            return Err(ExecutorError::UnsafePath);
        }
        for target in &envelope.targets {
            self.deny_protected(Path::new(&target.canonical_path))?;
        }
        let mut cwd_targets = envelope.targets.iter().filter(|target| {
            target.platform == self.identity.platform
                && target.canonical_path == operation.working_directory
        });
        let cwd_target = cwd_targets.next().ok_or(ExecutorError::InvalidOperation)?;
        if cwd_targets.next().is_some() || cwd_target.expected_object_sha256.is_none() {
            return Err(ExecutorError::InvalidOperation);
        }
        let mut executable_targets = envelope.targets.iter().filter(|target| {
            target.platform == self.identity.platform
                && target.canonical_path == operation.executable
        });
        let executable_target = executable_targets.next();
        if executable_targets.next().is_some()
            || self.identity.platform == ExecutionHostPlatform::Windows
                && executable_target
                    .and_then(|target| target.expected_object_sha256)
                    .is_none()
        {
            return Err(ExecutorError::InvalidOperation);
        }
        let prepared = platform::prepare(operation, cwd_target, executable_target)?;

        let mut replay = self
            .replay
            .lock()
            .map_err(|_| ExecutorError::StateUnavailable)?;
        let expected = *replay
            .next_sequence
            .entry(envelope.child_epoch_id)
            .or_insert(1);
        if replay.failed_epochs.contains(&envelope.child_epoch_id)
            || envelope.action_sequence != expected
            || replay.actions.contains(&envelope.action_id)
            || replay.nonces.contains(&envelope.nonce)
        {
            return Err(ExecutorError::Replay);
        }
        let next = expected.checked_add(1).ok_or(ExecutorError::Replay)?;
        replay.next_sequence.insert(envelope.child_epoch_id, next);
        replay.actions.insert(envelope.action_id);
        replay.nonces.insert(envelope.nonce);
        Ok(ExecutorAdmission {
            policy: self,
            envelope,
            operation,
            authority_revision,
            prepared,
        })
    }

    fn validate_path(&self, path: &Path, executable: bool) -> Result<PathBuf, ExecutorError> {
        self.deny_protected(path)?;
        let canonical = canonical_ordinary(path)?;
        let metadata = fs::metadata(&canonical).map_err(|_| ExecutorError::UnsafePath)?;
        if metadata.is_file() && link_count(&canonical, &metadata) != 1
            || !metadata.is_file() && executable
        {
            return Err(ExecutorError::UnsafePath);
        }
        #[cfg(unix)]
        if executable {
            use std::os::unix::fs::PermissionsExt;
            if metadata.permissions().mode() & 0o111 == 0 {
                return Err(ExecutorError::UnsafePath);
            }
        }
        Ok(canonical)
    }

    fn deny_protected(&self, path: &Path) -> Result<(), ExecutorError> {
        let candidate = compare_path(self.identity.platform, path)?;
        let separator = match self.identity.platform {
            ExecutionHostPlatform::Windows => '\\',
            ExecutionHostPlatform::Macos => '/',
        };
        if self
            .protected_roots
            .iter()
            .any(|root| candidate == *root || candidate.starts_with(&format!("{root}{separator}")))
        {
            return Err(ExecutorError::UnsafePath);
        }
        Ok(())
    }

    fn claim_execution(&self, envelope: &ExecutionActionEnvelope) -> Result<(), ExecutorError> {
        let now = now_ms()?;
        if now < envelope.issued_at_ms || now > envelope.deadline_ms {
            self.fail_execution(envelope);
            return Err(ExecutorError::InvalidDeadline);
        }
        let mut replay = self
            .replay
            .lock()
            .map_err(|_| ExecutorError::StateUnavailable)?;
        let expected = replay.next_execution.get(&envelope.child_epoch_id).copied();
        if replay.failed_epochs.contains(&envelope.child_epoch_id) {
            return Err(ExecutorError::Replay);
        }
        // A later admitted action may race this claim, but it must never
        // invalidate the action already at the resume boundary.
        if replay
            .active_execution
            .contains_key(&envelope.child_epoch_id)
        {
            return Err(ExecutorError::Replay);
        }
        if expected != Some(envelope.action_sequence) {
            replay.failed_epochs.insert(envelope.child_epoch_id);
            return Err(ExecutorError::Replay);
        }
        replay
            .active_execution
            .insert(envelope.child_epoch_id, envelope.action_sequence);
        Ok(())
    }

    fn complete_execution_resume(
        &self,
        envelope: &ExecutionActionEnvelope,
    ) -> Result<(), ExecutorError> {
        let mut replay = self
            .replay
            .lock()
            .map_err(|_| ExecutorError::StateUnavailable)?;
        if replay.failed_epochs.contains(&envelope.child_epoch_id)
            || replay.active_execution.get(&envelope.child_epoch_id)
                != Some(&envelope.action_sequence)
            || replay.next_execution.get(&envelope.child_epoch_id)
                != Some(&envelope.action_sequence)
        {
            replay.failed_epochs.insert(envelope.child_epoch_id);
            replay.active_execution.remove(&envelope.child_epoch_id);
            return Err(ExecutorError::Replay);
        }
        let next = envelope
            .action_sequence
            .checked_add(1)
            .ok_or(ExecutorError::Replay)?;
        replay.active_execution.remove(&envelope.child_epoch_id);
        replay.next_execution.insert(envelope.child_epoch_id, next);
        Ok(())
    }

    fn fail_execution(&self, envelope: &ExecutionActionEnvelope) {
        if let Ok(mut replay) = self.replay.lock() {
            if replay
                .active_execution
                .get(&envelope.child_epoch_id)
                .is_some_and(|sequence| *sequence != envelope.action_sequence)
            {
                return;
            }
            replay.active_execution.remove(&envelope.child_epoch_id);
            replay.failed_epochs.insert(envelope.child_epoch_id);
        }
    }

    fn validate_live_authority(
        &self,
        envelope: &ExecutionActionEnvelope,
    ) -> Result<u64, ExecutorError> {
        let live = self
            .live_authority
            .lock()
            .map_err(|_| ExecutorError::StateUnavailable)?;
        if live.authority_revision != envelope.authority_revision
            || live.emergency_paused
            || live.revoked
            || live.session_id != envelope.session_id
            || live.session_revision != envelope.session_revision
            || live.child_epoch_id != envelope.child_epoch_id
            || live.child_epoch_revision != envelope.child_epoch_revision
            || live.feature_lifecycle_revision != envelope.feature_lifecycle_revision
        {
            return Err(ExecutorError::InvalidIdentity);
        }
        Ok(live.authority_revision)
    }

    fn revalidate_and_resume(
        &self,
        envelope: &ExecutionActionEnvelope,
        operation: &UnprivilegedProcessOperation,
        admitted_authority_revision: u64,
        resume: &mut dyn FnMut() -> Result<(), ExecutorError>,
    ) -> Result<(), ExecutorError> {
        envelope
            .verify_signature(&self.identity.authority_verifying_key)
            .map_err(|_| ExecutorError::InvalidIdentity)?;
        let now = now_ms()?;
        if now < envelope.issued_at_ms
            || now > envelope.deadline_ms
            || envelope.host_platform != self.identity.platform
        {
            return Err(ExecutorError::InvalidDeadline);
        }
        if envelope.child_epoch_id != self.identity.bound_child_epoch_id
            || envelope.child_epoch_revision != self.identity.bound_child_epoch_revision
            || envelope.session_id != self.identity.bound_session_id
            || envelope.session_revision != self.identity.bound_session_revision
            || envelope.feature_lifecycle_revision != self.identity.bound_feature_lifecycle_revision
            || envelope.signer_key_id != self.identity.authority_key_id
            || envelope.executor_id != self.identity.executor_id
            || envelope.executor_revision != self.identity.executor_revision
            || envelope.executor_executable_sha256 != self.identity.executor_executable_sha256
            || envelope.broker_id != self.identity.broker_id
            || envelope.broker_revision != self.identity.broker_revision
            || envelope.broker_executable_sha256 != self.identity.broker_executable_sha256
            || envelope.protected_control_plane_sha256
                != self.identity.protected_control_plane_sha256
        {
            return Err(ExecutorError::InvalidIdentity);
        }
        if envelope.action_type != ExecutionActionType::RunUnprivilegedProcess
            || envelope.operation_sha256 != operation.sha256()?
            || operation.environment.keys().cloned().collect::<Vec<_>>()
                != envelope.environment_keys
        {
            return Err(ExecutorError::InvalidOperation);
        }
        self.deny_protected(Path::new(&operation.executable))?;
        self.deny_protected(Path::new(&operation.working_directory))?;
        for target in &envelope.targets {
            self.deny_protected(Path::new(&target.canonical_path))?;
        }
        // Hold the mutable authority snapshot and replay state across the OS
        // resume call. A concurrent pause/revocation update therefore lands
        // either entirely before this check (and denies) or after the child is
        // already within its assigned Job.
        let live = self
            .live_authority
            .lock()
            .map_err(|_| ExecutorError::StateUnavailable)?;
        if live.authority_revision != admitted_authority_revision
            || envelope.authority_revision != admitted_authority_revision
            || live.emergency_paused
            || live.revoked
            || live.session_id != envelope.session_id
            || live.session_revision != envelope.session_revision
            || live.child_epoch_id != envelope.child_epoch_id
            || live.child_epoch_revision != envelope.child_epoch_revision
            || live.feature_lifecycle_revision != envelope.feature_lifecycle_revision
        {
            return Err(ExecutorError::InvalidIdentity);
        }
        let replay = self
            .replay
            .lock()
            .map_err(|_| ExecutorError::StateUnavailable)?;
        if replay.failed_epochs.contains(&envelope.child_epoch_id)
            || replay
                .next_sequence
                .get(&envelope.child_epoch_id)
                .is_none_or(|next| *next <= envelope.action_sequence)
            || replay.next_execution.get(&envelope.child_epoch_id)
                != Some(&envelope.action_sequence)
            || replay.active_execution.get(&envelope.child_epoch_id)
                != Some(&envelope.action_sequence)
            || !replay.actions.contains(&envelope.action_id)
            || !replay.nonces.contains(&envelope.nonce)
        {
            return Err(ExecutorError::Replay);
        }
        resume()
    }
}

impl<'a> ExecutorAdmission<'a> {
    pub fn spawn(self) -> Result<OwnedExecution, ExecutorError> {
        self.policy.claim_execution(self.envelope)?;
        let process = platform::spawn(&self.prepared, self.operation, |resume| {
            self.policy.revalidate_and_resume(
                self.envelope,
                self.operation,
                self.authority_revision,
                resume,
            )
        });
        let process = match process {
            Ok(process) => process,
            Err(error) => {
                self.policy.fail_execution(self.envelope);
                return Err(error);
            }
        };
        if let Err(error) = self.policy.complete_execution_resume(self.envelope) {
            drop(process);
            self.policy.fail_execution(self.envelope);
            return Err(error);
        }
        Ok(OwnedExecution {
            child_epoch_id: self.envelope.child_epoch_id,
            platform: self.policy.identity.platform,
            receipt_key_id: self.policy.identity.receipt_key_id.clone(),
            receipt_signing_key: self.policy.identity.receipt_signing_key.clone(),
            process,
        })
    }
}

impl OwnedExecution {
    pub fn terminate(
        mut self,
        mode: ExecutionTerminationMode,
        last_checkpoint_sha256: [u8; 32],
        graceful_window: Duration,
        forced_window: Duration,
    ) -> Result<ExecutionTerminationReceipt, ExecutorError> {
        if last_checkpoint_sha256 == [0; 32] {
            return Err(ExecutorError::InvalidOperation);
        }
        let evidence = self
            .process
            .terminate(mode, graceful_window, forced_window)?;
        let outcome = if evidence.group_empty {
            ExecutionTerminationOutcome::Reaped
        } else {
            ExecutionTerminationOutcome::Incomplete
        };
        let mut receipt = ExecutionTerminationReceipt {
            schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
            receipt_id: Uuid::new_v4(),
            child_epoch_id: self.child_epoch_id,
            mode,
            outcome,
            tracked_root_process_count: 1,
            graceful_root_termination_count: u32::from(evidence.graceful && !evidence.forced),
            forced_root_termination_count: u32::from(evidence.forced),
            reaped_root_process_count: u32::from(evidence.root_reaped),
            survivor_root_process_count: u32::from(!evidence.root_reaped),
            descendant_scope: match self.platform {
                ExecutionHostPlatform::Macos => {
                    assemblywright_protocol::ExecutionDescendantScope::MacosProcessGroup
                }
                ExecutionHostPlatform::Windows => {
                    assemblywright_protocol::ExecutionDescendantScope::WindowsJobObject
                }
            },
            descendants_reaped: evidence.group_empty,
            last_checkpoint_sha256,
            observed_at_ms: now_ms()?,
            signer_key_id: self.receipt_key_id.clone(),
            signature: Vec::new(),
        };
        receipt
            .sign(&self.receipt_signing_key)
            .map_err(|_| ExecutorError::InvalidIdentity)?;
        Ok(receipt)
    }
}

fn now_ms() -> Result<u64, ExecutorError> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| ExecutorError::StateUnavailable)?
        .as_millis();
    u64::try_from(millis).map_err(|_| ExecutorError::StateUnavailable)
}

fn canonical_ordinary(path: &Path) -> Result<PathBuf, ExecutorError> {
    if !path.is_absolute() {
        return Err(ExecutorError::UnsafePath);
    }
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir | Component::Prefix(_) | Component::Normal(_) => {
                current.push(component)
            }
            _ => return Err(ExecutorError::UnsafePath),
        }
        let metadata = fs::symlink_metadata(&current).map_err(|_| ExecutorError::UnsafePath)?;
        if metadata.file_type().is_symlink() || is_windows_reparse_point(&current) {
            return Err(ExecutorError::UnsafePath);
        }
    }
    let canonical = path.canonicalize().map_err(|_| ExecutorError::UnsafePath)?;
    if !same_canonical_path(&canonical, path) {
        return Err(ExecutorError::UnsafePath);
    }
    Ok(canonical)
}

#[cfg(windows)]
fn is_windows_reparse_point(path: &Path) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0)
        .unwrap_or(true)
}

#[cfg(not(windows))]
fn is_windows_reparse_point(_path: &Path) -> bool {
    false
}

fn compare_path(platform: ExecutionHostPlatform, path: &Path) -> Result<String, ExecutorError> {
    let value = path.to_str().ok_or(ExecutorError::UnsafePath)?;
    Ok(match platform {
        ExecutionHostPlatform::Windows => value
            .strip_prefix(r"\\?\")
            .unwrap_or(value)
            .replace('/', "\\")
            .to_ascii_lowercase(),
        ExecutionHostPlatform::Macos => value.to_string(),
    })
}

#[cfg(windows)]
fn same_canonical_path(left: &Path, right: &Path) -> bool {
    compare_path(ExecutionHostPlatform::Windows, left).ok()
        == compare_path(ExecutionHostPlatform::Windows, right).ok()
}

#[cfg(not(windows))]
fn same_canonical_path(left: &Path, right: &Path) -> bool {
    left == right
}

#[cfg(unix)]
fn link_count(_path: &Path, metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.nlink()
}

#[cfg(windows)]
fn link_count(path: &Path, _metadata: &fs::Metadata) -> u64 {
    use std::fs::OpenOptions;
    use std::mem::zeroed;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS,
    };

    let Ok(file) = OpenOptions::new()
        .access_mode(windows_sys::Win32::Storage::FileSystem::FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
    else {
        return 0;
    };
    let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) } == 0 {
        0
    } else {
        information.nNumberOfLinks as u64
    }
}

#[cfg(not(any(unix, windows)))]
fn link_count(_path: &Path, _metadata: &fs::Metadata) -> u64 {
    0
}

#[derive(Debug, Clone, Copy)]
struct TerminationEvidence {
    graceful: bool,
    forced: bool,
    root_reaped: bool,
    group_empty: bool,
}

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use std::ffi::{c_char, c_int, c_void, CString};
    use std::mem::zeroed;
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
    use std::os::unix::ffi::OsStrExt;
    use std::ptr::null_mut;
    use std::thread;
    use std::time::Instant;

    const TRUSTED_EXECUTABLE_WRITE_BITS: libc::mode_t = libc::S_IWGRP | libc::S_IWOTH;
    const ACL_TYPE_EXTENDED: c_int = 0x0000_0100;
    const ACL_FIRST_ENTRY: c_int = 0;

    pub(super) struct PreparedProcess {
        _executable_parents: Vec<OwnedFd>,
        executable: OwnedFd,
        working_directory_parents: Vec<OwnedFd>,
        working_directory: OwnedFd,
        null_device: OwnedFd,
        executable_identity: FileIdentity,
        signed_target: HeldTargetIdentity,
    }

    pub(super) struct ContainedProcess {
        root_process: libc::pid_t,
        process_group: i32,
        root_reaped: bool,
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    struct FileIdentity {
        device: libc::dev_t,
        inode: libc::ino_t,
    }

    struct HeldTargetIdentity {
        canonical_path: PathBuf,
        canonical_parent: PathBuf,
        canonical_parent_sha256: [u8; 32],
        expected_object_sha256: [u8; 32],
    }

    pub(super) fn prepare(
        operation: &UnprivilegedProcessOperation,
        target: &ExecutionTargetIdentity,
        _executable_target: Option<&ExecutionTargetIdentity>,
    ) -> Result<PreparedProcess, ExecutorError> {
        let (executable_parents, executable) = open_held_path(
            Path::new(&operation.executable),
            libc::O_EXEC | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )?;
        for parent in &executable_parents {
            require_directory(parent.as_raw_fd())?;
            require_worker_immutable(parent.as_raw_fd())?;
        }
        let executable_stat = stat_fd(executable.as_raw_fd())?;
        if executable_stat.st_mode & libc::S_IFMT != libc::S_IFREG
            || executable_stat.st_nlink != 1
            || executable_stat.st_mode & (libc::S_IXUSR | libc::S_IXGRP | libc::S_IXOTH) == 0
        {
            return Err(ExecutorError::UnsafePath);
        }
        require_worker_immutable(executable.as_raw_fd())?;

        let (working_directory_parents, working_directory) = open_held_path(
            Path::new(&operation.working_directory),
            libc::O_SEARCH | libc::O_CLOEXEC | libc::O_NOFOLLOW,
        )?;
        for parent in &working_directory_parents {
            require_directory(parent.as_raw_fd())?;
        }
        require_directory(working_directory.as_raw_fd())?;
        let canonical_path = PathBuf::from(&target.canonical_path);
        let canonical_parent = canonical_path
            .parent()
            .ok_or(ExecutorError::UnsafePath)?
            .to_path_buf();
        let signed_target = HeldTargetIdentity {
            canonical_path,
            canonical_parent,
            canonical_parent_sha256: target.canonical_parent_sha256,
            expected_object_sha256: target
                .expected_object_sha256
                .ok_or(ExecutorError::InvalidOperation)?,
        };
        verify_target_identity(
            &signed_target,
            working_directory_parents
                .last()
                .ok_or(ExecutorError::UnsafePath)?
                .as_raw_fd(),
            working_directory.as_raw_fd(),
        )?;

        let null_path = CString::new("/dev/null").expect("static path has no NUL");
        let null_fd = unsafe {
            libc::open(
                null_path.as_ptr(),
                libc::O_RDWR | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if null_fd < 0 {
            return Err(ExecutorError::ContainmentFailed);
        }
        let null_device = unsafe { OwnedFd::from_raw_fd(null_fd) };
        Ok(PreparedProcess {
            _executable_parents: executable_parents,
            executable_identity: FileIdentity {
                device: executable_stat.st_dev,
                inode: executable_stat.st_ino,
            },
            executable,
            working_directory_parents,
            working_directory,
            null_device,
            signed_target,
        })
    }

    pub(super) fn spawn<F>(
        prepared: &PreparedProcess,
        operation: &UnprivilegedProcessOperation,
        _before_resume: F,
    ) -> Result<ContainedProcess, ExecutorError>
    where
        F: FnOnce(&mut dyn FnMut() -> Result<(), ExecutorError>) -> Result<(), ExecutorError>,
    {
        prepared.verify_signed_target()?;
        if !macos_descendant_containment_available() {
            return Err(ExecutorError::ContainmentFailed);
        }
        // This launch code remains dormant while descendant containment is
        // unavailable. Darwin has no fexecve/execveat, so any future activation
        // must retain this worker-race boundary as well: immutable no-follow
        // executable descriptors/parents, descriptor-derived kernel path, and an
        // identity-matched O_NOFOLLOW_ANY guard immediately before posix_spawn.
        let executable_path = fd_path(prepared.executable.as_raw_fd())?;
        let executable_guard = open_absolute(
            &executable_path,
            libc::O_EXEC | libc::O_CLOEXEC | libc::O_NOFOLLOW_ANY,
        )?;
        let guard_stat = stat_fd(executable_guard.as_raw_fd())?;
        if (FileIdentity {
            device: guard_stat.st_dev,
            inode: guard_stat.st_ino,
        }) != prepared.executable_identity
        {
            return Err(ExecutorError::UnsafePath);
        }

        let executable = CString::new(executable_path.as_os_str().as_bytes())
            .map_err(|_| ExecutorError::InvalidOperation)?;
        let mut argument_storage = Vec::with_capacity(operation.arguments.len() + 1);
        argument_storage.push(
            CString::new(operation.executable.as_bytes())
                .map_err(|_| ExecutorError::InvalidOperation)?,
        );
        for argument in &operation.arguments {
            argument_storage.push(
                CString::new(argument.as_bytes()).map_err(|_| ExecutorError::InvalidOperation)?,
            );
        }
        let mut arguments = argument_storage
            .iter()
            .map(|value| value.as_ptr().cast_mut())
            .collect::<Vec<*mut c_char>>();
        arguments.push(null_mut());

        let mut environment_storage = Vec::with_capacity(operation.environment.len());
        for (key, value) in &operation.environment {
            environment_storage.push(
                CString::new(format!("{key}={value}"))
                    .map_err(|_| ExecutorError::InvalidOperation)?,
            );
        }
        let mut environment = environment_storage
            .iter()
            .map(|value| value.as_ptr().cast_mut())
            .collect::<Vec<*mut c_char>>();
        environment.push(null_mut());

        let mut actions = SpawnFileActions::new()?;
        actions.fchdir(prepared.working_directory.as_raw_fd())?;
        for target in [libc::STDIN_FILENO, libc::STDOUT_FILENO, libc::STDERR_FILENO] {
            actions.dup2(prepared.null_device.as_raw_fd(), target)?;
        }
        let mut attributes = SpawnAttributes::new()?;
        attributes.process_group()?;
        let mut root_process = 0;
        let result = unsafe {
            libc::posix_spawn(
                &mut root_process,
                executable.as_ptr(),
                &actions.raw,
                &attributes.raw,
                arguments.as_ptr(),
                environment.as_ptr(),
            )
        };
        if result != 0 || root_process <= 0 {
            return Err(ExecutorError::ContainmentFailed);
        }
        Ok(ContainedProcess {
            root_process,
            process_group: root_process,
            root_reaped: false,
        })
    }

    impl PreparedProcess {
        fn verify_signed_target(&self) -> Result<(), ExecutorError> {
            verify_target_identity(
                &self.signed_target,
                self.working_directory_parents
                    .last()
                    .ok_or(ExecutorError::UnsafePath)?
                    .as_raw_fd(),
                self.working_directory.as_raw_fd(),
            )
        }
    }

    fn macos_descendant_containment_available() -> bool {
        // Process groups are advisory membership, not containment: untrusted
        // descendants may call setsid/setpgid and escape group-directed stop.
        // Darwin exposes no unprivileged Job-Object/process-reaper equivalent,
        // so arbitrary process execution remains disabled.
        false
    }

    impl ContainedProcess {
        pub(super) fn terminate(
            &mut self,
            mode: ExecutionTerminationMode,
            graceful_window: Duration,
            forced_window: Duration,
        ) -> Result<TerminationEvidence, ExecutorError> {
            let signal = match mode {
                ExecutionTerminationMode::Stop => libc::SIGTERM,
                ExecutionTerminationMode::EmergencyPause => libc::SIGKILL,
            };
            if unsafe { libc::kill(-self.process_group, signal) } != 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return Err(ExecutorError::ContainmentFailed);
                }
            }
            let graceful = signal == libc::SIGTERM;
            let mut forced = signal == libc::SIGKILL;
            let first_deadline = Instant::now()
                + if graceful {
                    graceful_window
                } else {
                    forced_window
                };
            while Instant::now() < first_deadline {
                if self.try_reap()? && group_empty(self.process_group) {
                    return Ok(TerminationEvidence {
                        graceful,
                        forced,
                        root_reaped: true,
                        group_empty: true,
                    });
                }
                thread::sleep(Duration::from_millis(10));
            }
            if graceful {
                forced = true;
                let _ = unsafe { libc::kill(-self.process_group, libc::SIGKILL) };
            }
            let deadline = Instant::now() + forced_window;
            while Instant::now() < deadline {
                self.try_reap()?;
                if group_empty(self.process_group) {
                    self.reap_blocking()?;
                    return Ok(TerminationEvidence {
                        graceful,
                        forced,
                        root_reaped: true,
                        group_empty: true,
                    });
                }
                thread::sleep(Duration::from_millis(10));
            }
            Ok(TerminationEvidence {
                graceful,
                forced,
                root_reaped: self.root_reaped,
                group_empty: false,
            })
        }

        fn try_reap(&mut self) -> Result<bool, ExecutorError> {
            if self.root_reaped {
                return Ok(true);
            }
            let mut status = 0;
            let result = unsafe { libc::waitpid(self.root_process, &mut status, libc::WNOHANG) };
            if result == self.root_process {
                self.root_reaped = true;
                Ok(true)
            } else if result == 0 {
                Ok(false)
            } else {
                Err(ExecutorError::ContainmentFailed)
            }
        }

        fn reap_blocking(&mut self) -> Result<(), ExecutorError> {
            if self.root_reaped {
                return Ok(());
            }
            let mut status = 0;
            if unsafe { libc::waitpid(self.root_process, &mut status, 0) } != self.root_process {
                return Err(ExecutorError::ContainmentFailed);
            }
            self.root_reaped = true;
            Ok(())
        }
    }

    impl Drop for ContainedProcess {
        fn drop(&mut self) {
            let _ = unsafe { libc::kill(-self.process_group, libc::SIGKILL) };
            let deadline = Instant::now() + Duration::from_secs(5);
            while Instant::now() < deadline {
                if self.try_reap().unwrap_or(false) && group_empty(self.process_group) {
                    return;
                }
                thread::sleep(Duration::from_millis(10));
            }
        }
    }

    fn group_empty(process_group: i32) -> bool {
        if unsafe { libc::kill(-process_group, 0) } == 0 {
            return false;
        }
        std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
    }

    fn open_held_path(
        path: &Path,
        final_flags: c_int,
    ) -> Result<(Vec<OwnedFd>, OwnedFd), ExecutorError> {
        if !path.is_absolute() {
            return Err(ExecutorError::UnsafePath);
        }
        let root = CString::new("/").expect("static path has no NUL");
        let root_fd = unsafe {
            libc::open(
                root.as_ptr(),
                libc::O_SEARCH | libc::O_CLOEXEC | libc::O_NOFOLLOW,
            )
        };
        if root_fd < 0 {
            return Err(ExecutorError::UnsafePath);
        }
        let mut current = unsafe { OwnedFd::from_raw_fd(root_fd) };
        let components = path
            .components()
            .filter_map(|component| match component {
                Component::RootDir => None,
                Component::Normal(value) => Some(value),
                _ => Some(std::ffi::OsStr::new("")),
            })
            .collect::<Vec<_>>();
        if components.iter().any(|value| value.is_empty()) {
            return Err(ExecutorError::UnsafePath);
        }
        if components.is_empty() {
            return Ok((Vec::new(), current));
        }
        let mut parents = Vec::with_capacity(components.len());
        for (index, component) in components.iter().enumerate() {
            let name = CString::new(component.as_bytes()).map_err(|_| ExecutorError::UnsafePath)?;
            let flags = if index + 1 == components.len() {
                final_flags
            } else {
                libc::O_SEARCH | libc::O_CLOEXEC | libc::O_NOFOLLOW
            };
            let next = unsafe { libc::openat(current.as_raw_fd(), name.as_ptr(), flags) };
            if next < 0 {
                return Err(ExecutorError::UnsafePath);
            }
            parents.push(current);
            current = unsafe { OwnedFd::from_raw_fd(next) };
        }
        Ok((parents, current))
    }

    fn open_absolute(path: &Path, flags: c_int) -> Result<OwnedFd, ExecutorError> {
        let path =
            CString::new(path.as_os_str().as_bytes()).map_err(|_| ExecutorError::UnsafePath)?;
        let fd = unsafe { libc::open(path.as_ptr(), flags) };
        if fd < 0 {
            Err(ExecutorError::UnsafePath)
        } else {
            Ok(unsafe { OwnedFd::from_raw_fd(fd) })
        }
    }

    fn stat_fd(fd: RawFd) -> Result<libc::stat, ExecutorError> {
        let mut stat = unsafe { zeroed() };
        if unsafe { libc::fstat(fd, &mut stat) } != 0 {
            Err(ExecutorError::UnsafePath)
        } else {
            Ok(stat)
        }
    }

    fn verify_target_identity(
        target: &HeldTargetIdentity,
        parent_fd: RawFd,
        object_fd: RawFd,
    ) -> Result<(), ExecutorError> {
        require_directory(parent_fd)?;
        require_directory(object_fd)?;
        if held_object_identity_sha256(&target.canonical_parent, parent_fd)?
            != target.canonical_parent_sha256
            || held_object_identity_sha256(&target.canonical_path, object_fd)?
                != target.expected_object_sha256
        {
            return Err(ExecutorError::UnsafePath);
        }
        Ok(())
    }

    fn held_object_identity_sha256(path: &Path, fd: RawFd) -> Result<[u8; 32], ExecutorError> {
        let path = path.to_str().ok_or(ExecutorError::UnsafePath)?;
        let stat = stat_fd(fd)?;
        let mut hasher = Sha256::new();
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update((stat.st_dev as u64).to_le_bytes());
        hasher.update(stat.st_ino.to_le_bytes());
        Ok(hasher.finalize().into())
    }

    fn require_directory(fd: RawFd) -> Result<(), ExecutorError> {
        if stat_fd(fd)?.st_mode & libc::S_IFMT == libc::S_IFDIR {
            Ok(())
        } else {
            Err(ExecutorError::UnsafePath)
        }
    }

    fn require_worker_immutable(fd: RawFd) -> Result<(), ExecutorError> {
        let stat = stat_fd(fd)?;
        if stat.st_uid != 0 || stat.st_mode & TRUSTED_EXECUTABLE_WRITE_BITS != 0 {
            return Err(ExecutorError::UnsafePath);
        }
        require_no_extended_acl(fd)?;
        let path = fd_path(fd)?;
        let path =
            CString::new(path.as_os_str().as_bytes()).map_err(|_| ExecutorError::UnsafePath)?;
        if unsafe { libc::faccessat(libc::AT_FDCWD, path.as_ptr(), libc::W_OK, libc::AT_EACCESS) }
            == 0
        {
            return Err(ExecutorError::UnsafePath);
        }
        match std::io::Error::last_os_error().raw_os_error() {
            Some(libc::EACCES) | Some(libc::EPERM) | Some(libc::EROFS) => Ok(()),
            _ => Err(ExecutorError::UnsafePath),
        }
    }

    fn require_no_extended_acl(fd: RawFd) -> Result<(), ExecutorError> {
        let acl = unsafe { acl_get_fd_np(fd, ACL_TYPE_EXTENDED) };
        if acl.is_null() {
            return match std::io::Error::last_os_error().raw_os_error() {
                Some(libc::ENOENT) => Ok(()),
                _ => Err(ExecutorError::UnsafePath),
            };
        }
        let mut entry = null_mut();
        let has_entry = unsafe { acl_get_entry(acl, ACL_FIRST_ENTRY, &mut entry) } == 0;
        let freed = unsafe { acl_free(acl) } == 0;
        if has_entry || !freed {
            Err(ExecutorError::UnsafePath)
        } else {
            Ok(())
        }
    }

    fn fd_path(fd: RawFd) -> Result<PathBuf, ExecutorError> {
        let mut bytes = vec![0_u8; libc::PATH_MAX as usize];
        if unsafe { libc::fcntl(fd, libc::F_GETPATH, bytes.as_mut_ptr()) } != 0 {
            return Err(ExecutorError::UnsafePath);
        }
        let end = bytes
            .iter()
            .position(|byte| *byte == 0)
            .ok_or(ExecutorError::UnsafePath)?;
        bytes.truncate(end);
        Ok(PathBuf::from(std::ffi::OsStr::from_bytes(&bytes)))
    }

    struct SpawnFileActions {
        raw: libc::posix_spawn_file_actions_t,
    }

    impl SpawnFileActions {
        fn new() -> Result<Self, ExecutorError> {
            let mut raw = null_mut();
            if unsafe { libc::posix_spawn_file_actions_init(&mut raw) } != 0 {
                Err(ExecutorError::ContainmentFailed)
            } else {
                Ok(Self { raw })
            }
        }

        fn fchdir(&mut self, fd: RawFd) -> Result<(), ExecutorError> {
            if unsafe { posix_spawn_file_actions_addfchdir_np(&mut self.raw, fd) } != 0 {
                Err(ExecutorError::ContainmentFailed)
            } else {
                Ok(())
            }
        }

        fn dup2(&mut self, fd: RawFd, target: RawFd) -> Result<(), ExecutorError> {
            if unsafe { libc::posix_spawn_file_actions_adddup2(&mut self.raw, fd, target) } != 0 {
                Err(ExecutorError::ContainmentFailed)
            } else {
                Ok(())
            }
        }
    }

    impl Drop for SpawnFileActions {
        fn drop(&mut self) {
            let _ = unsafe { libc::posix_spawn_file_actions_destroy(&mut self.raw) };
        }
    }

    struct SpawnAttributes {
        raw: libc::posix_spawnattr_t,
    }

    impl SpawnAttributes {
        fn new() -> Result<Self, ExecutorError> {
            let mut raw = null_mut();
            if unsafe { libc::posix_spawnattr_init(&mut raw) } != 0 {
                Err(ExecutorError::ContainmentFailed)
            } else {
                Ok(Self { raw })
            }
        }

        fn process_group(&mut self) -> Result<(), ExecutorError> {
            let flags = (libc::POSIX_SPAWN_SETPGROUP | libc::POSIX_SPAWN_CLOEXEC_DEFAULT)
                .try_into()
                .map_err(|_| ExecutorError::ContainmentFailed)?;
            if unsafe { libc::posix_spawnattr_setpgroup(&mut self.raw, 0) } != 0
                || unsafe { libc::posix_spawnattr_setflags(&mut self.raw, flags) } != 0
            {
                Err(ExecutorError::ContainmentFailed)
            } else {
                Ok(())
            }
        }
    }

    impl Drop for SpawnAttributes {
        fn drop(&mut self) {
            let _ = unsafe { libc::posix_spawnattr_destroy(&mut self.raw) };
        }
    }

    unsafe extern "C" {
        fn posix_spawn_file_actions_addfchdir_np(
            actions: *mut libc::posix_spawn_file_actions_t,
            fd: c_int,
        ) -> c_int;
        fn acl_get_fd_np(fd: c_int, acl_type: c_int) -> *mut c_void;
        fn acl_get_entry(acl: *mut c_void, entry_id: c_int, entry: *mut *mut c_void) -> c_int;
        fn acl_free(acl: *mut c_void) -> c_int;
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::ffi::{c_void, OsStr};
    use std::mem::{size_of, zeroed};
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle, RawHandle};
    use std::ptr::{null, null_mut};
    use std::thread;
    use std::time::Instant;
    use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, GetFileInformationByHandle, GetFinalPathNameByHandleW,
        BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_EXECUTE, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_LIST_DIRECTORY, FILE_NAME_NORMALIZED, FILE_READ_ATTRIBUTES, FILE_SHARE_READ,
        OPEN_EXISTING, VOLUME_NAME_DOS,
    };
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicAccountingInformation,
        JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
        TerminateJobObject, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    use windows_sys::Win32::System::Threading::{
        CreateProcessW, QueryFullProcessImageNameW, ResumeThread, TerminateProcess,
        WaitForSingleObject, CREATE_NO_WINDOW, CREATE_SUSPENDED, CREATE_UNICODE_ENVIRONMENT,
        PROCESS_INFORMATION, PROCESS_NAME_WIN32, STARTUPINFOW,
    };

    const FAILURE_EXIT_CODE: u32 = 0xA55E_0001;
    const TERMINATION_EXIT_CODE: u32 = 0xA55E_0002;
    const FAILURE_REAP_TIMEOUT_MS: u32 = 5_000;

    pub(super) struct PreparedProcess {
        executable: HeldPath,
        working_directory: HeldPath,
        signed_executable: HeldTargetIdentity,
        signed_target: HeldTargetIdentity,
    }

    pub(super) struct ContainedProcess {
        process: OwnedHandle,
        job: OwnedHandle,
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    struct WindowsFileIdentity {
        volume_serial: u32,
        file_index_high: u32,
        file_index_low: u32,
    }

    struct HeldPath {
        handles: Vec<OwnedHandle>,
        identities: Vec<WindowsFileIdentity>,
        canonical_path: String,
        directory: bool,
    }

    struct HeldTargetIdentity {
        canonical_path: String,
        canonical_parent: String,
        canonical_parent_sha256: [u8; 32],
        expected_object_sha256: [u8; 32],
    }

    pub(super) fn prepare(
        operation: &UnprivilegedProcessOperation,
        target: &ExecutionTargetIdentity,
        executable_target: Option<&ExecutionTargetIdentity>,
    ) -> Result<PreparedProcess, ExecutorError> {
        validate_windows_environment(&operation.environment)?;
        let executable = HeldPath::open(&operation.executable, false)?;
        if executable.final_information()?.nNumberOfLinks != 1 {
            return Err(ExecutorError::UnsafePath);
        }
        let executable_target = executable_target.ok_or(ExecutorError::InvalidOperation)?;
        let executable_parent = Path::new(&executable_target.canonical_path)
            .parent()
            .and_then(Path::to_str)
            .ok_or(ExecutorError::UnsafePath)?
            .to_string();
        let signed_executable = HeldTargetIdentity {
            canonical_path: executable_target.canonical_path.clone(),
            canonical_parent: executable_parent,
            canonical_parent_sha256: executable_target.canonical_parent_sha256,
            expected_object_sha256: executable_target
                .expected_object_sha256
                .ok_or(ExecutorError::InvalidOperation)?,
        };
        let working_directory = HeldPath::open(&operation.working_directory, true)?;
        let canonical_parent = Path::new(&target.canonical_path)
            .parent()
            .and_then(Path::to_str)
            .ok_or(ExecutorError::UnsafePath)?
            .to_string();
        let signed_target = HeldTargetIdentity {
            canonical_path: target.canonical_path.clone(),
            canonical_parent,
            canonical_parent_sha256: target.canonical_parent_sha256,
            expected_object_sha256: target
                .expected_object_sha256
                .ok_or(ExecutorError::InvalidOperation)?,
        };
        let prepared = PreparedProcess {
            executable,
            working_directory,
            signed_executable,
            signed_target,
        };
        prepared.revalidate()?;
        Ok(prepared)
    }

    pub(super) fn spawn<F>(
        prepared: &PreparedProcess,
        operation: &UnprivilegedProcessOperation,
        before_resume: F,
    ) -> Result<ContainedProcess, ExecutorError>
    where
        F: FnOnce(&mut dyn FnMut() -> Result<(), ExecutorError>) -> Result<(), ExecutorError>,
    {
        prepared.revalidate()?;
        let job = owned_handle(unsafe { CreateJobObjectW(null(), null()) })?;
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                raw_handle(&job),
                JobObjectExtendedLimitInformation,
                &limits as *const _ as *const c_void,
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
        {
            return Err(ExecutorError::ContainmentFailed);
        }
        let executable_path = prepared.executable.final_path()?;
        let cwd_path = prepared.working_directory.final_path()?;
        let mut command_line = windows_command_line(&executable_path, &operation.arguments)?;
        let mut environment = windows_environment(&operation.environment)?;
        let executable = wide(&executable_path)?;
        let cwd = wide(&cwd_path)?;
        let mut startup: STARTUPINFOW = unsafe { zeroed() };
        startup.cb = size_of::<STARTUPINFOW>() as u32;
        let mut process: PROCESS_INFORMATION = unsafe { zeroed() };
        let created = unsafe {
            CreateProcessW(
                executable.as_ptr(),
                command_line.as_mut_ptr(),
                null(),
                null(),
                0,
                CREATE_SUSPENDED | CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT,
                environment.as_mut_ptr() as *const c_void,
                cwd.as_ptr(),
                &startup,
                &mut process,
            )
        };
        if created == 0 {
            return Err(ExecutorError::ContainmentFailed);
        }
        // CreateProcessW success guarantees two valid handles. Convert both
        // without a fallible gap where the suspended process would exist
        // without its termination guard.
        let mut guard = SuspendedChildGuard {
            process: Some(unsafe { OwnedHandle::from_raw_handle(process.hProcess as RawHandle) }),
            thread: Some(unsafe { OwnedHandle::from_raw_handle(process.hThread as RawHandle) }),
            job: Some(job),
            assigned: false,
        };

        let actual_image = query_process_image_path(guard.process_raw()?)?;
        let actual_image_guard = HeldPath::open(&actual_image, false)?;
        if actual_image_guard.final_identity()? != prepared.executable.final_identity()?
            || windows_path_comparison(&actual_image)
                != windows_path_comparison(&operation.executable)
        {
            return Err(ExecutorError::UnsafePath);
        }
        if unsafe { AssignProcessToJobObject(guard.job_raw()?, guard.process_raw()?) } == 0 {
            return Err(ExecutorError::ContainmentFailed);
        }
        guard.assigned = true;
        prepared.revalidate()?;
        let mut resume = || {
            if unsafe { ResumeThread(guard.thread_raw()?) } == 1 {
                Ok(())
            } else {
                Err(ExecutorError::ContainmentFailed)
            }
        };
        before_resume(&mut resume)?;
        guard.into_contained()
    }

    impl ContainedProcess {
        pub(super) fn terminate(
            &mut self,
            _mode: ExecutionTerminationMode,
            _graceful_window: Duration,
            forced_window: Duration,
        ) -> Result<TerminationEvidence, ExecutorError> {
            if unsafe { TerminateJobObject(raw_handle(&self.job), TERMINATION_EXIT_CODE) } == 0 {
                return Err(ExecutorError::ContainmentFailed);
            }
            let deadline = Instant::now() + forced_window;
            while Instant::now() < deadline {
                let root_reaped =
                    unsafe { WaitForSingleObject(raw_handle(&self.process), 0) } == WAIT_OBJECT_0;
                let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
                let queried = unsafe {
                    QueryInformationJobObject(
                        raw_handle(&self.job),
                        JobObjectBasicAccountingInformation,
                        &mut accounting as *mut _ as *mut c_void,
                        size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                        null_mut(),
                    )
                };
                if root_reaped && queried != 0 && accounting.ActiveProcesses == 0 {
                    return Ok(TerminationEvidence {
                        graceful: false,
                        // Job emptiness and a signaled root are proven. The
                        // race between natural exit and TerminateJobObject is
                        // not observable, so causal forced-root count remains 0.
                        forced: false,
                        root_reaped: true,
                        group_empty: true,
                    });
                }
                thread::sleep(Duration::from_millis(10));
            }
            Ok(TerminationEvidence {
                graceful: false,
                forced: false,
                root_reaped: unsafe { WaitForSingleObject(raw_handle(&self.process), 0) }
                    == WAIT_OBJECT_0,
                group_empty: false,
            })
        }
    }

    impl Drop for ContainedProcess {
        fn drop(&mut self) {
            unsafe {
                TerminateJobObject(raw_handle(&self.job), TERMINATION_EXIT_CODE);
                TerminateProcess(raw_handle(&self.process), TERMINATION_EXIT_CODE);
                WaitForSingleObject(raw_handle(&self.process), 5_000);
            }
        }
    }

    impl PreparedProcess {
        fn revalidate(&self) -> Result<(), ExecutorError> {
            self.executable.revalidate()?;
            self.working_directory.revalidate()?;
            self.executable
                .verify_signed_identity(&self.signed_executable)?;
            self.working_directory
                .verify_signed_identity(&self.signed_target)?;
            Ok(())
        }
    }

    impl HeldPath {
        fn open(path: &str, directory: bool) -> Result<Self, ExecutorError> {
            let canonical_path = windows_plain_path(path)?;
            let components = windows_path_prefixes(&canonical_path)?;
            let mut handles = Vec::with_capacity(components.len());
            let mut identities = Vec::with_capacity(components.len());
            for (index, component) in components.iter().enumerate() {
                let is_final = index + 1 == components.len();
                let handle = open_component(component, !is_final || directory)?;
                let information = information(raw_handle(&handle))?;
                if information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
                    || (!is_final || directory)
                        && information.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
                    || is_final
                        && !directory
                        && information.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0
                {
                    return Err(ExecutorError::UnsafePath);
                }
                identities.push(identity(&information));
                handles.push(handle);
            }
            let held = Self {
                handles,
                identities,
                canonical_path,
                directory,
            };
            held.revalidate()?;
            Ok(held)
        }

        fn revalidate(&self) -> Result<(), ExecutorError> {
            for (index, handle) in self.handles.iter().enumerate() {
                let current = information(raw_handle(handle))?;
                if current.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
                    || identity(&current) != self.identities[index]
                {
                    return Err(ExecutorError::UnsafePath);
                }
            }
            let final_info = self.final_information()?;
            if self.directory != (final_info.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0)
                || windows_path_comparison(&self.final_path()?)
                    != windows_path_comparison(&self.canonical_path)
            {
                return Err(ExecutorError::UnsafePath);
            }
            Ok(())
        }

        fn verify_signed_identity(&self, signed: &HeldTargetIdentity) -> Result<(), ExecutorError> {
            if self.handles.len() < 2 {
                return Err(ExecutorError::UnsafePath);
            }
            let parent = self.information(self.handles.len() - 2)?;
            let object = self.final_information()?;
            if windows_object_identity_sha256(&signed.canonical_parent, &parent)
                != signed.canonical_parent_sha256
                || windows_object_identity_sha256(&signed.canonical_path, &object)
                    != signed.expected_object_sha256
            {
                return Err(ExecutorError::UnsafePath);
            }
            Ok(())
        }

        fn final_handle(&self) -> Result<HANDLE, ExecutorError> {
            self.handles
                .last()
                .map(raw_handle)
                .ok_or(ExecutorError::UnsafePath)
        }

        fn final_information(&self) -> Result<BY_HANDLE_FILE_INFORMATION, ExecutorError> {
            information(self.final_handle()?)
        }

        fn final_identity(&self) -> Result<WindowsFileIdentity, ExecutorError> {
            Ok(identity(&self.final_information()?))
        }

        fn information(&self, index: usize) -> Result<BY_HANDLE_FILE_INFORMATION, ExecutorError> {
            self.handles
                .get(index)
                .map(raw_handle)
                .ok_or(ExecutorError::UnsafePath)
                .and_then(information)
        }

        fn final_path(&self) -> Result<String, ExecutorError> {
            final_path(self.final_handle()?)
        }
    }

    struct SuspendedChildGuard {
        process: Option<OwnedHandle>,
        thread: Option<OwnedHandle>,
        job: Option<OwnedHandle>,
        assigned: bool,
    }

    impl SuspendedChildGuard {
        fn process_raw(&self) -> Result<HANDLE, ExecutorError> {
            self.process
                .as_ref()
                .map(raw_handle)
                .ok_or(ExecutorError::ContainmentFailed)
        }

        fn thread_raw(&self) -> Result<HANDLE, ExecutorError> {
            self.thread
                .as_ref()
                .map(raw_handle)
                .ok_or(ExecutorError::ContainmentFailed)
        }

        fn job_raw(&self) -> Result<HANDLE, ExecutorError> {
            self.job
                .as_ref()
                .map(raw_handle)
                .ok_or(ExecutorError::ContainmentFailed)
        }

        fn into_contained(mut self) -> Result<ContainedProcess, ExecutorError> {
            let process = self
                .process
                .take()
                .ok_or(ExecutorError::ContainmentFailed)?;
            let job = self.job.take().ok_or(ExecutorError::ContainmentFailed)?;
            self.thread.take();
            Ok(ContainedProcess { process, job })
        }
    }

    impl Drop for SuspendedChildGuard {
        fn drop(&mut self) {
            if let Some(process) = &self.process {
                unsafe {
                    if self.assigned {
                        if let Some(job) = &self.job {
                            TerminateJobObject(raw_handle(job), FAILURE_EXIT_CODE);
                        }
                    }
                    TerminateProcess(raw_handle(process), FAILURE_EXIT_CODE);
                    // Drop cannot emit termination evidence. Bound cleanup so
                    // an anomalous kernel wait cannot hang executor authority;
                    // assigned children also remain covered by kill-on-close.
                    WaitForSingleObject(raw_handle(process), FAILURE_REAP_TIMEOUT_MS);
                }
            }
        }
    }

    fn open_component(path: &str, directory: bool) -> Result<OwnedHandle, ExecutorError> {
        let path = wide(path)?;
        // Attribute-only directory opens are exempt from Windows share-mode
        // enforcement. FILE_LIST_DIRECTORY makes omitted delete sharing an
        // actual rename/delete lease for every retained directory component.
        let access = FILE_READ_ATTRIBUTES
            | if directory {
                FILE_LIST_DIRECTORY
            } else {
                FILE_EXECUTE
            };
        let flags = FILE_FLAG_OPEN_REPARSE_POINT
            | if directory {
                FILE_FLAG_BACKUP_SEMANTICS
            } else {
                0
            };
        owned_handle(unsafe {
            CreateFileW(
                path.as_ptr(),
                access,
                FILE_SHARE_READ,
                null(),
                OPEN_EXISTING,
                flags,
                null_mut(),
            )
        })
        .map_err(|_| ExecutorError::UnsafePath)
    }

    fn owned_handle(handle: HANDLE) -> Result<OwnedHandle, ExecutorError> {
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            Err(ExecutorError::ContainmentFailed)
        } else {
            Ok(unsafe { OwnedHandle::from_raw_handle(handle as RawHandle) })
        }
    }

    fn raw_handle(handle: &OwnedHandle) -> HANDLE {
        handle.as_raw_handle() as HANDLE
    }

    fn information(handle: HANDLE) -> Result<BY_HANDLE_FILE_INFORMATION, ExecutorError> {
        let mut information = unsafe { zeroed() };
        if unsafe { GetFileInformationByHandle(handle, &mut information) } == 0 {
            Err(ExecutorError::UnsafePath)
        } else {
            Ok(information)
        }
    }

    fn identity(information: &BY_HANDLE_FILE_INFORMATION) -> WindowsFileIdentity {
        WindowsFileIdentity {
            volume_serial: information.dwVolumeSerialNumber,
            file_index_high: information.nFileIndexHigh,
            file_index_low: information.nFileIndexLow,
        }
    }

    fn windows_object_identity_sha256(
        path: &str,
        information: &BY_HANDLE_FILE_INFORMATION,
    ) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(windows_path_comparison(path).as_bytes());
        hasher.update([0]);
        hasher.update(information.dwVolumeSerialNumber.to_le_bytes());
        hasher.update(information.nFileIndexHigh.to_le_bytes());
        hasher.update(information.nFileIndexLow.to_le_bytes());
        hasher.finalize().into()
    }

    fn final_path(handle: HANDLE) -> Result<String, ExecutorError> {
        let mut buffer = vec![0_u16; 32_768];
        let length = unsafe {
            GetFinalPathNameByHandleW(
                handle,
                buffer.as_mut_ptr(),
                buffer.len() as u32,
                FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
            )
        };
        if length == 0 || length as usize >= buffer.len() {
            return Err(ExecutorError::UnsafePath);
        }
        buffer.truncate(length as usize);
        String::from_utf16(&buffer).map_err(|_| ExecutorError::UnsafePath)
    }

    fn query_process_image_path(process: HANDLE) -> Result<String, ExecutorError> {
        let mut buffer = vec![0_u16; 32_768];
        let mut length = buffer.len() as u32;
        if unsafe {
            QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_WIN32,
                buffer.as_mut_ptr(),
                &mut length,
            )
        } == 0
            || length == 0
            || length as usize >= buffer.len()
        {
            return Err(ExecutorError::UnsafePath);
        }
        buffer.truncate(length as usize);
        String::from_utf16(&buffer).map_err(|_| ExecutorError::UnsafePath)
    }

    fn windows_plain_path(path: &str) -> Result<String, ExecutorError> {
        let plain = path
            .strip_prefix(r"\\?\")
            .unwrap_or(path)
            .replace('/', "\\");
        if plain.as_bytes().get(1) != Some(&b':') || plain.as_bytes().get(2) != Some(&b'\\') {
            return Err(ExecutorError::UnsafePath);
        }
        Ok(plain)
    }

    fn windows_path_comparison(path: &str) -> String {
        path.strip_prefix(r"\\?\")
            .unwrap_or(path)
            .replace('/', "\\")
            .to_ascii_lowercase()
    }

    fn windows_path_prefixes(path: &str) -> Result<Vec<String>, ExecutorError> {
        let plain = windows_plain_path(path)?;
        let mut prefixes = vec![plain[..3].to_string()];
        let mut current = plain[..3].to_string();
        for component in plain[3..].split('\\') {
            if component.is_empty() || component == "." || component == ".." {
                return Err(ExecutorError::UnsafePath);
            }
            if !current.ends_with('\\') {
                current.push('\\');
            }
            current.push_str(component);
            prefixes.push(current.clone());
        }
        Ok(prefixes)
    }

    fn wide(value: &str) -> Result<Vec<u16>, ExecutorError> {
        if value.contains('\0') {
            return Err(ExecutorError::InvalidOperation);
        }
        Ok(OsStr::new(value).encode_wide().chain(Some(0)).collect())
    }

    fn windows_command_line(
        executable: &str,
        arguments: &[String],
    ) -> Result<Vec<u16>, ExecutorError> {
        let mut value = quote(executable);
        for argument in arguments {
            value.push(' ');
            value.push_str(&quote(argument));
        }
        wide(&value)
    }

    fn quote(value: &str) -> String {
        if !value.is_empty()
            && !value
                .chars()
                .any(|character| character.is_whitespace() || character == '"')
        {
            return value.to_string();
        }
        let mut quoted = String::from("\"");
        let mut backslashes = 0usize;
        for character in value.chars() {
            if character == '\\' {
                backslashes += 1;
            } else if character == '"' {
                quoted.push_str(&"\\".repeat(backslashes * 2 + 1));
                quoted.push('"');
                backslashes = 0;
            } else {
                quoted.push_str(&"\\".repeat(backslashes));
                backslashes = 0;
                quoted.push(character);
            }
        }
        quoted.push_str(&"\\".repeat(backslashes * 2));
        quoted.push('"');
        quoted
    }

    fn validate_windows_environment(
        environment: &BTreeMap<String, String>,
    ) -> Result<(), ExecutorError> {
        let mut keys = HashSet::new();
        if environment
            .keys()
            .any(|key| !keys.insert(key.to_ascii_uppercase()))
        {
            Err(ExecutorError::InvalidOperation)
        } else {
            Ok(())
        }
    }

    fn windows_environment(
        environment: &BTreeMap<String, String>,
    ) -> Result<Vec<u16>, ExecutorError> {
        validate_windows_environment(environment)?;
        let mut entries = environment.iter().collect::<Vec<_>>();
        entries.sort_by_key(|(key, _)| key.to_ascii_uppercase());
        let mut block = Vec::new();
        for (key, value) in entries {
            if key.contains('\0') || value.contains('\0') {
                return Err(ExecutorError::InvalidOperation);
            }
            block.extend(OsStr::new(&format!("{key}={value}")).encode_wide());
            block.push(0);
        }
        if block.is_empty() {
            block.push(0);
        }
        block.push(0);
        Ok(block)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn command_line_quoting_and_environment_are_byte_exact() {
            assert_eq!(quote(""), "\"\"");
            assert_eq!(quote("plain"), "plain");
            assert_eq!(quote("a b"), "\"a b\"");
            assert_eq!(quote("a\"b"), "\"a\\\"b\"");
            assert_eq!(quote("a b\\"), "\"a b\\\\\"");
            assert_eq!(windows_environment(&BTreeMap::new()).unwrap(), vec![0, 0]);

            let mut collision = BTreeMap::new();
            collision.insert("Path".into(), "one".into());
            collision.insert("PATH".into(), "two".into());
            assert_eq!(
                windows_environment(&collision).unwrap_err(),
                ExecutorError::InvalidOperation
            );
        }

        #[test]
        fn path_walk_includes_volume_root_and_every_component() {
            assert_eq!(
                windows_path_prefixes(r"C:\work\feature").unwrap(),
                vec![r"C:\", r"C:\work", r"C:\work\feature"]
            );
            assert_eq!(
                windows_path_prefixes(r"C:\work\..\control").unwrap_err(),
                ExecutorError::UnsafePath
            );
        }
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod platform {
    use super::*;
    pub(super) struct PreparedProcess;
    pub(super) struct ContainedProcess;
    pub(super) fn prepare(
        _operation: &UnprivilegedProcessOperation,
        _target: &ExecutionTargetIdentity,
        _executable_target: Option<&ExecutionTargetIdentity>,
    ) -> Result<PreparedProcess, ExecutorError> {
        Err(ExecutorError::ContainmentFailed)
    }
    pub(super) fn spawn<F>(
        _prepared: &PreparedProcess,
        _operation: &UnprivilegedProcessOperation,
        _before_resume: F,
    ) -> Result<ContainedProcess, ExecutorError>
    where
        F: FnOnce(&mut dyn FnMut() -> Result<(), ExecutorError>) -> Result<(), ExecutorError>,
    {
        Err(ExecutorError::ContainmentFailed)
    }
    impl ContainedProcess {
        pub(super) fn terminate(
            &mut self,
            _mode: ExecutionTerminationMode,
            _graceful_window: Duration,
            _forced_window: Duration,
        ) -> Result<TerminationEvidence, ExecutorError> {
            Err(ExecutorError::ContainmentFailed)
        }
    }
}
