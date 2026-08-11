use assemblywright_protocol::{
    CancellationAcknowledgement, CancellationAcknowledgementStatus, CancellationInstruction,
    JobEnvelope, JobResultEnvelope, JobResultStatus, LocalCodingJobResult,
    LocalCodingSnapshotChunk, LocalCodingSnapshotChunkRequest, ProtocolError,
    MAX_LOCAL_CODING_SNAPSHOT_BUNDLE_BYTES, PROTOCOL_VERSION,
};
use git2::{ObjectType, Oid, Repository, TreeWalkMode, TreeWalkResult};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};

const BUNDLE_MAGIC: &[u8] = b"AW-SNAPSHOT-BUNDLE-V1\n";
const BUNDLE_END_MAGIC: &[u8] = b"AW-SNAPSHOT-END-V1\n";
const MAX_OBJECTS: usize = 50_000;
const MAX_OBJECT_BYTES: u64 = 256 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_PATH_BYTES: usize = 1024;

#[derive(Debug, thiserror::Error)]
pub enum LocalCodingSnapshotError {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("local coding snapshot materialization is disabled")]
    Disabled,
    #[error("another local coding snapshot is active")]
    AlreadyActive,
    #[error("no matching local coding snapshot is active")]
    NotActive,
    #[error("local coding snapshot filesystem operation failed")]
    Io(#[from] std::io::Error),
    #[error("local coding snapshot object operation failed")]
    Git(#[from] git2::Error),
    #[error("local coding snapshot bundle was rejected")]
    Rejected,
    #[error("local coding snapshot runtime lock was poisoned")]
    Unavailable,
}

#[derive(Debug)]
pub enum LocalCodingSnapshotAcceptance {
    Continue { next_offset: u64 },
    Complete(JobResultEnvelope),
}

#[derive(Debug)]
struct ActiveMaterialization {
    job: JobEnvelope,
    staging_directory: PathBuf,
    bundle_file: File,
    expected_offset: u64,
    total_bytes: Option<u64>,
    verifying: bool,
    cancellation_requested: Arc<AtomicBool>,
}

#[derive(Debug)]
pub struct LocalCodingSnapshotRuntime {
    enabled: bool,
    root: PathBuf,
    active: Mutex<Option<ActiveMaterialization>>,
    completed: Mutex<Option<JobEnvelope>>,
    state_changed: Condvar,
}

impl LocalCodingSnapshotRuntime {
    pub fn open(data_dir: &Path, enabled: bool) -> Result<Self, LocalCodingSnapshotError> {
        let root = data_dir.join("local-coding-snapshots");
        ensure_private_directory(&root)?;
        cleanup_children(&root)?;
        Ok(Self {
            enabled,
            root,
            active: Mutex::new(None),
            completed: Mutex::new(None),
            state_changed: Condvar::new(),
        })
    }

    pub fn admit(&self, job: JobEnvelope) -> Result<u64, LocalCodingSnapshotError> {
        if !self.enabled {
            return Err(LocalCodingSnapshotError::Disabled);
        }
        job.validate_local_coding()?;
        let mut active = self
            .active
            .lock()
            .map_err(|_| LocalCodingSnapshotError::Unavailable)?;
        if active.is_some() {
            return Err(LocalCodingSnapshotError::AlreadyActive);
        }
        *self
            .completed
            .lock()
            .map_err(|_| LocalCodingSnapshotError::Unavailable)? = None;
        let staging_directory = self.root.join(job.attempt_id.0.to_string());
        fs::create_dir(&staging_directory)?;
        fs::set_permissions(&staging_directory, fs::Permissions::from_mode(0o700))?;
        let bundle_path = staging_directory.join("snapshot.bundle.partial");
        let mut options = OpenOptions::new();
        options
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        let bundle_file = options.open(&bundle_path)?;
        *active = Some(ActiveMaterialization {
            job,
            staging_directory,
            bundle_file,
            expected_offset: 0,
            total_bytes: None,
            verifying: false,
            cancellation_requested: Arc::new(AtomicBool::new(false)),
        });
        Ok(0)
    }

    pub fn accept_chunk(
        &self,
        chunk: LocalCodingSnapshotChunk,
    ) -> Result<LocalCodingSnapshotAcceptance, LocalCodingSnapshotError> {
        let mut guard = self
            .active
            .lock()
            .map_err(|_| LocalCodingSnapshotError::Unavailable)?;
        let active = guard.as_mut().ok_or(LocalCodingSnapshotError::NotActive)?;
        if active.verifying || active.cancellation_requested.load(Ordering::Acquire) {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        let request = request_for_job(&active.job, active.expected_offset)?;
        chunk.validate_for_request(&request)?;
        if active.expected_offset != chunk.offset
            || active
                .total_bytes
                .is_some_and(|total| total != chunk.total_bytes)
        {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        active.total_bytes.get_or_insert(chunk.total_bytes);
        let content = chunk.decode_content()?;
        let metadata = active.bundle_file.metadata()?;
        if !metadata.is_file()
            || metadata.nlink() != 1
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.len() != active.expected_offset
        {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        active.bundle_file.write_all(&content)?;
        active.expected_offset = active
            .expected_offset
            .checked_add(content.len() as u64)
            .ok_or(LocalCodingSnapshotError::Rejected)?;
        if !chunk.complete {
            return Ok(LocalCodingSnapshotAcceptance::Continue {
                next_offset: active.expected_offset,
            });
        }
        active.bundle_file.sync_all()?;
        if active.total_bytes != Some(active.expected_offset) {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        active.verifying = true;
        let verification = ActiveMaterialization {
            job: active.job.clone(),
            staging_directory: active.staging_directory.clone(),
            bundle_file: active.bundle_file.try_clone()?,
            expected_offset: active.expected_offset,
            total_bytes: active.total_bytes,
            verifying: true,
            cancellation_requested: Arc::clone(&active.cancellation_requested),
        };
        drop(guard);
        let verification_result = materialize_and_verify(&self.root, &verification);
        let cleanup_result = cleanup_tree(&verification.staging_directory);
        let mut guard = self
            .active
            .lock()
            .map_err(|_| LocalCodingSnapshotError::Unavailable)?;
        let active = guard.as_ref().ok_or(LocalCodingSnapshotError::NotActive)?;
        if active.job.attempt_id != verification.job.attempt_id || !active.verifying {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        if let Err(error) = cleanup_result {
            if let Some(active) = guard.as_mut() {
                active.verifying = false;
            }
            self.state_changed.notify_all();
            return Err(error);
        }
        if active.cancellation_requested.load(Ordering::Acquire) {
            guard.take();
            self.state_changed.notify_all();
            return Err(LocalCodingSnapshotError::Rejected);
        }
        if let Err(error) = verification_result {
            guard.take();
            self.state_changed.notify_all();
            return Err(error);
        }
        let result = match build_result(&verification.job) {
            Ok(result) => result,
            Err(error) => {
                guard.take();
                self.state_changed.notify_all();
                return Err(error);
            }
        };
        *self
            .completed
            .lock()
            .map_err(|_| LocalCodingSnapshotError::Unavailable)? = Some(verification.job);
        guard.take();
        self.state_changed.notify_all();
        Ok(LocalCodingSnapshotAcceptance::Complete(result))
    }

    pub fn cancel(
        &self,
        instruction: &CancellationInstruction,
    ) -> Result<CancellationAcknowledgement, LocalCodingSnapshotError> {
        let mut guard = self
            .active
            .lock()
            .map_err(|_| LocalCodingSnapshotError::Unavailable)?;
        if let Some(active) = guard.as_mut() {
            instruction.validate_for_job(&active.job)?;
            if active.verifying {
                let attempt_id = active.job.attempt_id;
                active.cancellation_requested.store(true, Ordering::Release);
                while guard
                    .as_ref()
                    .is_some_and(|active| active.job.attempt_id == attempt_id && active.verifying)
                {
                    guard = self
                        .state_changed
                        .wait(guard)
                        .map_err(|_| LocalCodingSnapshotError::Unavailable)?;
                }
                if let Some(active) = guard
                    .as_ref()
                    .filter(|active| active.job.attempt_id == attempt_id)
                {
                    cleanup_tree(&active.staging_directory)?;
                    guard.take();
                }
                return cancellation_acknowledgement(instruction);
            }
            cleanup_tree(&active.staging_directory)?;
            guard.take();
            return cancellation_acknowledgement(instruction);
        }
        drop(guard);
        let mut completed = self
            .completed
            .lock()
            .map_err(|_| LocalCodingSnapshotError::Unavailable)?;
        let job = completed
            .as_ref()
            .ok_or(LocalCodingSnapshotError::NotActive)?;
        instruction.validate_for_job(job)?;
        completed.take();
        cancellation_acknowledgement(instruction)
    }

    pub fn shutdown(&self) -> Result<(), LocalCodingSnapshotError> {
        let mut guard = self
            .active
            .lock()
            .map_err(|_| LocalCodingSnapshotError::Unavailable)?;
        if guard.as_ref().is_some_and(|active| active.verifying) {
            if let Some(active) = guard.as_ref() {
                active.cancellation_requested.store(true, Ordering::Release);
            }
            while guard.as_ref().is_some_and(|active| active.verifying) {
                guard = self
                    .state_changed
                    .wait(guard)
                    .map_err(|_| LocalCodingSnapshotError::Unavailable)?;
            }
        }
        if let Some(active) = guard.as_ref() {
            cleanup_tree(&active.staging_directory)?;
            guard.take();
        }
        *self
            .completed
            .lock()
            .map_err(|_| LocalCodingSnapshotError::Unavailable)? = None;
        Ok(())
    }
}

fn cancellation_acknowledgement(
    instruction: &CancellationInstruction,
) -> Result<CancellationAcknowledgement, LocalCodingSnapshotError> {
    Ok(CancellationAcknowledgement {
        protocol_version: PROTOCOL_VERSION,
        connection_epoch: instruction.connection_epoch,
        sequence: instruction
            .sequence
            .checked_add(1)
            .ok_or(LocalCodingSnapshotError::Rejected)?,
        task_id: instruction.task_id,
        step_id: instruction.step_id,
        attempt_id: instruction.attempt_id,
        lease_id: instruction.lease_id,
        cancellation_id: instruction.cancellation_id,
        status: CancellationAcknowledgementStatus::Cancelled,
    })
}

fn request_for_job(
    job: &JobEnvelope,
    offset: u64,
) -> Result<LocalCodingSnapshotChunkRequest, LocalCodingSnapshotError> {
    let context = job.validate_local_coding()?;
    Ok(LocalCodingSnapshotChunkRequest {
        protocol_version: job.protocol_version,
        connection_epoch: job.connection_epoch,
        task_id: job.task_id,
        step_id: job.step_id,
        attempt_id: job.attempt_id,
        lease_id: job.lease_id,
        cancellation_id: job.cancellation_id,
        snapshot_id: context.snapshot_id,
        snapshot_sha256: context.snapshot_sha256,
        offset,
    })
}

fn build_result(job: &JobEnvelope) -> Result<JobResultEnvelope, LocalCodingSnapshotError> {
    let context = job.validate_local_coding()?;
    let mut admission = Sha256::new();
    admission.update(b"assemblywright.local-coding-snapshot-admission.v1\0");
    admission.update(serde_json::to_vec(job).map_err(|_| LocalCodingSnapshotError::Rejected)?);
    let payload = serde_json::to_value(LocalCodingJobResult {
        status: "snapshot_materialized".to_string(),
        work_packet_sha256: context.work_packet_sha256,
        admission_sha256: admission.finalize().into(),
        snapshot_sha256: context.snapshot_sha256,
        mutation_performed: false,
    })
    .map_err(|_| LocalCodingSnapshotError::Rejected)?;
    let payload_sha256 = json_sha256(&payload)?;
    Ok(JobResultEnvelope {
        protocol_version: job.protocol_version,
        connection_epoch: job.connection_epoch,
        sequence: job
            .sequence
            .checked_add(1)
            .ok_or(LocalCodingSnapshotError::Rejected)?,
        task_id: job.task_id,
        step_id: job.step_id,
        attempt_id: job.attempt_id,
        lease_id: job.lease_id,
        cancellation_id: job.cancellation_id,
        status: JobResultStatus::Completed,
        context_sha256: job.context_sha256,
        payload_sha256,
        payload,
    })
}

fn materialize_and_verify(
    root: &Path,
    active: &ActiveMaterialization,
) -> Result<(), LocalCodingSnapshotError> {
    reject_if_cancelled(&active.cancellation_requested)?;
    let context = active.job.validate_local_coding()?;
    let materialized = root.join(format!("{}.materialized", active.job.attempt_id.0));
    fs::create_dir(&materialized)?;
    fs::set_permissions(&materialized, fs::Permissions::from_mode(0o700))?;
    let mut bundle_file = active.bundle_file.try_clone()?;
    bundle_file.seek(SeekFrom::Start(0))?;
    let result = parse_bundle(
        &mut bundle_file,
        &materialized,
        context.snapshot_sha256,
        &active.cancellation_requested,
    );
    let cleanup = cleanup_tree(&materialized);
    match (result, cleanup) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(error),
    }
}

fn parse_bundle(
    file: &mut File,
    destination: &Path,
    expected_snapshot_sha256: [u8; 32],
    cancellation_requested: &AtomicBool,
) -> Result<(), LocalCodingSnapshotError> {
    reject_if_cancelled(cancellation_requested)?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.len() == 0
        || metadata.len() > MAX_LOCAL_CODING_SNAPSHOT_BUNDLE_BYTES
    {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    require_exact(file, BUNDLE_MAGIC)?;
    let commit_text = read_exact_vec(file, 40)?;
    if !commit_text
        .iter()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    let commit = Oid::from_str(
        std::str::from_utf8(&commit_text).map_err(|_| LocalCodingSnapshotError::Rejected)?,
    )?;
    let repository = Repository::init(destination)?;
    let odb = repository.odb()?;
    let mut object_count = 0_usize;
    let mut object_bytes = 0_u64;
    let mut object_ids = HashSet::new();
    loop {
        reject_if_cancelled(cancellation_requested)?;
        let marker = read_u8(file)?;
        if marker == 0 {
            break;
        }
        if marker != 1 {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        let kind = match read_u8(file)? {
            1 => ObjectType::Commit,
            2 => ObjectType::Tree,
            3 => ObjectType::Blob,
            _ => return Err(LocalCodingSnapshotError::Rejected),
        };
        let oid = Oid::from_bytes(&read_exact_vec(file, 20)?)?;
        let length = read_u64(file)?;
        if length > MAX_OBJECT_BYTES
            || (kind == ObjectType::Blob && length > MAX_FILE_BYTES)
            || object_count >= MAX_OBJECTS
        {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        object_bytes = object_bytes
            .checked_add(length)
            .ok_or(LocalCodingSnapshotError::Rejected)?;
        if object_bytes > MAX_OBJECT_BYTES || !object_ids.insert(oid) {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        let data = read_exact_vec(
            file,
            usize::try_from(length).map_err(|_| LocalCodingSnapshotError::Rejected)?,
        )?;
        reject_if_cancelled(cancellation_requested)?;
        if odb.write(kind, &data)? != oid {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        object_count += 1;
    }
    if !object_ids.contains(&commit) {
        return Err(LocalCodingSnapshotError::Rejected);
    }

    let mut digest = Sha256::new();
    digest.update(b"assemblywright.repository-snapshot.v1\0");
    digest.update(commit.as_bytes());
    let mut seen_paths = HashSet::new();
    let mut manifest = HashMap::new();
    let mut file_count = 0_usize;
    loop {
        reject_if_cancelled(cancellation_requested)?;
        let marker = read_u8(file)?;
        if marker == 0 {
            break;
        }
        if marker != 1 || file_count >= MAX_OBJECTS {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        let path_length = usize::from(read_u16(file)?);
        if path_length == 0 || path_length > MAX_PATH_BYTES {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        let path_bytes = read_exact_vec(file, path_length)?;
        let relative = validate_relative_path(&path_bytes, &mut seen_paths)?;
        let mode = read_u32(file)?;
        if !matches!(mode, 0o100644 | 0o100755) {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        let oid = Oid::from_bytes(&read_exact_vec(file, 20)?)?;
        let expected_size = read_u64(file)?;
        if expected_size > MAX_FILE_BYTES || !object_ids.contains(&oid) {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        let blob = odb.read(oid)?;
        if blob.kind() != ObjectType::Blob || blob.data().len() as u64 != expected_size {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        write_materialized_file(destination, &relative, blob.data(), mode)?;
        reject_if_cancelled(cancellation_requested)?;
        let normalized = relative
            .to_str()
            .ok_or(LocalCodingSnapshotError::Rejected)?
            .replace('\\', "/");
        manifest.insert(normalized.clone(), (mode, oid, expected_size));
        digest.update((normalized.len() as u64).to_be_bytes());
        digest.update(normalized.as_bytes());
        digest.update(mode.to_be_bytes());
        digest.update(oid.as_bytes());
        digest.update(expected_size.to_be_bytes());
        digest.update(blob.data());
        file_count += 1;
    }
    require_exact(file, BUNDLE_END_MAGIC)?;
    let footer = read_exact_vec(file, 32)?;
    let computed: [u8; 32] = digest.finalize().into();
    if footer.as_slice() != expected_snapshot_sha256 || computed != expected_snapshot_sha256 {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    let mut trailing = [0_u8; 1];
    if file.read(&mut trailing)? != 0 {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    validate_exact_object_graph(&repository, commit, &object_ids, &manifest)?;
    reject_if_cancelled(cancellation_requested)?;
    write_repository_metadata(&repository, commit)?;
    let reopened = Repository::open(destination)?;
    if reopened.head()?.target() != Some(commit) || !reopened.is_shallow() {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    Ok(())
}

fn reject_if_cancelled(
    cancellation_requested: &AtomicBool,
) -> Result<(), LocalCodingSnapshotError> {
    if cancellation_requested.load(Ordering::Acquire) {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    Ok(())
}

fn validate_exact_object_graph(
    repository: &Repository,
    commit: Oid,
    object_ids: &HashSet<Oid>,
    manifest: &HashMap<String, (u32, Oid, u64)>,
) -> Result<(), LocalCodingSnapshotError> {
    let commit_object = repository.find_commit(commit)?;
    let tree = commit_object.tree()?;
    let mut reachable = HashSet::from([commit, tree.id()]);
    let mut reached_paths = HashSet::new();
    let mut rejected = false;
    tree.walk(TreeWalkMode::PreOrder, |root, entry| {
        if rejected {
            return TreeWalkResult::Abort;
        }
        let Some(name) = entry.name() else {
            rejected = true;
            return TreeWalkResult::Abort;
        };
        if root.len() + name.len() > MAX_PATH_BYTES {
            rejected = true;
            return TreeWalkResult::Abort;
        }
        reachable.insert(entry.id());
        match entry.kind() {
            Some(ObjectType::Tree) => TreeWalkResult::Ok,
            Some(ObjectType::Blob) => {
                let path = format!("{root}{name}");
                let mode = entry.filemode() as u32;
                let expected = manifest.get(&path);
                let blob = repository.find_blob(entry.id());
                match (expected, blob) {
                    (Some((expected_mode, expected_oid, expected_size)), Ok(blob))
                        if *expected_mode == mode
                            && *expected_oid == entry.id()
                            && *expected_size == blob.size() as u64
                            && reached_paths.insert(path) =>
                    {
                        TreeWalkResult::Ok
                    }
                    _ => {
                        rejected = true;
                        TreeWalkResult::Abort
                    }
                }
            }
            _ => {
                rejected = true;
                TreeWalkResult::Abort
            }
        }
    })?;
    if rejected
        || reachable != *object_ids
        || reached_paths.len() != manifest.len()
        || !manifest.keys().all(|path| reached_paths.contains(path))
    {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    Ok(())
}

fn write_repository_metadata(
    repository: &Repository,
    commit: Oid,
) -> Result<(), LocalCodingSnapshotError> {
    let git_dir = repository.path();
    fs::write(git_dir.join("HEAD"), b"ref: refs/heads/snapshot\n")?;
    fs::write(git_dir.join("shallow"), format!("{commit}\n"))?;
    let refs = git_dir.join("refs").join("heads");
    fs::create_dir_all(&refs)?;
    fs::write(refs.join("snapshot"), format!("{commit}\n"))?;
    let mut config = repository.config()?;
    config.set_bool("core.bare", false)?;
    config.set_bool("core.filemode", false)?;
    config.set_bool("core.autocrlf", false)?;
    config.set_bool("core.symlinks", false)?;
    config.set_str("core.hooksPath", ".git/hooks-disabled")?;
    fs::create_dir_all(git_dir.join("hooks-disabled"))?;
    let commit = repository.find_commit(commit)?;
    let tree = commit.tree()?;
    let mut index = repository.index()?;
    index.read_tree(&tree)?;
    index.write()?;
    Ok(())
}

fn write_materialized_file(
    root: &Path,
    relative: &Path,
    data: &[u8],
    mode: u32,
) -> Result<(), LocalCodingSnapshotError> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        create_private_directories(root, parent)?;
    }
    let mut options = OpenOptions::new();
    options
        .write(true)
        .create_new(true)
        .mode(if mode == 0o100755 { 0o700 } else { 0o600 })
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let mut file = options.open(&path)?;
    file.write_all(data)?;
    file.sync_all()?;
    Ok(())
}

fn create_private_directories(root: &Path, target: &Path) -> Result<(), LocalCodingSnapshotError> {
    let relative = target
        .strip_prefix(root)
        .map_err(|_| LocalCodingSnapshotError::Rejected)?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(LocalCodingSnapshotError::Rejected);
        };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(LocalCodingSnapshotError::Rejected);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current)?;
                fs::set_permissions(&current, fs::Permissions::from_mode(0o700))?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn validate_relative_path(
    bytes: &[u8],
    seen: &mut HashSet<String>,
) -> Result<PathBuf, LocalCodingSnapshotError> {
    let text = std::str::from_utf8(bytes).map_err(|_| LocalCodingSnapshotError::Rejected)?;
    if text.is_empty()
        || text.len() > MAX_PATH_BYTES
        || text.contains('\\')
        || text.split('/').any(|component| component.is_empty())
    {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    let path = PathBuf::from(text);
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    for component in path.components() {
        let Component::Normal(name) = component else {
            return Err(LocalCodingSnapshotError::Rejected);
        };
        let name = name.to_str().ok_or(LocalCodingSnapshotError::Rejected)?;
        if name.is_empty()
            || name == "."
            || name == ".."
            || name.contains(['/', '\\', ':'])
            || name.ends_with([' ', '.'])
            || name.eq_ignore_ascii_case(".git")
            || is_windows_reserved_name(name)
        {
            return Err(LocalCodingSnapshotError::Rejected);
        }
    }
    if !seen.insert(text.to_ascii_lowercase()) {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    Ok(path)
}

fn is_windows_reserved_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

fn ensure_private_directory(path: &Path) -> Result<(), LocalCodingSnapshotError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink()
                || !metadata.is_dir()
                || metadata.permissions().mode() & 0o077 != 0
            {
                return Err(LocalCodingSnapshotError::Rejected);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path)?;
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

fn cleanup_children(path: &Path) -> Result<(), LocalCodingSnapshotError> {
    for entry in fs::read_dir(path)? {
        cleanup_tree(&entry?.path())?;
    }
    Ok(())
}

fn cleanup_tree(path: &Path) -> Result<(), LocalCodingSnapshotError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        fs::remove_file(path)?;
        return Ok(());
    }
    for entry in fs::read_dir(path)? {
        cleanup_tree(&entry?.path())?;
    }
    fs::remove_dir(path)?;
    Ok(())
}

fn require_exact(reader: &mut File, expected: &[u8]) -> Result<(), LocalCodingSnapshotError> {
    if read_exact_vec(reader, expected.len())? != expected {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    Ok(())
}

fn read_exact_vec(reader: &mut File, length: usize) -> Result<Vec<u8>, LocalCodingSnapshotError> {
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn read_u8(reader: &mut File) -> Result<u8, LocalCodingSnapshotError> {
    Ok(read_exact_vec(reader, 1)?[0])
}

fn read_u16(reader: &mut File) -> Result<u16, LocalCodingSnapshotError> {
    Ok(u16::from_be_bytes(
        read_exact_vec(reader, 2)?
            .try_into()
            .map_err(|_| LocalCodingSnapshotError::Rejected)?,
    ))
}

fn read_u32(reader: &mut File) -> Result<u32, LocalCodingSnapshotError> {
    Ok(u32::from_be_bytes(
        read_exact_vec(reader, 4)?
            .try_into()
            .map_err(|_| LocalCodingSnapshotError::Rejected)?,
    ))
}

fn read_u64(reader: &mut File) -> Result<u64, LocalCodingSnapshotError> {
    Ok(u64::from_be_bytes(
        read_exact_vec(reader, 8)?
            .try_into()
            .map_err(|_| LocalCodingSnapshotError::Rejected)?,
    ))
}

fn json_sha256(value: &serde_json::Value) -> Result<[u8; 32], LocalCodingSnapshotError> {
    let encoded = serde_json::to_vec(value).map_err(|_| LocalCodingSnapshotError::Rejected)?;
    Ok(Sha256::digest(encoded).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use assemblywright_protocol::{
        AttemptId, CancellationId, ContextHandlingPolicy, DeviceId,
        FeatureConveyorCodingWorkPacketMetadata, LeaseId, LocalCodingJobRequest, Sensitivity,
        StepId, TaskId, LOCAL_CODING_CAPABILITY_ID, LOCAL_CODING_MODEL,
    };
    use git2::{IndexAddOption, Signature};
    use serde_json::to_value;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn job() -> JobEnvelope {
        job_with_snapshot([3; 32])
    }

    fn job_with_snapshot(snapshot_sha256: [u8; 32]) -> JobEnvelope {
        let context = to_value(LocalCodingJobRequest {
            feature_id: Uuid::new_v4(),
            specification_revision: 1,
            lifecycle_revision: 2,
            feature_lease_id: Uuid::new_v4(),
            snapshot_id: Uuid::new_v4(),
            snapshot_sha256,
            work_packet_sha256: [4; 32],
            work_packet: FeatureConveyorCodingWorkPacketMetadata {
                packet_id: Uuid::new_v4(),
                ordinal: 1,
                acceptance_criteria_count: 1,
            },
            device_id: DeviceId::new(Uuid::new_v4()),
            device_registry_revision: 1,
            queue_revision: 1,
            emergency_pause_revision: 1,
        })
        .unwrap();
        JobEnvelope {
            protocol_version: PROTOCOL_VERSION,
            connection_epoch: 1,
            sequence: 1,
            task_id: TaskId::new(Uuid::new_v4()),
            step_id: StepId::new(Uuid::new_v4()),
            attempt_id: AttemptId::new(Uuid::new_v4()),
            lease_id: LeaseId::new(Uuid::new_v4()),
            cancellation_id: CancellationId::new(Uuid::new_v4()),
            capability_id: LOCAL_CODING_CAPABILITY_ID.to_string(),
            selected_model: LOCAL_CODING_MODEL.to_string(),
            sensitivity: Sensitivity::Workspace,
            context_handling: ContextHandlingPolicy::EphemeralNoRetention,
            lease_duration_ms: 60_000,
            deadline_after_ms: 60_000,
            context_sha256: json_sha256(&context).unwrap(),
            context,
        }
    }

    fn fixture_bundle() -> (Vec<u8>, [u8; 32]) {
        fixture_bundle_with_extra_object(false)
    }

    fn fixture_bundle_with_extra_object(include_extra_object: bool) -> (Vec<u8>, [u8; 32]) {
        let source = tempdir().unwrap();
        let repository = Repository::init(source.path()).unwrap();
        let content = b"bounded materialization\n";
        fs::write(source.path().join("README.md"), content).unwrap();
        let mut index = repository.index().unwrap();
        index
            .add_all(["README.md"].iter(), IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repository.find_tree(tree_oid).unwrap();
        let blob_oid = tree.get_name("README.md").unwrap().id();
        let signature = Signature::now("Assemblywright Test", "test@example.invalid").unwrap();
        let commit_oid = repository
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                "bounded fixture",
                &tree,
                &[],
            )
            .unwrap();
        drop(tree);
        drop(index);
        let odb = repository.odb().unwrap();
        let mut bundle = Vec::new();
        bundle.extend_from_slice(BUNDLE_MAGIC);
        bundle.extend_from_slice(commit_oid.to_string().as_bytes());
        for (kind, oid) in [
            (ObjectType::Commit, commit_oid),
            (ObjectType::Tree, tree_oid),
            (ObjectType::Blob, blob_oid),
        ] {
            let object = odb.read(oid).unwrap();
            assert_eq!(object.kind(), kind);
            bundle.extend_from_slice(&[
                1,
                match kind {
                    ObjectType::Commit => 1,
                    ObjectType::Tree => 2,
                    ObjectType::Blob => 3,
                    _ => unreachable!(),
                },
            ]);
            bundle.extend_from_slice(oid.as_bytes());
            bundle.extend_from_slice(&(object.data().len() as u64).to_be_bytes());
            bundle.extend_from_slice(object.data());
        }
        if include_extra_object {
            let content = b"unreferenced secret object\n";
            let oid = odb.write(ObjectType::Blob, content).unwrap();
            bundle.extend_from_slice(&[1, 3]);
            bundle.extend_from_slice(oid.as_bytes());
            bundle.extend_from_slice(&(content.len() as u64).to_be_bytes());
            bundle.extend_from_slice(content);
        }
        bundle.push(0);
        bundle.push(1);
        bundle.extend_from_slice(&("README.md".len() as u16).to_be_bytes());
        bundle.extend_from_slice(b"README.md");
        bundle.extend_from_slice(&0o100644_u32.to_be_bytes());
        bundle.extend_from_slice(blob_oid.as_bytes());
        bundle.extend_from_slice(&(content.len() as u64).to_be_bytes());
        bundle.push(0);
        let mut digest = Sha256::new();
        digest.update(b"assemblywright.repository-snapshot.v1\0");
        digest.update(commit_oid.as_bytes());
        digest.update(("README.md".len() as u64).to_be_bytes());
        digest.update(b"README.md");
        digest.update(0o100644_u32.to_be_bytes());
        digest.update(blob_oid.as_bytes());
        digest.update((content.len() as u64).to_be_bytes());
        digest.update(content);
        let snapshot_sha256: [u8; 32] = digest.finalize().into();
        bundle.extend_from_slice(BUNDLE_END_MAGIC);
        bundle.extend_from_slice(&snapshot_sha256);
        (bundle, snapshot_sha256)
    }

    fn chunk(
        job: &JobEnvelope,
        offset: u64,
        total_bytes: u64,
        content: &[u8],
    ) -> LocalCodingSnapshotChunk {
        let request = request_for_job(job, offset).unwrap();
        let mut content_hex = String::with_capacity(content.len() * 2);
        for byte in content {
            use std::fmt::Write as _;
            write!(&mut content_hex, "{byte:02x}").unwrap();
        }
        LocalCodingSnapshotChunk {
            protocol_version: request.protocol_version,
            connection_epoch: request.connection_epoch,
            task_id: request.task_id,
            step_id: request.step_id,
            attempt_id: request.attempt_id,
            lease_id: request.lease_id,
            cancellation_id: request.cancellation_id,
            snapshot_id: request.snapshot_id,
            snapshot_sha256: request.snapshot_sha256,
            offset,
            total_bytes,
            content_sha256: Sha256::digest(content).into(),
            content_hex,
            complete: offset + content.len() as u64 == total_bytes,
        }
    }

    fn cancellation(job: &JobEnvelope) -> CancellationInstruction {
        CancellationInstruction {
            protocol_version: job.protocol_version,
            connection_epoch: job.connection_epoch,
            sequence: job.sequence + 2,
            task_id: job.task_id,
            step_id: job.step_id,
            attempt_id: job.attempt_id,
            lease_id: job.lease_id,
            cancellation_id: job.cancellation_id,
            deadline_after_ms: 1_000,
        }
    }

    #[test]
    fn disabled_duplicate_and_startup_cleanup_fail_closed() {
        let directory = tempdir().unwrap();
        let stale = directory.path().join("local-coding-snapshots");
        fs::create_dir(&stale).unwrap();
        fs::set_permissions(&stale, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(stale.join("stale"), b"stale").unwrap();
        let disabled = LocalCodingSnapshotRuntime::open(directory.path(), false).unwrap();
        assert_eq!(fs::read_dir(&stale).unwrap().count(), 0);
        assert!(matches!(
            disabled.admit(job()),
            Err(LocalCodingSnapshotError::Disabled)
        ));

        let runtime = LocalCodingSnapshotRuntime::open(directory.path(), true).unwrap();
        runtime.admit(job()).unwrap();
        assert!(matches!(
            runtime.admit(job()),
            Err(LocalCodingSnapshotError::AlreadyActive)
        ));
        runtime.shutdown().unwrap();
        assert_eq!(fs::read_dir(&stale).unwrap().count(), 0);
    }

    #[test]
    fn exact_chunks_materialize_verify_receipt_and_leave_no_repository_state() {
        let directory = tempdir().unwrap();
        let runtime = LocalCodingSnapshotRuntime::open(directory.path(), true).unwrap();
        let (bundle, snapshot_sha256) = fixture_bundle();
        let job = job_with_snapshot(snapshot_sha256);
        runtime.admit(job.clone()).unwrap();
        let mut offset = 0_usize;
        let mut completed = None;
        while offset < bundle.len() {
            let end = (offset + 23).min(bundle.len());
            match runtime
                .accept_chunk(chunk(
                    &job,
                    offset as u64,
                    bundle.len() as u64,
                    &bundle[offset..end],
                ))
                .unwrap()
            {
                LocalCodingSnapshotAcceptance::Continue { next_offset } => {
                    assert_eq!(next_offset, end as u64)
                }
                LocalCodingSnapshotAcceptance::Complete(result) => completed = Some(result),
            }
            offset = end;
        }
        let result = completed.expect("final chunk returns one receipt");
        result.validate_local_coding_result(&job).unwrap();
        let acknowledgement = runtime.cancel(&cancellation(&job)).unwrap();
        assert_eq!(
            acknowledgement.status,
            CancellationAcknowledgementStatus::Cancelled
        );
        let root = directory.path().join("local-coding-snapshots");
        assert_eq!(fs::read_dir(root).unwrap().count(), 0);
    }

    #[test]
    fn corrupt_or_out_of_order_chunks_fail_and_shutdown_cleans_partial_state() {
        let directory = tempdir().unwrap();
        let runtime = LocalCodingSnapshotRuntime::open(directory.path(), true).unwrap();
        let (bundle, snapshot_sha256) = fixture_bundle();
        let job = job_with_snapshot(snapshot_sha256);
        runtime.admit(job.clone()).unwrap();
        let out_of_order = chunk(&job, 1, bundle.len() as u64, &bundle[..16]);
        assert!(matches!(
            runtime.accept_chunk(out_of_order),
            Err(LocalCodingSnapshotError::Protocol(_)) | Err(LocalCodingSnapshotError::Rejected)
        ));
        runtime.shutdown().unwrap();
        assert_eq!(
            fs::read_dir(directory.path().join("local-coding-snapshots"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn manifest_path_policy_rejects_ambiguous_or_cross_platform_unsafe_names() {
        let mut seen = HashSet::new();
        assert_eq!(
            validate_relative_path(b"Sources/Worker.swift", &mut seen).unwrap(),
            PathBuf::from("Sources/Worker.swift")
        );

        for invalid in [
            b"".as_slice(),
            b"/absolute",
            b"trailing/",
            b"double//separator",
            b"./relative",
            b"parent/../escape",
            b".git/config",
            b"metadata/.GIT/index",
            b"back\\slash",
            b"drive:name",
            b"trailing-dot.",
            b"trailing-space ",
            b"CON",
            b"aux.txt",
            b"COM1.log",
            b"lpt9",
        ] {
            assert!(
                matches!(
                    validate_relative_path(invalid, &mut HashSet::new()),
                    Err(LocalCodingSnapshotError::Rejected)
                ),
                "unsafe manifest path was accepted: {:?}",
                String::from_utf8_lossy(invalid)
            );
        }
        assert!(matches!(
            validate_relative_path(&[0xff], &mut HashSet::new()),
            Err(LocalCodingSnapshotError::Rejected)
        ));
        assert!(matches!(
            validate_relative_path(
                vec![b'a'; MAX_PATH_BYTES + 1].as_slice(),
                &mut HashSet::new()
            ),
            Err(LocalCodingSnapshotError::Rejected)
        ));
        let maximum = (0..5)
            .map(|_| "a".repeat(204))
            .collect::<Vec<_>>()
            .join("/");
        assert_eq!(maximum.len(), MAX_PATH_BYTES);
        assert_eq!(
            validate_relative_path(maximum.as_bytes(), &mut HashSet::new()).unwrap(),
            PathBuf::from(maximum)
        );

        let mut collisions = HashSet::new();
        validate_relative_path(b"Sources/File.swift", &mut collisions).unwrap();
        assert!(matches!(
            validate_relative_path(b"sources/file.swift", &mut collisions),
            Err(LocalCodingSnapshotError::Rejected)
        ));
    }

    #[test]
    fn unreferenced_object_is_rejected_even_when_manifest_digest_is_valid() {
        let directory = tempdir().unwrap();
        let runtime = LocalCodingSnapshotRuntime::open(directory.path(), true).unwrap();
        let (bundle, snapshot_sha256) = fixture_bundle_with_extra_object(true);
        let job = job_with_snapshot(snapshot_sha256);
        runtime.admit(job.clone()).unwrap();
        assert!(matches!(
            runtime.accept_chunk(chunk(&job, 0, bundle.len() as u64, &bundle)),
            Err(LocalCodingSnapshotError::Rejected)
        ));
        assert_eq!(fs::read_dir(&runtime.root).unwrap().count(), 0);
    }

    #[test]
    fn cancellation_during_verification_waits_until_cleanup_is_complete() {
        let directory = tempdir().unwrap();
        let runtime = Arc::new(LocalCodingSnapshotRuntime::open(directory.path(), true).unwrap());
        let job = job();
        runtime.admit(job.clone()).unwrap();
        let staging = {
            let mut active = runtime.active.lock().unwrap();
            let active = active.as_mut().unwrap();
            active.verifying = true;
            active.staging_directory.clone()
        };
        let instruction = cancellation(&job);
        let (sender, receiver) = mpsc::channel();
        let cancelling_runtime = Arc::clone(&runtime);
        let cancellation_thread = thread::spawn(move || {
            sender
                .send(cancelling_runtime.cancel(&instruction))
                .unwrap();
        });
        let mut requested = false;
        for _ in 0..100 {
            let was_requested = runtime
                .active
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .cancellation_requested
                .load(Ordering::Acquire);
            if was_requested {
                requested = true;
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert!(requested);
        assert!(receiver.recv_timeout(Duration::from_millis(25)).is_err());
        cleanup_tree(&staging).unwrap();
        runtime.active.lock().unwrap().take();
        runtime.state_changed.notify_all();
        assert_eq!(
            receiver
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
                .unwrap()
                .status,
            CancellationAcknowledgementStatus::Cancelled
        );
        cancellation_thread.join().unwrap();
        assert!(!staging.exists());
    }

    #[test]
    fn cancellation_during_verification_never_acknowledges_failed_cleanup() {
        let directory = tempdir().unwrap();
        let runtime = Arc::new(LocalCodingSnapshotRuntime::open(directory.path(), true).unwrap());
        let job = job();
        runtime.admit(job.clone()).unwrap();
        let staging = {
            let mut active = runtime.active.lock().unwrap();
            let active = active.as_mut().unwrap();
            active.verifying = true;
            active.staging_directory.clone()
        };
        fs::set_permissions(&staging, fs::Permissions::from_mode(0o000)).unwrap();
        let instruction = cancellation(&job);
        let (sender, receiver) = mpsc::channel();
        let cancelling_runtime = Arc::clone(&runtime);
        let cancellation_thread = thread::spawn(move || {
            sender
                .send(cancelling_runtime.cancel(&instruction))
                .unwrap();
        });
        for _ in 0..100 {
            if runtime
                .active
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .cancellation_requested
                .load(Ordering::Acquire)
            {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        {
            let mut active = runtime.active.lock().unwrap();
            active.as_mut().unwrap().verifying = false;
            runtime.state_changed.notify_all();
        }
        let failure = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(matches!(failure, Err(LocalCodingSnapshotError::Io(_))));
        cancellation_thread.join().unwrap();
        assert!(staging.exists());
        assert!(runtime.active.lock().unwrap().is_some());

        fs::set_permissions(&staging, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            runtime.cancel(&cancellation(&job)).unwrap().status,
            CancellationAcknowledgementStatus::Cancelled
        );
        assert!(!staging.exists());
    }

    #[test]
    fn terminal_failure_and_shutdown_preserve_state_until_cleanup_succeeds() {
        let directory = tempdir().unwrap();
        let runtime = LocalCodingSnapshotRuntime::open(directory.path(), true).unwrap();
        let job = job();
        runtime.admit(job.clone()).unwrap();
        let staging = runtime
            .active
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .staging_directory
            .clone();
        fs::set_permissions(&staging, fs::Permissions::from_mode(0o000)).unwrap();
        let invalid_bundle = [0_u8];
        assert!(matches!(
            runtime.accept_chunk(chunk(&job, 0, 1, &invalid_bundle)),
            Err(LocalCodingSnapshotError::Io(_))
        ));
        assert!(runtime.active.lock().unwrap().is_some());
        assert!(staging.exists());
        assert!(matches!(
            runtime.shutdown(),
            Err(LocalCodingSnapshotError::Io(_))
        ));
        assert!(runtime.active.lock().unwrap().is_some());

        fs::set_permissions(&staging, fs::Permissions::from_mode(0o700)).unwrap();
        runtime.shutdown().unwrap();
        assert!(runtime.active.lock().unwrap().is_none());
        assert!(!staging.exists());
    }
}
