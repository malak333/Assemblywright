use jarvis_protocol::{
    AttemptId, CancellationId, CapabilityDescriptor, ContextHandlingPolicy, DeviceId, DeviceRole,
    DistributedEvent, DistributedEventBatch, DistributedEventBatchRequest, DistributedEventCursor,
    DistributedEventKind, HandshakeRequest, HandshakeResponse, HandshakeStatus, JobEnvelope,
    JobResultEnvelope, JobResultStatus, LeaseId, ProtocolError, Sensitivity, StepId, TaskId,
    MAX_CAPABILITY_ID_BYTES, MAX_JOB_CONTEXT_BYTES, MAX_LEASE_DURATION_MS, MAX_STEP_DEADLINE_MS,
    PROTOCOL_VERSION,
};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use fs2::FileExt;

mod identity;

pub use identity::{
    EnrollmentGrantReceipt, EnrollmentGrantSpec, EnrollmentOperation, EnrollmentRequest,
    EphemeralServerIdentity, IdentityAuthority, IdentityAuthorityReceipt, IdentityError,
    IssuedDeviceCertificate, PlatformSecretProtector, SecretProtector,
    DEVICE_CERTIFICATE_LIFETIME_MS, ENROLLMENT_GRANT_TTL_MS, MAX_ENROLLED_DEVICES,
    SERVER_CERTIFICATE_LIFETIME_MS,
};

pub const MASTER_SCHEMA_VERSION: i64 = 3;
pub const MAX_QUEUED_OR_LEASED_STEPS: u64 = 256;
pub const MAX_CONCURRENT_JOBS: u64 = 4;

const REASON_UNKNOWN_DEVICE: &str = "unknown_device";
const REASON_REVOKED_DEVICE: &str = "revoked_device";
const REASON_REGISTRY_MISMATCH: &str = "registry_mismatch";
const REASON_IDENTITY_MISMATCH: &str = "identity_mismatch";
const REASON_CAPABILITY_MISMATCH: &str = "capability_mismatch";
const REASON_DUPLICATE_ACTIVE: &str = "duplicate_active_connection";

#[derive(Debug, thiserror::Error)]
pub enum MasterError {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("master storage error: {0}")]
    Storage(#[from] rusqlite::Error),
    #[error("master JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unsupported master schema version: expected {expected}, found {found}")]
    UnsupportedSchemaVersion { expected: i64, found: i64 },
    #[error("device is already registered")]
    DeviceAlreadyRegistered,
    #[error("device is not registered")]
    DeviceNotRegistered,
    #[error("device connection is not active")]
    ConnectionNotActive,
    #[error("connection epoch does not match the active device connection")]
    ConnectionEpochMismatch,
    #[error("device already has a leased job")]
    DeviceAlreadyLeased,
    #[error("the global concurrent-job limit has been reached")]
    ConcurrentJobLimit,
    #[error("no queued step is eligible for this device")]
    NoEligibleStep,
    #[error("job context or result exceeds the selected capability limit")]
    CapabilityLimitExceeded,
    #[error("the durable nonterminal queue limit has been reached")]
    QueueFull,
    #[error("task_id and step_id must not be nil")]
    NilStepIdentifier,
    #[error("capability_id is not a valid bounded protocol identifier")]
    InvalidCapabilityIdentifier,
    #[error("step context must be a bounded JSON object")]
    InvalidStepContext,
    #[error("step lease duration must be between 1 and {MAX_LEASE_DURATION_MS}")]
    InvalidLeaseDuration,
    #[error("step deadline must be between 1 and {MAX_STEP_DEADLINE_MS}")]
    InvalidStepDeadline,
    #[error("step is already present")]
    StepAlreadyExists,
    #[error("step was not found")]
    StepNotFound,
    #[error("step is not cancellable from status {0:?}")]
    StepNotCancellable(StepStatus),
    #[error("attempt was not found")]
    AttemptNotFound,
    #[error("attempt is not accepting results from status {0:?}")]
    ResultNotAccepting(AttemptStatus),
    #[error("result sequence is not newer than the connection high-water mark")]
    SequenceReplay,
    #[error("the result arrived after its lease expired")]
    LeaseExpired,
    #[error("event cursor belongs to a different master event stream")]
    EventCursorStreamMismatch,
    #[error("event cursor is ahead of the durable master event high-water mark")]
    EventCursorAhead,
    #[error("stored integer cannot be represented safely")]
    IntegerOutOfRange,
    #[error("stored state is invalid: {0}")]
    InvalidStoredState(String),
    #[error("system clock is before the Unix epoch or exceeds the durable range")]
    InvalidSystemClock,
    #[error("master filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("another jarvis-master process already owns {lock_path}")]
    OwnerAlreadyActive { lock_path: PathBuf },
    #[error(transparent)]
    Identity(#[from] IdentityError),
    #[error("the enrolled-device limit has been reached")]
    EnrolledDeviceLimit,
    #[error("the outstanding enrollment-grant limit has been reached")]
    EnrollmentGrantLimit,
    #[error("enrollment grant was not found")]
    EnrollmentGrantNotFound,
    #[error("enrollment grant secret is invalid")]
    InvalidEnrollmentGrantSecret,
    #[error("enrollment grant has expired")]
    EnrollmentGrantExpired,
    #[error("enrollment grant was already consumed")]
    EnrollmentGrantConsumed,
    #[error("enrollment grant state is invalid: {0}")]
    InvalidEnrollmentGrant(String),
    #[error("device certificate was not found")]
    DeviceCertificateNotFound,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceRegistration {
    pub device_id: DeviceId,
    pub device_name: String,
    pub role: DeviceRole,
    pub registry_revision: u64,
    pub capabilities: Vec<CapabilityDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewStep {
    pub task_id: TaskId,
    pub step_id: StepId,
    pub capability_id: String,
    pub sensitivity: Sensitivity,
    pub context: Value,
    pub lease_duration_ms: u64,
    pub deadline_after_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Queued,
    Leased,
    Succeeded,
    Failed,
    Cancelled,
}

impl StepStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Leased => "leased",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }

    fn parse(value: &str) -> Result<Self, MasterError> {
        match value {
            "queued" => Ok(Self::Queued),
            "leased" => Ok(Self::Leased),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(MasterError::InvalidStoredState(format!(
                "unknown step status {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptStatus {
    Leased,
    Succeeded,
    Failed,
    Cancelled,
    Abandoned,
    Expired,
}

impl AttemptStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Leased => "leased",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::Abandoned => "abandoned",
            Self::Expired => "expired",
        }
    }

    fn parse(value: &str) -> Result<Self, MasterError> {
        match value {
            "leased" => Ok(Self::Leased),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "abandoned" => Ok(Self::Abandoned),
            "expired" => Ok(Self::Expired),
            other => Err(MasterError::InvalidStoredState(format!(
                "unknown attempt status {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepSnapshot {
    pub task_id: TaskId,
    pub step_id: StepId,
    pub status: StepStatus,
    pub accepted_payload_sha256: Option<[u8; 32]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptedResult {
    pub task_id: TaskId,
    pub step_id: StepId,
    pub status: StepStatus,
    pub payload_sha256: [u8; 32],
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartupReconciliation {
    pub disconnected_connections: u64,
    pub abandoned_attempts: u64,
    pub requeued_steps: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeaseReconciliation {
    pub expired_attempts: u64,
    pub requeued_steps: u64,
}

pub struct MasterKernel {
    connection: Connection,
    startup_reconciliation: StartupReconciliation,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MasterHealthSnapshot {
    pub registered_devices: u64,
    pub active_device_certificates: u64,
    pub unconsumed_enrollment_grants: u64,
    pub active_connections: u64,
    pub queued_steps: u64,
    pub leased_steps: u64,
    pub terminal_steps: u64,
    pub active_attempts: u64,
}

pub struct MasterProcess {
    _owner_lock: File,
    data_dir: PathBuf,
    database_path: PathBuf,
    kernel: MasterKernel,
}

impl MasterProcess {
    pub fn acquire(data_dir: impl AsRef<Path>) -> Result<Self, MasterError> {
        fs::create_dir_all(data_dir.as_ref())?;
        let data_dir = fs::canonicalize(data_dir.as_ref())?;
        let lock_path = data_dir.join("master.owner.lock");
        let mut owner_lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&lock_path)?;
        if let Err(error) = owner_lock.try_lock_exclusive() {
            if error.kind() == std::io::ErrorKind::WouldBlock || error.raw_os_error() == Some(33) {
                return Err(MasterError::OwnerAlreadyActive { lock_path });
            }
            return Err(MasterError::Io(error));
        }

        owner_lock.set_len(0)?;
        owner_lock.seek(SeekFrom::Start(0))?;
        writeln!(
            owner_lock,
            "{{\"pid\":{},\"acquired_at_ms\":{}}}",
            std::process::id(),
            current_time_ms()?
        )?;
        owner_lock.flush()?;

        let database_path = data_dir.join("master.sqlite3");
        let kernel = MasterKernel::open(&database_path)?;
        Ok(Self {
            _owner_lock: owner_lock,
            data_dir,
            database_path,
            kernel,
        })
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    pub fn kernel(&self) -> &MasterKernel {
        &self.kernel
    }

    pub fn kernel_mut(&mut self) -> &mut MasterKernel {
        &mut self.kernel
    }
}

impl MasterKernel {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, MasterError> {
        let connection = Connection::open(path)?;
        Self::initialize(connection)
    }

    pub fn in_memory() -> Result<Self, MasterError> {
        let connection = Connection::open_in_memory()?;
        Self::initialize(connection)
    }

    fn initialize(connection: Connection) -> Result<Self, MasterError> {
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;\nPRAGMA busy_timeout = 5000;\nPRAGMA synchronous = FULL;",
        )?;
        let mut kernel = Self {
            connection,
            startup_reconciliation: StartupReconciliation::default(),
        };
        kernel.migrate()?;
        kernel.startup_reconciliation = kernel.reconcile_interrupted_state(current_time_ms()?)?;
        Ok(kernel)
    }

    pub fn schema_version(&self) -> Result<i64, MasterError> {
        self.connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(MasterError::from)
    }

    pub fn startup_reconciliation(&self) -> StartupReconciliation {
        self.startup_reconciliation
    }

    pub fn health_snapshot(&self) -> Result<MasterHealthSnapshot, MasterError> {
        fn count(connection: &Connection, sql: &str) -> Result<u64, MasterError> {
            let value: i64 = connection.query_row(sql, [], |row| row.get(0))?;
            u64::try_from(value).map_err(|_| MasterError::IntegerOutOfRange)
        }

        Ok(MasterHealthSnapshot {
            registered_devices: count(&self.connection, "SELECT COUNT(*) FROM master_devices")?,
            active_device_certificates: count(
                &self.connection,
                "SELECT COUNT(*) FROM master_device_certificates WHERE revoked_at_ms IS NULL",
            )?,
            unconsumed_enrollment_grants: count(
                &self.connection,
                "SELECT COUNT(*) FROM master_enrollment_grants WHERE consumed_at_ms IS NULL",
            )?,
            active_connections: count(
                &self.connection,
                "SELECT COUNT(*) FROM master_connections WHERE active = 1",
            )?,
            queued_steps: count(
                &self.connection,
                "SELECT COUNT(*) FROM master_steps WHERE status = 'queued'",
            )?,
            leased_steps: count(
                &self.connection,
                "SELECT COUNT(*) FROM master_steps WHERE status = 'leased'",
            )?,
            terminal_steps: count(
                &self.connection,
                "SELECT COUNT(*) FROM master_steps WHERE status IN ('succeeded', 'failed', 'cancelled')",
            )?,
            active_attempts: count(
                &self.connection,
                "SELECT COUNT(*) FROM master_attempts WHERE status = 'leased'",
            )?,
        })
    }

    pub fn distributed_events(
        &self,
        request: &DistributedEventBatchRequest,
    ) -> Result<DistributedEventBatch, MasterError> {
        request.validate()?;
        let (stream_id_text, high_water): (String, i64) = self.connection.query_row(
            "SELECT stream_id, next_sequence FROM master_event_stream WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let stream_id = parse_uuid(&stream_id_text)?;
        let durable_high_water = i64_to_u64(high_water)?;
        let after_sequence = match request.after {
            Some(cursor) => {
                if cursor.stream_id != stream_id {
                    return Err(MasterError::EventCursorStreamMismatch);
                }
                if cursor.sequence > durable_high_water {
                    return Err(MasterError::EventCursorAhead);
                }
                cursor.sequence
            }
            None => 0,
        };
        let query_limit = usize::from(request.limit)
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        let mut statement = self.connection.prepare(
            "SELECT sequence, occurred_at_ms, kind_json, task_id, step_id, device_id, connection_epoch\n\
             FROM master_events WHERE sequence > ?1 ORDER BY sequence ASC LIMIT ?2",
        )?;
        let mut events = statement
            .query_map(
                params![
                    u64_to_i64(after_sequence)?,
                    i64::try_from(query_limit).map_err(|_| MasterError::IntegerOutOfRange)?,
                ],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, Option<String>>(3)?,
                        row.get::<_, Option<String>>(4)?,
                        row.get::<_, Option<String>>(5)?,
                        row.get::<_, Option<i64>>(6)?,
                    ))
                },
            )?
            .map(|row| {
                let (
                    sequence,
                    occurred_at_ms,
                    kind_json,
                    task_id,
                    step_id,
                    device_id,
                    connection_epoch,
                ) = row?;
                Ok::<DistributedEvent, MasterError>(DistributedEvent {
                    protocol_version: PROTOCOL_VERSION,
                    cursor: DistributedEventCursor {
                        stream_id,
                        sequence: i64_to_u64(sequence)?,
                    },
                    occurred_at_ms: i64_to_u64(occurred_at_ms)?,
                    kind: serde_json::from_str(&kind_json)?,
                    task_id: task_id
                        .as_deref()
                        .map(parse_uuid)
                        .transpose()?
                        .map(TaskId::new),
                    step_id: step_id
                        .as_deref()
                        .map(parse_uuid)
                        .transpose()?
                        .map(StepId::new),
                    device_id: device_id
                        .as_deref()
                        .map(parse_uuid)
                        .transpose()?
                        .map(DeviceId::new),
                    connection_epoch: connection_epoch.map(i64_to_u64).transpose()?,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let has_more = events.len() > usize::from(request.limit);
        if has_more {
            events.truncate(usize::from(request.limit));
        }
        let next_sequence = events
            .last()
            .map(|event| event.cursor.sequence)
            .unwrap_or(after_sequence);
        let batch = DistributedEventBatch {
            protocol_version: PROTOCOL_VERSION,
            stream_id,
            after_sequence,
            next_sequence,
            events,
            has_more,
        };
        batch.validate()?;
        Ok(batch)
    }

    pub fn register_device(
        &mut self,
        registration: &DeviceRegistration,
    ) -> Result<(), MasterError> {
        let validation = HandshakeRequest {
            protocol_version: PROTOCOL_VERSION,
            device_id: registration.device_id,
            device_name: registration.device_name.clone(),
            role: registration.role,
            registry_revision: registration.registry_revision,
            capabilities: registration.capabilities.clone(),
        };
        validation.validate()?;

        let registered: i64 =
            self.connection
                .query_row("SELECT COUNT(*) FROM master_devices", [], |row| row.get(0))?;
        if i64_to_u64(registered)? >= MAX_ENROLLED_DEVICES {
            return Err(MasterError::EnrolledDeviceLimit);
        }

        let role_json = serde_json::to_string(&registration.role)?;
        let capabilities_json = serde_json::to_string(&registration.capabilities)?;
        let result = self.connection.execute(
            "INSERT INTO master_devices (device_id, device_name, role_json, registry_revision, capabilities_json, revoked)\n             VALUES (?1, ?2, ?3, ?4, ?5, 0)",
            params![
                registration.device_id.0.to_string(),
                registration.device_name,
                role_json,
                u64_to_i64(registration.registry_revision)?,
                capabilities_json,
            ],
        );
        match result {
            Ok(_) => Ok(()),
            Err(error) if is_constraint_violation(&error) => {
                Err(MasterError::DeviceAlreadyRegistered)
            }
            Err(error) => Err(error.into()),
        }
    }

    pub fn revoke_device(&mut self, device_id: DeviceId, now_ms: u64) -> Result<(), MasterError> {
        self.revoke_device_with_reason(device_id, now_ms, "device_revoked")
    }

    pub fn revoke_device_with_reason(
        &mut self,
        device_id: DeviceId,
        now_ms: u64,
        reason: &str,
    ) -> Result<(), MasterError> {
        identity::validate_revocation_reason(reason)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let changed = tx.execute(
            "UPDATE master_devices SET revoked = 1 WHERE device_id = ?1",
            [device_id.0.to_string()],
        )?;
        if changed == 0 {
            return Err(MasterError::DeviceNotRegistered);
        }
        tx.execute(
            "UPDATE master_device_certificates\n             SET revoked_at_ms = ?1, revocation_reason = ?2\n             WHERE device_id = ?3 AND revoked_at_ms IS NULL",
            params![
                u64_to_i64(now_ms)?,
                reason,
                device_id.0.to_string(),
            ],
        )?;
        if let Some(epoch) = active_connection_epoch(&tx, device_id)? {
            disconnect_device_tx(&tx, device_id, epoch, now_ms)?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn accept_handshake(
        &mut self,
        handshake: &HandshakeRequest,
        now_ms: u64,
    ) -> Result<HandshakeResponse, MasterError> {
        handshake.validate()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let stored = tx
            .query_row(
                "SELECT device_name, role_json, registry_revision, capabilities_json, revoked\n                 FROM master_devices WHERE device_id = ?1",
                [handshake.device_id.0.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .optional()?;

        let Some((device_name, role_json, revision, capabilities_json, revoked)) = stored else {
            return rejected_handshake(0, REASON_UNKNOWN_DEVICE);
        };
        if revoked != 0 {
            return rejected_handshake(handshake.registry_revision, REASON_REVOKED_DEVICE);
        }
        let stored_role: DeviceRole = serde_json::from_str(&role_json)?;
        let stored_capabilities: Vec<CapabilityDescriptor> =
            serde_json::from_str(&capabilities_json)?;
        if device_name != handshake.device_name || stored_role != handshake.role {
            return rejected_handshake(handshake.registry_revision, REASON_IDENTITY_MISMATCH);
        }
        let stored_revision = i64_to_u64(revision)?;
        if stored_revision != handshake.registry_revision {
            return rejected_handshake(stored_revision, REASON_REGISTRY_MISMATCH);
        }
        if !capabilities_match(&stored_capabilities, &handshake.capabilities) {
            return rejected_handshake(stored_revision, REASON_CAPABILITY_MISMATCH);
        }
        if active_connection_epoch(&tx, handshake.device_id)?.is_some() {
            return rejected_handshake(stored_revision, REASON_DUPLICATE_ACTIVE);
        }

        let current_epoch: i64 = tx.query_row(
            "SELECT integer_value FROM master_metadata WHERE key = 'next_connection_epoch'",
            [],
            |row| row.get(0),
        )?;
        let next_epoch = current_epoch
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        tx.execute(
            "UPDATE master_metadata SET integer_value = ?1 WHERE key = 'next_connection_epoch'",
            [next_epoch],
        )?;
        tx.execute(
            "INSERT INTO master_connections\n             (device_id, connection_epoch, active, last_sequence, connected_at_ms, disconnected_at_ms)\n             VALUES (?1, ?2, 1, 0, ?3, NULL)\n             ON CONFLICT(device_id) DO UPDATE SET\n               connection_epoch = excluded.connection_epoch,\n               active = 1,\n               last_sequence = 0,\n               connected_at_ms = excluded.connected_at_ms,\n               disconnected_at_ms = NULL",
            params![
                handshake.device_id.0.to_string(),
                next_epoch,
                u64_to_i64(now_ms)?,
            ],
        )?;
        append_distributed_event_tx(
            &tx,
            DistributedEventKind::DeviceConnected,
            now_ms,
            DistributedEventIdentity {
                task_id: None,
                step_id: None,
                device_id: Some(handshake.device_id),
                connection_epoch: Some(i64_to_u64(next_epoch)?),
            },
        )?;

        let response = HandshakeResponse {
            protocol_version: PROTOCOL_VERSION,
            status: HandshakeStatus::Accepted,
            connection_epoch: i64_to_u64(next_epoch)?,
            accepted_registry_revision: stored_revision,
            reason_code: None,
        };
        response.validate()?;
        tx.commit()?;
        Ok(response)
    }

    pub fn disconnect_device(
        &mut self,
        device_id: DeviceId,
        connection_epoch: u64,
        now_ms: u64,
    ) -> Result<StartupReconciliation, MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let reconciliation = disconnect_device_tx(&tx, device_id, connection_epoch, now_ms)?;
        tx.commit()?;
        Ok(reconciliation)
    }

    pub fn enqueue_step(&mut self, step: &NewStep, now_ms: u64) -> Result<(), MasterError> {
        validate_new_step(step)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM master_steps WHERE step_id = ?1)",
            [step.step_id.0.to_string()],
            |row| row.get(0),
        )?;
        if existing {
            return Err(MasterError::StepAlreadyExists);
        }
        let nonterminal: i64 = tx.query_row(
            "SELECT COUNT(*) FROM master_steps WHERE status IN ('queued', 'leased')",
            [],
            |row| row.get(0),
        )?;
        if i64_to_u64(nonterminal)? >= MAX_QUEUED_OR_LEASED_STEPS {
            return Err(MasterError::QueueFull);
        }

        let context_json = serde_json::to_string(&step.context)?;
        let context_sha256 = json_sha256(&step.context)?;
        let sensitivity_json = serde_json::to_string(&step.sensitivity)?;
        tx.execute(
            "INSERT INTO master_steps\n             (task_id, step_id, status, capability_id, sensitivity_json, context_json,\n              context_sha256, lease_duration_ms, deadline_after_ms, created_at_ms,\n              accepted_payload_json, accepted_payload_sha256, completed_at_ms)\n             VALUES (?1, ?2, 'queued', ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, NULL, NULL)",
            params![
                step.task_id.0.to_string(),
                step.step_id.0.to_string(),
                step.capability_id,
                sensitivity_json,
                context_json,
                context_sha256.as_slice(),
                u64_to_i64(step.lease_duration_ms)?,
                u64_to_i64(step.deadline_after_ms)?,
                u64_to_i64(now_ms)?,
            ],
        )?;
        append_distributed_event_tx(
            &tx,
            DistributedEventKind::StepQueued,
            now_ms,
            DistributedEventIdentity {
                task_id: Some(step.task_id),
                step_id: Some(step.step_id),
                device_id: None,
                connection_epoch: None,
            },
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn lease_next_step(
        &mut self,
        device_id: DeviceId,
        connection_epoch: u64,
        now_ms: u64,
    ) -> Result<JobEnvelope, MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        reconcile_expired_leases_tx(&tx, now_ms)?;
        let connection = connection_state(&tx, device_id)?;
        if !connection.active {
            return Err(MasterError::ConnectionNotActive);
        }
        if connection.epoch != connection_epoch {
            return Err(MasterError::ConnectionEpochMismatch);
        }
        let device_leases: i64 = tx.query_row(
            "SELECT COUNT(*) FROM master_attempts\n             WHERE device_id = ?1 AND connection_epoch = ?2 AND status = 'leased'",
            params![device_id.0.to_string(), u64_to_i64(connection_epoch)?],
            |row| row.get(0),
        )?;
        if device_leases != 0 {
            return Err(MasterError::DeviceAlreadyLeased);
        }
        let global_leases: i64 = tx.query_row(
            "SELECT COUNT(*) FROM master_attempts WHERE status = 'leased'",
            [],
            |row| row.get(0),
        )?;
        if i64_to_u64(global_leases)? >= MAX_CONCURRENT_JOBS {
            return Err(MasterError::ConcurrentJobLimit);
        }

        let capabilities_json: String = tx.query_row(
            "SELECT capabilities_json FROM master_devices WHERE device_id = ?1 AND revoked = 0",
            [device_id.0.to_string()],
            |row| row.get(0),
        )?;
        let capabilities: Vec<CapabilityDescriptor> = serde_json::from_str(&capabilities_json)?;
        let queued = load_queued_steps(&tx)?;
        let (step, capability) = queued
            .into_iter()
            .find_map(|step| {
                capabilities
                    .iter()
                    .find(|candidate| {
                        candidate.id == step.capability_id
                            && step.context_json.len() <= candidate.max_context_bytes as usize
                    })
                    .cloned()
                    .map(|capability| (step, capability))
            })
            .ok_or(MasterError::NoEligibleStep)?;

        let attempt_id = AttemptId::new(Uuid::new_v4());
        let lease_id = LeaseId::new(Uuid::new_v4());
        let cancellation_id = CancellationId::new(Uuid::new_v4());
        let sequence = connection
            .last_sequence
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        let lease_expires_at_ms = now_ms
            .checked_add(step.lease_duration_ms)
            .ok_or(MasterError::IntegerOutOfRange)?;
        let context: Value = serde_json::from_str(&step.context_json)?;
        let sensitivity: Sensitivity = serde_json::from_str(&step.sensitivity_json)?;
        let job = JobEnvelope {
            protocol_version: PROTOCOL_VERSION,
            connection_epoch,
            sequence,
            task_id: step.task_id,
            step_id: step.step_id,
            attempt_id,
            lease_id,
            cancellation_id,
            capability_id: capability.id,
            selected_model: capability.model,
            sensitivity,
            context_handling: ContextHandlingPolicy::EphemeralNoRetention,
            lease_duration_ms: step.lease_duration_ms,
            deadline_after_ms: step.deadline_after_ms,
            context_sha256: step.context_sha256,
            context,
        };
        job.validate()?;
        let job_json = serde_json::to_string(&job)?;

        tx.execute(
            "INSERT INTO master_attempts\n             (attempt_id, step_id, device_id, connection_epoch, lease_id, cancellation_id,\n              status, job_json, leased_at_ms, lease_expires_at_ms, completed_at_ms,\n              result_sequence, payload_sha256)\n             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'leased', ?7, ?8, ?9, NULL, NULL, NULL)",
            params![
                attempt_id.0.to_string(),
                step.step_id.0.to_string(),
                device_id.0.to_string(),
                u64_to_i64(connection_epoch)?,
                lease_id.0.to_string(),
                cancellation_id.0.to_string(),
                job_json,
                u64_to_i64(now_ms)?,
                u64_to_i64(lease_expires_at_ms)?,
            ],
        )?;
        let step_changed = tx.execute(
            "UPDATE master_steps SET status = 'leased' WHERE step_id = ?1 AND status = 'queued'",
            [step.step_id.0.to_string()],
        )?;
        if step_changed != 1 {
            return Err(MasterError::InvalidStoredState(
                "queued step changed before lease commit".to_string(),
            ));
        }
        let connection_changed = tx.execute(
            "UPDATE master_connections SET last_sequence = ?1\n             WHERE device_id = ?2 AND connection_epoch = ?3 AND active = 1",
            params![
                u64_to_i64(sequence)?,
                device_id.0.to_string(),
                u64_to_i64(connection_epoch)?,
            ],
        )?;
        if connection_changed != 1 {
            return Err(MasterError::ConnectionNotActive);
        }
        append_distributed_event_tx(
            &tx,
            DistributedEventKind::StepLeased,
            now_ms,
            DistributedEventIdentity {
                task_id: Some(step.task_id),
                step_id: Some(step.step_id),
                device_id: Some(device_id),
                connection_epoch: Some(connection_epoch),
            },
        )?;
        tx.commit()?;
        Ok(job)
    }

    pub fn accept_result(
        &mut self,
        result: &JobResultEnvelope,
        now_ms: u64,
    ) -> Result<AcceptedResult, MasterError> {
        result.validate()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let attempt = load_attempt(&tx, result.attempt_id)?.ok_or(MasterError::AttemptNotFound)?;
        let job: JobEnvelope = serde_json::from_str(&attempt.job_json)?;
        result.validate_for_job(&job)?;
        if attempt.status != AttemptStatus::Leased {
            return Err(MasterError::ResultNotAccepting(attempt.status));
        }
        if now_ms >= attempt.lease_expires_at_ms {
            tx.execute(
                "UPDATE master_attempts SET status = 'expired', completed_at_ms = ?1\n                 WHERE attempt_id = ?2 AND status = 'leased'",
                params![u64_to_i64(now_ms)?, result.attempt_id.0.to_string()],
            )?;
            let requeued = tx.execute(
                "UPDATE master_steps SET status = 'queued'\n                 WHERE step_id = ?1 AND status = 'leased'",
                [result.step_id.0.to_string()],
            )?;
            if requeued == 1 {
                append_distributed_event_tx(
                    &tx,
                    DistributedEventKind::StepQueued,
                    now_ms,
                    DistributedEventIdentity {
                        task_id: Some(result.task_id),
                        step_id: Some(result.step_id),
                        device_id: None,
                        connection_epoch: None,
                    },
                )?;
            }
            tx.commit()?;
            return Err(MasterError::LeaseExpired);
        }

        let connection = connection_state(&tx, attempt.device_id)?;
        if !connection.active {
            return Err(MasterError::ConnectionNotActive);
        }
        if connection.epoch != result.connection_epoch {
            return Err(MasterError::ConnectionEpochMismatch);
        }
        if result.sequence <= connection.last_sequence {
            return Err(MasterError::SequenceReplay);
        }
        let capability = capability_for_device(&tx, attempt.device_id, &job.capability_id)?;
        if serde_json::to_vec(&result.payload)?.len() > capability.max_result_bytes as usize {
            return Err(MasterError::CapabilityLimitExceeded);
        }
        let step_status = step_status_tx(&tx, result.step_id)?;
        if step_status != StepStatus::Leased {
            return Err(MasterError::ResultNotAccepting(attempt.status));
        }

        let (attempt_status, step_status) = match result.status {
            JobResultStatus::Completed => (AttemptStatus::Succeeded, StepStatus::Succeeded),
            JobResultStatus::Failed => (AttemptStatus::Failed, StepStatus::Failed),
            JobResultStatus::Cancelled => (AttemptStatus::Cancelled, StepStatus::Cancelled),
        };
        let payload_json = serde_json::to_string(&result.payload)?;
        tx.execute(
            "UPDATE master_attempts\n             SET status = ?1, completed_at_ms = ?2, result_sequence = ?3, payload_sha256 = ?4\n             WHERE attempt_id = ?5 AND status = 'leased'",
            params![
                attempt_status.as_str(),
                u64_to_i64(now_ms)?,
                u64_to_i64(result.sequence)?,
                result.payload_sha256.as_slice(),
                result.attempt_id.0.to_string(),
            ],
        )?;
        tx.execute(
            "UPDATE master_steps\n             SET status = ?1, accepted_payload_json = ?2, accepted_payload_sha256 = ?3, completed_at_ms = ?4\n             WHERE step_id = ?5 AND status = 'leased'",
            params![
                step_status.as_str(),
                payload_json,
                result.payload_sha256.as_slice(),
                u64_to_i64(now_ms)?,
                result.step_id.0.to_string(),
            ],
        )?;
        tx.execute(
            "UPDATE master_connections SET last_sequence = ?1\n             WHERE device_id = ?2 AND connection_epoch = ?3 AND active = 1",
            params![
                u64_to_i64(result.sequence)?,
                attempt.device_id.0.to_string(),
                u64_to_i64(result.connection_epoch)?,
            ],
        )?;
        let event_kind = match step_status {
            StepStatus::Succeeded => DistributedEventKind::StepSucceeded,
            StepStatus::Failed => DistributedEventKind::StepFailed,
            StepStatus::Cancelled => DistributedEventKind::StepCancelled,
            StepStatus::Queued | StepStatus::Leased => {
                return Err(MasterError::InvalidStoredState(
                    "accepted result produced a nonterminal step state".to_string(),
                ));
            }
        };
        let event_identity = if event_kind == DistributedEventKind::StepCancelled {
            DistributedEventIdentity {
                task_id: Some(result.task_id),
                step_id: Some(result.step_id),
                device_id: None,
                connection_epoch: None,
            }
        } else {
            DistributedEventIdentity {
                task_id: Some(result.task_id),
                step_id: Some(result.step_id),
                device_id: Some(attempt.device_id),
                connection_epoch: Some(result.connection_epoch),
            }
        };
        append_distributed_event_tx(&tx, event_kind, now_ms, event_identity)?;
        tx.commit()?;
        Ok(AcceptedResult {
            task_id: result.task_id,
            step_id: result.step_id,
            status: step_status,
            payload_sha256: result.payload_sha256,
        })
    }

    pub fn cancel_step(&mut self, step_id: StepId, now_ms: u64) -> Result<(), MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let task_id = tx
            .query_row(
                "SELECT task_id FROM master_steps WHERE step_id = ?1",
                [step_id.0.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or(MasterError::StepNotFound)?;
        let task_id = TaskId::new(parse_uuid(&task_id)?);
        let status = step_status_tx(&tx, step_id)?;
        match status {
            StepStatus::Queued => {
                tx.execute(
                    "UPDATE master_steps SET status = 'cancelled', completed_at_ms = ?1\n                     WHERE step_id = ?2 AND status = 'queued'",
                    params![u64_to_i64(now_ms)?, step_id.0.to_string()],
                )?;
            }
            StepStatus::Leased => {
                tx.execute(
                    "UPDATE master_attempts SET status = 'cancelled', completed_at_ms = ?1\n                     WHERE step_id = ?2 AND status = 'leased'",
                    params![u64_to_i64(now_ms)?, step_id.0.to_string()],
                )?;
                tx.execute(
                    "UPDATE master_steps SET status = 'cancelled', completed_at_ms = ?1\n                     WHERE step_id = ?2 AND status = 'leased'",
                    params![u64_to_i64(now_ms)?, step_id.0.to_string()],
                )?;
            }
            terminal => return Err(MasterError::StepNotCancellable(terminal)),
        }
        append_distributed_event_tx(
            &tx,
            DistributedEventKind::StepCancelled,
            now_ms,
            DistributedEventIdentity {
                task_id: Some(task_id),
                step_id: Some(step_id),
                device_id: None,
                connection_epoch: None,
            },
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn reconcile_expired_leases(
        &mut self,
        now_ms: u64,
    ) -> Result<LeaseReconciliation, MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let result = reconcile_expired_leases_tx(&tx, now_ms)?;
        tx.commit()?;
        Ok(result)
    }

    pub fn step_snapshot(&self, step_id: StepId) -> Result<StepSnapshot, MasterError> {
        self.connection
            .query_row(
                "SELECT task_id, status, accepted_payload_sha256\n                 FROM master_steps WHERE step_id = ?1",
                [step_id.0.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<Vec<u8>>>(2)?,
                    ))
                },
            )
            .optional()?
            .map(|(task_id, status, payload_sha256)| {
                Ok::<StepSnapshot, MasterError>(StepSnapshot {
                    task_id: TaskId::new(parse_uuid(&task_id)?),
                    step_id,
                    status: StepStatus::parse(&status)?,
                    accepted_payload_sha256: payload_sha256
                        .map(|value| digest_array(&value))
                        .transpose()?,
                })
            })
            .transpose()?
            .ok_or(MasterError::StepNotFound)
    }

    pub fn attempt_status(&self, attempt_id: AttemptId) -> Result<AttemptStatus, MasterError> {
        let status = self
            .connection
            .query_row(
                "SELECT status FROM master_attempts WHERE attempt_id = ?1",
                [attempt_id.0.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .ok_or(MasterError::AttemptNotFound)?;
        AttemptStatus::parse(&status)
    }

    fn migrate(&self) -> Result<(), MasterError> {
        let version = self.schema_version()?;
        if version == 0 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;\n\
                 CREATE TABLE master_metadata (\n\
                   key TEXT PRIMARY KEY NOT NULL,\n\
                   integer_value INTEGER NOT NULL\n\
                 );\n\
                 INSERT INTO master_metadata (key, integer_value) VALUES ('next_connection_epoch', 0);\n\
                 CREATE TABLE master_devices (\n\
                   device_id TEXT PRIMARY KEY NOT NULL,\n\
                   device_name TEXT NOT NULL,\n\
                   role_json TEXT NOT NULL,\n\
                   registry_revision INTEGER NOT NULL,\n\
                   capabilities_json TEXT NOT NULL,\n\
                   revoked INTEGER NOT NULL CHECK (revoked IN (0, 1))\n\
                 );\n\
                 CREATE TABLE master_connections (\n\
                   device_id TEXT PRIMARY KEY NOT NULL REFERENCES master_devices(device_id),\n\
                   connection_epoch INTEGER NOT NULL UNIQUE,\n\
                   active INTEGER NOT NULL CHECK (active IN (0, 1)),\n\
                   last_sequence INTEGER NOT NULL,\n\
                   connected_at_ms INTEGER NOT NULL,\n\
                   disconnected_at_ms INTEGER\n\
                 );\n\
                 CREATE TABLE master_steps (\n\
                   task_id TEXT NOT NULL,\n\
                   step_id TEXT PRIMARY KEY NOT NULL,\n\
                   status TEXT NOT NULL,\n\
                   capability_id TEXT NOT NULL,\n\
                   sensitivity_json TEXT NOT NULL,\n\
                   context_json TEXT NOT NULL,\n\
                   context_sha256 BLOB NOT NULL,\n\
                   lease_duration_ms INTEGER NOT NULL,\n\
                   deadline_after_ms INTEGER NOT NULL,\n\
                   created_at_ms INTEGER NOT NULL,\n\
                   accepted_payload_json TEXT,\n\
                   accepted_payload_sha256 BLOB,\n\
                   completed_at_ms INTEGER\n\
                 );\n\
                 CREATE TABLE master_attempts (\n\
                   attempt_id TEXT PRIMARY KEY NOT NULL,\n\
                   step_id TEXT NOT NULL REFERENCES master_steps(step_id),\n\
                   device_id TEXT NOT NULL REFERENCES master_devices(device_id),\n\
                   connection_epoch INTEGER NOT NULL,\n\
                   lease_id TEXT NOT NULL UNIQUE,\n\
                   cancellation_id TEXT NOT NULL UNIQUE,\n\
                   status TEXT NOT NULL,\n\
                   job_json TEXT NOT NULL,\n\
                   leased_at_ms INTEGER NOT NULL,\n\
                   lease_expires_at_ms INTEGER NOT NULL,\n\
                   completed_at_ms INTEGER,\n\
                   result_sequence INTEGER,\n\
                   payload_sha256 BLOB\n\
                 );\n\
                 CREATE INDEX master_steps_status_created_idx\n\
                   ON master_steps(status, created_at_ms, step_id);\n\
                 CREATE INDEX master_attempts_status_device_idx\n\
                   ON master_attempts(status, device_id, connection_epoch);\n\
                 CREATE TABLE master_identity_authority (\n\
                   authority_id INTEGER PRIMARY KEY NOT NULL CHECK (authority_id = 1),\n\
                   ca_fingerprint_sha256 BLOB NOT NULL UNIQUE,\n\
                   created_at_ms INTEGER NOT NULL,\n\
                   not_after_ms INTEGER NOT NULL,\n\
                   key_protection TEXT NOT NULL\n\
                 );\n\
                 CREATE TABLE master_enrollment_grants (\n\
                   grant_id TEXT PRIMARY KEY NOT NULL,\n\
                   operation TEXT NOT NULL CHECK (operation IN ('enroll', 'rotate')),\n\
                   secret_sha256 BLOB NOT NULL,\n\
                   device_id TEXT NOT NULL,\n\
                   device_name TEXT NOT NULL,\n\
                   role_json TEXT NOT NULL,\n\
                   registry_revision INTEGER NOT NULL,\n\
                   capabilities_json TEXT NOT NULL,\n\
                   created_at_ms INTEGER NOT NULL,\n\
                   expires_at_ms INTEGER NOT NULL,\n\
                   consumed_at_ms INTEGER,\n\
                   CHECK (expires_at_ms > created_at_ms)\n\
                 );\n\
                 CREATE INDEX master_enrollment_grants_pending_idx\n\
                   ON master_enrollment_grants(consumed_at_ms, expires_at_ms);\n\
                 CREATE TABLE master_device_certificates (\n\
                   serial_hex TEXT PRIMARY KEY NOT NULL,\n\
                   device_id TEXT NOT NULL REFERENCES master_devices(device_id),\n\
                   certificate_sha256 BLOB NOT NULL UNIQUE,\n\
                   issued_at_ms INTEGER NOT NULL,\n\
                   not_after_ms INTEGER NOT NULL,\n\
                   revoked_at_ms INTEGER,\n\
                   revocation_reason TEXT,\n\
                   replaced_by_serial_hex TEXT REFERENCES master_device_certificates(serial_hex),\n\
                   CHECK ((revoked_at_ms IS NULL AND revocation_reason IS NULL) OR\n\
                          (revoked_at_ms IS NOT NULL AND revocation_reason IS NOT NULL))\n\
                 );\n\
                 CREATE INDEX master_device_certificates_active_idx\n\
                   ON master_device_certificates(device_id, revoked_at_ms);\n\
                 CREATE TABLE master_event_stream (\n\
                   singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),\n\
                   stream_id TEXT NOT NULL UNIQUE,\n\
                   next_sequence INTEGER NOT NULL CHECK (next_sequence >= 0)\n\
                 );\n\
                 INSERT INTO master_event_stream (singleton, stream_id, next_sequence)\n\
                   VALUES (1, lower(substr(hex(randomblob(16)), 1, 8) || '-' ||\n\
                     substr(hex(randomblob(16)), 1, 4) || '-4' ||\n\
                     substr(hex(randomblob(16)), 1, 3) || '-8' ||\n\
                     substr(hex(randomblob(16)), 1, 3) || '-' ||\n\
                     substr(hex(randomblob(16)), 1, 12)), 0);\n\
                 CREATE TABLE master_events (\n\
                   sequence INTEGER PRIMARY KEY NOT NULL CHECK (sequence > 0),\n\
                   occurred_at_ms INTEGER NOT NULL CHECK (occurred_at_ms > 0),\n\
                   kind_json TEXT NOT NULL,\n\
                   task_id TEXT,\n\
                   step_id TEXT,\n\
                   device_id TEXT,\n\
                   connection_epoch INTEGER\n\
                 );\n\
                 PRAGMA user_version = 3;\n\
                 COMMIT;",
            )?;
            return Ok(());
        }
        if version == 1 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;\n\
                 CREATE TABLE master_identity_authority (\n\
                   authority_id INTEGER PRIMARY KEY NOT NULL CHECK (authority_id = 1),\n\
                   ca_fingerprint_sha256 BLOB NOT NULL UNIQUE,\n\
                   created_at_ms INTEGER NOT NULL,\n\
                   not_after_ms INTEGER NOT NULL,\n\
                   key_protection TEXT NOT NULL\n\
                 );\n\
                 CREATE TABLE master_enrollment_grants (\n\
                   grant_id TEXT PRIMARY KEY NOT NULL,\n\
                   operation TEXT NOT NULL CHECK (operation IN ('enroll', 'rotate')),\n\
                   secret_sha256 BLOB NOT NULL,\n\
                   device_id TEXT NOT NULL,\n\
                   device_name TEXT NOT NULL,\n\
                   role_json TEXT NOT NULL,\n\
                   registry_revision INTEGER NOT NULL,\n\
                   capabilities_json TEXT NOT NULL,\n\
                   created_at_ms INTEGER NOT NULL,\n\
                   expires_at_ms INTEGER NOT NULL,\n\
                   consumed_at_ms INTEGER,\n\
                   CHECK (expires_at_ms > created_at_ms)\n\
                 );\n\
                 CREATE INDEX master_enrollment_grants_pending_idx\n\
                   ON master_enrollment_grants(consumed_at_ms, expires_at_ms);\n\
                 CREATE TABLE master_device_certificates (\n\
                   serial_hex TEXT PRIMARY KEY NOT NULL,\n\
                   device_id TEXT NOT NULL REFERENCES master_devices(device_id),\n\
                   certificate_sha256 BLOB NOT NULL UNIQUE,\n\
                   issued_at_ms INTEGER NOT NULL,\n\
                   not_after_ms INTEGER NOT NULL,\n\
                   revoked_at_ms INTEGER,\n\
                   revocation_reason TEXT,\n\
                   replaced_by_serial_hex TEXT REFERENCES master_device_certificates(serial_hex),\n\
                   CHECK ((revoked_at_ms IS NULL AND revocation_reason IS NULL) OR\n\
                          (revoked_at_ms IS NOT NULL AND revocation_reason IS NOT NULL))\n\
                 );\n\
                 CREATE INDEX master_device_certificates_active_idx\n\
                   ON master_device_certificates(device_id, revoked_at_ms);\n\
                 PRAGMA user_version = 2;\n\
                 COMMIT;",
            )?;
        }
        let version = self.schema_version()?;
        if version == 2 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;\n\
                 CREATE TABLE master_event_stream (\n\
                   singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),\n\
                   stream_id TEXT NOT NULL UNIQUE,\n\
                   next_sequence INTEGER NOT NULL CHECK (next_sequence >= 0)\n\
                 );\n\
                 INSERT INTO master_event_stream (singleton, stream_id, next_sequence)\n\
                   VALUES (1, lower(substr(hex(randomblob(16)), 1, 8) || '-' ||\n\
                     substr(hex(randomblob(16)), 1, 4) || '-4' ||\n\
                     substr(hex(randomblob(16)), 1, 3) || '-8' ||\n\
                     substr(hex(randomblob(16)), 1, 3) || '-' ||\n\
                     substr(hex(randomblob(16)), 1, 12)), 0);\n\
                 CREATE TABLE master_events (\n\
                   sequence INTEGER PRIMARY KEY NOT NULL CHECK (sequence > 0),\n\
                   occurred_at_ms INTEGER NOT NULL CHECK (occurred_at_ms > 0),\n\
                   kind_json TEXT NOT NULL,\n\
                   task_id TEXT,\n\
                   step_id TEXT,\n\
                   device_id TEXT,\n\
                   connection_epoch INTEGER\n\
                 );\n\
                 PRAGMA user_version = 3;\n\
                 COMMIT;",
            )?;
            return Ok(());
        }
        if version != MASTER_SCHEMA_VERSION {
            return Err(MasterError::UnsupportedSchemaVersion {
                expected: MASTER_SCHEMA_VERSION,
                found: version,
            });
        }
        Ok(())
    }

    fn reconcile_interrupted_state(
        &mut self,
        now_ms: u64,
    ) -> Result<StartupReconciliation, MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let active_connections = active_connections(&tx)?;
        let leased_steps = leased_steps(&tx, None)?;
        let abandoned_attempts = tx.execute(
            "UPDATE master_attempts SET status = 'abandoned', completed_at_ms = ?1\n             WHERE status = 'leased'",
            [u64_to_i64(now_ms)?],
        )?;
        let mut disconnected_connections = 0_u64;
        for (device_id, connection_epoch) in active_connections {
            let changed = tx.execute(
                "UPDATE master_connections SET active = 0, disconnected_at_ms = ?1\n\
                 WHERE device_id = ?2 AND connection_epoch = ?3 AND active = 1",
                params![
                    u64_to_i64(now_ms)?,
                    device_id.0.to_string(),
                    u64_to_i64(connection_epoch)?,
                ],
            )?;
            if changed == 1 {
                disconnected_connections += 1;
                append_distributed_event_tx(
                    &tx,
                    DistributedEventKind::DeviceDisconnected,
                    now_ms,
                    DistributedEventIdentity {
                        task_id: None,
                        step_id: None,
                        device_id: Some(device_id),
                        connection_epoch: Some(connection_epoch),
                    },
                )?;
            }
        }
        let mut requeued_steps = 0_u64;
        for (task_id, step_id) in leased_steps {
            let changed = tx.execute(
                "UPDATE master_steps SET status = 'queued' WHERE step_id = ?1 AND status = 'leased'",
                [step_id.0.to_string()],
            )?;
            if changed == 1 {
                requeued_steps += 1;
                append_distributed_event_tx(
                    &tx,
                    DistributedEventKind::StepQueued,
                    now_ms,
                    DistributedEventIdentity {
                        task_id: Some(task_id),
                        step_id: Some(step_id),
                        device_id: None,
                        connection_epoch: None,
                    },
                )?;
            }
        }
        tx.commit()?;
        Ok(StartupReconciliation {
            disconnected_connections,
            abandoned_attempts: abandoned_attempts as u64,
            requeued_steps,
        })
    }
}

#[derive(Debug)]
struct ConnectionState {
    epoch: u64,
    active: bool,
    last_sequence: u64,
}

#[derive(Debug)]
struct StoredStep {
    task_id: TaskId,
    step_id: StepId,
    capability_id: String,
    sensitivity_json: String,
    context_json: String,
    context_sha256: [u8; 32],
    lease_duration_ms: u64,
    deadline_after_ms: u64,
}

#[derive(Debug)]
struct StoredAttempt {
    device_id: DeviceId,
    status: AttemptStatus,
    job_json: String,
    lease_expires_at_ms: u64,
}

#[derive(Debug, Clone, Copy)]
struct DistributedEventIdentity {
    task_id: Option<TaskId>,
    step_id: Option<StepId>,
    device_id: Option<DeviceId>,
    connection_epoch: Option<u64>,
}

fn append_distributed_event_tx(
    tx: &Transaction<'_>,
    kind: DistributedEventKind,
    occurred_at_ms: u64,
    identity: DistributedEventIdentity,
) -> Result<DistributedEvent, MasterError> {
    let (stream_id_text, current_sequence): (String, i64) = tx.query_row(
        "SELECT stream_id, next_sequence FROM master_event_stream WHERE singleton = 1",
        [],
        |row| Ok((row.get(0)?, row.get(1)?)),
    )?;
    let stream_id = parse_uuid(&stream_id_text)?;
    let next_sequence = current_sequence
        .checked_add(1)
        .ok_or(MasterError::IntegerOutOfRange)?;
    let event = DistributedEvent {
        protocol_version: PROTOCOL_VERSION,
        cursor: DistributedEventCursor {
            stream_id,
            sequence: i64_to_u64(next_sequence)?,
        },
        occurred_at_ms,
        kind,
        task_id: identity.task_id,
        step_id: identity.step_id,
        device_id: identity.device_id,
        connection_epoch: identity.connection_epoch,
    };
    event.validate()?;
    let changed = tx.execute(
        "UPDATE master_event_stream SET next_sequence = ?1\n\
         WHERE singleton = 1 AND next_sequence = ?2",
        params![next_sequence, current_sequence],
    )?;
    if changed != 1 {
        return Err(MasterError::InvalidStoredState(
            "master event cursor changed before append".to_string(),
        ));
    }
    tx.execute(
        "INSERT INTO master_events\n\
         (sequence, occurred_at_ms, kind_json, task_id, step_id, device_id, connection_epoch)\n\
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            next_sequence,
            u64_to_i64(occurred_at_ms)?,
            serde_json::to_string(&kind)?,
            identity.task_id.map(|value| value.0.to_string()),
            identity.step_id.map(|value| value.0.to_string()),
            identity.device_id.map(|value| value.0.to_string()),
            identity.connection_epoch.map(u64_to_i64).transpose()?,
        ],
    )?;
    Ok(event)
}

fn validate_new_step(step: &NewStep) -> Result<(), MasterError> {
    if step.task_id.0.is_nil() || step.step_id.0.is_nil() {
        return Err(MasterError::NilStepIdentifier);
    }
    if step.capability_id.is_empty()
        || step.capability_id.len() > MAX_CAPABILITY_ID_BYTES
        || step.capability_id.trim() != step.capability_id
        || !step
            .capability_id
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && !byte.is_ascii_control())
    {
        return Err(MasterError::InvalidCapabilityIdentifier);
    }
    if !step.context.is_object() {
        return Err(MasterError::InvalidStepContext);
    }
    if serde_json::to_vec(&step.context)?.len() > MAX_JOB_CONTEXT_BYTES {
        return Err(MasterError::InvalidStepContext);
    }
    if step.lease_duration_ms == 0 || step.lease_duration_ms > MAX_LEASE_DURATION_MS {
        return Err(MasterError::InvalidLeaseDuration);
    }
    if step.deadline_after_ms == 0 || step.deadline_after_ms > MAX_STEP_DEADLINE_MS {
        return Err(MasterError::InvalidStepDeadline);
    }
    Ok(())
}

fn rejected_handshake(
    accepted_registry_revision: u64,
    reason: &str,
) -> Result<HandshakeResponse, MasterError> {
    let response = HandshakeResponse {
        protocol_version: PROTOCOL_VERSION,
        status: HandshakeStatus::Rejected,
        connection_epoch: 0,
        accepted_registry_revision,
        reason_code: Some(reason.to_string()),
    };
    response.validate()?;
    Ok(response)
}

fn capabilities_match(expected: &[CapabilityDescriptor], actual: &[CapabilityDescriptor]) -> bool {
    expected.len() == actual.len()
        && expected.iter().all(|expected_capability| {
            actual
                .iter()
                .any(|actual_capability| actual_capability == expected_capability)
        })
}

fn connection_state(
    tx: &Transaction<'_>,
    device_id: DeviceId,
) -> Result<ConnectionState, MasterError> {
    tx.query_row(
        "SELECT connection_epoch, active, last_sequence\n         FROM master_connections WHERE device_id = ?1",
        [device_id.0.to_string()],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        },
    )
    .optional()?
    .map(|(epoch, active, last_sequence)| {
        Ok::<ConnectionState, MasterError>(ConnectionState {
            epoch: i64_to_u64(epoch)?,
            active: active != 0,
            last_sequence: i64_to_u64(last_sequence)?,
        })
    })
    .transpose()?
    .ok_or(MasterError::ConnectionNotActive)
}

fn active_connection_epoch(
    tx: &Transaction<'_>,
    device_id: DeviceId,
) -> Result<Option<u64>, MasterError> {
    tx.query_row(
        "SELECT connection_epoch FROM master_connections\n         WHERE device_id = ?1 AND active = 1",
        [device_id.0.to_string()],
        |row| row.get::<_, i64>(0),
    )
    .optional()?
    .map(i64_to_u64)
    .transpose()
}

fn disconnect_device_tx(
    tx: &Transaction<'_>,
    device_id: DeviceId,
    connection_epoch: u64,
    now_ms: u64,
) -> Result<StartupReconciliation, MasterError> {
    let connection = connection_state(tx, device_id)?;
    if !connection.active {
        return Err(MasterError::ConnectionNotActive);
    }
    if connection.epoch != connection_epoch {
        return Err(MasterError::ConnectionEpochMismatch);
    }
    let leased_steps = leased_steps(tx, Some((device_id, connection_epoch)))?;
    let abandoned_attempts = tx.execute(
        "UPDATE master_attempts SET status = 'abandoned', completed_at_ms = ?1\n         WHERE device_id = ?2 AND connection_epoch = ?3 AND status = 'leased'",
        params![
            u64_to_i64(now_ms)?,
            device_id.0.to_string(),
            u64_to_i64(connection_epoch)?,
        ],
    )?;
    let disconnected_connections = tx.execute(
        "UPDATE master_connections SET active = 0, disconnected_at_ms = ?1\n         WHERE device_id = ?2 AND connection_epoch = ?3 AND active = 1",
        params![
            u64_to_i64(now_ms)?,
            device_id.0.to_string(),
            u64_to_i64(connection_epoch)?,
        ],
    )?;
    if disconnected_connections == 1 {
        append_distributed_event_tx(
            tx,
            DistributedEventKind::DeviceDisconnected,
            now_ms,
            DistributedEventIdentity {
                task_id: None,
                step_id: None,
                device_id: Some(device_id),
                connection_epoch: Some(connection_epoch),
            },
        )?;
    }
    let mut requeued_steps = 0_u64;
    for (task_id, step_id) in leased_steps {
        let changed = tx.execute(
            "UPDATE master_steps SET status = 'queued' WHERE step_id = ?1 AND status = 'leased'",
            [step_id.0.to_string()],
        )?;
        if changed == 1 {
            requeued_steps += 1;
            append_distributed_event_tx(
                tx,
                DistributedEventKind::StepQueued,
                now_ms,
                DistributedEventIdentity {
                    task_id: Some(task_id),
                    step_id: Some(step_id),
                    device_id: None,
                    connection_epoch: None,
                },
            )?;
        }
    }
    Ok(StartupReconciliation {
        disconnected_connections: disconnected_connections as u64,
        abandoned_attempts: abandoned_attempts as u64,
        requeued_steps,
    })
}

fn reconcile_expired_leases_tx(
    tx: &Transaction<'_>,
    now_ms: u64,
) -> Result<LeaseReconciliation, MasterError> {
    let now = u64_to_i64(now_ms)?;
    let mut statement = tx.prepare(
        "SELECT s.task_id, a.step_id FROM master_attempts a\n\
         JOIN master_steps s ON s.step_id = a.step_id\n\
         WHERE a.status = 'leased' AND a.lease_expires_at_ms <= ?1",
    )?;
    let leased_steps = statement
        .query_map([now], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    let expired_attempts = tx.execute(
        "UPDATE master_attempts SET status = 'expired', completed_at_ms = ?1\n         WHERE status = 'leased' AND lease_expires_at_ms <= ?1",
        [now],
    )?;
    let mut requeued_steps = 0_u64;
    for (task_id, step_id) in leased_steps {
        let task_id = TaskId::new(parse_uuid(&task_id)?);
        let step_id = StepId::new(parse_uuid(&step_id)?);
        let changed = tx.execute(
            "UPDATE master_steps SET status = 'queued' WHERE step_id = ?1 AND status = 'leased'",
            [step_id.0.to_string()],
        )?;
        if changed == 1 {
            requeued_steps += 1;
            append_distributed_event_tx(
                tx,
                DistributedEventKind::StepQueued,
                now_ms,
                DistributedEventIdentity {
                    task_id: Some(task_id),
                    step_id: Some(step_id),
                    device_id: None,
                    connection_epoch: None,
                },
            )?;
        }
    }
    Ok(LeaseReconciliation {
        expired_attempts: expired_attempts as u64,
        requeued_steps,
    })
}

fn capability_for_device(
    tx: &Transaction<'_>,
    device_id: DeviceId,
    capability_id: &str,
) -> Result<CapabilityDescriptor, MasterError> {
    let capabilities_json: String = tx.query_row(
        "SELECT capabilities_json FROM master_devices WHERE device_id = ?1 AND revoked = 0",
        [device_id.0.to_string()],
        |row| row.get(0),
    )?;
    let capabilities: Vec<CapabilityDescriptor> = serde_json::from_str(&capabilities_json)?;
    capabilities
        .into_iter()
        .find(|capability| capability.id == capability_id)
        .ok_or(MasterError::InvalidStoredState(
            "leased capability is absent from the registered device".to_string(),
        ))
}

fn active_connections(tx: &Transaction<'_>) -> Result<Vec<(DeviceId, u64)>, MasterError> {
    let mut statement = tx.prepare(
        "SELECT device_id, connection_epoch FROM master_connections\n\
         WHERE active = 1 ORDER BY device_id ASC",
    )?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(device_id, connection_epoch)| {
            Ok((
                DeviceId::new(parse_uuid(&device_id)?),
                i64_to_u64(connection_epoch)?,
            ))
        })
        .collect()
}

fn leased_steps(
    tx: &Transaction<'_>,
    connection: Option<(DeviceId, u64)>,
) -> Result<Vec<(TaskId, StepId)>, MasterError> {
    let (sql, parameters): (&str, Vec<rusqlite::types::Value>) = match connection {
        Some((device_id, epoch)) => (
            "SELECT s.task_id, a.step_id FROM master_attempts a\n\
             JOIN master_steps s ON s.step_id = a.step_id\n\
             WHERE a.status = 'leased' AND a.device_id = ?1 AND a.connection_epoch = ?2",
            vec![device_id.0.to_string().into(), u64_to_i64(epoch)?.into()],
        ),
        None => (
            "SELECT s.task_id, a.step_id FROM master_attempts a\n\
             JOIN master_steps s ON s.step_id = a.step_id WHERE a.status = 'leased'",
            Vec::new(),
        ),
    };
    let mut statement = tx.prepare(sql)?;
    let rows = statement
        .query_map(rusqlite::params_from_iter(parameters), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(task_id, step_id)| {
            Ok((
                TaskId::new(parse_uuid(&task_id)?),
                StepId::new(parse_uuid(&step_id)?),
            ))
        })
        .collect()
}

fn load_queued_steps(tx: &Transaction<'_>) -> Result<Vec<StoredStep>, MasterError> {
    let mut statement = tx.prepare(
        "SELECT task_id, step_id, capability_id, sensitivity_json, context_json,\n                context_sha256, lease_duration_ms, deadline_after_ms\n         FROM master_steps WHERE status = 'queued'\n         ORDER BY created_at_ms ASC, step_id ASC",
    )?;
    let raw = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, Vec<u8>>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    raw.into_iter()
        .map(
            |(
                task_id,
                step_id,
                capability_id,
                sensitivity_json,
                context_json,
                context_sha256,
                lease_duration_ms,
                deadline_after_ms,
            )| {
                Ok(StoredStep {
                    task_id: TaskId::new(parse_uuid(&task_id)?),
                    step_id: StepId::new(parse_uuid(&step_id)?),
                    capability_id,
                    sensitivity_json,
                    context_json,
                    context_sha256: digest_array(&context_sha256)?,
                    lease_duration_ms: i64_to_u64(lease_duration_ms)?,
                    deadline_after_ms: i64_to_u64(deadline_after_ms)?,
                })
            },
        )
        .collect()
}

fn load_attempt(
    tx: &Transaction<'_>,
    attempt_id: AttemptId,
) -> Result<Option<StoredAttempt>, MasterError> {
    tx.query_row(
        "SELECT device_id, status, job_json, lease_expires_at_ms\n         FROM master_attempts WHERE attempt_id = ?1",
        [attempt_id.0.to_string()],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        },
    )
    .optional()?
    .map(|(device_id, status, job_json, lease_expires_at_ms)| {
        Ok(StoredAttempt {
            device_id: DeviceId::new(parse_uuid(&device_id)?),
            status: AttemptStatus::parse(&status)?,
            job_json,
            lease_expires_at_ms: i64_to_u64(lease_expires_at_ms)?,
        })
    })
    .transpose()
}

fn step_status_tx(tx: &Transaction<'_>, step_id: StepId) -> Result<StepStatus, MasterError> {
    let status = tx
        .query_row(
            "SELECT status FROM master_steps WHERE step_id = ?1",
            [step_id.0.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .ok_or(MasterError::StepNotFound)?;
    StepStatus::parse(&status)
}

fn json_sha256(value: &Value) -> Result<[u8; 32], MasterError> {
    Ok(Sha256::digest(serde_json::to_vec(value)?).into())
}

fn digest_array(value: &[u8]) -> Result<[u8; 32], MasterError> {
    value
        .try_into()
        .map_err(|_| MasterError::InvalidStoredState("SHA-256 digest is not 32 bytes".to_string()))
}

fn parse_uuid(value: &str) -> Result<Uuid, MasterError> {
    Uuid::parse_str(value)
        .map_err(|_| MasterError::InvalidStoredState("stored UUID is invalid".to_string()))
}

fn u64_to_i64(value: u64) -> Result<i64, MasterError> {
    i64::try_from(value).map_err(|_| MasterError::IntegerOutOfRange)
}

fn i64_to_u64(value: i64) -> Result<u64, MasterError> {
    u64::try_from(value).map_err(|_| MasterError::IntegerOutOfRange)
}

fn is_constraint_violation(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(code, _)
            if code.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

pub fn current_time_ms() -> Result<u64, MasterError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| MasterError::InvalidSystemClock)?;
    u64::try_from(duration.as_millis()).map_err(|_| MasterError::InvalidSystemClock)
}
