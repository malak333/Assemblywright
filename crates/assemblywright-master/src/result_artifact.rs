use assemblywright_protocol::{
    validate_historical_local_coding_v4_fixture_patch_artifact,
    validate_local_coding_fixture_patch_artifact, MAX_LOCAL_CODING_RESULT_ARTIFACT_BYTES,
};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

const ROOT_NAME: &str = "feature-result-artifacts";
const ARTIFACT_NAME: &str = "artifact.patch";

#[cfg(test)]
type CleanupTestHook = Option<(Arc<std::sync::Barrier>, Arc<std::sync::Barrier>)>;

#[derive(Debug, thiserror::Error)]
pub enum ResultArtifactStoreError {
    #[error("result artifact filesystem operation failed")]
    Io(#[from] std::io::Error),
    #[error("result artifact storage state was rejected")]
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResultArtifactReference {
    pub artifact_id: Uuid,
    pub artifact_sha256: [u8; 32],
    pub artifact_size_bytes: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ArtifactIdentity {
    directory: PlatformFileIdentity,
    file: PlatformFileIdentity,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PlatformFileIdentity {
    #[cfg(unix)]
    Unix { device: u64, inode: u64 },
    #[cfg(windows)]
    Windows {
        volume: u32,
        index_high: u32,
        index_low: u32,
    },
    #[cfg(not(any(unix, windows)))]
    Unsupported,
}

#[derive(Clone)]
pub struct ResultArtifactStore {
    root: PathBuf,
    active_preparations: Arc<Mutex<HashMap<Uuid, PreparationState>>>,
    #[cfg(test)]
    cleanup_test_hook: Arc<Mutex<CleanupTestHook>>,
}

#[derive(Debug, Default)]
struct PreparationState {
    count: usize,
    committed: bool,
}

pub struct VerifiedResultArtifact {
    reference: ResultArtifactReference,
    identity: ArtifactIdentity,
    file: File,
    _directory_handles: Vec<File>,
}

impl VerifiedResultArtifact {
    pub fn reference(&self) -> ResultArtifactReference {
        self.reference
    }

    /// Re-hashes the already-open no-follow handle and proves that the fixed
    /// canonical path still resolves to the same directory and file identity.
    /// The handles remain live while the caller commits authoritative result
    /// state, preventing path substitution from changing the verified bytes.
    pub fn revalidate(
        &mut self,
        store: &ResultArtifactStore,
    ) -> Result<(), ResultArtifactStoreError> {
        verify_open_file(&mut self.file, self.reference)?;
        let current = store.open_verified(self.reference)?;
        if current.identity != self.identity {
            return Err(ResultArtifactStoreError::Rejected);
        }
        Ok(())
    }

    /// Reads the exact bytes from the already-open stable handle after a full
    /// re-hash and canonical-path identity check. The guard remains live so a
    /// caller can retain it across a later authoritative transaction.
    pub fn read_revalidated(
        &mut self,
        store: &ResultArtifactStore,
    ) -> Result<Vec<u8>, ResultArtifactStoreError> {
        self.revalidate(store)?;
        self.file.seek(SeekFrom::Start(0))?;
        let mut bytes = Vec::with_capacity(self.reference.artifact_size_bytes as usize);
        self.file.read_to_end(&mut bytes)?;
        verify_exact_bytes(&mut self.file, &bytes)?;
        Ok(bytes)
    }
}

pub struct PreparedResultArtifact {
    store: ResultArtifactStore,
    artifact_id: Uuid,
    verified: Option<VerifiedResultArtifact>,
    active: bool,
}

impl PreparedResultArtifact {
    pub fn verified_mut(&mut self) -> &mut VerifiedResultArtifact {
        self.verified
            .as_mut()
            .expect("active preparation retains verified handles")
    }

    pub fn mark_committed(&mut self) -> Result<(), ResultArtifactStoreError> {
        let mut active = self
            .store
            .active_preparations
            .lock()
            .map_err(|_| ResultArtifactStoreError::Rejected)?;
        let state = active
            .get_mut(&self.artifact_id)
            .ok_or(ResultArtifactStoreError::Rejected)?;
        state.committed = true;
        Ok(())
    }

    /// Cleanup is permitted only for the last in-process preparation and only
    /// while the canonical path still has the identity this request prepared
    /// or recovered. A concurrent exact retry therefore cannot lose committed
    /// evidence to another request's failure cleanup.
    pub fn cleanup_if_unreferenced(
        mut self,
        referenced: bool,
    ) -> Result<(), ResultArtifactStoreError> {
        if referenced {
            self.release();
            return Ok(());
        }
        let verified = self
            .verified
            .take()
            .ok_or(ResultArtifactStoreError::Rejected)?;
        let reference = verified.reference;
        let identity = verified.identity;
        // In particular on Windows, the verified leaf is opened without
        // FILE_SHARE_DELETE. Close the original leaf and directory handles
        // before deletion while the preparation mutex still excludes a retry.
        drop(verified);
        let artifact_id = self.artifact_id;
        // Keep exclusion through the identity-checked unlink. `prepare` must
        // acquire this same mutex before it can inspect or open the canonical
        // path, so it cannot acquire a soon-to-be-unlinked handle between the
        // last-user decision and removal.
        let mut active = self
            .store
            .active_preparations
            .lock()
            .map_err(|_| ResultArtifactStoreError::Rejected)?;
        let can_remove = active
            .get(&artifact_id)
            .is_some_and(|state| state.count == 1 && !state.committed);
        if can_remove {
            #[cfg(test)]
            if let Some((entered, resume)) = self
                .store
                .cleanup_test_hook
                .lock()
                .map_err(|_| ResultArtifactStoreError::Rejected)?
                .clone()
            {
                entered.wait();
                resume.wait();
            }
            self.store.remove_matching(&reference, identity)?;
        }
        let remove_state = if let Some(state) = active.get_mut(&artifact_id) {
            state.count -= 1;
            state.count == 0
        } else {
            return Err(ResultArtifactStoreError::Rejected);
        };
        if remove_state {
            active.remove(&artifact_id);
        }
        self.active = false;
        Ok(())
    }

    fn release(&mut self) {
        if !self.active {
            return;
        }
        if let Ok(mut active) = self.store.active_preparations.lock() {
            if let Some(state) = active.get_mut(&self.artifact_id) {
                state.count -= 1;
                if state.count == 0 {
                    active.remove(&self.artifact_id);
                }
            }
        }
        self.active = false;
    }
}

impl Drop for PreparedResultArtifact {
    fn drop(&mut self) {
        self.release();
    }
}

impl ResultArtifactStore {
    pub fn open(data_dir: &Path) -> Result<Self, ResultArtifactStoreError> {
        let root = data_dir.join(ROOT_NAME);
        ensure_owner_private_directory(&root)?;
        Ok(Self {
            root,
            active_preparations: Arc::new(Mutex::new(HashMap::new())),
            #[cfg(test)]
            cleanup_test_hook: Arc::new(Mutex::new(None)),
        })
    }

    /// Writes outside SQLite. An exact preexisting final artifact is a
    /// recoverable crash/concurrent retry; every mismatch fails without
    /// replacing bytes. The returned guard coordinates cleanup ownership.
    pub fn prepare(
        &self,
        artifact_id: Uuid,
        expected_sha256: [u8; 32],
        bytes: &[u8],
    ) -> Result<PreparedResultArtifact, ResultArtifactStoreError> {
        let reference = ResultArtifactReference {
            artifact_id,
            artifact_sha256: expected_sha256,
            artifact_size_bytes: bytes.len() as u64,
        };
        validate_input(reference, bytes)?;
        {
            let mut active = self
                .active_preparations
                .lock()
                .map_err(|_| ResultArtifactStoreError::Rejected)?;
            active.entry(artifact_id).or_default().count += 1;
        }
        let prepared = (|| {
            let final_directory = self.root.join(artifact_id.to_string());
            if path_entry_exists(&final_directory)? {
                let mut verified = self.open_verified(reference)?;
                verify_exact_bytes(&mut verified.file, bytes)?;
                return Ok(verified);
            }

            let staging = self
                .root
                .join(format!(".{}.{}", artifact_id, Uuid::new_v4()));
            create_owner_private_directory(&staging)?;
            let result = (|| {
                let path = staging.join(ARTIFACT_NAME);
                let mut file = create_owner_private_file(&path)?;
                file.write_all(bytes)?;
                file.sync_all()?;
                // Windows denies renaming a directory while this child handle
                // is open without FILE_SHARE_DELETE. Close only after the
                // durable file flush, then perform the same-volume directory
                // rename and reopen through the no-reparse verifier.
                drop(file);
                sync_directory(&staging)?;
                match fs::rename(&staging, &final_directory) {
                    Ok(()) => sync_directory(&self.root)?,
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::AlreadyExists
                                | std::io::ErrorKind::DirectoryNotEmpty
                        ) =>
                    {
                        remove_unreferenced_tree(&staging)?;
                    }
                    Err(error) => return Err(ResultArtifactStoreError::Io(error)),
                }
                let mut verified = self.open_verified(reference)?;
                verify_exact_bytes(&mut verified.file, bytes)?;
                Ok(verified)
            })();
            if result.is_err() && path_entry_exists(&staging).unwrap_or(false) {
                let _ = remove_unreferenced_tree(&staging);
            }
            result
        })();
        match prepared {
            Ok(verified) => Ok(PreparedResultArtifact {
                store: self.clone(),
                artifact_id,
                verified: Some(verified),
                active: true,
            }),
            Err(error) => {
                self.release_preparation(artifact_id);
                Err(error)
            }
        }
    }

    pub fn open_verified(
        &self,
        reference: ResultArtifactReference,
    ) -> Result<VerifiedResultArtifact, ResultArtifactStoreError> {
        if reference.artifact_id.is_nil()
            || reference.artifact_size_bytes == 0
            || reference.artifact_size_bytes > MAX_LOCAL_CODING_RESULT_ARTIFACT_BYTES as u64
        {
            return Err(ResultArtifactStoreError::Rejected);
        }
        open_verified_platform(&self.root, reference, false)
    }

    pub fn verify_referenced(
        &self,
        referenced: &[ResultArtifactReference],
    ) -> Result<(), ResultArtifactStoreError> {
        for reference in referenced {
            // Schema-v12 rows may reference the immutable protocol-v4 fixture
            // shape. Preserve that historical evidence at startup, but keep
            // every new prepare/revalidate path strict to the v5 format.
            open_verified_platform(&self.root, *reference, true)?;
        }
        Ok(())
    }

    pub fn cleanup_unreferenced(
        &self,
        referenced: &HashSet<Uuid>,
    ) -> Result<(), ResultArtifactStoreError> {
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| ResultArtifactStoreError::Rejected)?;
            let retained = Uuid::parse_str(&name)
                .ok()
                .is_some_and(|id| referenced.contains(&id));
            if !retained {
                remove_unreferenced_tree(&entry.path())?;
            }
        }
        sync_directory(&self.root)?;
        Ok(())
    }

    fn remove_matching(
        &self,
        reference: &ResultArtifactReference,
        identity: ArtifactIdentity,
    ) -> Result<(), ResultArtifactStoreError> {
        let current = self.open_verified(*reference)?;
        if current.identity != identity {
            return Err(ResultArtifactStoreError::Rejected);
        }
        drop(current);
        remove_unreferenced_tree(&self.root.join(reference.artifact_id.to_string()))?;
        sync_directory(&self.root)?;
        Ok(())
    }

    fn release_preparation(&self, artifact_id: Uuid) {
        if let Ok(mut active) = self.active_preparations.lock() {
            if let Some(state) = active.get_mut(&artifact_id) {
                state.count -= 1;
                if state.count == 0 {
                    active.remove(&artifact_id);
                }
            }
        }
    }
}

fn validate_input(
    reference: ResultArtifactReference,
    bytes: &[u8],
) -> Result<(), ResultArtifactStoreError> {
    if bytes.is_empty()
        || bytes.len() > MAX_LOCAL_CODING_RESULT_ARTIFACT_BYTES
        || validate_local_coding_fixture_patch_artifact(bytes).ok()
            != Some(reference.artifact_sha256)
    {
        return Err(ResultArtifactStoreError::Rejected);
    }
    Ok(())
}

fn verify_open_file(
    file: &mut File,
    reference: ResultArtifactReference,
) -> Result<(), ResultArtifactStoreError> {
    verify_open_file_with_compatibility(file, reference, false)
}

fn verify_open_file_with_compatibility(
    file: &mut File,
    reference: ResultArtifactReference,
    allow_historical_v4: bool,
) -> Result<(), ResultArtifactStoreError> {
    let metadata = file.metadata()?;
    validate_plain_file_metadata(&metadata)?;
    if metadata.len() != reference.artifact_size_bytes {
        return Err(ResultArtifactStoreError::Rejected);
    }
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::with_capacity(reference.artifact_size_bytes as usize);
    file.take(reference.artifact_size_bytes + 1)
        .read_to_end(&mut bytes)?;
    let valid_format = validate_local_coding_fixture_patch_artifact(&bytes).is_ok()
        || (allow_historical_v4
            && validate_historical_local_coding_v4_fixture_patch_artifact(&bytes).is_ok());
    if bytes.len() as u64 != reference.artifact_size_bytes
        || Sha256::digest(&bytes).as_slice() != reference.artifact_sha256
        || !valid_format
    {
        return Err(ResultArtifactStoreError::Rejected);
    }
    file.seek(SeekFrom::Start(0))?;
    Ok(())
}

fn verify_exact_bytes(file: &mut File, expected: &[u8]) -> Result<(), ResultArtifactStoreError> {
    file.seek(SeekFrom::Start(0))?;
    let mut bytes = Vec::with_capacity(expected.len());
    file.take(expected.len() as u64 + 1)
        .read_to_end(&mut bytes)?;
    file.seek(SeekFrom::Start(0))?;
    if bytes != expected {
        return Err(ResultArtifactStoreError::Rejected);
    }
    Ok(())
}

fn ensure_owner_private_directory(path: &Path) -> Result<(), ResultArtifactStoreError> {
    if !path_entry_exists(path)? {
        create_owner_private_directory(path)?;
    }
    validate_plain_directory_metadata(&fs::symlink_metadata(path)?)?;
    Ok(())
}

fn path_entry_exists(path: &Path) -> Result<bool, ResultArtifactStoreError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(ResultArtifactStoreError::Io(error)),
    }
}

#[cfg(unix)]
fn create_owner_private_directory(path: &Path) -> Result<(), ResultArtifactStoreError> {
    use std::os::unix::fs::DirBuilderExt;
    let mut builder = fs::DirBuilder::new();
    builder.mode(0o700).create(path)?;
    validate_plain_directory_metadata(&fs::symlink_metadata(path)?)
}

#[cfg(not(unix))]
fn create_owner_private_directory(path: &Path) -> Result<(), ResultArtifactStoreError> {
    fs::create_dir(path)?;
    validate_plain_directory_metadata(&fs::symlink_metadata(path)?)
}

#[cfg(unix)]
fn create_owner_private_file(path: &Path) -> Result<File, ResultArtifactStoreError> {
    use std::os::unix::fs::OpenOptionsExt;
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)?;
    validate_plain_file_metadata(&file.metadata()?)?;
    Ok(file)
}

#[cfg(windows)]
fn create_owner_private_file(path: &Path) -> Result<File, ResultArtifactStoreError> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ};
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    validate_plain_file_metadata(&file.metadata()?)?;
    Ok(file)
}

#[cfg(not(any(unix, windows)))]
fn create_owner_private_file(path: &Path) -> Result<File, ResultArtifactStoreError> {
    let file = OpenOptions::new().write(true).create_new(true).open(path)?;
    validate_plain_file_metadata(&file.metadata()?)?;
    Ok(file)
}

#[cfg(unix)]
fn open_verified_platform(
    root: &Path,
    reference: ResultArtifactReference,
    allow_historical_v4: bool,
) -> Result<VerifiedResultArtifact, ResultArtifactStoreError> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::fs::OpenOptionsExt;

    let root_handle = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(root)?;
    validate_plain_directory_metadata(&root_handle.metadata()?)?;
    let name = CString::new(reference.artifact_id.to_string())
        .map_err(|_| ResultArtifactStoreError::Rejected)?;
    let directory_descriptor = unsafe {
        libc::openat(
            root_handle.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if directory_descriptor < 0 {
        return Err(ResultArtifactStoreError::Io(std::io::Error::last_os_error()));
    }
    let directory_handle = unsafe { File::from_raw_fd(directory_descriptor) };
    let directory_metadata = directory_handle.metadata()?;
    validate_plain_directory_metadata(&directory_metadata)?;
    validate_exact_directory_shape(&root.join(reference.artifact_id.to_string()))?;

    let file_name = CString::new(ARTIFACT_NAME).expect("fixed artifact name");
    let file_descriptor = unsafe {
        libc::openat(
            directory_handle.as_raw_fd(),
            file_name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if file_descriptor < 0 {
        return Err(ResultArtifactStoreError::Io(std::io::Error::last_os_error()));
    }
    let mut file = unsafe { File::from_raw_fd(file_descriptor) };
    verify_open_file_with_compatibility(&mut file, reference, allow_historical_v4)?;
    let identity = ArtifactIdentity {
        directory: platform_identity(&directory_metadata)?,
        file: platform_identity(&file.metadata()?)?,
    };
    Ok(VerifiedResultArtifact {
        reference,
        identity,
        file,
        _directory_handles: vec![root_handle, directory_handle],
    })
}

#[cfg(windows)]
fn open_verified_platform(
    root: &Path,
    reference: ResultArtifactReference,
    allow_historical_v4: bool,
) -> Result<VerifiedResultArtifact, ResultArtifactStoreError> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    let directory = root.join(reference.artifact_id.to_string());
    let mut directory_handles = Vec::with_capacity(2);
    for path in [root.to_path_buf(), directory.clone()] {
        let handle = OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
            .open(path)?;
        validate_windows_directory_handle(&handle)?;
        directory_handles.push(handle);
    }
    validate_exact_directory_shape(&directory)?;
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(directory.join(ARTIFACT_NAME))?;
    validate_windows_file_handle(&file)?;
    verify_open_file_with_compatibility(&mut file, reference, allow_historical_v4)?;
    let identity = ArtifactIdentity {
        directory: windows_handle_identity(
            directory_handles
                .last()
                .ok_or(ResultArtifactStoreError::Rejected)?,
        )?,
        file: windows_handle_identity(&file)?,
    };
    Ok(VerifiedResultArtifact {
        reference,
        identity,
        file,
        _directory_handles: directory_handles,
    })
}

#[cfg(not(any(unix, windows)))]
fn open_verified_platform(
    root: &Path,
    reference: ResultArtifactReference,
    allow_historical_v4: bool,
) -> Result<VerifiedResultArtifact, ResultArtifactStoreError> {
    let directory = root.join(reference.artifact_id.to_string());
    validate_plain_directory_metadata(&fs::symlink_metadata(&directory)?)?;
    validate_exact_directory_shape(&directory)?;
    let mut file = File::open(directory.join(ARTIFACT_NAME))?;
    verify_open_file_with_compatibility(&mut file, reference, allow_historical_v4)?;
    Ok(VerifiedResultArtifact {
        reference,
        identity: ArtifactIdentity {
            directory: PlatformFileIdentity::Unsupported,
            file: PlatformFileIdentity::Unsupported,
        },
        file,
        _directory_handles: Vec::new(),
    })
}

fn validate_exact_directory_shape(path: &Path) -> Result<(), ResultArtifactStoreError> {
    let mut entries = fs::read_dir(path)?;
    let entry = entries.next().ok_or(ResultArtifactStoreError::Rejected)??;
    if entry.file_name() != ARTIFACT_NAME || entries.next().is_some() {
        return Err(ResultArtifactStoreError::Rejected);
    }
    Ok(())
}

fn remove_unreferenced_tree(path: &Path) -> Result<(), ResultArtifactStoreError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata_is_link_or_reparse(&metadata) {
        if metadata.is_dir() {
            fs::remove_dir(path)?;
        } else {
            fs::remove_file(path)?;
        }
        return Ok(());
    }
    validate_plain_directory_metadata(&metadata)?;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_name() != ARTIFACT_NAME {
            return Err(ResultArtifactStoreError::Rejected);
        }
        let child_metadata = fs::symlink_metadata(entry.path())?;
        if metadata_is_link_or_reparse(&child_metadata) || !child_metadata.is_file() {
            return Err(ResultArtifactStoreError::Rejected);
        }
        validate_plain_file_metadata(&child_metadata)?;
        fs::remove_file(entry.path())?;
    }
    fs::remove_dir(path)?;
    Ok(())
}

#[cfg(unix)]
fn validate_plain_directory_metadata(
    metadata: &fs::Metadata,
) -> Result<(), ResultArtifactStoreError> {
    use std::os::unix::fs::MetadataExt;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err(ResultArtifactStoreError::Rejected);
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_plain_directory_metadata(
    metadata: &fs::Metadata,
) -> Result<(), ResultArtifactStoreError> {
    if !metadata.is_dir() || metadata_is_link_or_reparse(metadata) {
        return Err(ResultArtifactStoreError::Rejected);
    }
    Ok(())
}

#[cfg(unix)]
fn validate_plain_file_metadata(metadata: &fs::Metadata) -> Result<(), ResultArtifactStoreError> {
    use std::os::unix::fs::MetadataExt;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.nlink() != 1
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err(ResultArtifactStoreError::Rejected);
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_plain_file_metadata(metadata: &fs::Metadata) -> Result<(), ResultArtifactStoreError> {
    if !metadata.is_file() || metadata_is_link_or_reparse(metadata) {
        return Err(ResultArtifactStoreError::Rejected);
    }
    Ok(())
}

#[cfg(unix)]
fn platform_identity(
    metadata: &fs::Metadata,
) -> Result<PlatformFileIdentity, ResultArtifactStoreError> {
    use std::os::unix::fs::MetadataExt;
    Ok(PlatformFileIdentity::Unix {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(windows)]
fn windows_handle_information(
    file: &File,
) -> Result<
    windows_sys::Win32::Storage::FileSystem::BY_HANDLE_FILE_INFORMATION,
    ResultArtifactStoreError,
> {
    use std::mem::MaybeUninit;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };
    let mut information = MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::zeroed();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle() as _, information.as_mut_ptr()) }
        == 0
    {
        return Err(ResultArtifactStoreError::Io(std::io::Error::last_os_error()));
    }
    Ok(unsafe { information.assume_init() })
}

#[cfg(windows)]
fn windows_handle_identity(file: &File) -> Result<PlatformFileIdentity, ResultArtifactStoreError> {
    let information = windows_handle_information(file)?;
    Ok(PlatformFileIdentity::Windows {
        volume: information.dwVolumeSerialNumber,
        index_high: information.nFileIndexHigh,
        index_low: information.nFileIndexLow,
    })
}

#[cfg(windows)]
fn validate_windows_directory_handle(file: &File) -> Result<(), ResultArtifactStoreError> {
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    };
    let information = windows_handle_information(file)?;
    if information.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY == 0
        || information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(ResultArtifactStoreError::Rejected);
    }
    Ok(())
}

#[cfg(windows)]
fn validate_windows_file_handle(file: &File) -> Result<(), ResultArtifactStoreError> {
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    };
    let information = windows_handle_information(file)?;
    if information.dwFileAttributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) != 0
        || information.nNumberOfLinks != 1
    {
        return Err(ResultArtifactStoreError::Rejected);
    }
    Ok(())
}

#[cfg(not(windows))]
fn sync_directory(path: &Path) -> Result<(), ResultArtifactStoreError> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(windows)]
fn sync_directory(_path: &Path) -> Result<(), ResultArtifactStoreError> {
    // Rust/Win32 does not expose a portable directory FlushFileBuffers
    // contract. Artifact file bytes are flushed before the same-volume rename;
    // live Windows crash durability remains an explicit release-proof item.
    Ok(())
}

#[cfg(unix)]
fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(windows)]
fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(any(unix, windows)))]
fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(test)]
mod tests {
    use super::*;
    use assemblywright_protocol::{
        build_local_coding_fixture_patch_artifact, LocalCodingResultArtifact,
        LOCAL_CODING_FIXTURE_CONTENT,
    };
    use serde::Serialize;
    use std::sync::{mpsc, Barrier};
    use std::time::Duration;
    use tempfile::tempdir;

    #[derive(Serialize)]
    struct HistoricalV4Artifact<'a> {
        format: &'a str,
        path: &'a str,
        expected_before_sha256: [u8; 32],
        replacement_sha256: [u8; 32],
        replacement_hex: String,
    }

    fn historical_v4_artifact_bytes() -> Vec<u8> {
        serde_json::to_vec(&HistoricalV4Artifact {
            format: "assemblywright.readme-replacement.v1",
            path: "README.md",
            expected_before_sha256: [0x42; 32],
            replacement_sha256: Sha256::digest(LOCAL_CODING_FIXTURE_CONTENT).into(),
            replacement_hex: LOCAL_CODING_FIXTURE_CONTENT
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        })
        .unwrap()
    }

    #[test]
    fn startup_preserves_historical_v4_artifact_but_new_paths_reject_it() {
        let directory = tempdir().unwrap();
        let store = ResultArtifactStore::open(directory.path()).unwrap();
        let artifact_id = Uuid::new_v4();
        let artifact_directory = directory
            .path()
            .join(ROOT_NAME)
            .join(artifact_id.to_string());
        create_owner_private_directory(&artifact_directory).unwrap();
        let bytes = historical_v4_artifact_bytes();
        let mut file = create_owner_private_file(&artifact_directory.join(ARTIFACT_NAME)).unwrap();
        file.write_all(&bytes).unwrap();
        file.sync_all().unwrap();
        drop(file);
        let reference = ResultArtifactReference {
            artifact_id,
            artifact_sha256: Sha256::digest(&bytes).into(),
            artifact_size_bytes: bytes.len() as u64,
        };

        store.verify_referenced(&[reference]).unwrap();
        assert!(matches!(
            store.open_verified(reference),
            Err(ResultArtifactStoreError::Rejected)
        ));
        assert!(matches!(
            store.prepare(artifact_id, reference.artifact_sha256, &bytes),
            Err(ResultArtifactStoreError::Rejected)
        ));
    }

    #[test]
    fn cleanup_excludes_prepare_until_identity_checked_unlink_finishes() {
        let directory = tempdir().unwrap();
        let store = ResultArtifactStore::open(directory.path()).unwrap();
        let bytes = build_local_coding_fixture_patch_artifact([0x55; 32]).unwrap();
        let artifact = LocalCodingResultArtifact::from_bytes(Uuid::new_v4(), &bytes).unwrap();
        let cleanup = store
            .prepare(artifact.artifact_id, artifact.artifact_sha256, &bytes)
            .unwrap();

        let cleanup_entered = Arc::new(Barrier::new(2));
        let cleanup_resume = Arc::new(Barrier::new(2));
        *store.cleanup_test_hook.lock().unwrap() =
            Some((cleanup_entered.clone(), cleanup_resume.clone()));
        let cleanup_thread = std::thread::spawn(move || {
            cleanup.cleanup_if_unreferenced(false).unwrap();
        });
        cleanup_entered.wait();

        let (started_tx, started_rx) = mpsc::channel();
        let (prepared_tx, prepared_rx) = mpsc::channel();
        let retry_store = store.clone();
        let retry_bytes = bytes.clone();
        let retry_thread = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            let retry = retry_store
                .prepare(artifact.artifact_id, artifact.artifact_sha256, &retry_bytes)
                .unwrap();
            prepared_tx.send(retry).unwrap();
        });
        started_rx.recv().unwrap();
        assert!(prepared_rx.recv_timeout(Duration::from_millis(50)).is_err());

        cleanup_resume.wait();
        cleanup_thread.join().unwrap();
        let mut retry = prepared_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        retry.verified_mut().revalidate(&store).unwrap();
        retry_thread.join().unwrap();
        assert!(directory
            .path()
            .join(ROOT_NAME)
            .join(artifact.artifact_id.to_string())
            .join(ARTIFACT_NAME)
            .exists());
        *store.cleanup_test_hook.lock().unwrap() = None;
        retry.cleanup_if_unreferenced(false).unwrap();
        assert!(!directory
            .path()
            .join(ROOT_NAME)
            .join(artifact.artifact_id.to_string())
            .exists());
    }

    #[cfg(windows)]
    #[test]
    fn windows_prepare_closes_staging_file_before_rename_and_exact_retry() {
        let directory = tempdir().unwrap();
        let store = ResultArtifactStore::open(directory.path()).unwrap();
        let bytes = build_local_coding_fixture_patch_artifact([0x56; 32]).unwrap();
        let artifact = LocalCodingResultArtifact::from_bytes(Uuid::new_v4(), &bytes).unwrap();

        let mut first = store
            .prepare(artifact.artifact_id, artifact.artifact_sha256, &bytes)
            .unwrap();
        first.verified_mut().revalidate(&store).unwrap();
        let mut retry = store
            .prepare(artifact.artifact_id, artifact.artifact_sha256, &bytes)
            .unwrap();
        retry.verified_mut().revalidate(&store).unwrap();
        first.cleanup_if_unreferenced(false).unwrap();
        assert!(directory
            .path()
            .join(ROOT_NAME)
            .join(artifact.artifact_id.to_string())
            .join(ARTIFACT_NAME)
            .is_file());
        retry.cleanup_if_unreferenced(false).unwrap();
        assert!(!directory
            .path()
            .join(ROOT_NAME)
            .join(artifact.artifact_id.to_string())
            .exists());
    }
}
