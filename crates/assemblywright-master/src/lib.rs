use assemblywright_protocol::{
    AttemptId, CancellationAcknowledgement, CancellationId, CancellationInstruction,
    CapabilityDescriptor, ContextHandlingPolicy, DeviceId, DeviceRole, DistributedEvent,
    DistributedEventBatch, DistributedEventBatchRequest, DistributedEventCursor,
    DistributedEventKind, FeatureConveyorApprovedSpecification, FeatureConveyorRepositoryGrantSet,
    FeatureConveyorRepositoryGrantView, HandshakeRequest, HandshakeResponse, HandshakeStatus,
    JobEnvelope, JobResultEnvelope, JobResultStatus, LeaseId, ProtocolError, Sensitivity, StepId,
    TaskId, CANCELLATION_ACK_DEADLINE_MS, FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
    FIXTURE_REASONING_CAPABILITY_ID, MAX_CAPABILITY_ID_BYTES, MAX_JOB_CONTEXT_BYTES,
    MAX_LEASE_DURATION_MS, MAX_STEP_DEADLINE_MS, MLX_REASONING_CAPABILITY_ID, PROTOCOL_VERSION,
};
use rusqlite::{
    params, Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior,
};
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
    CapabilityRebindAcknowledgement, CapabilityRebindActivation, EnrollmentGrantReceipt,
    EnrollmentGrantSpec, EnrollmentOperation, EnrollmentRequest, EphemeralServerIdentity,
    IdentityAuthority, IdentityAuthorityReceipt, IdentityError, IssuedDeviceCertificate,
    PendingCapabilityRebindCertificate, PlatformSecretProtector, SecretProtector,
    DEVICE_CERTIFICATE_LIFETIME_MS, ENROLLMENT_GRANT_TTL_MS, MAX_ENROLLED_DEVICES,
    SERVER_CERTIFICATE_LIFETIME_MS,
};

pub const MASTER_SCHEMA_VERSION: i64 = 8;
pub const MAX_QUEUED_OR_LEASED_STEPS: u64 = 256;
pub const MAX_CONCURRENT_JOBS: u64 = 4;
pub const MAX_CONVEYOR_NONTERMINAL_FEATURES: u64 = 100;
pub const MAX_CONVEYOR_STATUS_FEATURES: usize = 100;
pub const MAX_APPROVED_FEATURE_SPECIFICATION_BYTES: usize = 256 * 1024;

const REASON_UNKNOWN_DEVICE: &str = "unknown_device";
const REASON_REVOKED_DEVICE: &str = "revoked_device";
const REASON_REGISTRY_MISMATCH: &str = "registry_mismatch";
const REASON_IDENTITY_MISMATCH: &str = "identity_mismatch";
const REASON_CAPABILITY_MISMATCH: &str = "capability_mismatch";
const REASON_DUPLICATE_ACTIVE: &str = "duplicate_active_connection";
const FEATURE_CONVEYOR_STATUS_COUNTS_SQL: &str = "
    WITH visible_features AS (
      SELECT f.status
      FROM feature_conveyor_queue q
      JOIN feature_conveyor_features f ON f.feature_id = q.feature_id
      UNION ALL
      SELECT f.status
      FROM feature_active_lease l
      JOIN feature_conveyor_features f ON f.feature_id = l.feature_id
      WHERE NOT EXISTS (
        SELECT 1 FROM feature_conveyor_queue q
        WHERE q.feature_id = l.feature_id
      )
    )
    SELECT
      COUNT(*),
      COALESCE(SUM(CASE WHEN status = 'queued' THEN 1 ELSE 0 END), 0),
      COALESCE(SUM(CASE WHEN status = 'implementing' THEN 1 ELSE 0 END), 0),
      COALESCE(SUM(CASE WHEN status = 'validating' THEN 1 ELSE 0 END), 0),
      COALESCE(SUM(CASE WHEN status = 'reviewing' THEN 1 ELSE 0 END), 0),
      COALESCE(SUM(CASE WHEN status = 'publishing' THEN 1 ELSE 0 END), 0),
      COALESCE(SUM(CASE WHEN status = 'verifying_main' THEN 1 ELSE 0 END), 0),
      COALESCE(SUM(CASE WHEN status = 'succeeded' THEN 1 ELSE 0 END), 0),
      COALESCE(SUM(CASE WHEN status = 'cancelled' THEN 1 ELSE 0 END), 0),
      COALESCE(SUM(CASE WHEN status = 'abandoned' THEN 1 ELSE 0 END), 0),
      COALESCE(SUM(CASE WHEN status = 'quarantined' THEN 1 ELSE 0 END), 0)
    FROM visible_features";
const FEATURE_CONVEYOR_STATUS_ENTRIES_SQL: &str = "
    WITH visible_features AS (
      SELECT f.feature_id, f.current_specification_revision,
             f.lifecycle_revision, f.queue_position, f.status,
             CASE WHEN l.feature_id IS NULL THEN 0 ELSE 1 END AS lease_present,
             f.effect_possible
      FROM feature_conveyor_queue q
      JOIN feature_conveyor_features f ON f.feature_id = q.feature_id
      LEFT JOIN feature_active_lease l ON l.feature_id = f.feature_id
      UNION ALL
      SELECT f.feature_id, f.current_specification_revision,
             f.lifecycle_revision, f.queue_position, f.status,
             1 AS lease_present, f.effect_possible
      FROM feature_active_lease l
      JOIN feature_conveyor_features f ON f.feature_id = l.feature_id
      WHERE NOT EXISTS (
        SELECT 1 FROM feature_conveyor_queue q
        WHERE q.feature_id = l.feature_id
      )
    )
    SELECT feature_id, current_specification_revision,
           lifecycle_revision, queue_position, status,
           lease_present, effect_possible
    FROM visible_features
    ORDER BY queue_position ASC, feature_id ASC
    LIMIT ?1";

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
    #[error("authoritative emergency pause blocks distributed work")]
    EmergencyPaused,
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
    #[error("authenticated device does not own this attempt")]
    ResultDeviceMismatch,
    #[error("attempt is not accepting results from status {0:?}")]
    ResultNotAccepting(AttemptStatus),
    #[error("result sequence is not newer than the connection high-water mark")]
    SequenceReplay,
    #[error("the result arrived after its lease expired")]
    LeaseExpired,
    #[error("cancellation acknowledgement deadline expired")]
    CancellationExpired,
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
    #[error("another assemblywright-master process already owns {lock_path}")]
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
    #[error("device registration does not declare one exact supported remote work capability")]
    InvalidRemoteWorkContract,
    #[error("feature conveyor input is invalid: {0}")]
    InvalidFeatureConveyorInput(String),
    #[error("feature conveyor queue capacity has been reached")]
    FeatureQueueFull,
    #[error("feature conveyor queue revision is stale: expected {expected}, found {found}")]
    StaleFeatureQueueRevision { expected: u64, found: u64 },
    #[error("feature conveyor owner-control designation revision is stale: expected {expected}, found {found}")]
    StaleOwnerControlDesignationRevision { expected: u64, found: u64 },
    #[error(
        "feature conveyor emergency-pause revision is stale: expected {expected}, found {found}"
    )]
    StaleEmergencyPauseRevision { expected: u64, found: u64 },
    #[error("no owner-control Mac bridge is designated")]
    OwnerControlBridgeNotDesignated,
    #[error("the authenticated device is not the exact designated owner-control Mac bridge")]
    OwnerControlBridgeUnauthorized,
    #[error("feature lifecycle revision is stale: expected {expected}, found {found}")]
    StaleFeatureLifecycleRevision { expected: u64, found: u64 },
    #[error("feature was not found")]
    FeatureNotFound,
    #[error("feature specification revision is immutable or already present")]
    FeatureSpecificationImmutable,
    #[error("repository grant revision is immutable or already present")]
    RepositoryGrantImmutable,
    #[error("repository grant revision is stale: expected {expected}, found {found}")]
    StaleRepositoryGrantRevision { expected: u64, found: u64 },
    #[error("repository grant is absent, revoked, expired, or does not match the approved specification")]
    RepositoryGrantUnavailable,
    #[error("the strict queue head is dependency-blocked")]
    FeatureDependencyBlocked,
    #[error("a feature already owns the singleton active lease")]
    FeatureLeaseAlreadyActive,
    #[error("the requested feature lifecycle transition is invalid")]
    InvalidFeatureTransition,
    #[error(
        "feature cancellation retains the active lease and requires explicit safe abandonment"
    )]
    FeatureCancellationBlocksAdvancement,
    #[error("verified healthy main evidence is required")]
    VerifiedHealthyMainRequired,
    #[error("feature migration backup failed: {0}")]
    MigrationBackup(String),
    #[error("feature migration failed and backup restoration also failed: migration={migration}; restore={restore}")]
    MigrationAndRestoreFailed { migration: String, restore: String },
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteWorkContract {
    Fixture,
    Mlx(CapabilityDescriptor),
}

impl RemoteWorkContract {
    pub fn from_registration(registration: &DeviceRegistration) -> Result<Self, MasterError> {
        if !matches!(
            registration.role,
            DeviceRole::MacBridge | DeviceRole::InferenceWorker
        ) || registration.capabilities.len() != 1
        {
            return Err(MasterError::InvalidRemoteWorkContract);
        }
        let capability = &registration.capabilities[0];
        capability.validate()?;
        if *capability == CapabilityDescriptor::fixture_reasoning() {
            return Ok(Self::Fixture);
        }
        if capability.id == MLX_REASONING_CAPABILITY_ID {
            return Ok(Self::Mlx(capability.clone()));
        }
        Err(MasterError::InvalidRemoteWorkContract)
    }

    fn capability(&self) -> CapabilityDescriptor {
        match self {
            Self::Fixture => CapabilityDescriptor::fixture_reasoning(),
            Self::Mlx(capability) => capability.clone(),
        }
    }

    fn validate_job(&self, job: &JobEnvelope) -> Result<(), MasterError> {
        match self {
            Self::Fixture => {
                job.validate_fixture_reasoning()?;
            }
            Self::Mlx(capability) => {
                job.validate_mlx_reasoning()?;
                if job.selected_model != capability.model {
                    return Err(MasterError::InvalidRemoteWorkContract);
                }
            }
        }
        Ok(())
    }

    fn validate_result(
        &self,
        result: &JobResultEnvelope,
        job: &JobEnvelope,
    ) -> Result<(), MasterError> {
        match self {
            Self::Fixture => result.validate_fixture_reasoning_result(job)?,
            Self::Mlx(capability) => {
                result.validate_mlx_reasoning_result(job)?;
                if job.selected_model != capability.model {
                    return Err(MasterError::InvalidRemoteWorkContract);
                }
            }
        }
        Ok(())
    }
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
    CancellationPending,
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
            Self::CancellationPending => "cancellation_pending",
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
            "cancellation_pending" => Ok(Self::CancellationPending),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptedCancellation {
    pub accepted: bool,
    pub status: StepStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryGrantKind {
    Registration,
    CloudDisclosure,
    AutonomousPublication,
}

impl RepositoryGrantKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Registration => "registration",
            Self::CloudDisclosure => "cloud_disclosure",
            Self::AutonomousPublication => "autonomous_publication",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryGrantRevision {
    pub repository_id: Uuid,
    pub kind: RepositoryGrantKind,
    pub revision: u64,
    pub scope_sha256: [u8; 32],
    pub owner_approval_sha256: [u8; 32],
    pub expires_at_ms: Option<u64>,
    pub revoked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureGrantRevisions {
    pub registration: u64,
    pub cloud_disclosure: u64,
    pub autonomous_publication: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovedFeatureSpecification {
    pub feature_id: Uuid,
    pub revision: u64,
    pub repository_id: Uuid,
    pub manifest: Value,
    pub manifest_sha256: [u8; 32],
    pub design_sha256: [u8; 32],
    pub brainstorming_sha256: [u8; 32],
    pub owner_approval_sha256: [u8; 32],
    pub grants: FeatureGrantRevisions,
    pub provider_id: String,
    pub model_id: String,
    pub dependencies: Vec<Uuid>,
}

impl From<FeatureConveyorApprovedSpecification> for ApprovedFeatureSpecification {
    fn from(specification: FeatureConveyorApprovedSpecification) -> Self {
        Self {
            feature_id: specification.feature_id,
            revision: specification.revision,
            repository_id: specification.repository_id,
            manifest: specification.manifest,
            manifest_sha256: specification.manifest_sha256,
            design_sha256: specification.design_sha256,
            brainstorming_sha256: specification.brainstorming_sha256,
            owner_approval_sha256: specification.owner_approval_sha256,
            grants: FeatureGrantRevisions {
                registration: specification.grants.registration,
                cloud_disclosure: specification.grants.cloud_disclosure,
                autonomous_publication: specification.grants.autonomous_publication,
            },
            provider_id: specification.provider_id,
            model_id: specification.model_id,
            dependencies: specification.dependencies,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerControlBridgeDesignation {
    pub device_id: DeviceId,
    pub registry_revision: u64,
    pub designation_revision: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureLifecycleStatus {
    Queued,
    Implementing,
    Validating,
    Reviewing,
    Publishing,
    VerifyingMain,
    Succeeded,
    Cancelled,
    Abandoned,
    Quarantined,
}

impl FeatureLifecycleStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Implementing => "implementing",
            Self::Validating => "validating",
            Self::Reviewing => "reviewing",
            Self::Publishing => "publishing",
            Self::VerifyingMain => "verifying_main",
            Self::Succeeded => "succeeded",
            Self::Cancelled => "cancelled",
            Self::Abandoned => "abandoned",
            Self::Quarantined => "quarantined",
        }
    }

    fn parse(value: &str) -> Result<Self, MasterError> {
        match value {
            "queued" => Ok(Self::Queued),
            "implementing" => Ok(Self::Implementing),
            "validating" => Ok(Self::Validating),
            "reviewing" => Ok(Self::Reviewing),
            "publishing" => Ok(Self::Publishing),
            "verifying_main" => Ok(Self::VerifyingMain),
            "succeeded" => Ok(Self::Succeeded),
            "cancelled" => Ok(Self::Cancelled),
            "abandoned" => Ok(Self::Abandoned),
            "quarantined" => Ok(Self::Quarantined),
            other => Err(MasterError::InvalidStoredState(format!(
                "unknown feature lifecycle status {other}"
            ))),
        }
    }

    fn is_active_execution(self) -> bool {
        matches!(
            self,
            Self::Implementing
                | Self::Validating
                | Self::Reviewing
                | Self::Publishing
                | Self::VerifyingMain
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureTransitionEvidence {
    pub repository_snapshot_sha256: [u8; 32],
    pub accepted_evidence_sha256: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifiedFeatureSuccess {
    pub main_commit_sha256: [u8; 32],
    pub post_merge_evidence_sha256: [u8; 32],
    pub main_healthy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureAbandonmentEvidence {
    pub safe_reconciliation_sha256: [u8; 32],
    pub merged: bool,
    pub verified_healthy_main_sha256: Option<[u8; 32]>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureSnapshot {
    pub feature_id: Uuid,
    pub specification_revision: u64,
    pub status: FeatureLifecycleStatus,
    pub lifecycle_revision: u64,
    pub queue_position: u64,
    pub active_lease_id: Option<Uuid>,
    pub effect_possible: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorStatusCounts {
    pub queued: u64,
    pub implementing: u64,
    pub validating: u64,
    pub reviewing: u64,
    pub publishing: u64,
    pub verifying_main: u64,
    pub succeeded: u64,
    pub cancelled: u64,
    pub abandoned: u64,
    pub quarantined: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorStatusEntry {
    pub feature_id: Uuid,
    pub specification_revision: u64,
    pub lifecycle_revision: u64,
    pub queue_position: u64,
    pub status: FeatureLifecycleStatus,
    pub lease_present: bool,
    pub effect_possible: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorGuidanceState {
    Idle,
    Ready,
    Blocked,
    InProgress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorGuidanceReason {
    QueueEmpty,
    HeadDependencySatisfied,
    HeadDependencyUnsatisfied,
    ActiveFeatureLeased,
    ActiveRequiresReconciliation,
    EmergencyPaused,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorNextOwnerAction {
    PrepareApprovedFeature,
    AwaitOwnerControlSurface,
    ResolveHeadDependency,
    Wait,
    ReconcileActiveFeature,
    ResumeEmergencyPause,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorOwnerGuidance {
    pub state: FeatureConveyorGuidanceState,
    pub reason_code: FeatureConveyorGuidanceReason,
    pub next_owner_action: FeatureConveyorNextOwnerAction,
    pub feature_id: Option<Uuid>,
    pub specification_revision: Option<u64>,
    pub lifecycle_revision: Option<u64>,
    pub queue_revision: u64,
    pub emergency_pause_revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorStatus {
    pub schema_version: i64,
    pub queue_revision: u64,
    pub startup_quarantine_count: u64,
    pub counts_by_status: FeatureConveyorStatusCounts,
    pub visible_feature_count: u64,
    pub features_truncated: bool,
    pub features: Vec<FeatureConveyorStatusEntry>,
    pub owner_guidance: FeatureConveyorOwnerGuidance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureClaim {
    pub feature_id: Uuid,
    pub specification_revision: u64,
    pub lifecycle_revision: u64,
    pub lease_id: Uuid,
    pub provider_id: String,
    pub model_id: String,
    pub grants: FeatureGrantRevisions,
}

pub struct MasterKernel {
    connection: Connection,
    startup_reconciliation: StartupReconciliation,
    feature_startup_quarantines: u64,
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
    migration_backup_path: Option<PathBuf>,
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
        let migration_backup_path = prepare_legacy_migration_backup(&database_path)?;
        let kernel = match MasterKernel::open_after_verified_migration_backup(&database_path) {
            Ok(kernel) => kernel,
            Err(error) => {
                if let Some(backup_path) = migration_backup_path.as_ref() {
                    if let Err(restore_error) =
                        restore_verified_migration_backup(&database_path, backup_path)
                    {
                        return Err(MasterError::MigrationAndRestoreFailed {
                            migration: error.to_string(),
                            restore: restore_error.to_string(),
                        });
                    }
                }
                return Err(error);
            }
        };
        Ok(Self {
            _owner_lock: owner_lock,
            data_dir,
            database_path,
            migration_backup_path,
            kernel,
        })
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn database_path(&self) -> &Path {
        &self.database_path
    }

    pub fn migration_backup_path(&self) -> Option<&Path> {
        self.migration_backup_path.as_deref()
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
        let path = path.as_ref();
        if path.exists() {
            let existing = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            let version: i64 = existing.query_row("PRAGMA user_version", [], |row| row.get(0))?;
            if (1..MASTER_SCHEMA_VERSION).contains(&version) {
                return Err(MasterError::MigrationBackup(
                    "file-backed legacy master schemas must be opened through MasterProcess"
                        .to_string(),
                ));
            }
        }
        let connection = Connection::open(path)?;
        Self::initialize(connection)
    }

    fn open_after_verified_migration_backup(path: &Path) -> Result<Self, MasterError> {
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
            feature_startup_quarantines: 0,
        };
        kernel.migrate()?;
        kernel.startup_reconciliation = kernel.reconcile_interrupted_state(current_time_ms()?)?;
        kernel.feature_startup_quarantines =
            kernel.reconcile_feature_conveyor_startup(current_time_ms()?)?;
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

    pub fn feature_startup_quarantines(&self) -> u64 {
        self.feature_startup_quarantines
    }

    pub fn emergency_paused(&self) -> Result<bool, MasterError> {
        Ok(self.emergency_pause_snapshot()?.0)
    }

    pub fn emergency_pause_revision(&self) -> Result<u64, MasterError> {
        Ok(self.emergency_pause_snapshot()?.1)
    }

    fn emergency_pause_snapshot(&self) -> Result<(bool, u64), MasterError> {
        let (paused, revision): (i64, i64) = self.connection.query_row(
            "SELECT paused.integer_value, revision.integer_value
             FROM master_metadata paused
             JOIN master_metadata revision
               ON revision.key = 'emergency_pause_revision'
             WHERE paused.key = 'emergency_paused'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let paused = match paused {
            0 => false,
            1 => true,
            _ => {
                return Err(MasterError::InvalidStoredState(
                    "emergency pause state is not boolean".to_string(),
                ));
            }
        };
        Ok((paused, i64_to_u64(revision)?))
    }

    pub fn set_emergency_paused(&mut self, paused: bool) -> Result<(), MasterError> {
        self.set_emergency_paused_at(paused, current_time_ms()?)
    }

    pub fn set_emergency_paused_at(
        &mut self,
        paused: bool,
        now_ms: u64,
    ) -> Result<(), MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let was_paused = emergency_paused_tx(&tx)?;
        if paused && !was_paused {
            request_active_remote_work_cancellations_tx(&tx, now_ms)?;
        }
        let changed = tx.execute(
            "UPDATE master_metadata SET integer_value = ?1\n\
             WHERE key = 'emergency_paused'",
            [i64::from(paused)],
        )?;
        if changed != 1 {
            return Err(MasterError::InvalidStoredState(
                "emergency pause state is missing".to_string(),
            ));
        }
        if paused != was_paused {
            let revision = emergency_pause_revision_tx(&tx)?;
            let next_revision = revision.checked_add(1).ok_or_else(|| {
                MasterError::InvalidStoredState("emergency pause revision overflowed".to_string())
            })?;
            let changed = tx.execute(
                "UPDATE master_metadata SET integer_value = ?1
                 WHERE key = 'emergency_pause_revision' AND integer_value = ?2",
                params![u64_to_i64(next_revision)?, u64_to_i64(revision)?],
            )?;
            if changed != 1 {
                return Err(MasterError::InvalidStoredState(
                    "emergency pause revision is missing or changed".to_string(),
                ));
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn feature_queue_revision(&self) -> Result<u64, MasterError> {
        let revision: i64 = self.connection.query_row(
            "SELECT queue_revision FROM feature_conveyor_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )?;
        i64_to_u64(revision)
    }

    pub fn owner_control_bridge_designation(
        &self,
    ) -> Result<Option<OwnerControlBridgeDesignation>, MasterError> {
        owner_control_bridge_designation_connection(&self.connection)
    }

    pub fn designate_owner_control_bridge(
        &mut self,
        device_id: DeviceId,
        expected_designation_revision: u64,
        now_ms: u64,
    ) -> Result<OwnerControlBridgeDesignation, MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (current_device_id, current_revision): (Option<String>, i64) = tx.query_row(
            "SELECT owner_bridge_device_id, designation_revision
             FROM feature_owner_control_state WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let current_revision = i64_to_u64(current_revision)?;
        if current_revision != expected_designation_revision {
            return Err(MasterError::StaleOwnerControlDesignationRevision {
                expected: expected_designation_revision,
                found: current_revision,
            });
        }
        let registration = device_registration_tx(&tx, device_id)?;
        require_owner_control_eligible_registration(&registration)?;
        let next_revision = current_revision
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        let changed = tx.execute(
            "UPDATE feature_owner_control_state
             SET owner_bridge_device_id = ?1,
                 owner_bridge_registry_revision = ?2,
                 designation_revision = ?3
             WHERE singleton = 1 AND designation_revision = ?4",
            params![
                device_id.0.to_string(),
                u64_to_i64(registration.registry_revision)?,
                u64_to_i64(next_revision)?,
                u64_to_i64(current_revision)?,
            ],
        )?;
        if changed != 1 {
            return Err(MasterError::InvalidStoredState(
                "owner-control designation changed during compare-and-set".to_string(),
            ));
        }
        append_feature_audit_tx(
            &tx,
            "owner_control_bridge_designated",
            None,
            now_ms,
            serde_json::json!({
                "designation_revision": next_revision,
                "registry_revision": registration.registry_revision,
                "rebound": current_device_id.is_some(),
                "side_effect_executed": false
            }),
        )?;
        tx.commit()?;
        Ok(OwnerControlBridgeDesignation {
            device_id,
            registry_revision: registration.registry_revision,
            designation_revision: next_revision,
        })
    }

    pub fn feature_conveyor_status(&self) -> Result<FeatureConveyorStatus, MasterError> {
        let schema_version = self.schema_version()?;
        let queue_revision = self.feature_queue_revision()?;
        let (emergency_paused, emergency_pause_revision) = self.emergency_pause_snapshot()?;
        let owner_guidance = self.feature_conveyor_owner_guidance(
            queue_revision,
            emergency_paused,
            emergency_pause_revision,
        )?;
        let counts = self
            .connection
            .query_row(FEATURE_CONVEYOR_STATUS_COUNTS_SQL, [], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, i64>(10)?,
                ))
            })?;
        let visible_feature_count = i64_to_u64(counts.0)?;
        let counts_by_status = FeatureConveyorStatusCounts {
            queued: i64_to_u64(counts.1)?,
            implementing: i64_to_u64(counts.2)?,
            validating: i64_to_u64(counts.3)?,
            reviewing: i64_to_u64(counts.4)?,
            publishing: i64_to_u64(counts.5)?,
            verifying_main: i64_to_u64(counts.6)?,
            succeeded: i64_to_u64(counts.7)?,
            cancelled: i64_to_u64(counts.8)?,
            abandoned: i64_to_u64(counts.9)?,
            quarantined: i64_to_u64(counts.10)?,
        };
        let mut statement = self
            .connection
            .prepare(FEATURE_CONVEYOR_STATUS_ENTRIES_SQL)?;
        let features = statement
            .query_map([MAX_CONVEYOR_STATUS_FEATURES as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                ))
            })?
            .map(|row| {
                let (
                    feature_id,
                    specification_revision,
                    lifecycle_revision,
                    queue_position,
                    status,
                    lease_present,
                    effect_possible,
                ) = row?;
                Ok(FeatureConveyorStatusEntry {
                    feature_id: parse_uuid(&feature_id)?,
                    specification_revision: i64_to_u64(specification_revision)?,
                    lifecycle_revision: i64_to_u64(lifecycle_revision)?,
                    queue_position: i64_to_u64(queue_position)?,
                    status: FeatureLifecycleStatus::parse(&status)?,
                    lease_present: parse_stored_boolean(lease_present, "feature lease presence")?,
                    effect_possible: parse_stored_boolean(
                        effect_possible,
                        "feature effect_possible",
                    )?,
                })
            })
            .collect::<Result<Vec<_>, MasterError>>()?;
        Ok(FeatureConveyorStatus {
            schema_version,
            queue_revision,
            startup_quarantine_count: self.feature_startup_quarantines,
            counts_by_status,
            visible_feature_count,
            features_truncated: visible_feature_count > features.len() as u64,
            features,
            owner_guidance,
        })
    }

    fn feature_conveyor_owner_guidance(
        &self,
        queue_revision: u64,
        emergency_paused: bool,
        emergency_pause_revision: u64,
    ) -> Result<FeatureConveyorOwnerGuidance, MasterError> {
        if emergency_paused {
            return Ok(FeatureConveyorOwnerGuidance {
                state: FeatureConveyorGuidanceState::Blocked,
                reason_code: FeatureConveyorGuidanceReason::EmergencyPaused,
                next_owner_action: FeatureConveyorNextOwnerAction::ResumeEmergencyPause,
                feature_id: None,
                specification_revision: None,
                lifecycle_revision: None,
                queue_revision,
                emergency_pause_revision,
            });
        }

        let active = self
            .connection
            .query_row(
                "SELECT f.feature_id, f.current_specification_revision,
                        f.lifecycle_revision, f.status
                 FROM feature_active_lease l
                 JOIN feature_conveyor_features f ON f.feature_id = l.feature_id
                 WHERE l.singleton = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;
        if let Some((feature_id, specification_revision, lifecycle_revision, status)) = active {
            let status = FeatureLifecycleStatus::parse(&status)?;
            let (state, reason_code, next_owner_action) = match status {
                FeatureLifecycleStatus::Cancelled | FeatureLifecycleStatus::Quarantined => (
                    FeatureConveyorGuidanceState::Blocked,
                    FeatureConveyorGuidanceReason::ActiveRequiresReconciliation,
                    FeatureConveyorNextOwnerAction::ReconcileActiveFeature,
                ),
                status if status.is_active_execution() => (
                    FeatureConveyorGuidanceState::InProgress,
                    FeatureConveyorGuidanceReason::ActiveFeatureLeased,
                    FeatureConveyorNextOwnerAction::Wait,
                ),
                _ => {
                    return Err(MasterError::InvalidStoredState(
                        "active Feature Conveyor lease has an invalid lifecycle status".to_string(),
                    ));
                }
            };
            return Ok(FeatureConveyorOwnerGuidance {
                state,
                reason_code,
                next_owner_action,
                feature_id: Some(parse_uuid(&feature_id)?),
                specification_revision: Some(i64_to_u64(specification_revision)?),
                lifecycle_revision: Some(i64_to_u64(lifecycle_revision)?),
                queue_revision,
                emergency_pause_revision,
            });
        }

        let head = self
            .connection
            .query_row(
                "SELECT f.feature_id, f.current_specification_revision,
                        f.lifecycle_revision, f.status,
                        EXISTS(
                          SELECT 1 FROM feature_dependencies d
                          JOIN feature_conveyor_features dependency
                            ON dependency.feature_id = d.dependency_feature_id
                          WHERE d.feature_id = f.feature_id
                            AND dependency.status <> 'succeeded'
                        )
                 FROM feature_conveyor_queue q
                 JOIN feature_conveyor_features f ON f.feature_id = q.feature_id
                 ORDER BY q.queue_position ASC, f.feature_id ASC
                 LIMIT 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                },
            )
            .optional()?;
        let Some((
            feature_id,
            specification_revision,
            lifecycle_revision,
            status,
            dependency_blocked,
        )) = head
        else {
            return Ok(FeatureConveyorOwnerGuidance {
                state: FeatureConveyorGuidanceState::Idle,
                reason_code: FeatureConveyorGuidanceReason::QueueEmpty,
                next_owner_action: FeatureConveyorNextOwnerAction::PrepareApprovedFeature,
                feature_id: None,
                specification_revision: None,
                lifecycle_revision: None,
                queue_revision,
                emergency_pause_revision,
            });
        };
        if FeatureLifecycleStatus::parse(&status)? != FeatureLifecycleStatus::Queued {
            return Err(MasterError::InvalidStoredState(
                "unleased Feature Conveyor queue head is not queued".to_string(),
            ));
        }
        let dependency_blocked =
            parse_stored_boolean(dependency_blocked, "feature dependency blocker")?;
        Ok(FeatureConveyorOwnerGuidance {
            state: if dependency_blocked {
                FeatureConveyorGuidanceState::Blocked
            } else {
                FeatureConveyorGuidanceState::Ready
            },
            reason_code: if dependency_blocked {
                FeatureConveyorGuidanceReason::HeadDependencyUnsatisfied
            } else {
                FeatureConveyorGuidanceReason::HeadDependencySatisfied
            },
            next_owner_action: if dependency_blocked {
                FeatureConveyorNextOwnerAction::ResolveHeadDependency
            } else {
                FeatureConveyorNextOwnerAction::AwaitOwnerControlSurface
            },
            feature_id: Some(parse_uuid(&feature_id)?),
            specification_revision: Some(i64_to_u64(specification_revision)?),
            lifecycle_revision: Some(i64_to_u64(lifecycle_revision)?),
            queue_revision,
            emergency_pause_revision,
        })
    }

    pub fn record_repository_grant_revision(
        &mut self,
        grant: &RepositoryGrantRevision,
        expected_current_revision: u64,
        expected_emergency_pause_revision: u64,
        now_ms: u64,
    ) -> Result<(), MasterError> {
        validate_repository_grant(grant)?;
        if !grant.revoked && grant.expires_at_ms.is_some_and(|expiry| expiry <= now_ms) {
            return Err(MasterError::InvalidFeatureConveyorInput(
                "an active repository grant must not already be expired".to_string(),
            ));
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_emergency_pause_revision_tx(&tx, expected_emergency_pause_revision)?;
        if !grant.revoked {
            require_emergency_unpaused_tx(&tx)?;
        }
        let current_revision: i64 = tx.query_row(
            "SELECT COALESCE(MAX(revision), 0) FROM feature_repository_grants
             WHERE repository_id = ?1 AND grant_kind = ?2",
            params![grant.repository_id.to_string(), grant.kind.as_str()],
            |row| row.get(0),
        )?;
        let current_revision = i64_to_u64(current_revision)?;
        if current_revision != expected_current_revision {
            return Err(MasterError::StaleRepositoryGrantRevision {
                expected: expected_current_revision,
                found: current_revision,
            });
        }
        let next_revision = current_revision.checked_add(1).ok_or_else(|| {
            MasterError::InvalidStoredState("repository grant revision overflowed".to_string())
        })?;
        if grant.revision != next_revision {
            return Err(MasterError::InvalidFeatureConveyorInput(
                "repository grant revisions must be contiguous".to_string(),
            ));
        }
        insert_repository_grant_revision_tx(&tx, grant, now_ms)?;
        tx.commit()?;
        Ok(())
    }

    pub fn repository_grant_set(
        &self,
        repository_id: Uuid,
        now_ms: u64,
    ) -> Result<FeatureConveyorRepositoryGrantSet, MasterError> {
        if repository_id.is_nil() {
            return Err(MasterError::InvalidFeatureConveyorInput(
                "repository identity is required".to_string(),
            ));
        }
        let (emergency_paused, emergency_pause_revision) = self.emergency_pause_snapshot()?;
        Ok(FeatureConveyorRepositoryGrantSet {
            schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
            repository_id,
            emergency_paused,
            emergency_pause_revision,
            registration: repository_grant_view(
                &self.connection,
                repository_id,
                RepositoryGrantKind::Registration,
                now_ms,
            )?,
            cloud_disclosure: repository_grant_view(
                &self.connection,
                repository_id,
                RepositoryGrantKind::CloudDisclosure,
                now_ms,
            )?,
            autonomous_publication: repository_grant_view(
                &self.connection,
                repository_id,
                RepositoryGrantKind::AutonomousPublication,
                now_ms,
            )?,
        })
    }

    /// Checks whether one exact repository scope is currently eligible for a
    /// read-only point-in-time preflight. This grants no repository action;
    /// callers must recheck through `record_repository_preflight` after the
    /// observation and before issuing any receipt.
    pub fn authorize_repository_preflight(
        &self,
        repository_id: Uuid,
        registration_grant_revision: u64,
        scope_sha256: &[u8; 32],
        expected_emergency_pause_revision: u64,
        now_ms: u64,
    ) -> Result<(), MasterError> {
        validate_repository_preflight_binding(
            repository_id,
            registration_grant_revision,
            scope_sha256,
            now_ms,
        )?;
        require_repository_preflight_binding(
            &self.connection,
            repository_id,
            registration_grant_revision,
            scope_sha256,
            expected_emergency_pause_revision,
            now_ms,
        )
    }

    /// Atomically rechecks the exact active registration grant and Emergency
    /// Pause binding, then appends only structurally redacted point-in-time
    /// audit evidence. No path or preflight result is stored durably.
    pub fn record_repository_preflight(
        &mut self,
        repository_id: Uuid,
        registration_grant_revision: u64,
        scope_sha256: &[u8; 32],
        expected_emergency_pause_revision: u64,
        observed_at_ms: u64,
    ) -> Result<(), MasterError> {
        validate_repository_preflight_binding(
            repository_id,
            registration_grant_revision,
            scope_sha256,
            observed_at_ms,
        )?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_repository_preflight_binding(
            &tx,
            repository_id,
            registration_grant_revision,
            scope_sha256,
            expected_emergency_pause_revision,
            observed_at_ms,
        )?;
        append_feature_audit_tx(
            &tx,
            "repository_identity_preflight_eligible",
            None,
            observed_at_ms,
            serde_json::json!({
                "grant_kind": "registration",
                "grant_revision": registration_grant_revision,
                "emergency_pause_revision": expected_emergency_pause_revision,
                "scope_digest_matched": true,
                "point_in_time": true,
                "identity_only": true,
                "side_effect_executed": false
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn enqueue_approved_feature(
        &mut self,
        specification: &ApprovedFeatureSpecification,
        expected_queue_revision: u64,
        now_ms: u64,
    ) -> Result<FeatureSnapshot, MasterError> {
        self.enqueue_approved_feature_with_owner_binding(
            specification,
            expected_queue_revision,
            now_ms,
            None,
        )
    }

    pub fn enqueue_approved_feature_from_owner_bridge(
        &mut self,
        specification: &ApprovedFeatureSpecification,
        expected_queue_revision: u64,
        expected_designation_revision: u64,
        expected_emergency_pause_revision: u64,
        registration: &DeviceRegistration,
        now_ms: u64,
    ) -> Result<FeatureSnapshot, MasterError> {
        self.enqueue_approved_feature_with_owner_binding(
            specification,
            expected_queue_revision,
            now_ms,
            Some((
                registration,
                expected_designation_revision,
                expected_emergency_pause_revision,
            )),
        )
    }

    fn enqueue_approved_feature_with_owner_binding(
        &mut self,
        specification: &ApprovedFeatureSpecification,
        expected_queue_revision: u64,
        now_ms: u64,
        owner_binding: Option<(&DeviceRegistration, u64, u64)>,
    ) -> Result<FeatureSnapshot, MasterError> {
        let canonical_manifest = validate_approved_specification(specification)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_queue_revision_tx(&tx, expected_queue_revision)?;
        if let Some((registration, designation_revision, emergency_pause_revision)) = owner_binding
        {
            require_owner_control_bridge_tx(&tx, registration, designation_revision)?;
            require_unpaused_revision_tx(&tx, emergency_pause_revision)?;
        }
        require_grants_tx(
            &tx,
            specification.repository_id,
            specification.grants,
            now_ms,
        )?;
        let nonterminal: i64 = tx.query_row(
            "SELECT COUNT(*) FROM feature_conveyor_features
             WHERE status NOT IN ('succeeded', 'cancelled', 'abandoned')",
            [],
            |row| row.get(0),
        )?;
        if i64_to_u64(nonterminal)? >= MAX_CONVEYOR_NONTERMINAL_FEATURES {
            return Err(MasterError::FeatureQueueFull);
        }
        for dependency_id in &specification.dependencies {
            let exists: bool = tx.query_row(
                "SELECT EXISTS(SELECT 1 FROM feature_conveyor_features WHERE feature_id = ?1)",
                [dependency_id.to_string()],
                |row| row.get(0),
            )?;
            if !exists {
                return Err(MasterError::InvalidFeatureConveyorInput(
                    "dependency does not identify an existing feature".to_string(),
                ));
            }
        }
        let position: i64 = tx.query_row(
            "SELECT COALESCE(MAX(queue_position), 0) + 1 FROM feature_conveyor_queue",
            [],
            |row| row.get(0),
        )?;
        let inserted = tx.execute(
            "INSERT INTO feature_specification_revisions (
               feature_id, revision, repository_id, canonical_manifest_json,
               manifest_sha256, design_sha256, brainstorming_sha256,
               owner_approval_sha256, registration_grant_revision,
               cloud_disclosure_grant_revision, publication_grant_revision,
               provider_id, model_id, approved_at_ms
             ) VALUES (
               ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14
             )",
            params![
                specification.feature_id.to_string(),
                u64_to_i64(specification.revision)?,
                specification.repository_id.to_string(),
                canonical_manifest,
                specification.manifest_sha256.as_slice(),
                specification.design_sha256.as_slice(),
                specification.brainstorming_sha256.as_slice(),
                specification.owner_approval_sha256.as_slice(),
                u64_to_i64(specification.grants.registration)?,
                u64_to_i64(specification.grants.cloud_disclosure)?,
                u64_to_i64(specification.grants.autonomous_publication)?,
                specification.provider_id,
                specification.model_id,
                u64_to_i64(now_ms)?,
            ],
        );
        match inserted {
            Ok(1) => {}
            Err(error) if is_constraint_violation(&error) => {
                return Err(MasterError::FeatureSpecificationImmutable);
            }
            Err(error) => return Err(error.into()),
            Ok(_) => {
                return Err(MasterError::InvalidStoredState(
                    "feature specification insert did not affect one row".to_string(),
                ));
            }
        }
        tx.execute(
            "INSERT INTO feature_conveyor_features (
               feature_id, current_specification_revision, status,
               lifecycle_revision, queue_position, effect_possible,
               created_at_ms, updated_at_ms
             ) VALUES (?1, ?2, 'queued', 1, ?3, 0, ?4, ?4)",
            params![
                specification.feature_id.to_string(),
                u64_to_i64(specification.revision)?,
                position,
                u64_to_i64(now_ms)?,
            ],
        )?;
        tx.execute(
            "INSERT INTO feature_conveyor_queue (feature_id, queue_position)
             VALUES (?1, ?2)",
            params![specification.feature_id.to_string(), position],
        )?;
        for dependency_id in &specification.dependencies {
            tx.execute(
                "INSERT INTO feature_dependencies (feature_id, dependency_feature_id)
                 VALUES (?1, ?2)",
                params![
                    specification.feature_id.to_string(),
                    dependency_id.to_string()
                ],
            )?;
        }
        let next_queue_revision = increment_queue_revision_tx(&tx, expected_queue_revision)?;
        append_feature_audit_tx(
            &tx,
            "feature_enqueued",
            Some(specification.feature_id),
            now_ms,
            serde_json::json!({
                "specification_revision": specification.revision,
                "queue_revision": next_queue_revision,
                "queue_position": position,
                "dependency_count": specification.dependencies.len(),
                "manifest_digest_present": true,
                "design_digest_present": true,
                "brainstorming_digest_present": true,
                "owner_approval_digest_present": true,
                "registration_grant_revision": specification.grants.registration,
                "cloud_disclosure_grant_revision": specification.grants.cloud_disclosure,
                "publication_grant_revision": specification.grants.autonomous_publication,
                "provider_snapshot_present": true,
                "side_effect_executed": false
            }),
        )?;
        tx.commit()?;
        self.feature_snapshot(specification.feature_id)
    }

    pub fn reorder_queued_features(
        &mut self,
        ordered_feature_ids: &[Uuid],
        expected_queue_revision: u64,
        now_ms: u64,
    ) -> Result<u64, MasterError> {
        if ordered_feature_ids.len() > MAX_CONVEYOR_NONTERMINAL_FEATURES as usize {
            return Err(MasterError::InvalidFeatureConveyorInput(
                "queue order exceeds the feature capacity".to_string(),
            ));
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_queue_revision_tx(&tx, expected_queue_revision)?;
        let mut statement = tx.prepare(
            "SELECT q.feature_id FROM feature_conveyor_queue q
             JOIN feature_conveyor_features f ON f.feature_id = q.feature_id
             WHERE f.status = 'queued' ORDER BY q.queue_position ASC",
        )?;
        let current = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        let requested = ordered_feature_ids
            .iter()
            .map(Uuid::to_string)
            .collect::<Vec<_>>();
        let mut current_sorted = current.clone();
        let mut requested_sorted = requested.clone();
        current_sorted.sort();
        requested_sorted.sort();
        if current_sorted != requested_sorted {
            return Err(MasterError::InvalidFeatureConveyorInput(
                "reorder must contain every queued feature exactly once".to_string(),
            ));
        }
        let active_position: i64 = tx.query_row(
            "SELECT COALESCE(MAX(q.queue_position), 0) FROM feature_conveyor_queue q
             JOIN feature_conveyor_features f ON f.feature_id = q.feature_id
             WHERE f.status <> 'queued'",
            [],
            |row| row.get(0),
        )?;
        tx.execute(
            "UPDATE feature_conveyor_queue
             SET queue_position = queue_position + 1000000
             WHERE feature_id IN (
               SELECT feature_id FROM feature_conveyor_features WHERE status = 'queued'
             )",
            [],
        )?;
        tx.execute(
            "UPDATE feature_conveyor_features
             SET queue_position = queue_position + 1000000
             WHERE status = 'queued'",
            [],
        )?;
        for (offset, feature_id) in ordered_feature_ids.iter().enumerate() {
            let position = active_position
                .checked_add(i64::try_from(offset + 1).map_err(|_| MasterError::IntegerOutOfRange)?)
                .ok_or(MasterError::IntegerOutOfRange)?;
            tx.execute(
                "UPDATE feature_conveyor_queue SET queue_position = ?1 WHERE feature_id = ?2",
                params![position, feature_id.to_string()],
            )?;
            tx.execute(
                "UPDATE feature_conveyor_features SET queue_position = ?1, updated_at_ms = ?2
                 WHERE feature_id = ?3 AND status = 'queued'",
                params![position, u64_to_i64(now_ms)?, feature_id.to_string()],
            )?;
        }
        let next = increment_queue_revision_tx(&tx, expected_queue_revision)?;
        append_feature_audit_tx(
            &tx,
            "feature_queue_reordered",
            None,
            now_ms,
            serde_json::json!({
                "queue_revision": next,
                "queued_feature_count": ordered_feature_ids.len(),
                "side_effect_executed": false
            }),
        )?;
        tx.commit()?;
        Ok(next)
    }

    pub fn claim_next_feature(
        &mut self,
        expected_queue_revision: u64,
        now_ms: u64,
    ) -> Result<FeatureClaim, MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_queue_revision_tx(&tx, expected_queue_revision)?;
        if emergency_paused_tx(&tx)? {
            return Err(MasterError::EmergencyPaused);
        }
        let lease_exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM feature_active_lease WHERE singleton = 1)",
            [],
            |row| row.get(0),
        )?;
        if lease_exists {
            return Err(MasterError::FeatureLeaseAlreadyActive);
        }
        let (feature_id, specification_revision): (String, i64) = tx
            .query_row(
                "SELECT f.feature_id, f.current_specification_revision
                 FROM feature_conveyor_queue q
                 JOIN feature_conveyor_features f ON f.feature_id = q.feature_id
                 WHERE f.status = 'queued'
                 ORDER BY q.queue_position ASC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?
            .ok_or(MasterError::FeatureNotFound)?;
        let dependency_blocked: bool = tx.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM feature_dependencies d
               JOIN feature_conveyor_features dependency
                 ON dependency.feature_id = d.dependency_feature_id
               WHERE d.feature_id = ?1 AND dependency.status <> 'succeeded'
             )",
            [&feature_id],
            |row| row.get(0),
        )?;
        if dependency_blocked {
            return Err(MasterError::FeatureDependencyBlocked);
        }
        let (repository_id, registration, cloud_disclosure, publication, provider_id, model_id): (
            String,
            i64,
            i64,
            i64,
            String,
            String,
        ) = tx.query_row(
            "SELECT repository_id, registration_grant_revision,
                    cloud_disclosure_grant_revision, publication_grant_revision,
                    provider_id, model_id
             FROM feature_specification_revisions
             WHERE feature_id = ?1 AND revision = ?2",
            params![feature_id, specification_revision],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )?;
        let grants = FeatureGrantRevisions {
            registration: i64_to_u64(registration)?,
            cloud_disclosure: i64_to_u64(cloud_disclosure)?,
            autonomous_publication: i64_to_u64(publication)?,
        };
        require_grants_tx(&tx, parse_uuid(&repository_id)?, grants, now_ms)?;
        let feature_uuid = parse_uuid(&feature_id)?;
        let lease_id = Uuid::new_v4();
        tx.execute(
            "INSERT INTO feature_active_lease (
               singleton, feature_id, lease_id, claimed_at_ms
             ) VALUES (1, ?1, ?2, ?3)",
            params![feature_id, lease_id.to_string(), u64_to_i64(now_ms)?],
        )?;
        let changed = tx.execute(
            "UPDATE feature_conveyor_features
             SET status = 'implementing', lifecycle_revision = lifecycle_revision + 1,
                 updated_at_ms = ?1
             WHERE feature_id = ?2 AND status = 'queued'",
            params![u64_to_i64(now_ms)?, feature_id],
        )?;
        if changed != 1 {
            return Err(MasterError::InvalidFeatureTransition);
        }
        let lifecycle_revision = tx.query_row(
            "SELECT lifecycle_revision FROM feature_conveyor_features WHERE feature_id = ?1",
            [feature_uuid.to_string()],
            |row| row.get::<_, i64>(0),
        )?;
        let next_queue_revision = increment_queue_revision_tx(&tx, expected_queue_revision)?;
        append_feature_audit_tx(
            &tx,
            "feature_claimed",
            Some(feature_uuid),
            now_ms,
            serde_json::json!({
                "from_status": "queued",
                "to_status": "implementing",
                "lifecycle_revision": lifecycle_revision,
                "queue_revision": next_queue_revision,
                "lease_present": true,
                "provider_snapshot_present": true,
                "grant_snapshot_present": true,
                "effect_possible": false,
                "side_effect_executed": false
            }),
        )?;
        tx.commit()?;
        Ok(FeatureClaim {
            feature_id: feature_uuid,
            specification_revision: i64_to_u64(specification_revision)?,
            lifecycle_revision: i64_to_u64(lifecycle_revision)?,
            lease_id,
            provider_id,
            model_id,
            grants,
        })
    }

    pub fn advance_feature_lifecycle(
        &mut self,
        feature_id: Uuid,
        expected_lifecycle_revision: u64,
        next_status: FeatureLifecycleStatus,
        evidence: FeatureTransitionEvidence,
        now_ms: u64,
    ) -> Result<FeatureSnapshot, MasterError> {
        if evidence.repository_snapshot_sha256 == [0; 32]
            || evidence.accepted_evidence_sha256 == [0; 32]
        {
            return Err(MasterError::InvalidFeatureConveyorInput(
                "exact repository snapshot and accepted evidence digests are required".to_string(),
            ));
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_active_lease_tx(&tx, feature_id)?;
        if emergency_paused_tx(&tx)? {
            return Err(MasterError::EmergencyPaused);
        }
        require_current_feature_grants_tx(&tx, feature_id, now_ms)?;
        let current = feature_status_and_revision_tx(&tx, feature_id)?;
        if current.1 != expected_lifecycle_revision {
            return Err(MasterError::StaleFeatureLifecycleRevision {
                expected: expected_lifecycle_revision,
                found: current.1,
            });
        }
        let valid = matches!(
            (current.0, next_status),
            (
                FeatureLifecycleStatus::Implementing,
                FeatureLifecycleStatus::Validating
            ) | (
                FeatureLifecycleStatus::Validating,
                FeatureLifecycleStatus::Reviewing
            ) | (
                FeatureLifecycleStatus::Reviewing,
                FeatureLifecycleStatus::Publishing
            ) | (
                FeatureLifecycleStatus::Publishing,
                FeatureLifecycleStatus::VerifyingMain
            )
        );
        if !valid {
            return Err(MasterError::InvalidFeatureTransition);
        }
        let changed = tx.execute(
            "UPDATE feature_conveyor_features
             SET status = ?1, lifecycle_revision = lifecycle_revision + 1,
                 effect_possible = ?2, updated_at_ms = ?3
             WHERE feature_id = ?4 AND status = ?5 AND lifecycle_revision = ?6",
            params![
                next_status.as_str(),
                i64::from(matches!(
                    next_status,
                    FeatureLifecycleStatus::Publishing | FeatureLifecycleStatus::VerifyingMain
                )),
                u64_to_i64(now_ms)?,
                feature_id.to_string(),
                current.0.as_str(),
                u64_to_i64(expected_lifecycle_revision)?,
            ],
        )?;
        if changed != 1 {
            return Err(MasterError::InvalidFeatureTransition);
        }
        tx.execute(
            "INSERT INTO feature_transition_evidence (
               feature_id, lifecycle_revision, from_status, to_status,
               repository_snapshot_sha256, accepted_evidence_sha256, recorded_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                feature_id.to_string(),
                u64_to_i64(expected_lifecycle_revision + 1)?,
                current.0.as_str(),
                next_status.as_str(),
                evidence.repository_snapshot_sha256.as_slice(),
                evidence.accepted_evidence_sha256.as_slice(),
                u64_to_i64(now_ms)?,
            ],
        )?;
        append_feature_audit_tx(
            &tx,
            "feature_lifecycle_advanced",
            Some(feature_id),
            now_ms,
            serde_json::json!({
                "from_status": current.0.as_str(),
                "to_status": next_status.as_str(),
                "lifecycle_revision": expected_lifecycle_revision + 1,
                "repository_snapshot_digest_present":
                    true,
                "accepted_evidence_digest_present":
                    true,
                "effect_possible": matches!(
                    next_status,
                    FeatureLifecycleStatus::Publishing
                        | FeatureLifecycleStatus::VerifyingMain
                ),
                "side_effect_executed": false
            }),
        )?;
        tx.commit()?;
        self.feature_snapshot(feature_id)
    }

    pub fn cancel_active_feature(
        &mut self,
        feature_id: Uuid,
        expected_lifecycle_revision: u64,
        now_ms: u64,
    ) -> Result<FeatureSnapshot, MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_active_lease_tx(&tx, feature_id)?;
        let (status, revision) = feature_status_and_revision_tx(&tx, feature_id)?;
        if revision != expected_lifecycle_revision {
            return Err(MasterError::StaleFeatureLifecycleRevision {
                expected: expected_lifecycle_revision,
                found: revision,
            });
        }
        if !status.is_active_execution() {
            return Err(MasterError::InvalidFeatureTransition);
        }
        tx.execute(
            "UPDATE feature_conveyor_features
             SET status = 'cancelled', lifecycle_revision = lifecycle_revision + 1,
                 effect_possible = 1, updated_at_ms = ?1
             WHERE feature_id = ?2",
            params![u64_to_i64(now_ms)?, feature_id.to_string()],
        )?;
        append_feature_audit_tx(
            &tx,
            "feature_cancelled",
            Some(feature_id),
            now_ms,
            serde_json::json!({
                "from_status": status.as_str(),
                "to_status": "cancelled",
                "lifecycle_revision": revision + 1,
                "lease_retained": true,
                "advancement_authorized": false,
                "effect_possible": true,
                "side_effect_executed": false
            }),
        )?;
        tx.commit()?;
        self.feature_snapshot(feature_id)
    }

    pub fn mark_feature_succeeded(
        &mut self,
        feature_id: Uuid,
        expected_lifecycle_revision: u64,
        expected_queue_revision: u64,
        success: VerifiedFeatureSuccess,
        now_ms: u64,
    ) -> Result<FeatureSnapshot, MasterError> {
        if !success.main_healthy
            || success.main_commit_sha256 == [0; 32]
            || success.post_merge_evidence_sha256 == [0; 32]
        {
            return Err(MasterError::VerifiedHealthyMainRequired);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_queue_revision_tx(&tx, expected_queue_revision)?;
        require_active_lease_tx(&tx, feature_id)?;
        require_current_feature_grants_tx(&tx, feature_id, now_ms)?;
        let (status, revision) = feature_status_and_revision_tx(&tx, feature_id)?;
        if revision != expected_lifecycle_revision {
            return Err(MasterError::StaleFeatureLifecycleRevision {
                expected: expected_lifecycle_revision,
                found: revision,
            });
        }
        if status != FeatureLifecycleStatus::VerifyingMain {
            return Err(MasterError::InvalidFeatureTransition);
        }
        tx.execute(
            "UPDATE feature_conveyor_features
             SET status = 'succeeded', lifecycle_revision = lifecycle_revision + 1,
                 effect_possible = 0, updated_at_ms = ?1
             WHERE feature_id = ?2",
            params![u64_to_i64(now_ms)?, feature_id.to_string()],
        )?;
        tx.execute(
            "INSERT INTO feature_transition_evidence (
               feature_id, lifecycle_revision, from_status, to_status,
               verified_main_commit_sha256, post_merge_evidence_sha256, recorded_at_ms
             ) VALUES (?1, ?2, 'verifying_main', 'succeeded', ?3, ?4, ?5)",
            params![
                feature_id.to_string(),
                u64_to_i64(revision + 1)?,
                success.main_commit_sha256.as_slice(),
                success.post_merge_evidence_sha256.as_slice(),
                u64_to_i64(now_ms)?,
            ],
        )?;
        tx.execute(
            "DELETE FROM feature_conveyor_queue WHERE feature_id = ?1",
            [feature_id.to_string()],
        )?;
        tx.execute(
            "DELETE FROM feature_active_lease WHERE singleton = 1 AND feature_id = ?1",
            [feature_id.to_string()],
        )?;
        let next_queue_revision = increment_queue_revision_tx(&tx, expected_queue_revision)?;
        append_feature_audit_tx(
            &tx,
            "feature_succeeded",
            Some(feature_id),
            now_ms,
            serde_json::json!({
                "from_status": "verifying_main",
                "to_status": "succeeded",
                "lifecycle_revision": revision + 1,
                "queue_revision": next_queue_revision,
                "verified_main_commit_digest_present": true,
                "post_merge_evidence_digest_present": true,
                "main_healthy": true,
                "lease_released": true,
                "effect_possible": false,
                "side_effect_executed": false
            }),
        )?;
        tx.commit()?;
        self.feature_snapshot(feature_id)
    }

    pub fn abandon_and_advance(
        &mut self,
        feature_id: Uuid,
        expected_lifecycle_revision: u64,
        expected_queue_revision: u64,
        evidence: FeatureAbandonmentEvidence,
        now_ms: u64,
    ) -> Result<FeatureSnapshot, MasterError> {
        if evidence.safe_reconciliation_sha256 == [0; 32] {
            return Err(MasterError::InvalidFeatureConveyorInput(
                "safe reconciliation proof is required".to_string(),
            ));
        }
        if evidence
            .verified_healthy_main_sha256
            .is_some_and(|digest| digest == [0; 32])
        {
            return Err(MasterError::InvalidFeatureConveyorInput(
                "present healthy-main evidence must have an exact nonzero digest".to_string(),
            ));
        }
        if evidence.merged
            && evidence
                .verified_healthy_main_sha256
                .filter(|digest| *digest != [0; 32])
                .is_none()
        {
            return Err(MasterError::VerifiedHealthyMainRequired);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_queue_revision_tx(&tx, expected_queue_revision)?;
        require_active_lease_tx(&tx, feature_id)?;
        let (status, revision) = feature_status_and_revision_tx(&tx, feature_id)?;
        if revision != expected_lifecycle_revision {
            return Err(MasterError::StaleFeatureLifecycleRevision {
                expected: expected_lifecycle_revision,
                found: revision,
            });
        }
        if !matches!(
            status,
            FeatureLifecycleStatus::Cancelled | FeatureLifecycleStatus::Quarantined
        ) {
            return Err(MasterError::InvalidFeatureTransition);
        }
        tx.execute(
            "UPDATE feature_conveyor_features
             SET status = 'abandoned', lifecycle_revision = lifecycle_revision + 1,
                 effect_possible = 0, updated_at_ms = ?1
             WHERE feature_id = ?2",
            params![u64_to_i64(now_ms)?, feature_id.to_string()],
        )?;
        tx.execute(
            "INSERT INTO feature_transition_evidence (
               feature_id, lifecycle_revision, from_status, to_status,
               safe_reconciliation_sha256, verified_healthy_main_sha256, recorded_at_ms
             ) VALUES (?1, ?2, ?3, 'abandoned', ?4, ?5, ?6)",
            params![
                feature_id.to_string(),
                u64_to_i64(revision + 1)?,
                status.as_str(),
                evidence.safe_reconciliation_sha256.as_slice(),
                evidence
                    .verified_healthy_main_sha256
                    .as_ref()
                    .map(|digest| digest.as_slice()),
                u64_to_i64(now_ms)?,
            ],
        )?;
        tx.execute(
            "DELETE FROM feature_conveyor_queue WHERE feature_id = ?1",
            [feature_id.to_string()],
        )?;
        tx.execute(
            "DELETE FROM feature_active_lease WHERE singleton = 1 AND feature_id = ?1",
            [feature_id.to_string()],
        )?;
        let next_queue_revision = increment_queue_revision_tx(&tx, expected_queue_revision)?;
        append_feature_audit_tx(
            &tx,
            "feature_abandoned",
            Some(feature_id),
            now_ms,
            serde_json::json!({
                "from_status": status.as_str(),
                "to_status": "abandoned",
                "lifecycle_revision": revision + 1,
                "queue_revision": next_queue_revision,
                "safe_reconciliation_digest_present": true,
                "merged": evidence.merged,
                "verified_healthy_main_digest_present":
                    evidence.verified_healthy_main_sha256.is_some(),
                "lease_released": true,
                "effect_possible": false,
                "side_effect_executed": false
            }),
        )?;
        tx.commit()?;
        self.feature_snapshot(feature_id)
    }

    pub fn feature_snapshot(&self, feature_id: Uuid) -> Result<FeatureSnapshot, MasterError> {
        self.connection
            .query_row(
                "SELECT f.current_specification_revision, f.status,
                        f.lifecycle_revision, f.queue_position, f.effect_possible,
                        l.lease_id
                 FROM feature_conveyor_features f
                 LEFT JOIN feature_active_lease l ON l.feature_id = f.feature_id
                 WHERE f.feature_id = ?1",
                [feature_id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, i64>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                },
            )
            .optional()?
            .map(
                |(
                    specification_revision,
                    status,
                    lifecycle_revision,
                    queue_position,
                    effect_possible,
                    lease_id,
                )| {
                    Ok(FeatureSnapshot {
                        feature_id,
                        specification_revision: i64_to_u64(specification_revision)?,
                        status: FeatureLifecycleStatus::parse(&status)?,
                        lifecycle_revision: i64_to_u64(lifecycle_revision)?,
                        queue_position: i64_to_u64(queue_position)?,
                        active_lease_id: lease_id.map(|value| parse_uuid(&value)).transpose()?,
                        effect_possible: match effect_possible {
                            0 => false,
                            1 => true,
                            _ => {
                                return Err(MasterError::InvalidStoredState(
                                    "feature effect_possible is not boolean".to_string(),
                                ));
                            }
                        },
                    })
                },
            )
            .transpose()?
            .ok_or(MasterError::FeatureNotFound)
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
                "SELECT COUNT(*) FROM master_attempts\n\
                 WHERE status IN ('leased', 'cancellation_pending')",
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
        self.lease_next_step_bound(device_id, connection_epoch, now_ms, None)
    }

    pub fn lease_next_fixture_step(
        &mut self,
        device_id: DeviceId,
        connection_epoch: u64,
        now_ms: u64,
    ) -> Result<JobEnvelope, MasterError> {
        self.lease_next_step_bound(
            device_id,
            connection_epoch,
            now_ms,
            Some(&RemoteWorkContract::Fixture),
        )
    }

    pub fn lease_next_remote_step(
        &mut self,
        device_id: DeviceId,
        connection_epoch: u64,
        now_ms: u64,
        contract: &RemoteWorkContract,
    ) -> Result<JobEnvelope, MasterError> {
        self.lease_next_step_bound(device_id, connection_epoch, now_ms, Some(contract))
    }

    fn lease_next_step_bound(
        &mut self,
        device_id: DeviceId,
        connection_epoch: u64,
        now_ms: u64,
        remote_contract: Option<&RemoteWorkContract>,
    ) -> Result<JobEnvelope, MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_emergency_unpaused_tx(&tx)?;
        reconcile_expired_leases_tx(&tx, now_ms)?;
        let connection = connection_state(&tx, device_id)?;
        if !connection.active {
            return Err(MasterError::ConnectionNotActive);
        }
        if connection.epoch != connection_epoch {
            return Err(MasterError::ConnectionEpochMismatch);
        }
        let device_leases: i64 = tx.query_row(
            "SELECT COUNT(*) FROM master_attempts\n\
             WHERE device_id = ?1 AND connection_epoch = ?2\n\
               AND status IN ('leased', 'cancellation_pending')",
            params![device_id.0.to_string(), u64_to_i64(connection_epoch)?],
            |row| row.get(0),
        )?;
        if device_leases != 0 {
            return Err(MasterError::DeviceAlreadyLeased);
        }
        let global_leases: i64 = tx.query_row(
            "SELECT COUNT(*) FROM master_attempts\n\
             WHERE status IN ('leased', 'cancellation_pending')",
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
        if let Some(contract) = remote_contract {
            if capabilities != vec![contract.capability()] {
                return Err(MasterError::InvalidRemoteWorkContract);
            }
        }
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
        if let Some(contract) = remote_contract {
            contract.validate_job(&job)?;
        }
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
        self.accept_result_bound(None, result, now_ms, None)
    }

    pub fn accept_result_from(
        &mut self,
        authenticated_device_id: DeviceId,
        result: &JobResultEnvelope,
        now_ms: u64,
    ) -> Result<AcceptedResult, MasterError> {
        self.accept_result_bound(Some(authenticated_device_id), result, now_ms, None)
    }

    pub fn accept_fixture_result_from(
        &mut self,
        authenticated_device_id: DeviceId,
        result: &JobResultEnvelope,
        now_ms: u64,
    ) -> Result<AcceptedResult, MasterError> {
        self.accept_result_bound(
            Some(authenticated_device_id),
            result,
            now_ms,
            Some(&RemoteWorkContract::Fixture),
        )
    }

    pub fn accept_remote_result_from(
        &mut self,
        authenticated_device_id: DeviceId,
        result: &JobResultEnvelope,
        now_ms: u64,
        contract: &RemoteWorkContract,
    ) -> Result<AcceptedResult, MasterError> {
        self.accept_result_bound(
            Some(authenticated_device_id),
            result,
            now_ms,
            Some(contract),
        )
    }

    fn accept_result_bound(
        &mut self,
        authenticated_device_id: Option<DeviceId>,
        result: &JobResultEnvelope,
        now_ms: u64,
        remote_contract: Option<&RemoteWorkContract>,
    ) -> Result<AcceptedResult, MasterError> {
        result.validate()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_emergency_unpaused_tx(&tx)?;
        let attempt = load_attempt(&tx, result.attempt_id)?.ok_or(MasterError::AttemptNotFound)?;
        if authenticated_device_id.is_some_and(|device_id| device_id != attempt.device_id) {
            return Err(MasterError::ResultDeviceMismatch);
        }
        let job: JobEnvelope = serde_json::from_str(&attempt.job_json)?;
        if let Some(contract) = remote_contract {
            contract.validate_result(result, &job)?;
            let registered = capability_for_device(&tx, attempt.device_id, &job.capability_id)?;
            if registered != contract.capability() {
                return Err(MasterError::InvalidRemoteWorkContract);
            }
        } else {
            result.validate_for_job(&job)?;
        }
        if attempt.status != AttemptStatus::Leased {
            return Err(MasterError::ResultNotAccepting(attempt.status));
        }
        if now_ms >= attempt.lease_expires_at_ms {
            tx.execute(
                "UPDATE master_attempts SET status = 'expired', completed_at_ms = ?1\n                 WHERE attempt_id = ?2 AND status = 'leased'",
                params![u64_to_i64(now_ms)?, result.attempt_id.0.to_string()],
            )?;
            append_distributed_event_tx(
                &tx,
                DistributedEventKind::StepLeaseExpired,
                now_ms,
                DistributedEventIdentity {
                    task_id: Some(result.task_id),
                    step_id: Some(result.step_id),
                    device_id: Some(attempt.device_id),
                    connection_epoch: Some(result.connection_epoch),
                },
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
                request_leased_step_cancellation_tx(&tx, task_id, step_id, now_ms)?;
                tx.commit()?;
                return Ok(());
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

    pub fn next_cancellation(
        &mut self,
        device_id: DeviceId,
        connection_epoch: u64,
        now_ms: u64,
    ) -> Result<Option<CancellationInstruction>, MasterError> {
        self.next_cancellation_bound(device_id, connection_epoch, now_ms, None)
    }

    pub fn next_remote_cancellation(
        &mut self,
        device_id: DeviceId,
        connection_epoch: u64,
        now_ms: u64,
        contract: &RemoteWorkContract,
    ) -> Result<Option<CancellationInstruction>, MasterError> {
        self.next_cancellation_bound(device_id, connection_epoch, now_ms, Some(contract))
    }

    fn next_cancellation_bound(
        &mut self,
        device_id: DeviceId,
        connection_epoch: u64,
        now_ms: u64,
        remote_contract: Option<&RemoteWorkContract>,
    ) -> Result<Option<CancellationInstruction>, MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        reconcile_cancellation_deadlines_tx(&tx, now_ms)?;
        let connection = connection_state(&tx, device_id)?;
        if !connection.active {
            return Err(MasterError::ConnectionNotActive);
        }
        if connection.epoch != connection_epoch {
            return Err(MasterError::ConnectionEpochMismatch);
        }
        let stored = tx
            .query_row(
                "SELECT job_json, cancellation_sequence, cancellation_deadline_at_ms\n\
                 FROM master_attempts\n\
                 WHERE device_id = ?1 AND connection_epoch = ?2\n\
                   AND status = 'cancellation_pending'\n\
                 ORDER BY cancellation_requested_at_ms ASC LIMIT 1",
                params![device_id.0.to_string(), u64_to_i64(connection_epoch)?,],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?;
        let instruction = stored
            .map(|(job_json, sequence, deadline_at_ms)| {
                let job: JobEnvelope = serde_json::from_str(&job_json)?;
                if let Some(contract) = remote_contract {
                    let registered = capability_for_device(&tx, device_id, &job.capability_id)?;
                    if registered != contract.capability() {
                        return Err(MasterError::InvalidRemoteWorkContract);
                    }
                    contract.validate_job(&job)?;
                }
                let deadline_after_ms = i64_to_u64(deadline_at_ms)?
                    .checked_sub(now_ms)
                    .ok_or(MasterError::CancellationExpired)?;
                let instruction = CancellationInstruction {
                    protocol_version: PROTOCOL_VERSION,
                    connection_epoch,
                    sequence: i64_to_u64(sequence)?,
                    task_id: job.task_id,
                    step_id: job.step_id,
                    attempt_id: job.attempt_id,
                    lease_id: job.lease_id,
                    cancellation_id: job.cancellation_id,
                    deadline_after_ms,
                };
                instruction.validate_for_job(&job)?;
                Ok::<CancellationInstruction, MasterError>(instruction)
            })
            .transpose()?;
        tx.commit()?;
        Ok(instruction)
    }

    pub fn accept_cancellation_ack_from(
        &mut self,
        authenticated_device_id: DeviceId,
        acknowledgement: &CancellationAcknowledgement,
        now_ms: u64,
    ) -> Result<AcceptedCancellation, MasterError> {
        self.accept_cancellation_ack_bound(authenticated_device_id, acknowledgement, now_ms, None)
    }

    pub fn accept_remote_cancellation_ack_from(
        &mut self,
        authenticated_device_id: DeviceId,
        acknowledgement: &CancellationAcknowledgement,
        now_ms: u64,
        contract: &RemoteWorkContract,
    ) -> Result<AcceptedCancellation, MasterError> {
        self.accept_cancellation_ack_bound(
            authenticated_device_id,
            acknowledgement,
            now_ms,
            Some(contract),
        )
    }

    fn accept_cancellation_ack_bound(
        &mut self,
        authenticated_device_id: DeviceId,
        acknowledgement: &CancellationAcknowledgement,
        now_ms: u64,
        remote_contract: Option<&RemoteWorkContract>,
    ) -> Result<AcceptedCancellation, MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let attempt =
            load_attempt(&tx, acknowledgement.attempt_id)?.ok_or(MasterError::AttemptNotFound)?;
        if attempt.device_id != authenticated_device_id {
            return Err(MasterError::ResultDeviceMismatch);
        }
        if attempt.status != AttemptStatus::CancellationPending {
            return Err(MasterError::ResultNotAccepting(attempt.status));
        }
        let job: JobEnvelope = serde_json::from_str(&attempt.job_json)?;
        if let Some(contract) = remote_contract {
            let registered =
                capability_for_device(&tx, authenticated_device_id, &job.capability_id)?;
            if registered != contract.capability() {
                return Err(MasterError::InvalidRemoteWorkContract);
            }
            contract.validate_job(&job)?;
        }
        let instruction = CancellationInstruction {
            protocol_version: PROTOCOL_VERSION,
            connection_epoch: job.connection_epoch,
            sequence: attempt.cancellation_sequence.ok_or_else(|| {
                MasterError::InvalidStoredState("missing cancellation sequence".into())
            })?,
            task_id: job.task_id,
            step_id: job.step_id,
            attempt_id: job.attempt_id,
            lease_id: job.lease_id,
            cancellation_id: job.cancellation_id,
            deadline_after_ms: CANCELLATION_ACK_DEADLINE_MS,
        };
        acknowledgement.validate_for_instruction(&instruction)?;
        if now_ms
            >= attempt.cancellation_deadline_at_ms.ok_or_else(|| {
                MasterError::InvalidStoredState("missing cancellation deadline".into())
            })?
        {
            reconcile_cancellation_deadlines_tx(&tx, now_ms)?;
            tx.commit()?;
            return Err(MasterError::CancellationExpired);
        }
        let connection = connection_state(&tx, authenticated_device_id)?;
        if !connection.active || connection.epoch != acknowledgement.connection_epoch {
            return Err(MasterError::ConnectionNotActive);
        }
        if acknowledgement.sequence <= connection.last_sequence {
            return Err(MasterError::SequenceReplay);
        }
        tx.execute(
            "UPDATE master_attempts SET status = 'cancelled', completed_at_ms = ?1,\n\
               cancellation_ack_sequence = ?2\n\
             WHERE attempt_id = ?3 AND status = 'cancellation_pending'",
            params![
                u64_to_i64(now_ms)?,
                u64_to_i64(acknowledgement.sequence)?,
                acknowledgement.attempt_id.0.to_string(),
            ],
        )?;
        tx.execute(
            "UPDATE master_steps SET status = 'cancelled', completed_at_ms = ?1\n\
             WHERE step_id = ?2 AND status = 'leased'",
            params![u64_to_i64(now_ms)?, acknowledgement.step_id.0.to_string()],
        )?;
        tx.execute(
            "UPDATE master_connections SET last_sequence = ?1\n\
             WHERE device_id = ?2 AND connection_epoch = ?3 AND active = 1",
            params![
                u64_to_i64(acknowledgement.sequence)?,
                authenticated_device_id.0.to_string(),
                u64_to_i64(acknowledgement.connection_epoch)?,
            ],
        )?;
        append_distributed_event_tx(
            &tx,
            DistributedEventKind::StepCancellationAcknowledged,
            now_ms,
            DistributedEventIdentity {
                task_id: Some(acknowledgement.task_id),
                step_id: Some(acknowledgement.step_id),
                device_id: Some(authenticated_device_id),
                connection_epoch: Some(acknowledgement.connection_epoch),
            },
        )?;
        append_distributed_event_tx(
            &tx,
            DistributedEventKind::StepCancelled,
            now_ms,
            DistributedEventIdentity {
                task_id: Some(acknowledgement.task_id),
                step_id: Some(acknowledgement.step_id),
                device_id: None,
                connection_epoch: None,
            },
        )?;
        tx.commit()?;
        Ok(AcceptedCancellation {
            accepted: true,
            status: StepStatus::Cancelled,
        })
    }

    pub fn reconcile_cancellation_deadlines(&mut self, now_ms: u64) -> Result<u64, MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let expired = reconcile_cancellation_deadlines_tx(&tx, now_ms)?;
        tx.commit()?;
        Ok(expired)
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
                 INSERT INTO master_metadata (key, integer_value) VALUES ('emergency_paused', 0);\n\
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
                   payload_sha256 BLOB,\n\
                   cancellation_sequence INTEGER,\n\
                   cancellation_requested_at_ms INTEGER,\n\
                   cancellation_deadline_at_ms INTEGER,\n\
                   cancellation_ack_sequence INTEGER\n\
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
                 PRAGMA user_version = 4;\n\
                 COMMIT;",
            )?;
        }
        let version = self.schema_version()?;
        if version == 3 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;\n\
                 ALTER TABLE master_attempts ADD COLUMN cancellation_sequence INTEGER;\n\
                 ALTER TABLE master_attempts ADD COLUMN cancellation_requested_at_ms INTEGER;\n\
                 ALTER TABLE master_attempts ADD COLUMN cancellation_deadline_at_ms INTEGER;\n\
                 ALTER TABLE master_attempts ADD COLUMN cancellation_ack_sequence INTEGER;\n\
                 PRAGMA user_version = 4;\n\
                 COMMIT;",
            )?;
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
        }
        let version = self.schema_version()?;
        if version == 3 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;\n\
                 ALTER TABLE master_attempts ADD COLUMN cancellation_sequence INTEGER;\n\
                 ALTER TABLE master_attempts ADD COLUMN cancellation_requested_at_ms INTEGER;\n\
                 ALTER TABLE master_attempts ADD COLUMN cancellation_deadline_at_ms INTEGER;\n\
                 ALTER TABLE master_attempts ADD COLUMN cancellation_ack_sequence INTEGER;\n\
                 PRAGMA user_version = 4;\n\
                 COMMIT;",
            )?;
        }
        let version = self.schema_version()?;
        if version == 4 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE feature_conveyor_state (
                   singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
                   queue_revision INTEGER NOT NULL CHECK (queue_revision >= 0)
                 );
                 INSERT INTO feature_conveyor_state (singleton, queue_revision)
                   VALUES (1, 0);
                 CREATE TABLE feature_repository_grants (
                   repository_id TEXT NOT NULL,
                   grant_kind TEXT NOT NULL CHECK (
                     grant_kind IN (
                       'registration', 'cloud_disclosure', 'autonomous_publication'
                     )
                   ),
                   revision INTEGER NOT NULL CHECK (revision > 0),
                   scope_sha256 BLOB NOT NULL CHECK (length(scope_sha256) = 32),
                   owner_approval_sha256 BLOB NOT NULL
                     CHECK (length(owner_approval_sha256) = 32),
                   expires_at_ms INTEGER,
                   revoked INTEGER NOT NULL CHECK (revoked IN (0, 1)),
                   created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
                   PRIMARY KEY (repository_id, grant_kind, revision)
                 );
                 CREATE TABLE feature_specification_revisions (
                   feature_id TEXT NOT NULL,
                   revision INTEGER NOT NULL CHECK (revision > 0),
                   repository_id TEXT NOT NULL,
                   canonical_manifest_json TEXT NOT NULL
                     CHECK (
                       length(canonical_manifest_json) > 1
                       AND length(CAST(canonical_manifest_json AS BLOB)) <= 262144
                     ),
                   manifest_sha256 BLOB NOT NULL CHECK (length(manifest_sha256) = 32),
                   design_sha256 BLOB NOT NULL CHECK (length(design_sha256) = 32),
                   brainstorming_sha256 BLOB NOT NULL
                     CHECK (length(brainstorming_sha256) = 32),
                   owner_approval_sha256 BLOB NOT NULL
                     CHECK (length(owner_approval_sha256) = 32),
                   registration_grant_revision INTEGER NOT NULL
                     CHECK (registration_grant_revision > 0),
                   cloud_disclosure_grant_revision INTEGER NOT NULL
                     CHECK (cloud_disclosure_grant_revision > 0),
                   publication_grant_revision INTEGER NOT NULL
                     CHECK (publication_grant_revision > 0),
                   provider_id TEXT NOT NULL CHECK (
                     length(provider_id) BETWEEN 1 AND 128
                   ),
                   model_id TEXT NOT NULL CHECK (
                     length(model_id) BETWEEN 1 AND 128
                   ),
                   approved_at_ms INTEGER NOT NULL CHECK (approved_at_ms >= 0),
                   PRIMARY KEY (feature_id, revision)
                 );
                 CREATE TABLE feature_conveyor_features (
                   feature_id TEXT PRIMARY KEY NOT NULL,
                   current_specification_revision INTEGER NOT NULL
                     CHECK (current_specification_revision > 0),
                   status TEXT NOT NULL CHECK (
                     status IN (
                       'queued', 'implementing', 'validating', 'reviewing',
                       'publishing', 'verifying_main', 'succeeded', 'cancelled',
                       'abandoned', 'quarantined'
                     )
                   ),
                   lifecycle_revision INTEGER NOT NULL CHECK (lifecycle_revision > 0),
                   queue_position INTEGER NOT NULL CHECK (queue_position > 0),
                   effect_possible INTEGER NOT NULL CHECK (effect_possible IN (0, 1)),
                   created_at_ms INTEGER NOT NULL CHECK (created_at_ms >= 0),
                   updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= created_at_ms),
                   FOREIGN KEY (feature_id, current_specification_revision)
                     REFERENCES feature_specification_revisions(feature_id, revision)
                 );
                 CREATE TABLE feature_dependencies (
                   feature_id TEXT NOT NULL REFERENCES feature_conveyor_features(feature_id),
                   dependency_feature_id TEXT NOT NULL
                     REFERENCES feature_conveyor_features(feature_id),
                   PRIMARY KEY (feature_id, dependency_feature_id),
                   CHECK (feature_id <> dependency_feature_id)
                 );
                 CREATE TABLE feature_conveyor_queue (
                   feature_id TEXT PRIMARY KEY NOT NULL
                     REFERENCES feature_conveyor_features(feature_id),
                   queue_position INTEGER NOT NULL UNIQUE CHECK (queue_position > 0)
                 );
                 CREATE TABLE feature_active_lease (
                   singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
                   feature_id TEXT NOT NULL UNIQUE
                     REFERENCES feature_conveyor_features(feature_id),
                   lease_id TEXT NOT NULL UNIQUE,
                   claimed_at_ms INTEGER NOT NULL CHECK (claimed_at_ms >= 0)
                 );
                 CREATE TABLE feature_transition_evidence (
                   feature_id TEXT NOT NULL
                     REFERENCES feature_conveyor_features(feature_id),
                   lifecycle_revision INTEGER NOT NULL CHECK (lifecycle_revision > 0),
                   from_status TEXT NOT NULL,
                   to_status TEXT NOT NULL,
                   repository_snapshot_sha256 BLOB
                     CHECK (
                       repository_snapshot_sha256 IS NULL
                       OR length(repository_snapshot_sha256) = 32
                     ),
                   accepted_evidence_sha256 BLOB
                     CHECK (
                       accepted_evidence_sha256 IS NULL
                       OR length(accepted_evidence_sha256) = 32
                     ),
                   verified_main_commit_sha256 BLOB
                     CHECK (
                       verified_main_commit_sha256 IS NULL
                       OR length(verified_main_commit_sha256) = 32
                     ),
                   post_merge_evidence_sha256 BLOB
                     CHECK (
                       post_merge_evidence_sha256 IS NULL
                       OR length(post_merge_evidence_sha256) = 32
                     ),
                   safe_reconciliation_sha256 BLOB
                     CHECK (
                       safe_reconciliation_sha256 IS NULL
                       OR length(safe_reconciliation_sha256) = 32
                     ),
                   verified_healthy_main_sha256 BLOB
                     CHECK (
                       verified_healthy_main_sha256 IS NULL
                       OR length(verified_healthy_main_sha256) = 32
                     ),
                   recorded_at_ms INTEGER NOT NULL CHECK (recorded_at_ms >= 0),
                   PRIMARY KEY (feature_id, lifecycle_revision)
                 );
                 CREATE TABLE feature_conveyor_audit (
                   audit_id INTEGER PRIMARY KEY AUTOINCREMENT,
                   event_kind TEXT NOT NULL CHECK (
                     length(event_kind) BETWEEN 1 AND 96
                   ),
                   feature_id TEXT,
                   occurred_at_ms INTEGER NOT NULL CHECK (occurred_at_ms >= 0),
                   redacted_metadata_json TEXT NOT NULL CHECK (
                     length(CAST(redacted_metadata_json AS BLOB)) <= 8192
                   )
                 );
                 CREATE TRIGGER feature_specification_revisions_no_update
                   BEFORE UPDATE ON feature_specification_revisions
                   BEGIN SELECT RAISE(ABORT, 'immutable feature specification'); END;
                 CREATE TRIGGER feature_specification_revisions_no_delete
                   BEFORE DELETE ON feature_specification_revisions
                   BEGIN SELECT RAISE(ABORT, 'immutable feature specification'); END;
                 CREATE TRIGGER feature_repository_grants_no_update
                   BEFORE UPDATE ON feature_repository_grants
                   BEGIN SELECT RAISE(ABORT, 'immutable repository grant'); END;
                 CREATE TRIGGER feature_repository_grants_no_delete
                   BEFORE DELETE ON feature_repository_grants
                   BEGIN SELECT RAISE(ABORT, 'immutable repository grant'); END;
                 CREATE TRIGGER feature_conveyor_audit_no_update
                   BEFORE UPDATE ON feature_conveyor_audit
                   BEGIN SELECT RAISE(ABORT, 'append-only feature audit'); END;
                 CREATE TRIGGER feature_conveyor_audit_no_delete
                   BEFORE DELETE ON feature_conveyor_audit
                   BEGIN SELECT RAISE(ABORT, 'append-only feature audit'); END;
                 CREATE TRIGGER feature_transition_evidence_no_update
                   BEFORE UPDATE ON feature_transition_evidence
                   BEGIN SELECT RAISE(ABORT, 'immutable transition evidence'); END;
                 CREATE TRIGGER feature_transition_evidence_no_delete
                   BEFORE DELETE ON feature_transition_evidence
                   BEGIN SELECT RAISE(ABORT, 'immutable transition evidence'); END;
                 CREATE INDEX feature_conveyor_features_status_idx
                   ON feature_conveyor_features(status, queue_position);
                 CREATE INDEX feature_conveyor_audit_feature_idx
                   ON feature_conveyor_audit(feature_id, audit_id);
                 PRAGMA user_version = 5;
                 COMMIT;",
            )?;
        }
        let version = self.schema_version()?;
        if version == 5 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 ALTER TABLE master_enrollment_grants
                   RENAME TO master_enrollment_grants_v5;
                 CREATE TABLE master_enrollment_grants (
                   grant_id TEXT PRIMARY KEY NOT NULL,
                   operation TEXT NOT NULL
                     CHECK (operation IN ('enroll', 'rotate', 'capability_rebind')),
                   secret_sha256 BLOB NOT NULL CHECK (length(secret_sha256) = 32),
                   device_id TEXT NOT NULL,
                   device_name TEXT NOT NULL,
                   role_json TEXT NOT NULL,
                   registry_revision INTEGER NOT NULL CHECK (registry_revision > 0),
                   capabilities_json TEXT NOT NULL,
                   source_registration_sha256 BLOB
                     CHECK (
                       source_registration_sha256 IS NULL
                       OR length(source_registration_sha256) = 32
                     ),
                   created_at_ms INTEGER NOT NULL,
                   expires_at_ms INTEGER NOT NULL,
                   consumed_at_ms INTEGER,
                   CHECK (expires_at_ms > created_at_ms),
                   CHECK (
                     (operation = 'capability_rebind' AND source_registration_sha256 IS NOT NULL)
                     OR
                     (operation <> 'capability_rebind' AND source_registration_sha256 IS NULL)
                   )
                 );
                 INSERT INTO master_enrollment_grants
                   (grant_id, operation, secret_sha256, device_id, device_name, role_json,
                    registry_revision, capabilities_json, source_registration_sha256,
                    created_at_ms, expires_at_ms,
                    consumed_at_ms)
                 SELECT grant_id, operation, secret_sha256, device_id, device_name, role_json,
                        registry_revision, capabilities_json, NULL, created_at_ms, expires_at_ms,
                        consumed_at_ms
                 FROM master_enrollment_grants_v5;
                 DROP TABLE master_enrollment_grants_v5;
                 CREATE INDEX master_enrollment_grants_pending_idx
                   ON master_enrollment_grants(consumed_at_ms, expires_at_ms);
                 CREATE TABLE master_pending_capability_rebinds (
                   grant_id TEXT PRIMARY KEY NOT NULL
                     REFERENCES master_enrollment_grants(grant_id),
                   device_id TEXT NOT NULL REFERENCES master_devices(device_id),
                   current_registration_sha256 BLOB NOT NULL
                     CHECK (length(current_registration_sha256) = 32),
                   target_registration_json TEXT NOT NULL,
                   target_registration_sha256 BLOB NOT NULL
                     CHECK (length(target_registration_sha256) = 32),
                   certificate_serial_hex TEXT NOT NULL UNIQUE,
                   certificate_sha256 BLOB NOT NULL UNIQUE
                     CHECK (length(certificate_sha256) = 32),
                   replacement_public_key_x963 BLOB NOT NULL
                     CHECK (
                       length(replacement_public_key_x963) = 65
                       AND hex(substr(replacement_public_key_x963, 1, 1)) = '04'
                     ),
                   issued_at_ms INTEGER NOT NULL,
                   certificate_not_after_ms INTEGER NOT NULL,
                   expires_at_ms INTEGER NOT NULL,
                   status TEXT NOT NULL
                     CHECK (status IN ('pending', 'activated', 'aborted')),
                   terminal_at_ms INTEGER,
                   acknowledgement_sha256 BLOB
                     CHECK (
                       acknowledgement_sha256 IS NULL
                       OR length(acknowledgement_sha256) = 32
                     ),
                   CHECK (certificate_not_after_ms > issued_at_ms),
                   CHECK (expires_at_ms > issued_at_ms),
                   CHECK (length(CAST(target_registration_json AS BLOB)) <= 65536),
                   CHECK (
                     (status = 'pending' AND terminal_at_ms IS NULL
                       AND acknowledgement_sha256 IS NULL) OR
                     (status = 'activated' AND terminal_at_ms IS NOT NULL
                       AND acknowledgement_sha256 IS NOT NULL) OR
                     (status = 'aborted' AND terminal_at_ms IS NOT NULL
                       AND acknowledgement_sha256 IS NULL)
                   )
                 );
                 CREATE TABLE master_identity_rebind_audit (
                   audit_id INTEGER PRIMARY KEY AUTOINCREMENT,
                   event_kind TEXT NOT NULL CHECK (
                     event_kind IN (
                       'grant_created', 'pending_issued', 'activated', 'aborted'
                     )
                   ),
                   grant_id TEXT NOT NULL,
                   device_id TEXT NOT NULL,
                   registry_revision INTEGER NOT NULL CHECK (registry_revision > 0),
                   occurred_at_ms INTEGER NOT NULL CHECK (occurred_at_ms >= 0)
                 );
                 CREATE TRIGGER master_identity_rebind_audit_no_update
                   BEFORE UPDATE ON master_identity_rebind_audit
                   BEGIN SELECT RAISE(ABORT, 'append-only identity rebind audit'); END;
                 CREATE TRIGGER master_identity_rebind_audit_no_delete
                   BEFORE DELETE ON master_identity_rebind_audit
                   BEGIN SELECT RAISE(ABORT, 'append-only identity rebind audit'); END;
                 CREATE INDEX master_identity_rebind_audit_grant_idx
                   ON master_identity_rebind_audit(grant_id, audit_id);
                 CREATE UNIQUE INDEX master_pending_capability_rebinds_device_pending_idx
                   ON master_pending_capability_rebinds(device_id)
                   WHERE status = 'pending';
                 CREATE TRIGGER master_pending_capability_rebinds_no_delete
                   BEFORE DELETE ON master_pending_capability_rebinds
                   BEGIN SELECT RAISE(ABORT, 'durable capability rebind evidence'); END;
                 CREATE TRIGGER master_pending_capability_rebinds_terminal_only
                   BEFORE UPDATE ON master_pending_capability_rebinds
                   WHEN OLD.status <> 'pending'
                     OR NEW.grant_id <> OLD.grant_id
                     OR NEW.device_id <> OLD.device_id
                     OR NEW.current_registration_sha256 <> OLD.current_registration_sha256
                     OR NEW.target_registration_json <> OLD.target_registration_json
                     OR NEW.target_registration_sha256 <> OLD.target_registration_sha256
                     OR NEW.certificate_serial_hex <> OLD.certificate_serial_hex
                     OR NEW.certificate_sha256 <> OLD.certificate_sha256
                     OR NEW.replacement_public_key_x963 <> OLD.replacement_public_key_x963
                     OR NEW.issued_at_ms <> OLD.issued_at_ms
                     OR NEW.certificate_not_after_ms <> OLD.certificate_not_after_ms
                     OR NEW.expires_at_ms <> OLD.expires_at_ms
                     OR NEW.status NOT IN ('activated', 'aborted')
                     OR NEW.terminal_at_ms IS NULL
                     OR (
                       NEW.status = 'activated'
                       AND NEW.acknowledgement_sha256 IS NULL
                     )
                     OR (
                       NEW.status = 'aborted'
                       AND NEW.acknowledgement_sha256 IS NOT NULL
                     )
                   BEGIN SELECT RAISE(ABORT, 'invalid capability rebind evidence transition'); END;
                 PRAGMA user_version = 6;
                 COMMIT;",
            )?;
        }
        let version = self.schema_version()?;
        if version == 6 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 INSERT OR IGNORE INTO master_metadata (key, integer_value)
                   VALUES ('emergency_paused', 0);
                 INSERT OR REPLACE INTO master_metadata (key, integer_value)
                   VALUES ('emergency_pause_revision', 0);
                 PRAGMA user_version = 7;
                 COMMIT;",
            )?;
        }
        let version = self.schema_version()?;
        if version == 7 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE feature_owner_control_state (
                   singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
                   owner_bridge_device_id TEXT REFERENCES master_devices(device_id),
                   owner_bridge_registry_revision INTEGER,
                   designation_revision INTEGER NOT NULL CHECK (designation_revision >= 0),
                   CHECK (
                     (
                       owner_bridge_device_id IS NULL
                       AND owner_bridge_registry_revision IS NULL
                       AND designation_revision = 0
                     ) OR (
                       owner_bridge_device_id IS NOT NULL
                       AND owner_bridge_registry_revision > 0
                       AND designation_revision > 0
                     )
                   )
                 );
                 INSERT INTO feature_owner_control_state (
                   singleton, owner_bridge_device_id,
                   owner_bridge_registry_revision, designation_revision
                 ) VALUES (1, NULL, NULL, 0);
                 PRAGMA user_version = 8;
                 COMMIT;",
            )?;
        }
        let version = self.schema_version()?;
        if version != MASTER_SCHEMA_VERSION {
            return Err(MasterError::UnsupportedSchemaVersion {
                expected: MASTER_SCHEMA_VERSION,
                found: version,
            });
        }
        Ok(())
    }

    fn reconcile_feature_conveyor_startup(&mut self, now_ms: u64) -> Result<u64, MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let active = tx
            .query_row(
                "SELECT f.feature_id, f.status, f.lifecycle_revision
                 FROM feature_active_lease l
                 JOIN feature_conveyor_features f ON f.feature_id = l.feature_id
                 WHERE l.singleton = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?;
        let mut quarantined = 0;
        if let Some((feature_id, status, revision)) = active {
            let status = FeatureLifecycleStatus::parse(&status)?;
            if status.is_active_execution() {
                let changed = tx.execute(
                    "UPDATE feature_conveyor_features
                     SET status = 'quarantined',
                         lifecycle_revision = lifecycle_revision + 1,
                         effect_possible = 1, updated_at_ms = ?1
                     WHERE feature_id = ?2 AND status = ?3
                       AND lifecycle_revision = ?4",
                    params![u64_to_i64(now_ms)?, feature_id, status.as_str(), revision],
                )?;
                if changed != 1 {
                    return Err(MasterError::InvalidStoredState(
                        "active feature changed during startup quarantine".to_string(),
                    ));
                }
                append_feature_audit_tx(
                    &tx,
                    "feature_startup_quarantined",
                    Some(parse_uuid(&feature_id)?),
                    now_ms,
                    serde_json::json!({
                        "from_status": status.as_str(),
                        "to_status": "quarantined",
                        "lifecycle_revision": i64_to_u64(revision)? + 1,
                        "lease_retained": true,
                        "automatic_retry_authorized": false,
                        "effect_possible": true,
                        "side_effect_executed": false
                    }),
                )?;
                quarantined = 1;
            }
        }
        tx.commit()?;
        Ok(quarantined)
    }

    fn reconcile_interrupted_state(
        &mut self,
        now_ms: u64,
    ) -> Result<StartupReconciliation, MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute(
            "UPDATE master_attempts SET cancellation_deadline_at_ms = ?1\n\
             WHERE status = 'cancellation_pending'",
            [u64_to_i64(now_ms)?],
        )?;
        let interrupted_cancellations = reconcile_cancellation_deadlines_tx(&tx, now_ms)?;
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
            abandoned_attempts: abandoned_attempts as u64 + interrupted_cancellations,
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
    cancellation_sequence: Option<u64>,
    cancellation_deadline_at_ms: Option<u64>,
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

fn require_emergency_unpaused_tx(tx: &Transaction<'_>) -> Result<(), MasterError> {
    if emergency_paused_tx(tx)? {
        Err(MasterError::EmergencyPaused)
    } else {
        Ok(())
    }
}

pub(crate) fn emergency_paused_tx(tx: &Transaction<'_>) -> Result<bool, MasterError> {
    let value: i64 = tx.query_row(
        "SELECT integer_value FROM master_metadata WHERE key = 'emergency_paused'",
        [],
        |row| row.get(0),
    )?;
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(MasterError::InvalidStoredState(
            "emergency pause state is not boolean".to_string(),
        )),
    }
}

fn emergency_pause_revision_tx(tx: &Transaction<'_>) -> Result<u64, MasterError> {
    let value: i64 = tx.query_row(
        "SELECT integer_value FROM master_metadata WHERE key = 'emergency_pause_revision'",
        [],
        |row| row.get(0),
    )?;
    i64_to_u64(value)
}

fn request_active_remote_work_cancellations_tx(
    tx: &Transaction<'_>,
    now_ms: u64,
) -> Result<(), MasterError> {
    let mut statement = tx.prepare(
        "SELECT s.task_id, s.step_id FROM master_steps s\n\
         JOIN master_attempts a ON a.step_id = s.step_id\n\
         WHERE s.capability_id IN (?1, ?2) AND s.status = 'leased' AND a.status = 'leased'\n\
         ORDER BY a.leased_at_ms ASC",
    )?;
    let remote_steps = statement
        .query_map(
            [FIXTURE_REASONING_CAPABILITY_ID, MLX_REASONING_CAPABILITY_ID],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    for (task_id, step_id) in remote_steps {
        request_fixture_pause_cancellation_tx(
            tx,
            TaskId::new(parse_uuid(&task_id)?),
            StepId::new(parse_uuid(&step_id)?),
            now_ms,
        )?;
    }
    Ok(())
}

fn request_fixture_pause_cancellation_tx(
    tx: &Transaction<'_>,
    task_id: TaskId,
    step_id: StepId,
    now_ms: u64,
) -> Result<(), MasterError> {
    let (attempt_epoch, connection_epoch, connection_active): (i64, Option<i64>, Option<i64>) = tx
        .query_row(
            "SELECT a.connection_epoch, c.connection_epoch, c.active\n\
             FROM master_attempts a\n\
             LEFT JOIN master_connections c ON c.device_id = a.device_id\n\
             WHERE a.step_id = ?1 AND a.status = 'leased'",
            [step_id.0.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
    if connection_active == Some(1) && connection_epoch == Some(attempt_epoch) {
        return request_leased_step_cancellation_tx(tx, task_id, step_id, now_ms);
    }

    let attempt_changed = tx.execute(
        "UPDATE master_attempts SET status = 'abandoned', completed_at_ms = ?1\n\
         WHERE step_id = ?2 AND status = 'leased'",
        params![u64_to_i64(now_ms)?, step_id.0.to_string()],
    )?;
    let step_changed = tx.execute(
        "UPDATE master_steps SET status = 'cancelled', completed_at_ms = ?1\n\
         WHERE step_id = ?2 AND status = 'leased'",
        params![u64_to_i64(now_ms)?, step_id.0.to_string()],
    )?;
    if attempt_changed != 1 || step_changed != 1 {
        return Err(MasterError::InvalidStoredState(
            "inactive fixture attempt changed before pause cancellation commit".to_string(),
        ));
    }
    append_distributed_event_tx(
        tx,
        DistributedEventKind::StepCancelled,
        now_ms,
        DistributedEventIdentity {
            task_id: Some(task_id),
            step_id: Some(step_id),
            device_id: None,
            connection_epoch: None,
        },
    )?;
    Ok(())
}

fn request_leased_step_cancellation_tx(
    tx: &Transaction<'_>,
    task_id: TaskId,
    step_id: StepId,
    now_ms: u64,
) -> Result<(), MasterError> {
    let (attempt_id, device_id, connection_epoch): (String, String, i64) = tx.query_row(
        "SELECT attempt_id, device_id, connection_epoch FROM master_attempts\n\
         WHERE step_id = ?1 AND status = 'leased'",
        [step_id.0.to_string()],
        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    )?;
    let device_id = DeviceId::new(parse_uuid(&device_id)?);
    let connection_epoch = i64_to_u64(connection_epoch)?;
    let connection = connection_state(tx, device_id)?;
    if !connection.active || connection.epoch != connection_epoch {
        return Err(MasterError::ConnectionNotActive);
    }
    let sequence = connection
        .last_sequence
        .checked_add(1)
        .ok_or(MasterError::IntegerOutOfRange)?;
    let deadline = now_ms
        .checked_add(CANCELLATION_ACK_DEADLINE_MS)
        .ok_or(MasterError::IntegerOutOfRange)?;
    let attempt_changed = tx.execute(
        "UPDATE master_attempts\n\
         SET status = 'cancellation_pending', cancellation_sequence = ?1,\n\
             cancellation_requested_at_ms = ?2, cancellation_deadline_at_ms = ?3\n\
         WHERE attempt_id = ?4 AND status = 'leased'",
        params![
            u64_to_i64(sequence)?,
            u64_to_i64(now_ms)?,
            u64_to_i64(deadline)?,
            attempt_id,
        ],
    )?;
    if attempt_changed != 1 {
        return Err(MasterError::InvalidStoredState(
            "leased attempt changed before cancellation commit".to_string(),
        ));
    }
    let connection_changed = tx.execute(
        "UPDATE master_connections SET last_sequence = ?1\n\
         WHERE device_id = ?2 AND connection_epoch = ?3 AND active = 1",
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
        tx,
        DistributedEventKind::StepCancellationRequested,
        now_ms,
        DistributedEventIdentity {
            task_id: Some(task_id),
            step_id: Some(step_id),
            device_id: Some(device_id),
            connection_epoch: Some(connection_epoch),
        },
    )?;
    Ok(())
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
    let pending_cancellation = tx
        .query_row(
            "SELECT s.task_id, a.step_id FROM master_attempts a\n\
             JOIN master_steps s ON s.step_id = a.step_id\n\
             WHERE a.device_id = ?1 AND a.connection_epoch = ?2\n\
               AND a.status = 'cancellation_pending'",
            params![device_id.0.to_string(), u64_to_i64(connection_epoch)?],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    let mut cancelled_pending = 0_u64;
    if let Some((task_id, step_id)) = pending_cancellation {
        let task_id = TaskId::new(parse_uuid(&task_id)?);
        let step_id = StepId::new(parse_uuid(&step_id)?);
        cancelled_pending = tx.execute(
            "UPDATE master_attempts SET status = 'abandoned', completed_at_ms = ?1\n\
             WHERE step_id = ?2 AND status = 'cancellation_pending'",
            params![u64_to_i64(now_ms)?, step_id.0.to_string()],
        )? as u64;
        tx.execute(
            "UPDATE master_steps SET status = 'cancelled', completed_at_ms = ?1\n\
             WHERE step_id = ?2 AND status = 'leased'",
            params![u64_to_i64(now_ms)?, step_id.0.to_string()],
        )?;
        append_distributed_event_tx(
            tx,
            DistributedEventKind::StepCancellationExpired,
            now_ms,
            DistributedEventIdentity {
                task_id: Some(task_id),
                step_id: Some(step_id),
                device_id: Some(device_id),
                connection_epoch: Some(connection_epoch),
            },
        )?;
        append_distributed_event_tx(
            tx,
            DistributedEventKind::StepCancelled,
            now_ms,
            DistributedEventIdentity {
                task_id: Some(task_id),
                step_id: Some(step_id),
                device_id: None,
                connection_epoch: None,
            },
        )?;
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
        abandoned_attempts: abandoned_attempts as u64 + cancelled_pending,
        requeued_steps,
    })
}

fn reconcile_expired_leases_tx(
    tx: &Transaction<'_>,
    now_ms: u64,
) -> Result<LeaseReconciliation, MasterError> {
    let now = u64_to_i64(now_ms)?;
    let mut statement = tx.prepare(
        "SELECT s.task_id, a.step_id, a.device_id, a.connection_epoch FROM master_attempts a\n\
         JOIN master_steps s ON s.step_id = a.step_id\n\
         WHERE a.status = 'leased' AND a.lease_expires_at_ms <= ?1",
    )?;
    let leased_steps = statement
        .query_map([now], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    let expired_attempts = tx.execute(
        "UPDATE master_attempts SET status = 'expired', completed_at_ms = ?1\n         WHERE status = 'leased' AND lease_expires_at_ms <= ?1",
        [now],
    )?;
    let mut requeued_steps = 0_u64;
    for (task_id, step_id, device_id, connection_epoch) in leased_steps {
        let task_id = TaskId::new(parse_uuid(&task_id)?);
        let step_id = StepId::new(parse_uuid(&step_id)?);
        let device_id = DeviceId::new(parse_uuid(&device_id)?);
        let connection_epoch = i64_to_u64(connection_epoch)?;
        append_distributed_event_tx(
            tx,
            DistributedEventKind::StepLeaseExpired,
            now_ms,
            DistributedEventIdentity {
                task_id: Some(task_id),
                step_id: Some(step_id),
                device_id: Some(device_id),
                connection_epoch: Some(connection_epoch),
            },
        )?;
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

fn reconcile_cancellation_deadlines_tx(
    tx: &Transaction<'_>,
    now_ms: u64,
) -> Result<u64, MasterError> {
    let mut statement = tx.prepare(
        "SELECT a.attempt_id, s.task_id, a.step_id, a.device_id, a.connection_epoch\n\
         FROM master_attempts a JOIN master_steps s ON s.step_id = a.step_id\n\
         WHERE a.status = 'cancellation_pending' AND a.cancellation_deadline_at_ms <= ?1",
    )?;
    let rows = statement
        .query_map([u64_to_i64(now_ms)?], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    let mut expired = 0_u64;
    for (attempt_id, task_id, step_id, device_id, connection_epoch) in rows {
        let task_id = TaskId::new(parse_uuid(&task_id)?);
        let step_id = StepId::new(parse_uuid(&step_id)?);
        let device_id = DeviceId::new(parse_uuid(&device_id)?);
        let connection_epoch = i64_to_u64(connection_epoch)?;
        let changed = tx.execute(
            "UPDATE master_attempts SET status = 'abandoned', completed_at_ms = ?1\n\
             WHERE attempt_id = ?2 AND status = 'cancellation_pending'",
            params![u64_to_i64(now_ms)?, attempt_id],
        )?;
        if changed != 1 {
            continue;
        }
        expired += 1;
        tx.execute(
            "UPDATE master_steps SET status = 'cancelled', completed_at_ms = ?1\n\
             WHERE step_id = ?2 AND status = 'leased'",
            params![u64_to_i64(now_ms)?, step_id.0.to_string()],
        )?;
        append_distributed_event_tx(
            tx,
            DistributedEventKind::StepCancellationExpired,
            now_ms,
            DistributedEventIdentity {
                task_id: Some(task_id),
                step_id: Some(step_id),
                device_id: Some(device_id),
                connection_epoch: Some(connection_epoch),
            },
        )?;
        append_distributed_event_tx(
            tx,
            DistributedEventKind::StepCancelled,
            now_ms,
            DistributedEventIdentity {
                task_id: Some(task_id),
                step_id: Some(step_id),
                device_id: None,
                connection_epoch: None,
            },
        )?;
        disconnect_device_tx(tx, device_id, connection_epoch, now_ms)?;
    }
    Ok(expired)
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
        "SELECT device_id, status, job_json, lease_expires_at_ms,\n\
                cancellation_sequence, cancellation_deadline_at_ms\n\
         FROM master_attempts WHERE attempt_id = ?1",
        [attempt_id.0.to_string()],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, Option<i64>>(5)?,
            ))
        },
    )
    .optional()?
    .map(
        |(
            device_id,
            status,
            job_json,
            lease_expires_at_ms,
            cancellation_sequence,
            cancellation_deadline_at_ms,
        )| {
            Ok(StoredAttempt {
                device_id: DeviceId::new(parse_uuid(&device_id)?),
                status: AttemptStatus::parse(&status)?,
                job_json,
                lease_expires_at_ms: i64_to_u64(lease_expires_at_ms)?,
                cancellation_sequence: cancellation_sequence.map(i64_to_u64).transpose()?,
                cancellation_deadline_at_ms: cancellation_deadline_at_ms
                    .map(i64_to_u64)
                    .transpose()?,
            })
        },
    )
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

fn parse_stored_boolean(value: i64, field: &str) -> Result<bool, MasterError> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(MasterError::InvalidStoredState(format!(
            "{field} is not boolean"
        ))),
    }
}

fn is_constraint_violation(error: &rusqlite::Error) -> bool {
    matches!(
        error,
        rusqlite::Error::SqliteFailure(code, _)
            if code.code == rusqlite::ErrorCode::ConstraintViolation
    )
}

fn validate_repository_grant(grant: &RepositoryGrantRevision) -> Result<(), MasterError> {
    if grant.repository_id.is_nil()
        || grant.revision == 0
        || grant.scope_sha256 == [0; 32]
        || grant.owner_approval_sha256 == [0; 32]
    {
        return Err(MasterError::InvalidFeatureConveyorInput(
            "grant identity, revision, scope digest, and owner approval digest are required"
                .to_string(),
        ));
    }
    Ok(())
}

fn insert_repository_grant_revision_tx(
    tx: &Transaction<'_>,
    grant: &RepositoryGrantRevision,
    now_ms: u64,
) -> Result<(), MasterError> {
    let inserted = tx.execute(
        "INSERT INTO feature_repository_grants (
           repository_id, grant_kind, revision, scope_sha256,
           owner_approval_sha256, expires_at_ms, revoked, created_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            grant.repository_id.to_string(),
            grant.kind.as_str(),
            u64_to_i64(grant.revision)?,
            grant.scope_sha256.as_slice(),
            grant.owner_approval_sha256.as_slice(),
            grant.expires_at_ms.map(u64_to_i64).transpose()?,
            i64::from(grant.revoked),
            u64_to_i64(now_ms)?,
        ],
    );
    match inserted {
        Ok(1) => {}
        Ok(_) => {
            return Err(MasterError::InvalidStoredState(
                "repository grant insert did not affect one row".to_string(),
            ));
        }
        Err(error) if is_constraint_violation(&error) => {
            return Err(MasterError::RepositoryGrantImmutable);
        }
        Err(error) => return Err(error.into()),
    }
    append_feature_audit_tx(
        tx,
        "repository_grant_revision_recorded",
        None,
        now_ms,
        serde_json::json!({
            "grant_kind": grant.kind.as_str(),
            "revision": grant.revision,
            "revoked": grant.revoked,
            "scope_digest_present": true,
            "owner_approval_digest_present": true,
            "side_effect_executed": false
        }),
    )?;
    Ok(())
}

fn repository_grant_view(
    connection: &Connection,
    repository_id: Uuid,
    kind: RepositoryGrantKind,
    now_ms: u64,
) -> Result<Option<FeatureConveyorRepositoryGrantView>, MasterError> {
    let stored = connection
        .query_row(
            "SELECT revision, scope_sha256, owner_approval_sha256, expires_at_ms, revoked
             FROM feature_repository_grants
             WHERE repository_id = ?1 AND grant_kind = ?2
             ORDER BY revision DESC LIMIT 1",
            params![repository_id.to_string(), kind.as_str()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .optional()?;
    let Some((revision, scope_sha256, owner_approval_sha256, expires_at_ms, revoked)) = stored
    else {
        return Ok(None);
    };
    let revoked = parse_stored_boolean(revoked, "repository grant revocation")?;
    let expires_at_ms = expires_at_ms.map(i64_to_u64).transpose()?;
    Ok(Some(FeatureConveyorRepositoryGrantView {
        revision: i64_to_u64(revision)?,
        scope_sha256: digest_array(&scope_sha256)?,
        owner_approval_sha256: digest_array(&owner_approval_sha256)?,
        expires_at_ms,
        revoked,
        active: !revoked && expires_at_ms.is_none_or(|expiry| expiry > now_ms),
    }))
}

fn validate_approved_specification(
    specification: &ApprovedFeatureSpecification,
) -> Result<String, MasterError> {
    if specification.feature_id.is_nil()
        || specification.repository_id.is_nil()
        || specification.revision == 0
        || specification.manifest_sha256 == [0; 32]
        || specification.design_sha256 == [0; 32]
        || specification.brainstorming_sha256 == [0; 32]
        || specification.owner_approval_sha256 == [0; 32]
        || specification.grants.registration == 0
        || specification.grants.cloud_disclosure == 0
        || specification.grants.autonomous_publication == 0
    {
        return Err(MasterError::InvalidFeatureConveyorInput(
            "approved specification identity, revisions, and exact digests are required"
                .to_string(),
        ));
    }
    if !specification.manifest.is_object() {
        return Err(MasterError::InvalidFeatureConveyorInput(
            "approved specification manifest must be a JSON object".to_string(),
        ));
    }
    validate_bounded_feature_identifier(&specification.provider_id, "provider")?;
    validate_bounded_feature_identifier(&specification.model_id, "model")?;
    if specification.dependencies.len() > MAX_CONVEYOR_NONTERMINAL_FEATURES as usize {
        return Err(MasterError::InvalidFeatureConveyorInput(
            "dependency count exceeds queue capacity".to_string(),
        ));
    }
    let mut dependencies = specification.dependencies.clone();
    dependencies.sort();
    dependencies.dedup();
    if dependencies.len() != specification.dependencies.len()
        || dependencies
            .iter()
            .any(|dependency| dependency.is_nil() || *dependency == specification.feature_id)
    {
        return Err(MasterError::InvalidFeatureConveyorInput(
            "dependencies must be unique, non-nil, and cannot reference the feature itself"
                .to_string(),
        ));
    }
    let canonical = canonical_json(&specification.manifest)?;
    if canonical.len() > MAX_APPROVED_FEATURE_SPECIFICATION_BYTES {
        return Err(MasterError::InvalidFeatureConveyorInput(
            "approved specification exceeds the fixed review envelope".to_string(),
        ));
    }
    let computed: [u8; 32] = Sha256::digest(canonical.as_bytes()).into();
    if computed != specification.manifest_sha256 {
        return Err(MasterError::InvalidFeatureConveyorInput(
            "manifest digest does not match canonical JSON".to_string(),
        ));
    }
    Ok(canonical)
}

fn validate_bounded_feature_identifier(value: &str, label: &str) -> Result<(), MasterError> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && byte != b'\\' && byte != b'"')
    {
        return Err(MasterError::InvalidFeatureConveyorInput(format!(
            "{label} identity is not a bounded visible identifier"
        )));
    }
    Ok(())
}

fn canonical_json(value: &Value) -> Result<String, MasterError> {
    fn write_value(value: &Value, output: &mut String) -> Result<(), MasterError> {
        match value {
            Value::Null => output.push_str("null"),
            Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
            Value::Number(value) => output.push_str(&value.to_string()),
            Value::String(value) => output.push_str(&serde_json::to_string(value)?),
            Value::Array(values) => {
                output.push('[');
                for (index, value) in values.iter().enumerate() {
                    if index != 0 {
                        output.push(',');
                    }
                    write_value(value, output)?;
                }
                output.push(']');
            }
            Value::Object(values) => {
                output.push('{');
                let mut keys = values.keys().collect::<Vec<_>>();
                keys.sort();
                for (index, key) in keys.into_iter().enumerate() {
                    if index != 0 {
                        output.push(',');
                    }
                    output.push_str(&serde_json::to_string(key)?);
                    output.push(':');
                    write_value(&values[key], output)?;
                }
                output.push('}');
            }
        }
        Ok(())
    }
    let mut output = String::new();
    write_value(value, &mut output)?;
    Ok(output)
}

fn owner_control_bridge_designation_connection(
    connection: &Connection,
) -> Result<Option<OwnerControlBridgeDesignation>, MasterError> {
    let (device_id, registry_revision, designation_revision): (Option<String>, Option<i64>, i64) =
        connection.query_row(
            "SELECT owner_bridge_device_id, owner_bridge_registry_revision, designation_revision
         FROM feature_owner_control_state WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
    match (
        device_id,
        registry_revision,
        i64_to_u64(designation_revision)?,
    ) {
        (None, None, 0) => Ok(None),
        (Some(device_id), Some(registry_revision), designation_revision)
            if designation_revision > 0 =>
        {
            Ok(Some(OwnerControlBridgeDesignation {
                device_id: DeviceId::new(parse_uuid(&device_id)?),
                registry_revision: i64_to_u64(registry_revision)?,
                designation_revision,
            }))
        }
        _ => Err(MasterError::InvalidStoredState(
            "owner-control designation is inconsistent".to_string(),
        )),
    }
}

fn device_registration_tx(
    tx: &Transaction<'_>,
    device_id: DeviceId,
) -> Result<DeviceRegistration, MasterError> {
    let stored = tx
        .query_row(
            "SELECT device_name, role_json, registry_revision, capabilities_json, revoked
             FROM master_devices WHERE device_id = ?1",
            [device_id.0.to_string()],
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
    let Some((device_name, role_json, registry_revision, capabilities_json, revoked)) = stored
    else {
        return Err(MasterError::DeviceNotRegistered);
    };
    if revoked != 0 {
        return Err(MasterError::OwnerControlBridgeUnauthorized);
    }
    let registration = DeviceRegistration {
        device_id,
        device_name,
        role: serde_json::from_str(&role_json)?,
        registry_revision: i64_to_u64(registry_revision)?,
        capabilities: serde_json::from_str(&capabilities_json)?,
    };
    let validation = HandshakeRequest {
        protocol_version: PROTOCOL_VERSION,
        device_id: registration.device_id,
        device_name: registration.device_name.clone(),
        role: registration.role,
        registry_revision: registration.registry_revision,
        capabilities: registration.capabilities.clone(),
    };
    validation.validate()?;
    Ok(registration)
}

fn require_owner_control_eligible_registration(
    registration: &DeviceRegistration,
) -> Result<(), MasterError> {
    if registration.role != DeviceRole::MacBridge
        || registration
            .capabilities
            .iter()
            .any(|capability| capability.id == FIXTURE_REASONING_CAPABILITY_ID)
    {
        return Err(MasterError::OwnerControlBridgeUnauthorized);
    }
    Ok(())
}

fn require_owner_control_bridge_tx(
    tx: &Transaction<'_>,
    registration: &DeviceRegistration,
    expected_designation_revision: u64,
) -> Result<(), MasterError> {
    require_owner_control_eligible_registration(registration)?;
    let (device_id, registry_revision, designation_revision): (Option<String>, Option<i64>, i64) =
        tx.query_row(
            "SELECT owner_bridge_device_id, owner_bridge_registry_revision, designation_revision
         FROM feature_owner_control_state WHERE singleton = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )?;
    let designation_revision = i64_to_u64(designation_revision)?;
    if designation_revision != expected_designation_revision {
        return Err(MasterError::StaleOwnerControlDesignationRevision {
            expected: expected_designation_revision,
            found: designation_revision,
        });
    }
    let Some(device_id) = device_id else {
        return Err(MasterError::OwnerControlBridgeNotDesignated);
    };
    let Some(registry_revision) = registry_revision else {
        return Err(MasterError::InvalidStoredState(
            "designated owner-control bridge omitted its registry revision".to_string(),
        ));
    };
    let current = device_registration_tx(tx, registration.device_id)?;
    if parse_uuid(&device_id)? != registration.device_id.0
        || i64_to_u64(registry_revision)? != registration.registry_revision
        || current != *registration
    {
        return Err(MasterError::OwnerControlBridgeUnauthorized);
    }
    Ok(())
}

fn require_unpaused_revision_tx(
    tx: &Transaction<'_>,
    expected_revision: u64,
) -> Result<(), MasterError> {
    let found = emergency_pause_revision_tx(tx)?;
    if found != expected_revision {
        return Err(MasterError::StaleEmergencyPauseRevision {
            expected: expected_revision,
            found,
        });
    }
    if emergency_paused_tx(tx)? {
        return Err(MasterError::EmergencyPaused);
    }
    Ok(())
}

fn require_emergency_pause_revision_tx(
    tx: &Transaction<'_>,
    expected_revision: u64,
) -> Result<(), MasterError> {
    let found = emergency_pause_revision_tx(tx)?;
    if found != expected_revision {
        return Err(MasterError::StaleEmergencyPauseRevision {
            expected: expected_revision,
            found,
        });
    }
    Ok(())
}

fn require_queue_revision_tx(
    tx: &Transaction<'_>,
    expected_queue_revision: u64,
) -> Result<(), MasterError> {
    let found: i64 = tx.query_row(
        "SELECT queue_revision FROM feature_conveyor_state WHERE singleton = 1",
        [],
        |row| row.get(0),
    )?;
    let found = i64_to_u64(found)?;
    if found != expected_queue_revision {
        return Err(MasterError::StaleFeatureQueueRevision {
            expected: expected_queue_revision,
            found,
        });
    }
    Ok(())
}

fn increment_queue_revision_tx(
    tx: &Transaction<'_>,
    expected_queue_revision: u64,
) -> Result<u64, MasterError> {
    let next = expected_queue_revision
        .checked_add(1)
        .ok_or(MasterError::IntegerOutOfRange)?;
    let changed = tx.execute(
        "UPDATE feature_conveyor_state SET queue_revision = ?1
         WHERE singleton = 1 AND queue_revision = ?2",
        params![u64_to_i64(next)?, u64_to_i64(expected_queue_revision)?],
    )?;
    if changed != 1 {
        let found: i64 = tx.query_row(
            "SELECT queue_revision FROM feature_conveyor_state WHERE singleton = 1",
            [],
            |row| row.get(0),
        )?;
        return Err(MasterError::StaleFeatureQueueRevision {
            expected: expected_queue_revision,
            found: i64_to_u64(found)?,
        });
    }
    Ok(next)
}

fn validate_repository_preflight_binding(
    repository_id: Uuid,
    registration_grant_revision: u64,
    scope_sha256: &[u8; 32],
    now_ms: u64,
) -> Result<(), MasterError> {
    if repository_id.is_nil()
        || registration_grant_revision == 0
        || *scope_sha256 == [0; 32]
        || now_ms == 0
    {
        return Err(MasterError::InvalidFeatureConveyorInput(
            "repository preflight requires exact nonzero identity, grant, scope, and observation time"
                .to_string(),
        ));
    }
    Ok(())
}

fn require_repository_preflight_binding(
    connection: &Connection,
    repository_id: Uuid,
    registration_grant_revision: u64,
    scope_sha256: &[u8; 32],
    expected_emergency_pause_revision: u64,
    now_ms: u64,
) -> Result<(), MasterError> {
    let row = connection.query_row(
        "SELECT paused.integer_value, pause_revision.integer_value,
                grant.revision, grant.scope_sha256, grant.expires_at_ms, grant.revoked
         FROM master_metadata paused
         JOIN master_metadata pause_revision
           ON pause_revision.key = 'emergency_pause_revision'
         LEFT JOIN feature_repository_grants grant
           ON grant.repository_id = ?1
          AND grant.grant_kind = 'registration'
          AND grant.revision = (
            SELECT MAX(current.revision)
            FROM feature_repository_grants current
            WHERE current.repository_id = ?1
              AND current.grant_kind = 'registration'
          )
         WHERE paused.key = 'emergency_paused'",
        [repository_id.to_string()],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?,
                row.get::<_, Option<Vec<u8>>>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, Option<i64>>(5)?,
            ))
        },
    )?;
    let paused = parse_stored_boolean(row.0, "emergency pause state")?;
    let pause_revision = i64_to_u64(row.1)?;
    if pause_revision != expected_emergency_pause_revision {
        return Err(MasterError::StaleEmergencyPauseRevision {
            expected: expected_emergency_pause_revision,
            found: pause_revision,
        });
    }
    if paused {
        return Err(MasterError::EmergencyPaused);
    }
    let (Some(revision), Some(stored_scope), expires_at_ms, Some(revoked)) =
        (row.2, row.3, row.4, row.5)
    else {
        return Err(MasterError::RepositoryGrantUnavailable);
    };
    if i64_to_u64(revision)? != registration_grant_revision
        || stored_scope.as_slice() != scope_sha256
        || parse_stored_boolean(revoked, "repository grant revoked state")?
        || expires_at_ms
            .map(i64_to_u64)
            .transpose()?
            .is_some_and(|expiry| expiry <= now_ms)
    {
        return Err(MasterError::RepositoryGrantUnavailable);
    }
    Ok(())
}

fn require_grants_tx(
    tx: &Transaction<'_>,
    repository_id: Uuid,
    grants: FeatureGrantRevisions,
    now_ms: u64,
) -> Result<(), MasterError> {
    for (kind, revision) in [
        (RepositoryGrantKind::Registration, grants.registration),
        (
            RepositoryGrantKind::CloudDisclosure,
            grants.cloud_disclosure,
        ),
        (
            RepositoryGrantKind::AutonomousPublication,
            grants.autonomous_publication,
        ),
    ] {
        let row = tx
            .query_row(
                "SELECT revision, expires_at_ms, revoked
                 FROM feature_repository_grants
                 WHERE repository_id = ?1 AND grant_kind = ?2
                 ORDER BY revision DESC LIMIT 1",
                params![repository_id.to_string(), kind.as_str()],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<i64>>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((current_revision, expires_at_ms, revoked)) = row else {
            return Err(MasterError::RepositoryGrantUnavailable);
        };
        if i64_to_u64(current_revision)? != revision
            || revoked != 0
            || expires_at_ms
                .map(i64_to_u64)
                .transpose()?
                .is_some_and(|expiry| expiry <= now_ms)
        {
            return Err(MasterError::RepositoryGrantUnavailable);
        }
    }
    Ok(())
}

fn require_current_feature_grants_tx(
    tx: &Transaction<'_>,
    feature_id: Uuid,
    now_ms: u64,
) -> Result<(), MasterError> {
    let (repository_id, registration, cloud_disclosure, publication): (String, i64, i64, i64) = tx
        .query_row(
            "SELECT s.repository_id, s.registration_grant_revision,
                    s.cloud_disclosure_grant_revision, s.publication_grant_revision
             FROM feature_conveyor_features f
             JOIN feature_specification_revisions s
               ON s.feature_id = f.feature_id
              AND s.revision = f.current_specification_revision
             WHERE f.feature_id = ?1",
            [feature_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .optional()?
        .ok_or(MasterError::FeatureNotFound)?;
    require_grants_tx(
        tx,
        parse_uuid(&repository_id)?,
        FeatureGrantRevisions {
            registration: i64_to_u64(registration)?,
            cloud_disclosure: i64_to_u64(cloud_disclosure)?,
            autonomous_publication: i64_to_u64(publication)?,
        },
        now_ms,
    )
}

fn feature_status_and_revision_tx(
    tx: &Transaction<'_>,
    feature_id: Uuid,
) -> Result<(FeatureLifecycleStatus, u64), MasterError> {
    tx.query_row(
        "SELECT status, lifecycle_revision FROM feature_conveyor_features
         WHERE feature_id = ?1",
        [feature_id.to_string()],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
    )
    .optional()?
    .map(|(status, revision)| {
        Ok::<(FeatureLifecycleStatus, u64), MasterError>((
            FeatureLifecycleStatus::parse(&status)?,
            i64_to_u64(revision)?,
        ))
    })
    .transpose()?
    .ok_or(MasterError::FeatureNotFound)
}

fn require_active_lease_tx(tx: &Transaction<'_>, feature_id: Uuid) -> Result<(), MasterError> {
    let matches: bool = tx.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM feature_active_lease
           WHERE singleton = 1 AND feature_id = ?1
         )",
        [feature_id.to_string()],
        |row| row.get(0),
    )?;
    if !matches {
        return Err(MasterError::InvalidFeatureTransition);
    }
    Ok(())
}

fn append_feature_audit_tx(
    tx: &Transaction<'_>,
    event_kind: &str,
    feature_id: Option<Uuid>,
    occurred_at_ms: u64,
    redacted_metadata: Value,
) -> Result<(), MasterError> {
    if event_kind.is_empty() || event_kind.len() > 96 || !redacted_metadata.is_object() {
        return Err(MasterError::InvalidFeatureConveyorInput(
            "feature audit metadata is invalid".to_string(),
        ));
    }
    let metadata_json = canonical_json(&redacted_metadata)?;
    if metadata_json.len() > 8192 {
        return Err(MasterError::InvalidFeatureConveyorInput(
            "feature audit metadata exceeds the redacted bound".to_string(),
        ));
    }
    tx.execute(
        "INSERT INTO feature_conveyor_audit (
           event_kind, feature_id, occurred_at_ms, redacted_metadata_json
         ) VALUES (?1, ?2, ?3, ?4)",
        params![
            event_kind,
            feature_id.map(|value| value.to_string()),
            u64_to_i64(occurred_at_ms)?,
            metadata_json,
        ],
    )?;
    Ok(())
}

fn sqlite_integrity_ok(connection: &Connection) -> Result<bool, MasterError> {
    let result: String = connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
    Ok(result == "ok")
}

fn prepare_legacy_migration_backup(database_path: &Path) -> Result<Option<PathBuf>, MasterError> {
    if !database_path.exists() {
        return Ok(None);
    }
    let source = Connection::open_with_flags(database_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let version: i64 = source.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if !(1..MASTER_SCHEMA_VERSION).contains(&version) {
        return Ok(None);
    }
    if !sqlite_integrity_ok(&source)? {
        return Err(MasterError::MigrationBackup(format!(
            "source v{version} database failed integrity_check"
        )));
    }
    let backup_path = database_path.with_file_name(format!(
        "master.pre-v{}.{}.sqlite3",
        MASTER_SCHEMA_VERSION,
        Uuid::new_v4()
    ));
    source.execute("VACUUM INTO ?1", [backup_path.to_string_lossy().as_ref()])?;
    drop(source);
    restrict_backup_permissions(&backup_path)?;
    sync_file_contents(&backup_path)?;
    sync_parent_directory(&backup_path)?;
    let backup = Connection::open_with_flags(&backup_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let backup_version: i64 = backup.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if backup_version != version || !sqlite_integrity_ok(&backup)? {
        return Err(MasterError::MigrationBackup(
            "copied legacy backup failed version or integrity verification".to_string(),
        ));
    }
    Ok(Some(backup_path))
}

fn restore_verified_migration_backup(
    database_path: &Path,
    backup_path: &Path,
) -> Result<(), MasterError> {
    let backup = Connection::open_with_flags(backup_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let backup_version: i64 = backup.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if !(1..MASTER_SCHEMA_VERSION).contains(&backup_version) || !sqlite_integrity_ok(&backup)? {
        return Err(MasterError::MigrationBackup(
            "refusing to restore an unverified legacy backup".to_string(),
        ));
    }
    drop(backup);
    let restore_path =
        database_path.with_file_name(format!(".master.restore.{}.sqlite3", Uuid::new_v4()));
    fs::copy(backup_path, &restore_path)?;
    restrict_backup_permissions(&restore_path)?;
    sync_file_contents(&restore_path)?;
    let staged = Connection::open_with_flags(&restore_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let staged_version: i64 = staged.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if staged_version != backup_version || !sqlite_integrity_ok(&staged)? {
        return Err(MasterError::MigrationBackup(
            "staged restore failed version or integrity verification".to_string(),
        ));
    }
    drop(staged);
    atomic_replace_file(&restore_path, database_path)?;
    sync_parent_directory(database_path)?;
    let restored = Connection::open_with_flags(database_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let restored_version: i64 = restored.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if restored_version != backup_version || !sqlite_integrity_ok(&restored)? {
        return Err(MasterError::MigrationBackup(
            "restored database failed version or integrity verification".to_string(),
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn restrict_backup_permissions(path: &Path) -> Result<(), MasterError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_backup_permissions(_path: &Path) -> Result<(), MasterError> {
    Ok(())
}

fn sync_file_contents(path: &Path) -> Result<(), MasterError> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?
        .sync_all()?;
    Ok(())
}

#[cfg(unix)]
fn atomic_replace_file(source: &Path, destination: &Path) -> Result<(), MasterError> {
    fs::rename(source, destination)?;
    Ok(())
}

#[cfg(windows)]
fn atomic_replace_file(source: &Path, destination: &Path) -> Result<(), MasterError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let succeeded = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if succeeded == 0 {
        return Err(MasterError::Io(std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> Result<(), MasterError> {
    let parent = path.parent().ok_or_else(|| {
        MasterError::MigrationBackup("master database has no parent directory".to_string())
    })?;
    File::open(parent)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_parent_directory(_path: &Path) -> Result<(), MasterError> {
    Ok(())
}

pub fn current_time_ms() -> Result<u64, MasterError> {
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| MasterError::InvalidSystemClock)?;
    u64::try_from(duration.as_millis()).map_err(|_| MasterError::InvalidSystemClock)
}

#[cfg(test)]
mod feature_conveyor_unit_tests {
    use super::*;
    use serde_json::json;

    fn digest(label: &str) -> [u8; 32] {
        Sha256::digest(label.as_bytes()).into()
    }

    fn valid_specification() -> ApprovedFeatureSpecification {
        let manifest = json!({
            "z_last": [true, null, 7],
            "a_first": {"quoted": "value"}
        });
        let canonical = canonical_json(&manifest).unwrap();
        ApprovedFeatureSpecification {
            feature_id: Uuid::new_v4(),
            revision: 1,
            repository_id: Uuid::new_v4(),
            manifest,
            manifest_sha256: Sha256::digest(canonical.as_bytes()).into(),
            design_sha256: digest("design"),
            brainstorming_sha256: digest("brainstorming"),
            owner_approval_sha256: digest("approval"),
            grants: FeatureGrantRevisions {
                registration: 1,
                cloud_disclosure: 1,
                autonomous_publication: 1,
            },
            provider_id: "local.review".to_string(),
            model_id: "review-v1".to_string(),
            dependencies: Vec::new(),
        }
    }

    fn assert_invalid_specification(specification: &ApprovedFeatureSpecification) {
        assert!(matches!(
            validate_approved_specification(specification),
            Err(MasterError::InvalidFeatureConveyorInput(_))
        ));
    }

    #[test]
    fn feature_conveyor_unit_canonical_json_is_order_stable_and_compact() {
        let left: Value =
            serde_json::from_str(r#"{"z":[3,{"b":2,"a":1}],"a":"quoted\"value"}"#).unwrap();
        let right: Value =
            serde_json::from_str(r#"{"a":"quoted\"value","z":[3,{"a":1,"b":2}]}"#).unwrap();

        let expected = r#"{"a":"quoted\"value","z":[3,{"a":1,"b":2}]}"#;
        assert_eq!(canonical_json(&left).unwrap(), expected);
        assert_eq!(canonical_json(&right).unwrap(), expected);
    }

    #[test]
    fn feature_conveyor_unit_grant_validation_requires_exact_identity_and_digests() {
        let valid = RepositoryGrantRevision {
            repository_id: Uuid::new_v4(),
            kind: RepositoryGrantKind::Registration,
            revision: 1,
            scope_sha256: digest("scope"),
            owner_approval_sha256: digest("approval"),
            expires_at_ms: None,
            revoked: false,
        };
        assert!(validate_repository_grant(&valid).is_ok());

        for invalid in [
            RepositoryGrantRevision {
                repository_id: Uuid::nil(),
                ..valid.clone()
            },
            RepositoryGrantRevision {
                revision: 0,
                ..valid.clone()
            },
            RepositoryGrantRevision {
                scope_sha256: [0; 32],
                ..valid.clone()
            },
            RepositoryGrantRevision {
                owner_approval_sha256: [0; 32],
                ..valid
            },
        ] {
            assert!(matches!(
                validate_repository_grant(&invalid),
                Err(MasterError::InvalidFeatureConveyorInput(_))
            ));
        }
    }

    #[test]
    fn feature_conveyor_unit_specification_rejects_malformed_identity_and_manifest() {
        let valid = valid_specification();
        assert!(validate_approved_specification(&valid).is_ok());

        let mut invalid = valid.clone();
        invalid.feature_id = Uuid::nil();
        assert_invalid_specification(&invalid);

        let mut invalid = valid.clone();
        invalid.revision = 0;
        assert_invalid_specification(&invalid);

        let mut invalid = valid.clone();
        invalid.manifest = json!(["not", "an", "object"]);
        assert_invalid_specification(&invalid);

        let mut invalid = valid.clone();
        invalid.manifest_sha256 = digest("wrong-manifest");
        assert_invalid_specification(&invalid);

        let mut invalid = valid.clone();
        invalid.provider_id = "contains space".to_string();
        assert_invalid_specification(&invalid);

        let mut invalid = valid.clone();
        invalid.model_id = r#"invalid\model"#.to_string();
        assert_invalid_specification(&invalid);

        let mut invalid = valid;
        invalid.manifest =
            json!({"oversized": "x".repeat(MAX_APPROVED_FEATURE_SPECIFICATION_BYTES)});
        invalid.manifest_sha256 =
            Sha256::digest(canonical_json(&invalid.manifest).unwrap().as_bytes()).into();
        assert_invalid_specification(&invalid);
    }

    #[test]
    fn feature_conveyor_unit_specification_rejects_ambiguous_dependencies() {
        let valid = valid_specification();
        let dependency = Uuid::new_v4();

        let mut invalid = valid.clone();
        invalid.dependencies = vec![dependency, dependency];
        assert_invalid_specification(&invalid);

        let mut invalid = valid.clone();
        invalid.dependencies = vec![Uuid::nil()];
        assert_invalid_specification(&invalid);

        let mut invalid = valid.clone();
        invalid.dependencies = vec![invalid.feature_id];
        assert_invalid_specification(&invalid);

        let mut invalid = valid;
        invalid.dependencies = (0..=MAX_CONVEYOR_NONTERMINAL_FEATURES)
            .map(|_| Uuid::new_v4())
            .collect();
        assert_invalid_specification(&invalid);
    }

    #[test]
    fn feature_conveyor_unit_status_boolean_parser_is_exact() {
        assert!(!parse_stored_boolean(0, "feature effect_possible").unwrap());
        assert!(parse_stored_boolean(1, "feature effect_possible").unwrap());
        for invalid in [-1, 2, i64::MAX] {
            assert!(matches!(
                parse_stored_boolean(invalid, "feature effect_possible"),
                Err(MasterError::InvalidStoredState(_))
            ));
        }
    }
}
