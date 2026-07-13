use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{JarvisError, JarvisResult, MemoryItem, Sensitivity};

pub const MEMORY_INDEX_VERSION: u32 = 1;
pub const MAX_MEMORY_INDEX_BYTES: u64 = 8 * 1024 * 1024;
pub const MAX_MEMORY_INDEX_ENTRIES: usize = 10_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryIndexState {
    Unavailable,
    Missing,
    Corrupt,
    Stale,
    Current,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryIndexStatus {
    pub generated_at: DateTime<Utc>,
    pub state: MemoryIndexState,
    pub index_version: Option<u32>,
    pub rebuilt_at: Option<DateTime<Utc>>,
    pub active_record_count: usize,
    pub indexed_entry_count: usize,
    pub current_entry_count: usize,
    pub missing_entry_count: usize,
    pub stale_entry_count: usize,
    pub orphaned_entry_count: usize,
    pub deleted_projection_count: usize,
    pub canonical_source: String,
    pub retrieval_enabled: bool,
    pub redaction: String,
    pub detail: String,
}

impl MemoryIndexStatus {
    pub fn unavailable(active_record_count: usize) -> Self {
        Self {
            generated_at: Utc::now(),
            state: MemoryIndexState::Unavailable,
            index_version: None,
            rebuilt_at: None,
            active_record_count,
            indexed_entry_count: 0,
            current_entry_count: 0,
            missing_entry_count: active_record_count,
            stale_entry_count: 0,
            orphaned_entry_count: 0,
            deleted_projection_count: 0,
            canonical_source: "sqlite_memory_items".to_string(),
            retrieval_enabled: false,
            redaction: redaction_statement(),
            detail: "memory index artifact path is not configured".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MemoryIndexStore {
    path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct MemoryIndexManifest {
    schema: String,
    version: u32,
    rebuilt_at: DateTime<Utc>,
    entries: Vec<MemoryIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct MemoryIndexEntry {
    memory_id: Uuid,
    canonical_updated_at: DateTime<Utc>,
    sensitivity: Sensitivity,
    content_sha256: String,
}

impl MemoryIndexStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn status(&self, all_items: &[MemoryItem]) -> MemoryIndexStatus {
        let active = all_items
            .iter()
            .filter(|item| item.deleted_at.is_none())
            .map(|item| (item.id, item))
            .collect::<BTreeMap<_, _>>();
        let deleted = all_items
            .iter()
            .filter(|item| item.deleted_at.is_some())
            .map(|item| item.id)
            .collect::<BTreeSet<_>>();

        if !self.path.exists() {
            return status_without_manifest(
                MemoryIndexState::Missing,
                active.len(),
                "memory index artifact is missing",
            );
        }
        let manifest = match self.read_manifest() {
            Ok(manifest) => manifest,
            Err(_) => {
                return status_without_manifest(
                    MemoryIndexState::Corrupt,
                    active.len(),
                    "memory index artifact failed schema, version, or JSON validation",
                )
            }
        };

        let mut entries = BTreeMap::new();
        for entry in &manifest.entries {
            if entries.insert(entry.memory_id, entry).is_some() {
                return status_without_manifest(
                    MemoryIndexState::Corrupt,
                    active.len(),
                    "memory index artifact contains duplicate source records",
                );
            }
        }

        let mut current = 0;
        let mut missing = 0;
        let mut stale = 0;
        for (id, item) in &active {
            match entries.get(id) {
                None => missing += 1,
                Some(entry)
                    if entry.canonical_updated_at == item.updated_at
                        && entry.sensitivity == item.sensitivity
                        && entry.content_sha256 == content_digest(item) =>
                {
                    current += 1;
                }
                Some(_) => stale += 1,
            }
        }

        let mut orphaned = 0;
        let mut deleted_projection = 0;
        for id in entries.keys().filter(|id| !active.contains_key(id)) {
            if deleted.contains(id) {
                deleted_projection += 1;
            } else {
                orphaned += 1;
            }
        }
        let is_current = missing == 0 && stale == 0 && orphaned == 0 && deleted_projection == 0;
        MemoryIndexStatus {
            generated_at: Utc::now(),
            state: if is_current {
                MemoryIndexState::Current
            } else {
                MemoryIndexState::Stale
            },
            index_version: Some(manifest.version),
            rebuilt_at: Some(manifest.rebuilt_at),
            active_record_count: active.len(),
            indexed_entry_count: entries.len(),
            current_entry_count: current,
            missing_entry_count: missing,
            stale_entry_count: stale,
            orphaned_entry_count: orphaned,
            deleted_projection_count: deleted_projection,
            canonical_source: "sqlite_memory_items".to_string(),
            retrieval_enabled: false,
            redaction: redaction_statement(),
            detail: if is_current {
                "index projection matches canonical active memory records".to_string()
            } else {
                "index projection requires a canonical rebuild".to_string()
            },
        }
    }

    pub fn rebuild(&self, all_items: &[MemoryItem]) -> JarvisResult<MemoryIndexStatus> {
        let rebuilt_at = Utc::now();
        let entries = all_items
            .iter()
            .filter(|item| item.deleted_at.is_none())
            .map(|item| MemoryIndexEntry {
                memory_id: item.id,
                canonical_updated_at: item.updated_at,
                sensitivity: item.sensitivity,
                content_sha256: content_digest(item),
            })
            .collect::<Vec<_>>();
        if entries.len() > MAX_MEMORY_INDEX_ENTRIES {
            return Err(JarvisError::Storage(
                "canonical memory exceeds the index entry limit".to_string(),
            ));
        }
        let manifest = MemoryIndexManifest {
            schema: "jarvis.memory_index".to_string(),
            version: MEMORY_INDEX_VERSION,
            rebuilt_at,
            entries,
        };
        self.write_manifest_atomically(&manifest)?;
        let status = self.status(all_items);
        if status.state != MemoryIndexState::Current {
            return Err(JarvisError::Storage(
                "memory index rebuild did not produce a current projection".to_string(),
            ));
        }
        Ok(status)
    }

    fn read_manifest(&self) -> JarvisResult<MemoryIndexManifest> {
        let file = File::open(&self.path).map_err(index_io_error)?;
        if file.metadata().map_err(index_io_error)?.len() > MAX_MEMORY_INDEX_BYTES {
            return Err(JarvisError::Storage(
                "memory index artifact exceeds the byte limit".to_string(),
            ));
        }
        let mut bytes = Vec::new();
        file.take(MAX_MEMORY_INDEX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(index_io_error)?;
        if bytes.len() as u64 > MAX_MEMORY_INDEX_BYTES {
            return Err(JarvisError::Storage(
                "memory index artifact exceeds the byte limit".to_string(),
            ));
        }
        let manifest: MemoryIndexManifest = serde_json::from_slice(&bytes).map_err(|_| {
            JarvisError::Storage("memory index artifact is invalid JSON".to_string())
        })?;
        if manifest.schema != "jarvis.memory_index" || manifest.version != MEMORY_INDEX_VERSION {
            return Err(JarvisError::Storage(
                "memory index artifact schema or version mismatch".to_string(),
            ));
        }
        if manifest.entries.len() > MAX_MEMORY_INDEX_ENTRIES {
            return Err(JarvisError::Storage(
                "memory index artifact exceeds the entry limit".to_string(),
            ));
        }
        Ok(manifest)
    }

    fn write_manifest_atomically(&self, manifest: &MemoryIndexManifest) -> JarvisResult<()> {
        if manifest.entries.len() > MAX_MEMORY_INDEX_ENTRIES {
            return Err(JarvisError::Storage(
                "memory index artifact exceeds the entry limit".to_string(),
            ));
        }
        let bytes = serde_json::to_vec_pretty(manifest)
            .map_err(|_| JarvisError::Storage("serialize memory index artifact".to_string()))?;
        if bytes.len() as u64 > MAX_MEMORY_INDEX_BYTES {
            return Err(JarvisError::Storage(
                "memory index artifact exceeds the byte limit".to_string(),
            ));
        }
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(index_io_error)?;
        let file_name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("memory-index.json");
        let temp_path = parent.join(format!(".{file_name}.{}.tmp", Uuid::new_v4()));
        let result = (|| {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&temp_path).map_err(index_io_error)?;
            file.write_all(&bytes).map_err(index_io_error)?;
            file.sync_all().map_err(index_io_error)?;
            fs::rename(&temp_path, &self.path).map_err(index_io_error)?;
            File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(index_io_error)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp_path);
        }
        result
    }
}

fn status_without_manifest(
    state: MemoryIndexState,
    active_record_count: usize,
    detail: &str,
) -> MemoryIndexStatus {
    MemoryIndexStatus {
        generated_at: Utc::now(),
        state,
        index_version: None,
        rebuilt_at: None,
        active_record_count,
        indexed_entry_count: 0,
        current_entry_count: 0,
        missing_entry_count: active_record_count,
        stale_entry_count: 0,
        orphaned_entry_count: 0,
        deleted_projection_count: 0,
        canonical_source: "sqlite_memory_items".to_string(),
        retrieval_enabled: false,
        redaction: redaction_statement(),
        detail: detail.to_string(),
    }
}

fn redaction_statement() -> String {
    "status omits memory values, keys, categories, provenance, source identifiers, content digests, and artifact paths".to_string()
}

fn content_digest(item: &MemoryItem) -> String {
    let mut hasher = Sha256::new();
    for field in [&item.category, &item.key, &item.value, &item.provenance] {
        hasher.update((field.len() as u64).to_be_bytes());
        hasher.update(field.as_bytes());
    }
    hasher.update([item.sensitivity as u8]);
    format!("{:x}", hasher.finalize())
}

fn index_io_error(error: std::io::Error) -> JarvisError {
    JarvisError::Storage(format!("memory index artifact operation failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: Uuid, value: &str, deleted: bool) -> MemoryItem {
        let now = Utc::now();
        MemoryItem {
            id,
            category: "profile".to_string(),
            key: "preference".to_string(),
            value: value.to_string(),
            provenance: "user".to_string(),
            sensitivity: Sensitivity::Private,
            created_at: now,
            updated_at: now,
            reviewed_at: None,
            deleted_at: deleted.then_some(now),
        }
    }

    #[test]
    fn rebuild_and_status_track_canonical_lifecycle_without_public_content() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("memory-index.json");
        let store = MemoryIndexStore::new(&path);
        let id = Uuid::new_v4();
        let mut items = vec![item(id, "secret value", false)];

        assert_eq!(store.status(&items).state, MemoryIndexState::Missing);
        let current = store.rebuild(&items).unwrap();
        assert_eq!(current.state, MemoryIndexState::Current);
        assert_eq!(current.current_entry_count, 1);

        items[0].value = "changed secret".to_string();
        items[0].updated_at = Utc::now() + chrono::Duration::seconds(1);
        let stale = store.status(&items);
        assert_eq!(stale.state, MemoryIndexState::Stale);
        assert_eq!(stale.stale_entry_count, 1);
        let public = serde_json::to_string(&stale).unwrap();
        assert!(!public.contains("secret"));
        assert!(!public.contains(&id.to_string()));

        items[0].deleted_at = Some(Utc::now());
        let deleted = store.status(&items);
        assert_eq!(deleted.deleted_projection_count, 1);
        let rebuilt = store.rebuild(&items).unwrap();
        assert_eq!(rebuilt.indexed_entry_count, 0);
    }

    #[test]
    fn corrupt_artifact_fails_closed_and_rebuild_recovers() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("memory-index.json");
        fs::write(&path, b"not-json").unwrap();
        let store = MemoryIndexStore::new(&path);
        let items = vec![item(Uuid::new_v4(), "private", false)];
        assert_eq!(store.status(&items).state, MemoryIndexState::Corrupt);
        assert_eq!(
            store.rebuild(&items).unwrap().state,
            MemoryIndexState::Current
        );
    }

    #[test]
    fn oversized_duplicate_and_version_mismatch_artifacts_fail_closed() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("memory-index.json");
        let store = MemoryIndexStore::new(&path);
        let item = item(Uuid::new_v4(), "private", false);
        let items = vec![item.clone()];

        File::create(&path)
            .unwrap()
            .set_len(MAX_MEMORY_INDEX_BYTES + 1)
            .unwrap();
        assert_eq!(store.status(&items).state, MemoryIndexState::Corrupt);

        let entry = MemoryIndexEntry {
            memory_id: item.id,
            canonical_updated_at: item.updated_at,
            sensitivity: item.sensitivity,
            content_sha256: content_digest(&item),
        };
        fs::write(
            &path,
            serde_json::to_vec(&MemoryIndexManifest {
                schema: "jarvis.memory_index".to_string(),
                version: MEMORY_INDEX_VERSION,
                rebuilt_at: Utc::now(),
                entries: vec![entry.clone(), entry],
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(store.status(&items).state, MemoryIndexState::Corrupt);

        fs::write(
            &path,
            serde_json::to_vec(&MemoryIndexManifest {
                schema: "jarvis.memory_index".to_string(),
                version: MEMORY_INDEX_VERSION + 1,
                rebuilt_at: Utc::now(),
                entries: vec![],
            })
            .unwrap(),
        )
        .unwrap();
        assert_eq!(store.status(&items).state, MemoryIndexState::Corrupt);
    }

    #[test]
    fn oversized_rebuild_preserves_the_previous_valid_manifest() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("memory-index.json");
        let store = MemoryIndexStore::new(&path);
        let original = vec![item(Uuid::new_v4(), "original", false)];
        store.rebuild(&original).unwrap();
        let original_bytes = fs::read(&path).unwrap();

        let oversized = (0..=MAX_MEMORY_INDEX_ENTRIES)
            .map(|index| item(Uuid::new_v4(), &format!("value-{index}"), false))
            .collect::<Vec<_>>();
        assert!(store.rebuild(&oversized).is_err());
        assert_eq!(fs::read(&path).unwrap(), original_bytes);
        assert_eq!(store.status(&original).state, MemoryIndexState::Current);
    }

    #[test]
    fn status_counts_unknown_manifest_entries_as_orphaned() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("memory-index.json");
        let store = MemoryIndexStore::new(&path);
        let active = item(Uuid::new_v4(), "active", false);
        store.rebuild(std::slice::from_ref(&active)).unwrap();
        let mut manifest = store.read_manifest().unwrap();
        manifest.entries.push(MemoryIndexEntry {
            memory_id: Uuid::new_v4(),
            canonical_updated_at: Utc::now(),
            sensitivity: Sensitivity::Public,
            content_sha256: "0".repeat(64),
        });
        store.write_manifest_atomically(&manifest).unwrap();
        let status = store.status(&[active]);
        assert_eq!(status.state, MemoryIndexState::Stale);
        assert_eq!(status.orphaned_entry_count, 1);
    }
}
