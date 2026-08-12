use assemblywright_protocol::{
    validate_local_coding_patch_artifact_for_packet, FeatureConveyorCodingWorkPacketMetadata,
    LocalCodingEditOperation,
};
use git2::{Index, IndexEntry, ObjectType, Odb, Oid, Repository, Signature, Time};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;

use crate::{ResultArtifactReference, ResultArtifactStore, VerifiedResultArtifact};

const SNAPSHOT_ROOT: &str = "feature-conveyor-repository-snapshots";
const CANDIDATE_ROOT: &str = "feature-conveyor-candidates";

#[cfg(test)]
type SourceRevalidationHook = Option<Box<dyn FnOnce(&Path) + Send + 'static>>;

#[derive(Clone, Copy)]
enum CleanupHookPhase {
    BeforeCapture,
    AfterCapture,
    #[cfg(windows)]
    AfterInventory,
}

#[cfg(test)]
thread_local! {
    static CLEANUP_BEFORE_CAPTURE_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        std::cell::RefCell::new(None);
    static CLEANUP_AFTER_CAPTURE_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        std::cell::RefCell::new(None);
    #[cfg(windows)]
    static CLEANUP_AFTER_INVENTORY_HOOK: std::cell::RefCell<Option<Box<dyn FnOnce()>>> =
        std::cell::RefCell::new(None);
}

#[cfg(test)]
fn run_cleanup_test_hook(phase: CleanupHookPhase) {
    let hook = match phase {
        CleanupHookPhase::BeforeCapture => {
            CLEANUP_BEFORE_CAPTURE_HOOK.with(|hook| hook.borrow_mut().take())
        }
        CleanupHookPhase::AfterCapture => {
            CLEANUP_AFTER_CAPTURE_HOOK.with(|hook| hook.borrow_mut().take())
        }
        #[cfg(windows)]
        CleanupHookPhase::AfterInventory => {
            CLEANUP_AFTER_INVENTORY_HOOK.with(|hook| hook.borrow_mut().take())
        }
    };
    if let Some(hook) = hook {
        hook();
    }
}

#[cfg(not(test))]
fn run_cleanup_test_hook(_phase: CleanupHookPhase) {}

#[derive(Debug, thiserror::Error)]
pub enum ArtifactIntegrationError {
    #[error("artifact integration filesystem operation failed")]
    Io(#[from] std::io::Error),
    #[error("artifact integration Git object operation failed")]
    Git(#[from] git2::Error),
    #[error("artifact integration evidence was rejected")]
    Rejected,
    #[error("artifact integration path overlaps another accepted artifact")]
    OverlappingPath,
    #[error("artifact integration content compare-and-set failed")]
    ContentCasMismatch,
}

#[derive(Debug, Clone)]
pub struct IntegrationArtifact {
    pub reference: ResultArtifactReference,
    pub packet: FeatureConveyorCodingWorkPacketMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateEvidence {
    pub integration_id: Uuid,
    pub artifact_set_sha256: [u8; 32],
    pub candidate_commit: String,
    pub candidate_tree: String,
    pub base_commit: String,
    pub artifact_ids: Vec<Uuid>,
}

pub struct PreparedCandidate {
    pub evidence: CandidateEvidence,
    path: Option<PathBuf>,
    artifacts: Vec<VerifiedResultArtifact>,
    verified: VerifiedCandidate,
    cleanup_identity: Option<PlatformIdentity>,
}

pub struct VerifiedCandidate {
    evidence: CandidateEvidence,
    entries: Vec<StableTreeEntry>,
}

struct StableTreeEntry {
    relative: PathBuf,
    identity: PlatformIdentity,
    directory: bool,
    length: u64,
    sha256: [u8; 32],
    _handle: File,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlatformIdentity {
    #[cfg(unix)]
    Unix { device: u64, inode: u64 },
    #[cfg(windows)]
    Windows { volume: u32, high: u32, low: u32 },
    #[cfg(not(any(unix, windows)))]
    Unsupported,
}

impl PreparedCandidate {
    pub fn retain(mut self) {
        self.path = None;
    }

    pub fn revalidate_artifacts(
        &mut self,
        store: &ResultArtifactStore,
    ) -> Result<(), ArtifactIntegrationError> {
        for artifact in &mut self.artifacts {
            artifact
                .revalidate(store)
                .map_err(|_| ArtifactIntegrationError::Rejected)?;
        }
        Ok(())
    }

    pub fn revalidate_candidate(
        &mut self,
        store: &ArtifactIntegrationStore,
    ) -> Result<(), ArtifactIntegrationError> {
        self.verified.revalidate(store)
    }
}

impl VerifiedCandidate {
    pub fn evidence(&self) -> &CandidateEvidence {
        &self.evidence
    }

    pub fn revalidate(
        &mut self,
        store: &ArtifactIntegrationStore,
    ) -> Result<(), ArtifactIntegrationError> {
        let current = store.open_verified_candidate(&self.evidence)?;
        if !same_stable_tree(&self.entries, &current.entries) {
            return Err(ArtifactIntegrationError::Rejected);
        }
        Ok(())
    }
}

impl Drop for PreparedCandidate {
    fn drop(&mut self) {
        self.verified.entries.clear();
        self.artifacts.clear();
        if let Some(path) = self.path.take() {
            let _ = remove_plain_tree_matching(&path, self.cleanup_identity);
        }
    }
}

#[derive(Clone)]
pub struct ArtifactIntegrationStore {
    data_dir: PathBuf,
    root: PathBuf,
    #[cfg(test)]
    source_revalidation_hook: std::sync::Arc<std::sync::Mutex<SourceRevalidationHook>>,
    #[cfg(test)]
    source_capture_hook: std::sync::Arc<std::sync::Mutex<SourceRevalidationHook>>,
}

impl ArtifactIntegrationStore {
    pub fn open(data_dir: &Path) -> Result<Self, ArtifactIntegrationError> {
        let root = data_dir.join(CANDIDATE_ROOT);
        ensure_directory(&root)?;
        ensure_directory(&root.join("staging"))?;
        ensure_directory(&root.join("candidates"))?;
        Ok(Self {
            data_dir: data_dir.to_path_buf(),
            root,
            #[cfg(test)]
            source_revalidation_hook: std::sync::Arc::new(std::sync::Mutex::new(None)),
            #[cfg(test)]
            source_capture_hook: std::sync::Arc::new(std::sync::Mutex::new(None)),
        })
    }

    pub fn cleanup_unreferenced(
        &self,
        referenced: &HashSet<Uuid>,
    ) -> Result<(), ArtifactIntegrationError> {
        cleanup_children(&self.root.join("staging"), &HashSet::new())?;
        cleanup_children(&self.root.join("candidates"), referenced)
    }

    pub fn verify_referenced(
        &self,
        references: &[CandidateEvidence],
    ) -> Result<(), ArtifactIntegrationError> {
        for evidence in references {
            self.open_verified_candidate(evidence)?;
        }
        Ok(())
    }

    pub fn open_verified_candidate(
        &self,
        evidence: &CandidateEvidence,
    ) -> Result<VerifiedCandidate, ArtifactIntegrationError> {
        let path = self
            .root
            .join("candidates")
            .join(evidence.integration_id.to_string());
        let entries = open_stable_tree(&path)?;
        validate_stable_git_shape(&entries, GitRepositoryShape::Candidate)?;
        {
            validate_candidate_config_stable(&entries)?;
            let stable_odb = StableOdb::from_entries(&self.root.join("staging"), &entries)?;
            let odb = stable_odb.odb();
            let commit_oid = Oid::from_str(&evidence.candidate_commit)?;
            let commit = odb.read(commit_oid)?;
            let expected_commit =
                deterministic_commit_bytes(&evidence.candidate_tree, &evidence.base_commit);
            if stable_file_bytes(&entries, Path::new(".git/shallow"), 256)?
                != format!("{}\n", evidence.base_commit).as_bytes()
            {
                return Err(ArtifactIntegrationError::Rejected);
            }
            if stable_file_bytes(&entries, Path::new(".git/HEAD"), 256)?
                != format!("{}\n", evidence.candidate_commit).as_bytes()
                || commit.kind() != ObjectType::Commit
                || commit.data() != expected_commit
                || Oid::hash_object(ObjectType::Commit, commit.data())? != commit_oid
            {
                return Err(ArtifactIntegrationError::Rejected);
            }
            verify_materialized_stable(odb, &entries, Oid::from_str(&evidence.candidate_tree)?)?;
        }
        let current = open_stable_tree(&path)?;
        if !same_stable_tree(&entries, &current) {
            return Err(ArtifactIntegrationError::Rejected);
        }
        Ok(VerifiedCandidate {
            evidence: evidence.clone(),
            entries,
        })
    }

    pub fn prepare(
        &self,
        integration_id: Uuid,
        snapshot_id: Uuid,
        base_commit: &str,
        artifacts: &[IntegrationArtifact],
    ) -> Result<PreparedCandidate, ArtifactIntegrationError> {
        if integration_id.is_nil() || artifacts.is_empty() || artifacts.len() > 3 {
            return Err(ArtifactIntegrationError::Rejected);
        }
        let artifact_set_sha256 = artifact_set_sha256(artifacts);
        let final_path = self
            .root
            .join("candidates")
            .join(integration_id.to_string());
        if final_path.exists() {
            return Err(ArtifactIntegrationError::Rejected);
        }
        let staging_path = self.root.join("staging").join(Uuid::new_v4().to_string());
        fs::create_dir(&staging_path)?;
        ensure_directory(&staging_path)?;
        let mut cleanup = PreparedCandidate {
            evidence: CandidateEvidence {
                integration_id,
                artifact_set_sha256,
                candidate_commit: String::new(),
                candidate_tree: String::new(),
                base_commit: base_commit.to_string(),
                artifact_ids: artifacts
                    .iter()
                    .map(|artifact| artifact.reference.artifact_id)
                    .collect(),
            },
            path: Some(staging_path.clone()),
            artifacts: Vec::new(),
            verified: VerifiedCandidate {
                evidence: CandidateEvidence {
                    integration_id,
                    artifact_set_sha256,
                    candidate_commit: String::new(),
                    candidate_tree: String::new(),
                    base_commit: base_commit.to_string(),
                    artifact_ids: Vec::new(),
                },
                entries: Vec::new(),
            },
            cleanup_identity: Some(path_identity(&staging_path)?),
        };
        let source_path = self
            .data_dir
            .join(SNAPSHOT_ROOT)
            .join("snapshots")
            .join(snapshot_id.to_string());
        let source_handles = open_stable_tree(&source_path)?;
        #[cfg(test)]
        if let Some(hook) = self
            .source_capture_hook
            .lock()
            .map_err(|_| ArtifactIntegrationError::Rejected)?
            .take()
        {
            hook(&source_path);
        }
        validate_stable_git_shape(&source_handles, GitRepositoryShape::Snapshot)?;
        validate_snapshot_binding_stable(&source_handles, base_commit)?;
        let stable_source = StableOdb::from_entries(&self.root.join("staging"), &source_handles)?;
        #[cfg(all(test, windows))]
        eprintln!("candidate-stage stable-source");
        let source = stable_source.odb();
        let base_oid = Oid::from_str(base_commit)?;
        let base_object = source.read(base_oid)?;
        if base_object.kind() != ObjectType::Commit {
            return Err(ArtifactIntegrationError::Rejected);
        }
        let base_tree_oid = commit_tree_oid(base_object.data())?;
        let destination = Repository::init(&staging_path)?;
        #[cfg(all(test, windows))]
        eprintln!("candidate-stage initialized");
        {
            let mut config = git2::Config::open(&staging_path.join(".git").join("config"))?;
            config.set_bool("core.autocrlf", false)?;
            config.set_bool("core.symlinks", false)?;
            config.set_bool("core.logallrefupdates", false)?;
            config.set_str("core.hooksPath", "disabled-hooks")?;
        }
        drop(destination);
        normalize_candidate_git_metadata(&staging_path.join(".git"))?;
        #[cfg(all(test, windows))]
        eprintln!("candidate-stage normalized");
        let destination = Repository::open(&staging_path)?;
        fs::write(
            staging_path.join(".git").join("shallow"),
            format!("{base_commit}\n"),
        )?;
        copy_commit_tree(source, &destination, base_oid)?;
        #[cfg(all(test, windows))]
        eprintln!("candidate-stage copied");

        let mut index = destination.index()?;
        index.read_tree(&destination.find_tree(base_tree_oid)?)?;
        let artifact_store = ResultArtifactStore::open(&self.data_dir)
            .map_err(|_| ArtifactIntegrationError::Rejected)?;
        let mut seen_paths = HashSet::new();
        for artifact in artifacts {
            let mut verified = artifact_store
                .open_verified(artifact.reference)
                .map_err(|_| ArtifactIntegrationError::Rejected)?;
            let bytes = verified
                .read_revalidated(&artifact_store)
                .map_err(|_| ArtifactIntegrationError::Rejected)?;
            validate_local_coding_patch_artifact_for_packet(&bytes, &artifact.packet)
                .map_err(|_| ArtifactIntegrationError::Rejected)?;
            for operation in &artifact.packet.operations {
                if !reserve_integration_path(&mut seen_paths, operation.path()) {
                    return Err(ArtifactIntegrationError::OverlappingPath);
                }
                apply_operation(source, &destination, &mut index, operation)?;
            }
            cleanup.artifacts.push(verified);
        }
        #[cfg(all(test, windows))]
        eprintln!("candidate-stage applied");
        let tree_oid = index.write_tree_to(&destination)?;
        let tree = destination.find_tree(tree_oid)?;
        let parent = destination.find_commit(base_oid)?;
        let signature = Signature::new(
            "Assemblywright Integration",
            "integration@assemblywright.invalid",
            &Time::new(0, 0),
        )?;
        let commit_oid = destination.commit(
            None,
            &signature,
            &signature,
            "Assemblywright deterministic candidate\n",
            &tree,
            &[&parent],
        )?;
        destination.set_head_detached(commit_oid)?;
        index.write()?;
        materialize_tree(&destination, &staging_path, tree_oid)?;
        verify_materialized_tree(&destination, &staging_path, tree_oid)?;
        #[cfg(all(test, windows))]
        eprintln!("candidate-stage materialized");
        #[cfg(test)]
        if let Some(hook) = self
            .source_revalidation_hook
            .lock()
            .map_err(|_| ArtifactIntegrationError::Rejected)?
            .take()
        {
            hook(&source_path);
        }
        let current_source = open_stable_tree(&source_path)?;
        validate_stable_git_shape(&current_source, GitRepositoryShape::Snapshot)?;
        validate_snapshot_binding_stable(&current_source, base_commit)?;
        if !same_stable_tree(&source_handles, &current_source)
            || source.read(base_oid)?.data() != base_object.data()
        {
            return Err(ArtifactIntegrationError::Rejected);
        }
        drop(parent);
        drop(tree);
        drop(base_object);
        drop(destination);
        #[cfg(all(test, windows))]
        eprintln!("candidate-stage dropped");
        secure_tree_permissions(&staging_path)?;
        secure_materialized_permissions(
            &Repository::open(&staging_path)?,
            &staging_path,
            tree_oid,
        )?;
        sync_plain_tree(&staging_path)?;
        #[cfg(all(test, windows))]
        eprintln!("candidate-stage synced");
        fs::rename(&staging_path, &final_path)?;
        #[cfg(all(test, windows))]
        eprintln!("candidate-stage frozen");
        sync_directory_portable(
            final_path
                .parent()
                .ok_or(ArtifactIntegrationError::Rejected)?,
        )?;
        cleanup.path = Some(final_path);
        cleanup.evidence.candidate_commit = commit_oid.to_string();
        cleanup.evidence.candidate_tree = tree_oid.to_string();
        cleanup.verified = self.open_verified_candidate(&cleanup.evidence)?;
        #[cfg(all(test, windows))]
        eprintln!("candidate-stage verified");
        Ok(cleanup)
    }
}

fn reserve_integration_path(paths: &mut HashSet<String>, path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    if paths.iter().any(|existing| {
        existing == &normalized
            || existing
                .strip_prefix(&normalized)
                .is_some_and(|suffix| suffix.starts_with('/'))
            || normalized
                .strip_prefix(existing)
                .is_some_and(|suffix| suffix.starts_with('/'))
    }) {
        return false;
    }
    paths.insert(normalized)
}

#[derive(Clone, Copy)]
enum GitRepositoryShape {
    Candidate,
    Snapshot,
}

fn normalize_candidate_git_metadata(git: &Path) -> Result<(), ArtifactIntegrationError> {
    let hooks = git.join("hooks");
    if hooks.exists() {
        remove_plain_tree(&hooks)?;
    }
    let info = git.join("info");
    if info.exists() {
        remove_plain_tree(&info)?;
    }
    let description = git.join("description");
    if description.exists() {
        remove_plain_file(&description)?;
    }
    ensure_directory(&git.join("disabled-hooks"))?;
    Ok(())
}

fn same_stable_tree(left: &[StableTreeEntry], right: &[StableTreeEntry]) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.relative == right.relative
                && left.identity == right.identity
                && left.directory == right.directory
                && left.length == right.length
                && left.sha256 == right.sha256
        })
}

fn stable_entry<'a>(
    entries: &'a [StableTreeEntry],
    relative: &Path,
) -> Result<&'a StableTreeEntry, ArtifactIntegrationError> {
    entries
        .iter()
        .find(|entry| entry.relative == relative)
        .ok_or(ArtifactIntegrationError::Rejected)
}

fn stable_file_bytes(
    entries: &[StableTreeEntry],
    relative: &Path,
    maximum: usize,
) -> Result<Vec<u8>, ArtifactIntegrationError> {
    let entry = stable_entry(entries, relative)?;
    if entry.directory || entry.length > maximum as u64 {
        return Err(ArtifactIntegrationError::Rejected);
    }
    let mut file = entry._handle.try_clone()?;
    use std::io::{Seek, SeekFrom};
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > maximum || <[u8; 32]>::from(Sha256::digest(&bytes)) != entry.sha256 {
        return Err(ArtifactIntegrationError::Rejected);
    }
    Ok(bytes)
}

fn validate_stable_git_shape(
    entries: &[StableTreeEntry],
    shape: GitRepositoryShape,
) -> Result<(), ArtifactIntegrationError> {
    for directory in [
        Path::new(""),
        Path::new(".git"),
        Path::new(".git/objects"),
        Path::new(".git/refs"),
    ] {
        if !stable_entry(entries, directory)?.directory {
            return Err(ArtifactIntegrationError::Rejected);
        }
    }
    let required_files: &[&str] = match shape {
        GitRepositoryShape::Candidate => {
            &[".git/HEAD", ".git/config", ".git/index", ".git/shallow"]
        }
        GitRepositoryShape::Snapshot => &[
            ".git/HEAD",
            ".git/config",
            ".git/index",
            ".git/shallow",
            ".git/assemblywright-snapshot-v1.bundle",
            ".git/refs/heads/snapshot",
        ],
    };
    for required in required_files {
        if stable_entry(entries, Path::new(required))?.directory {
            return Err(ArtifactIntegrationError::Rejected);
        }
    }
    for forbidden in [
        ".git/objects/info/alternates",
        ".git/objects/info/http-alternates",
        ".git/commondir",
        ".git/gitdir",
    ] {
        if entries
            .iter()
            .any(|entry| entry.relative == Path::new(forbidden))
        {
            return Err(ArtifactIntegrationError::Rejected);
        }
    }
    let config = String::from_utf8(stable_file_bytes(
        entries,
        Path::new(".git/config"),
        8 * 1024,
    )?)
    .map_err(|_| ArtifactIntegrationError::Rejected)?
    .to_ascii_lowercase();
    if config.contains("include")
        || config.contains("alternates")
        || config.contains("objectdirectory")
        || config.contains("worktreeconfig")
    {
        return Err(ArtifactIntegrationError::Rejected);
    }
    if matches!(shape, GitRepositoryShape::Candidate) {
        for entry in entries
            .iter()
            .filter(|entry| entry.relative.starts_with(".git"))
        {
            let relative = entry.relative.to_string_lossy().replace('\\', "/");
            let allowed = matches!(
                relative.as_str(),
                ".git"
                    | ".git/HEAD"
                    | ".git/config"
                    | ".git/index"
                    | ".git/shallow"
                    | ".git/objects"
                    | ".git/objects/info"
                    | ".git/objects/pack"
                    | ".git/refs"
                    | ".git/refs/heads"
                    | ".git/refs/tags"
                    | ".git/disabled-hooks"
            ) || is_loose_object_path(&relative);
            if !allowed {
                return Err(ArtifactIntegrationError::Rejected);
            }
        }
    } else {
        const HOOK_SAMPLES: &[&str] = &[
            "README.sample",
            "applypatch-msg.sample",
            "commit-msg.sample",
            "fsmonitor-watchman.sample",
            "post-update.sample",
            "pre-applypatch.sample",
            "pre-commit.sample",
            "pre-merge-commit.sample",
            "pre-push.sample",
            "pre-rebase.sample",
            "pre-receive.sample",
            "prepare-commit-msg.sample",
            "push-to-checkout.sample",
            "sendemail-validate.sample",
            "update.sample",
        ];
        for entry in entries
            .iter()
            .filter(|entry| entry.relative.starts_with(".git"))
        {
            let relative = entry.relative.to_string_lossy().replace('\\', "/");
            let hook = relative
                .strip_prefix(".git/hooks/")
                .is_some_and(|name| HOOK_SAMPLES.contains(&name));
            let allowed = matches!(
                relative.as_str(),
                ".git"
                    | ".git/HEAD"
                    | ".git/config"
                    | ".git/description"
                    | ".git/index"
                    | ".git/shallow"
                    | ".git/assemblywright-snapshot-v1.bundle"
                    | ".git/hooks"
                    | ".git/hooks-disabled"
                    | ".git/info"
                    | ".git/info/exclude"
                    | ".git/objects"
                    | ".git/objects/info"
                    | ".git/objects/pack"
                    | ".git/refs"
                    | ".git/refs/heads"
                    | ".git/refs/heads/snapshot"
                    | ".git/refs/tags"
            ) || hook
                || is_loose_object_path(&relative);
            if !allowed {
                return Err(ArtifactIntegrationError::Rejected);
            }
        }
    }
    Ok(())
}

fn is_loose_object_path(path: &str) -> bool {
    let Some(rest) = path.strip_prefix(".git/objects/") else {
        return false;
    };
    let mut parts = rest.split('/');
    let first = parts.next().unwrap_or_default();
    let second = parts.next();
    match second {
        None => first.len() == 2 && first.bytes().all(|byte| byte.is_ascii_hexdigit()),
        Some(second) => {
            parts.next().is_none()
                && first.len() == 2
                && first.bytes().all(|byte| byte.is_ascii_hexdigit())
                && second.len() == 38
                && second.bytes().all(|byte| byte.is_ascii_hexdigit())
        }
    }
}

fn validate_snapshot_binding_stable(
    entries: &[StableTreeEntry],
    base_commit: &str,
) -> Result<(), ArtifactIntegrationError> {
    if stable_file_bytes(entries, Path::new(".git/HEAD"), 256)? != b"ref: refs/heads/snapshot\n"
        || stable_file_bytes(entries, Path::new(".git/shallow"), 256)?
            != format!("{base_commit}\n").as_bytes()
        || stable_file_bytes(entries, Path::new(".git/refs/heads/snapshot"), 256)?
            != format!("{base_commit}\n").as_bytes()
    {
        return Err(ArtifactIntegrationError::Rejected);
    }
    Ok(())
}

struct StableOdb {
    odb: Option<Odb<'static>>,
    path: PathBuf,
    identity: PlatformIdentity,
}

impl StableOdb {
    fn from_entries(
        staging: &Path,
        entries: &[StableTreeEntry],
    ) -> Result<Self, ArtifactIntegrationError> {
        let path = staging.join(Uuid::new_v4().to_string());
        ensure_directory(&path)?;
        let identity = path_identity(&path)?;
        let objects = path.join("objects");
        ensure_directory(&objects)?;
        for entry in entries.iter().filter(|entry| !entry.directory) {
            let relative = entry.relative.to_string_lossy().replace('\\', "/");
            let Some(object) = relative.strip_prefix(".git/objects/") else {
                continue;
            };
            let mut parts = object.split('/');
            let prefix = parts.next().unwrap_or_default();
            let suffix = parts.next().unwrap_or_default();
            if parts.next().is_some()
                || prefix.len() != 2
                || suffix.len() != 38
                || !prefix.bytes().all(|byte| byte.is_ascii_hexdigit())
                || !suffix.bytes().all(|byte| byte.is_ascii_hexdigit())
            {
                return Err(ArtifactIntegrationError::Rejected);
            }
            let directory = objects.join(prefix);
            if !directory.exists() {
                ensure_directory(&directory)?;
            }
            let destination = directory.join(suffix);
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options
                    .mode(0o600)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                options.custom_flags(
                    windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT,
                );
            }
            let mut output = options.open(destination)?;
            let mut input = entry._handle.try_clone()?;
            use std::io::{Seek, SeekFrom};
            input.seek(SeekFrom::Start(0))?;
            std::io::copy(&mut input, &mut output)?;
            output.sync_all()?;
        }
        sync_plain_tree(&path)?;
        let odb = Odb::new()?;
        odb.add_disk_alternate(objects.to_str().ok_or(ArtifactIntegrationError::Rejected)?)?;
        Ok(Self {
            odb: Some(odb),
            path,
            identity,
        })
    }

    fn odb(&self) -> &Odb<'static> {
        self.odb.as_ref().expect("stable ODB remains live")
    }
}

impl Drop for StableOdb {
    fn drop(&mut self) {
        drop(self.odb.take());
        let _ = remove_plain_tree_matching(&self.path, Some(self.identity));
    }
}

fn verify_materialized_stable(
    odb: &Odb<'_>,
    entries: &[StableTreeEntry],
    tree_oid: Oid,
) -> Result<(), ArtifactIntegrationError> {
    let mut expected = HashSet::new();
    collect_raw_tree_paths(odb, Path::new(""), tree_oid, &mut expected)?;
    let actual: HashSet<PathBuf> = entries
        .iter()
        .filter(|entry| {
            !entry.relative.as_os_str().is_empty()
                && entry.relative != Path::new(".git")
                && !entry.relative.starts_with(".git")
        })
        .map(|entry| entry.relative.clone())
        .collect();
    if expected != actual {
        return Err(ArtifactIntegrationError::Rejected);
    }
    for path in expected {
        let tree_entry = raw_tree_entry_at(odb, tree_oid, &path)?;
        let stable = stable_entry(entries, &path)?;
        if tree_entry.mode == 0o040000 {
            if !stable.directory {
                return Err(ArtifactIntegrationError::Rejected);
            }
            continue;
        }
        if stable.directory || !matches!(tree_entry.mode, 0o100644 | 0o100755) {
            return Err(ArtifactIntegrationError::Rejected);
        }
        let bytes = stable_file_bytes(entries, &path, 32 * 1024 * 1024)?;
        if Oid::hash_object(ObjectType::Blob, &bytes)? != tree_entry.oid {
            return Err(ArtifactIntegrationError::Rejected);
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct RawTreeEntry {
    mode: u32,
    name: Vec<u8>,
    oid: Oid,
}

fn parse_raw_tree(mut data: &[u8]) -> Result<Vec<RawTreeEntry>, ArtifactIntegrationError> {
    let mut entries = Vec::new();
    while !data.is_empty() {
        let space = data
            .iter()
            .position(|byte| *byte == b' ')
            .ok_or(ArtifactIntegrationError::Rejected)?;
        let mode = u32::from_str_radix(
            std::str::from_utf8(&data[..space]).map_err(|_| ArtifactIntegrationError::Rejected)?,
            8,
        )
        .map_err(|_| ArtifactIntegrationError::Rejected)?;
        data = &data[space + 1..];
        let nul = data
            .iter()
            .position(|byte| *byte == 0)
            .ok_or(ArtifactIntegrationError::Rejected)?;
        let name = data[..nul].to_vec();
        if name.is_empty()
            || name == b"."
            || name == b".."
            || name.contains(&b'/')
            || name.contains(&b'\\')
        {
            return Err(ArtifactIntegrationError::Rejected);
        }
        data = &data[nul + 1..];
        if data.len() < 20 {
            return Err(ArtifactIntegrationError::Rejected);
        }
        let oid = Oid::from_bytes(&data[..20])?;
        data = &data[20..];
        entries.push(RawTreeEntry { mode, name, oid });
    }
    Ok(entries)
}

fn commit_tree_oid(data: &[u8]) -> Result<Oid, ArtifactIntegrationError> {
    let first = data
        .split(|byte| *byte == b'\n')
        .next()
        .ok_or(ArtifactIntegrationError::Rejected)?;
    let value = first
        .strip_prefix(b"tree ")
        .ok_or(ArtifactIntegrationError::Rejected)?;
    Ok(Oid::from_str(
        std::str::from_utf8(value).map_err(|_| ArtifactIntegrationError::Rejected)?,
    )?)
}

fn collect_raw_tree_paths(
    odb: &Odb<'_>,
    relative: &Path,
    oid: Oid,
    paths: &mut HashSet<PathBuf>,
) -> Result<(), ArtifactIntegrationError> {
    let object = odb.read(oid)?;
    if object.kind() != ObjectType::Tree {
        return Err(ArtifactIntegrationError::Rejected);
    }
    for entry in parse_raw_tree(object.data())? {
        let name =
            std::str::from_utf8(&entry.name).map_err(|_| ArtifactIntegrationError::Rejected)?;
        let child = relative.join(name);
        paths.insert(child.clone());
        if entry.mode == 0o040000 {
            collect_raw_tree_paths(odb, &child, entry.oid, paths)?;
        } else if !matches!(entry.mode, 0o100644 | 0o100755) {
            return Err(ArtifactIntegrationError::Rejected);
        }
    }
    Ok(())
}

fn raw_tree_entry_at(
    odb: &Odb<'_>,
    mut tree_oid: Oid,
    path: &Path,
) -> Result<RawTreeEntry, ArtifactIntegrationError> {
    let components: Vec<_> = path.components().collect();
    for (position, component) in components.iter().enumerate() {
        let Component::Normal(name) = component else {
            return Err(ArtifactIntegrationError::Rejected);
        };
        let name = name.to_str().ok_or(ArtifactIntegrationError::Rejected)?;
        let object = odb.read(tree_oid)?;
        if object.kind() != ObjectType::Tree {
            return Err(ArtifactIntegrationError::Rejected);
        }
        let entry = parse_raw_tree(object.data())?
            .into_iter()
            .find(|entry| entry.name == name.as_bytes())
            .ok_or(ArtifactIntegrationError::Rejected)?;
        if position + 1 == components.len() {
            return Ok(entry);
        }
        if entry.mode != 0o040000 {
            return Err(ArtifactIntegrationError::Rejected);
        }
        tree_oid = entry.oid;
    }
    Err(ArtifactIntegrationError::Rejected)
}

fn deterministic_commit_bytes(tree: &str, parent: &str) -> Vec<u8> {
    format!(
        "tree {tree}\nparent {parent}\nauthor Assemblywright Integration <integration@assemblywright.invalid> 0 +0000\ncommitter Assemblywright Integration <integration@assemblywright.invalid> 0 +0000\n\nAssemblywright deterministic candidate\n"
    )
    .into_bytes()
}

fn validate_candidate_config_stable(
    entries: &[StableTreeEntry],
) -> Result<(), ArtifactIntegrationError> {
    let config = String::from_utf8(stable_file_bytes(
        entries,
        Path::new(".git/config"),
        8 * 1024,
    )?)
    .map_err(|_| ArtifactIntegrationError::Rejected)?;
    let mut core = false;
    let mut seen = HashSet::new();
    for raw in config.lines() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            core = line[1..line.len() - 1].eq_ignore_ascii_case("core");
            if !core {
                return Err(ArtifactIntegrationError::Rejected);
            }
            continue;
        }
        let (key, value) = line
            .split_once('=')
            .ok_or(ArtifactIntegrationError::Rejected)?;
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        if !core || !seen.insert(key.clone()) {
            return Err(ArtifactIntegrationError::Rejected);
        }
        let allowed = match key.as_str() {
            "repositoryformatversion" => value == "0",
            "filemode" | "ignorecase" | "precomposeunicode" => matches!(value, "true" | "false"),
            "bare" | "logallrefupdates" | "autocrlf" | "symlinks" => value == "false",
            "hookspath" => value == "disabled-hooks",
            _ => false,
        };
        if !allowed {
            return Err(ArtifactIntegrationError::Rejected);
        }
    }
    if !["bare", "autocrlf", "symlinks", "hookspath"]
        .iter()
        .all(|key| seen.contains(*key))
    {
        return Err(ArtifactIntegrationError::Rejected);
    }
    Ok(())
}

#[cfg(unix)]
fn open_stable_tree(root: &Path) -> Result<Vec<StableTreeEntry>, ArtifactIntegrationError> {
    let mut options = OpenOptions::new();
    options.read(true);
    configure_no_follow_directory(&mut options);
    let handle = options.open(root)?;
    validate_open_plain_directory(&handle)?;
    let mut entries = Vec::new();
    open_stable_tree_at_unix(Path::new(""), handle, &mut entries)?;
    entries.sort_by(|left, right| left.relative.cmp(&right.relative));
    Ok(entries)
}

#[cfg(unix)]
fn open_stable_tree_at_unix(
    relative: &Path,
    handle: File,
    entries: &mut Vec<StableTreeEntry>,
) -> Result<(), ArtifactIntegrationError> {
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStringExt;
    let identity = platform_identity(&handle)?;
    for name in directory_entry_names(&handle)? {
        let child_name = std::ffi::OsString::from_vec(name.to_bytes().to_vec());
        let child_relative = relative.join(child_name);
        let directory_descriptor = unsafe {
            libc::openat(
                handle.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if directory_descriptor >= 0 {
            let child = unsafe { File::from_raw_fd(directory_descriptor) };
            validate_open_plain_directory(&child)?;
            open_stable_tree_at_unix(&child_relative, child, entries)?;
            continue;
        }
        let file_descriptor = unsafe {
            libc::openat(
                handle.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if file_descriptor < 0 {
            return Err(ArtifactIntegrationError::Rejected);
        }
        let mut file = unsafe { File::from_raw_fd(file_descriptor) };
        validate_open_plain_file(&file)?;
        let metadata = file.metadata()?;
        let mut digest = Sha256::new();
        std::io::copy(&mut file, &mut digest)?;
        entries.push(StableTreeEntry {
            relative: child_relative,
            identity: platform_identity(&file)?,
            directory: false,
            length: metadata.len(),
            sha256: digest.finalize().into(),
            _handle: file,
        });
    }
    entries.push(StableTreeEntry {
        relative: relative.to_path_buf(),
        identity,
        directory: true,
        length: 0,
        sha256: [0; 32],
        _handle: handle,
    });
    Ok(())
}

#[cfg(not(unix))]
fn open_stable_tree(root: &Path) -> Result<Vec<StableTreeEntry>, ArtifactIntegrationError> {
    let mut entries = Vec::new();
    open_stable_tree_at_path(root, root, &mut entries)?;
    entries.sort_by(|left, right| left.relative.cmp(&right.relative));
    Ok(entries)
}

#[cfg(not(unix))]
fn open_stable_tree_at_path(
    root: &Path,
    path: &Path,
    entries: &mut Vec<StableTreeEntry>,
) -> Result<(), ArtifactIntegrationError> {
    let mut options = OpenOptions::new();
    options.read(true);
    configure_no_follow_directory(&mut options);
    let handle = options.open(path)?;
    validate_open_plain_directory(&handle)?;
    entries.push(StableTreeEntry {
        relative: path
            .strip_prefix(root)
            .map_err(|_| ArtifactIntegrationError::Rejected)?
            .to_path_buf(),
        identity: platform_identity(&handle)?,
        directory: true,
        length: 0,
        sha256: [0; 32],
        _handle: handle,
    });
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            open_stable_tree_at_path(root, &entry.path(), entries)?;
        } else {
            let mut options = OpenOptions::new();
            options.read(true);
            configure_no_follow_read(&mut options);
            let mut handle = options.open(entry.path())?;
            validate_open_plain_file(&handle)?;
            let metadata = handle.metadata()?;
            let mut digest = Sha256::new();
            std::io::copy(&mut handle, &mut digest)?;
            entries.push(StableTreeEntry {
                relative: entry
                    .path()
                    .strip_prefix(root)
                    .map_err(|_| ArtifactIntegrationError::Rejected)?
                    .to_path_buf(),
                identity: platform_identity(&handle)?,
                directory: false,
                length: metadata.len(),
                sha256: digest.finalize().into(),
                _handle: handle,
            });
        }
    }
    Ok(())
}

fn artifact_set_sha256(artifacts: &[IntegrationArtifact]) -> [u8; 32] {
    let references: Vec<_> = artifacts
        .iter()
        .map(|artifact| artifact.reference)
        .collect();
    artifact_reference_set_sha256(&references)
}

pub(crate) fn artifact_reference_set_sha256(references: &[ResultArtifactReference]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"assemblywright.artifact-set.v1\0");
    digest.update((references.len() as u64).to_be_bytes());
    let mut entries = references.to_vec();
    entries.sort_by_key(|a| a.artifact_id);
    for reference in entries {
        digest.update(reference.artifact_id.as_bytes());
        digest.update(reference.artifact_sha256);
        digest.update(reference.artifact_size_bytes.to_be_bytes());
    }
    digest.finalize().into()
}

fn apply_operation(
    source: &Odb<'_>,
    destination: &Repository,
    index: &mut Index,
    operation: &LocalCodingEditOperation,
) -> Result<(), ArtifactIntegrationError> {
    let path = Path::new(operation.path());
    if path
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(ArtifactIntegrationError::Rejected);
    }
    let current = index.get_path(path, 0);
    let current_bytes = match current {
        Some(entry) if entry.mode == 0o100644 || entry.mode == 0o100755 => Some(
            source
                .read(entry.id)
                .map(|object| object.data().to_vec())
                .or_else(|_| {
                    destination
                        .find_blob(entry.id)
                        .map(|blob| blob.content().to_vec())
                })?,
        ),
        Some(_) => return Err(ArtifactIntegrationError::ContentCasMismatch),
        None => None,
    };
    match operation {
        LocalCodingEditOperation::Write(arguments) => {
            match (arguments.expected_before_sha256, current_bytes.as_deref()) {
                (None, None) => {}
                (Some(expected), Some(bytes))
                    if expected == <[u8; 32]>::from(Sha256::digest(bytes)) => {}
                _ => return Err(ArtifactIntegrationError::ContentCasMismatch),
            }
            let replacement = arguments
                .replacement_bytes()
                .map_err(|_| ArtifactIntegrationError::Rejected)?;
            let oid = destination.blob(&replacement)?;
            let mut entry = IndexEntry {
                ctime: git2::IndexTime::new(0, 0),
                mtime: git2::IndexTime::new(0, 0),
                dev: 0,
                ino: 0,
                mode: if arguments.executable {
                    0o100755
                } else {
                    0o100644
                },
                uid: 0,
                gid: 0,
                file_size: replacement.len() as u32,
                id: oid,
                flags: 0,
                flags_extended: 0,
                path: operation.path().as_bytes().to_vec(),
            };
            index.add(&entry)?;
            entry.path.fill(0);
        }
        LocalCodingEditOperation::Delete(arguments) => {
            let bytes = current_bytes.ok_or(ArtifactIntegrationError::ContentCasMismatch)?;
            if <[u8; 32]>::from(Sha256::digest(&bytes)) != arguments.expected_before_sha256 {
                return Err(ArtifactIntegrationError::ContentCasMismatch);
            }
            index.remove_path(path)?;
        }
    }
    Ok(())
}

fn copy_commit_tree(
    source: &Odb<'_>,
    destination: &Repository,
    commit: Oid,
) -> Result<(), ArtifactIntegrationError> {
    let destination_odb = destination.odb()?;
    let commit_object = source.read(commit)?;
    if commit_object.kind() != ObjectType::Commit {
        return Err(ArtifactIntegrationError::Rejected);
    }
    destination_odb.write(ObjectType::Commit, commit_object.data())?;
    let tree = commit_tree_oid(commit_object.data())?;
    copy_tree(source, &destination_odb, tree)
}

fn copy_tree(
    source: &Odb<'_>,
    destination: &git2::Odb<'_>,
    oid: Oid,
) -> Result<(), ArtifactIntegrationError> {
    let object = source.read(oid)?;
    if object.kind() != ObjectType::Tree {
        return Err(ArtifactIntegrationError::Rejected);
    }
    destination.write(ObjectType::Tree, object.data())?;
    for entry in parse_raw_tree(object.data())? {
        match entry.mode {
            0o040000 => copy_tree(source, destination, entry.oid)?,
            0o100644 | 0o100755 => {
                let blob = source.read(entry.oid)?;
                if blob.kind() != ObjectType::Blob {
                    return Err(ArtifactIntegrationError::Rejected);
                }
                destination.write(ObjectType::Blob, blob.data())?;
            }
            _ => return Err(ArtifactIntegrationError::Rejected),
        }
    }
    Ok(())
}

fn ensure_directory(path: &Path) -> Result<(), ArtifactIntegrationError> {
    if !path.exists() {
        fs::create_dir(path)?;
    }
    if !fs::symlink_metadata(path)?.file_type().is_dir() {
        return Err(ArtifactIntegrationError::Rejected);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if fs::symlink_metadata(path)?.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            return Err(ArtifactIntegrationError::Rejected);
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn validate_plain_file(
    _path: &Path,
    metadata: &fs::Metadata,
) -> Result<(), ArtifactIntegrationError> {
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err(ArtifactIntegrationError::Rejected);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() != 1 {
            return Err(ArtifactIntegrationError::Rejected);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            return Err(ArtifactIntegrationError::Rejected);
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn configure_no_follow_read(options: &mut OpenOptions) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
        };
        options
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    #[cfg(not(any(unix, windows)))]
    options.read(true);
}

fn configure_no_follow_directory(options: &mut OpenOptions) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
        };
        options
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS);
    }
}

fn validate_open_plain_file(file: &File) -> Result<(), ArtifactIntegrationError> {
    let metadata = file.metadata()?;
    validate_plain_file(Path::new(""), &metadata)?;
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        };
        let information = windows_file_information(file)?;
        if information.dwFileAttributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT)
            != 0
            || information.nNumberOfLinks != 1
        {
            return Err(ArtifactIntegrationError::Rejected);
        }
    }
    Ok(())
}

fn validate_open_plain_directory(file: &File) -> Result<(), ArtifactIntegrationError> {
    if !file.metadata()?.is_dir() {
        return Err(ArtifactIntegrationError::Rejected);
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        };
        let information = windows_file_information(file)?;
        if information.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
            || information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        {
            return Err(ArtifactIntegrationError::Rejected);
        }
    }
    Ok(())
}

fn platform_identity(file: &File) -> Result<PlatformIdentity, ArtifactIntegrationError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = file.metadata()?;
        Ok(PlatformIdentity::Unix {
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
    #[cfg(windows)]
    {
        let information = windows_file_information(file)?;
        Ok(PlatformIdentity::Windows {
            volume: information.dwVolumeSerialNumber,
            high: information.nFileIndexHigh,
            low: information.nFileIndexLow,
        })
    }
    #[cfg(not(any(unix, windows)))]
    Ok(PlatformIdentity::Unsupported)
}

#[cfg(not(unix))]
fn cleanup_children(path: &Path, retained: &HashSet<Uuid>) -> Result<(), ArtifactIntegrationError> {
    #[cfg(windows)]
    let _parent = {
        let parent = open_containment_handle(path, true)?;
        validate_open_plain_directory(&parent)?;
        parent
    };
    for child in fs::read_dir(path)? {
        let child = child?;
        let id = child
            .file_name()
            .to_str()
            .and_then(|v| Uuid::parse_str(v).ok());
        if id.is_none() || !retained.contains(&id.unwrap()) {
            remove_plain_tree(&child.path())?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn cleanup_children(path: &Path, retained: &HashSet<Uuid>) -> Result<(), ArtifactIntegrationError> {
    use std::os::fd::{AsRawFd, FromRawFd};
    let mut options = OpenOptions::new();
    options.read(true);
    configure_no_follow_directory(&mut options);
    let parent = options.open(path)?;
    for name in directory_entry_names(&parent)? {
        let id = std::str::from_utf8(name.to_bytes())
            .ok()
            .and_then(|value| Uuid::parse_str(value).ok());
        if id.is_none() || !retained.contains(&id.unwrap()) {
            let descriptor = unsafe {
                libc::openat(
                    parent.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            };
            if descriptor < 0 {
                return Err(ArtifactIntegrationError::Rejected);
            }
            let child = unsafe { File::from_raw_fd(descriptor) };
            let identity = platform_identity(&child)?;
            drop(child);
            remove_tree_at(parent.as_raw_fd(), &name, Some(identity))?;
        }
    }
    Ok(())
}

#[cfg(unix)]
fn directory_entry_names(
    directory: &File,
) -> Result<Vec<std::ffi::CString>, ArtifactIntegrationError> {
    use std::ffi::{CStr, CString};
    use std::os::fd::AsRawFd;
    let duplicate = unsafe { libc::dup(directory.as_raw_fd()) };
    if duplicate < 0 {
        return Err(ArtifactIntegrationError::Io(std::io::Error::last_os_error()));
    }
    let stream = unsafe { libc::fdopendir(duplicate) };
    if stream.is_null() {
        unsafe { libc::close(duplicate) };
        return Err(ArtifactIntegrationError::Io(std::io::Error::last_os_error()));
    }
    let mut names = Vec::new();
    loop {
        let entry = unsafe { libc::readdir(stream) };
        if entry.is_null() {
            break;
        }
        let value = unsafe { CStr::from_ptr((*entry).d_name.as_ptr()) };
        if value.to_bytes() != b"." && value.to_bytes() != b".." {
            names.push(
                CString::new(value.to_bytes()).map_err(|_| ArtifactIntegrationError::Rejected)?,
            );
        }
    }
    unsafe { libc::closedir(stream) };
    Ok(names)
}

fn materialize_tree(
    repository: &Repository,
    root: &Path,
    tree_oid: Oid,
) -> Result<(), ArtifactIntegrationError> {
    materialize_tree_at(repository, root, Path::new(""), tree_oid)
}

fn materialize_tree_at(
    repository: &Repository,
    root: &Path,
    relative: &Path,
    tree_oid: Oid,
) -> Result<(), ArtifactIntegrationError> {
    let tree = repository.find_tree(tree_oid)?;
    for entry in tree.iter() {
        let name = entry.name().ok_or(ArtifactIntegrationError::Rejected)?;
        if name.eq_ignore_ascii_case(".git") || name.contains('/') || name.contains('\\') {
            return Err(ArtifactIntegrationError::Rejected);
        }
        let child_relative = relative.join(name);
        let child = root.join(&child_relative);
        match entry.kind() {
            Some(ObjectType::Tree) => {
                fs::create_dir(&child)?;
                ensure_directory(&child)?;
                materialize_tree_at(repository, root, &child_relative, entry.id())?;
            }
            Some(ObjectType::Blob)
                if entry.filemode() == 0o100644 || entry.filemode() == 0o100755 =>
            {
                let bytes = repository.find_blob(entry.id())?;
                let mut options = OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
                }
                #[cfg(windows)]
                {
                    use std::os::windows::fs::OpenOptionsExt;
                    options.custom_flags(
                        windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT,
                    );
                }
                let mut file = options.open(&child)?;
                file.write_all(bytes.content())?;
                file.sync_all()?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    fs::set_permissions(
                        &child,
                        fs::Permissions::from_mode(if entry.filemode() == 0o100755 {
                            0o700
                        } else {
                            0o600
                        }),
                    )?;
                }
            }
            _ => return Err(ArtifactIntegrationError::Rejected),
        }
    }
    Ok(())
}

fn verify_materialized_tree(
    repository: &Repository,
    root: &Path,
    tree_oid: Oid,
) -> Result<(), ArtifactIntegrationError> {
    let mut expected = HashSet::new();
    collect_tree_paths(repository, Path::new(""), tree_oid, &mut expected)?;
    let mut actual = HashSet::new();
    collect_worktree_paths(root, root, &mut actual)?;
    if expected != actual {
        return Err(ArtifactIntegrationError::Rejected);
    }
    for path in expected {
        let entry = repository
            .find_tree(tree_oid)?
            .get_path(&path)
            .map_err(|_| ArtifactIntegrationError::Rejected)?;
        if entry.kind() != Some(ObjectType::Blob) {
            continue;
        }
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.custom_flags(
                windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT,
            );
        }
        let mut file = options.open(root.join(&path))?;
        let metadata = file.metadata()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let expected_mode = if entry.filemode() == 0o100755 {
                0o700
            } else {
                0o600
            };
            if !metadata.is_file()
                || metadata.nlink() != 1
                || metadata.mode() & 0o777 != expected_mode
            {
                return Err(ArtifactIntegrationError::Rejected);
            }
        }
        #[cfg(not(unix))]
        if !metadata.is_file() {
            return Err(ArtifactIntegrationError::Rejected);
        }
        #[cfg(windows)]
        {
            let info = windows_file_information(&file)?;
            if info.dwFileAttributes
                & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
                != 0
                || info.nNumberOfLinks != 1
            {
                return Err(ArtifactIntegrationError::Rejected);
            }
        }
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        if Oid::hash_object(ObjectType::Blob, &bytes)? != entry.id() {
            return Err(ArtifactIntegrationError::Rejected);
        }
    }
    Ok(())
}

fn ensure_plain_directory(path: &Path) -> Result<(), ArtifactIntegrationError> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(ArtifactIntegrationError::Rejected);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.mode() & 0o077 != 0 {
            return Err(ArtifactIntegrationError::Rejected);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        };
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(ArtifactIntegrationError::Rejected);
        }
        let handle = OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)?;
        if windows_file_information(&handle)?.nNumberOfLinks != 1 {
            return Err(ArtifactIntegrationError::Rejected);
        }
    }
    Ok(())
}

fn sync_plain_tree(path: &Path) -> Result<(), ArtifactIntegrationError> {
    ensure_plain_directory(path)?;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.is_dir() {
            sync_plain_tree(&entry.path())?;
        } else if metadata.is_file() {
            sync_plain_file(&entry.path())?;
        } else {
            return Err(ArtifactIntegrationError::Rejected);
        }
    }
    sync_directory_portable(path)
}

fn sync_plain_file(path: &Path) -> Result<(), ArtifactIntegrationError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut options = OpenOptions::new();
        options.read(true);
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        let file = options.open(path)?;
        validate_open_plain_file(&file)?;
        file.sync_all()?;
        Ok(())
    }
    #[cfg(windows)]
    {
        use std::mem::size_of;
        use std::os::windows::fs::OpenOptionsExt;
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            FileBasicInfo, GetFileInformationByHandleEx, FILE_ATTRIBUTE_READONLY, FILE_BASIC_INFO,
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_READ,
            FILE_SHARE_WRITE, FILE_WRITE_ATTRIBUTES,
        };

        let mut attribute_options = OpenOptions::new();
        attribute_options
            .access_mode(FILE_READ_ATTRIBUTES | FILE_WRITE_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        let attribute_handle = attribute_options.open(path)?;
        validate_open_plain_file(&attribute_handle)?;
        let identity = platform_identity(&attribute_handle)?;
        let mut original = FILE_BASIC_INFO::default();
        if unsafe {
            GetFileInformationByHandleEx(
                attribute_handle.as_raw_handle() as _,
                FileBasicInfo,
                &mut original as *mut _ as _,
                size_of::<FILE_BASIC_INFO>() as u32,
            )
        } == 0
        {
            return Err(ArtifactIntegrationError::Io(std::io::Error::last_os_error()));
        }
        let was_readonly = original.FileAttributes & FILE_ATTRIBUTE_READONLY != 0;
        if was_readonly {
            let mut writable = original;
            writable.FileAttributes &= !FILE_ATTRIBUTE_READONLY;
            set_windows_basic_info(&attribute_handle, &writable)?;
        }

        let sync_result = (|| {
            let mut sync_options = OpenOptions::new();
            sync_options
                .write(true)
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
            let sync_handle = sync_options.open(path)?;
            validate_open_plain_file(&sync_handle)?;
            if platform_identity(&sync_handle)? != identity {
                return Err(ArtifactIntegrationError::Rejected);
            }
            sync_handle.sync_all()?;
            Ok(())
        })();
        let restore_result = if was_readonly {
            set_windows_basic_info(&attribute_handle, &original)
        } else {
            Ok(())
        };
        restore_result?;
        sync_result
    }
    #[cfg(not(any(unix, windows)))]
    {
        let file = File::open(path)?;
        validate_open_plain_file(&file)?;
        file.sync_all()?;
        Ok(())
    }
}

#[cfg(windows)]
fn set_windows_basic_info(
    file: &File,
    information: &windows_sys::Win32::Storage::FileSystem::FILE_BASIC_INFO,
) -> Result<(), ArtifactIntegrationError> {
    use std::mem::size_of;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FileBasicInfo, SetFileInformationByHandle, FILE_BASIC_INFO,
    };
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle() as _,
            FileBasicInfo,
            information as *const _ as _,
            size_of::<FILE_BASIC_INFO>() as u32,
        )
    } == 0
    {
        return Err(ArtifactIntegrationError::Io(std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(unix)]
fn secure_tree_permissions(path: &Path) -> Result<(), ArtifactIntegrationError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.is_dir() {
            secure_tree_permissions(&entry.path())?;
        } else if metadata.is_file() {
            fs::set_permissions(entry.path(), fs::Permissions::from_mode(0o600))?;
        } else {
            return Err(ArtifactIntegrationError::Rejected);
        }
    }
    Ok(())
}

#[cfg(unix)]
fn secure_materialized_permissions(
    repository: &Repository,
    root: &Path,
    tree_oid: Oid,
) -> Result<(), ArtifactIntegrationError> {
    use std::os::unix::fs::PermissionsExt;
    let tree = repository.find_tree(tree_oid)?;
    for entry in tree.iter() {
        let name = entry.name().ok_or(ArtifactIntegrationError::Rejected)?;
        let path = root.join(name);
        match entry.kind() {
            Some(ObjectType::Tree) => {
                fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
                secure_materialized_permissions(repository, &path, entry.id())?;
            }
            Some(ObjectType::Blob) => {
                let mode = if entry.filemode() == 0o100755 {
                    0o700
                } else {
                    0o600
                };
                fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
            }
            _ => return Err(ArtifactIntegrationError::Rejected),
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn secure_materialized_permissions(
    _repository: &Repository,
    _root: &Path,
    _tree_oid: Oid,
) -> Result<(), ArtifactIntegrationError> {
    Ok(())
}

#[cfg(not(unix))]
fn secure_tree_permissions(_path: &Path) -> Result<(), ArtifactIntegrationError> {
    Ok(())
}

#[cfg(unix)]
fn sync_directory_portable(path: &Path) -> Result<(), ArtifactIntegrationError> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?
        .sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_directory_portable(_path: &Path) -> Result<(), ArtifactIntegrationError> {
    Ok(())
}

#[cfg(unix)]
fn remove_plain_tree(path: &Path) -> Result<(), ArtifactIntegrationError> {
    remove_plain_tree_matching(path, Some(path_identity(path)?))
}

#[cfg(unix)]
fn remove_plain_tree_matching(
    path: &Path,
    expected: Option<PlatformIdentity>,
) -> Result<(), ArtifactIntegrationError> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    let parent = path.parent().ok_or(ArtifactIntegrationError::Rejected)?;
    let name = CString::new(
        path.file_name()
            .ok_or(ArtifactIntegrationError::Rejected)?
            .as_bytes(),
    )
    .map_err(|_| ArtifactIntegrationError::Rejected)?;
    let mut options = OpenOptions::new();
    options.read(true);
    configure_no_follow_directory(&mut options);
    let parent_handle = options.open(parent)?;
    validate_open_plain_directory(&parent_handle)?;
    remove_tree_at(parent_handle.as_raw_fd(), &name, expected)
}

#[cfg(unix)]
fn remove_plain_file(path: &Path) -> Result<(), ArtifactIntegrationError> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    let parent = path.parent().ok_or(ArtifactIntegrationError::Rejected)?;
    let name = CString::new(
        path.file_name()
            .ok_or(ArtifactIntegrationError::Rejected)?
            .as_bytes(),
    )
    .map_err(|_| ArtifactIntegrationError::Rejected)?;
    let mut options = OpenOptions::new();
    options.read(true);
    configure_no_follow_directory(&mut options);
    let parent_handle = options.open(parent)?;
    remove_file_at(parent_handle.as_raw_fd(), &name)
}

#[cfg(unix)]
fn remove_tree_at(
    parent_fd: std::os::fd::RawFd,
    name: &std::ffi::CStr,
    expected: Option<PlatformIdentity>,
) -> Result<(), ArtifactIntegrationError> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    let original_descriptor = unsafe {
        libc::openat(
            parent_fd,
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if original_descriptor < 0 {
        return Err(ArtifactIntegrationError::Rejected);
    }
    let original = unsafe { File::from_raw_fd(original_descriptor) };
    validate_open_plain_directory(&original)?;
    let original_identity = platform_identity(&original)?;
    if expected.is_some_and(|expected| expected != original_identity) {
        return Err(ArtifactIntegrationError::Rejected);
    }
    run_cleanup_test_hook(CleanupHookPhase::BeforeCapture);
    let captured = CString::new(format!(".assemblywright-delete-{}", Uuid::new_v4()))
        .map_err(|_| ArtifactIntegrationError::Rejected)?;
    if unsafe { libc::renameat(parent_fd, name.as_ptr(), parent_fd, captured.as_ptr()) } != 0 {
        return Err(ArtifactIntegrationError::Io(std::io::Error::last_os_error()));
    }
    run_cleanup_test_hook(CleanupHookPhase::AfterCapture);
    let descriptor = unsafe {
        libc::openat(
            parent_fd,
            captured.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if descriptor < 0 {
        return Err(ArtifactIntegrationError::Rejected);
    }
    let directory = unsafe { File::from_raw_fd(descriptor) };
    if validate_open_plain_directory(&directory).is_err()
        || platform_identity(&directory).ok() != Some(original_identity)
    {
        let _ = rename_noreplace_at(parent_fd, &captured, name);
        return Err(ArtifactIntegrationError::Rejected);
    }
    for child in directory_entry_names(&directory)? {
        if remove_tree_at(directory.as_raw_fd(), &child, None).is_err()
            && remove_file_at(directory.as_raw_fd(), &child).is_err()
        {
            let _ = rename_noreplace_at(parent_fd, &captured, name);
            return Err(ArtifactIntegrationError::Rejected);
        }
    }
    if unsafe { libc::unlinkat(parent_fd, captured.as_ptr(), libc::AT_REMOVEDIR) } != 0 {
        return Err(ArtifactIntegrationError::Io(std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(unix)]
fn metadata_identity(
    metadata: &fs::Metadata,
) -> Result<PlatformIdentity, ArtifactIntegrationError> {
    use std::os::unix::fs::MetadataExt;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(ArtifactIntegrationError::Rejected);
    }
    Ok(PlatformIdentity::Unix {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(unix)]
fn path_identity(path: &Path) -> Result<PlatformIdentity, ArtifactIntegrationError> {
    metadata_identity(&fs::symlink_metadata(path)?)
}

#[cfg(unix)]
fn remove_file_at(
    parent_fd: std::os::fd::RawFd,
    name: &std::ffi::CStr,
) -> Result<(), ArtifactIntegrationError> {
    use std::ffi::CString;
    use std::os::fd::FromRawFd;
    let original_descriptor = unsafe {
        libc::openat(
            parent_fd,
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if original_descriptor < 0 {
        return Err(ArtifactIntegrationError::Rejected);
    }
    let original = unsafe { File::from_raw_fd(original_descriptor) };
    validate_open_plain_file(&original)?;
    let original_identity = platform_identity(&original)?;
    run_cleanup_test_hook(CleanupHookPhase::BeforeCapture);
    let captured = CString::new(format!(".assemblywright-delete-{}", Uuid::new_v4()))
        .map_err(|_| ArtifactIntegrationError::Rejected)?;
    if unsafe { libc::renameat(parent_fd, name.as_ptr(), parent_fd, captured.as_ptr()) } != 0 {
        return Err(ArtifactIntegrationError::Io(std::io::Error::last_os_error()));
    }
    run_cleanup_test_hook(CleanupHookPhase::AfterCapture);
    let descriptor = unsafe {
        libc::openat(
            parent_fd,
            captured.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if descriptor < 0 {
        let _ = rename_noreplace_at(parent_fd, &captured, name);
        return Err(ArtifactIntegrationError::Rejected);
    }
    let file = unsafe { File::from_raw_fd(descriptor) };
    if validate_open_plain_file(&file).is_err()
        || platform_identity(&file).ok() != Some(original_identity)
    {
        let _ = rename_noreplace_at(parent_fd, &captured, name);
        return Err(ArtifactIntegrationError::Rejected);
    }
    if unsafe { libc::unlinkat(parent_fd, captured.as_ptr(), 0) } != 0 {
        return Err(ArtifactIntegrationError::Io(std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(all(unix, target_vendor = "apple"))]
fn rename_noreplace_at(
    parent_fd: std::os::fd::RawFd,
    from: &std::ffi::CStr,
    to: &std::ffi::CStr,
) -> bool {
    unsafe {
        libc::renameatx_np(
            parent_fd,
            from.as_ptr(),
            parent_fd,
            to.as_ptr(),
            libc::RENAME_EXCL,
        ) == 0
    }
}

#[cfg(all(unix, target_os = "linux"))]
fn rename_noreplace_at(
    parent_fd: std::os::fd::RawFd,
    from: &std::ffi::CStr,
    to: &std::ffi::CStr,
) -> bool {
    unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            parent_fd,
            from.as_ptr(),
            parent_fd,
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        ) == 0
    }
}

#[cfg(all(unix, not(any(target_vendor = "apple", target_os = "linux"))))]
fn rename_noreplace_at(
    _parent_fd: std::os::fd::RawFd,
    _from: &std::ffi::CStr,
    _to: &std::ffi::CStr,
) -> bool {
    false
}

#[cfg(windows)]
fn remove_plain_tree(path: &Path) -> Result<(), ArtifactIntegrationError> {
    remove_windows_entry(path, None, true)
}

#[cfg(windows)]
fn remove_plain_tree_matching(
    path: &Path,
    expected: Option<PlatformIdentity>,
) -> Result<(), ArtifactIntegrationError> {
    remove_windows_entry(path, expected, true)
}

#[cfg(windows)]
fn path_identity(path: &Path) -> Result<PlatformIdentity, ArtifactIntegrationError> {
    let handle = open_identity_handle(path, true)?;
    validate_open_plain_directory(&handle)?;
    platform_identity(&handle)
}

#[cfg(windows)]
fn remove_plain_file(path: &Path) -> Result<(), ArtifactIntegrationError> {
    remove_windows_entry(path, None, false)
}

#[cfg(windows)]
fn remove_windows_entry(
    path: &Path,
    expected: Option<PlatformIdentity>,
    directory: bool,
) -> Result<(), ArtifactIntegrationError> {
    let original = open_identity_handle(path, directory)?;
    if directory {
        validate_open_plain_directory(&original)?;
    } else {
        validate_open_plain_file(&original)?;
    }
    let identity = platform_identity(&original)?;
    if expected.is_some_and(|expected| expected != identity) {
        return Err(ArtifactIntegrationError::Rejected);
    }
    run_cleanup_test_hook(CleanupHookPhase::BeforeCapture);
    let captured = path
        .parent()
        .ok_or(ArtifactIntegrationError::Rejected)?
        .join(format!(".assemblywright-delete-{}", Uuid::new_v4()));
    fs::rename(path, &captured)?;
    run_cleanup_test_hook(CleanupHookPhase::AfterCapture);
    let captured_entries = if directory {
        collect_windows_cleanup_entries(&captured)
    } else {
        open_windows_cleanup_entry(&captured, PathBuf::new(), false).map(|entry| vec![entry])
    };
    let mut captured_entries = match captured_entries {
        Ok(entries)
            if entries.iter().any(|entry| {
                entry.relative.as_os_str().is_empty() && entry.identity == identity
            }) =>
        {
            entries
        }
        _ => {
            let _ = windows_move_noreplace(&captured, path);
            return Err(ArtifactIntegrationError::Rejected);
        }
    };
    if platform_identity(&original)? != identity {
        let _ = windows_move_noreplace(&captured, path);
        return Err(ArtifactIntegrationError::Rejected);
    }
    run_cleanup_test_hook(CleanupHookPhase::AfterInventory);
    captured_entries.sort_by(|left, right| {
        right
            .relative
            .components()
            .count()
            .cmp(&left.relative.components().count())
            .then_with(|| right.directory.cmp(&left.directory))
    });
    for entry in captured_entries {
        if platform_identity(&open_identity_handle(&captured, true)?)? != identity {
            return Err(ArtifactIntegrationError::Rejected);
        }
        remove_windows_captured_entry(
            &captured.join(&entry.relative),
            entry.identity,
            entry.directory,
        )?;
    }
    Ok(())
}

#[cfg(windows)]
struct WindowsCleanupEntry {
    relative: PathBuf,
    identity: PlatformIdentity,
    directory: bool,
    _handle: File,
}

#[cfg(windows)]
fn collect_windows_cleanup_entries(
    root: &Path,
) -> Result<Vec<WindowsCleanupEntry>, ArtifactIntegrationError> {
    fn collect(
        root: &Path,
        path: &Path,
        entries: &mut Vec<WindowsCleanupEntry>,
    ) -> Result<(), ArtifactIntegrationError> {
        let relative = path
            .strip_prefix(root)
            .map_err(|_| ArtifactIntegrationError::Rejected)?
            .to_path_buf();
        let directory = open_windows_cleanup_entry(path, relative, true)?;
        for child in fs::read_dir(path)? {
            let child = child?;
            let child_path = child.path();
            let metadata = fs::symlink_metadata(&child_path)?;
            if metadata.is_dir() && !metadata.file_type().is_symlink() {
                collect(root, &child_path, entries)?;
            } else {
                let relative = child_path
                    .strip_prefix(root)
                    .map_err(|_| ArtifactIntegrationError::Rejected)?
                    .to_path_buf();
                entries.push(open_windows_cleanup_entry(&child_path, relative, false)?);
            }
        }
        entries.push(directory);
        Ok(())
    }

    let mut entries = Vec::new();
    collect(root, root, &mut entries)?;
    Ok(entries)
}

#[cfg(windows)]
fn open_windows_cleanup_entry(
    path: &Path,
    relative: PathBuf,
    directory: bool,
) -> Result<WindowsCleanupEntry, ArtifactIntegrationError> {
    let handle = open_identity_handle(path, directory)?;
    if directory {
        validate_open_plain_directory(&handle)?;
    } else {
        validate_open_plain_file(&handle)?;
    }
    Ok(WindowsCleanupEntry {
        relative,
        identity: platform_identity(&handle)?,
        directory,
        _handle: handle,
    })
}

#[cfg(windows)]
fn remove_windows_captured_entry(
    path: &Path,
    expected: PlatformIdentity,
    directory: bool,
) -> Result<(), ArtifactIntegrationError> {
    let identity_handle = open_identity_handle(path, directory)?;
    if directory {
        validate_open_plain_directory(&identity_handle)?;
    } else {
        validate_open_plain_file(&identity_handle)?;
    }
    if platform_identity(&identity_handle)? != expected {
        return Err(ArtifactIntegrationError::Rejected);
    }
    let recaptured = path
        .parent()
        .ok_or(ArtifactIntegrationError::Rejected)?
        .join(format!(".assemblywright-delete-{}", Uuid::new_v4()));
    fs::rename(path, &recaptured)?;
    let containment_handle = open_containment_handle(&recaptured, directory)?;
    if directory {
        validate_open_plain_directory(&containment_handle)?;
    } else {
        validate_open_plain_file(&containment_handle)?;
    }
    if platform_identity(&containment_handle)? != expected {
        return Err(ArtifactIntegrationError::Rejected);
    }
    mark_delete_by_handle(&containment_handle)
}

#[cfg(windows)]
fn open_identity_handle(path: &Path, directory: bool) -> Result<File, ArtifactIntegrationError> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    let mut options = OpenOptions::new();
    options
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(
            FILE_FLAG_OPEN_REPARSE_POINT
                | if directory {
                    FILE_FLAG_BACKUP_SEMANTICS
                } else {
                    0
                },
        );
    Ok(options.open(path)?)
}

#[cfg(windows)]
fn open_containment_handle(path: &Path, directory: bool) -> Result<File, ArtifactIntegrationError> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        DELETE, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    let mut options = OpenOptions::new();
    options
        .access_mode(DELETE | FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(
            FILE_FLAG_OPEN_REPARSE_POINT
                | if directory {
                    FILE_FLAG_BACKUP_SEMANTICS
                } else {
                    0
                },
        );
    Ok(options.open(path)?)
}

#[cfg(windows)]
fn windows_move_noreplace(from: &Path, to: &Path) -> Result<(), ArtifactIntegrationError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_WRITE_THROUGH};

    let from = from
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let to = to
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    if unsafe { MoveFileExW(from.as_ptr(), to.as_ptr(), MOVEFILE_WRITE_THROUGH) } == 0 {
        return Err(ArtifactIntegrationError::Io(std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(windows)]
fn mark_delete_by_handle(file: &File) -> Result<(), ArtifactIntegrationError> {
    use std::mem::size_of;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        FileDispositionInfo, SetFileInformationByHandle, FILE_DISPOSITION_INFO,
    };
    let disposition = FILE_DISPOSITION_INFO { DeleteFile: true };
    if unsafe {
        SetFileInformationByHandle(
            file.as_raw_handle() as _,
            FileDispositionInfo,
            &disposition as *const _ as _,
            size_of::<FILE_DISPOSITION_INFO>() as u32,
        )
    } == 0
    {
        return Err(ArtifactIntegrationError::Io(std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(not(any(unix, windows)))]
fn remove_plain_tree(_path: &Path) -> Result<(), ArtifactIntegrationError> {
    Err(ArtifactIntegrationError::Rejected)
}

#[cfg(not(any(unix, windows)))]
fn remove_plain_tree_matching(
    _path: &Path,
    _expected: Option<PlatformIdentity>,
) -> Result<(), ArtifactIntegrationError> {
    Err(ArtifactIntegrationError::Rejected)
}

#[cfg(not(any(unix, windows)))]
fn path_identity(_path: &Path) -> Result<PlatformIdentity, ArtifactIntegrationError> {
    Err(ArtifactIntegrationError::Rejected)
}

#[cfg(not(any(unix, windows)))]
fn remove_plain_file(_path: &Path) -> Result<(), ArtifactIntegrationError> {
    Err(ArtifactIntegrationError::Rejected)
}

fn collect_tree_paths(
    repository: &Repository,
    relative: &Path,
    oid: Oid,
    paths: &mut HashSet<PathBuf>,
) -> Result<(), ArtifactIntegrationError> {
    let tree = repository.find_tree(oid)?;
    for entry in tree.iter() {
        let child = relative.join(entry.name().ok_or(ArtifactIntegrationError::Rejected)?);
        paths.insert(child.clone());
        if entry.kind() == Some(ObjectType::Tree) {
            collect_tree_paths(repository, &child, entry.id(), paths)?;
        }
    }
    Ok(())
}

fn collect_worktree_paths(
    root: &Path,
    current: &Path,
    paths: &mut HashSet<PathBuf>,
) -> Result<(), ArtifactIntegrationError> {
    for entry in fs::read_dir(current)? {
        let entry = entry?;
        if current == root && entry.file_name() == ".git" {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            return Err(ArtifactIntegrationError::Rejected);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes()
                & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
                != 0
            {
                return Err(ArtifactIntegrationError::Rejected);
            }
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(|_| ArtifactIntegrationError::Rejected)?
            .to_path_buf();
        paths.insert(relative);
        if metadata.is_dir() {
            collect_worktree_paths(root, &entry.path(), paths)?;
        } else if !metadata.is_file() {
            return Err(ArtifactIntegrationError::Rejected);
        }
    }
    Ok(())
}

#[cfg(windows)]
fn windows_file_information(
    file: &std::fs::File,
) -> Result<
    windows_sys::Win32::Storage::FileSystem::BY_HANDLE_FILE_INFORMATION,
    ArtifactIntegrationError,
> {
    use std::mem::MaybeUninit;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };
    let mut info = MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::zeroed();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, info.as_mut_ptr()) } == 0 {
        return Err(ArtifactIntegrationError::Io(std::io::Error::last_os_error()));
    }
    Ok(unsafe { info.assume_init() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RepositorySnapshotStore;
    use assemblywright_protocol::{
        build_local_coding_patch_artifact, FeatureConveyorCodingWorkPacketMetadata,
    };
    use tempfile::tempdir;

    fn source_repository(root: &Path) -> (Repository, String, [u8; 32]) {
        let repository = Repository::init(root).unwrap();
        fs::write(root.join("README.md"), b"before\n").unwrap();
        let before = Sha256::digest(b"before\n").into();
        let mut index = repository.index().unwrap();
        index.add_path(Path::new("README.md")).unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repository.find_tree(tree_id).unwrap();
        let signature =
            Signature::new("fixture", "fixture@example.invalid", &Time::new(1, 0)).unwrap();
        let commit = repository
            .commit(Some("HEAD"), &signature, &signature, "base", &tree, &[])
            .unwrap();
        drop(tree);
        (repository, commit.to_string(), before)
    }

    #[test]
    fn integration_materializes_deterministic_candidate_and_restart_verifies_tamper() {
        let directory = tempdir().unwrap();
        let source_dir = directory.path().join("source");
        let (_source, base, before) = source_repository(&source_dir);
        let snapshot = RepositorySnapshotStore::open(directory.path())
            .unwrap()
            .prepare(&source_dir, &base)
            .unwrap();
        let packet = FeatureConveyorCodingWorkPacketMetadata::fixture(Uuid::from_u128(9), before);
        let bytes = build_local_coding_patch_artifact(&packet).unwrap();
        let reference = ResultArtifactReference {
            artifact_id: Uuid::from_u128(10),
            artifact_sha256: Sha256::digest(&bytes).into(),
            artifact_size_bytes: bytes.len() as u64,
        };
        let artifact_store = ResultArtifactStore::open(directory.path()).unwrap();
        let mut admitted = artifact_store
            .prepare(reference.artifact_id, reference.artifact_sha256, &bytes)
            .unwrap();
        admitted.mark_committed().unwrap();
        let store = ArtifactIntegrationStore::open(directory.path()).unwrap();
        let integration_id = Uuid::from_u128(11);
        let mut prepared = store
            .prepare(
                integration_id,
                snapshot.snapshot_id,
                &base,
                &[IntegrationArtifact { reference, packet }],
            )
            .unwrap();
        let evidence = prepared.evidence.clone();
        let repo_path = directory
            .path()
            .join(CANDIDATE_ROOT)
            .join("candidates")
            .join(integration_id.to_string());
        assert_eq!(
            fs::read(repo_path.join("README.md")).unwrap(),
            assemblywright_protocol::LOCAL_CODING_FIXTURE_CONTENT
        );
        snapshot.retain();
        store
            .verify_referenced(std::slice::from_ref(&evidence))
            .unwrap();
        fs::write(repo_path.join("README.md"), b"tampered-before-finalize").unwrap();
        assert!(matches!(
            prepared.revalidate_candidate(&store),
            Err(ArtifactIntegrationError::Rejected)
        ));
        fs::write(
            repo_path.join("README.md"),
            assemblywright_protocol::LOCAL_CODING_FIXTURE_CONTENT,
        )
        .unwrap();
        prepared.revalidate_candidate(&store).unwrap();
        prepared.retain();
        let repository = Repository::open(&repo_path).unwrap();
        let original_config = fs::read(repo_path.join(".git/config")).unwrap();
        repository
            .set_head_detached(Oid::from_str(&evidence.base_commit).unwrap())
            .unwrap();
        assert!(store
            .verify_referenced(std::slice::from_ref(&evidence))
            .is_err());
        repository
            .set_head_detached(Oid::from_str(&evidence.candidate_commit).unwrap())
            .unwrap();
        repository
            .config()
            .unwrap()
            .set_str("filter.evil.clean", "steal")
            .unwrap();
        assert!(store
            .verify_referenced(std::slice::from_ref(&evidence))
            .is_err());
        fs::write(repo_path.join(".git/config"), &original_config).unwrap();
        repository
            .remote("origin", "https://example.invalid/repo")
            .unwrap();
        assert!(store
            .verify_referenced(std::slice::from_ref(&evidence))
            .is_err());
        repository.remote_delete("origin").unwrap();
        fs::write(repo_path.join(".git/config"), &original_config).unwrap();
        store
            .verify_referenced(std::slice::from_ref(&evidence))
            .unwrap();
        let alternates = repo_path.join(".git/objects/info/alternates");
        fs::write(&alternates, b"/external/object/store\n").unwrap();
        assert!(matches!(
            store.verify_referenced(std::slice::from_ref(&evidence)),
            Err(ArtifactIntegrationError::Rejected)
        ));
        fs::remove_file(alternates).unwrap();
        fs::write(repo_path.join("README.md"), b"tampered").unwrap();
        assert!(matches!(
            store.verify_referenced(&[evidence]),
            Err(ArtifactIntegrationError::Rejected)
        ));
    }

    #[test]
    fn integration_rejects_content_cas_mismatch() {
        let directory = tempdir().unwrap();
        let source_dir = directory.path().join("source");
        let (_source, base, before) = source_repository(&source_dir);
        let snapshot = RepositorySnapshotStore::open(directory.path())
            .unwrap()
            .prepare(&source_dir, &base)
            .unwrap();
        let mut wrong =
            FeatureConveyorCodingWorkPacketMetadata::fixture(Uuid::from_u128(12), before);
        if let LocalCodingEditOperation::Write(arguments) = &mut wrong.operations[0] {
            arguments.expected_before_sha256 = Some([9; 32]);
        }
        let bytes = build_local_coding_patch_artifact(&wrong).unwrap();
        let reference = ResultArtifactReference {
            artifact_id: Uuid::from_u128(13),
            artifact_sha256: Sha256::digest(&bytes).into(),
            artifact_size_bytes: bytes.len() as u64,
        };
        let artifact_store = ResultArtifactStore::open(directory.path()).unwrap();
        artifact_store
            .prepare(reference.artifact_id, reference.artifact_sha256, &bytes)
            .unwrap()
            .mark_committed()
            .unwrap();
        let store = ArtifactIntegrationStore::open(directory.path()).unwrap();
        assert!(matches!(
            store.prepare(
                Uuid::from_u128(14),
                snapshot.snapshot_id,
                &base,
                &[IntegrationArtifact {
                    reference,
                    packet: wrong
                }]
            ),
            Err(ArtifactIntegrationError::ContentCasMismatch)
        ));
        snapshot.retain();
    }

    #[cfg(windows)]
    #[test]
    fn windows_sync_plain_tree_flushes_and_restores_readonly_loose_objects() {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_READONLY;

        let directory = tempdir().unwrap();
        let repository_path = directory.path().join("repository");
        let (repository, _, _) = source_repository(&repository_path);
        drop(repository);
        let objects = repository_path.join(".git").join("objects");
        let loose = fs::read_dir(&objects)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().len() == 2)
            .flat_map(|entry| {
                fs::read_dir(entry.path())
                    .unwrap()
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .collect::<Vec<_>>()
            })
            .next()
            .unwrap();
        let before = fs::metadata(&loose).unwrap().file_attributes();
        assert_ne!(before & FILE_ATTRIBUTE_READONLY, 0);

        sync_plain_tree(&repository_path).unwrap();

        let after = fs::metadata(&loose).unwrap().file_attributes();
        assert_ne!(after & FILE_ATTRIBUTE_READONLY, 0);
    }

    #[cfg(unix)]
    #[test]
    fn integration_uses_captured_snapshot_handles_then_rejects_path_aba() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;
        let directory = tempdir().unwrap();
        let source_dir = directory.path().join("source");
        let (_source, base, before) = source_repository(&source_dir);
        let snapshot = RepositorySnapshotStore::open(directory.path())
            .unwrap()
            .prepare(&source_dir, &base)
            .unwrap();
        let packet = FeatureConveyorCodingWorkPacketMetadata::fixture(Uuid::from_u128(21), before);
        let bytes = build_local_coding_patch_artifact(&packet).unwrap();
        let reference = ResultArtifactReference {
            artifact_id: Uuid::from_u128(22),
            artifact_sha256: Sha256::digest(&bytes).into(),
            artifact_size_bytes: bytes.len() as u64,
        };
        ResultArtifactStore::open(directory.path())
            .unwrap()
            .prepare(reference.artifact_id, reference.artifact_sha256, &bytes)
            .unwrap()
            .mark_committed()
            .unwrap();
        let store = ArtifactIntegrationStore::open(directory.path()).unwrap();
        *store.source_capture_hook.lock().unwrap() = Some(Box::new(|source| {
            let displaced = source.parent().unwrap().join("captured-original");
            fs::rename(source, displaced).unwrap();
            fs::create_dir(source).unwrap();
            fs::create_dir(source.join(".git")).unwrap();
        }));
        let source_consumed = Arc::new(AtomicBool::new(false));
        let observed = source_consumed.clone();
        *store.source_revalidation_hook.lock().unwrap() = Some(Box::new(move |_| {
            observed.store(true, Ordering::SeqCst);
        }));
        assert!(matches!(
            store.prepare(
                Uuid::from_u128(23),
                snapshot.snapshot_id,
                &base,
                &[IntegrationArtifact { reference, packet }]
            ),
            Err(ArtifactIntegrationError::Rejected)
        ));
        assert!(source_consumed.load(Ordering::SeqCst));
        snapshot.retain();
    }

    #[test]
    fn integration_paths_reject_casefolded_exact_and_component_wise_df_collisions() {
        let mut paths = HashSet::new();
        assert!(reserve_integration_path(&mut paths, "README.md"));
        assert!(!reserve_integration_path(&mut paths, "readme.MD"));
        assert!(reserve_integration_path(&mut paths, "src"));
        assert!(!reserve_integration_path(&mut paths, "SRC/lib.rs"));

        let mut reverse = HashSet::new();
        assert!(reserve_integration_path(&mut reverse, "nested/file.rs"));
        assert!(!reserve_integration_path(&mut reverse, "NESTED"));
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_rejects_symlink_and_hardlink_without_following() {
        use std::os::unix::fs::symlink;
        let directory = tempdir().unwrap();
        let store = ArtifactIntegrationStore::open(directory.path()).unwrap();
        let staging = directory.path().join(CANDIDATE_ROOT).join("staging");
        let linked = staging.join(Uuid::new_v4().to_string());
        fs::create_dir(&linked).unwrap();
        fs::write(linked.join("one"), b"x").unwrap();
        fs::hard_link(linked.join("one"), linked.join("two")).unwrap();
        assert!(matches!(
            store.cleanup_unreferenced(&HashSet::new()),
            Err(ArtifactIntegrationError::Rejected)
        ));
        fs::remove_file(linked.join("two")).unwrap();
        fs::remove_file(linked.join("one")).unwrap();
        fs::remove_dir(&linked).unwrap();
        let symlinked = staging.join(Uuid::new_v4().to_string());
        fs::create_dir(&symlinked).unwrap();
        symlink(directory.path(), symlinked.join("escape")).unwrap();
        assert!(matches!(
            store.cleanup_unreferenced(&HashSet::new()),
            Err(ArtifactIntegrationError::Rejected)
        ));
        assert!(directory.path().exists());
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_rejects_replaced_root_identity_without_deleting_replacement() {
        let directory = tempdir().unwrap();
        let parent = directory.path().join("parent");
        fs::create_dir(&parent).unwrap();
        let target = parent.join("target");
        fs::create_dir(&target).unwrap();
        let expected = path_identity(&target).unwrap();
        let displaced = parent.join("displaced");
        fs::rename(&target, &displaced).unwrap();
        fs::create_dir(&target).unwrap();
        fs::write(target.join("must-remain"), b"safe").unwrap();
        assert!(matches!(
            remove_plain_tree_matching(&target, Some(expected)),
            Err(ArtifactIntegrationError::Rejected)
        ));
        assert_eq!(fs::read(target.join("must-remain")).unwrap(), b"safe");
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_leaf_and_directory_substitution_quarantines_without_clobbering() {
        use std::ffi::CString;
        use std::os::fd::AsRawFd;
        let directory = tempdir().unwrap();
        let parent = directory.path().join("parent");
        fs::create_dir(&parent).unwrap();
        let parent_handle = OpenOptions::new().read(true).open(&parent).unwrap();

        let leaf = parent.join("leaf");
        let displaced_leaf = parent.join("displaced-leaf");
        fs::write(&leaf, b"original").unwrap();
        let leaf_before = leaf.clone();
        let leaf_displaced = displaced_leaf.clone();
        CLEANUP_BEFORE_CAPTURE_HOOK.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                fs::rename(&leaf_before, &leaf_displaced).unwrap();
                fs::write(&leaf_before, b"replacement").unwrap();
            }));
        });
        let leaf_after = leaf.clone();
        CLEANUP_AFTER_CAPTURE_HOOK.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                fs::write(&leaf_after, b"occupied").unwrap();
            }));
        });
        let leaf_name = CString::new("leaf").unwrap();
        assert!(matches!(
            remove_file_at(parent_handle.as_raw_fd(), &leaf_name),
            Err(ArtifactIntegrationError::Rejected)
        ));
        assert_eq!(fs::read(&displaced_leaf).unwrap(), b"original");
        assert_eq!(fs::read(&leaf).unwrap(), b"occupied");
        assert!(fs::read_dir(&parent).unwrap().any(|entry| {
            let entry = entry.unwrap();
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".assemblywright-delete-")
                && fs::read(entry.path()).ok().as_deref() == Some(b"replacement")
        }));

        let tree = parent.join("tree");
        let displaced_tree = parent.join("displaced-tree");
        fs::create_dir(&tree).unwrap();
        fs::write(tree.join("original"), b"original").unwrap();
        let tree_before = tree.clone();
        let tree_displaced = displaced_tree.clone();
        CLEANUP_BEFORE_CAPTURE_HOOK.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                fs::rename(&tree_before, &tree_displaced).unwrap();
                fs::create_dir(&tree_before).unwrap();
                fs::write(tree_before.join("replacement"), b"replacement").unwrap();
            }));
        });
        let tree_after = tree.clone();
        CLEANUP_AFTER_CAPTURE_HOOK.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                fs::create_dir(&tree_after).unwrap();
                fs::write(tree_after.join("occupied"), b"occupied").unwrap();
            }));
        });
        let tree_name = CString::new("tree").unwrap();
        assert!(matches!(
            remove_tree_at(parent_handle.as_raw_fd(), &tree_name, None),
            Err(ArtifactIntegrationError::Rejected)
        ));
        assert_eq!(
            fs::read(displaced_tree.join("original")).unwrap(),
            b"original"
        );
        assert_eq!(fs::read(tree.join("occupied")).unwrap(), b"occupied");
        assert!(fs::read_dir(&parent).unwrap().any(|entry| {
            let entry = entry.unwrap();
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".assemblywright-delete-")
                && entry.path().join("replacement").is_file()
        }));
    }

    #[cfg(windows)]
    #[test]
    fn windows_cleanup_substitution_quarantines_without_clobbering() {
        let directory = tempdir().unwrap();
        let leaf = directory.path().join("leaf");
        let displaced = directory.path().join("displaced");
        fs::write(&leaf, b"original").unwrap();
        let before_leaf = leaf.clone();
        let before_displaced = displaced.clone();
        CLEANUP_BEFORE_CAPTURE_HOOK.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                fs::rename(&before_leaf, &before_displaced).unwrap();
                fs::write(&before_leaf, b"replacement").unwrap();
            }));
        });
        let after_leaf = leaf.clone();
        CLEANUP_AFTER_CAPTURE_HOOK.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                fs::write(&after_leaf, b"occupied").unwrap();
            }));
        });
        assert!(matches!(
            remove_windows_entry(&leaf, None, false),
            Err(ArtifactIntegrationError::Rejected)
        ));
        assert_eq!(fs::read(displaced).unwrap(), b"original");
        assert_eq!(fs::read(leaf).unwrap(), b"occupied");
        assert!(fs::read_dir(directory.path()).unwrap().any(|entry| {
            let entry = entry.unwrap();
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".assemblywright-delete-")
                && fs::read(entry.path()).ok().as_deref() == Some(b"replacement")
        }));

        let tree = directory.path().join("tree");
        let displaced_tree = directory.path().join("displaced-tree");
        fs::create_dir(&tree).unwrap();
        fs::write(tree.join("original"), b"original").unwrap();
        let before_tree = tree.clone();
        let before_displaced_tree = displaced_tree.clone();
        CLEANUP_BEFORE_CAPTURE_HOOK.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                fs::rename(&before_tree, &before_displaced_tree).unwrap();
                fs::create_dir(&before_tree).unwrap();
                fs::write(before_tree.join("replacement"), b"replacement").unwrap();
            }));
        });
        let after_tree = tree.clone();
        CLEANUP_AFTER_CAPTURE_HOOK.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                fs::create_dir(&after_tree).unwrap();
                fs::write(after_tree.join("occupied"), b"occupied").unwrap();
            }));
        });
        assert!(matches!(
            remove_windows_entry(&tree, None, true),
            Err(ArtifactIntegrationError::Rejected)
        ));
        assert_eq!(
            fs::read(displaced_tree.join("original")).unwrap(),
            b"original"
        );
        assert_eq!(fs::read(tree.join("occupied")).unwrap(), b"occupied");
        assert!(fs::read_dir(directory.path()).unwrap().any(|entry| {
            let entry = entry.unwrap();
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".assemblywright-delete-")
                && entry.path().join("replacement").is_file()
        }));
    }

    #[cfg(windows)]
    #[test]
    fn windows_cleanup_inventory_holds_captured_directory_against_rebinding() {
        let directory = tempdir().unwrap();
        let tree = directory.path().join("tree");
        let rebound = directory.path().join("rebound");
        fs::create_dir(&tree).unwrap();
        fs::write(tree.join("child"), b"content").unwrap();

        let parent = directory.path().to_path_buf();
        let rebound_for_hook = rebound.clone();
        CLEANUP_AFTER_INVENTORY_HOOK.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                let captured = fs::read_dir(&parent)
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .find(|path| {
                        path.file_name()
                            .unwrap()
                            .to_string_lossy()
                            .starts_with(".assemblywright-delete-")
                    })
                    .unwrap();
                assert!(fs::rename(captured, &rebound_for_hook).is_err());
            }));
        });

        remove_windows_entry(&tree, None, true).unwrap();
        assert!(!tree.exists());
        assert!(!rebound.exists());
    }

    #[cfg(windows)]
    #[test]
    fn windows_cleanup_quarantines_unexpected_post_inventory_entry_without_clobbering() {
        let directory = tempdir().unwrap();
        let tree = directory.path().join("tree");
        fs::create_dir(&tree).unwrap();
        fs::write(tree.join("expected"), b"expected").unwrap();

        let parent = directory.path().to_path_buf();
        let canonical = tree.clone();
        CLEANUP_AFTER_INVENTORY_HOOK.with(|hook| {
            *hook.borrow_mut() = Some(Box::new(move || {
                let captured = fs::read_dir(&parent)
                    .unwrap()
                    .map(|entry| entry.unwrap().path())
                    .find(|path| {
                        path.file_name()
                            .unwrap()
                            .to_string_lossy()
                            .starts_with(".assemblywright-delete-")
                    })
                    .unwrap();
                fs::write(captured.join("unexpected"), b"unexpected").unwrap();
                fs::create_dir(&canonical).unwrap();
                fs::write(canonical.join("occupied"), b"occupied").unwrap();
            }));
        });

        assert!(remove_windows_entry(&tree, None, true).is_err());
        assert_eq!(fs::read(tree.join("occupied")).unwrap(), b"occupied");
        assert!(fs::read_dir(directory.path()).unwrap().any(|entry| {
            let entry = entry.unwrap();
            entry
                .file_name()
                .to_string_lossy()
                .starts_with(".assemblywright-delete-")
                && fs::read(entry.path().join("unexpected")).ok().as_deref() == Some(b"unexpected")
        }));
    }
}
