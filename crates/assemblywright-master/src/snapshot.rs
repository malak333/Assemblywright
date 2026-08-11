use git2::{ObjectType, Odb, Oid, Repository};
use sha2::{Digest, Sha256};
use std::collections::{HashSet, VecDeque};
#[cfg(not(windows))]
use std::fs::File;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;

const MAX_SNAPSHOT_OBJECTS: usize = 50_000;
const MAX_SNAPSHOT_OBJECT_BYTES: u64 = 256 * 1024 * 1024;
const MAX_SNAPSHOT_FILE_BYTES: usize = 32 * 1024 * 1024;
const MAX_SNAPSHOT_PATH_BYTES: usize = 1024;

#[derive(Debug, thiserror::Error)]
pub enum RepositorySnapshotError {
    #[error("repository snapshot filesystem operation failed")]
    Io(#[from] std::io::Error),
    #[error("repository snapshot object operation failed")]
    Git(#[from] git2::Error),
    #[error("repository snapshot input or object graph is unsupported")]
    Rejected,
}

#[derive(Debug)]
pub struct PreparedRepositorySnapshot {
    pub snapshot_id: Uuid,
    pub snapshot_sha256: [u8; 32],
    pub base_commit: String,
    path: Option<PathBuf>,
}

impl PreparedRepositorySnapshot {
    pub fn retain(mut self) {
        self.path = None;
    }
}

impl Drop for PreparedRepositorySnapshot {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = remove_snapshot_tree(&path);
        }
    }
}

#[derive(Debug, Clone)]
pub struct RepositorySnapshotStore {
    root: PathBuf,
}

impl RepositorySnapshotStore {
    pub fn open(data_dir: &Path) -> Result<Self, RepositorySnapshotError> {
        let root = data_dir.join("feature-conveyor-repository-snapshots");
        ensure_plain_directory(&root)?;
        ensure_plain_directory(&root.join("staging"))?;
        ensure_plain_directory(&root.join("snapshots"))?;
        Ok(Self { root })
    }

    pub fn prepare(
        &self,
        source_repository: &Path,
        expected_commit: &str,
    ) -> Result<PreparedRepositorySnapshot, RepositorySnapshotError> {
        if expected_commit.len() != 40
            || !expected_commit
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(RepositorySnapshotError::Rejected);
        }
        validate_source_object_store(source_repository)?;
        let snapshot_id = Uuid::new_v4();
        let snapshot_path = self.root.join("snapshots").join(snapshot_id.to_string());
        fs::create_dir(&snapshot_path)?;
        let mut cleanup = SnapshotCleanup(Some(snapshot_path.clone()));
        let repository = Repository::init(&snapshot_path)?;
        let source_odb = Odb::new()?;
        source_odb.add_disk_alternate(
            source_repository
                .join(".git")
                .join("objects")
                .to_str()
                .ok_or(RepositorySnapshotError::Rejected)?,
        )?;
        let destination_odb = repository.odb()?;
        let commit_oid = Oid::from_str(expected_commit)?;
        let tree_oid = copy_current_commit_objects(&source_odb, &destination_odb, commit_oid)?;
        let mut digest = Sha256::new();
        digest.update(b"assemblywright.repository-snapshot.v1\0");
        digest.update(commit_oid.as_bytes());
        let mut seen_paths = HashSet::new();
        materialize_tree(
            &source_odb,
            tree_oid,
            &snapshot_path,
            Path::new(""),
            &mut digest,
            &mut seen_paths,
        )?;
        write_snapshot_repository_metadata(&repository, expected_commit, tree_oid)?;
        // Close libgit2 handles before flushing the object files on Windows;
        // its open ODB handles do not share the write access required by
        // FlushFileBuffers. The random UUID directory remains private and
        // unreferenced until the later SQLite finalizer commits its binding.
        drop(destination_odb);
        drop(repository);
        sync_tree(&snapshot_path)?;
        cleanup.0 = None;
        sync_directory(
            snapshot_path
                .parent()
                .ok_or(RepositorySnapshotError::Rejected)?,
        )?;
        Ok(PreparedRepositorySnapshot {
            snapshot_id,
            snapshot_sha256: digest.finalize().into(),
            base_commit: expected_commit.to_string(),
            path: Some(snapshot_path),
        })
    }

    pub fn cleanup_unreferenced(
        &self,
        referenced: &HashSet<Uuid>,
    ) -> Result<(), RepositorySnapshotError> {
        cleanup_uuid_children(&self.root.join("staging"), &HashSet::new())?;
        cleanup_uuid_children(&self.root.join("snapshots"), referenced)
    }
}

struct SnapshotCleanup(Option<PathBuf>);

impl Drop for SnapshotCleanup {
    fn drop(&mut self) {
        if let Some(path) = self.0.take() {
            let _ = remove_snapshot_tree(&path);
        }
    }
}

fn copy_current_commit_objects(
    source: &Odb<'_>,
    destination: &Odb<'_>,
    base_commit: Oid,
) -> Result<Oid, RepositorySnapshotError> {
    let mut total_bytes = 0_u64;
    let commit_size = checked_object_header(source, base_commit, ObjectType::Commit)?;
    account_declared_object_size(commit_size, &mut total_bytes)?;
    let commit = source.read(base_commit)?;
    if commit.kind() != ObjectType::Commit
        || commit.data().len() != commit_size
        || destination.write(ObjectType::Commit, commit.data())? != base_commit
    {
        return Err(RepositorySnapshotError::Rejected);
    }
    let tree_oid = commit_tree_oid(commit.data())?;
    let mut pending = VecDeque::from([(tree_oid, ObjectType::Tree)]);
    let mut copied = HashSet::new();
    while let Some((oid, expected_kind)) = pending.pop_front() {
        if !copied.insert(oid) {
            continue;
        }
        if copied
            .len()
            .checked_add(1)
            .ok_or(RepositorySnapshotError::Rejected)?
            > MAX_SNAPSHOT_OBJECTS
        {
            return Err(RepositorySnapshotError::Rejected);
        }
        let declared_size = checked_object_header(source, oid, expected_kind)?;
        account_declared_object_size(declared_size, &mut total_bytes)?;
        let object = source.read(oid)?;
        if object.kind() != expected_kind
            || object.data().len() != declared_size
            || destination.write(expected_kind, object.data())? != oid
        {
            return Err(RepositorySnapshotError::Rejected);
        }
        match expected_kind {
            ObjectType::Tree => {
                for entry in parse_tree(object.data())? {
                    pending.push_back((
                        entry.oid,
                        if entry.mode == 0o040000 {
                            ObjectType::Tree
                        } else if matches!(entry.mode, 0o100644 | 0o100755) {
                            ObjectType::Blob
                        } else {
                            return Err(RepositorySnapshotError::Rejected);
                        },
                    ));
                }
            }
            ObjectType::Blob => {}
            _ => return Err(RepositorySnapshotError::Rejected),
        }
    }
    Ok(tree_oid)
}

fn checked_object_header(
    odb: &Odb<'_>,
    oid: Oid,
    expected_kind: ObjectType,
) -> Result<usize, RepositorySnapshotError> {
    let (declared_size, actual_kind) = odb.read_header(oid)?;
    validate_declared_object_header(declared_size, actual_kind, expected_kind)?;
    Ok(declared_size)
}

fn validate_declared_object_header(
    declared_size: usize,
    actual_kind: ObjectType,
    expected_kind: ObjectType,
) -> Result<(), RepositorySnapshotError> {
    if actual_kind != expected_kind
        || declared_size as u64 > MAX_SNAPSHOT_OBJECT_BYTES
        || (expected_kind == ObjectType::Blob && declared_size > MAX_SNAPSHOT_FILE_BYTES)
    {
        return Err(RepositorySnapshotError::Rejected);
    }
    Ok(())
}

fn account_declared_object_size(
    declared_size: usize,
    total_bytes: &mut u64,
) -> Result<(), RepositorySnapshotError> {
    let next = total_bytes
        .checked_add(u64::try_from(declared_size).map_err(|_| RepositorySnapshotError::Rejected)?)
        .ok_or(RepositorySnapshotError::Rejected)?;
    if next > MAX_SNAPSHOT_OBJECT_BYTES {
        return Err(RepositorySnapshotError::Rejected);
    }
    *total_bytes = next;
    Ok(())
}

fn materialize_tree(
    odb: &Odb<'_>,
    tree_oid: Oid,
    root: &Path,
    relative: &Path,
    digest: &mut Sha256,
    seen_paths: &mut HashSet<String>,
) -> Result<(), RepositorySnapshotError> {
    let declared_size = checked_object_header(odb, tree_oid, ObjectType::Tree)?;
    let object = odb.read(tree_oid)?;
    if object.kind() != ObjectType::Tree || object.data().len() != declared_size {
        return Err(RepositorySnapshotError::Rejected);
    }
    for entry in parse_tree(object.data())? {
        let name = validate_tree_name(&entry.name)?;
        let child_relative = relative.join(name);
        validate_snapshot_relative_path(&child_relative, seen_paths)?;
        let child = root.join(&child_relative);
        if entry.mode == 0o040000 {
            fs::create_dir(&child)?;
            materialize_tree(odb, entry.oid, root, &child_relative, digest, seen_paths)?;
            continue;
        }
        if !matches!(entry.mode, 0o100644 | 0o100755) {
            return Err(RepositorySnapshotError::Rejected);
        }
        let declared_size = checked_object_header(odb, entry.oid, ObjectType::Blob)?;
        let blob = odb.read(entry.oid)?;
        if blob.kind() != ObjectType::Blob || blob.data().len() != declared_size {
            return Err(RepositorySnapshotError::Rejected);
        }
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&child)?;
        file.write_all(blob.data())?;
        file.sync_all()?;
        let path = child_relative
            .to_str()
            .ok_or(RepositorySnapshotError::Rejected)?
            .replace('\\', "/");
        digest.update((path.len() as u64).to_be_bytes());
        digest.update(path.as_bytes());
        digest.update(entry.mode.to_be_bytes());
        digest.update(entry.oid.as_bytes());
        digest.update((blob.data().len() as u64).to_be_bytes());
        digest.update(blob.data());
    }
    Ok(())
}

fn write_snapshot_repository_metadata(
    repository: &Repository,
    commit: &str,
    tree_oid: Oid,
) -> Result<(), RepositorySnapshotError> {
    let git_dir = repository.path();
    fs::write(git_dir.join("HEAD"), b"ref: refs/heads/snapshot\n")?;
    // The preserved commit object still names its original parents. Marking it
    // shallow makes those intentionally absent historical objects a valid
    // repository boundary rather than an incomplete object store.
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
    let tree = repository.find_tree(tree_oid)?;
    let mut index = repository.index()?;
    index.read_tree(&tree)?;
    index.write()?;
    Ok(())
}

#[derive(Debug)]
struct RawTreeEntry {
    mode: u32,
    name: Vec<u8>,
    oid: Oid,
}

fn parse_tree(mut data: &[u8]) -> Result<Vec<RawTreeEntry>, RepositorySnapshotError> {
    let mut entries = Vec::new();
    while !data.is_empty() {
        let space = data
            .iter()
            .position(|byte| *byte == b' ')
            .ok_or(RepositorySnapshotError::Rejected)?;
        let mode =
            std::str::from_utf8(&data[..space]).map_err(|_| RepositorySnapshotError::Rejected)?;
        let mode = u32::from_str_radix(mode, 8).map_err(|_| RepositorySnapshotError::Rejected)?;
        data = &data[space + 1..];
        let nul = data
            .iter()
            .position(|byte| *byte == 0)
            .ok_or(RepositorySnapshotError::Rejected)?;
        let name = data[..nul].to_vec();
        data = &data[nul + 1..];
        if data.len() < 20 {
            return Err(RepositorySnapshotError::Rejected);
        }
        let oid = Oid::from_bytes(&data[..20])?;
        data = &data[20..];
        entries.push(RawTreeEntry { mode, name, oid });
    }
    Ok(entries)
}

fn commit_tree_oid(data: &[u8]) -> Result<Oid, RepositorySnapshotError> {
    let line = data
        .split(|byte| *byte == b'\n')
        .next()
        .ok_or(RepositorySnapshotError::Rejected)?;
    let value = line
        .strip_prefix(b"tree ")
        .ok_or(RepositorySnapshotError::Rejected)?;
    Ok(Oid::from_str(
        std::str::from_utf8(value).map_err(|_| RepositorySnapshotError::Rejected)?,
    )?)
}

fn validate_tree_name(name: &[u8]) -> Result<&str, RepositorySnapshotError> {
    let name = std::str::from_utf8(name).map_err(|_| RepositorySnapshotError::Rejected)?;
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', ':'])
        || name.ends_with([' ', '.'])
        || name.eq_ignore_ascii_case(".git")
        || is_windows_reserved_name(name)
    {
        return Err(RepositorySnapshotError::Rejected);
    }
    Ok(name)
}

fn validate_snapshot_relative_path(
    path: &Path,
    seen: &mut HashSet<String>,
) -> Result<(), RepositorySnapshotError> {
    if path.as_os_str().len() > MAX_SNAPSHOT_PATH_BYTES
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(RepositorySnapshotError::Rejected);
    }
    let normalized = path
        .to_str()
        .ok_or(RepositorySnapshotError::Rejected)?
        .replace('\\', "/")
        .to_ascii_lowercase();
    if !seen.insert(normalized) {
        return Err(RepositorySnapshotError::Rejected);
    }
    Ok(())
}

fn is_windows_reserved_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

fn validate_source_object_store(repository: &Path) -> Result<(), RepositorySnapshotError> {
    let objects = repository.join(".git").join("objects");
    ensure_plain_directory(&objects)?;
    for forbidden in ["alternates", "http-alternates"] {
        if objects.join("info").join(forbidden).exists() {
            return Err(RepositorySnapshotError::Rejected);
        }
    }
    for entry in fs::read_dir(&objects)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if metadata_is_link_or_reparse(&metadata) {
            return Err(RepositorySnapshotError::Rejected);
        }
        if metadata.is_dir() {
            for child in fs::read_dir(entry.path())? {
                let child = child?;
                let child_metadata = fs::symlink_metadata(child.path())?;
                if metadata_is_link_or_reparse(&child_metadata) || !child_metadata.is_file() {
                    return Err(RepositorySnapshotError::Rejected);
                }
            }
        } else {
            return Err(RepositorySnapshotError::Rejected);
        }
    }
    Ok(())
}

fn ensure_plain_directory(path: &Path) -> Result<(), RepositorySnapshotError> {
    if !path.exists() {
        fs::create_dir_all(path)?;
    }
    let metadata = fs::symlink_metadata(path)?;
    if metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
        return Err(RepositorySnapshotError::Rejected);
    }
    Ok(())
}

fn cleanup_uuid_children(
    directory: &Path,
    retained: &HashSet<Uuid>,
) -> Result<(), RepositorySnapshotError> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| RepositorySnapshotError::Rejected)?;
        let id = Uuid::parse_str(&name).map_err(|_| RepositorySnapshotError::Rejected)?;
        if !retained.contains(&id) {
            remove_snapshot_tree(&entry.path())?;
        }
    }
    Ok(())
}

fn remove_snapshot_tree(path: &Path) -> Result<(), RepositorySnapshotError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata_is_link_or_reparse(&metadata) {
        if metadata.is_dir() {
            fs::remove_dir(path)?;
        } else {
            fs::remove_file(path)?;
        }
    } else if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            remove_snapshot_tree(&entry?.path())?;
        }
        fs::remove_dir(path)?;
    } else {
        remove_plain_snapshot_file(path)?;
    }
    Ok(())
}

#[cfg(windows)]
fn remove_plain_snapshot_file(path: &Path) -> Result<(), RepositorySnapshotError> {
    let was_readonly = fs::metadata(path)?.permissions().readonly();
    if was_readonly {
        set_windows_readonly(path, false)?;
    }
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) => {
            if was_readonly {
                let _ = set_windows_readonly(path, true);
            }
            Err(RepositorySnapshotError::Io(error))
        }
    }
}

#[cfg(not(windows))]
fn remove_plain_snapshot_file(path: &Path) -> Result<(), RepositorySnapshotError> {
    fs::remove_file(path)?;
    Ok(())
}

fn sync_tree(path: &Path) -> Result<(), RepositorySnapshotError> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            sync_tree(&entry.path())?;
        } else {
            sync_snapshot_file(&entry.path())?;
        }
    }
    sync_directory(path)
}

#[cfg(windows)]
fn sync_snapshot_file(path: &Path) -> Result<(), RepositorySnapshotError> {
    let permissions = fs::metadata(path)?.permissions();
    let was_readonly = permissions.readonly();
    if was_readonly {
        set_windows_readonly(path, false)?;
    }
    let sync_result = OpenOptions::new()
        .write(true)
        .open(path)
        .and_then(|file| file.sync_all());
    if was_readonly {
        set_windows_readonly(path, true)?;
    }
    sync_result.map_err(RepositorySnapshotError::Io)
}

#[cfg(not(windows))]
fn sync_snapshot_file(path: &Path) -> Result<(), RepositorySnapshotError> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(windows)]
fn sync_directory(path: &Path) -> Result<(), RepositorySnapshotError> {
    let _ = path;
    Ok(())
}

#[cfg(not(windows))]
fn sync_directory(path: &Path) -> Result<(), RepositorySnapshotError> {
    File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(unix)]
fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(windows)]
fn set_windows_readonly(path: &Path, readonly: bool) -> Result<(), RepositorySnapshotError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileAttributesW, SetFileAttributesW, FILE_ATTRIBUTE_READONLY, INVALID_FILE_ATTRIBUTES,
    };

    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    // SAFETY: `wide` is a live, NUL-terminated UTF-16 buffer for both calls.
    let current = unsafe { GetFileAttributesW(wide.as_ptr()) };
    if current == INVALID_FILE_ATTRIBUTES {
        return Err(RepositorySnapshotError::Io(std::io::Error::last_os_error()));
    }
    let next = if readonly {
        current | FILE_ATTRIBUTE_READONLY
    } else {
        current & !FILE_ATTRIBUTE_READONLY
    };
    // SAFETY: the pointer remains valid and the attributes preserve every bit
    // except the explicitly controlled read-only flag.
    if unsafe { SetFileAttributesW(wide.as_ptr(), next) } == 0 {
        return Err(RepositorySnapshotError::Io(std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(windows)]
fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
    metadata.file_type().is_symlink()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use git2::{IndexAddOption, Signature};
    use tempfile::tempdir;

    fn standalone_repository(path: &Path) -> (Repository, Oid) {
        let repository = Repository::init(path).unwrap();
        fs::write(path.join("README.md"), b"bounded snapshot fixture\n").unwrap();
        fs::create_dir(path.join("src")).unwrap();
        fs::write(path.join("src").join("lib.rs"), b"pub fn bounded() {}\n").unwrap();
        let mut index = repository.index().unwrap();
        index
            .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repository.find_tree(tree_oid).unwrap();
        let signature = Signature::now("Assemblywright Test", "test@example.invalid").unwrap();
        let commit = repository
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
        (repository, commit)
    }

    fn repository_with_deleted_history(path: &Path) -> (Repository, Oid, Oid, Oid) {
        let repository = Repository::init(path).unwrap();
        fs::write(path.join("deleted-secret.txt"), b"parent-only-secret\n").unwrap();
        let mut index = repository.index().unwrap();
        index.add_path(Path::new("deleted-secret.txt")).unwrap();
        index.write().unwrap();
        let parent_tree_oid = index.write_tree().unwrap();
        let parent_tree = repository.find_tree(parent_tree_oid).unwrap();
        let secret_blob = parent_tree
            .get_path(Path::new("deleted-secret.txt"))
            .unwrap()
            .id();
        let signature = Signature::now("Assemblywright Test", "test@example.invalid").unwrap();
        let parent = repository
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                "secret parent",
                &parent_tree,
                &[],
            )
            .unwrap();
        drop(parent_tree);

        fs::remove_file(path.join("deleted-secret.txt")).unwrap();
        fs::write(path.join("public.txt"), b"current content\n").unwrap();
        index.remove_path(Path::new("deleted-secret.txt")).unwrap();
        index.add_path(Path::new("public.txt")).unwrap();
        index.write().unwrap();
        let current_tree_oid = index.write_tree().unwrap();
        let current_tree = repository.find_tree(current_tree_oid).unwrap();
        let parent_commit = repository.find_commit(parent).unwrap();
        let current = repository
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                "current content",
                &current_tree,
                &[&parent_commit],
            )
            .unwrap();
        drop(parent_commit);
        drop(current_tree);
        drop(index);
        (repository, current, parent, secret_blob)
    }

    #[test]
    fn snapshot_is_independent_filter_free_and_cleanup_is_fail_closed() {
        let source = tempdir().unwrap();
        let (_repository, commit) = standalone_repository(source.path());
        let data = tempdir().unwrap();
        let store = RepositorySnapshotStore::open(data.path()).unwrap();
        let prepared = store.prepare(source.path(), &commit.to_string()).unwrap();
        let snapshot_path = prepared.path.as_ref().unwrap().clone();
        assert_eq!(
            fs::read(snapshot_path.join("README.md")).unwrap(),
            b"bounded snapshot fixture\n"
        );
        let snapshot_repository = Repository::open(&snapshot_path).unwrap();
        assert_eq!(snapshot_repository.head().unwrap().target(), Some(commit));
        assert!(snapshot_repository.remotes().unwrap().is_empty());
        let config = fs::read_to_string(snapshot_path.join(".git").join("config")).unwrap();
        assert!(!config.contains(source.path().to_string_lossy().as_ref()));
        assert!(!config.contains("remote"));
        assert_ne!(prepared.snapshot_sha256, [0; 32]);
        drop(snapshot_repository);
        drop(prepared);
        assert!(!snapshot_path.exists());

        let retained = store.prepare(source.path(), &commit.to_string()).unwrap();
        let retained_path = retained.path.as_ref().unwrap().clone();
        retained.retain();
        assert!(retained_path.exists());
        store.cleanup_unreferenced(&HashSet::new()).unwrap();
        assert!(!retained_path.exists());

        fs::create_dir(store.root.join("staging").join(Uuid::new_v4().to_string())).unwrap();
        store.cleanup_unreferenced(&HashSet::new()).unwrap();
        assert_eq!(fs::read_dir(store.root.join("staging")).unwrap().count(), 0);
    }

    #[test]
    fn snapshot_rejects_alternates_and_symlink_entries() {
        let source = tempdir().unwrap();
        let (_repository, commit) = standalone_repository(source.path());
        let info = source.path().join(".git").join("objects").join("info");
        fs::create_dir_all(&info).unwrap();
        fs::write(info.join("alternates"), b"private-object-path").unwrap();
        let data = tempdir().unwrap();
        let store = RepositorySnapshotStore::open(data.path()).unwrap();
        assert!(matches!(
            store.prepare(source.path(), &commit.to_string()),
            Err(RepositorySnapshotError::Rejected)
        ));
        assert_eq!(fs::read_dir(store.root.join("staging")).unwrap().count(), 0);
    }

    #[test]
    fn snapshot_is_shallow_and_excludes_deleted_parent_content() {
        let source = tempdir().unwrap();
        let (_repository, current, parent, deleted_blob) =
            repository_with_deleted_history(source.path());
        let data = tempdir().unwrap();
        let store = RepositorySnapshotStore::open(data.path()).unwrap();
        let prepared = store.prepare(source.path(), &current.to_string()).unwrap();
        let snapshot_path = prepared.path.as_ref().unwrap();
        let snapshot_repository = Repository::open(snapshot_path).unwrap();
        let snapshot_odb = snapshot_repository.odb().unwrap();

        assert!(snapshot_repository.is_shallow());
        assert_eq!(snapshot_repository.head().unwrap().target(), Some(current));
        assert!(snapshot_odb.exists(current));
        assert!(!snapshot_odb.exists(parent));
        assert!(!snapshot_odb.exists(deleted_blob));
        assert!(!snapshot_path.join("deleted-secret.txt").exists());
        assert_eq!(
            fs::read(snapshot_path.join("public.txt")).unwrap(),
            b"current content\n"
        );
        let mut walk = snapshot_repository.revwalk().unwrap();
        walk.push_head().unwrap();
        assert_eq!(walk.collect::<Result<Vec<_>, _>>().unwrap(), vec![current]);
    }

    #[test]
    fn declared_object_headers_and_aggregate_budget_fail_before_read() {
        assert!(validate_declared_object_header(
            MAX_SNAPSHOT_FILE_BYTES,
            ObjectType::Blob,
            ObjectType::Blob,
        )
        .is_ok());
        assert!(matches!(
            validate_declared_object_header(
                MAX_SNAPSHOT_FILE_BYTES + 1,
                ObjectType::Blob,
                ObjectType::Blob,
            ),
            Err(RepositorySnapshotError::Rejected)
        ));
        assert!(matches!(
            validate_declared_object_header(1, ObjectType::Tree, ObjectType::Blob),
            Err(RepositorySnapshotError::Rejected)
        ));
        assert!(matches!(
            validate_declared_object_header(
                usize::try_from(MAX_SNAPSHOT_OBJECT_BYTES).unwrap() + 1,
                ObjectType::Commit,
                ObjectType::Commit,
            ),
            Err(RepositorySnapshotError::Rejected)
        ));

        let mut total = MAX_SNAPSHOT_OBJECT_BYTES - 1;
        assert!(matches!(
            account_declared_object_size(2, &mut total),
            Err(RepositorySnapshotError::Rejected)
        ));
        assert_eq!(total, MAX_SNAPSHOT_OBJECT_BYTES - 1);
        account_declared_object_size(1, &mut total).unwrap();
        assert_eq!(total, MAX_SNAPSHOT_OBJECT_BYTES);
    }
}
