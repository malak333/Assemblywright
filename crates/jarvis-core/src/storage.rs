use std::path::Path;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{AuditEntry, JarvisError, JarvisResult, Sensitivity, TaskRecord, TaskStatus};

const CURRENT_SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EmergencyPauseState {
    pub paused: bool,
    pub reason: Option<String>,
    pub updated_by: Option<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryItem {
    pub id: Uuid,
    pub category: String,
    pub key: String,
    pub value: String,
    pub provenance: String,
    pub sensitivity: Sensitivity,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewMemoryItem {
    pub category: String,
    pub key: String,
    pub value: String,
    pub provenance: String,
    pub sensitivity: Sensitivity,
}

pub struct SqliteRepository {
    conn: Connection,
}

impl SqliteRepository {
    pub fn open(path: impl AsRef<Path>) -> JarvisResult<Self> {
        let conn = Connection::open(path).map_err(storage_error)?;
        let repo = Self { conn };
        repo.configure()?;
        repo.migrate()?;
        Ok(repo)
    }

    pub fn in_memory() -> JarvisResult<Self> {
        let conn = Connection::open_in_memory().map_err(storage_error)?;
        let repo = Self { conn };
        repo.configure()?;
        repo.migrate()?;
        Ok(repo)
    }

    pub fn schema_version(&self) -> JarvisResult<i64> {
        self.conn
            .query_row(
                "SELECT version FROM schema_migrations ORDER BY version DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .optional()
            .map(|version| version.unwrap_or(0))
            .map_err(storage_error)
    }

    pub fn create_task(
        &self,
        session_id: Uuid,
        user_input: impl Into<String>,
    ) -> JarvisResult<TaskRecord> {
        let now = Utc::now();
        let task = TaskRecord {
            id: Uuid::new_v4(),
            session_id,
            user_input: user_input.into(),
            status: TaskStatus::Created,
            created_at: now,
            updated_at: now,
        };

        self.conn
            .execute(
                "INSERT INTO tasks (id, session_id, user_input, status, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    task.id.to_string(),
                    task.session_id.to_string(),
                    task.user_input,
                    task_status_to_str(&task.status),
                    to_db_time(task.created_at),
                    to_db_time(task.updated_at)
                ],
            )
            .map_err(storage_error)?;

        Ok(task)
    }

    pub fn get_task(&self, id: Uuid) -> JarvisResult<Option<TaskRecord>> {
        self.conn
            .query_row(
                "SELECT id, session_id, user_input, status, created_at, updated_at FROM tasks WHERE id = ?1",
                params![id.to_string()],
                task_from_row,
            )
            .optional()
            .map_err(storage_error)
    }

    pub fn delete_task(&self, id: Uuid) -> JarvisResult<bool> {
        self.conn
            .execute("DELETE FROM tasks WHERE id = ?1", params![id.to_string()])
            .map(|changed| changed > 0)
            .map_err(storage_error)
    }

    pub fn list_tasks(&self) -> JarvisResult<Vec<TaskRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, session_id, user_input, status, created_at, updated_at
                 FROM tasks
                 ORDER BY created_at ASC, id ASC",
            )
            .map_err(storage_error)?;

        let rows = stmt.query_map([], task_from_row).map_err(storage_error)?;
        collect_rows(rows)
    }

    pub fn update_task_status(&self, id: Uuid, status: TaskStatus) -> JarvisResult<TaskRecord> {
        let updated_at = Utc::now();
        let changed = self
            .conn
            .execute(
                "UPDATE tasks SET status = ?1, updated_at = ?2 WHERE id = ?3",
                params![
                    task_status_to_str(&status),
                    to_db_time(updated_at),
                    id.to_string()
                ],
            )
            .map_err(storage_error)?;

        if changed == 0 {
            return Err(JarvisError::Storage(format!("task not found: {id}")));
        }

        self.get_task(id)?
            .ok_or_else(|| JarvisError::Storage(format!("task not found after update: {id}")))
    }

    pub fn append_audit_entry(&self, entry: &AuditEntry) -> JarvisResult<()> {
        self.conn
            .execute(
                "INSERT INTO audit_entries (id, task_id, event_type, summary, payload, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    entry.id.to_string(),
                    entry.task_id.map(|id| id.to_string()),
                    entry.event_type,
                    entry.summary,
                    entry.payload,
                    to_db_time(entry.created_at),
                ],
            )
            .map_err(storage_error)?;
        Ok(())
    }

    pub fn get_audit_entry(&self, id: Uuid) -> JarvisResult<Option<AuditEntry>> {
        self.conn
            .query_row(
                "SELECT id, task_id, event_type, summary, payload, created_at
                 FROM audit_entries
                 WHERE id = ?1",
                params![id.to_string()],
                audit_entry_from_row,
            )
            .optional()
            .map_err(storage_error)
    }

    pub fn list_audit_entries(&self, task_id: Option<Uuid>) -> JarvisResult<Vec<AuditEntry>> {
        let sql = match task_id {
            Some(_) => {
                "SELECT id, task_id, event_type, summary, payload, created_at
                 FROM audit_entries
                 WHERE task_id = ?1
                 ORDER BY sequence ASC"
            }
            None => {
                "SELECT id, task_id, event_type, summary, payload, created_at
                 FROM audit_entries
                 ORDER BY sequence ASC"
            }
        };
        let mut stmt = self.conn.prepare(sql).map_err(storage_error)?;
        let rows = match task_id {
            Some(id) => stmt
                .query_map(params![id.to_string()], audit_entry_from_row)
                .map_err(storage_error)?,
            None => stmt
                .query_map([], audit_entry_from_row)
                .map_err(storage_error)?,
        };

        collect_rows(rows)
    }

    pub fn emergency_pause_state(&self) -> JarvisResult<EmergencyPauseState> {
        self.conn
            .query_row(
                "SELECT paused, reason, updated_by, updated_at FROM emergency_pause WHERE id = 1",
                [],
                |row| {
                    Ok(EmergencyPauseState {
                        paused: row.get::<_, i64>(0)? != 0,
                        reason: row.get(1)?,
                        updated_by: row.get(2)?,
                        updated_at: parse_db_time(&row.get::<_, String>(3)?)?,
                    })
                },
            )
            .map_err(storage_error)
    }

    pub fn set_emergency_pause(
        &self,
        paused: bool,
        reason: Option<&str>,
        updated_by: Option<&str>,
    ) -> JarvisResult<EmergencyPauseState> {
        let updated_at = Utc::now();
        self.conn
            .execute(
                "UPDATE emergency_pause
                 SET paused = ?1, reason = ?2, updated_by = ?3, updated_at = ?4
                 WHERE id = 1",
                params![
                    if paused { 1 } else { 0 },
                    reason,
                    updated_by,
                    to_db_time(updated_at)
                ],
            )
            .map_err(storage_error)?;

        self.emergency_pause_state()
    }

    pub fn create_memory_item(&self, item: NewMemoryItem) -> JarvisResult<MemoryItem> {
        let now = Utc::now();
        let memory = MemoryItem {
            id: Uuid::new_v4(),
            category: item.category,
            key: item.key,
            value: item.value,
            provenance: item.provenance,
            sensitivity: item.sensitivity,
            created_at: now,
            updated_at: now,
            reviewed_at: None,
            deleted_at: None,
        };

        self.conn
            .execute(
                "INSERT INTO memory_items
                 (id, category, key, value, provenance, sensitivity, created_at, updated_at, reviewed_at, deleted_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, NULL)",
                params![
                    memory.id.to_string(),
                    memory.category,
                    memory.key,
                    memory.value,
                    memory.provenance,
                    sensitivity_to_str(memory.sensitivity),
                    to_db_time(memory.created_at),
                    to_db_time(memory.updated_at),
                ],
            )
            .map_err(storage_error)?;

        Ok(memory)
    }

    pub fn get_memory_item(&self, id: Uuid) -> JarvisResult<Option<MemoryItem>> {
        self.conn
            .query_row(
                "SELECT id, category, key, value, provenance, sensitivity, created_at, updated_at, reviewed_at, deleted_at
                 FROM memory_items
                 WHERE id = ?1",
                params![id.to_string()],
                memory_item_from_row,
            )
            .optional()
            .map_err(storage_error)
    }

    pub fn list_memory_items(&self, include_deleted: bool) -> JarvisResult<Vec<MemoryItem>> {
        let sql = if include_deleted {
            "SELECT id, category, key, value, provenance, sensitivity, created_at, updated_at, reviewed_at, deleted_at
             FROM memory_items
             ORDER BY updated_at DESC, id ASC"
        } else {
            "SELECT id, category, key, value, provenance, sensitivity, created_at, updated_at, reviewed_at, deleted_at
             FROM memory_items
             WHERE deleted_at IS NULL
             ORDER BY updated_at DESC, id ASC"
        };
        let mut stmt = self.conn.prepare(sql).map_err(storage_error)?;
        let rows = stmt
            .query_map([], memory_item_from_row)
            .map_err(storage_error)?;
        collect_rows(rows)
    }

    pub fn update_memory_item(
        &self,
        id: Uuid,
        value: impl Into<String>,
        provenance: impl Into<String>,
        sensitivity: Sensitivity,
    ) -> JarvisResult<MemoryItem> {
        let updated_at = Utc::now();
        let changed = self
            .conn
            .execute(
                "UPDATE memory_items
                 SET value = ?1, provenance = ?2, sensitivity = ?3, updated_at = ?4
                 WHERE id = ?5 AND deleted_at IS NULL",
                params![
                    value.into(),
                    provenance.into(),
                    sensitivity_to_str(sensitivity),
                    to_db_time(updated_at),
                    id.to_string()
                ],
            )
            .map_err(storage_error)?;

        if changed == 0 {
            return Err(JarvisError::Storage(format!(
                "active memory item not found: {id}"
            )));
        }

        self.get_memory_item(id)?.ok_or_else(|| {
            JarvisError::Storage(format!("memory item not found after update: {id}"))
        })
    }

    pub fn mark_memory_reviewed(&self, id: Uuid) -> JarvisResult<MemoryItem> {
        let now = Utc::now();
        let changed = self
            .conn
            .execute(
                "UPDATE memory_items
                 SET reviewed_at = ?1, updated_at = ?1
                 WHERE id = ?2 AND deleted_at IS NULL",
                params![to_db_time(now), id.to_string()],
            )
            .map_err(storage_error)?;

        if changed == 0 {
            return Err(JarvisError::Storage(format!(
                "active memory item not found: {id}"
            )));
        }

        self.get_memory_item(id)?.ok_or_else(|| {
            JarvisError::Storage(format!("memory item not found after review: {id}"))
        })
    }

    pub fn delete_memory_item(&self, id: Uuid) -> JarvisResult<MemoryItem> {
        let now = Utc::now();
        let changed = self
            .conn
            .execute(
                "UPDATE memory_items
                 SET deleted_at = ?1, updated_at = ?1
                 WHERE id = ?2 AND deleted_at IS NULL",
                params![to_db_time(now), id.to_string()],
            )
            .map_err(storage_error)?;

        if changed == 0 {
            return Err(JarvisError::Storage(format!(
                "active memory item not found: {id}"
            )));
        }

        self.get_memory_item(id)?.ok_or_else(|| {
            JarvisError::Storage(format!("memory item not found after delete: {id}"))
        })
    }

    #[cfg(test)]
    fn raw_connection(&self) -> &Connection {
        &self.conn
    }

    fn configure(&self) -> JarvisResult<()> {
        self.conn
            .execute_batch(
                "
                PRAGMA foreign_keys = ON;
                PRAGMA journal_mode = WAL;
                PRAGMA synchronous = NORMAL;
                ",
            )
            .map_err(storage_error)
    }

    fn migrate(&self) -> JarvisResult<()> {
        self.conn
            .execute_batch(
                "
                CREATE TABLE IF NOT EXISTS schema_migrations (
                    version INTEGER PRIMARY KEY,
                    applied_at TEXT NOT NULL
                );
                ",
            )
            .map_err(storage_error)?;

        let version = self.schema_version()?;
        if version < 1 {
            self.apply_migration_1()?;
        }

        let migrated = self.schema_version()?;
        if migrated != CURRENT_SCHEMA_VERSION {
            return Err(JarvisError::Storage(format!(
                "unexpected schema version after migrations: {migrated}"
            )));
        }

        Ok(())
    }

    fn apply_migration_1(&self) -> JarvisResult<()> {
        let now = to_db_time(Utc::now());
        let tx = self.conn.unchecked_transaction().map_err(storage_error)?;

        tx.execute_batch(
            "
                CREATE TABLE tasks (
                    id TEXT PRIMARY KEY NOT NULL,
                    session_id TEXT NOT NULL,
                    user_input TEXT NOT NULL,
                    status TEXT NOT NULL CHECK (status IN (
                        'created',
                        'running',
                        'waiting_for_approval',
                        'blocked',
                        'completed',
                        'failed',
                        'cancelled'
                    )),
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE INDEX idx_tasks_session_created ON tasks (session_id, created_at);
                CREATE INDEX idx_tasks_status_updated ON tasks (status, updated_at);

                CREATE TABLE audit_entries (
                    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                    id TEXT NOT NULL UNIQUE,
                    task_id TEXT NULL REFERENCES tasks(id) ON DELETE SET NULL,
                    event_type TEXT NOT NULL,
                    summary TEXT NOT NULL,
                    payload TEXT NOT NULL,
                    created_at TEXT NOT NULL
                );

                CREATE INDEX idx_audit_entries_task_sequence ON audit_entries (task_id, sequence);
                CREATE INDEX idx_audit_entries_created ON audit_entries (created_at);

                CREATE TRIGGER audit_entries_no_update
                BEFORE UPDATE ON audit_entries
                BEGIN
                    SELECT RAISE(ABORT, 'audit_entries are append-only');
                END;

                CREATE TRIGGER audit_entries_no_delete
                BEFORE DELETE ON audit_entries
                BEGIN
                    SELECT RAISE(ABORT, 'audit_entries are append-only');
                END;

                CREATE TABLE emergency_pause (
                    id INTEGER PRIMARY KEY CHECK (id = 1),
                    paused INTEGER NOT NULL CHECK (paused IN (0, 1)),
                    reason TEXT NULL,
                    updated_by TEXT NULL,
                    updated_at TEXT NOT NULL
                );

                CREATE TABLE memory_items (
                    id TEXT PRIMARY KEY NOT NULL,
                    category TEXT NOT NULL,
                    key TEXT NOT NULL,
                    value TEXT NOT NULL,
                    provenance TEXT NOT NULL,
                    sensitivity TEXT NOT NULL CHECK (sensitivity IN (
                        'public',
                        'workspace',
                        'personal',
                        'private',
                        'credential_adjacent',
                        'restricted'
                    )),
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    reviewed_at TEXT NULL,
                    deleted_at TEXT NULL
                );

                CREATE UNIQUE INDEX idx_memory_items_active_key
                    ON memory_items (category, key)
                    WHERE deleted_at IS NULL;
                CREATE INDEX idx_memory_items_sensitivity ON memory_items (sensitivity);
                CREATE INDEX idx_memory_items_updated ON memory_items (updated_at);
                ",
        )
        .map_err(storage_error)?;

        tx.execute(
            "INSERT INTO emergency_pause (id, paused, reason, updated_by, updated_at)
                 VALUES (1, 0, NULL, NULL, ?1)",
            params![now],
        )
        .map_err(storage_error)?;

        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            params![CURRENT_SCHEMA_VERSION, now],
        )
        .map_err(storage_error)?;

        tx.commit().map_err(storage_error)?;
        Ok(())
    }
}

fn task_from_row(row: &Row<'_>) -> rusqlite::Result<TaskRecord> {
    Ok(TaskRecord {
        id: parse_uuid(&row.get::<_, String>(0)?)?,
        session_id: parse_uuid(&row.get::<_, String>(1)?)?,
        user_input: row.get(2)?,
        status: task_status_from_str(&row.get::<_, String>(3)?)?,
        created_at: parse_db_time(&row.get::<_, String>(4)?)?,
        updated_at: parse_db_time(&row.get::<_, String>(5)?)?,
    })
}

fn audit_entry_from_row(row: &Row<'_>) -> rusqlite::Result<AuditEntry> {
    let task_id = row
        .get::<_, Option<String>>(1)?
        .map(|id| parse_uuid(&id))
        .transpose()?;

    Ok(AuditEntry {
        id: parse_uuid(&row.get::<_, String>(0)?)?,
        task_id,
        event_type: row.get(2)?,
        summary: row.get(3)?,
        payload: row.get(4)?,
        created_at: parse_db_time(&row.get::<_, String>(5)?)?,
    })
}

fn memory_item_from_row(row: &Row<'_>) -> rusqlite::Result<MemoryItem> {
    Ok(MemoryItem {
        id: parse_uuid(&row.get::<_, String>(0)?)?,
        category: row.get(1)?,
        key: row.get(2)?,
        value: row.get(3)?,
        provenance: row.get(4)?,
        sensitivity: sensitivity_from_str(&row.get::<_, String>(5)?)?,
        created_at: parse_db_time(&row.get::<_, String>(6)?)?,
        updated_at: parse_db_time(&row.get::<_, String>(7)?)?,
        reviewed_at: row
            .get::<_, Option<String>>(8)?
            .map(|time| parse_db_time(&time))
            .transpose()?,
        deleted_at: row
            .get::<_, Option<String>>(9)?
            .map(|time| parse_db_time(&time))
            .transpose()?,
    })
}

fn collect_rows<T>(rows: impl Iterator<Item = rusqlite::Result<T>>) -> JarvisResult<Vec<T>> {
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(storage_error)
}

fn to_db_time(time: DateTime<Utc>) -> String {
    time.to_rfc3339()
}

fn parse_db_time(value: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
        })
}

fn parse_uuid(value: &str) -> rusqlite::Result<Uuid> {
    Uuid::parse_str(value).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
    })
}

fn task_status_to_str(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Created => "created",
        TaskStatus::Running => "running",
        TaskStatus::WaitingForApproval => "waiting_for_approval",
        TaskStatus::Blocked => "blocked",
        TaskStatus::Completed => "completed",
        TaskStatus::Failed => "failed",
        TaskStatus::Cancelled => "cancelled",
    }
}

fn task_status_from_str(value: &str) -> rusqlite::Result<TaskStatus> {
    serde_json::from_value(json!(value)).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
    })
}

fn sensitivity_to_str(sensitivity: Sensitivity) -> &'static str {
    match sensitivity {
        Sensitivity::Public => "public",
        Sensitivity::Workspace => "workspace",
        Sensitivity::Personal => "personal",
        Sensitivity::Private => "private",
        Sensitivity::CredentialAdjacent => "credential_adjacent",
        Sensitivity::Restricted => "restricted",
    }
}

fn sensitivity_from_str(value: &str) -> rusqlite::Result<Sensitivity> {
    serde_json::from_value(json!(value)).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
    })
}

fn storage_error(error: rusqlite::Error) -> JarvisError {
    JarvisError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn migrations_create_versioned_schema_and_default_pause_state() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("jarvis.sqlite");

        let repo = SqliteRepository::open(&db_path).unwrap();
        assert_eq!(repo.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);

        let pause = repo.emergency_pause_state().unwrap();
        assert!(!pause.paused);
        assert_eq!(pause.reason, None);

        drop(repo);

        let reopened = SqliteRepository::open(db_path).unwrap();
        assert_eq!(reopened.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
    }

    #[test]
    fn task_crud_persists_status_changes() {
        let repo = SqliteRepository::in_memory().unwrap();
        let session_id = Uuid::new_v4();

        let task = repo.create_task(session_id, "summarize my day").unwrap();
        assert_eq!(task.session_id, session_id);
        assert_eq!(task.status, TaskStatus::Created);

        let running = repo
            .update_task_status(task.id, TaskStatus::Running)
            .unwrap();
        assert_eq!(running.status, TaskStatus::Running);
        assert!(running.updated_at >= task.updated_at);

        let fetched = repo.get_task(task.id).unwrap().unwrap();
        assert_eq!(fetched.status, TaskStatus::Running);
        assert_eq!(repo.list_tasks().unwrap().len(), 1);

        assert!(repo.delete_task(task.id).unwrap());
        assert!(repo.get_task(task.id).unwrap().is_none());
        assert!(!repo.delete_task(task.id).unwrap());
    }

    #[test]
    fn audit_entries_are_append_only_and_ordered() {
        let repo = SqliteRepository::in_memory().unwrap();
        let task = repo.create_task(Uuid::new_v4(), "route model").unwrap();
        let first = AuditEntry::new(
            Some(task.id),
            "task.created",
            "Task created",
            json!({ "source": "test" }),
        );
        let second = AuditEntry::new(
            Some(task.id),
            "model.route",
            "Local model selected",
            json!({ "model": "local" }),
        );

        repo.append_audit_entry(&first).unwrap();
        repo.append_audit_entry(&second).unwrap();

        let fetched = repo.get_audit_entry(first.id).unwrap().unwrap();
        assert_eq!(fetched.id, first.id);
        assert_eq!(fetched.payload, json!({ "source": "test" }));

        let entries = repo.list_audit_entries(Some(task.id)).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, first.id);
        assert_eq!(entries[1].id, second.id);

        let update_error = repo
            .raw_connection()
            .execute(
                "UPDATE audit_entries SET summary = 'tampered' WHERE id = ?1",
                params![first.id.to_string()],
            )
            .unwrap_err();
        assert!(update_error.to_string().contains("append-only"));

        let delete_error = repo
            .raw_connection()
            .execute(
                "DELETE FROM audit_entries WHERE id = ?1",
                params![first.id.to_string()],
            )
            .unwrap_err();
        assert!(delete_error.to_string().contains("append-only"));
    }

    #[test]
    fn emergency_pause_state_can_be_toggled_durably() {
        let repo = SqliteRepository::in_memory().unwrap();

        let paused = repo
            .set_emergency_pause(true, Some("manual stop"), Some("app"))
            .unwrap();
        assert!(paused.paused);
        assert_eq!(paused.reason.as_deref(), Some("manual stop"));
        assert_eq!(paused.updated_by.as_deref(), Some("app"));

        let resumed = repo
            .set_emergency_pause(false, Some("manual resume"), Some("app"))
            .unwrap();
        assert!(!resumed.paused);
        assert_eq!(resumed.reason.as_deref(), Some("manual resume"));
    }

    #[test]
    fn memory_items_persist_provenance_sensitivity_review_and_soft_delete() {
        let repo = SqliteRepository::in_memory().unwrap();

        let memory = repo
            .create_memory_item(NewMemoryItem {
                category: "preference".to_string(),
                key: "default_model".to_string(),
                value: "local".to_string(),
                provenance: "user command 2026-05-20".to_string(),
                sensitivity: Sensitivity::Workspace,
            })
            .unwrap();

        assert_eq!(memory.provenance, "user command 2026-05-20");
        assert_eq!(memory.sensitivity, Sensitivity::Workspace);

        let updated = repo
            .update_memory_item(
                memory.id,
                "local-small",
                "settings panel change",
                Sensitivity::Personal,
            )
            .unwrap();
        assert_eq!(updated.value, "local-small");
        assert_eq!(updated.provenance, "settings panel change");
        assert_eq!(updated.sensitivity, Sensitivity::Personal);

        let reviewed = repo.mark_memory_reviewed(memory.id).unwrap();
        assert!(reviewed.reviewed_at.is_some());

        let deleted = repo.delete_memory_item(memory.id).unwrap();
        assert!(deleted.deleted_at.is_some());
        assert!(repo.list_memory_items(false).unwrap().is_empty());
        assert_eq!(repo.list_memory_items(true).unwrap().len(), 1);
    }

    #[test]
    fn active_memory_keys_are_unique_until_soft_deleted() {
        let repo = SqliteRepository::in_memory().unwrap();
        let item = NewMemoryItem {
            category: "project".to_string(),
            key: "jarvis".to_string(),
            value: "local first".to_string(),
            provenance: "test".to_string(),
            sensitivity: Sensitivity::Workspace,
        };

        let created = repo.create_memory_item(item.clone()).unwrap();
        assert!(repo.create_memory_item(item.clone()).is_err());

        repo.delete_memory_item(created.id).unwrap();
        assert!(repo.create_memory_item(item).is_ok());
    }
}
