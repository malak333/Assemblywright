use fs2::FileExt;
use jarvis_protocol::{
    CancellationAcknowledgement, CancellationAcknowledgementStatus, CancellationId,
    CancellationInstruction, DistributedEventBatch, DistributedEventCursor, FixtureJobResult,
    JobEnvelope, JobResultEnvelope, JobResultStatus, ProtocolError, PROTOCOL_VERSION,
};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use tokio::sync::Notify;
use uuid::Uuid;

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

pub const AGENT_SCHEMA_VERSION: i64 = 1;

#[derive(Debug, thiserror::Error)]
pub enum FixtureRuntimeError {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("fixture jobs are disabled")]
    Disabled,
    #[error("fixture job is already active")]
    AlreadyActive,
    #[error("fixture job is not active")]
    NotActive,
    #[error("fixture job was cancelled")]
    Cancelled,
    #[error("fixture runtime state is unavailable")]
    Unavailable,
    #[error("fixture result serialization failed")]
    Serialization,
}

#[derive(Clone)]
pub struct FixtureJobRuntime {
    enabled: bool,
    active: Arc<Mutex<HashMap<CancellationId, ActiveFixtureJob>>>,
}

#[derive(Clone)]
struct ActiveFixtureJob {
    job: JobEnvelope,
    cancelled: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl FixtureJobRuntime {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            active: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn execute(
        &self,
        job: JobEnvelope,
    ) -> Result<JobResultEnvelope, FixtureRuntimeError> {
        if !self.enabled {
            return Err(FixtureRuntimeError::Disabled);
        }
        let request = job.validate_fixture_reasoning()?;
        let active = ActiveFixtureJob {
            job: job.clone(),
            cancelled: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
        };
        {
            let mut jobs = self
                .active
                .lock()
                .map_err(|_| FixtureRuntimeError::Unavailable)?;
            if !jobs.is_empty() {
                return Err(FixtureRuntimeError::AlreadyActive);
            }
            jobs.insert(job.cancellation_id, active.clone());
        }

        if request.delay_ms > 0 {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(request.delay_ms)) => {}
                _ = active.notify.notified() => {}
            }
        }
        let cancelled = {
            let mut jobs = self
                .active
                .lock()
                .map_err(|_| FixtureRuntimeError::Unavailable)?;
            let cancelled = active.cancelled.load(Ordering::SeqCst);
            jobs.remove(&job.cancellation_id);
            cancelled
        };
        if cancelled {
            return Err(FixtureRuntimeError::Cancelled);
        }

        let payload = serde_json::to_value(FixtureJobResult::synthetic_echo(request.input))
            .map_err(|_| FixtureRuntimeError::Serialization)?;
        let payload_bytes =
            serde_json::to_vec(&payload).map_err(|_| FixtureRuntimeError::Serialization)?;
        let result = JobResultEnvelope {
            protocol_version: PROTOCOL_VERSION,
            connection_epoch: job.connection_epoch,
            sequence: job
                .sequence
                .checked_add(1)
                .ok_or(FixtureRuntimeError::Unavailable)?,
            task_id: job.task_id,
            step_id: job.step_id,
            attempt_id: job.attempt_id,
            lease_id: job.lease_id,
            cancellation_id: job.cancellation_id,
            status: JobResultStatus::Completed,
            context_sha256: job.context_sha256,
            payload_sha256: Sha256::digest(&payload_bytes).into(),
            payload,
        };
        result.validate_for_job(&job)?;
        Ok(result)
    }

    pub fn cancel(
        &self,
        instruction: &CancellationInstruction,
    ) -> Result<CancellationAcknowledgement, FixtureRuntimeError> {
        if !self.enabled {
            return Err(FixtureRuntimeError::Disabled);
        }
        let active = {
            let mut jobs = self
                .active
                .lock()
                .map_err(|_| FixtureRuntimeError::Unavailable)?;
            let active = jobs
                .get(&instruction.cancellation_id)
                .cloned()
                .ok_or(FixtureRuntimeError::NotActive)?;
            instruction.validate_for_job(&active.job)?;
            active.cancelled.store(true, Ordering::SeqCst);
            jobs.remove(&instruction.cancellation_id);
            active
        };
        active.notify.notify_waiters();
        let acknowledgement = CancellationAcknowledgement {
            protocol_version: PROTOCOL_VERSION,
            connection_epoch: instruction.connection_epoch,
            sequence: instruction
                .sequence
                .checked_add(1)
                .ok_or(FixtureRuntimeError::Unavailable)?,
            task_id: instruction.task_id,
            step_id: instruction.step_id,
            attempt_id: instruction.attempt_id,
            lease_id: instruction.lease_id,
            cancellation_id: instruction.cancellation_id,
            status: CancellationAcknowledgementStatus::Cancelled,
        };
        acknowledgement.validate_for_instruction(instruction)?;
        Ok(acknowledgement)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error(transparent)]
    Protocol(#[from] jarvis_protocol::ProtocolError),
    #[error("agent storage error: {0}")]
    Storage(#[from] rusqlite::Error),
    #[error("agent filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("agent data directory must be an absolute owner-only directory")]
    UnsafeDataDirectory,
    #[error("another jarvis-agent process already owns {lock_path}")]
    OwnerAlreadyActive { lock_path: PathBuf },
    #[error("event batch belongs to a different master stream")]
    EventStreamMismatch,
    #[error("event batch does not continue from the accepted durable cursor")]
    EventCursorGap,
    #[error("stored agent cursor is invalid")]
    InvalidStoredCursor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCursorSnapshot {
    pub cursor: Option<DistributedEventCursor>,
    pub updated_at_ms: Option<u64>,
}

pub struct AgentCursorStore {
    _owner_lock: File,
    connection: Connection,
}

impl AgentCursorStore {
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self, AgentError> {
        let data_dir = data_dir.as_ref();
        prepare_data_directory(data_dir)?;
        let lock_path = data_dir.join("agent.owner.lock");
        let owner_lock = open_owner_lock(&lock_path)?;
        owner_lock
            .try_lock_exclusive()
            .map_err(|_| AgentError::OwnerAlreadyActive {
                lock_path: lock_path.clone(),
            })?;
        let database_path = data_dir.join("agent.sqlite3");
        let connection = Connection::open(database_path)?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;\nPRAGMA busy_timeout = 5000;\nPRAGMA synchronous = FULL;",
        )?;
        migrate(&connection)?;
        Ok(Self {
            _owner_lock: owner_lock,
            connection,
        })
    }

    pub fn snapshot(&self) -> Result<AgentCursorSnapshot, AgentError> {
        let stored = self
            .connection
            .query_row(
                "SELECT stream_id, sequence, updated_at_ms FROM agent_event_cursor WHERE singleton = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                    ))
                },
            )
            .optional()?
            .ok_or(AgentError::InvalidStoredCursor)?;
        snapshot_from_stored(stored)
    }

    pub fn accept_batch(
        &mut self,
        batch: &DistributedEventBatch,
        now_ms: u64,
    ) -> Result<AgentCursorSnapshot, AgentError> {
        batch.validate()?;
        if now_ms == 0 {
            return Err(AgentError::InvalidStoredCursor);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let stored = tx.query_row(
            "SELECT stream_id, sequence, updated_at_ms FROM agent_event_cursor WHERE singleton = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            },
        )?;
        let current = snapshot_from_stored(stored)?;
        match current.cursor {
            Some(cursor) => {
                if cursor.stream_id != batch.stream_id {
                    return Err(AgentError::EventStreamMismatch);
                }
                if cursor.sequence != batch.after_sequence {
                    return Err(AgentError::EventCursorGap);
                }
            }
            None if batch.after_sequence != 0 => return Err(AgentError::EventCursorGap),
            None => {}
        }
        tx.execute(
            "UPDATE agent_event_cursor\n\
             SET stream_id = ?1, sequence = ?2, updated_at_ms = ?3\n\
             WHERE singleton = 1",
            params![
                batch.stream_id.to_string(),
                i64::try_from(batch.next_sequence).map_err(|_| AgentError::InvalidStoredCursor)?,
                i64::try_from(now_ms).map_err(|_| AgentError::InvalidStoredCursor)?,
            ],
        )?;
        tx.commit()?;
        Ok(AgentCursorSnapshot {
            cursor: Some(DistributedEventCursor {
                stream_id: batch.stream_id,
                sequence: batch.next_sequence,
            }),
            updated_at_ms: Some(now_ms),
        })
    }
}

fn migrate(connection: &Connection) -> Result<(), AgentError> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version == 0 {
        connection.execute_batch(
            "BEGIN IMMEDIATE;\n\
             CREATE TABLE agent_event_cursor (\n\
               singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),\n\
               stream_id TEXT,\n\
               sequence INTEGER NOT NULL CHECK (sequence >= 0),\n\
               updated_at_ms INTEGER,\n\
               CHECK ((stream_id IS NULL AND sequence = 0 AND updated_at_ms IS NULL) OR\n\
                      (stream_id IS NOT NULL AND updated_at_ms IS NOT NULL))\n\
             );\n\
             INSERT INTO agent_event_cursor (singleton, stream_id, sequence, updated_at_ms)\n\
               VALUES (1, NULL, 0, NULL);\n\
             PRAGMA user_version = 1;\n\
             COMMIT;",
        )?;
        return Ok(());
    }
    if version != AGENT_SCHEMA_VERSION {
        return Err(AgentError::InvalidStoredCursor);
    }
    Ok(())
}

fn snapshot_from_stored(
    (stream_id, sequence, updated_at_ms): (Option<String>, i64, Option<i64>),
) -> Result<AgentCursorSnapshot, AgentError> {
    let sequence = u64::try_from(sequence).map_err(|_| AgentError::InvalidStoredCursor)?;
    let updated_at_ms = updated_at_ms
        .map(|value| u64::try_from(value).map_err(|_| AgentError::InvalidStoredCursor))
        .transpose()?;
    let cursor = match stream_id {
        Some(stream_id) => {
            let stream_id =
                Uuid::parse_str(&stream_id).map_err(|_| AgentError::InvalidStoredCursor)?;
            if stream_id.is_nil() || updated_at_ms.is_none() {
                return Err(AgentError::InvalidStoredCursor);
            }
            Some(DistributedEventCursor {
                stream_id,
                sequence,
            })
        }
        None if sequence == 0 && updated_at_ms.is_none() => None,
        None => return Err(AgentError::InvalidStoredCursor),
    };
    Ok(AgentCursorSnapshot {
        cursor,
        updated_at_ms,
    })
}

fn prepare_data_directory(data_dir: &Path) -> Result<(), AgentError> {
    if !data_dir.is_absolute() {
        return Err(AgentError::UnsafeDataDirectory);
    }
    match fs::symlink_metadata(data_dir) {
        Ok(metadata) => validate_data_directory(&metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(data_dir)?;
            #[cfg(unix)]
            fs::set_permissions(data_dir, fs::Permissions::from_mode(0o700))?;
            validate_data_directory(&fs::symlink_metadata(data_dir)?)
        }
        Err(error) => Err(error.into()),
    }
}

fn validate_data_directory(metadata: &fs::Metadata) -> Result<(), AgentError> {
    if !metadata.file_type().is_dir() {
        return Err(AgentError::UnsafeDataDirectory);
    }
    #[cfg(unix)]
    if metadata.uid() != unsafe { libc::geteuid() }
        || metadata.permissions().mode() & 0o7777 != 0o700
    {
        return Err(AgentError::UnsafeDataDirectory);
    }
    Ok(())
}

fn open_owner_lock(path: &Path) -> Result<File, AgentError> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        options.mode(0o600);
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let file = options.open(path)?;
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        let metadata = file.metadata()?;
        if !metadata.file_type().is_file()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.nlink() != 1
            || metadata.permissions().mode() & 0o7777 != 0o600
        {
            return Err(AgentError::UnsafeDataDirectory);
        }
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use jarvis_protocol::{
        AttemptId, CancellationInstruction, ContextHandlingPolicy, DistributedEvent,
        DistributedEventKind, LeaseId, Sensitivity, StepId, TaskId, CANCELLATION_ACK_DEADLINE_MS,
        FIXTURE_REASONING_CAPABILITY_ID, FIXTURE_REASONING_MODEL, MAX_FIXTURE_INPUT_BYTES,
        PROTOCOL_VERSION,
    };
    use serde_json::json;
    use tempfile::tempdir;

    fn batch(stream_id: Uuid, after_sequence: u64, next_sequence: u64) -> DistributedEventBatch {
        let events = if next_sequence == after_sequence {
            Vec::new()
        } else {
            vec![DistributedEvent {
                protocol_version: PROTOCOL_VERSION,
                cursor: DistributedEventCursor {
                    stream_id,
                    sequence: next_sequence,
                },
                occurred_at_ms: 1_000,
                kind: DistributedEventKind::StepQueued,
                task_id: Some(TaskId::new(Uuid::new_v4())),
                step_id: Some(StepId::new(Uuid::new_v4())),
                device_id: None,
                connection_epoch: None,
            }]
        };
        DistributedEventBatch {
            protocol_version: PROTOCOL_VERSION,
            stream_id,
            after_sequence,
            next_sequence,
            events,
            has_more: false,
        }
    }

    fn fixture_job(delay_ms: u64, input: &str) -> JobEnvelope {
        let context = json!({
            "operation": "synthetic_echo",
            "input": input,
            "delay_ms": delay_ms
        });
        JobEnvelope {
            protocol_version: PROTOCOL_VERSION,
            connection_epoch: 7,
            sequence: 1,
            task_id: TaskId::new(Uuid::new_v4()),
            step_id: StepId::new(Uuid::new_v4()),
            attempt_id: AttemptId::new(Uuid::new_v4()),
            lease_id: LeaseId::new(Uuid::new_v4()),
            cancellation_id: CancellationId::new(Uuid::new_v4()),
            capability_id: FIXTURE_REASONING_CAPABILITY_ID.to_string(),
            selected_model: FIXTURE_REASONING_MODEL.to_string(),
            sensitivity: Sensitivity::Public,
            context_handling: ContextHandlingPolicy::EphemeralNoRetention,
            lease_duration_ms: 60_000,
            deadline_after_ms: 60_000,
            context_sha256: Sha256::digest(serde_json::to_vec(&context).unwrap()).into(),
            context,
        }
    }

    fn cancellation(job: &JobEnvelope) -> CancellationInstruction {
        CancellationInstruction {
            protocol_version: PROTOCOL_VERSION,
            connection_epoch: job.connection_epoch,
            sequence: job.sequence + 1,
            task_id: job.task_id,
            step_id: job.step_id,
            attempt_id: job.attempt_id,
            lease_id: job.lease_id,
            cancellation_id: job.cancellation_id,
            deadline_after_ms: CANCELLATION_ACK_DEADLINE_MS,
        }
    }

    #[test]
    fn agent_cursor_acceptance_is_durable_and_fail_closed() {
        let directory = tempdir().expect("temporary parent");
        let data_dir = directory.path().join("agent");
        let stream_id = Uuid::new_v4();
        let mut store = AgentCursorStore::open(&data_dir).expect("open agent store");
        assert_eq!(
            store.snapshot().expect("empty cursor"),
            AgentCursorSnapshot {
                cursor: None,
                updated_at_ms: None
            }
        );
        store
            .accept_batch(&batch(stream_id, 0, 1), 2_000)
            .expect("accept first event");
        assert!(matches!(
            store.accept_batch(&batch(stream_id, 0, 1), 2_001),
            Err(AgentError::EventCursorGap)
        ));
        drop(store);

        let mut reopened = AgentCursorStore::open(&data_dir).expect("reopen agent store");
        assert_eq!(
            reopened.snapshot().expect("durable cursor").cursor,
            Some(DistributedEventCursor {
                stream_id,
                sequence: 1
            })
        );
        assert!(matches!(
            reopened.accept_batch(&batch(Uuid::new_v4(), 1, 2), 3_000),
            Err(AgentError::EventStreamMismatch)
        ));
        reopened
            .accept_batch(&batch(stream_id, 1, 2), 3_001)
            .expect("accept exact successor");
    }

    #[tokio::test]
    async fn fixture_runtime_is_default_off_and_accepts_only_exact_bounded_contract() {
        let job = fixture_job(0, "fixture");
        assert!(matches!(
            FixtureJobRuntime::new(false).execute(job.clone()).await,
            Err(FixtureRuntimeError::Disabled)
        ));
        let result = FixtureJobRuntime::new(true)
            .execute(job.clone())
            .await
            .expect("execute fixed fixture");
        assert_eq!(result.payload["output"], "fixture");
        assert_eq!(result.payload["synthetic"], true);

        let mut wrong_model = job.clone();
        wrong_model.selected_model = "mlx-real-model".to_string();
        assert!(matches!(
            FixtureJobRuntime::new(true).execute(wrong_model).await,
            Err(FixtureRuntimeError::Protocol(_))
        ));
        let oversized = "x".repeat(MAX_FIXTURE_INPUT_BYTES + 1);
        assert!(matches!(
            FixtureJobRuntime::new(true)
                .execute(fixture_job(0, &oversized))
                .await,
            Err(FixtureRuntimeError::Protocol(_))
        ));
        let mut malformed = job;
        malformed.context =
            json!({"operation":"synthetic_echo","input":"x","delay_ms":0,"extra":true});
        malformed.context_sha256 =
            Sha256::digest(serde_json::to_vec(&malformed.context).unwrap()).into();
        assert!(matches!(
            FixtureJobRuntime::new(true).execute(malformed).await,
            Err(FixtureRuntimeError::Protocol(_))
        ));
    }

    #[tokio::test]
    async fn cancellation_suppresses_late_fixture_output() {
        let runtime = FixtureJobRuntime::new(true);
        let job = fixture_job(5_000, "must-not-escape");
        let execution = {
            let runtime = runtime.clone();
            let job = job.clone();
            tokio::spawn(async move { runtime.execute(job).await })
        };
        tokio::task::yield_now().await;
        let acknowledgement = runtime
            .cancel(&cancellation(&job))
            .expect("cancel active fixture");
        assert_eq!(
            acknowledgement.status,
            CancellationAcknowledgementStatus::Cancelled
        );
        assert!(matches!(
            execution.await.expect("join fixture"),
            Err(FixtureRuntimeError::Cancelled)
        ));
        assert!(matches!(
            runtime.cancel(&cancellation(&job)),
            Err(FixtureRuntimeError::NotActive)
        ));
    }
}
