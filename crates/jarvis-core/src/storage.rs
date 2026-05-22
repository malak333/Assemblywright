use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::router::{
    ModelProvider as RouteModelProvider, ModelRouteRecord, RouteEvidence, RouteOutcome,
};
use crate::{
    ApprovalStatus, AuditEntry, CapabilityScope, InstalledPlugin, InstalledPluginExecutionGrant,
    InstalledPluginProvenance, JarvisError, JarvisResult, PluginManifest, RiskTier, SchedulerJob,
    SchedulerJobStatus, Sensitivity, TaskRecord, TaskStatus, TriggerKind,
};

const CURRENT_SCHEMA_VERSION: i64 = 8;

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InstalledPluginRecord {
    pub id: String,
    pub manifest: PluginManifest,
    pub source_path: String,
    pub provenance: InstalledPluginProvenance,
    pub execution_enabled: bool,
    pub execution_grant: InstalledPluginExecutionGrant,
    pub installed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingApproval {
    pub id: Uuid,
    pub task_id: Uuid,
    pub action: String,
    pub requested_scopes: Vec<CapabilityScope>,
    pub risk_tier: RiskTier,
    pub sensitivity: Sensitivity,
    pub status: ApprovalStatus,
    pub reason: String,
    pub requested_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
    pub decided_by: Option<String>,
    pub decision_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NewPendingApproval {
    pub task_id: Uuid,
    pub action: String,
    pub requested_scopes: Vec<CapabilityScope>,
    pub risk_tier: RiskTier,
    pub sensitivity: Sensitivity,
    pub reason: String,
}

pub struct SqliteRepository {
    conn: Connection,
}

#[derive(Debug)]
struct MigrationBackup {
    original_path: PathBuf,
    backup_path: PathBuf,
    original_wal_path: PathBuf,
    backup_wal_path: Option<PathBuf>,
    original_shm_path: PathBuf,
    backup_shm_path: Option<PathBuf>,
}

impl std::fmt::Debug for SqliteRepository {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SqliteRepository")
            .finish_non_exhaustive()
    }
}

impl SqliteRepository {
    pub fn open(path: impl AsRef<Path>) -> JarvisResult<Self> {
        Self::open_with_migration_backup_dir(path, None::<&Path>)
    }

    pub fn open_with_migration_backup_dir(
        path: impl AsRef<Path>,
        backup_dir: Option<impl AsRef<Path>>,
    ) -> JarvisResult<Self> {
        open_with_migration_backup_dir_and_hook(path, backup_dir, |_| Ok(()))
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

    pub fn append_model_route_record(&self, record: &ModelRouteRecord) -> JarvisResult<()> {
        let evidence_json = serde_json::to_string(&record.evidence).map_err(|err| {
            JarvisError::Storage(format!("serialize model route evidence: {err}"))
        })?;
        self.conn
            .execute(
                "INSERT INTO model_route_records
                 (id, task_id, outcome, selected_provider, reason, sensitivity, approval_status,
                  redaction_applied, context_for_model, local_available, local_sufficient,
                  evidence_json, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, ?9, ?10, ?11, ?12)",
                params![
                    record.id.to_string(),
                    record.task_id.map(|id| id.to_string()),
                    route_outcome_to_str(&record.outcome),
                    record.selected_provider.map(route_model_provider_to_str),
                    &record.reason,
                    sensitivity_to_str(record.sensitivity),
                    approval_status_to_str(record.approval_status),
                    if record.redaction_applied { 1 } else { 0 },
                    if record.local_available { 1 } else { 0 },
                    if record.local_sufficient { 1 } else { 0 },
                    evidence_json,
                    to_db_time(record.created_at),
                ],
            )
            .map_err(storage_error)?;
        Ok(())
    }

    pub fn get_model_route_record(&self, id: Uuid) -> JarvisResult<Option<ModelRouteRecord>> {
        self.conn
            .query_row(
                "SELECT id, task_id, outcome, selected_provider, reason, sensitivity,
                        approval_status, redaction_applied, context_for_model,
                        local_available, local_sufficient, evidence_json, created_at
                 FROM model_route_records
                 WHERE id = ?1",
                params![id.to_string()],
                model_route_record_from_row,
            )
            .optional()
            .map_err(storage_error)
    }

    pub fn list_model_route_records(
        &self,
        task_id: Option<Uuid>,
    ) -> JarvisResult<Vec<ModelRouteRecord>> {
        let sql = match task_id {
            Some(_) => {
                "SELECT id, task_id, outcome, selected_provider, reason, sensitivity,
                        approval_status, redaction_applied, context_for_model,
                        local_available, local_sufficient, evidence_json, created_at
                 FROM model_route_records
                 WHERE task_id = ?1
                 ORDER BY created_at ASC, id ASC"
            }
            None => {
                "SELECT id, task_id, outcome, selected_provider, reason, sensitivity,
                        approval_status, redaction_applied, context_for_model,
                        local_available, local_sufficient, evidence_json, created_at
                 FROM model_route_records
                 ORDER BY created_at ASC, id ASC"
            }
        };
        let mut stmt = self.conn.prepare(sql).map_err(storage_error)?;
        let rows = match task_id {
            Some(id) => stmt
                .query_map(params![id.to_string()], model_route_record_from_row)
                .map_err(storage_error)?,
            None => stmt
                .query_map([], model_route_record_from_row)
                .map_err(storage_error)?,
        };
        collect_rows(rows)
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

    pub fn upsert_scheduler_job(&self, job: &SchedulerJob) -> JarvisResult<()> {
        self.conn
            .execute(
                "INSERT INTO scheduler_jobs
                 (id, name, command, trigger, status, created_at, updated_at, cancelled_at, cancellation_reason)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(id) DO UPDATE SET
                    name = excluded.name,
                    command = excluded.command,
                    trigger = excluded.trigger,
                    status = excluded.status,
                    updated_at = excluded.updated_at,
                    cancelled_at = excluded.cancelled_at,
                    cancellation_reason = excluded.cancellation_reason",
                params![
                    job.id.to_string(),
                    &job.name,
                    &job.command,
                    trigger_to_json(&job.trigger)?,
                    scheduler_status_to_str(job.status),
                    to_db_time(job.created_at),
                    to_db_time(job.updated_at),
                    job.cancelled_at.map(to_db_time),
                    &job.cancellation_reason,
                ],
            )
            .map_err(storage_error)?;
        Ok(())
    }

    pub fn get_scheduler_job(&self, id: Uuid) -> JarvisResult<Option<SchedulerJob>> {
        self.conn
            .query_row(
                "SELECT id, name, command, trigger, status, created_at, updated_at, cancelled_at, cancellation_reason
                 FROM scheduler_jobs
                 WHERE id = ?1",
                params![id.to_string()],
                scheduler_job_from_row,
            )
            .optional()
            .map_err(storage_error)
    }

    pub fn mark_scheduler_job_running(&self, id: Uuid) -> JarvisResult<SchedulerJob> {
        self.update_scheduler_job_status(id, SchedulerJobStatus::Running, None)
    }

    pub fn complete_scheduler_job(&self, id: Uuid) -> JarvisResult<SchedulerJob> {
        self.update_scheduler_job_status(id, SchedulerJobStatus::Completed, None)
    }

    pub fn fail_scheduler_job(&self, id: Uuid) -> JarvisResult<SchedulerJob> {
        self.update_scheduler_job_status(id, SchedulerJobStatus::Failed, None)
    }

    pub fn reschedule_interval_scheduler_job(&self, id: Uuid) -> JarvisResult<SchedulerJob> {
        let current = self
            .get_scheduler_job(id)?
            .ok_or_else(|| JarvisError::Storage(format!("scheduler job not found: {id}")))?;

        if !matches!(current.trigger, TriggerKind::Interval { .. }) {
            return Err(JarvisError::Storage(format!(
                "non-interval scheduler job cannot be rescheduled: {id}"
            )));
        }

        if matches!(
            current.status,
            SchedulerJobStatus::Completed
                | SchedulerJobStatus::Cancelled
                | SchedulerJobStatus::Failed
        ) {
            return Err(JarvisError::Storage(format!(
                "terminal scheduler job cannot be rescheduled: {id}"
            )));
        }

        let updated_at = Utc::now();
        let changed = self
            .conn
            .execute(
                "UPDATE scheduler_jobs
                 SET status = 'scheduled',
                     updated_at = ?1,
                     cancelled_at = NULL,
                     cancellation_reason = NULL
                 WHERE id = ?2",
                params![to_db_time(updated_at), id.to_string()],
            )
            .map_err(storage_error)?;

        if changed == 0 {
            return Err(JarvisError::Storage(format!(
                "scheduler job not found after reschedule: {id}"
            )));
        }

        self.get_scheduler_job(id)?.ok_or_else(|| {
            JarvisError::Storage(format!("scheduler job not found after reschedule: {id}"))
        })
    }

    pub fn cancel_scheduler_job(
        &self,
        id: Uuid,
        reason: impl Into<String>,
    ) -> JarvisResult<SchedulerJob> {
        self.update_scheduler_job_status(id, SchedulerJobStatus::Cancelled, Some(reason.into()))
    }

    pub fn list_scheduler_jobs(&self) -> JarvisResult<Vec<SchedulerJob>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, name, command, trigger, status, created_at, updated_at, cancelled_at, cancellation_reason
                 FROM scheduler_jobs
                 ORDER BY created_at ASC, id ASC",
            )
            .map_err(storage_error)?;
        let rows = stmt
            .query_map([], scheduler_job_from_row)
            .map_err(storage_error)?;
        collect_rows(rows)
    }

    pub fn install_plugin_metadata(
        &self,
        installed: InstalledPlugin,
    ) -> JarvisResult<InstalledPluginRecord> {
        let now = Utc::now();
        let manifest_json = serde_json::to_string(&installed.manifest).map_err(|err| {
            JarvisError::Storage(format!(
                "serialize plugin manifest {}: {err}",
                installed.manifest.id
            ))
        })?;
        let provenance_json = serde_json::to_string(&installed.provenance).map_err(|err| {
            JarvisError::Storage(format!(
                "serialize plugin provenance {}: {err}",
                installed.manifest.id
            ))
        })?;
        self.conn
            .execute(
                "INSERT INTO installed_plugins
                 (id, manifest_json, source_path, provenance_json, execution_enabled, execution_grant, installed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET
                    manifest_json = excluded.manifest_json,
                    source_path = excluded.source_path,
                    provenance_json = excluded.provenance_json,
                    execution_enabled = 0,
                    execution_grant = 'metadata_only',
                    installed_at = excluded.installed_at",
                params![
                    &installed.manifest.id,
                    manifest_json,
                    &installed.source_path,
                    provenance_json,
                    if installed.execution_enabled { 1 } else { 0 },
                    installed.execution_grant.as_str(),
                    to_db_time(now),
                ],
            )
            .map_err(storage_error)?;

        self.get_installed_plugin(&installed.manifest.id)?
            .ok_or_else(|| {
                JarvisError::Storage(format!(
                    "installed plugin not found after upsert: {}",
                    installed.manifest.id
                ))
            })
    }

    pub fn get_installed_plugin(&self, id: &str) -> JarvisResult<Option<InstalledPluginRecord>> {
        self.conn
            .query_row(
                "SELECT id, manifest_json, source_path, provenance_json, execution_enabled, execution_grant, installed_at
                 FROM installed_plugins
                 WHERE id = ?1",
                params![id],
                installed_plugin_from_row,
            )
            .optional()
            .map_err(storage_error)
    }

    pub fn list_installed_plugins(&self) -> JarvisResult<Vec<InstalledPluginRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, manifest_json, source_path, provenance_json, execution_enabled, execution_grant, installed_at
                 FROM installed_plugins
                 ORDER BY installed_at DESC, id ASC",
            )
            .map_err(storage_error)?;
        let rows = stmt
            .query_map([], installed_plugin_from_row)
            .map_err(storage_error)?;
        collect_rows(rows)
    }

    pub fn set_installed_plugin_execution(
        &self,
        id: &str,
        execution_enabled: bool,
        execution_grant: InstalledPluginExecutionGrant,
    ) -> JarvisResult<InstalledPluginRecord> {
        if execution_enabled && execution_grant == InstalledPluginExecutionGrant::MetadataOnly {
            return Err(JarvisError::Validation(
                "metadata_only grants cannot enable installed plugin execution".to_string(),
            ));
        }
        let changed = self
            .conn
            .execute(
                "UPDATE installed_plugins
                 SET execution_enabled = ?2,
                     execution_grant = ?3
                 WHERE id = ?1",
                params![
                    id,
                    if execution_enabled { 1 } else { 0 },
                    execution_grant.as_str(),
                ],
            )
            .map_err(storage_error)?;
        if changed == 0 {
            return Err(JarvisError::Storage(format!(
                "installed plugin not found: {id}"
            )));
        }
        self.get_installed_plugin(id)?
            .ok_or_else(|| JarvisError::Storage(format!("installed plugin not found: {id}")))
    }

    pub fn verify_installed_plugin_provenance(
        &self,
        id: &str,
    ) -> JarvisResult<InstalledPluginRecord> {
        let record = self
            .get_installed_plugin(id)?
            .ok_or_else(|| JarvisError::Storage(format!("installed plugin not found: {id}")))?;
        let provenance = record
            .provenance
            .verify_snapshot(&record.manifest, Utc::now());
        let provenance_json = serde_json::to_string(&provenance).map_err(|err| {
            JarvisError::Storage(format!("serialize plugin provenance {id}: {err}"))
        })?;
        self.conn
            .execute(
                "UPDATE installed_plugins
                 SET provenance_json = ?2
                 WHERE id = ?1",
                params![id, provenance_json],
            )
            .map_err(storage_error)?;
        self.get_installed_plugin(id)?
            .ok_or_else(|| JarvisError::Storage(format!("installed plugin not found: {id}")))
    }

    pub fn create_pending_approval(
        &self,
        approval: NewPendingApproval,
    ) -> JarvisResult<PendingApproval> {
        if self.get_task(approval.task_id)?.is_none() {
            return Err(JarvisError::Storage(format!(
                "task not found for approval: {}",
                approval.task_id
            )));
        }

        let pending = PendingApproval {
            id: Uuid::new_v4(),
            task_id: approval.task_id,
            action: approval.action,
            requested_scopes: approval.requested_scopes,
            risk_tier: approval.risk_tier,
            sensitivity: approval.sensitivity,
            status: ApprovalStatus::Pending,
            reason: approval.reason,
            requested_at: Utc::now(),
            decided_at: None,
            decided_by: None,
            decision_reason: None,
        };

        self.conn
            .execute(
                "INSERT INTO pending_approvals
                 (id, task_id, action, requested_scopes, risk_tier, sensitivity, status, reason, requested_at, decided_at, decided_by, decision_reason)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, NULL, NULL)",
                params![
                    pending.id.to_string(),
                    pending.task_id.to_string(),
                    &pending.action,
                    scopes_to_json(&pending.requested_scopes)?,
                    risk_tier_to_str(pending.risk_tier),
                    sensitivity_to_str(pending.sensitivity),
                    approval_status_to_str(pending.status),
                    &pending.reason,
                    to_db_time(pending.requested_at),
                ],
            )
            .map_err(storage_error)?;

        Ok(pending)
    }

    pub fn get_pending_approval(&self, id: Uuid) -> JarvisResult<Option<PendingApproval>> {
        self.conn
            .query_row(
                "SELECT id, task_id, action, requested_scopes, risk_tier, sensitivity, status, reason, requested_at, decided_at, decided_by, decision_reason
                 FROM pending_approvals
                 WHERE id = ?1",
                params![id.to_string()],
                pending_approval_from_row,
            )
            .optional()
            .map_err(storage_error)
    }

    pub fn list_pending_approvals(
        &self,
        status: Option<ApprovalStatus>,
    ) -> JarvisResult<Vec<PendingApproval>> {
        let sql = match status {
            Some(_) => {
                "SELECT id, task_id, action, requested_scopes, risk_tier, sensitivity, status, reason, requested_at, decided_at, decided_by, decision_reason
                 FROM pending_approvals
                 WHERE status = ?1
                 ORDER BY requested_at ASC, id ASC"
            }
            None => {
                "SELECT id, task_id, action, requested_scopes, risk_tier, sensitivity, status, reason, requested_at, decided_at, decided_by, decision_reason
                 FROM pending_approvals
                 ORDER BY requested_at ASC, id ASC"
            }
        };
        let mut stmt = self.conn.prepare(sql).map_err(storage_error)?;
        let rows = match status {
            Some(status) => stmt
                .query_map(
                    params![approval_status_to_str(status)],
                    pending_approval_from_row,
                )
                .map_err(storage_error)?,
            None => stmt
                .query_map([], pending_approval_from_row)
                .map_err(storage_error)?,
        };

        collect_rows(rows)
    }

    pub fn decide_pending_approval(
        &self,
        id: Uuid,
        status: ApprovalStatus,
        decided_by: impl Into<String>,
        decision_reason: Option<String>,
    ) -> JarvisResult<PendingApproval> {
        if !matches!(status, ApprovalStatus::Approved | ApprovalStatus::Denied) {
            return Err(JarvisError::Validation(
                "approval decision must be approved or denied".to_string(),
            ));
        }

        let current = self
            .get_pending_approval(id)?
            .ok_or_else(|| JarvisError::Storage(format!("pending approval not found: {id}")))?;
        if current.status != ApprovalStatus::Pending {
            return Err(JarvisError::Validation(format!(
                "approval is already {}: {id}",
                approval_status_to_str(current.status)
            )));
        }

        let decided_at = Utc::now();
        self.conn
            .execute(
                "UPDATE pending_approvals
                 SET status = ?1, decided_at = ?2, decided_by = ?3, decision_reason = ?4
                 WHERE id = ?5 AND status = 'pending'",
                params![
                    approval_status_to_str(status),
                    to_db_time(decided_at),
                    decided_by.into(),
                    decision_reason,
                    id.to_string()
                ],
            )
            .map_err(storage_error)?;

        self.get_pending_approval(id)?.ok_or_else(|| {
            JarvisError::Storage(format!("pending approval not found after decision: {id}"))
        })
    }

    fn update_scheduler_job_status(
        &self,
        id: Uuid,
        status: SchedulerJobStatus,
        cancellation_reason: Option<String>,
    ) -> JarvisResult<SchedulerJob> {
        let current = self
            .get_scheduler_job(id)?
            .ok_or_else(|| JarvisError::Storage(format!("scheduler job not found: {id}")))?;

        if current.status == SchedulerJobStatus::Cancelled
            && status == SchedulerJobStatus::Cancelled
        {
            return Ok(current);
        }

        if matches!(
            current.status,
            SchedulerJobStatus::Completed
                | SchedulerJobStatus::Cancelled
                | SchedulerJobStatus::Failed
        ) {
            return Err(JarvisError::Storage(format!(
                "terminal scheduler job cannot transition from {} to {}: {id}",
                scheduler_status_to_str(current.status),
                scheduler_status_to_str(status)
            )));
        }

        let updated_at = Utc::now();
        let cancelled_at = (status == SchedulerJobStatus::Cancelled).then_some(updated_at);
        let changed = self
            .conn
            .execute(
                "UPDATE scheduler_jobs
                 SET status = ?1,
                     updated_at = ?2,
                     cancelled_at = ?3,
                     cancellation_reason = CASE WHEN ?1 = 'cancelled' THEN ?4 ELSE cancellation_reason END
                 WHERE id = ?5",
                params![
                    scheduler_status_to_str(status),
                    to_db_time(updated_at),
                    cancelled_at.map(to_db_time),
                    cancellation_reason,
                    id.to_string()
                ],
            )
            .map_err(storage_error)?;

        if changed == 0 {
            return Err(JarvisError::Storage(format!(
                "scheduler job not found after transition: {id}"
            )));
        }

        self.get_scheduler_job(id)?.ok_or_else(|| {
            JarvisError::Storage(format!("scheduler job not found after transition: {id}"))
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
        if version > CURRENT_SCHEMA_VERSION {
            return Err(JarvisError::Storage(format!(
                "database schema version {version} is newer than this Jarvis build supports ({CURRENT_SCHEMA_VERSION}); upgrade Jarvis before opening this database"
            )));
        }
        if version < 1 {
            self.apply_migration_1()?;
        }
        if version < 2 {
            self.apply_migration_2()?;
        }
        if version < 3 {
            self.apply_migration_3()?;
        }
        if version < 4 {
            self.apply_migration_4()?;
        }
        if version < 5 {
            self.apply_migration_5()?;
        }
        if version < 6 {
            self.apply_migration_6()?;
        }
        if version < 7 {
            self.apply_migration_7()?;
        }
        if version < 8 {
            self.apply_migration_8()?;
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
            params![1, now],
        )
        .map_err(storage_error)?;

        tx.commit().map_err(storage_error)?;
        Ok(())
    }

    fn apply_migration_2(&self) -> JarvisResult<()> {
        let now = to_db_time(Utc::now());
        let tx = self.conn.unchecked_transaction().map_err(storage_error)?;

        tx.execute_batch(
            "
                CREATE TABLE scheduler_jobs (
                    id TEXT PRIMARY KEY NOT NULL,
                    name TEXT NOT NULL,
                    command TEXT NOT NULL,
                    trigger TEXT NOT NULL,
                    status TEXT NOT NULL CHECK (status IN (
                        'scheduled',
                        'running',
                        'completed',
                        'cancelled',
                        'failed'
                    )),
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL,
                    cancelled_at TEXT NULL,
                    cancellation_reason TEXT NULL
                );

                CREATE INDEX idx_scheduler_jobs_status_updated
                    ON scheduler_jobs (status, updated_at);
                CREATE INDEX idx_scheduler_jobs_created
                    ON scheduler_jobs (created_at);
                ",
        )
        .map_err(storage_error)?;

        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            params![2, now],
        )
        .map_err(storage_error)?;

        tx.commit().map_err(storage_error)?;
        Ok(())
    }

    fn apply_migration_3(&self) -> JarvisResult<()> {
        let now = to_db_time(Utc::now());
        let tx = self.conn.unchecked_transaction().map_err(storage_error)?;

        tx.execute_batch(
            "
                CREATE TABLE pending_approvals (
                    id TEXT PRIMARY KEY NOT NULL,
                    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
                    action TEXT NOT NULL,
                    requested_scopes TEXT NOT NULL,
                    risk_tier TEXT NOT NULL CHECK (risk_tier IN (
                        'low',
                        'notify',
                        'confirm',
                        'block'
                    )),
                    sensitivity TEXT NOT NULL CHECK (sensitivity IN (
                        'public',
                        'workspace',
                        'personal',
                        'private',
                        'credential_adjacent',
                        'restricted'
                    )),
                    status TEXT NOT NULL CHECK (status IN (
                        'pending',
                        'approved',
                        'denied'
                    )),
                    reason TEXT NOT NULL,
                    requested_at TEXT NOT NULL,
                    decided_at TEXT NULL,
                    decided_by TEXT NULL,
                    decision_reason TEXT NULL
                );

                CREATE INDEX idx_pending_approvals_task
                    ON pending_approvals (task_id, requested_at);
                CREATE INDEX idx_pending_approvals_status_requested
                    ON pending_approvals (status, requested_at);
                ",
        )
        .map_err(storage_error)?;

        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            params![3, now],
        )
        .map_err(storage_error)?;

        tx.commit().map_err(storage_error)?;
        Ok(())
    }

    fn apply_migration_4(&self) -> JarvisResult<()> {
        let now = to_db_time(Utc::now());
        let tx = self.conn.unchecked_transaction().map_err(storage_error)?;

        tx.execute_batch(
            "
                CREATE TABLE installed_plugins (
                    id TEXT PRIMARY KEY NOT NULL,
                    manifest_json TEXT NOT NULL,
                    source_path TEXT NOT NULL,
                    execution_enabled INTEGER NOT NULL CHECK (execution_enabled = 0),
                    installed_at TEXT NOT NULL
                );

                CREATE INDEX idx_installed_plugins_installed_at
                    ON installed_plugins (installed_at);
                ",
        )
        .map_err(storage_error)?;

        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            params![4, now],
        )
        .map_err(storage_error)?;

        tx.commit().map_err(storage_error)?;
        Ok(())
    }

    fn apply_migration_5(&self) -> JarvisResult<()> {
        let now = to_db_time(Utc::now());
        let tx = self.conn.unchecked_transaction().map_err(storage_error)?;

        tx.execute(
            "ALTER TABLE installed_plugins
             ADD COLUMN execution_grant TEXT NOT NULL DEFAULT 'metadata_only'
             CHECK (execution_grant = 'metadata_only')",
            [],
        )
        .map_err(storage_error)?;

        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            params![5, now],
        )
        .map_err(storage_error)?;

        tx.commit().map_err(storage_error)?;
        Ok(())
    }

    fn apply_migration_6(&self) -> JarvisResult<()> {
        let now = to_db_time(Utc::now());
        let tx = self.conn.unchecked_transaction().map_err(storage_error)?;

        tx.execute_batch(
            "
                ALTER TABLE installed_plugins RENAME TO installed_plugins_v5;

                CREATE TABLE installed_plugins (
                    id TEXT PRIMARY KEY NOT NULL,
                    manifest_json TEXT NOT NULL,
                    source_path TEXT NOT NULL,
                    execution_enabled INTEGER NOT NULL CHECK (execution_enabled IN (0, 1)),
                    execution_grant TEXT NOT NULL CHECK (
                        execution_grant IN ('metadata_only', 'subprocess_stdio')
                    ),
                    installed_at TEXT NOT NULL
                );

                INSERT INTO installed_plugins
                    (id, manifest_json, source_path, execution_enabled, execution_grant, installed_at)
                SELECT id, manifest_json, source_path, 0, 'metadata_only', installed_at
                FROM installed_plugins_v5;

                DROP TABLE installed_plugins_v5;

                CREATE INDEX idx_installed_plugins_installed_at
                    ON installed_plugins (installed_at);
                ",
        )
        .map_err(storage_error)?;

        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            params![6, now],
        )
        .map_err(storage_error)?;

        tx.commit().map_err(storage_error)?;
        Ok(())
    }

    fn apply_migration_7(&self) -> JarvisResult<()> {
        let now = to_db_time(Utc::now());
        let tx = self.conn.unchecked_transaction().map_err(storage_error)?;

        tx.execute_batch(
            "
                CREATE TABLE model_route_records (
                    sequence INTEGER PRIMARY KEY AUTOINCREMENT,
                    id TEXT NOT NULL UNIQUE,
                    task_id TEXT NULL REFERENCES tasks(id) ON DELETE SET NULL,
                    outcome TEXT NOT NULL CHECK (outcome IN (
                        'selected',
                        'needs_approval',
                        'blocked'
                    )),
                    selected_provider TEXT NULL CHECK (
                        selected_provider IS NULL OR selected_provider IN ('local', 'chat_gpt')
                    ),
                    reason TEXT NOT NULL,
                    sensitivity TEXT NOT NULL CHECK (sensitivity IN (
                        'public',
                        'workspace',
                        'personal',
                        'private',
                        'credential_adjacent',
                        'restricted'
                    )),
                    approval_status TEXT NOT NULL CHECK (approval_status IN (
                        'not_required',
                        'pending',
                        'approved',
                        'denied'
                    )),
                    redaction_applied INTEGER NOT NULL CHECK (redaction_applied IN (0, 1)),
                    context_for_model TEXT NULL CHECK (context_for_model IS NULL),
                    local_available INTEGER NOT NULL CHECK (local_available IN (0, 1)),
                    local_sufficient INTEGER NOT NULL CHECK (local_sufficient IN (0, 1)),
                    evidence_json TEXT NOT NULL,
                    created_at TEXT NOT NULL
                );

                CREATE INDEX idx_model_route_records_task_sequence
                    ON model_route_records (task_id, sequence);
                CREATE INDEX idx_model_route_records_created
                    ON model_route_records (created_at);
                CREATE INDEX idx_model_route_records_outcome
                    ON model_route_records (outcome);

                CREATE TRIGGER model_route_records_no_update
                BEFORE UPDATE ON model_route_records
                BEGIN
                    SELECT RAISE(ABORT, 'model_route_records are append-only');
                END;

                CREATE TRIGGER model_route_records_no_delete
                BEFORE DELETE ON model_route_records
                BEGIN
                    SELECT RAISE(ABORT, 'model_route_records are append-only');
                END;
                ",
        )
        .map_err(storage_error)?;

        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            params![7, now],
        )
        .map_err(storage_error)?;

        tx.commit().map_err(storage_error)?;
        Ok(())
    }

    fn apply_migration_8(&self) -> JarvisResult<()> {
        let now = to_db_time(Utc::now());
        let tx = self.conn.unchecked_transaction().map_err(storage_error)?;

        tx.execute_batch(
            "
                ALTER TABLE installed_plugins RENAME TO installed_plugins_v7;

                CREATE TABLE installed_plugins (
                    id TEXT PRIMARY KEY NOT NULL,
                    manifest_json TEXT NOT NULL,
                    source_path TEXT NOT NULL,
                    provenance_json TEXT NOT NULL,
                    execution_enabled INTEGER NOT NULL CHECK (execution_enabled IN (0, 1)),
                    execution_grant TEXT NOT NULL CHECK (
                        execution_grant IN ('metadata_only', 'subprocess_stdio')
                    ),
                    installed_at TEXT NOT NULL
                );

                INSERT INTO installed_plugins
                    (id, manifest_json, source_path, provenance_json, execution_enabled, execution_grant, installed_at)
                SELECT
                    id,
                    manifest_json,
                    source_path,
                    json_object(
                        'provenance_schema_version', 1,
                        'capture_method', 'legacy_migration',
                        'manifest_path', '',
                        'manifest_sha256', '',
                        'source_path', source_path,
                        'source_path_canonicalized', 1,
                        'captured_at', installed_at,
                        'last_verified_at', NULL,
                        'integrity_status', 'not_verified',
                        'origin_claim', NULL,
                        'origin_claim_verified', 0
                    ),
                    execution_enabled,
                    execution_grant,
                    installed_at
                FROM installed_plugins_v7;

                DROP TABLE installed_plugins_v7;

                CREATE INDEX idx_installed_plugins_installed_at
                    ON installed_plugins (installed_at);
                ",
        )
        .map_err(storage_error)?;

        tx.execute(
            "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
            params![8, now],
        )
        .map_err(storage_error)?;

        tx.commit().map_err(storage_error)?;
        Ok(())
    }
}

fn open_with_migration_backup_dir_and_hook(
    path: impl AsRef<Path>,
    backup_dir: Option<impl AsRef<Path>>,
    before_open: impl FnOnce(&Path) -> JarvisResult<()>,
) -> JarvisResult<SqliteRepository> {
    let path = path.as_ref();
    let backup_dir = backup_dir.as_ref().map(|dir| dir.as_ref());
    let backup = prepare_migration_backup(path, backup_dir)?;
    before_open(path)?;

    let conn = match Connection::open(path).map_err(storage_error) {
        Ok(conn) => conn,
        Err(error) => {
            restore_after_open_failure(backup, error)?;
            unreachable!("restore_after_open_failure always returns Err");
        }
    };
    let repo = SqliteRepository { conn };

    if let Err(error) = repo.configure().and_then(|()| repo.migrate()) {
        drop(repo);
        restore_after_open_failure(backup, error)?;
        unreachable!("restore_after_open_failure always returns Err");
    }

    Ok(repo)
}

fn prepare_migration_backup(
    db_path: &Path,
    backup_dir: Option<&Path>,
) -> JarvisResult<Option<MigrationBackup>> {
    if !db_path.exists() || db_path.metadata().map_err(storage_io_error)?.len() == 0 {
        return Ok(None);
    }

    let schema_version = schema_version_for_existing_database(db_path).unwrap_or(0);
    if schema_version >= CURRENT_SCHEMA_VERSION {
        return Ok(None);
    }

    let backup_root = backup_dir.map(PathBuf::from).unwrap_or_else(|| {
        db_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(".jarvis-migration-backups")
    });
    fs::create_dir_all(&backup_root).map_err(storage_io_error)?;

    let db_file_name = db_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("jarvis.sqlite");
    let timestamp = Utc::now().format("%Y%m%dT%H%M%S%.fZ");
    let backup_file_name = format!("{db_file_name}.schema-v{schema_version}.{timestamp}.bak");
    let backup_path = backup_root.join(backup_file_name);
    fs::copy(db_path, &backup_path).map_err(storage_io_error)?;

    let original_wal_path = sqlite_companion_path(db_path, "-wal");
    let backup_wal_path = copy_companion_if_present(&original_wal_path, &backup_path, "-wal")?;
    let original_shm_path = sqlite_companion_path(db_path, "-shm");
    let backup_shm_path = copy_companion_if_present(&original_shm_path, &backup_path, "-shm")?;

    Ok(Some(MigrationBackup {
        original_path: db_path.to_path_buf(),
        backup_path,
        original_wal_path,
        backup_wal_path,
        original_shm_path,
        backup_shm_path,
    }))
}

fn restore_after_open_failure(
    backup: Option<MigrationBackup>,
    original_error: JarvisError,
) -> JarvisResult<SqliteRepository> {
    if let Some(backup) = backup {
        if let Err(restore_error) = restore_migration_backup(&backup) {
            return Err(JarvisError::Storage(format!(
                "migration failed: {original_error}; restore from {} failed: {restore_error}",
                backup.backup_path.display()
            )));
        }
    }
    Err(original_error)
}

fn restore_migration_backup(backup: &MigrationBackup) -> JarvisResult<()> {
    fs::copy(&backup.backup_path, &backup.original_path).map_err(storage_io_error)?;
    restore_companion(backup.backup_wal_path.as_deref(), &backup.original_wal_path)?;
    restore_companion(backup.backup_shm_path.as_deref(), &backup.original_shm_path)?;
    Ok(())
}

fn restore_companion(backup_path: Option<&Path>, original_path: &Path) -> JarvisResult<()> {
    if let Some(backup_path) = backup_path {
        fs::copy(backup_path, original_path).map_err(storage_io_error)?;
    } else if original_path.exists() {
        fs::remove_file(original_path).map_err(storage_io_error)?;
    }
    Ok(())
}

fn copy_companion_if_present(
    original_path: &Path,
    backup_path: &Path,
    suffix: &str,
) -> JarvisResult<Option<PathBuf>> {
    if !original_path.exists() {
        return Ok(None);
    }
    let companion_backup_path = sqlite_companion_path(backup_path, suffix);
    fs::copy(original_path, &companion_backup_path).map_err(storage_io_error)?;
    Ok(Some(companion_backup_path))
}

fn sqlite_companion_path(path: &Path, suffix: &str) -> PathBuf {
    let mut value = path.as_os_str().to_owned();
    value.push(suffix);
    PathBuf::from(value)
}

fn schema_version_for_existing_database(path: &Path) -> JarvisResult<i64> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(storage_error)?;
    let has_migration_table: Option<i64> = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_migrations'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(storage_error)?;

    if has_migration_table.is_none() {
        return Ok(0);
    }

    conn.query_row(
        "SELECT version FROM schema_migrations ORDER BY version DESC LIMIT 1",
        [],
        |row| row.get(0),
    )
    .optional()
    .map(|version| version.unwrap_or(0))
    .map_err(storage_error)
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

fn model_route_record_from_row(row: &Row<'_>) -> rusqlite::Result<ModelRouteRecord> {
    let task_id = row
        .get::<_, Option<String>>(1)?
        .map(|id| parse_uuid(&id))
        .transpose()?;
    let selected_provider = row
        .get::<_, Option<String>>(3)?
        .map(|provider| route_model_provider_from_str(&provider))
        .transpose()?;
    let evidence_json: String = row.get(11)?;
    let evidence = serde_json::from_str::<RouteEvidence>(&evidence_json).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(11, rusqlite::types::Type::Text, Box::new(err))
    })?;

    Ok(ModelRouteRecord {
        id: parse_uuid(&row.get::<_, String>(0)?)?,
        task_id,
        outcome: route_outcome_from_str(&row.get::<_, String>(2)?)?,
        selected_provider,
        reason: row.get(4)?,
        sensitivity: sensitivity_from_str(&row.get::<_, String>(5)?)?,
        approval_status: approval_status_from_str(&row.get::<_, String>(6)?)?,
        redaction_applied: row.get::<_, i64>(7)? != 0,
        context_for_model: row.get(8)?,
        local_available: row.get::<_, i64>(9)? != 0,
        local_sufficient: row.get::<_, i64>(10)? != 0,
        evidence,
        created_at: parse_db_time(&row.get::<_, String>(12)?)?,
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

fn scheduler_job_from_row(row: &Row<'_>) -> rusqlite::Result<SchedulerJob> {
    Ok(SchedulerJob {
        id: parse_uuid(&row.get::<_, String>(0)?)?,
        name: row.get(1)?,
        command: row.get(2)?,
        trigger: trigger_from_json(&row.get::<_, String>(3)?)?,
        status: scheduler_status_from_str(&row.get::<_, String>(4)?)?,
        created_at: parse_db_time(&row.get::<_, String>(5)?)?,
        updated_at: parse_db_time(&row.get::<_, String>(6)?)?,
        cancelled_at: row
            .get::<_, Option<String>>(7)?
            .map(|time| parse_db_time(&time))
            .transpose()?,
        cancellation_reason: row.get(8)?,
    })
}

fn installed_plugin_from_row(row: &Row<'_>) -> rusqlite::Result<InstalledPluginRecord> {
    let manifest_json: String = row.get(1)?;
    let manifest = serde_json::from_str::<PluginManifest>(&manifest_json).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(err))
    })?;
    let provenance_json: String = row.get(3)?;
    let provenance =
        serde_json::from_str::<InstalledPluginProvenance>(&provenance_json).map_err(|err| {
            rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Text, Box::new(err))
        })?;

    Ok(InstalledPluginRecord {
        id: row.get(0)?,
        manifest,
        source_path: row.get(2)?,
        provenance,
        execution_enabled: row.get::<_, i64>(4)? != 0,
        execution_grant: InstalledPluginExecutionGrant::parse(&row.get::<_, String>(5)?).map_err(
            |err| {
                rusqlite::Error::FromSqlConversionFailure(
                    5,
                    rusqlite::types::Type::Text,
                    Box::new(err),
                )
            },
        )?,
        installed_at: parse_db_time(&row.get::<_, String>(6)?)?,
    })
}

fn pending_approval_from_row(row: &Row<'_>) -> rusqlite::Result<PendingApproval> {
    Ok(PendingApproval {
        id: parse_uuid(&row.get::<_, String>(0)?)?,
        task_id: parse_uuid(&row.get::<_, String>(1)?)?,
        action: row.get(2)?,
        requested_scopes: scopes_from_json(&row.get::<_, String>(3)?)?,
        risk_tier: risk_tier_from_str(&row.get::<_, String>(4)?)?,
        sensitivity: sensitivity_from_str(&row.get::<_, String>(5)?)?,
        status: approval_status_from_str(&row.get::<_, String>(6)?)?,
        reason: row.get(7)?,
        requested_at: parse_db_time(&row.get::<_, String>(8)?)?,
        decided_at: row
            .get::<_, Option<String>>(9)?
            .map(|time| parse_db_time(&time))
            .transpose()?,
        decided_by: row.get(10)?,
        decision_reason: row.get(11)?,
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

fn scheduler_status_to_str(status: SchedulerJobStatus) -> &'static str {
    match status {
        SchedulerJobStatus::Scheduled => "scheduled",
        SchedulerJobStatus::Running => "running",
        SchedulerJobStatus::Completed => "completed",
        SchedulerJobStatus::Cancelled => "cancelled",
        SchedulerJobStatus::Failed => "failed",
    }
}

fn scheduler_status_from_str(value: &str) -> rusqlite::Result<SchedulerJobStatus> {
    serde_json::from_value(json!(value)).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
    })
}

fn risk_tier_to_str(risk_tier: RiskTier) -> &'static str {
    match risk_tier {
        RiskTier::Low => "low",
        RiskTier::Notify => "notify",
        RiskTier::Confirm => "confirm",
        RiskTier::Block => "block",
    }
}

fn risk_tier_from_str(value: &str) -> rusqlite::Result<RiskTier> {
    serde_json::from_value(json!(value)).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
    })
}

fn approval_status_to_str(status: ApprovalStatus) -> &'static str {
    match status {
        ApprovalStatus::NotRequired => "not_required",
        ApprovalStatus::Pending => "pending",
        ApprovalStatus::Approved => "approved",
        ApprovalStatus::Denied => "denied",
    }
}

fn approval_status_from_str(value: &str) -> rusqlite::Result<ApprovalStatus> {
    serde_json::from_value(json!(value)).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
    })
}

fn route_outcome_to_str(outcome: &RouteOutcome) -> &'static str {
    match outcome {
        RouteOutcome::Selected => "selected",
        RouteOutcome::NeedsApproval => "needs_approval",
        RouteOutcome::Blocked => "blocked",
    }
}

fn route_outcome_from_str(value: &str) -> rusqlite::Result<RouteOutcome> {
    serde_json::from_value(json!(value)).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
    })
}

fn route_model_provider_to_str(provider: RouteModelProvider) -> &'static str {
    match provider {
        RouteModelProvider::Local => "local",
        RouteModelProvider::ChatGpt => "chat_gpt",
    }
}

fn route_model_provider_from_str(value: &str) -> rusqlite::Result<RouteModelProvider> {
    serde_json::from_value(json!(value)).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
    })
}

fn trigger_to_json(trigger: &TriggerKind) -> JarvisResult<String> {
    serde_json::to_string(trigger)
        .map_err(|err| JarvisError::Storage(format!("serialize scheduler trigger: {err}")))
}

fn trigger_from_json(value: &str) -> rusqlite::Result<TriggerKind> {
    serde_json::from_str(value).map_err(|err| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
    })
}

fn scopes_to_json(scopes: &[CapabilityScope]) -> JarvisResult<String> {
    serde_json::to_string(scopes)
        .map_err(|err| JarvisError::Storage(format!("serialize approval scopes: {err}")))
}

fn scopes_from_json(value: &str) -> rusqlite::Result<Vec<CapabilityScope>> {
    serde_json::from_str(value).map_err(|err| {
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

fn storage_io_error(error: std::io::Error) -> JarvisError {
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
    fn migrating_legacy_file_database_creates_backup_snapshot() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("jarvis.sqlite");
        let backup_dir = dir.path().join("migration-backups");

        let repo = SqliteRepository::open(&db_path).unwrap();
        repo.raw_connection()
            .execute_batch(
                "
                DROP TRIGGER model_route_records_no_delete;
                DROP TRIGGER model_route_records_no_update;
                DROP TABLE model_route_records;
                DELETE FROM schema_migrations WHERE version IN (7, 8);
                ",
            )
            .unwrap();
        assert_eq!(repo.schema_version().unwrap(), 6);
        drop(repo);

        let migrated =
            SqliteRepository::open_with_migration_backup_dir(&db_path, Some(&backup_dir)).unwrap();
        assert_eq!(migrated.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);
        assert_eq!(
            migrated
                .raw_connection()
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'model_route_records'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
                .unwrap(),
            1
        );

        let backups = fs::read_dir(&backup_dir)
            .unwrap()
            .filter(|entry| {
                entry
                    .as_ref()
                    .ok()
                    .and_then(|entry| entry.file_name().into_string().ok())
                    .is_some_and(|name| name.ends_with(".bak"))
            })
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(backups.len(), 1);

        let backup_conn = Connection::open(backups[0].path()).unwrap();
        let backup_version: i64 = backup_conn
            .query_row(
                "SELECT version FROM schema_migrations ORDER BY version DESC LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(backup_version, 6);
        let backup_route_tables: i64 = backup_conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'model_route_records'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(backup_route_tables, 0);
    }

    #[test]
    fn migration_failure_restores_preflight_backup() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("jarvis.sqlite");
        let backup_dir = dir.path().join("migration-backups");

        let repo = SqliteRepository::open(&db_path).unwrap();
        repo.raw_connection()
            .execute_batch(
                "
                DROP TRIGGER model_route_records_no_delete;
                DROP TRIGGER model_route_records_no_update;
                DROP TABLE model_route_records;
                DELETE FROM schema_migrations WHERE version IN (7, 8);
                ",
            )
            .unwrap();
        assert_eq!(repo.schema_version().unwrap(), 6);
        drop(repo);

        let result = open_with_migration_backup_dir_and_hook(&db_path, Some(&backup_dir), |path| {
            fs::write(path, b"not a sqlite database").map_err(storage_io_error)?;
            Ok(())
        });

        assert!(result.is_err());
        let restored =
            SqliteRepository::open_with_migration_backup_dir(&db_path, Some(&backup_dir)).unwrap();
        assert_eq!(restored.schema_version().unwrap(), CURRENT_SCHEMA_VERSION);

        let backups = fs::read_dir(&backup_dir)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert!(backups.len() >= 2);
    }

    #[test]
    fn newer_schema_version_fails_with_explicit_upgrade_message() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("jarvis.sqlite");

        let repo = SqliteRepository::open(&db_path).unwrap();
        repo.raw_connection()
            .execute(
                "INSERT INTO schema_migrations (version, applied_at) VALUES (?1, ?2)",
                params![CURRENT_SCHEMA_VERSION + 1, to_db_time(Utc::now())],
            )
            .unwrap();
        drop(repo);

        let error = SqliteRepository::open(db_path).unwrap_err();
        let message = error.to_string();
        assert!(message.contains("newer than this Jarvis build supports"));
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
    fn model_route_records_are_append_only_redacted_and_durable() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("jarvis.sqlite");
        let repo = SqliteRepository::open(&db_path).unwrap();
        let task = repo
            .create_task(Uuid::new_v4(), "route with api_key=secret-value")
            .unwrap();
        let mut route = crate::ModelRouter::route(&crate::ModelRouteRequest::local(
            "route with api_key=secret-value",
            "route with api_key=secret-value",
        ));
        route.task_id = Some(task.id);

        repo.append_model_route_record(&route).unwrap();
        let fetched = repo.get_model_route_record(route.id).unwrap().unwrap();

        assert_eq!(fetched.id, route.id);
        assert_eq!(fetched.task_id, Some(task.id));
        assert_eq!(fetched.outcome, RouteOutcome::Selected);
        assert_eq!(fetched.selected_provider, Some(RouteModelProvider::Local));
        assert_eq!(fetched.context_for_model, None);
        assert_eq!(fetched.evidence.local_model, "fake-local-model");

        let stored_json = serde_json::to_string(&fetched).unwrap();
        assert!(!stored_json.contains("secret-value"));
        assert!(!stored_json.contains("api_key"));

        let update_error = repo
            .raw_connection()
            .execute(
                "UPDATE model_route_records SET reason = 'tampered' WHERE id = ?1",
                params![route.id.to_string()],
            )
            .unwrap_err();
        assert!(update_error.to_string().contains("append-only"));

        drop(repo);

        let reopened = SqliteRepository::open(&db_path).unwrap();
        let routes = reopened.list_model_route_records(Some(task.id)).unwrap();
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].id, route.id);
        assert_eq!(routes[0].context_for_model, None);
    }

    #[test]
    fn emergency_pause_state_can_be_toggled_durably() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("jarvis.sqlite");
        let repo = SqliteRepository::open(&db_path).unwrap();

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

        drop(repo);

        let reopened = SqliteRepository::open(db_path).unwrap();
        let persisted = reopened.emergency_pause_state().unwrap();
        assert!(!persisted.paused);
        assert_eq!(persisted.reason.as_deref(), Some("manual resume"));
        assert_eq!(persisted.updated_by.as_deref(), Some("app"));
        assert!(persisted.updated_at >= paused.updated_at);
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

    #[test]
    fn scheduler_jobs_persist_trigger_status_and_cancellation() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("jarvis.sqlite");
        let repo = SqliteRepository::open(&db_path).unwrap();
        let scheduler = crate::Scheduler::new();
        let job = scheduler
            .schedule(crate::SchedulerJobSpec {
                name: "daily review".to_string(),
                command: "summarize open loops".to_string(),
                trigger: TriggerKind::Interval { every_seconds: 900 },
            })
            .unwrap();

        repo.upsert_scheduler_job(&job).unwrap();
        let cancelled = scheduler.cancel(job.id, "test cleanup").unwrap();
        repo.upsert_scheduler_job(&cancelled).unwrap();
        drop(repo);

        let repo = SqliteRepository::open(db_path).unwrap();
        let jobs = repo.list_scheduler_jobs().unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, job.id);
        assert_eq!(jobs[0].status, SchedulerJobStatus::Cancelled);
        assert_eq!(
            jobs[0].trigger,
            TriggerKind::Interval { every_seconds: 900 }
        );
        assert_eq!(jobs[0].cancellation_reason.as_deref(), Some("test cleanup"));
    }

    #[test]
    fn scheduler_job_lifecycle_transitions_are_durable() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("jarvis.sqlite");
        let repo = SqliteRepository::open(&db_path).unwrap();
        let scheduler = crate::Scheduler::new();
        let complete_job = scheduler
            .schedule(crate::SchedulerJobSpec {
                name: "complete".to_string(),
                command: "record local completion".to_string(),
                trigger: TriggerKind::Manual,
            })
            .unwrap();
        let fail_job = scheduler
            .schedule(crate::SchedulerJobSpec {
                name: "fail".to_string(),
                command: "record local failure".to_string(),
                trigger: TriggerKind::Manual,
            })
            .unwrap();

        repo.upsert_scheduler_job(&complete_job).unwrap();
        repo.upsert_scheduler_job(&fail_job).unwrap();
        repo.mark_scheduler_job_running(complete_job.id).unwrap();
        let completed = repo.complete_scheduler_job(complete_job.id).unwrap();
        let failed = repo.fail_scheduler_job(fail_job.id).unwrap();

        assert_eq!(completed.status, SchedulerJobStatus::Completed);
        assert_eq!(failed.status, SchedulerJobStatus::Failed);
        assert!(repo.cancel_scheduler_job(completed.id, "too late").is_err());
        drop(repo);

        let repo = SqliteRepository::open(db_path).unwrap();
        let jobs = repo.list_scheduler_jobs().unwrap();
        assert_eq!(jobs.len(), 2);
        assert!(jobs
            .iter()
            .any(|job| job.id == complete_job.id && job.status == SchedulerJobStatus::Completed));
        assert!(jobs
            .iter()
            .any(|job| job.id == fail_job.id && job.status == SchedulerJobStatus::Failed));
    }

    #[test]
    fn installed_plugin_metadata_persists_disabled_registry_record() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("jarvis.sqlite");
        let source_path = dir.path().join("plugin-source");
        std::fs::create_dir(&source_path).unwrap();
        let source_path = source_path.canonicalize().unwrap().display().to_string();
        let repo = SqliteRepository::open(&db_path).unwrap();

        let installed = InstalledPlugin {
            manifest: local_test_manifest(&source_path),
            source_path: source_path.clone(),
            provenance: InstalledPluginProvenance::legacy_unverified(
                source_path.clone(),
                Utc::now(),
            ),
            execution_enabled: false,
            execution_grant: InstalledPluginExecutionGrant::MetadataOnly,
        };
        let record = repo.install_plugin_metadata(installed).unwrap();

        assert_eq!(record.id, "local_registry_test");
        assert_eq!(record.source_path, source_path);
        assert!(!record.execution_enabled);
        assert_eq!(
            record.execution_grant,
            InstalledPluginExecutionGrant::MetadataOnly
        );
        assert_eq!(record.manifest.actions[0].name, "inspect");
        assert_eq!(
            record.provenance.integrity_status,
            crate::InstalledPluginIntegrityStatus::NotVerified
        );
        assert_eq!(repo.list_installed_plugins().unwrap().len(), 1);

        drop(repo);

        let repo = SqliteRepository::open(db_path).unwrap();
        let persisted = repo
            .get_installed_plugin("local_registry_test")
            .unwrap()
            .unwrap();
        assert_eq!(persisted.id, "local_registry_test");
        assert!(!persisted.execution_enabled);
        assert_eq!(
            persisted.execution_grant,
            InstalledPluginExecutionGrant::MetadataOnly
        );
        assert_eq!(
            persisted.provenance.integrity_status,
            crate::InstalledPluginIntegrityStatus::NotVerified
        );
        assert_eq!(
            persisted.manifest.source_path.as_deref(),
            Some(source_path.as_str())
        );
    }

    fn local_test_manifest(source_path: &str) -> PluginManifest {
        PluginManifest {
            manifest_schema_version: 1,
            id: "local_registry_test".to_string(),
            name: "Local Registry Test".to_string(),
            version: "0.1.0".to_string(),
            source: crate::PluginSource::LocalDevelopment,
            author: "Jarvis Test".to_string(),
            source_path: Some(source_path.to_string()),
            subprocess: None,
            actions: vec![crate::PluginActionManifest {
                name: "inspect".to_string(),
                description: "Validate registry persistence.".to_string(),
                permissions: vec![crate::PluginPermission::ReadWorkspace],
                risk_tier: crate::RiskTier::Low,
                input_schema: crate::JsonSchema::empty_object(),
                output_schema: crate::JsonSchema::empty_object(),
                proactive: false,
                memory_access: crate::PluginAccess::None,
                model_access: crate::PluginAccess::None,
                audit_fields: Vec::new(),
                timeout: crate::PluginTimeout::default_for_action(),
                cancellation: crate::CancellationBehavior::Cooperative,
            }],
        }
    }

    #[test]
    fn interval_scheduler_job_reschedule_is_durable() {
        let repo = SqliteRepository::in_memory().unwrap();
        let scheduler = crate::Scheduler::new();
        let job = scheduler
            .schedule(crate::SchedulerJobSpec {
                name: "interval".to_string(),
                command: "status".to_string(),
                trigger: TriggerKind::Interval { every_seconds: 60 },
            })
            .unwrap();

        repo.upsert_scheduler_job(&job).unwrap();
        repo.mark_scheduler_job_running(job.id).unwrap();
        let rescheduled = repo.reschedule_interval_scheduler_job(job.id).unwrap();

        assert_eq!(rescheduled.status, SchedulerJobStatus::Scheduled);
        assert_eq!(
            rescheduled.trigger,
            TriggerKind::Interval { every_seconds: 60 }
        );
        assert!(rescheduled.updated_at >= job.updated_at);
    }

    #[test]
    fn pending_approvals_persist_and_can_be_decided_once() {
        let repo = SqliteRepository::in_memory().unwrap();
        let task = repo
            .create_task(Uuid::new_v4(), "plugin echo private context")
            .unwrap();

        let pending = repo
            .create_pending_approval(NewPendingApproval {
                task_id: task.id,
                action: "fake_echo.echo".to_string(),
                requested_scopes: vec![CapabilityScope::PluginRun],
                risk_tier: RiskTier::Confirm,
                sensitivity: Sensitivity::Private,
                reason: "action requires explicit user confirmation".to_string(),
            })
            .unwrap();

        assert_eq!(pending.task_id, task.id);
        assert_eq!(pending.status, ApprovalStatus::Pending);
        assert_eq!(repo.list_pending_approvals(None).unwrap().len(), 1);
        assert_eq!(
            repo.list_pending_approvals(Some(ApprovalStatus::Pending))
                .unwrap()
                .len(),
            1
        );

        let approved = repo
            .decide_pending_approval(
                pending.id,
                ApprovalStatus::Approved,
                "cli",
                Some("approved after review".to_string()),
            )
            .unwrap();

        assert_eq!(approved.status, ApprovalStatus::Approved);
        assert_eq!(approved.decided_by.as_deref(), Some("cli"));
        assert_eq!(
            approved.decision_reason.as_deref(),
            Some("approved after review")
        );
        assert!(approved.decided_at.is_some());
        assert_eq!(
            repo.list_pending_approvals(Some(ApprovalStatus::Pending))
                .unwrap()
                .len(),
            0
        );
        assert!(repo
            .decide_pending_approval(pending.id, ApprovalStatus::Denied, "cli", None)
            .is_err());
    }
}
