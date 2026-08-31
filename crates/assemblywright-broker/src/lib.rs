use assemblywright_protocol::{
    execution_path_sha256, ExecutionActionEnvelope, ExecutionActionType, ExecutionHostPlatform,
    ExecutionTargetIdentity, ProtectedControlPlanePathManifest,
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
}

#[derive(serde::Serialize)]
struct CanonicalProtectedRoot {
    path: PathBuf,
    comparison: String,
}

pub struct BrokerAdmission<'a> {
    policy: &'a BrokerPolicy,
    envelope: &'a ExecutionActionEnvelope,
    operation: &'a BrokerOperation,
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
        self.validate_target(signed_target, envelope.action_type)?;

        let mut replay = self
            .replay
            .lock()
            .map_err(|_| BrokerError::StateUnavailable)?;
        let expected = *replay
            .next_sequence
            .entry(envelope.child_epoch_id)
            .or_insert(1);
        if envelope.action_sequence != expected
            || replay.seen_actions.contains(&envelope.action_id)
            || replay.seen_nonces.contains(&envelope.nonce)
        {
            return Err(BrokerError::Replay);
        }
        let next = expected.checked_add(1).ok_or(BrokerError::Replay)?;
        replay.next_sequence.insert(envelope.child_epoch_id, next);
        replay.seen_actions.insert(envelope.action_id);
        replay.seen_nonces.insert(envelope.nonce);
        Ok(BrokerAdmission {
            policy: self,
            envelope,
            operation,
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

impl BrokerAdmission<'_> {
    /// Privileged effects remain closed until each adapter can hold and
    /// revalidate parent/object handles across the effect boundary. A path-only
    /// admission is diagnostic evidence, never effect authority.
    pub fn execute(self) -> Result<(), BrokerError> {
        let _ = (self.policy, self.envelope, self.operation);
        Err(BrokerError::InvalidOperation)
    }
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
