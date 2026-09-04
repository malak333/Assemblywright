use assemblywright_protocol::{
    execution_path_sha256, ExecutionActionEnvelope, ExecutionActionType,
    ExecutionCancellationBehavior, ExecutionEffectClassification, ExecutionHostPlatform,
    ExecutionReconciliationStrategy, ExecutionTargetIdentity, ProtectedControlPlanePathManifest,
};
use ed25519_dalek::VerifyingKey;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

pub mod runtime;
pub mod startup;
#[cfg(windows)]
pub mod windows_service_host;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum BrokerError {
    #[error("broker is not configured for this host")]
    WrongHost,
    #[error("signed action identity or executable binding is invalid")]
    InvalidIdentity,
    #[error("signed action is expired or from the future")]
    InvalidDeadline,
    #[error("signed action replay or sequence gap")]
    Replay,
    #[error("operation digest or type mismatch")]
    InvalidOperation,
    #[error("target path identity is ambiguous")]
    AmbiguousTarget,
    #[error("target is in the protected Assemblywright control plane")]
    ProtectedTarget,
    #[error("target is not an ordinary single-link filesystem object")]
    UnsafeLink,
    #[error("broker state lock is unavailable")]
    StateUnavailable,
    #[error("broker effect failed")]
    EffectFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum BrokerOperation {
    CreateDirectory {
        target: String,
    },
    ReplaceFile {
        target: String,
        content_sha256: [u8; 32],
    },
    RemoveFile {
        target: String,
    },
    SetRestrictedServiceEnabled {
        service_identity: String,
        enabled: bool,
    },
}

impl BrokerOperation {
    pub fn action_type(&self) -> ExecutionActionType {
        match self {
            Self::CreateDirectory { .. } => ExecutionActionType::CreateDirectory,
            Self::ReplaceFile { .. } => ExecutionActionType::ReplaceFile,
            Self::RemoveFile { .. } => ExecutionActionType::RemoveFile,
            Self::SetRestrictedServiceEnabled { .. } => {
                ExecutionActionType::SetRestrictedServiceEnabled
            }
        }
    }

    pub fn sha256(&self) -> Result<[u8; 32], BrokerError> {
        let bytes = serde_json::to_vec(self).map_err(|_| BrokerError::InvalidOperation)?;
        Ok(Sha256::digest(bytes).into())
    }

    fn target(&self) -> Option<&str> {
        match self {
            Self::CreateDirectory { target }
            | Self::ReplaceFile { target, .. }
            | Self::RemoveFile { target } => Some(target),
            Self::SetRestrictedServiceEnabled { .. } => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BrokerIdentity {
    pub platform: ExecutionHostPlatform,
    pub broker_id: Uuid,
    pub broker_revision: u64,
    pub broker_executable_sha256: [u8; 32],
    pub executor_id: Uuid,
    pub executor_revision: u64,
    pub executor_executable_sha256: [u8; 32],
    pub protected_control_plane_sha256: [u8; 32],
    pub signer_key_id: String,
    pub verifying_key: VerifyingKey,
    /// Restored from the Windows-authoritative durable action ledger. A broker
    /// restart must receive the current child and next sequence, never reset 1.
    pub bound_child_epoch_id: Uuid,
    pub bound_session_id: Uuid,
    pub bound_session_revision: u64,
    pub bound_child_epoch_revision: u64,
    pub bound_feature_lifecycle_revision: u64,
    /// Exact durable master-ledger authority revision for this runtime.
    pub bound_authority_revision: u64,
    pub next_action_sequence: u64,
}

pub struct BrokerPolicy {
    identity: BrokerIdentity,
    protected_roots: Vec<CanonicalProtectedRoot>,
    replay: Mutex<ReplayState>,
}

fn canonical_manifest_roots(
    manifest: &ProtectedControlPlanePathManifest,
) -> Result<Vec<CanonicalProtectedRoot>, BrokerError> {
    let mut roots = Vec::with_capacity(manifest.paths().len());
    for path in manifest.paths() {
        let canonical = canonical_ordinary_existing_path(Path::new(path))?;
        roots.push(CanonicalProtectedRoot {
            comparison: path_comparison(manifest.platform, &canonical)?,
            path: canonical,
        });
    }
    Ok(roots)
}

#[derive(Default)]
struct ReplayState {
    next_sequence: HashMap<Uuid, u64>,
    seen_actions: HashSet<Uuid>,
    seen_nonces: HashSet<Uuid>,
    quarantined: bool,
}

#[derive(serde::Serialize)]
struct CanonicalProtectedRoot {
    path: PathBuf,
    comparison: String,
}

pub struct BrokerAdmission<'a> {
    #[cfg(windows)]
    policy: &'a BrokerPolicy,
    #[cfg(windows)]
    envelope: &'a ExecutionActionEnvelope,
    #[cfg(windows)]
    operation: &'a BrokerOperation,
    #[cfg(windows)]
    prepared_effect: PreparedEffect,
    #[cfg(not(windows))]
    marker: std::marker::PhantomData<(
        &'a BrokerPolicy,
        &'a ExecutionActionEnvelope,
        &'a BrokerOperation,
    )>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrokerExecutionResult {
    pub action_id: Uuid,
    pub post_state_identity_sha256: [u8; 32],
    pub post_state_sha256: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
pub enum BrokerExecutionOutcome {
    Applied {
        result: BrokerExecutionResult,
    },
    EffectPossibleReconciliationRequired {
        action_id: Uuid,
        signed_target_sha256: [u8; 32],
        observed_post_state_identity_sha256: Option<[u8; 32]>,
        observed_post_state_sha256: Option<[u8; 32]>,
    },
}

#[cfg(windows)]
enum PreparedEffect {
    CreateDirectory(WindowsCreateDirectoryEffect),
    Disabled,
}

impl BrokerPolicy {
    pub fn new(
        identity: BrokerIdentity,
        protected_manifest: ProtectedControlPlanePathManifest,
    ) -> Result<Self, BrokerError> {
        if identity.broker_id.is_nil()
            || identity.executor_id.is_nil()
            || identity.broker_id == identity.executor_id
            || identity.broker_revision == 0
            || identity.executor_revision == 0
            || identity.broker_executable_sha256 == [0; 32]
            || identity.executor_executable_sha256 == [0; 32]
            || identity.protected_control_plane_sha256 == [0; 32]
            || identity.signer_key_id.is_empty()
            || identity.bound_child_epoch_id.is_nil()
            || identity.bound_session_id.is_nil()
            || identity.bound_session_revision == 0
            || identity.bound_child_epoch_revision == 0
            || identity.bound_feature_lifecycle_revision == 0
            || identity.bound_authority_revision == 0
            || identity.next_action_sequence == 0
        {
            return Err(BrokerError::InvalidIdentity);
        }
        if protected_manifest.platform != identity.platform
            || protected_manifest
                .canonical_sha256()
                .map_err(|_| BrokerError::InvalidIdentity)?
                != identity.protected_control_plane_sha256
        {
            return Err(BrokerError::InvalidIdentity);
        }
        let mut roots = canonical_manifest_roots(&protected_manifest)?;
        roots.sort_by(|left, right| left.comparison.cmp(&right.comparison));
        roots.dedup_by(|left, right| left.comparison == right.comparison);
        let replay_seed = (identity.bound_child_epoch_id, identity.next_action_sequence);
        Ok(Self {
            identity,
            protected_roots: roots,
            replay: Mutex::new(ReplayState {
                next_sequence: HashMap::from([replay_seed]),
                ..ReplayState::default()
            }),
        })
    }

    pub fn admit<'a>(
        &'a self,
        envelope: &'a ExecutionActionEnvelope,
        operation: &'a BrokerOperation,
    ) -> Result<BrokerAdmission<'a>, BrokerError> {
        if self
            .replay
            .lock()
            .map_err(|_| BrokerError::StateUnavailable)?
            .quarantined
        {
            return Err(BrokerError::StateUnavailable);
        }
        let now_ms = system_now_ms()?;
        envelope
            .verify_signature(&self.identity.verifying_key)
            .map_err(|_| BrokerError::InvalidIdentity)?;
        if envelope.host_platform != self.identity.platform {
            return Err(BrokerError::WrongHost);
        }
        if envelope.signer_key_id != self.identity.signer_key_id
            || envelope.broker_id != self.identity.broker_id
            || envelope.broker_revision != self.identity.broker_revision
            || envelope.broker_executable_sha256 != self.identity.broker_executable_sha256
            || envelope.executor_id != self.identity.executor_id
            || envelope.executor_revision != self.identity.executor_revision
            || envelope.executor_executable_sha256 != self.identity.executor_executable_sha256
            || envelope.protected_control_plane_sha256
                != self.identity.protected_control_plane_sha256
        {
            return Err(BrokerError::InvalidIdentity);
        }
        if envelope.child_epoch_id != self.identity.bound_child_epoch_id
            || envelope.child_epoch_revision != self.identity.bound_child_epoch_revision
            || envelope.session_id != self.identity.bound_session_id
            || envelope.session_revision != self.identity.bound_session_revision
            || envelope.feature_lifecycle_revision != self.identity.bound_feature_lifecycle_revision
            || envelope.authority_revision != self.identity.bound_authority_revision
        {
            self.quarantine();
            return Err(BrokerError::Replay);
        }
        if now_ms < envelope.issued_at_ms || now_ms > envelope.deadline_ms {
            return Err(BrokerError::InvalidDeadline);
        }
        if !envelope.action_type.requires_privileged_broker()
            || envelope.action_type != operation.action_type()
            || envelope.operation_sha256 != operation.sha256()?
        {
            return Err(BrokerError::InvalidOperation);
        }
        let target = operation.target().ok_or(BrokerError::InvalidOperation)?;
        let signed_target = envelope
            .targets
            .iter()
            .find(|candidate| candidate.canonical_path == target)
            .ok_or(BrokerError::InvalidOperation)?;
        validate_effect_contract(envelope, operation)?;
        self.validate_target(signed_target, envelope.action_type)?;
        #[cfg(windows)]
        let prepared_effect = prepare_effect(self, signed_target, operation)?;

        let mut replay = self
            .replay
            .lock()
            .map_err(|_| BrokerError::StateUnavailable)?;
        if replay.quarantined {
            return Err(BrokerError::StateUnavailable);
        }
        let expected = *replay
            .next_sequence
            .entry(envelope.child_epoch_id)
            .or_insert(1);
        if envelope.action_sequence != expected
            || replay.seen_actions.contains(&envelope.action_id)
            || replay.seen_nonces.contains(&envelope.nonce)
        {
            replay.quarantined = true;
            return Err(BrokerError::Replay);
        }
        let next = expected.checked_add(1).ok_or(BrokerError::Replay)?;
        replay.next_sequence.insert(envelope.child_epoch_id, next);
        replay.seen_actions.insert(envelope.action_id);
        replay.seen_nonces.insert(envelope.nonce);
        Ok(BrokerAdmission {
            #[cfg(windows)]
            policy: self,
            #[cfg(windows)]
            envelope,
            #[cfg(windows)]
            operation,
            #[cfg(windows)]
            prepared_effect,
            #[cfg(not(windows))]
            marker: std::marker::PhantomData,
        })
    }

    fn validate_target(
        &self,
        target: &ExecutionTargetIdentity,
        action_type: ExecutionActionType,
    ) -> Result<(), BrokerError> {
        let path = PathBuf::from(&target.canonical_path);
        if execution_path_sha256(self.identity.platform, &target.canonical_path)
            .map_err(|_| BrokerError::AmbiguousTarget)?
            != target.canonical_path_sha256
        {
            return Err(BrokerError::AmbiguousTarget);
        }
        let parent = path.parent().ok_or(BrokerError::AmbiguousTarget)?;
        let canonical_parent = canonical_ordinary_existing_path(parent)?;
        if object_identity_sha256(self.identity.platform, &canonical_parent)?
            != target.canonical_parent_sha256
        {
            return Err(BrokerError::AmbiguousTarget);
        }
        let candidate_comparison = path_comparison(self.identity.platform, &path)?;
        for root in &self.protected_roots {
            let _ = &root.path;
            if candidate_comparison == root.comparison
                || candidate_comparison.starts_with(&format!(
                    "{}{}",
                    root.comparison,
                    separator(self.identity.platform)
                ))
            {
                return Err(BrokerError::ProtectedTarget);
            }
        }
        reject_case_alias(parent, &path, self.identity.platform)?;
        match fs::symlink_metadata(&path) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || link_count(&path, &metadata) != 1 {
                    return Err(BrokerError::UnsafeLink);
                }
                let expected = target
                    .expected_object_sha256
                    .ok_or(BrokerError::AmbiguousTarget)?;
                let canonical = canonical_ordinary_existing_path(&path)?;
                if canonical != path
                    || object_identity_sha256(self.identity.platform, &canonical)? != expected
                {
                    return Err(BrokerError::AmbiguousTarget);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if !matches!(
                    action_type,
                    ExecutionActionType::CreateDirectory | ExecutionActionType::ReplaceFile
                ) || target.expected_object_sha256.is_some()
                {
                    return Err(BrokerError::AmbiguousTarget);
                }
            }
            Err(_) => return Err(BrokerError::AmbiguousTarget),
        }
        Ok(())
    }

    fn quarantine(&self) {
        if let Ok(mut replay) = self.replay.lock() {
            replay.quarantined = true;
        }
    }
}

fn system_now_ms() -> Result<u64, BrokerError> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| BrokerError::InvalidDeadline)?
            .as_millis(),
    )
    .map_err(|_| BrokerError::InvalidDeadline)
}

#[cfg(windows)]
impl BrokerAdmission<'_> {
    /// Only the Windows create-directory adapter has effect authority. Every
    /// other host and operation remains closed.
    fn execute(self) -> Result<BrokerExecutionOutcome, BrokerError> {
        match self.prepared_effect {
            PreparedEffect::CreateDirectory(effect) => {
                effect.execute(self.policy, self.envelope, self.operation)
            }
            PreparedEffect::Disabled => {
                let _ = (self.policy, self.envelope, self.operation);
                Err(BrokerError::InvalidOperation)
            }
        }
    }
}

/// Dedicated native proof seam for the first atomic Windows adapter. The
/// long-running broker runtime does not call this until asynchronous active-
/// effect termination and durable reconciliation are implemented.
#[cfg(windows)]
pub struct WindowsCreateDirectoryProof<'a> {
    policy: &'a BrokerPolicy,
    admission: BrokerAdmission<'a>,
}

#[cfg(windows)]
impl WindowsCreateDirectoryProof<'_> {
    pub fn execute(self) -> Result<BrokerExecutionOutcome, BrokerError> {
        finish_windows_proof(self.policy, self.admission.execute())
    }
}

#[cfg(windows)]
fn finish_windows_proof(
    policy: &BrokerPolicy,
    result: Result<BrokerExecutionOutcome, BrokerError>,
) -> Result<BrokerExecutionOutcome, BrokerError> {
    let outcome = match result {
        Ok(outcome) => outcome,
        Err(error) => {
            policy.quarantine();
            return Err(error);
        }
    };
    if matches!(
        outcome,
        BrokerExecutionOutcome::EffectPossibleReconciliationRequired { .. }
    ) {
        policy.quarantine();
    }
    Ok(outcome)
}

#[cfg(windows)]
pub fn prepare_windows_create_directory_proof<'a>(
    policy: &'a BrokerPolicy,
    envelope: &'a ExecutionActionEnvelope,
    operation: &'a BrokerOperation,
) -> Result<WindowsCreateDirectoryProof<'a>, BrokerError> {
    if !matches!(operation, BrokerOperation::CreateDirectory { .. }) {
        return Err(BrokerError::InvalidOperation);
    }
    let admission = policy.admit(envelope, operation)?;
    Ok(WindowsCreateDirectoryProof { policy, admission })
}

#[cfg(windows)]
pub fn execute_windows_create_directory_once(
    policy: &BrokerPolicy,
    envelope: &ExecutionActionEnvelope,
    operation: &BrokerOperation,
) -> Result<BrokerExecutionOutcome, BrokerError> {
    prepare_windows_create_directory_proof(policy, envelope, operation)?.execute()
}

fn validate_effect_contract(
    envelope: &ExecutionActionEnvelope,
    operation: &BrokerOperation,
) -> Result<(), BrokerError> {
    if matches!(operation, BrokerOperation::CreateDirectory { .. })
        && (envelope.targets.len() != 1
            || envelope.effect_classification != ExecutionEffectClassification::LocalDurable
            || envelope.cancellation_behavior
                != ExecutionCancellationBehavior::CheckpointThenTerminate
            || envelope.reconciliation_strategy
                != ExecutionReconciliationStrategy::ExactPostStateDigest)
    {
        return Err(BrokerError::InvalidOperation);
    }
    Ok(())
}

#[cfg(windows)]
fn prepare_effect(
    policy: &BrokerPolicy,
    target: &ExecutionTargetIdentity,
    operation: &BrokerOperation,
) -> Result<PreparedEffect, BrokerError> {
    if policy.identity.platform == ExecutionHostPlatform::Windows
        && matches!(operation, BrokerOperation::CreateDirectory { .. })
    {
        return WindowsCreateDirectoryEffect::prepare(target).map(PreparedEffect::CreateDirectory);
    }
    let _ = (policy, target, operation);
    Ok(PreparedEffect::Disabled)
}

#[cfg(windows)]
struct WindowsCreateDirectoryEffect {
    ancestors: Vec<WindowsRetainedAncestor>,
    parent_path: PathBuf,
    target_path: PathBuf,
    leaf_name: Vec<u16>,
    signed_parent_identity_sha256: [u8; 32],
}

#[cfg(windows)]
struct WindowsRetainedAncestor {
    path: PathBuf,
    handle: fs::File,
    identity_sha256: [u8; 32],
}

#[cfg(windows)]
impl WindowsCreateDirectoryEffect {
    fn prepare(target: &ExecutionTargetIdentity) -> Result<Self, BrokerError> {
        use std::os::windows::ffi::OsStrExt;

        if target.expected_object_sha256.is_some() {
            return Err(BrokerError::AmbiguousTarget);
        }
        let target_path = PathBuf::from(&target.canonical_path);
        let parent_path = target_path
            .parent()
            .ok_or(BrokerError::AmbiguousTarget)?
            .to_path_buf();
        let leaf = target_path
            .file_name()
            .ok_or(BrokerError::AmbiguousTarget)?;
        validate_windows_leaf_name(leaf)?;
        let leaf_name: Vec<u16> = leaf.encode_wide().collect();
        let byte_length = leaf_name
            .len()
            .checked_mul(std::mem::size_of::<u16>())
            .and_then(|length| u16::try_from(length).ok())
            .ok_or(BrokerError::AmbiguousTarget)?;
        if byte_length == 0 {
            return Err(BrokerError::AmbiguousTarget);
        }

        let mut ancestor_paths: Vec<PathBuf> = parent_path
            .ancestors()
            .filter(|path| !path.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .collect();
        ancestor_paths.reverse();
        if ancestor_paths.is_empty() {
            return Err(BrokerError::AmbiguousTarget);
        }
        let mut ancestors = Vec::with_capacity(ancestor_paths.len());
        for path in ancestor_paths {
            let handle = open_windows_directory_no_reparse(&path)?;
            let information = windows_file_information_from_handle(&handle)?;
            if information.dwFileAttributes & WINDOWS_FILE_ATTRIBUTE_DIRECTORY == 0
                || information.dwFileAttributes & WINDOWS_FILE_ATTRIBUTE_REPARSE_POINT != 0
            {
                return Err(BrokerError::UnsafeLink);
            }
            let identity_sha256 =
                windows_identity_sha256(ExecutionHostPlatform::Windows, &path, &information)?;
            ancestors.push(WindowsRetainedAncestor {
                path,
                handle,
                identity_sha256,
            });
        }
        if ancestors.last().map(|ancestor| ancestor.identity_sha256)
            != Some(target.canonical_parent_sha256)
        {
            return Err(BrokerError::AmbiguousTarget);
        }
        Ok(Self {
            ancestors,
            parent_path,
            target_path,
            leaf_name,
            signed_parent_identity_sha256: target.canonical_parent_sha256,
        })
    }

    fn execute(
        self,
        policy: &BrokerPolicy,
        envelope: &ExecutionActionEnvelope,
        operation: &BrokerOperation,
    ) -> Result<BrokerExecutionOutcome, BrokerError> {
        self.execute_with_post_create_verifier(policy, envelope, operation, Self::verify_created)
    }

    fn execute_with_post_create_verifier<F>(
        self,
        policy: &BrokerPolicy,
        envelope: &ExecutionActionEnvelope,
        operation: &BrokerOperation,
        verifier: F,
    ) -> Result<BrokerExecutionOutcome, BrokerError>
    where
        F: FnOnce(
            &Self,
            &fs::File,
            &ExecutionActionEnvelope,
            &ExecutionTargetIdentity,
        ) -> Result<BrokerExecutionResult, BrokerError>,
    {
        if !matches!(operation, BrokerOperation::CreateDirectory { .. })
            || system_now_ms()? > envelope.deadline_ms
        {
            return Err(BrokerError::InvalidOperation);
        }
        let target = envelope
            .targets
            .first()
            .ok_or(BrokerError::InvalidOperation)?;

        // This is the final path-policy check. The retained parent identity is
        // then compared with the current path identity before the relative
        // native create, so a rename or reparse substitution cannot redirect it.
        policy.validate_target(target, ExecutionActionType::CreateDirectory)?;
        for ancestor in &self.ancestors {
            let retained = windows_file_information_from_handle(&ancestor.handle)?;
            let current = windows_file_information(&ancestor.path)?;
            if !same_windows_file(&retained, &current)
                || windows_identity_sha256(
                    ExecutionHostPlatform::Windows,
                    &ancestor.path,
                    &retained,
                )? != ancestor.identity_sha256
            {
                return Err(BrokerError::AmbiguousTarget);
            }
        }
        if self
            .ancestors
            .last()
            .map(|ancestor| ancestor.identity_sha256)
            != Some(self.signed_parent_identity_sha256)
        {
            return Err(BrokerError::AmbiguousTarget);
        }

        let parent = &self
            .ancestors
            .last()
            .ok_or(BrokerError::AmbiguousTarget)?
            .handle;
        let created = match nt_create_directory_relative(parent, &self.leaf_name) {
            Ok(created) => created,
            Err(NativeCreateFailure::BeforeEffect(error)) => return Err(error),
            Err(NativeCreateFailure::EffectPossible) => {
                return Ok(reconciliation_required(envelope, target, None));
            }
        };
        let observed_information = windows_file_information_from_handle(&created).ok();
        let observed_identity = observed_information.as_ref().and_then(|information| {
            windows_identity_sha256(
                ExecutionHostPlatform::Windows,
                &self.target_path,
                information,
            )
            .ok()
        });
        let verified = verifier(&self, &created, envelope, target);
        match verified {
            Ok(result) => Ok(BrokerExecutionOutcome::Applied { result }),
            Err(_) => Ok(reconciliation_required(envelope, target, observed_identity)),
        }
    }

    fn verify_created(
        &self,
        created: &fs::File,
        envelope: &ExecutionActionEnvelope,
        target: &ExecutionTargetIdentity,
    ) -> Result<BrokerExecutionResult, BrokerError> {
        let information = windows_file_information_from_handle(created)?;
        if information.dwFileAttributes & WINDOWS_FILE_ATTRIBUTE_DIRECTORY == 0
            || information.dwFileAttributes & WINDOWS_FILE_ATTRIBUTE_REPARSE_POINT != 0
            || information.nNumberOfLinks != 1
        {
            return Err(BrokerError::UnsafeLink);
        }

        reject_case_alias(
            &self.parent_path,
            &self.target_path,
            ExecutionHostPlatform::Windows,
        )?;
        let canonical_created = canonical_ordinary_existing_path(&self.target_path)?;
        let path_handle = open_windows_directory_no_reparse(&canonical_created)?;
        let path_information = windows_file_information_from_handle(&path_handle)?;
        if !same_windows_file(&information, &path_information)
            || path_information.dwFileAttributes & WINDOWS_FILE_ATTRIBUTE_DIRECTORY == 0
            || path_information.dwFileAttributes & WINDOWS_FILE_ATTRIBUTE_REPARSE_POINT != 0
            || path_information.nNumberOfLinks != 1
        {
            return Err(BrokerError::AmbiguousTarget);
        }

        let post_state_identity_sha256 = windows_identity_sha256(
            ExecutionHostPlatform::Windows,
            &canonical_created,
            &information,
        )?;
        Ok(BrokerExecutionResult {
            action_id: envelope.action_id,
            post_state_identity_sha256,
            post_state_sha256: post_state_sha256(
                envelope.action_id,
                target.canonical_path_sha256,
                post_state_identity_sha256,
            ),
        })
    }
}

#[cfg(windows)]
fn reconciliation_required(
    envelope: &ExecutionActionEnvelope,
    target: &ExecutionTargetIdentity,
    observed_identity: Option<[u8; 32]>,
) -> BrokerExecutionOutcome {
    let observed_post_state_sha256 = observed_identity.map(|identity| {
        post_state_sha256(envelope.action_id, target.canonical_path_sha256, identity)
    });
    BrokerExecutionOutcome::EffectPossibleReconciliationRequired {
        action_id: envelope.action_id,
        signed_target_sha256: target.canonical_path_sha256,
        observed_post_state_identity_sha256: observed_identity,
        observed_post_state_sha256,
    }
}

#[cfg(windows)]
fn post_state_sha256(
    action_id: Uuid,
    signed_target_sha256: [u8; 32],
    post_state_identity_sha256: [u8; 32],
) -> [u8; 32] {
    let mut post_state = Sha256::new();
    post_state.update(b"assemblywright.broker.create-directory.post-state.v1\0");
    post_state.update(action_id.as_bytes());
    post_state.update(signed_target_sha256);
    post_state.update(post_state_identity_sha256);
    post_state.finalize().into()
}

#[cfg(windows)]
const WINDOWS_FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
#[cfg(windows)]
const WINDOWS_FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

#[cfg(windows)]
fn validate_windows_leaf_name(name: &std::ffi::OsStr) -> Result<(), BrokerError> {
    let name = name.to_str().ok_or(BrokerError::AmbiguousTarget)?;
    if name.is_empty()
        || name.ends_with('.')
        || name.ends_with(' ')
        || name
            .bytes()
            .any(|byte| byte == 0 || byte == b'/' || byte == b'\\' || byte == b':' || byte < 0x20)
    {
        return Err(BrokerError::AmbiguousTarget);
    }
    let device_stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    if matches!(device_stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || device_stem.strip_prefix("COM").is_some_and(|suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        })
        || device_stem.strip_prefix("LPT").is_some_and(|suffix| {
            matches!(suffix, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
        })
    {
        return Err(BrokerError::AmbiguousTarget);
    }
    Ok(())
}

#[cfg(windows)]
fn open_windows_directory_no_reparse(path: &Path) -> Result<fs::File, BrokerError> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_LIST_DIRECTORY,
        FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    fs::OpenOptions::new()
        .access_mode(FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| BrokerError::AmbiguousTarget)
}

#[cfg(windows)]
#[allow(non_snake_case)]
#[repr(C)]
struct NtUnicodeString {
    Length: u16,
    MaximumLength: u16,
    Buffer: *mut u16,
}

#[cfg(windows)]
#[allow(non_snake_case)]
#[repr(C)]
struct NtObjectAttributes {
    Length: u32,
    RootDirectory: windows_sys::Win32::Foundation::HANDLE,
    ObjectName: *mut NtUnicodeString,
    Attributes: u32,
    SecurityDescriptor: *mut std::ffi::c_void,
    SecurityQualityOfService: *mut std::ffi::c_void,
}

#[cfg(windows)]
#[repr(C)]
struct NtIoStatusBlock {
    status_or_pointer: isize,
    information: usize,
}

#[cfg(windows)]
#[link(name = "ntdll")]
extern "system" {
    fn NtCreateFile(
        file_handle: *mut windows_sys::Win32::Foundation::HANDLE,
        desired_access: u32,
        object_attributes: *const NtObjectAttributes,
        io_status_block: *mut NtIoStatusBlock,
        allocation_size: *const i64,
        file_attributes: u32,
        share_access: u32,
        create_disposition: u32,
        create_options: u32,
        ea_buffer: *const std::ffi::c_void,
        ea_length: u32,
    ) -> i32;
}

#[cfg(windows)]
enum NativeCreateFailure {
    BeforeEffect(BrokerError),
    EffectPossible,
}

#[cfg(windows)]
fn classify_nt_create_completion(
    status: i32,
    raw_handle: windows_sys::Win32::Foundation::HANDLE,
) -> Result<(), NativeCreateFailure> {
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;

    // Once the native create call has been entered, a failure status or a
    // missing handle cannot prove that the target was never materialized. The
    // caller must reconcile and quarantine instead of reporting a terminal
    // pre-effect failure.
    if status < 0 || raw_handle == INVALID_HANDLE_VALUE || raw_handle.is_null() {
        return Err(NativeCreateFailure::EffectPossible);
    }
    Ok(())
}

#[cfg(windows)]
fn nt_create_directory_relative(
    parent: &fs::File,
    leaf_name: &[u16],
) -> Result<fs::File, NativeCreateFailure> {
    use std::os::windows::io::{AsRawHandle, FromRawHandle};
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_LIST_DIRECTORY, FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    const FILE_CREATE: u32 = 2;
    const FILE_DIRECTORY_FILE: u32 = 0x0000_0001;
    const FILE_SYNCHRONOUS_IO_NONALERT: u32 = 0x0000_0020;
    const FILE_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const OBJ_CASE_INSENSITIVE: u32 = 0x0000_0040;
    const SYNCHRONIZE: u32 = 0x0010_0000;

    let byte_length = u16::try_from(
        leaf_name
            .len()
            .checked_mul(std::mem::size_of::<u16>())
            .ok_or(NativeCreateFailure::BeforeEffect(
                BrokerError::AmbiguousTarget,
            ))?,
    )
    .map_err(|_| NativeCreateFailure::BeforeEffect(BrokerError::AmbiguousTarget))?;
    let mut name = NtUnicodeString {
        Length: byte_length,
        MaximumLength: byte_length,
        Buffer: leaf_name.as_ptr().cast_mut(),
    };
    let attributes = NtObjectAttributes {
        Length: std::mem::size_of::<NtObjectAttributes>() as u32,
        RootDirectory: parent.as_raw_handle(),
        ObjectName: &mut name,
        Attributes: OBJ_CASE_INSENSITIVE,
        SecurityDescriptor: std::ptr::null_mut(),
        SecurityQualityOfService: std::ptr::null_mut(),
    };
    let mut io_status = NtIoStatusBlock {
        status_or_pointer: 0,
        information: 0,
    };
    let mut raw_handle = INVALID_HANDLE_VALUE;
    let status = unsafe {
        NtCreateFile(
            &mut raw_handle,
            FILE_LIST_DIRECTORY | FILE_READ_ATTRIBUTES | SYNCHRONIZE,
            &attributes,
            &mut io_status,
            std::ptr::null(),
            WINDOWS_FILE_ATTRIBUTE_DIRECTORY,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            FILE_CREATE,
            FILE_DIRECTORY_FILE | FILE_SYNCHRONOUS_IO_NONALERT | FILE_OPEN_REPARSE_POINT,
            std::ptr::null(),
            0,
        )
    };
    classify_nt_create_completion(status, raw_handle)?;
    Ok(unsafe { fs::File::from_raw_handle(raw_handle) })
}

#[cfg(windows)]
fn windows_file_information_from_handle(
    file: &fs::File,
) -> Result<windows_sys::Win32::Storage::FileSystem::BY_HANDLE_FILE_INFORMATION, BrokerError> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };

    let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) } == 0 {
        return Err(BrokerError::AmbiguousTarget);
    }
    Ok(information)
}

#[cfg(windows)]
fn same_windows_file(
    left: &windows_sys::Win32::Storage::FileSystem::BY_HANDLE_FILE_INFORMATION,
    right: &windows_sys::Win32::Storage::FileSystem::BY_HANDLE_FILE_INFORMATION,
) -> bool {
    left.dwVolumeSerialNumber == right.dwVolumeSerialNumber
        && left.nFileIndexHigh == right.nFileIndexHigh
        && left.nFileIndexLow == right.nFileIndexLow
}

#[cfg(windows)]
fn windows_identity_sha256(
    platform: ExecutionHostPlatform,
    canonical_path: &Path,
    information: &windows_sys::Win32::Storage::FileSystem::BY_HANDLE_FILE_INFORMATION,
) -> Result<[u8; 32], BrokerError> {
    let mut hasher = Sha256::new();
    hasher.update(path_comparison(platform, canonical_path)?.as_bytes());
    hasher.update([0]);
    hasher.update(information.dwVolumeSerialNumber.to_le_bytes());
    hasher.update(information.nFileIndexHigh.to_le_bytes());
    hasher.update(information.nFileIndexLow.to_le_bytes());
    Ok(hasher.finalize().into())
}

pub fn object_identity_sha256(
    platform: ExecutionHostPlatform,
    path: &Path,
) -> Result<[u8; 32], BrokerError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| BrokerError::AmbiguousTarget)?;
    if metadata.file_type().is_symlink() || metadata.is_file() && link_count(path, &metadata) != 1 {
        return Err(BrokerError::UnsafeLink);
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| BrokerError::AmbiguousTarget)?;
    let mut hasher = Sha256::new();
    hasher.update(path_comparison(platform, &canonical)?.as_bytes());
    hasher.update([0]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        hasher.update(metadata.dev().to_le_bytes());
        hasher.update(metadata.ino().to_le_bytes());
    }
    #[cfg(windows)]
    {
        let information = windows_file_information(path)?;
        hasher.update(information.dwVolumeSerialNumber.to_le_bytes());
        hasher.update(information.nFileIndexHigh.to_le_bytes());
        hasher.update(information.nFileIndexLow.to_le_bytes());
    }
    Ok(hasher.finalize().into())
}

fn canonical_ordinary_existing_path(path: &Path) -> Result<PathBuf, BrokerError> {
    if !path.is_absolute() {
        return Err(BrokerError::AmbiguousTarget);
    }
    let mut current = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir | Component::Prefix(_) | Component::Normal(_) => {
                current.push(component)
            }
            _ => return Err(BrokerError::AmbiguousTarget),
        }
        let metadata = fs::symlink_metadata(&current).map_err(|_| BrokerError::AmbiguousTarget)?;
        if metadata.file_type().is_symlink() || is_windows_reparse_point(&current) {
            return Err(BrokerError::UnsafeLink);
        }
    }
    let canonical = path
        .canonicalize()
        .map_err(|_| BrokerError::AmbiguousTarget)?;
    if !same_canonical_path(&canonical, path) {
        return Err(BrokerError::AmbiguousTarget);
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

fn path_comparison(platform: ExecutionHostPlatform, path: &Path) -> Result<String, BrokerError> {
    let value = path.to_str().ok_or(BrokerError::AmbiguousTarget)?;
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
    path_comparison(ExecutionHostPlatform::Windows, left).ok()
        == path_comparison(ExecutionHostPlatform::Windows, right).ok()
}

#[cfg(not(windows))]
fn same_canonical_path(left: &Path, right: &Path) -> bool {
    left == right
}

fn separator(platform: ExecutionHostPlatform) -> char {
    match platform {
        ExecutionHostPlatform::Windows => '\\',
        ExecutionHostPlatform::Macos => '/',
    }
}

fn reject_case_alias(
    parent: &Path,
    target: &Path,
    platform: ExecutionHostPlatform,
) -> Result<(), BrokerError> {
    let Some(name) = target.file_name().and_then(|name| name.to_str()) else {
        return Err(BrokerError::AmbiguousTarget);
    };
    for entry in fs::read_dir(parent).map_err(|_| BrokerError::AmbiguousTarget)? {
        let entry = entry.map_err(|_| BrokerError::AmbiguousTarget)?;
        let entry_name = entry
            .file_name()
            .into_string()
            .map_err(|_| BrokerError::AmbiguousTarget)?;
        let aliases = match platform {
            ExecutionHostPlatform::Windows => entry_name.eq_ignore_ascii_case(name),
            ExecutionHostPlatform::Macos => entry_name == name,
        };
        if aliases && entry_name != name {
            return Err(BrokerError::AmbiguousTarget);
        }
    }
    Ok(())
}

#[cfg(unix)]
fn link_count(_path: &Path, metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.nlink()
}

#[cfg(windows)]
fn link_count(path: &Path, _metadata: &fs::Metadata) -> u64 {
    windows_file_information(path)
        .map(|information| information.nNumberOfLinks as u64)
        .unwrap_or(0)
}

#[cfg(not(any(unix, windows)))]
fn link_count(_path: &Path, _metadata: &fs::Metadata) -> u64 {
    0
}

#[cfg(windows)]
fn windows_file_information(
    path: &Path,
) -> Result<windows_sys::Win32::Storage::FileSystem::BY_HANDLE_FILE_INFORMATION, BrokerError> {
    use std::fs::OpenOptions;
    use std::mem::zeroed;
    use std::os::windows::fs::OpenOptionsExt;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS,
    };

    let file = OpenOptions::new()
        .access_mode(windows_sys::Win32::Storage::FileSystem::FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .map_err(|_| BrokerError::AmbiguousTarget)?;
    let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) } == 0 {
        return Err(BrokerError::AmbiguousTarget);
    }
    Ok(information)
}

#[cfg(all(test, windows))]
mod windows_post_create_tests {
    use super::*;
    use assemblywright_protocol::{
        ExecutionEffectClassification, EXECUTION_ACTION_ENVELOPE_SCHEMA_VERSION,
        FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
    };
    use ed25519_dalek::SigningKey;

    fn protocol_path(path: &Path) -> PathBuf {
        let canonical = path.canonicalize().unwrap();
        let value = canonical.to_str().unwrap();
        PathBuf::from(value.strip_prefix(r"\\?\").unwrap_or(value))
    }

    #[test]
    fn leaf_name_validation_covers_empty_reserved_and_ambiguous_boundaries() {
        for rejected in [
            "",
            ".",
            "..",
            "name.",
            "name ",
            "CON",
            "con.txt",
            "PRN",
            "AUX",
            "NUL",
            "COM1",
            "com9.log",
            "LPT1",
            "lpt9.log",
            "bad:name",
            "bad/name",
            "bad\\name",
            "bad\0name",
            "bad\u{1f}name",
        ] {
            assert_eq!(
                validate_windows_leaf_name(std::ffi::OsStr::new(rejected)),
                Err(BrokerError::AmbiguousTarget),
                "accepted ambiguous Windows leaf {rejected:?}"
            );
        }
        for accepted in ["created", "café", "COM10", "LPT10.txt", "name..inside"] {
            assert_eq!(
                validate_windows_leaf_name(std::ffi::OsStr::new(accepted)),
                Ok(()),
                "rejected ordinary Windows leaf {accepted:?}"
            );
        }
    }

    #[test]
    fn create_preparation_rejects_a_leaf_larger_than_the_native_length_field() {
        let target_path = PathBuf::from(r"C:\\").join("a".repeat(32_768));
        let target = ExecutionTargetIdentity {
            platform: ExecutionHostPlatform::Windows,
            canonical_path: target_path.to_str().unwrap().into(),
            canonical_path_sha256: [1; 32],
            canonical_parent_sha256: [2; 32],
            expected_object_sha256: None,
            expected_single_link: true,
        };

        assert!(matches!(
            WindowsCreateDirectoryEffect::prepare(&target),
            Err(BrokerError::AmbiguousTarget)
        ));
    }

    #[test]
    fn every_uncertain_native_create_completion_requires_reconciliation() {
        use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;

        assert!(matches!(
            classify_nt_create_completion(-1, INVALID_HANDLE_VALUE),
            Err(NativeCreateFailure::EffectPossible)
        ));
        assert!(matches!(
            classify_nt_create_completion(0, INVALID_HANDLE_VALUE),
            Err(NativeCreateFailure::EffectPossible)
        ));
        assert!(matches!(
            classify_nt_create_completion(0, std::ptr::null_mut()),
            Err(NativeCreateFailure::EffectPossible)
        ));
        assert!(classify_nt_create_completion(0, 1usize as _).is_ok());
    }

    #[test]
    fn post_state_digest_is_deterministic_and_binds_every_identity() {
        let action_id = Uuid::from_u128(10);
        let target = [11; 32];
        let identity = [12; 32];
        let digest = post_state_sha256(action_id, target, identity);

        assert_eq!(digest, post_state_sha256(action_id, target, identity));
        assert_ne!(
            digest,
            post_state_sha256(Uuid::from_u128(13), target, identity)
        );
        assert_ne!(digest, post_state_sha256(action_id, [14; 32], identity));
        assert_ne!(digest, post_state_sha256(action_id, target, [15; 32]));
        assert_ne!(digest, [0; 32]);
    }

    #[test]
    fn reconciliation_without_observation_is_path_free_and_not_applied() {
        let key = SigningKey::from_bytes(&[40; 32]);
        let temp = tempfile::tempdir().unwrap();
        let root = protocol_path(temp.path());
        let target = root.join("unobserved");
        let operation = BrokerOperation::CreateDirectory {
            target: target.to_str().unwrap().into(),
        };
        let protected = root.join("protected");
        fs::create_dir(&protected).unwrap();
        let protected_manifest = manifest(&protected);
        let identity = BrokerIdentity {
            platform: ExecutionHostPlatform::Windows,
            broker_id: Uuid::from_u128(1),
            broker_revision: 1,
            broker_executable_sha256: [1; 32],
            executor_id: Uuid::from_u128(2),
            executor_revision: 1,
            executor_executable_sha256: [2; 32],
            protected_control_plane_sha256: protected_manifest.canonical_sha256().unwrap(),
            signer_key_id: "master-action-v1".into(),
            verifying_key: key.verifying_key(),
            bound_child_epoch_id: Uuid::from_u128(3),
            bound_session_id: Uuid::from_u128(4),
            bound_session_revision: 1,
            bound_child_epoch_revision: 1,
            bound_feature_lifecycle_revision: 1,
            bound_authority_revision: 1,
            next_action_sequence: 1,
        };
        let envelope = action(&key, &identity, &operation, &target, 1);
        let outcome = reconciliation_required(&envelope, &envelope.targets[0], None);
        let encoded = serde_json::to_string(&outcome).unwrap();

        assert!(matches!(
            outcome,
            BrokerExecutionOutcome::EffectPossibleReconciliationRequired {
                observed_post_state_identity_sha256: None,
                observed_post_state_sha256: None,
                ..
            }
        ));
        assert!(!encoded.contains(target.to_str().unwrap()));
        assert!(!encoded.contains("\"outcome\":\"applied\""));
    }

    fn manifest(path: &Path) -> ProtectedControlPlanePathManifest {
        let path = path.to_str().unwrap().to_string();
        ProtectedControlPlanePathManifest {
            schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
            platform: ExecutionHostPlatform::Windows,
            master_binary: path.clone(),
            broker_binary: path.clone(),
            service_configuration: path.clone(),
            authority_database: path.clone(),
            database_backups: path.clone(),
            audit: path.clone(),
            owner_tokens_and_signing: path.clone(),
            trust_and_update_roots: path.clone(),
            ipc_and_enforcement_state: path.clone(),
            release_evidence: path.clone(),
            resource_reservations: path,
        }
    }

    fn action(
        key: &SigningKey,
        identity: &BrokerIdentity,
        operation: &BrokerOperation,
        target: &Path,
        sequence: u64,
    ) -> ExecutionActionEnvelope {
        let now = system_now_ms().unwrap();
        let target_text = target.to_str().unwrap();
        let mut envelope = ExecutionActionEnvelope {
            schema_version: EXECUTION_ACTION_ENVELOPE_SCHEMA_VERSION,
            action_id: Uuid::new_v4(),
            action_sequence: sequence,
            feature_id: Uuid::new_v4(),
            repository_id: Uuid::new_v4(),
            session_id: identity.bound_session_id,
            session_revision: identity.bound_session_revision,
            child_epoch_id: identity.bound_child_epoch_id,
            child_epoch_revision: identity.bound_child_epoch_revision,
            feature_lifecycle_revision: identity.bound_feature_lifecycle_revision,
            authority_revision: identity.bound_authority_revision,
            executor_id: identity.executor_id,
            executor_revision: identity.executor_revision,
            executor_executable_sha256: identity.executor_executable_sha256,
            broker_id: identity.broker_id,
            broker_revision: identity.broker_revision,
            broker_executable_sha256: identity.broker_executable_sha256,
            protected_control_plane_sha256: identity.protected_control_plane_sha256,
            host_platform: ExecutionHostPlatform::Windows,
            action_type: ExecutionActionType::CreateDirectory,
            targets: vec![ExecutionTargetIdentity {
                platform: ExecutionHostPlatform::Windows,
                canonical_path: target_text.into(),
                canonical_path_sha256: execution_path_sha256(
                    ExecutionHostPlatform::Windows,
                    target_text,
                )
                .unwrap(),
                canonical_parent_sha256: object_identity_sha256(
                    ExecutionHostPlatform::Windows,
                    target.parent().unwrap(),
                )
                .unwrap(),
                expected_object_sha256: None,
                expected_single_link: true,
            }],
            operation_sha256: operation.sha256().unwrap(),
            working_directory_sha256: [3; 32],
            environment_keys: Vec::new(),
            effect_classification: ExecutionEffectClassification::LocalDurable,
            deadline_ms: now + 60_000,
            cancellation_behavior: ExecutionCancellationBehavior::CheckpointThenTerminate,
            reconciliation_strategy: ExecutionReconciliationStrategy::ExactPostStateDigest,
            issued_at_ms: now,
            nonce: Uuid::new_v4(),
            signer_key_id: identity.signer_key_id.clone(),
            signature: Vec::new(),
        };
        envelope.sign(key).unwrap();
        envelope
    }

    #[test]
    fn post_create_verification_failure_is_typed_path_free_and_quarantines() {
        let temp = tempfile::tempdir().unwrap();
        let root = protocol_path(temp.path());
        let protected = root.join("protected");
        let allowed = root.join("allowed");
        fs::create_dir(&protected).unwrap();
        fs::create_dir(&allowed).unwrap();
        let key = SigningKey::from_bytes(&[41; 32]);
        let manifest = manifest(&protected);
        let identity = BrokerIdentity {
            platform: ExecutionHostPlatform::Windows,
            broker_id: Uuid::from_u128(1),
            broker_revision: 1,
            broker_executable_sha256: [1; 32],
            executor_id: Uuid::from_u128(2),
            executor_revision: 1,
            executor_executable_sha256: [2; 32],
            protected_control_plane_sha256: manifest.canonical_sha256().unwrap(),
            signer_key_id: "master-action-v1".into(),
            verifying_key: key.verifying_key(),
            bound_child_epoch_id: Uuid::from_u128(3),
            bound_session_id: Uuid::from_u128(4),
            bound_session_revision: 1,
            bound_child_epoch_revision: 1,
            bound_feature_lifecycle_revision: 1,
            bound_authority_revision: 1,
            next_action_sequence: 1,
        };
        let policy = BrokerPolicy::new(identity.clone(), manifest).unwrap();
        let target = allowed.join("created-but-unverified");
        let operation = BrokerOperation::CreateDirectory {
            target: target.to_str().unwrap().into(),
        };
        let envelope = action(&key, &identity, &operation, &target, 1);
        let admission = policy.admit(&envelope, &operation).unwrap();
        let PreparedEffect::CreateDirectory(effect) = admission.prepared_effect else {
            panic!("Windows create effect was not prepared");
        };
        let result = effect.execute_with_post_create_verifier(
            &policy,
            &envelope,
            &operation,
            |_, _, _, _| Err(BrokerError::AmbiguousTarget),
        );
        let outcome = finish_windows_proof(&policy, result).unwrap();
        let encoded = serde_json::to_string(&outcome).unwrap();
        let BrokerExecutionOutcome::EffectPossibleReconciliationRequired {
            action_id,
            signed_target_sha256,
            observed_post_state_identity_sha256,
            observed_post_state_sha256,
        } = outcome
        else {
            panic!("post-create failure was reported as applied");
        };
        assert_eq!(action_id, envelope.action_id);
        assert_eq!(
            signed_target_sha256,
            envelope.targets[0].canonical_path_sha256
        );
        assert!(observed_post_state_identity_sha256.is_some());
        assert!(observed_post_state_sha256.is_some());
        assert!(target.is_dir());
        assert!(!encoded.contains(target_text(&target)));

        let second_target = allowed.join("must-not-run");
        let second_operation = BrokerOperation::CreateDirectory {
            target: second_target.to_str().unwrap().into(),
        };
        let second = action(&key, &identity, &second_operation, &second_target, 2);
        assert_eq!(
            policy.admit(&second, &second_operation).err().unwrap(),
            BrokerError::StateUnavailable
        );
        assert!(!second_target.exists());
    }

    fn target_text(path: &Path) -> &str {
        path.to_str().unwrap()
    }
}
