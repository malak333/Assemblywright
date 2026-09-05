use assemblywright_protocol::{
    feature_conveyor_publication_request_binding_sha256,
    feature_conveyor_publication_required_checks_sha256,
    feature_conveyor_review_request_binding_sha256,
    feature_conveyor_validation_request_binding_sha256, AssemblyLineAutoRunReceipt,
    AssemblyLineAutoRunRequest, AssemblyLineChildEpoch, AssemblyLineEmergencyPauseRequest,
    AssemblyLineLifecycleState, AssemblyLineOwnerProjection, AssemblyLineRepositoryIdentity,
    AssemblyLineRuntimeAvailabilityProjection, AssemblyLineSessionEpoch, AssemblyLineStartReceipt,
    AssemblyLineStartRequest, AssemblyLineState, AssemblyLineStopRequest, AttemptId,
    BrainstormingOwnerApprovalBinding, BrainstormingTargetKind, CancellationAcknowledgement,
    CancellationId, CancellationInstruction, CapabilityDescriptor, ContextHandlingPolicy, DeviceId,
    DeviceRole, DistributedEvent, DistributedEventBatch, DistributedEventBatchRequest,
    DistributedEventCursor, DistributedEventKind, ExecutionActivationReceipt,
    ExecutionCheckpointPhase, ExecutionCheckpointReceipt, ExecutionDescendantScope,
    ExecutionHostPlatform, ExecutionTerminationMode, ExecutionTerminationOutcome,
    ExecutionTerminationReceipt, FeatureBrainstormingDraft, FeatureConveyorActivationBlocker,
    FeatureConveyorActivationEvidenceAdmissionProjection,
    FeatureConveyorActivationEvidenceAdmissionReceipt,
    FeatureConveyorActivationEvidenceAdmissionRequest, FeatureConveyorActivationEvidenceCategory,
    FeatureConveyorActivationEvidenceOrigin, FeatureConveyorActivationEvidenceProjection,
    FeatureConveyorActivationEvidenceReference, FeatureConveyorActivationEvidenceSet,
    FeatureConveyorActivationReceipt, FeatureConveyorActivationRequest,
    FeatureConveyorActivationStatus, FeatureConveyorApprovedSpecification,
    FeatureConveyorArtifactIntegrationPlan, FeatureConveyorArtifactIntegrationReceipt,
    FeatureConveyorArtifactIntegrationRequest, FeatureConveyorArtifactIntegrationStatus,
    FeatureConveyorCodingDispatchReceipt, FeatureConveyorCodingDispatchRequest,
    FeatureConveyorCodingDispatchStatus, FeatureConveyorCodingWorkPacketMetadata,
    FeatureConveyorGrantRevisions, FeatureConveyorOrchestrationAction,
    FeatureConveyorOrchestrationPauseKind, FeatureConveyorOrchestrationProjection,
    FeatureConveyorOrchestrationReason, FeatureConveyorOrchestrationStage,
    FeatureConveyorOwnerActiveFeature, FeatureConveyorOwnerControlProjection,
    FeatureConveyorOwnerLifecycleStatus, FeatureConveyorOwnerOrchestrationControlReceipt,
    FeatureConveyorOwnerOrchestrationControlRequest,
    FeatureConveyorOwnerOrchestrationControlStatus, FeatureConveyorPublicationReceipt,
    FeatureConveyorPublicationRequest, FeatureConveyorPublicationStatus,
    FeatureConveyorRemoteAbandonAndAdvanceRequest, FeatureConveyorRemoteCancelActiveFeatureRequest,
    FeatureConveyorRepositoryGrantSet, FeatureConveyorRepositoryGrantView,
    FeatureConveyorReviewDecision, FeatureConveyorReviewGatewayReceipt,
    FeatureConveyorReviewGatewayRequest, FeatureConveyorReviewGatewayStatus,
    FeatureConveyorReviewPacket, FeatureConveyorReviewProviderOutput,
    FeatureConveyorValidationCommandId, FeatureConveyorValidationGateReceipt,
    FeatureConveyorValidationGateRequest, FeatureConveyorValidationGateStatus,
    FeatureQueueEntryProjection, FeatureQueueLifecycle, FrozenBrainstormingSpecification,
    HandshakeRequest, HandshakeResponse, HandshakeStatus, JobEnvelope, JobResultEnvelope,
    JobResultStatus, LeaseId, LocalCodingJobRequest, LocalCodingJobResult,
    LocalCodingResultArtifactAdmission, LocalCodingResultArtifactReceipt,
    LocalCodingSnapshotChunkRequest, LocalModelSelectionProjection, LocalModelSelectionReceipt,
    LocalModelSelectionRequest, LocalModelSelectionStatus, OrchestratorCatalog,
    OrchestratorProfile, ProjectBrainstormingDraft, ProjectVisibility, ProtocolError,
    RepositoryCreationLifecycle, RepositoryCreationProjection, RuntimeAvailabilityStatus,
    RuntimeComponentAvailability, RuntimeUnavailableReason, Sensitivity, StepId, TaskId,
    CANCELLATION_ACK_DEADLINE_MS, FEATURE_CONVEYOR_ORCHESTRATION_SCHEMA_VERSION,
    FEATURE_CONVEYOR_OWNER_ACTIVATION_SCHEMA_VERSION,
    FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
    FEATURE_CONVEYOR_PUBLICATION_COORDINATOR_SCHEMA_VERSION, FEATURE_CONVEYOR_REVIEW_BACKOFF_MS,
    FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION,
    FEATURE_CONVEYOR_VALIDATION_GATE_SCHEMA_VERSION, FIXTURE_REASONING_CAPABILITY_ID,
    FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION, LOCAL_CODING_CAPABILITY_ID,
    LOCAL_MODEL_SELECTION_SCHEMA_VERSION, MAX_ASSEMBLY_LINE_QUEUE_COUNT, MAX_CAPABILITY_ID_BYTES,
    MAX_FEATURE_CONVEYOR_ACTIVE_PROCESSING_MS, MAX_FEATURE_CONVEYOR_REPLACEMENT_CANDIDATES,
    MAX_FEATURE_CONVEYOR_REVIEW_CALLS_PER_FEATURE,
    MAX_FEATURE_CONVEYOR_REVIEW_REQUIREMENT_COVERAGE,
    MAX_FEATURE_CONVEYOR_REVIEW_TRANSPORT_ATTEMPTS_PER_CANDIDATE, MAX_JOB_CONTEXT_BYTES,
    MAX_LEASE_DURATION_MS, MAX_STEP_DEADLINE_MS, MLX_REASONING_CAPABILITY_ID, PROTOCOL_VERSION,
};
use rusqlite::{
    params, Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

use fs2::FileExt;

pub mod execution_ipc;
mod github_publication;
mod identity;
mod integration;
mod planning_effects;
mod planning_runtime;
pub mod publication;
mod result_artifact;
mod review_provider;
mod snapshot;
pub mod validation_containment;
#[cfg(windows)]
pub mod windows_execution_ipc;

#[cfg(windows)]
pub use github_publication::github_publication_launcher_exit_code;
pub use github_publication::{
    credential_git_process_boundary, execute_github_publication_live_proof,
    sanitized_publication_command_path, validate_github_branch_protection_observation,
    validate_github_required_checks_observation, validate_github_workflow_content,
    validate_proof_cleanup_status, validate_proof_source_binding, validate_remote_base_observation,
    GithubPublicationConfigError, GithubPublicationLiveProofReceipt, GithubPublicationSession,
    ProcessGithubPublication, GITHUB_PUBLICATION_ACTION_DEADLINE,
};
pub use integration::{
    ArtifactIntegrationError, ArtifactIntegrationStore, CandidateEvidence, IntegrationArtifact,
    PreparedCandidate, ValidationCandidateScratch,
};
pub use planning_effects::{
    run_brainstorming, run_brainstorming_authorized, run_github_repository_creation,
    BrainstormingAdapter, BrainstormingAdapterBinding, BrainstormingAdapterError,
    BrainstormingCloudAuthorization, BrainstormingDraft, GithubRepositoryCreationAdapter,
    GithubRepositoryCreationError, GithubRepositoryObservation, PlanningEffectControl,
    WindowsPlanningEffectAuthority,
};
pub use planning_runtime::{
    PlanningRuntime, PlanningRuntimeConfigError, PlanningRuntimeStatus, PLANNING_EFFECT_DEADLINE,
};
pub use publication::{PublicationAdapter, PublicationAdapterError, PublicationExecutionControl};
pub use result_artifact::{
    PreparedResultArtifact, ResultArtifactReference, ResultArtifactStore, ResultArtifactStoreError,
    VerifiedResultArtifact,
};
#[cfg(windows)]
pub use review_provider::review_provider_launcher_exit_code;
pub use review_provider::{
    execute_review_provider_live_proof, invoke_review_provider, prepare_review_provider_call,
    PreparedReviewProviderCall, ProcessReviewProvider, ReviewProvider, ReviewProviderCapabilities,
    ReviewProviderConfigError, ReviewProviderInvocationError, ReviewProviderLiveProofReceipt,
    ReviewProviderTokenCountError, ReviewProviderTransportError, UnavailableReviewProvider,
    MAX_FEATURE_CONVEYOR_REVIEW_INPUT_TOKENS,
};
pub use snapshot::{PreparedRepositorySnapshot, RepositorySnapshotError, RepositorySnapshotStore};

pub use assemblywright_protocol::{
    FeatureConveyorPublicationAction as PublicationActionKind,
    FeatureConveyorPublicationActionEvidence as PublicationActionEvidence,
};
pub use identity::{
    CapabilityRebindAcknowledgement, CapabilityRebindActivation, EnrollmentGrantReceipt,
    EnrollmentGrantSpec, EnrollmentOperation, EnrollmentRequest, EphemeralServerIdentity,
    IdentityAuthority, IdentityAuthorityReceipt, IdentityError, IssuedDeviceCertificate,
    PendingCapabilityRebindCertificate, PlatformSecretProtector, SecretProtector,
    DEVICE_CERTIFICATE_LIFETIME_MS, ENROLLMENT_GRANT_TTL_MS, MAX_ENROLLED_DEVICES,
    SERVER_CERTIFICATE_LIFETIME_MS,
};

pub const MASTER_SCHEMA_VERSION: i64 = 22;
pub const MAX_QUEUED_OR_LEASED_STEPS: u64 = 256;
pub const MAX_CONCURRENT_JOBS: u64 = 4;
pub const MAX_CONVEYOR_NONTERMINAL_FEATURES: u64 = 100;
pub const MAX_CONVEYOR_STATUS_FEATURES: usize = 100;
pub const MAX_APPROVED_FEATURE_SPECIFICATION_BYTES: usize = 256 * 1024;
pub const FEATURE_CONVEYOR_STATUS_SCHEMA_VERSION: i64 = 9;
pub const MAX_RETAINED_CODING_WORKSPACE_MS: u64 = 60 * 60 * 1000;

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
      COALESCE(SUM(CASE WHEN status = 'repairing' THEN 1 ELSE 0 END), 0),
      COALESCE(SUM(CASE WHEN status = 'paused' THEN 1 ELSE 0 END), 0),
      COALESCE(SUM(CASE WHEN status = 'attention_required' THEN 1 ELSE 0 END), 0),
      COALESCE(SUM(CASE WHEN status = 'failed' THEN 1 ELSE 0 END), 0),
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
    #[error("the local model selection is stale, unsafe, or outside the model-only boundary")]
    LocalModelSelectionRejected,
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
    #[error("the exact snapshot-bound coding dispatch binding is stale or unavailable")]
    FeatureCodingDispatchUnavailable,
    #[error("the exact local-coding result artifact admission is stale or unavailable")]
    ResultArtifactUnavailable,
    #[error("the exact artifact integration binding is stale or unavailable")]
    ArtifactIntegrationUnavailable,
    #[error("artifact integration conflicts with the immutable base")]
    ArtifactIntegrationConflict,
    #[error("the exact validation gate binding is stale or unavailable")]
    ValidationGateUnavailable,
    #[error("required validation evidence did not pass")]
    ValidationGateFailed,
    #[error("the exact independent-review gateway binding is stale or unavailable")]
    ReviewGatewayUnavailable,
    #[error("the independent-review transport retry is not permitted before {next_retry_at_ms}")]
    ReviewRetryNotReady { next_retry_at_ms: u64 },
    #[error("the independent-review call budget is exhausted")]
    ReviewBudgetExhausted,
    #[error("the exact publication coordinator binding is stale or unavailable")]
    PublicationCoordinatorUnavailable,
    #[error("a publication effect is ambiguous and requires reconciliation")]
    PublicationEffectAmbiguous,
    #[error(
        "feature orchestration is inactive until separately activated with durable owner evidence"
    )]
    OrchestrationInactive,
    #[error("feature orchestration revision is stale: expected {expected}, found {found}")]
    StaleOrchestrationRevision { expected: u64, found: u64 },
    #[error("the activation evidence revision or identity is stale or unavailable")]
    FeatureActivationEvidenceUnavailable,
    #[error("the singleton feature orchestration activation is immutable")]
    FeatureActivationImmutable,
    #[error("the orchestration checkpoint is effect-possible and must be quarantined")]
    OrchestrationEffectAmbiguous,
    #[error("feature coding work must be terminal before lifecycle advancement")]
    FeatureCodingWorkOutstanding,
    #[error(
        "feature cancellation retains the active lease and requires explicit safe abandonment"
    )]
    FeatureCancellationBlocksAdvancement,
    #[error("verified healthy main evidence is required")]
    VerifiedHealthyMainRequired,
    #[error("assembly-line planning input is invalid: {0}")]
    InvalidAssemblyLinePlanningInput(String),
    #[error("assembly-line planning record is immutable or conflicts with an existing record")]
    AssemblyLinePlanningImmutable,
    #[error("assembly-line owner-control revision is stale: expected {expected}, found {found}")]
    StaleAssemblyLineOwnerControlRevision { expected: u64, found: u64 },
    #[error("assembly-line state revision is stale: expected {expected}, found {found}")]
    StaleAssemblyLineStateRevision { expected: u64, found: u64 },
    #[error("assembly-line queue revision is stale: expected {expected}, found {found}")]
    StaleAssemblyLineQueueRevision { expected: u64, found: u64 },
    #[error("the assembly-line repository is absent, stale, or not created")]
    AssemblyLineRepositoryUnavailable,
    #[error("the assembly-line queue has reached its bounded capacity")]
    AssemblyLineQueueFull,
    #[error("the selected planning-only brainstorming provider is unavailable")]
    AssemblyLineBrainstormingUnavailable,
    #[error("the planning-only brainstorming provider rejected the bounded request")]
    AssemblyLineBrainstormingRejected,
    #[error("the GitHub repository-creation adapter is unavailable")]
    AssemblyLineGithubCreationUnavailable,
    #[error("the requested GitHub repository already exists or conflicts with the frozen intent")]
    AssemblyLineGithubCreationConflict,
    #[error("the GitHub creation result is ambiguous and requires exact reconciliation")]
    AssemblyLineGithubCreationReconciliationRequired,
    #[error("the assembly-line execution capabilities are absent, unhealthy, or stale")]
    AssemblyLineExecutionCapabilityUnavailable,
    #[error("the assembly-line execution control binding is stale or unavailable")]
    AssemblyLineExecutionControlUnavailable,
    #[error("the assembly-line execution receipt does not match its durable intent")]
    AssemblyLineExecutionReceiptMismatch,
    #[error("feature migration backup failed: {0}")]
    MigrationBackup(String),
    #[error("feature migration failed and backup restoration also failed: migration={migration}; restore={restore}")]
    MigrationAndRestoreFailed { migration: String, restore: String },
    #[error(transparent)]
    RepositorySnapshot(#[from] RepositorySnapshotError),
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
    LocalCoding,
}

impl RemoteWorkContract {
    pub fn from_registration(registration: &DeviceRegistration) -> Result<Self, MasterError> {
        if registration.capabilities.len() != 1 {
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
        if registration.role == DeviceRole::InferenceWorker
            && *capability == CapabilityDescriptor::local_coding()
        {
            return Ok(Self::LocalCoding);
        }
        Err(MasterError::InvalidRemoteWorkContract)
    }

    fn capability(&self) -> CapabilityDescriptor {
        match self {
            Self::Fixture => CapabilityDescriptor::fixture_reasoning(),
            Self::Mlx(capability) => capability.clone(),
            Self::LocalCoding => CapabilityDescriptor::local_coding(),
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
            Self::LocalCoding => {
                job.validate_local_coding()?;
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
            Self::LocalCoding => result.validate_local_coding_result(job)?,
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
    Repairing,
    Paused,
    AttentionRequired,
    Failed,
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
            Self::Repairing => "repairing",
            Self::Paused => "paused",
            Self::AttentionRequired => "attention_required",
            Self::Failed => "failed",
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
            "repairing" => Ok(Self::Repairing),
            "paused" => Ok(Self::Paused),
            "attention_required" => Ok(Self::AttentionRequired),
            "failed" => Ok(Self::Failed),
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
                | Self::Repairing
                | Self::Paused
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

#[derive(Debug, Clone, Copy)]
struct FeatureOwnerResolutionBinding {
    feature_id: Uuid,
    expected_lifecycle_revision: u64,
    expected_queue_revision: u64,
    expected_emergency_pause_revision: u64,
}

#[derive(Debug, Clone, Copy)]
struct OwnerControlBridgeBinding<'a> {
    registration: &'a DeviceRegistration,
    expected_designation_revision: u64,
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
    pub repairing: u64,
    pub paused: u64,
    pub attention_required: u64,
    pub failed: u64,
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
    pub snapshot_id: Uuid,
    pub snapshot_sha256: [u8; 32],
    pub base_commit: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureSnapshotClaimPlan {
    pub feature_id: Uuid,
    pub specification_revision: u64,
    pub repository_id: Uuid,
    pub expected_queue_revision: u64,
    pub expected_emergency_pause_revision: u64,
    pub scope_sha256: [u8; 32],
    pub provider_id: String,
    pub model_id: String,
    pub grants: FeatureGrantRevisions,
    pub base_commit: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositorySnapshotEvidence {
    pub snapshot_id: Uuid,
    pub snapshot_sha256: [u8; 32],
    pub base_commit: String,
}

#[derive(Debug, Clone)]
pub struct ArtifactIntegrationPlan {
    pub request: FeatureConveyorArtifactIntegrationRequest,
    pub artifacts: Vec<IntegrationArtifact>,
}

#[derive(Debug, Clone)]
pub enum ArtifactIntegrationAuthorization {
    Existing(FeatureConveyorArtifactIntegrationReceipt),
    Planned(ArtifactIntegrationPlan),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationGateExecutionPlan {
    pub request: FeatureConveyorValidationGateRequest,
    pub candidate: CandidateEvidence,
    pub approved_paths: Vec<String>,
    pub acceptance_criteria_count: u64,
    pub requirements_sha256: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ValidationCommandEvidence {
    pub command_id: FeatureConveyorValidationCommandId,
    pub passed: bool,
    pub result_sha256: [u8; 32],
    pub duration_ms: u64,
    pub output_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ValidationGateEvidence {
    pub commands: Vec<ValidationCommandEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationGateAuthorization {
    ExistingPassed {
        receipt: FeatureConveyorValidationGateReceipt,
        candidate: CandidateEvidence,
    },
    ExistingFailed,
    Planned(ValidationGateExecutionPlan),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReviewGatewayExecutionPlan {
    pub request: FeatureConveyorReviewGatewayRequest,
    pub candidate: CandidateEvidence,
    pub approved_specification: Value,
    pub approved_specification_sha256: [u8; 32],
    pub requirements_sha256: [u8; 32],
    pub requirement_ids: Vec<String>,
    pub evidence_digests: Vec<[u8; 32]>,
    pub candidate_attempt: u8,
    pub feature_call: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ReviewGatewayAuthorization {
    ExistingDecision(Box<FeatureConveyorReviewGatewayReceipt>),
    ExistingTransportFailure { next_retry_at_ms: u64 },
    Planned(Box<ReviewGatewayExecutionPlan>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewTransportFailure {
    ProviderOutage,
    MalformedOutput,
    IncompleteTransport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicationExecutionPlan {
    pub request: FeatureConveyorPublicationRequest,
    pub repository_id: Uuid,
    pub feature_branch: String,
    pub base_branch: String,
    pub required_checks: Vec<String>,
    pub merge_strategy: String,
    pub post_merge_gate: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublicationAuthorization {
    Existing(Box<FeatureConveyorPublicationReceipt>),
    Planned(Box<PublicationExecutionPlan>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DerivedOrchestrationDecision {
    stage: FeatureConveyorOrchestrationStage,
    action: FeatureConveyorOrchestrationAction,
    reason: FeatureConveyorOrchestrationReason,
    pause_kind: Option<FeatureConveyorOrchestrationPauseKind>,
    next_retry_at_ms: Option<u64>,
    evidence_sha256: Option<[u8; 32]>,
    effect_possible: bool,
}

pub fn publication_branch_policy_sha256(
    repository_id: Uuid,
    feature_id: Uuid,
    base_branch: &str,
    required_checks: &[String],
    merge_strategy: &str,
    post_merge_gate: &str,
) -> Result<[u8; 32], MasterError> {
    if repository_id.is_nil()
        || feature_id.is_nil()
        || !valid_publication_token(base_branch, 255)
        || required_checks.is_empty()
        || required_checks.len() > assemblywright_protocol::MAX_FEATURE_CONVEYOR_PUBLICATION_CHECKS
        || required_checks
            .iter()
            .any(|check| !valid_publication_token(check, 128))
        || !matches!(merge_strategy, "merge" | "squash" | "rebase")
        || post_merge_gate != "release-local"
    {
        return Err(MasterError::PublicationCoordinatorUnavailable);
    }
    let mut checks = required_checks.to_vec();
    let original_len = checks.len();
    checks.sort();
    checks.dedup();
    if checks.len() != original_len {
        return Err(MasterError::PublicationCoordinatorUnavailable);
    }
    let policy = serde_json::json!({
        "repository_id": repository_id,
        "feature_branch": format!("assemblywright-{feature_id}"),
        "base_branch": base_branch,
        "required_checks": checks,
        "merge_strategy": merge_strategy,
        "post_merge_gate": post_merge_gate,
        "branch_protection_required": true,
        "bypass_allowed": false
    });
    let canonical = canonical_json(&policy)?;
    let mut digest = Sha256::new();
    digest.update(b"assemblywright.publication-branch-policy.v1\0");
    digest.update((canonical.len() as u64).to_be_bytes());
    digest.update(canonical.as_bytes());
    Ok(digest.finalize().into())
}

impl ReviewTransportFailure {
    fn as_str(self) -> &'static str {
        match self {
            Self::ProviderOutage => "provider_outage",
            Self::MalformedOutput => "malformed_output",
            Self::IncompleteTransport => "incomplete_transport",
        }
    }
}

pub struct MasterKernel {
    connection: Connection,
    startup_reconciliation: StartupReconciliation,
    feature_startup_quarantines: u64,
    assembly_line_startup_reconciliation: AssemblyLineStartupReconciliation,
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

/// Provisioned, source-only execution identity binding. Recording this binding
/// does not install, launch, or authorize an executor or broker. A later Start
/// must match every executor identity and revision exactly and the binding must
/// still be marked healthy inside the same durable transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyLineExecutionCapabilityBinding {
    pub binding_revision: u64,
    pub expected_state_revision: u64,
    pub expected_emergency_pause_revision: u64,
    pub windows_executor_id: Uuid,
    pub windows_executor_revision: u64,
    pub windows_executor_sha256: [u8; 32],
    pub mac_executor_id: Uuid,
    pub mac_executor_revision: u64,
    pub mac_executor_sha256: [u8; 32],
    pub windows_broker_id: Uuid,
    pub windows_broker_revision: u64,
    pub windows_broker_sha256: [u8; 32],
    pub mac_broker_id: Uuid,
    pub mac_broker_revision: u64,
    pub mac_broker_sha256: [u8; 32],
    pub protected_control_plane_sha256: [u8; 32],
    pub windows_receipt_signer_key_id: String,
    pub windows_receipt_verifying_key: [u8; 32],
    pub mac_receipt_signer_key_id: String,
    pub mac_receipt_verifying_key: [u8; 32],
    pub healthy: bool,
    pub provisioning_evidence_sha256: [u8; 32],
}

impl AssemblyLineExecutionCapabilityBinding {
    fn validate(&self) -> Result<(), MasterError> {
        for value in [
            self.binding_revision,
            self.expected_state_revision,
            self.windows_executor_revision,
            self.mac_executor_revision,
            self.windows_broker_revision,
            self.mac_broker_revision,
        ] {
            if value == 0 {
                return Err(MasterError::AssemblyLineExecutionCapabilityUnavailable);
            }
        }
        let identities = [
            self.windows_executor_id,
            self.mac_executor_id,
            self.windows_broker_id,
            self.mac_broker_id,
        ];
        if identities.iter().any(Uuid::is_nil)
            || identities
                .iter()
                .enumerate()
                .any(|(index, value)| identities[index + 1..].contains(value))
            || [
                self.windows_executor_sha256,
                self.mac_executor_sha256,
                self.windows_broker_sha256,
                self.mac_broker_sha256,
                self.protected_control_plane_sha256,
                self.provisioning_evidence_sha256,
                self.windows_receipt_verifying_key,
                self.mac_receipt_verifying_key,
            ]
            .contains(&[0; 32])
            || !valid_execution_signer_key_id(&self.windows_receipt_signer_key_id)
            || !valid_execution_signer_key_id(&self.mac_receipt_signer_key_id)
            || self.windows_receipt_signer_key_id == self.mac_receipt_signer_key_id
            || self.windows_receipt_verifying_key == self.mac_receipt_verifying_key
        {
            return Err(MasterError::AssemblyLineExecutionCapabilityUnavailable);
        }
        Ok(())
    }
}

/// Evidence that the product runtime has a concrete authenticated path to both
/// platform executors and their protected brokers. Source capability rows alone
/// never create this status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssemblyLineExecutionRuntimeStatus {
    pub binding_revision: u64,
    pub dispatcher_sha256: [u8; 32],
}

impl AssemblyLineExecutionRuntimeStatus {
    fn validate(self) -> Result<(), MasterError> {
        if self.binding_revision == 0 || self.dispatcher_sha256 == [0; 32] {
            return Err(MasterError::AssemblyLineExecutionCapabilityUnavailable);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyLineStartDispatchIntent {
    pub request_id: Uuid,
    pub session_id: Uuid,
    pub child_epoch_id: Uuid,
    pub authority_revision: u64,
    pub state_revision: u64,
    pub queue_revision: u64,
    pub owner_start_approval_sha256: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AssemblyLineEffectDispatchError {
    #[error("assembly-line host-effect dispatch is unavailable")]
    Unavailable,
    #[error("assembly-line host-effect dispatch outcome is ambiguous")]
    Ambiguous,
}

/// Host effects live behind this boundary. Implementations must authenticate
/// the exact request and make request IDs idempotent; the master still verifies
/// every signed receipt before changing lifecycle state.
pub trait AssemblyLineEffectDispatcher: Send + Sync {
    fn runtime_status(&self) -> Option<AssemblyLineExecutionRuntimeStatus>;

    fn dispatch_start(
        &self,
        intent: &AssemblyLineStartDispatchIntent,
    ) -> Result<Vec<ExecutionActivationReceipt>, AssemblyLineEffectDispatchError>;

    fn dispatch_termination(
        &self,
        intent: &AssemblyLineTerminationIntent,
    ) -> Result<Vec<ExecutionTerminationReceipt>, AssemblyLineEffectDispatchError>;
}

#[derive(Debug, Default)]
pub struct UnavailableAssemblyLineEffectDispatcher;

impl AssemblyLineEffectDispatcher for UnavailableAssemblyLineEffectDispatcher {
    fn runtime_status(&self) -> Option<AssemblyLineExecutionRuntimeStatus> {
        None
    }

    fn dispatch_start(
        &self,
        _intent: &AssemblyLineStartDispatchIntent,
    ) -> Result<Vec<ExecutionActivationReceipt>, AssemblyLineEffectDispatchError> {
        Err(AssemblyLineEffectDispatchError::Unavailable)
    }

    fn dispatch_termination(
        &self,
        _intent: &AssemblyLineTerminationIntent,
    ) -> Result<Vec<ExecutionTerminationReceipt>, AssemblyLineEffectDispatchError> {
        Err(AssemblyLineEffectDispatchError::Unavailable)
    }
}

/// Durable control-plane intent only. `external_effect_performed` remains false
/// until separately installed executors and brokers consume the intent and
/// return matching signed receipts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyLineTerminationIntent {
    pub request_id: Uuid,
    pub session_id: Uuid,
    pub child_epoch_id: Uuid,
    pub mode: ExecutionTerminationMode,
    pub authority_revision: u64,
    pub checkpoint_id: Uuid,
    pub checkpoint_sha256: [u8; 32],
    pub resulting_state: AssemblyLineState,
    pub external_effect_performed: bool,
}

/// Result of durable restart reconciliation for the new Assembly Line state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AssemblyLineStartupReconciliation {
    pub quarantined_effect_possible_session: bool,
    pub pending_termination_intent: bool,
}

struct AssemblyLineTerminationControlBinding {
    request_kind: &'static str,
    request_id: Uuid,
    session_id: Uuid,
    child_epoch_id: Uuid,
    expected_state_revision: u64,
    expected_emergency_pause_revision: Option<u64>,
}

struct AssemblyLineActivationVerificationBinding {
    child_session: String,
    child_authority_revision: i64,
    start_request_id: String,
    windows_executor_id: String,
    windows_executor_revision: i64,
    mac_executor_id: String,
    mac_executor_revision: i64,
    windows_key_id: String,
    windows_key: Vec<u8>,
    mac_key_id: String,
    mac_key: Vec<u8>,
}

pub struct MasterProcess {
    _owner_lock: File,
    data_dir: PathBuf,
    database_path: PathBuf,
    migration_backup_path: Option<PathBuf>,
    result_artifact_store: ResultArtifactStore,
    artifact_integration_store: ArtifactIntegrationStore,
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
        RepositorySnapshotStore::open(&data_dir)?
            .cleanup_unreferenced(&kernel.repository_snapshot_ids()?)?;
        let result_artifact_store = ResultArtifactStore::open(&data_dir)
            .map_err(|error| MasterError::InvalidStoredState(error.to_string()))?;
        let result_artifacts = kernel.result_artifact_references()?;
        result_artifact_store
            .cleanup_unreferenced(
                &result_artifacts
                    .iter()
                    .map(|reference| reference.artifact_id)
                    .collect(),
            )
            .map_err(|error| MasterError::InvalidStoredState(error.to_string()))?;
        result_artifact_store
            .verify_referenced(&result_artifacts)
            .map_err(|error| MasterError::InvalidStoredState(error.to_string()))?;
        let artifact_integration_store = ArtifactIntegrationStore::open(&data_dir)
            .map_err(|error| MasterError::InvalidStoredState(error.to_string()))?;
        let candidates = kernel.candidate_references()?;
        artifact_integration_store
            .cleanup_unreferenced(
                &candidates
                    .iter()
                    .map(|candidate| candidate.integration_id)
                    .collect(),
            )
            .map_err(|error| MasterError::InvalidStoredState(error.to_string()))?;
        artifact_integration_store
            .verify_referenced(&candidates)
            .map_err(|error| MasterError::InvalidStoredState(error.to_string()))?;
        Ok(Self {
            _owner_lock: owner_lock,
            data_dir,
            database_path,
            migration_backup_path,
            result_artifact_store,
            artifact_integration_store,
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

    pub fn result_artifact_store(&self) -> ResultArtifactStore {
        self.result_artifact_store.clone()
    }

    pub fn artifact_integration_store(&self) -> ArtifactIntegrationStore {
        self.artifact_integration_store.clone()
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

    /// Opens one additional connection owned by the running Windows master.
    /// It neither migrates nor performs startup reconciliation; those remain
    /// exclusive to `MasterProcess::acquire`.
    pub fn open_planning_runtime_connection(path: impl AsRef<Path>) -> Result<Self, MasterError> {
        let connection = Connection::open(path)?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;\nPRAGMA busy_timeout = 5000;\nPRAGMA synchronous = FULL;",
        )?;
        let kernel = Self {
            connection,
            startup_reconciliation: StartupReconciliation::default(),
            feature_startup_quarantines: 0,
            assembly_line_startup_reconciliation: AssemblyLineStartupReconciliation::default(),
        };
        let found = kernel.schema_version()?;
        if found != MASTER_SCHEMA_VERSION {
            return Err(MasterError::UnsupportedSchemaVersion {
                expected: MASTER_SCHEMA_VERSION,
                found,
            });
        }
        Ok(kernel)
    }

    fn initialize(connection: Connection) -> Result<Self, MasterError> {
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;\nPRAGMA busy_timeout = 5000;\nPRAGMA synchronous = FULL;",
        )?;
        let mut kernel = Self {
            connection,
            startup_reconciliation: StartupReconciliation::default(),
            feature_startup_quarantines: 0,
            assembly_line_startup_reconciliation: AssemblyLineStartupReconciliation::default(),
        };
        kernel.migrate()?;
        kernel.startup_reconciliation = kernel.reconcile_interrupted_state(current_time_ms()?)?;
        kernel.feature_startup_quarantines =
            kernel.reconcile_feature_conveyor_startup(current_time_ms()?)?;
        kernel.assembly_line_startup_reconciliation =
            kernel.reconcile_assembly_line_startup(current_time_ms()?)?;
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

    pub fn assembly_line_startup_reconciliation(&self) -> AssemblyLineStartupReconciliation {
        self.assembly_line_startup_reconciliation
    }

    pub fn emergency_paused(&self) -> Result<bool, MasterError> {
        Ok(self.emergency_pause_snapshot()?.0)
    }

    pub fn emergency_pause_revision(&self) -> Result<u64, MasterError> {
        Ok(self.emergency_pause_snapshot()?.1)
    }

    pub fn planning_effect_pause_snapshot(&self) -> Result<(bool, u64), MasterError> {
        self.emergency_pause_snapshot()
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
            let active_clock = tx
                .query_row(
                    "SELECT s.feature_id,s.orchestration_revision,
                            s.active_processing_ms,s.clock_started_at_ms
                     FROM feature_orchestration_state s
                     JOIN feature_active_lease l ON l.feature_id=s.feature_id
                     WHERE l.singleton=1 AND s.clock_started_at_ms IS NOT NULL",
                    [],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, i64>(1)?,
                            row.get::<_, i64>(2)?,
                            row.get::<_, i64>(3)?,
                        ))
                    },
                )
                .optional()?;
            if let Some((feature_id, orchestration_revision, active_ms, clock_started_at_ms)) =
                active_clock
            {
                let feature_id = parse_uuid(&feature_id)?;
                let orchestration_revision = i64_to_u64(orchestration_revision)?;
                let active_ms = i64_to_u64(active_ms)?;
                if active_ms > MAX_FEATURE_CONVEYOR_ACTIVE_PROCESSING_MS {
                    return Err(MasterError::InvalidStoredState(
                        "orchestration active-processing budget is invalid".to_string(),
                    ));
                }
                let clock_started_at_ms = i64_to_u64(clock_started_at_ms)?;
                let elapsed_ms = now_ms.saturating_sub(clock_started_at_ms);
                let charged_ms = active_ms
                    .saturating_add(elapsed_ms)
                    .min(MAX_FEATURE_CONVEYOR_ACTIVE_PROCESSING_MS);
                let charged_interval_ms = charged_ms.saturating_sub(active_ms);
                if tx.execute(
                    "UPDATE feature_orchestration_state
                     SET active_processing_ms=?1,clock_started_at_ms=NULL,updated_at_ms=?2
                     WHERE feature_id=?3 AND orchestration_revision=?4
                       AND active_processing_ms=?5 AND clock_started_at_ms=?6",
                    params![
                        u64_to_i64(charged_ms)?,
                        u64_to_i64(now_ms)?,
                        feature_id.to_string(),
                        u64_to_i64(orchestration_revision)?,
                        u64_to_i64(active_ms)?,
                        u64_to_i64(clock_started_at_ms)?,
                    ],
                )? != 1
                {
                    return Err(MasterError::InvalidStoredState(
                        "orchestration clock changed during emergency pause".to_string(),
                    ));
                }
                append_feature_audit_tx(
                    &tx,
                    "feature_orchestration_clock_suspended",
                    Some(feature_id),
                    now_ms,
                    serde_json::json!({
                        "orchestration_revision": orchestration_revision,
                        "elapsed_charged_ms": charged_interval_ms,
                        "active_processing_ms": charged_ms,
                        "active_processing_budget_ms":
                            MAX_FEATURE_CONVEYOR_ACTIVE_PROCESSING_MS,
                        "clock_suspended": true,
                        "emergency_pause": true,
                        "effect_possible": false,
                        "side_effect_executed": false
                    }),
                )?;
            }
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

    pub fn repository_snapshot_ids(&self) -> Result<HashSet<Uuid>, MasterError> {
        let mut statement = self
            .connection
            .prepare("SELECT snapshot_id FROM feature_repository_snapshot_claims")?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .map(|row| {
                let value = row?;
                parse_uuid(&value)
            })
            .collect::<Result<HashSet<_>, MasterError>>()?;
        Ok(ids)
    }

    pub fn result_artifact_ids(&self) -> Result<HashSet<Uuid>, MasterError> {
        let mut statement = self
            .connection
            .prepare("SELECT artifact_id FROM feature_result_artifacts")?;
        let ids = statement
            .query_map([], |row| row.get::<_, String>(0))?
            .map(|value| {
                value
                    .map_err(MasterError::from)
                    .and_then(|value| parse_uuid(&value))
            })
            .collect::<Result<HashSet<_>, _>>()?;
        Ok(ids)
    }

    pub fn result_artifact_references(&self) -> Result<Vec<ResultArtifactReference>, MasterError> {
        let mut statement = self.connection.prepare(
            "SELECT artifact_id, artifact_sha256, artifact_size_bytes
             FROM feature_result_artifacts ORDER BY artifact_id",
        )?;
        let references = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?
            .map(|row| {
                let (artifact_id, digest, size) = row?;
                Ok(ResultArtifactReference {
                    artifact_id: parse_uuid(&artifact_id)?,
                    artifact_sha256: digest_array(&digest)?,
                    artifact_size_bytes: i64_to_u64(size)?,
                })
            })
            .collect::<Result<Vec<_>, MasterError>>()?;
        Ok(references)
    }

    pub fn integration_artifact_references(
        &self,
        integration_id: Uuid,
    ) -> Result<Vec<ResultArtifactReference>, MasterError> {
        let mut statement = self.connection.prepare(
            "SELECT a.artifact_id,a.artifact_sha256,a.artifact_size_bytes
             FROM feature_artifact_integration_artifacts ia
             JOIN feature_result_artifacts a ON a.artifact_id=ia.artifact_id
             WHERE ia.integration_id=?1 ORDER BY a.artifact_id",
        )?;
        let references = statement
            .query_map([integration_id.to_string()], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?
            .map(|row| {
                let (artifact_id, digest, size) = row?;
                Ok(ResultArtifactReference {
                    artifact_id: parse_uuid(&artifact_id)?,
                    artifact_sha256: digest_array(&digest)?,
                    artifact_size_bytes: i64_to_u64(size)?,
                })
            })
            .collect::<Result<Vec<_>, MasterError>>()?;
        Ok(references)
    }

    pub fn candidate_references(&self) -> Result<Vec<CandidateEvidence>, MasterError> {
        let mut statement = self.connection.prepare(
            "SELECT integration_id, artifact_set_sha256, candidate_commit, candidate_tree, base_commit
             FROM feature_artifact_integrations ORDER BY integration_id",
        )?;
        let references = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                ))
            })?
            .map(|row| {
                let (id, digest, commit, tree, base) = row?;
                Ok(CandidateEvidence {
                    integration_id: parse_uuid(&id)?,
                    artifact_set_sha256: digest_array(&digest)?,
                    candidate_commit: commit,
                    candidate_tree: tree,
                    base_commit: base,
                    artifact_ids: Vec::new(),
                })
            })
            .collect::<Result<Vec<_>, MasterError>>()?;
        let mut references = references;
        for reference in &mut references {
            let mut ids = self.connection.prepare(
                "SELECT a.artifact_id,a.artifact_sha256,a.artifact_size_bytes
                 FROM feature_artifact_integration_artifacts ia
                 JOIN feature_result_artifacts a ON a.artifact_id=ia.artifact_id
                 WHERE ia.integration_id=?1 ORDER BY a.artifact_id",
            )?;
            let artifact_references = ids
                .query_map([reference.integration_id.to_string()], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, Vec<u8>>(1)?,
                        row.get::<_, i64>(2)?,
                    ))
                })?
                .map(|row| {
                    let (id, digest, size) = row?;
                    Ok(ResultArtifactReference {
                        artifact_id: parse_uuid(&id)?,
                        artifact_sha256: digest_array(&digest)?,
                        artifact_size_bytes: i64_to_u64(size)?,
                    })
                })
                .collect::<Result<Vec<_>, MasterError>>()?;
            reference.artifact_ids = artifact_references
                .iter()
                .map(|artifact| artifact.artifact_id)
                .collect();
            if reference.artifact_ids.is_empty() {
                return Err(MasterError::InvalidStoredState(
                    "candidate artifact set is empty".to_string(),
                ));
            }
            if integration::artifact_reference_set_sha256(&artifact_references)
                != reference.artifact_set_sha256
            {
                return Err(MasterError::InvalidStoredState(
                    "candidate artifact set digest is invalid".to_string(),
                ));
            }
        }
        Ok(references)
    }

    pub fn prepare_artifact_integration(
        &self,
        request: &FeatureConveyorArtifactIntegrationRequest,
        now_ms: u64,
    ) -> Result<ArtifactIntegrationAuthorization, MasterError> {
        request.validate()?;
        let tx = self.connection.unchecked_transaction()?;
        if let Some(receipt) = load_integration_receipt_tx(&tx, request.integration_id)? {
            if integration_receipt_matches_request(&receipt, request)
                && integration_artifact_ids_tx(&tx, request.integration_id)? == request.artifact_ids
            {
                return Ok(ArtifactIntegrationAuthorization::Existing(receipt));
            }
            return Err(MasterError::ArtifactIntegrationUnavailable);
        }
        let previously_conflicted: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM feature_artifact_integration_conflicts
            WHERE integration_id=?1)",
            [request.integration_id.to_string()],
            |row| row.get(0),
        )?;
        if previously_conflicted {
            return Err(MasterError::ArtifactIntegrationUnavailable);
        }
        validate_integration_binding_tx(&tx, request, now_ms)?;
        let artifacts = load_complete_integration_artifacts_tx(&tx, request, true)?;
        Ok(ArtifactIntegrationAuthorization::Planned(
            ArtifactIntegrationPlan {
                request: request.clone(),
                artifacts,
            },
        ))
    }

    pub fn artifact_integration_plan(
        &self,
        feature_id: Uuid,
        now_ms: u64,
    ) -> Result<FeatureConveyorArtifactIntegrationPlan, MasterError> {
        let row = self.connection.query_row(
            "SELECT f.current_specification_revision,f.lifecycle_revision,l.lease_id,l.snapshot_id,
             c.snapshot_sha256,c.base_commit,c.registration_grant_revision,c.cloud_disclosure_grant_revision,
             c.publication_grant_revision,q.queue_revision,p.integer_value
             FROM feature_conveyor_features f JOIN feature_active_lease l ON l.feature_id=f.feature_id
             JOIN feature_repository_snapshot_claims c ON c.snapshot_id=l.snapshot_id
             JOIN feature_conveyor_state q ON q.singleton=1
             JOIN master_metadata p ON p.key='emergency_pause_revision'
             WHERE f.feature_id=?1 AND f.status='implementing'", [feature_id.to_string()], |r| Ok((
                r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,String>(2)?,r.get::<_,String>(3)?,
                r.get::<_,Vec<u8>>(4)?,r.get::<_,String>(5)?,r.get::<_,i64>(6)?,r.get::<_,i64>(7)?,
                r.get::<_,i64>(8)?,r.get::<_,i64>(9)?,r.get::<_,i64>(10)?)))
            .optional()?.ok_or(MasterError::ArtifactIntegrationUnavailable)?;
        let mut plan = FeatureConveyorArtifactIntegrationPlan {
            schema_version: 1,
            feature_id,
            specification_revision: i64_to_u64(row.0)?,
            lifecycle_revision: i64_to_u64(row.1)?,
            feature_lease_id: parse_uuid(&row.2)?,
            snapshot_id: parse_uuid(&row.3)?,
            snapshot_sha256: digest_array(&row.4)?,
            artifact_ids: vec![Uuid::from_u128(1)],
            queue_revision: i64_to_u64(row.9)?,
            emergency_pause_revision: i64_to_u64(row.10)?,
            grants: FeatureConveyorGrantRevisions {
                registration: i64_to_u64(row.6)?,
                cloud_disclosure: i64_to_u64(row.7)?,
                autonomous_publication: i64_to_u64(row.8)?,
            },
            base_commit: row.5,
        };
        let request = FeatureConveyorArtifactIntegrationRequest {
            schema_version: 1,
            integration_id: Uuid::from_u128(1),
            feature_id: plan.feature_id,
            specification_revision: plan.specification_revision,
            expected_lifecycle_revision: plan.lifecycle_revision,
            feature_lease_id: plan.feature_lease_id,
            snapshot_id: plan.snapshot_id,
            snapshot_sha256: plan.snapshot_sha256,
            artifact_ids: plan.artifact_ids.clone(),
            expected_queue_revision: plan.queue_revision,
            expected_emergency_pause_revision: plan.emergency_pause_revision,
            grants: plan.grants,
            base_commit: plan.base_commit.clone(),
        };
        let tx = self.connection.unchecked_transaction()?;
        validate_integration_binding_tx(&tx, &request, now_ms)?;
        let artifacts =
            load_complete_integration_artifacts_tx_without_requested_ids(&tx, &request)?;
        plan.artifact_ids = artifacts
            .iter()
            .map(|artifact| artifact.reference.artifact_id)
            .collect();
        plan.artifact_ids.sort();
        plan.validate()?;
        Ok(plan)
    }

    pub fn record_artifact_integration_conflict(
        &mut self,
        request: &FeatureConveyorArtifactIntegrationRequest,
        reason: &str,
        now_ms: u64,
    ) -> Result<(), MasterError> {
        if !matches!(
            reason,
            "content_cas_mismatch" | "overlapping_path" | "duplicate_ordinal"
        ) {
            return Err(MasterError::ArtifactIntegrationConflict);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_integration_binding_tx(&tx, request, now_ms)?;
        let binding_sha256: [u8; 32] = Sha256::digest(serde_json::to_vec(request)?).into();
        let existing: Option<(Vec<u8>,String)>=tx.query_row(
            "SELECT request_binding_sha256,reason_code FROM feature_artifact_integration_conflicts
             WHERE integration_id=?1",[request.integration_id.to_string()],|row| Ok((row.get(0)?,row.get(1)?)))
            .optional()?;
        if let Some((binding, existing_reason)) = existing {
            if digest_array(&binding)? == binding_sha256 && existing_reason == reason {
                return Ok(());
            }
            return Err(MasterError::ArtifactIntegrationUnavailable);
        }
        tx.execute(
            "INSERT OR IGNORE INTO feature_artifact_integration_conflicts
            (integration_id, feature_id, lifecycle_revision, request_binding_sha256, reason_code, recorded_at_ms)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                request.integration_id.to_string(),
                request.feature_id.to_string(),
                u64_to_i64(request.expected_lifecycle_revision)?,
                binding_sha256.as_slice(),
                reason,
                u64_to_i64(now_ms)?
            ],
        )?;
        append_feature_audit_tx(
            &tx,
            "artifact_integration_conflict",
            Some(request.feature_id),
            now_ms,
            serde_json::json!({"integration_id": request.integration_id, "reason_code": reason,
                "path_present": false, "content_present": false, "candidate_created": false,
                "lifecycle_advanced": false}),
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn finalize_artifact_integration(
        &mut self,
        plan: &ArtifactIntegrationPlan,
        candidate: &CandidateEvidence,
        now_ms: u64,
    ) -> Result<FeatureConveyorArtifactIntegrationReceipt, MasterError> {
        let request = &plan.request;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(receipt) = load_integration_receipt_tx(&tx, request.integration_id)? {
            return if integration_receipt_matches_request(&receipt, request)
                && integration_artifact_ids_tx(&tx, request.integration_id)? == request.artifact_ids
                && receipt.candidate_commit == candidate.candidate_commit
                && receipt.candidate_tree == candidate.candidate_tree
                && receipt.artifact_set_sha256 == candidate.artifact_set_sha256
            {
                Ok(receipt)
            } else {
                Err(MasterError::ArtifactIntegrationUnavailable)
            };
        }
        validate_integration_binding_tx(&tx, request, now_ms)?;
        let current = load_complete_integration_artifacts_tx(&tx, request, true)?;
        if current.iter().map(|a| a.reference).collect::<Vec<_>>()
            != plan
                .artifacts
                .iter()
                .map(|a| a.reference)
                .collect::<Vec<_>>()
        {
            return Err(MasterError::ArtifactIntegrationUnavailable);
        }
        tx.execute(
            "INSERT INTO feature_artifact_integrations
            (integration_id, feature_id, specification_revision, lifecycle_revision,
             feature_lease_id, snapshot_id, snapshot_sha256, artifact_set_sha256,
             candidate_commit, candidate_tree, base_commit, queue_revision,
             emergency_pause_revision, registration_grant_revision,
             cloud_disclosure_grant_revision, publication_grant_revision, integrated_at_ms)
            VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
            params![
                request.integration_id.to_string(),
                request.feature_id.to_string(),
                u64_to_i64(request.specification_revision)?,
                u64_to_i64(request.expected_lifecycle_revision)?,
                request.feature_lease_id.to_string(),
                request.snapshot_id.to_string(),
                request.snapshot_sha256.as_slice(),
                candidate.artifact_set_sha256.as_slice(),
                candidate.candidate_commit,
                candidate.candidate_tree,
                candidate.base_commit,
                u64_to_i64(request.expected_queue_revision)?,
                u64_to_i64(request.expected_emergency_pause_revision)?,
                u64_to_i64(request.grants.registration)?,
                u64_to_i64(request.grants.cloud_disclosure)?,
                u64_to_i64(request.grants.autonomous_publication)?,
                u64_to_i64(now_ms)?
            ],
        )?;
        for (position, artifact) in plan.artifacts.iter().enumerate() {
            tx.execute(
                "INSERT INTO feature_artifact_integration_artifacts
                 (integration_id, artifact_id, ordinal, packet_id, position)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    request.integration_id.to_string(),
                    artifact.reference.artifact_id.to_string(),
                    i64::from(artifact.packet.ordinal),
                    artifact.packet.packet_id.to_string(),
                    i64::try_from(position + 1)
                        .map_err(|_| MasterError::ArtifactIntegrationUnavailable)?
                ],
            )?;
        }
        let next_lifecycle = request
            .expected_lifecycle_revision
            .checked_add(1)
            .ok_or(MasterError::ArtifactIntegrationUnavailable)?;
        if tx.execute("UPDATE feature_conveyor_features SET status='validating', lifecycle_revision=?1,
            updated_at_ms=?2 WHERE feature_id=?3 AND status='implementing' AND lifecycle_revision=?4",
            params![u64_to_i64(next_lifecycle)?, u64_to_i64(now_ms)?, request.feature_id.to_string(),
                u64_to_i64(request.expected_lifecycle_revision)?])? != 1 {
            return Err(MasterError::ArtifactIntegrationUnavailable);
        }
        tx.execute("INSERT INTO feature_transition_evidence
            (feature_id,lifecycle_revision,from_status,to_status,accepted_evidence_sha256,recorded_at_ms)
            VALUES (?1,?2,'implementing','validating',?3,?4)", params![request.feature_id.to_string(),
                u64_to_i64(next_lifecycle)?, candidate.artifact_set_sha256.as_slice(), u64_to_i64(now_ms)?])?;
        append_feature_audit_tx(
            &tx,
            "artifact_candidate_frozen",
            Some(request.feature_id),
            now_ms,
            serde_json::json!({"integration_id": request.integration_id,
                "artifact_count": request.artifact_ids.len(), "artifact_set_digest_present": true,
                "candidate_commit_present": true, "candidate_tree_present": true,
                "base_commit_present": true, "from_status":"implementing", "to_status":"validating",
                "lifecycle_revision": next_lifecycle, "publication_authorized": false,
                "test_authority_granted": false, "review_authority_granted": false}),
        )?;
        let receipt = integration_receipt(request, candidate, next_lifecycle);
        tx.commit()?;
        Ok(receipt)
    }

    pub fn plan_validation_gate(
        &mut self,
        request: &FeatureConveyorValidationGateRequest,
        now_ms: u64,
    ) -> Result<ValidationGateAuthorization, MasterError> {
        self.authorize_validation_gate(request, now_ms, true)
    }

    /// Performs the complete durable-authority comparison without creating a
    /// new attempt. The process layer uses this before validating candidate,
    /// scratch, and toolchain resources.
    pub fn prepare_validation_gate(
        &mut self,
        request: &FeatureConveyorValidationGateRequest,
        now_ms: u64,
    ) -> Result<ValidationGateAuthorization, MasterError> {
        self.authorize_validation_gate(request, now_ms, false)
    }

    fn authorize_validation_gate(
        &mut self,
        request: &FeatureConveyorValidationGateRequest,
        now_ms: u64,
        persist_new_attempt: bool,
    ) -> Result<ValidationGateAuthorization, MasterError> {
        request.validate()?;
        let request_binding_sha256 = feature_conveyor_validation_request_binding_sha256(request)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(stored_binding) = tx
            .query_row(
                "SELECT request_binding_sha256 FROM feature_validation_attempts
                 WHERE validation_id=?1",
                [request.validation_id.to_string()],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()?
        {
            if digest_array(&stored_binding)? != request_binding_sha256 {
                return Err(MasterError::ValidationGateUnavailable);
            }
            let completion = load_validation_completion_tx(&tx, request.validation_id)?;
            return match completion {
                Some((true, evidence_manifest_sha256, lifecycle_revision)) => {
                    let candidate = validate_completed_validation_gate_binding_tx(
                        &tx,
                        request,
                        lifecycle_revision,
                        now_ms,
                    )?;
                    Ok(ValidationGateAuthorization::ExistingPassed {
                        receipt: validation_gate_receipt(
                            request,
                            lifecycle_revision,
                            evidence_manifest_sha256,
                        ),
                        candidate,
                    })
                }
                Some((false, _, _)) => {
                    validate_validation_gate_binding_tx(&tx, request, now_ms)?;
                    Ok(ValidationGateAuthorization::ExistingFailed)
                }
                None => {
                    let candidate = validate_validation_gate_binding_tx(&tx, request, now_ms)?;
                    let (approved_paths, acceptance_criteria_count) =
                        validation_work_packet_scope_tx(&tx, request.integration_id)?;
                    let requirements_sha256 = validation_requirements_sha256_tx(
                        &tx,
                        request.feature_id,
                        request.specification_revision,
                    )?;
                    Ok(ValidationGateAuthorization::Planned(
                        ValidationGateExecutionPlan {
                            request: request.clone(),
                            candidate,
                            approved_paths,
                            acceptance_criteria_count,
                            requirements_sha256,
                        },
                    ))
                }
            };
        }
        let candidate = validate_validation_gate_binding_tx(&tx, request, now_ms)?;
        let (approved_paths, acceptance_criteria_count) =
            validation_work_packet_scope_tx(&tx, request.integration_id)?;
        let requirements_sha256 = validation_requirements_sha256_tx(
            &tx,
            request.feature_id,
            request.specification_revision,
        )?;
        if !persist_new_attempt {
            return Ok(ValidationGateAuthorization::Planned(
                ValidationGateExecutionPlan {
                    request: request.clone(),
                    candidate,
                    approved_paths,
                    acceptance_criteria_count,
                    requirements_sha256,
                },
            ));
        }
        tx.execute(
            "INSERT INTO feature_validation_attempts (
               validation_id,feature_id,specification_revision,lifecycle_revision,
               feature_lease_id,snapshot_id,snapshot_sha256,integration_id,
               artifact_set_sha256,candidate_commit,candidate_tree,base_commit,
               plan_sha256,command_ids_json,queue_revision,emergency_pause_revision,
               registration_grant_revision,cloud_disclosure_grant_revision,
               publication_grant_revision,request_binding_sha256,started_at_ms
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,
                       ?16,?17,?18,?19,?20,?21)",
            params![
                request.validation_id.to_string(),
                request.feature_id.to_string(),
                u64_to_i64(request.specification_revision)?,
                u64_to_i64(request.expected_lifecycle_revision)?,
                request.feature_lease_id.to_string(),
                request.snapshot_id.to_string(),
                request.snapshot_sha256.as_slice(),
                request.integration_id.to_string(),
                request.artifact_set_sha256.as_slice(),
                request.candidate_commit,
                request.candidate_tree,
                request.base_commit,
                request.plan_sha256.as_slice(),
                serde_json::to_string(&request.command_ids)?,
                u64_to_i64(request.expected_queue_revision)?,
                u64_to_i64(request.expected_emergency_pause_revision)?,
                u64_to_i64(request.grants.registration)?,
                u64_to_i64(request.grants.cloud_disclosure)?,
                u64_to_i64(request.grants.autonomous_publication)?,
                request_binding_sha256.as_slice(),
                u64_to_i64(now_ms)?,
            ],
        )?;
        append_feature_audit_tx(
            &tx,
            "feature_validation_started",
            Some(request.feature_id),
            now_ms,
            serde_json::json!({
                "validation_id": request.validation_id,
                "command_count": request.command_ids.len(),
                "plan_digest_present": true,
                "candidate_commit_present": true,
                "raw_output_present": false,
                "path_present": false,
                "review_authorized": false,
                "publication_authorized": false
            }),
        )?;
        tx.commit()?;
        Ok(ValidationGateAuthorization::Planned(
            ValidationGateExecutionPlan {
                request: request.clone(),
                candidate,
                approved_paths,
                acceptance_criteria_count,
                requirements_sha256,
            },
        ))
    }

    pub fn finalize_validation_gate(
        &mut self,
        plan: &ValidationGateExecutionPlan,
        evidence: &ValidationGateEvidence,
        now_ms: u64,
    ) -> Result<FeatureConveyorValidationGateReceipt, MasterError> {
        validate_validation_gate_evidence(&plan.request, evidence)?;
        let evidence_manifest_sha256: [u8; 32] =
            Sha256::digest(serde_json::to_vec(evidence)?).into();
        let passed = evidence.commands.iter().all(|command| command.passed);
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some((stored_passed, stored_digest, lifecycle_revision)) =
            load_validation_completion_tx(&tx, plan.request.validation_id)?
        {
            if stored_passed != passed || stored_digest != evidence_manifest_sha256 {
                return Err(MasterError::ValidationGateUnavailable);
            }
            return if stored_passed {
                Ok(validation_gate_receipt(
                    &plan.request,
                    lifecycle_revision,
                    stored_digest,
                ))
            } else {
                Err(MasterError::ValidationGateFailed)
            };
        }
        let stored_binding: Vec<u8> = tx
            .query_row(
                "SELECT request_binding_sha256 FROM feature_validation_attempts
                 WHERE validation_id=?1",
                [plan.request.validation_id.to_string()],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(MasterError::ValidationGateUnavailable)?;
        let current_binding = feature_conveyor_validation_request_binding_sha256(&plan.request)?;
        if digest_array(&stored_binding)? != current_binding
            || validate_validation_gate_binding_tx(&tx, &plan.request, now_ms)? != plan.candidate
        {
            return Err(MasterError::ValidationGateUnavailable);
        }
        for command in &evidence.commands {
            tx.execute(
                "INSERT INTO feature_validation_command_evidence (
                   validation_id,command_id,passed,result_sha256,duration_ms,output_truncated
                 ) VALUES (?1,?2,?3,?4,?5,?6)",
                params![
                    plan.request.validation_id.to_string(),
                    serde_json::to_string(&command.command_id)?,
                    i64::from(command.passed),
                    command.result_sha256.as_slice(),
                    u64_to_i64(command.duration_ms)?,
                    i64::from(command.output_truncated),
                ],
            )?;
        }
        let lifecycle_revision = if passed {
            plan.request
                .expected_lifecycle_revision
                .checked_add(1)
                .ok_or(MasterError::ValidationGateUnavailable)?
        } else {
            plan.request.expected_lifecycle_revision
        };
        tx.execute(
            "INSERT INTO feature_validation_completions (
               validation_id,passed,evidence_manifest_sha256,lifecycle_revision,completed_at_ms
             ) VALUES (?1,?2,?3,?4,?5)",
            params![
                plan.request.validation_id.to_string(),
                i64::from(passed),
                evidence_manifest_sha256.as_slice(),
                u64_to_i64(lifecycle_revision)?,
                u64_to_i64(now_ms)?,
            ],
        )?;
        if passed {
            if tx.execute(
                "UPDATE feature_conveyor_features SET status='reviewing',
                   lifecycle_revision=?1,updated_at_ms=?2
                 WHERE feature_id=?3 AND status='validating' AND lifecycle_revision=?4",
                params![
                    u64_to_i64(lifecycle_revision)?,
                    u64_to_i64(now_ms)?,
                    plan.request.feature_id.to_string(),
                    u64_to_i64(plan.request.expected_lifecycle_revision)?,
                ],
            )? != 1
            {
                return Err(MasterError::ValidationGateUnavailable);
            }
            tx.execute(
                "INSERT INTO feature_transition_evidence (
                   feature_id,lifecycle_revision,from_status,to_status,
                   repository_snapshot_sha256,accepted_evidence_sha256,recorded_at_ms
                 ) VALUES (?1,?2,'validating','reviewing',?3,?4,?5)",
                params![
                    plan.request.feature_id.to_string(),
                    u64_to_i64(lifecycle_revision)?,
                    plan.request.snapshot_sha256.as_slice(),
                    evidence_manifest_sha256.as_slice(),
                    u64_to_i64(now_ms)?,
                ],
            )?;
        }
        append_feature_audit_tx(
            &tx,
            if passed {
                "feature_validation_passed"
            } else {
                "feature_validation_failed"
            },
            Some(plan.request.feature_id),
            now_ms,
            serde_json::json!({
                "validation_id": plan.request.validation_id,
                "command_count": evidence.commands.len(),
                "evidence_manifest_digest_present": true,
                "all_required_evidence_present": true,
                "passed": passed,
                "raw_output_present": false,
                "path_present": false,
                "from_status": "validating",
                "to_status": if passed { "reviewing" } else { "validating" },
                "lifecycle_advanced": passed,
                "publication_authorized": false
            }),
        )?;
        tx.commit()?;
        if passed {
            Ok(validation_gate_receipt(
                &plan.request,
                lifecycle_revision,
                evidence_manifest_sha256,
            ))
        } else {
            Err(MasterError::ValidationGateFailed)
        }
    }

    /// Read-only authority and budget check. The process layer calls this only
    /// after a provider has advertised the fixed response-only envelope; it
    /// deliberately creates no call intent.
    pub fn prepare_review_gateway(
        &mut self,
        request: &FeatureConveyorReviewGatewayRequest,
        now_ms: u64,
    ) -> Result<ReviewGatewayAuthorization, MasterError> {
        self.authorize_review_gateway(request, None, now_ms)
    }

    /// Rechecks the exact locally assembled packet and durably opens one
    /// provider-call intent immediately before crossing the provider boundary.
    pub fn begin_review_gateway(
        &mut self,
        request: &FeatureConveyorReviewGatewayRequest,
        packet: &FeatureConveyorReviewPacket,
        now_ms: u64,
    ) -> Result<ReviewGatewayAuthorization, MasterError> {
        self.authorize_review_gateway(request, Some(packet), now_ms)
    }

    fn authorize_review_gateway(
        &mut self,
        request: &FeatureConveyorReviewGatewayRequest,
        packet: Option<&FeatureConveyorReviewPacket>,
        now_ms: u64,
    ) -> Result<ReviewGatewayAuthorization, MasterError> {
        request.validate()?;
        let request_binding = feature_conveyor_review_request_binding_sha256(request)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(stored_binding) = tx
            .query_row(
                "SELECT request_binding_sha256 FROM feature_review_calls WHERE review_call_id=?1",
                [request.review_call_id.to_string()],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()?
        {
            if digest_array(&stored_binding)? != request_binding {
                return Err(MasterError::ReviewGatewayUnavailable);
            }
            let outcome = tx
                .query_row(
                    "SELECT outcome_kind,next_retry_at_ms FROM feature_review_call_outcomes
                     WHERE review_call_id=?1",
                    [request.review_call_id.to_string()],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<i64>>(1)?)),
                )
                .optional()?;
            return match outcome {
                Some((kind, _)) if kind == "decision" => {
                    Ok(ReviewGatewayAuthorization::ExistingDecision(Box::new(
                        load_review_gateway_receipt_tx(&tx, request)?,
                    )))
                }
                Some((_, Some(next_retry))) => {
                    Ok(ReviewGatewayAuthorization::ExistingTransportFailure {
                        next_retry_at_ms: i64_to_u64(next_retry)?,
                    })
                }
                _ => Err(MasterError::ReviewGatewayUnavailable),
            };
        }
        let plan = review_gateway_plan_tx(&tx, request, now_ms)?;
        if packet.is_none() {
            return Ok(ReviewGatewayAuthorization::Planned(Box::new(plan)));
        }
        let packet = packet.ok_or(MasterError::ReviewGatewayUnavailable)?;
        validate_review_packet_plan(&plan, packet)?;
        tx.execute(
            "INSERT INTO feature_review_calls (
               review_call_id,feature_id,specification_revision,lifecycle_revision,
               feature_lease_id,integration_id,validation_id,candidate_commit,candidate_tree,
               base_commit,candidate_diff_sha256,evidence_manifest_sha256,review_packet_sha256,
               provider_id,model_id,candidate_attempt,feature_call,queue_revision,
               emergency_pause_revision,registration_grant_revision,
               cloud_disclosure_grant_revision,publication_grant_revision,
               request_binding_sha256,started_at_ms
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,
                       ?17,?18,?19,?20,?21,?22,?23,?24)",
            params![
                request.review_call_id.to_string(),
                request.feature_id.to_string(),
                u64_to_i64(request.specification_revision)?,
                u64_to_i64(request.expected_lifecycle_revision)?,
                request.feature_lease_id.to_string(),
                request.integration_id.to_string(),
                request.validation_id.to_string(),
                request.candidate_commit,
                request.candidate_tree,
                request.base_commit,
                request.candidate_diff_sha256.as_slice(),
                request.evidence_manifest_sha256.as_slice(),
                request.review_packet_sha256.as_slice(),
                request.provider_id,
                request.model_id,
                i64::from(plan.candidate_attempt),
                i64::from(plan.feature_call),
                u64_to_i64(request.expected_queue_revision)?,
                u64_to_i64(request.expected_emergency_pause_revision)?,
                u64_to_i64(request.grants.registration)?,
                u64_to_i64(request.grants.cloud_disclosure)?,
                u64_to_i64(request.grants.autonomous_publication)?,
                request_binding.as_slice(),
                u64_to_i64(now_ms)?,
            ],
        )?;
        append_feature_audit_tx(
            &tx,
            "feature_review_call_started",
            Some(request.feature_id),
            now_ms,
            serde_json::json!({
                "review_call_id": request.review_call_id,
                "candidate_attempt": plan.candidate_attempt,
                "feature_call": plan.feature_call,
                "response_only": true,
                "fresh_session": true,
                "review_packet_digest_present": true,
                "raw_packet_present": false,
                "transcript_present": false,
                "memory_present": false,
                "publication_authorized": false
            }),
        )?;
        tx.commit()?;
        Ok(ReviewGatewayAuthorization::Planned(Box::new(plan)))
    }

    pub fn finalize_review_transport_failure(
        &mut self,
        plan: &ReviewGatewayExecutionPlan,
        failure: ReviewTransportFailure,
        now_ms: u64,
    ) -> Result<u64, MasterError> {
        let backoff = FEATURE_CONVEYOR_REVIEW_BACKOFF_MS
            .get(usize::from(plan.candidate_attempt.saturating_sub(1)))
            .copied()
            .ok_or(MasterError::ReviewGatewayUnavailable)?;
        let next_retry_at_ms = now_ms
            .checked_add(backoff)
            .ok_or(MasterError::ReviewGatewayUnavailable)?;
        let mut digest = Sha256::new();
        digest.update(b"assemblywright.review-transport-failure.v1\0");
        digest.update(failure.as_str().as_bytes());
        digest.update(plan.request.review_call_id.as_bytes());
        let outcome_sha256: [u8; 32] = digest.finalize().into();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_started_review_call_tx(&tx, plan)?;
        tx.execute(
            "INSERT INTO feature_review_call_outcomes (
               review_call_id,outcome_kind,outcome_sha256,next_retry_at_ms,completed_at_ms
             ) VALUES (?1,?2,?3,?4,?5)",
            params![
                plan.request.review_call_id.to_string(),
                failure.as_str(),
                outcome_sha256.as_slice(),
                u64_to_i64(next_retry_at_ms)?,
                u64_to_i64(now_ms)?,
            ],
        )?;
        append_feature_audit_tx(
            &tx,
            "feature_review_transport_failed",
            Some(plan.request.feature_id),
            now_ms,
            serde_json::json!({
                "review_call_id": plan.request.review_call_id,
                "failure_code": failure.as_str(),
                "candidate_attempt": plan.candidate_attempt,
                "feature_call": plan.feature_call,
                "backoff_ms": backoff,
                "repair_consumed": false,
                "decision_recorded": false,
                "raw_error_present": false
            }),
        )?;
        tx.commit()?;
        Ok(next_retry_at_ms)
    }

    pub fn finalize_interrupted_review_call(
        &mut self,
        plan: &ReviewGatewayExecutionPlan,
        now_ms: u64,
    ) -> Result<(), MasterError> {
        let mut digest = Sha256::new();
        digest.update(b"assemblywright.review-interrupted.v1\0");
        digest.update(plan.request.review_call_id.as_bytes());
        let outcome_sha256: [u8; 32] = digest.finalize().into();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_started_review_call_tx(&tx, plan)?;
        tx.execute(
            "INSERT INTO feature_review_call_outcomes (
               review_call_id,outcome_kind,outcome_sha256,next_retry_at_ms,completed_at_ms
             ) VALUES (?1,'interrupted',?2,NULL,?3)",
            params![
                plan.request.review_call_id.to_string(),
                outcome_sha256.as_slice(),
                u64_to_i64(now_ms)?,
            ],
        )?;
        let next_lifecycle_revision = plan
            .request
            .expected_lifecycle_revision
            .checked_add(1)
            .ok_or(MasterError::ReviewGatewayUnavailable)?;
        let quarantined = tx.execute(
            "UPDATE feature_conveyor_features SET status='quarantined',
               lifecycle_revision=?1,effect_possible=1,updated_at_ms=?2
             WHERE feature_id=?3 AND status='reviewing' AND lifecycle_revision=?4",
            params![
                u64_to_i64(next_lifecycle_revision)?,
                u64_to_i64(now_ms)?,
                plan.request.feature_id.to_string(),
                u64_to_i64(plan.request.expected_lifecycle_revision)?,
            ],
        )? == 1;
        if quarantined {
            tx.execute(
                "INSERT INTO feature_transition_evidence (
                   feature_id,lifecycle_revision,from_status,to_status,recorded_at_ms
                 ) VALUES (?1,?2,'reviewing','quarantined',?3)",
                params![
                    plan.request.feature_id.to_string(),
                    u64_to_i64(next_lifecycle_revision)?,
                    u64_to_i64(now_ms)?,
                ],
            )?;
        }
        append_feature_audit_tx(
            &tx,
            "feature_review_interrupted",
            Some(plan.request.feature_id),
            now_ms,
            serde_json::json!({
                "review_call_id": plan.request.review_call_id,
                "candidate_attempt": plan.candidate_attempt,
                "feature_call": plan.feature_call,
                "outcome_recorded": true,
                "quarantined": quarantined,
                "effect_possible": true,
                "automatic_retry_authorized": false,
                "raw_error_present": false
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn finalize_review_decision(
        &mut self,
        plan: &ReviewGatewayExecutionPlan,
        packet: &FeatureConveyorReviewPacket,
        output: &FeatureConveyorReviewProviderOutput,
        now_ms: u64,
    ) -> Result<FeatureConveyorReviewGatewayReceipt, MasterError> {
        validate_review_packet_plan(plan, packet)?;
        output.validate()?;
        if output.review_packet_sha256 != plan.request.review_packet_sha256
            || output.provider_id != plan.request.provider_id
            || output.model_id != plan.request.model_id
            || output.evidence_digests != plan.evidence_digests
            || output
                .requirement_coverage
                .iter()
                .map(|coverage| coverage.requirement_id.as_str())
                .ne(plan.requirement_ids.iter().map(String::as_str))
            || output
                .blocking_findings
                .iter()
                .chain(&output.non_blocking_findings)
                .any(|finding| {
                    !plan.requirement_ids.contains(&finding.requirement_id)
                        || !plan.evidence_digests.contains(&finding.evidence_sha256)
                })
            || output
                .requirement_coverage
                .iter()
                .any(|coverage| !plan.evidence_digests.contains(&coverage.evidence_sha256))
            || !plan
                .evidence_digests
                .contains(&output.knowledge_base_evidence_sha256)
        {
            return Err(MasterError::ReviewGatewayUnavailable);
        }
        let structured_value = serde_json::to_value(output)?;
        let structured_result_json = canonical_json(&structured_value)?;
        let decision_sha256: [u8; 32] = Sha256::digest(structured_result_json.as_bytes()).into();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        validate_started_review_call_tx(&tx, plan)?;
        let current = review_gateway_plan_tx(&tx, &plan.request, now_ms)?;
        if current.request != plan.request
            || current.candidate != plan.candidate
            || current.approved_specification_sha256 != plan.approved_specification_sha256
            || current.requirements_sha256 != plan.requirements_sha256
            || current.evidence_digests != plan.evidence_digests
        {
            return Err(MasterError::ReviewGatewayUnavailable);
        }
        let approved = output.decision == FeatureConveyorReviewDecision::Approved;
        let lifecycle_revision = if approved {
            plan.request
                .expected_lifecycle_revision
                .checked_add(1)
                .ok_or(MasterError::ReviewGatewayUnavailable)?
        } else {
            plan.request.expected_lifecycle_revision
        };
        tx.execute(
            "INSERT INTO feature_review_call_outcomes (
               review_call_id,outcome_kind,outcome_sha256,next_retry_at_ms,completed_at_ms
             ) VALUES (?1,'decision',?2,NULL,?3)",
            params![
                plan.request.review_call_id.to_string(),
                decision_sha256.as_slice(),
                u64_to_i64(now_ms)?,
            ],
        )?;
        tx.execute(
            "INSERT INTO feature_review_decisions (
               review_call_id,feature_id,candidate_commit,decision,decision_sha256,
               structured_result_json,lifecycle_revision,decided_at_ms
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                plan.request.review_call_id.to_string(),
                plan.request.feature_id.to_string(),
                plan.request.candidate_commit,
                if approved { "approved" } else { "rejected" },
                decision_sha256.as_slice(),
                structured_result_json,
                u64_to_i64(lifecycle_revision)?,
                u64_to_i64(now_ms)?,
            ],
        )?;
        if approved {
            if tx.execute(
                "UPDATE feature_conveyor_features SET status='publishing',
                   lifecycle_revision=?1,updated_at_ms=?2
                 WHERE feature_id=?3 AND status='reviewing' AND lifecycle_revision=?4",
                params![
                    u64_to_i64(lifecycle_revision)?,
                    u64_to_i64(now_ms)?,
                    plan.request.feature_id.to_string(),
                    u64_to_i64(plan.request.expected_lifecycle_revision)?,
                ],
            )? != 1
            {
                return Err(MasterError::ReviewGatewayUnavailable);
            }
            tx.execute(
                "INSERT INTO feature_transition_evidence (
                   feature_id,lifecycle_revision,from_status,to_status,
                   repository_snapshot_sha256,accepted_evidence_sha256,recorded_at_ms
                 ) VALUES (?1,?2,'reviewing','publishing',?3,?4,?5)",
                params![
                    plan.request.feature_id.to_string(),
                    u64_to_i64(lifecycle_revision)?,
                    plan.request.candidate_diff_sha256.as_slice(),
                    decision_sha256.as_slice(),
                    u64_to_i64(now_ms)?,
                ],
            )?;
        }
        append_feature_audit_tx(
            &tx,
            if approved {
                "feature_review_approved"
            } else {
                "feature_review_rejected"
            },
            Some(plan.request.feature_id),
            now_ms,
            serde_json::json!({
                "review_call_id": plan.request.review_call_id,
                "candidate_attempt": plan.candidate_attempt,
                "feature_call": plan.feature_call,
                "decision_digest_present": true,
                "blocking_finding_count": output.blocking_findings.len(),
                "non_blocking_finding_count": output.non_blocking_findings.len(),
                "requirement_coverage_count": output.requirement_coverage.len(),
                "knowledge_base_determination_present": true,
                "from_status": "reviewing",
                "to_status": if approved { "publishing" } else { "reviewing" },
                "lifecycle_advanced": approved,
                "repair_consumed": false,
                "raw_provider_output_present": false,
                "transcript_present": false,
                "memory_present": false
            }),
        )?;
        let receipt = review_gateway_receipt(plan, lifecycle_revision, decision_sha256, approved);
        tx.commit()?;
        Ok(receipt)
    }

    pub fn prepare_publication(
        &mut self,
        request: &FeatureConveyorPublicationRequest,
        now_ms: u64,
    ) -> Result<PublicationAuthorization, MasterError> {
        request.validate()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let binding = feature_conveyor_publication_request_binding_sha256(request)?;
        if let Some(stored) = tx
            .query_row(
                "SELECT request_binding_sha256 FROM feature_publications WHERE publication_id=?1",
                [request.publication_id.to_string()],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()?
        {
            if digest_array(&stored)? != binding {
                return Err(MasterError::PublicationCoordinatorUnavailable);
            }
            let receipt = load_publication_receipt_tx(&tx, request)?;
            return match receipt {
                Some(receipt) => Ok(PublicationAuthorization::Existing(Box::new(receipt))),
                None => Err(MasterError::PublicationEffectAmbiguous),
            };
        }
        Ok(PublicationAuthorization::Planned(Box::new(
            publication_plan_tx(&tx, request, now_ms)?,
        )))
    }

    pub fn begin_publication(
        &mut self,
        plan: &PublicationExecutionPlan,
        now_ms: u64,
    ) -> Result<(), MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if publication_plan_tx(&tx, &plan.request, now_ms)? != *plan {
            return Err(MasterError::PublicationCoordinatorUnavailable);
        }
        tx.execute(
            "INSERT INTO feature_publications (
               publication_id,feature_id,specification_revision,lifecycle_revision,
               feature_lease_id,integration_id,validation_id,review_call_id,
               candidate_commit,candidate_tree,candidate_diff_sha256,
               evidence_manifest_sha256,review_decision_sha256,provider_id,model_id,
               repository_id,feature_branch,base_branch,remote_base_commit,
               branch_policy_sha256,required_checks_json,merge_strategy,post_merge_gate,
               queue_revision,emergency_pause_revision,registration_grant_revision,
               cloud_disclosure_grant_revision,publication_grant_revision,
               request_binding_sha256,started_at_ms
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,
                       ?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29,?30)",
            params![
                plan.request.publication_id.to_string(),
                plan.request.feature_id.to_string(),
                u64_to_i64(plan.request.specification_revision)?,
                u64_to_i64(plan.request.expected_lifecycle_revision)?,
                plan.request.feature_lease_id.to_string(),
                plan.request.integration_id.to_string(),
                plan.request.validation_id.to_string(),
                plan.request.review_call_id.to_string(),
                plan.request.candidate_commit,
                plan.request.candidate_tree,
                plan.request.candidate_diff_sha256.as_slice(),
                plan.request.evidence_manifest_sha256.as_slice(),
                plan.request.review_decision_sha256.as_slice(),
                plan.request.provider_id,
                plan.request.model_id,
                plan.repository_id.to_string(),
                plan.feature_branch,
                plan.base_branch,
                plan.request.remote_base_commit,
                plan.request.branch_policy_sha256.as_slice(),
                serde_json::to_string(&plan.required_checks)?,
                plan.merge_strategy,
                plan.post_merge_gate,
                u64_to_i64(plan.request.expected_queue_revision)?,
                u64_to_i64(plan.request.expected_emergency_pause_revision)?,
                u64_to_i64(plan.request.grants.registration)?,
                u64_to_i64(plan.request.grants.cloud_disclosure)?,
                u64_to_i64(plan.request.grants.autonomous_publication)?,
                feature_conveyor_publication_request_binding_sha256(&plan.request)?.as_slice(),
                u64_to_i64(now_ms)?,
            ],
        )?;
        insert_publication_action_intent_tx(
            &tx,
            plan.request.publication_id,
            PublicationActionKind::PushBranch,
            1,
            now_ms,
        )?;
        append_feature_audit_tx(
            &tx,
            "feature_publication_started",
            Some(plan.request.feature_id),
            now_ms,
            serde_json::json!({
                "publication_id": plan.request.publication_id,
                "candidate_digest_present": true,
                "branch_policy_digest_present": true,
                "remote_base_digest_present": true,
                "action": "push_branch",
                "intent_durable": true,
                "branch_protection_bypass_authorized": false,
                "credential_present": false,
                "path_present": false,
                "command_present": false
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn publication_execution_is_current(
        &self,
        request: &FeatureConveyorPublicationRequest,
        action: PublicationActionKind,
        now_ms: u64,
    ) -> Result<bool, MasterError> {
        if request.validate().is_err() {
            return Ok(false);
        }
        let tx = self.connection.unchecked_transaction()?;
        if publication_plan_tx(&tx, request, now_ms).is_err() {
            return Ok(false);
        }
        let expected = next_publication_action_tx(&tx, request.publication_id)?;
        Ok(expected == Some(action))
    }

    pub fn complete_publication_action(
        &mut self,
        plan: &PublicationExecutionPlan,
        evidence: &PublicationActionEvidence,
        now_ms: u64,
    ) -> Result<Option<FeatureConveyorPublicationReceipt>, MasterError> {
        validate_publication_action_evidence(plan, evidence)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if publication_plan_tx(&tx, &plan.request, now_ms)? != *plan
            || next_publication_action_tx(&tx, plan.request.publication_id)?
                != Some(evidence.action)
        {
            return Err(MasterError::PublicationCoordinatorUnavailable);
        }
        validate_publication_action_evidence_tx(&tx, plan, evidence)?;
        let ordinal = PublicationActionKind::ORDERED
            .iter()
            .position(|action| *action == evidence.action)
            .and_then(|index| u64::try_from(index + 1).ok())
            .ok_or(MasterError::PublicationCoordinatorUnavailable)?;
        tx.execute(
            "INSERT INTO feature_publication_action_outcomes (
               publication_id,ordinal,action_kind,evidence_sha256,pull_request_number,
               observed_commit,merge_commit,passed,structured_evidence_json,completed_at_ms
             ) VALUES (?1,?2,?3,?4,?5,?6,?7,1,?8,?9)",
            params![
                plan.request.publication_id.to_string(),
                u64_to_i64(ordinal)?,
                evidence.action.as_str(),
                evidence.evidence_sha256.as_slice(),
                evidence.pull_request_number.map(u64_to_i64).transpose()?,
                evidence.observed_head_commit,
                evidence.resulting_main_commit,
                serde_json::to_string(evidence)?,
                u64_to_i64(now_ms)?,
            ],
        )?;
        let next =
            PublicationActionKind::ORDERED.get(usize::try_from(ordinal).unwrap_or(usize::MAX));
        if evidence.action == PublicationActionKind::MergePullRequest {
            let next_revision = plan
                .request
                .expected_lifecycle_revision
                .checked_add(1)
                .ok_or(MasterError::PublicationCoordinatorUnavailable)?;
            if tx.execute(
                "UPDATE feature_conveyor_features SET status='verifying_main',
                   lifecycle_revision=?1,updated_at_ms=?2
                 WHERE feature_id=?3 AND status='publishing' AND lifecycle_revision=?4",
                params![
                    u64_to_i64(next_revision)?,
                    u64_to_i64(now_ms)?,
                    plan.request.feature_id.to_string(),
                    u64_to_i64(plan.request.expected_lifecycle_revision)?,
                ],
            )? != 1
            {
                return Err(MasterError::PublicationCoordinatorUnavailable);
            }
            tx.execute(
                "INSERT INTO feature_transition_evidence (
                   feature_id,lifecycle_revision,from_status,to_status,
                   accepted_evidence_sha256,recorded_at_ms
                 ) VALUES (?1,?2,'publishing','verifying_main',?3,?4)",
                params![
                    plan.request.feature_id.to_string(),
                    u64_to_i64(next_revision)?,
                    evidence.evidence_sha256.as_slice(),
                    u64_to_i64(now_ms)?,
                ],
            )?;
        }
        if let Some(next) = next {
            insert_publication_action_intent_tx(
                &tx,
                plan.request.publication_id,
                *next,
                ordinal + 1,
                now_ms,
            )?;
            append_feature_audit_tx(
                &tx,
                "feature_publication_action_completed",
                Some(plan.request.feature_id),
                now_ms,
                serde_json::json!({
                    "publication_id": plan.request.publication_id,
                    "action": evidence.action.as_str(),
                    "action_evidence_digest_present": true,
                    "next_action": next.as_str(),
                    "next_intent_durable": true,
                    "external_output_present": false,
                    "branch_protection_bypass_authorized": false
                }),
            )?;
            tx.commit()?;
            return Ok(None);
        }

        let merge_commit = publication_merge_commit_tx(&tx, plan.request.publication_id)?;
        let final_revision = plan
            .request
            .expected_lifecycle_revision
            .checked_add(2)
            .ok_or(MasterError::PublicationCoordinatorUnavailable)?;
        if tx.execute(
            "UPDATE feature_conveyor_features SET status='succeeded',
               lifecycle_revision=?1,effect_possible=0,updated_at_ms=?2
             WHERE feature_id=?3 AND status='verifying_main' AND lifecycle_revision=?4",
            params![
                u64_to_i64(final_revision)?,
                u64_to_i64(now_ms)?,
                plan.request.feature_id.to_string(),
                u64_to_i64(final_revision - 1)?,
            ],
        )? != 1
        {
            return Err(MasterError::PublicationCoordinatorUnavailable);
        }
        tx.execute(
            "INSERT INTO feature_transition_evidence (
               feature_id,lifecycle_revision,from_status,to_status,
               verified_main_commit_sha256,post_merge_evidence_sha256,recorded_at_ms
             ) VALUES (?1,?2,'verifying_main','succeeded',?3,?4,?5)",
            params![
                plan.request.feature_id.to_string(),
                u64_to_i64(final_revision)?,
                Sha256::digest(merge_commit.as_bytes()).as_slice(),
                evidence.evidence_sha256.as_slice(),
                u64_to_i64(now_ms)?,
            ],
        )?;
        tx.execute(
            "DELETE FROM feature_conveyor_queue WHERE feature_id=?1",
            [plan.request.feature_id.to_string()],
        )?;
        tx.execute(
            "DELETE FROM feature_active_lease WHERE singleton=1 AND feature_id=?1",
            [plan.request.feature_id.to_string()],
        )?;
        let queue_revision =
            increment_queue_revision_tx(&tx, plan.request.expected_queue_revision)?;
        tx.execute(
            "INSERT INTO feature_publication_completions (
               publication_id,merge_commit,remote_main_commit,post_merge_evidence_sha256,
               lifecycle_revision,queue_revision,completed_at_ms
             ) VALUES (?1,?2,?2,?3,?4,?5,?6)",
            params![
                plan.request.publication_id.to_string(),
                merge_commit,
                evidence.evidence_sha256.as_slice(),
                u64_to_i64(final_revision)?,
                u64_to_i64(queue_revision)?,
                u64_to_i64(now_ms)?,
            ],
        )?;
        append_feature_audit_tx(
            &tx,
            "feature_publication_succeeded",
            Some(plan.request.feature_id),
            now_ms,
            serde_json::json!({
                "publication_id": plan.request.publication_id,
                "from_status": "verifying_main",
                "to_status": "succeeded",
                "merge_commit_digest_present": true,
                "remote_main_exact": true,
                "post_merge_evidence_digest_present": true,
                "lease_released": true,
                "queue_revision": queue_revision,
                "branch_protection_bypass_authorized": false,
                "credential_present": false,
                "external_output_present": false
            }),
        )?;
        let receipt = publication_receipt(
            plan,
            final_revision,
            queue_revision,
            &merge_commit,
            evidence.evidence_sha256,
        );
        tx.commit()?;
        Ok(Some(receipt))
    }

    pub fn quarantine_ambiguous_publication(
        &mut self,
        plan: &PublicationExecutionPlan,
        action: PublicationActionKind,
        now_ms: u64,
    ) -> Result<(), MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (status, revision) = feature_status_and_revision_tx(&tx, plan.request.feature_id)?;
        if !matches!(
            status,
            FeatureLifecycleStatus::Publishing | FeatureLifecycleStatus::VerifyingMain
        ) {
            return Err(MasterError::PublicationCoordinatorUnavailable);
        }
        tx.execute(
            "UPDATE feature_conveyor_features SET status='quarantined',
               lifecycle_revision=lifecycle_revision+1,effect_possible=1,updated_at_ms=?1
             WHERE feature_id=?2 AND lifecycle_revision=?3",
            params![
                u64_to_i64(now_ms)?,
                plan.request.feature_id.to_string(),
                u64_to_i64(revision)?
            ],
        )?;
        tx.execute(
            "INSERT INTO feature_transition_evidence (
               feature_id,lifecycle_revision,from_status,to_status,recorded_at_ms
             ) VALUES (?1,?2,?3,'quarantined',?4)",
            params![
                plan.request.feature_id.to_string(),
                u64_to_i64(revision + 1)?,
                status.as_str(),
                u64_to_i64(now_ms)?
            ],
        )?;
        append_feature_audit_tx(
            &tx,
            "feature_publication_ambiguous",
            Some(plan.request.feature_id),
            now_ms,
            serde_json::json!({
                "publication_id": plan.request.publication_id,
                "action": action.as_str(),
                "effect_possible": true,
                "automatic_retry_authorized": false,
                "reconciliation_required": true,
                "raw_error_present": false
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Read-only cancellation probe for the contained validator. Any authority
    /// drift is reported as not current; storage corruption remains an error.
    pub fn validation_gate_execution_is_current(
        &self,
        request: &FeatureConveyorValidationGateRequest,
        now_ms: u64,
    ) -> Result<bool, MasterError> {
        if request.validate().is_err() {
            return Ok(false);
        }
        let tx = self.connection.unchecked_transaction()?;
        let pending: bool = tx.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM feature_validation_attempts a
               WHERE a.validation_id=?1
                 AND a.request_binding_sha256=?2
                 AND NOT EXISTS (
                   SELECT 1 FROM feature_validation_completions c
                   WHERE c.validation_id=a.validation_id
                 )
             )",
            params![
                request.validation_id.to_string(),
                feature_conveyor_validation_request_binding_sha256(request)?.as_slice(),
            ],
            |row| row.get(0),
        )?;
        if !pending {
            return Ok(false);
        }
        match validate_validation_gate_binding_tx(&tx, request, now_ms) {
            Ok(_) => Ok(true),
            Err(MasterError::Storage(error)) => Err(MasterError::Storage(error)),
            Err(MasterError::Json(error)) => Err(MasterError::Json(error)),
            Err(MasterError::InvalidStoredState(error)) => {
                Err(MasterError::InvalidStoredState(error))
            }
            Err(_) => Ok(false),
        }
    }

    /// Read-only cancellation probe for an in-flight response-only review.
    /// Pause, cancellation, lifecycle, queue, grant, provider, candidate, or
    /// evidence drift becomes `false`; corrupt storage remains an error.
    pub fn review_gateway_execution_is_current(
        &self,
        request: &FeatureConveyorReviewGatewayRequest,
        now_ms: u64,
    ) -> Result<bool, MasterError> {
        if request.validate().is_err() {
            return Ok(false);
        }
        let tx = self.connection.unchecked_transaction()?;
        let pending: bool = tx.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM feature_review_calls c
               WHERE c.review_call_id=?1 AND c.request_binding_sha256=?2
                 AND NOT EXISTS(SELECT 1 FROM feature_review_call_outcomes o
                                WHERE o.review_call_id=c.review_call_id)
             )",
            params![
                request.review_call_id.to_string(),
                feature_conveyor_review_request_binding_sha256(request)?.as_slice(),
            ],
            |row| row.get(0),
        )?;
        if !pending {
            return Ok(false);
        }
        match review_gateway_plan_tx(&tx, request, now_ms) {
            Ok(_) => Ok(true),
            Err(MasterError::Storage(error)) => Err(MasterError::Storage(error)),
            Err(MasterError::Json(error)) => Err(MasterError::Json(error)),
            Err(MasterError::InvalidStoredState(error)) => {
                Err(MasterError::InvalidStoredState(error))
            }
            Err(_) => Ok(false),
        }
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

    /// Pure-SELECT exact current MLX selection for the authenticated designated
    /// bridge. This is the only reconciliation projection and contains no Mac
    /// executable or model-directory paths.
    pub fn local_model_selection_projection(
        &self,
        registration: &DeviceRegistration,
    ) -> Result<LocalModelSelectionProjection, MasterError> {
        let tx = self.connection.unchecked_transaction()?;
        let designation = owner_control_bridge_designation_connection(&tx)?
            .ok_or(MasterError::OwnerControlBridgeNotDesignated)?;
        require_owner_control_bridge_tx(&tx, registration, designation.designation_revision)?;
        let capability = match RemoteWorkContract::from_registration(registration)? {
            RemoteWorkContract::Mlx(capability) => capability,
            _ => return Err(MasterError::LocalModelSelectionRejected),
        };
        let projection = LocalModelSelectionProjection {
            schema_version: LOCAL_MODEL_SELECTION_SCHEMA_VERSION,
            device_id: registration.device_id,
            device_name: registration.device_name.clone(),
            registry_revision: registration.registry_revision,
            designation_revision: designation.designation_revision,
            emergency_pause_revision: emergency_pause_revision_tx(&tx)?,
            emergency_paused: emergency_paused_tx(&tx)?,
            model_id: capability.model,
        };
        projection.validate()?;
        Ok(projection)
    }

    /// Atomically performs the sole permitted model-only capability mutation.
    /// Identity, role, capability kind/provider/bounds, certificate material,
    /// and device/designation identity are copied unchanged from durable state.
    pub fn select_local_model_from_owner_bridge(
        &mut self,
        request: &LocalModelSelectionRequest,
        registration: &DeviceRegistration,
        expected_connection_epoch: u64,
        now_ms: u64,
    ) -> Result<LocalModelSelectionReceipt, MasterError> {
        request.validate()?;
        if request.device_id != registration.device_id
            || request.expected_registry_revision != registration.registry_revision
        {
            return Err(MasterError::LocalModelSelectionRejected);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_owner_control_bridge_tx(&tx, registration, request.expected_designation_revision)?;
        require_unpaused_revision_tx(&tx, request.expected_emergency_pause_revision)?;
        let capability = match RemoteWorkContract::from_registration(registration)? {
            RemoteWorkContract::Mlx(capability) => capability,
            _ => return Err(MasterError::LocalModelSelectionRejected),
        };
        if capability.model == request.model_id {
            return Err(MasterError::LocalModelSelectionRejected);
        }
        let active_attempts: i64 = tx.query_row(
            "SELECT COUNT(*) FROM master_attempts
             WHERE device_id=?1 AND status IN ('leased','cancellation_pending')",
            [registration.device_id.0.to_string()],
            |row| row.get(0),
        )?;
        if active_attempts != 0 {
            return Err(MasterError::LocalModelSelectionRejected);
        }
        let connection_epoch = active_connection_epoch(&tx, registration.device_id)?
            .ok_or(MasterError::ConnectionNotActive)?;
        if connection_epoch != expected_connection_epoch {
            return Err(MasterError::ConnectionEpochMismatch);
        }
        let next_registry_revision = registration
            .registry_revision
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        let next_designation_revision = request
            .expected_designation_revision
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        let mut target_capability = capability;
        target_capability.model = request.model_id.clone();
        target_capability.validate()?;
        let capabilities_json = serde_json::to_string(&vec![target_capability])?;
        let device_changed = tx.execute(
            "UPDATE master_devices SET registry_revision=?1, capabilities_json=?2
             WHERE device_id=?3 AND device_name=?4 AND role_json=?5
               AND registry_revision=?6 AND capabilities_json=?7 AND revoked=0",
            params![
                u64_to_i64(next_registry_revision)?,
                capabilities_json,
                registration.device_id.0.to_string(),
                registration.device_name.as_str(),
                serde_json::to_string(&registration.role)?,
                u64_to_i64(registration.registry_revision)?,
                serde_json::to_string(&registration.capabilities)?,
            ],
        )?;
        let designation_changed = tx.execute(
            "UPDATE feature_owner_control_state
             SET owner_bridge_registry_revision=?1, designation_revision=?2
             WHERE singleton=1 AND owner_bridge_device_id=?3
               AND owner_bridge_registry_revision=?4 AND designation_revision=?5",
            params![
                u64_to_i64(next_registry_revision)?,
                u64_to_i64(next_designation_revision)?,
                registration.device_id.0.to_string(),
                u64_to_i64(registration.registry_revision)?,
                u64_to_i64(request.expected_designation_revision)?,
            ],
        )?;
        if device_changed != 1 || designation_changed != 1 {
            return Err(MasterError::LocalModelSelectionRejected);
        }
        disconnect_device_tx(&tx, registration.device_id, connection_epoch, now_ms)?;
        append_feature_audit_tx(
            &tx,
            "local_model_selected",
            None,
            now_ms,
            serde_json::json!({
                "registry_revision": next_registry_revision,
                "designation_revision": next_designation_revision,
                "emergency_pause_revision": request.expected_emergency_pause_revision,
                "model_changed": true,
                "identity_changed": false,
                "capability_bounds_changed": false,
                "side_effect_executed": true
            }),
        )?;
        let receipt = LocalModelSelectionReceipt {
            schema_version: LOCAL_MODEL_SELECTION_SCHEMA_VERSION,
            device_id: registration.device_id,
            registry_revision: next_registry_revision,
            designation_revision: next_designation_revision,
            emergency_pause_revision: request.expected_emergency_pause_revision,
            model_id: request.model_id.clone(),
            selected_at_ms: now_ms,
            status: LocalModelSelectionStatus::Selected,
        };
        receipt.validate()?;
        tx.commit()?;
        Ok(receipt)
    }

    pub fn feature_conveyor_status(&self) -> Result<FeatureConveyorStatus, MasterError> {
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
                    row.get::<_, i64>(11)?,
                    row.get::<_, i64>(12)?,
                    row.get::<_, i64>(13)?,
                    row.get::<_, i64>(14)?,
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
            repairing: i64_to_u64(counts.7)?,
            paused: i64_to_u64(counts.8)?,
            attention_required: i64_to_u64(counts.9)?,
            failed: i64_to_u64(counts.10)?,
            succeeded: i64_to_u64(counts.11)?,
            cancelled: i64_to_u64(counts.12)?,
            abandoned: i64_to_u64(counts.13)?,
            quarantined: i64_to_u64(counts.14)?,
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
            schema_version: FEATURE_CONVEYOR_STATUS_SCHEMA_VERSION,
            queue_revision,
            startup_quarantine_count: self.feature_startup_quarantines,
            counts_by_status,
            visible_feature_count,
            features_truncated: visible_feature_count > features.len() as u64,
            features,
            owner_guidance,
        })
    }

    /// Returns only the exact designated bridge's bounded owner-control view.
    /// The projection is path/content/provider-output/credential free and
    /// grants no authority by itself.
    pub fn feature_conveyor_owner_control_projection(
        &mut self,
        registration: &DeviceRegistration,
    ) -> Result<FeatureConveyorOwnerControlProjection, MasterError> {
        let tx = self.connection.transaction()?;
        let designation_revision: i64 = tx.query_row(
            "SELECT designation_revision FROM feature_owner_control_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        let designation_revision = i64_to_u64(designation_revision)?;
        require_owner_control_bridge_tx(&tx, registration, designation_revision)?;
        let queue_revision = feature_queue_revision_tx(&tx)?;
        let emergency_paused = emergency_paused_tx(&tx)?;
        let emergency_pause_revision = emergency_pause_revision_tx(&tx)?;
        let current_evidence = activation_evidence_projection_tx(&tx)?;
        let activation = load_feature_activation_tx(&tx)?;
        let evidence = activation
            .as_ref()
            .map(|activation| FeatureConveyorActivationEvidenceProjection {
                repository_gate_proof: Some(activation.evidence.repository_gate_proof),
                restricted_worker_live: Some(activation.evidence.restricted_worker_live),
                review_provider_live: Some(activation.evidence.review_provider_live),
                github_publication_live: Some(activation.evidence.github_publication_live),
                restart_recovery_live: Some(activation.evidence.restart_recovery_live),
                mac_windows_control_event_streaming_live: Some(
                    activation.evidence.mac_windows_control_event_streaming_live,
                ),
            })
            .unwrap_or(current_evidence);
        let active_feature = tx
            .query_row(
                "SELECT f.feature_id,f.current_specification_revision,f.lifecycle_revision,f.status
                 FROM feature_active_lease l
                 JOIN feature_conveyor_features f ON f.feature_id=l.feature_id
                 WHERE l.singleton=1",
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
            .optional()?
            .map(
                |(feature_id, specification_revision, lifecycle_revision, status)| {
                    let feature_id = parse_uuid(&feature_id)?;
                    let lifecycle = FeatureLifecycleStatus::parse(&status)?;
                    let orchestration = load_orchestration_state_tx(&tx, feature_id)?;
                    Ok::<_, MasterError>(FeatureConveyorOwnerActiveFeature {
                        feature_id,
                        specification_revision: i64_to_u64(specification_revision)?,
                        lifecycle_revision: i64_to_u64(lifecycle_revision)?,
                        orchestration_revision: orchestration
                            .as_ref()
                            .map(|state| state.orchestration_revision)
                            .unwrap_or(0),
                        lifecycle_status: owner_lifecycle_status(lifecycle)?,
                        stage: orchestration
                            .as_ref()
                            .map(|state| state.stage)
                            .unwrap_or_else(|| orchestration_stage_for_lifecycle_status(lifecycle)),
                        owner_paused: orchestration.as_ref().is_some_and(|state| {
                            state.pause_kind == Some(FeatureConveyorOrchestrationPauseKind::Owner)
                        }),
                    })
                },
            )
            .transpose()?;
        let activation_status = if activation.is_some() {
            FeatureConveyorActivationStatus::Active
        } else {
            FeatureConveyorActivationStatus::Inactive
        };
        let activation_ready =
            activation.is_none() && !emergency_paused && evidence.complete().is_some();
        let activation_blocker = if activation.is_some() {
            FeatureConveyorActivationBlocker::AlreadyActivated
        } else if emergency_paused {
            FeatureConveyorActivationBlocker::EmergencyPaused
        } else if evidence.complete().is_none() {
            FeatureConveyorActivationBlocker::EvidenceRequired
        } else {
            FeatureConveyorActivationBlocker::None
        };
        let projection = FeatureConveyorOwnerControlProjection {
            schema_version: FEATURE_CONVEYOR_OWNER_ACTIVATION_SCHEMA_VERSION,
            queue_revision,
            emergency_paused,
            emergency_pause_revision,
            owner_control_designation_revision: designation_revision,
            activation_status,
            activation_id: activation.as_ref().map(|receipt| receipt.activation_id),
            activation_ready,
            activation_blocker,
            active_feature,
            evidence,
        };
        projection.validate()?;
        tx.commit()?;
        Ok(projection)
    }

    /// Returns the bounded owner-token loopback preflight needed to construct
    /// one pause-bound, contiguous activation-evidence admission. This has no
    /// enrolled-device route and grants no mutation authority by itself.
    pub fn feature_conveyor_activation_evidence_admission_projection(
        &mut self,
    ) -> Result<FeatureConveyorActivationEvidenceAdmissionProjection, MasterError> {
        let tx = self.connection.transaction()?;
        let evidence = activation_evidence_projection_tx(&tx)?;
        let activation = load_feature_activation_tx(&tx)?;
        let projection = FeatureConveyorActivationEvidenceAdmissionProjection {
            schema_version: FEATURE_CONVEYOR_OWNER_ACTIVATION_SCHEMA_VERSION,
            emergency_paused: emergency_paused_tx(&tx)?,
            emergency_pause_revision: emergency_pause_revision_tx(&tx)?,
            activation_status: if activation.is_some() {
                FeatureConveyorActivationStatus::Active
            } else {
                FeatureConveyorActivationStatus::Inactive
            },
            activation_id: activation.as_ref().map(|receipt| receipt.activation_id),
            evidence,
        };
        projection.validate()?;
        tx.commit()?;
        Ok(projection)
    }

    /// Owner-token loopback admission of one exact proof-controller receipt.
    /// The report body remains outside Assemblywright durable state.
    pub fn admit_feature_activation_evidence(
        &mut self,
        request: &FeatureConveyorActivationEvidenceAdmissionRequest,
        recorded_at_ms: u64,
    ) -> Result<FeatureConveyorActivationEvidenceAdmissionReceipt, MasterError> {
        request.validate()?;
        if recorded_at_ms == 0 || request.observed_at_ms > recorded_at_ms {
            return Err(MasterError::InvalidFeatureConveyorInput(
                "activation evidence time is invalid".to_string(),
            ));
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(existing) = activation_evidence_by_id_tx(&tx, request.evidence_id)? {
            let expected = activation_evidence_receipt(request);
            if existing == expected {
                tx.commit()?;
                return Ok(existing);
            }
            return Err(MasterError::FeatureActivationEvidenceUnavailable);
        }
        let activated: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM feature_orchestration_activation WHERE singleton=1)",
            [],
            |row| row.get(0),
        )?;
        if activated {
            return Err(MasterError::FeatureActivationImmutable);
        }
        require_unpaused_revision_tx(&tx, request.expected_emergency_pause_revision)?;
        let category = activation_evidence_category_str(request.category);
        let current: i64 = tx.query_row(
            "SELECT COALESCE(MAX(revision),0) FROM feature_activation_evidence WHERE category=?1",
            [category],
            |row| row.get(0),
        )?;
        if i64_to_u64(current)? != request.expected_current_revision {
            return Err(MasterError::FeatureActivationEvidenceUnavailable);
        }
        tx.execute(
            "INSERT INTO feature_activation_evidence(
               category,revision,evidence_id,origin,receipt_sha256,observed_at_ms,
               emergency_pause_revision,recorded_at_ms
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                category,
                u64_to_i64(request.revision)?,
                request.evidence_id.to_string(),
                activation_evidence_origin_str(request.origin),
                request.receipt_sha256.as_slice(),
                u64_to_i64(request.observed_at_ms)?,
                u64_to_i64(request.expected_emergency_pause_revision)?,
                u64_to_i64(recorded_at_ms)?,
            ],
        )?;
        append_feature_audit_tx(
            &tx,
            "feature_activation_evidence_admitted",
            None,
            recorded_at_ms,
            serde_json::json!({
                "category": category,
                "origin": activation_evidence_origin_str(request.origin),
                "revision": request.revision,
                "receipt_digest_present": true,
                "observed_at_ms": request.observed_at_ms,
                "emergency_pause_revision": request.expected_emergency_pause_revision,
                "raw_evidence_retained": false,
                "side_effect_executed": false
            }),
        )?;
        let receipt = activation_evidence_receipt(request);
        receipt.validate()?;
        tx.commit()?;
        Ok(receipt)
    }

    /// The singleton activation transition. Remote callers can select only
    /// exact already-admitted Windows records; they cannot supply proof bytes.
    pub fn activate_feature_orchestration_from_owner_bridge(
        &mut self,
        request: &FeatureConveyorActivationRequest,
        registration: &DeviceRegistration,
        now_ms: u64,
    ) -> Result<FeatureConveyorActivationReceipt, MasterError> {
        request.validate()?;
        if now_ms == 0 {
            return Err(MasterError::InvalidSystemClock);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_owner_control_bridge_tx(
            &tx,
            registration,
            request.expected_owner_control_designation_revision,
        )?;
        require_unpaused_revision_tx(&tx, request.expected_emergency_pause_revision)?;
        if let Some(existing) = load_feature_activation_tx(&tx)? {
            if activation_receipt_matches_request(&existing, request) {
                tx.commit()?;
                return Ok(existing);
            }
            return Err(MasterError::FeatureActivationImmutable);
        }
        require_queue_revision_tx(&tx, request.expected_queue_revision)?;
        require_current_activation_evidence_tx(&tx, &request.evidence)?;
        let activation_id = activation_id_for_request(request);
        tx.execute(
            "INSERT INTO feature_orchestration_activation(
               singleton,activation_id,queue_revision,owner_control_designation_revision,
               emergency_pause_revision,repository_gate_evidence_id,
               restricted_worker_evidence_id,review_provider_evidence_id,
               github_publication_evidence_id,restart_recovery_evidence_id,
               control_event_streaming_evidence_id,activated_at_ms
             ) VALUES(1,?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                activation_id.to_string(),
                u64_to_i64(request.expected_queue_revision)?,
                u64_to_i64(request.expected_owner_control_designation_revision)?,
                u64_to_i64(request.expected_emergency_pause_revision)?,
                request
                    .evidence
                    .repository_gate_proof
                    .evidence_id
                    .to_string(),
                request
                    .evidence
                    .restricted_worker_live
                    .evidence_id
                    .to_string(),
                request
                    .evidence
                    .review_provider_live
                    .evidence_id
                    .to_string(),
                request
                    .evidence
                    .github_publication_live
                    .evidence_id
                    .to_string(),
                request
                    .evidence
                    .restart_recovery_live
                    .evidence_id
                    .to_string(),
                request
                    .evidence
                    .mac_windows_control_event_streaming_live
                    .evidence_id
                    .to_string(),
                u64_to_i64(now_ms)?,
            ],
        )?;
        append_feature_audit_tx(
            &tx,
            "feature_orchestration_activated",
            None,
            now_ms,
            serde_json::json!({
                "queue_revision": request.expected_queue_revision,
                "owner_control_designation_revision": request.expected_owner_control_designation_revision,
                "emergency_pause_revision": request.expected_emergency_pause_revision,
                "repository_gate_proof_present": true,
                "live_evidence_category_count": 5,
                "evidence_reference_count": 6,
                "raw_evidence_retained": false,
                "side_effect_executed": false
            }),
        )?;
        let receipt = FeatureConveyorActivationReceipt {
            schema_version: FEATURE_CONVEYOR_OWNER_ACTIVATION_SCHEMA_VERSION,
            activation_id,
            queue_revision: request.expected_queue_revision,
            owner_control_designation_revision: request.expected_owner_control_designation_revision,
            emergency_pause_revision: request.expected_emergency_pause_revision,
            evidence: request.evidence,
            activated_at_ms: now_ms,
            status: FeatureConveyorActivationStatus::Active,
        };
        receipt.validate()?;
        tx.commit()?;
        Ok(receipt)
    }

    pub fn pause_feature_orchestration_from_owner_bridge(
        &mut self,
        request: &FeatureConveyorOwnerOrchestrationControlRequest,
        registration: &DeviceRegistration,
        now_ms: u64,
    ) -> Result<FeatureConveyorOwnerOrchestrationControlReceipt, MasterError> {
        self.owner_control_feature_orchestration(request, registration, now_ms, true)
    }

    pub fn resume_feature_orchestration_from_owner_bridge(
        &mut self,
        request: &FeatureConveyorOwnerOrchestrationControlRequest,
        registration: &DeviceRegistration,
        now_ms: u64,
    ) -> Result<FeatureConveyorOwnerOrchestrationControlReceipt, MasterError> {
        self.owner_control_feature_orchestration(request, registration, now_ms, false)
    }

    fn owner_control_feature_orchestration(
        &mut self,
        request: &FeatureConveyorOwnerOrchestrationControlRequest,
        registration: &DeviceRegistration,
        now_ms: u64,
        pause: bool,
    ) -> Result<FeatureConveyorOwnerOrchestrationControlReceipt, MasterError> {
        request.validate()?;
        if now_ms == 0 {
            return Err(MasterError::InvalidSystemClock);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_owner_control_bridge_tx(
            &tx,
            registration,
            request.expected_owner_control_designation_revision,
        )?;
        require_unpaused_revision_tx(&tx, request.expected_emergency_pause_revision)?;
        let action = if pause { "pause" } else { "resume" };
        let request_sha256 = owner_orchestration_control_request_sha256(action, request);
        if let Some(existing) = load_owner_orchestration_control_tx(&tx, &request_sha256)? {
            tx.commit()?;
            return Ok(existing);
        }
        require_queue_revision_tx(&tx, request.expected_queue_revision)?;
        let activated: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM feature_orchestration_activation WHERE singleton=1)",
            [],
            |row| row.get(0),
        )?;
        if !activated {
            return Err(MasterError::OrchestrationInactive);
        }
        require_active_lease_tx(&tx, request.feature_id)?;
        let (current_status, lifecycle_revision) =
            feature_status_and_revision_tx(&tx, request.feature_id)?;
        if lifecycle_revision != request.expected_lifecycle_revision {
            return Err(MasterError::StaleFeatureLifecycleRevision {
                expected: request.expected_lifecycle_revision,
                found: lifecycle_revision,
            });
        }
        let state = load_orchestration_state_tx(&tx, request.feature_id)?.ok_or(
            MasterError::StaleOrchestrationRevision {
                expected: request.expected_orchestration_revision,
                found: 0,
            },
        )?;
        if state.orchestration_revision != request.expected_orchestration_revision {
            return Err(MasterError::StaleOrchestrationRevision {
                expected: request.expected_orchestration_revision,
                found: state.orchestration_revision,
            });
        }
        if state.effect_possible {
            return Err(MasterError::OrchestrationEffectAmbiguous);
        }
        let previous_checkpoint = load_orchestration_checkpoint_tx(&tx, state.checkpoint_id)?;
        let (
            stage,
            resume_stage,
            pause_kind,
            target_status,
            checkpoint_action,
            active_processing_ms,
            clock_started_at_ms,
        ) = if pause {
            if current_status == FeatureLifecycleStatus::Paused
                || state.stage == FeatureConveyorOrchestrationStage::Paused
            {
                return Err(MasterError::InvalidFeatureTransition);
            }
            let active_processing_ms = state.active_processing_ms.saturating_add(
                state
                    .clock_started_at_ms
                    .map(|started| now_ms.saturating_sub(started))
                    .unwrap_or(0),
            );
            if active_processing_ms > MAX_FEATURE_CONVEYOR_ACTIVE_PROCESSING_MS {
                return Err(MasterError::InvalidStoredState(
                    "owner pause exceeded orchestration budget".to_string(),
                ));
            }
            (
                FeatureConveyorOrchestrationStage::Paused,
                Some(state.stage),
                Some(FeatureConveyorOrchestrationPauseKind::Owner),
                FeatureLifecycleStatus::Paused,
                previous_checkpoint.action,
                active_processing_ms,
                None,
            )
        } else {
            if current_status != FeatureLifecycleStatus::Paused
                || state.stage != FeatureConveyorOrchestrationStage::Paused
                || state.pause_kind != Some(FeatureConveyorOrchestrationPauseKind::Owner)
            {
                return Err(MasterError::InvalidFeatureTransition);
            }
            let resume_stage = state.resume_stage.ok_or_else(|| {
                MasterError::InvalidStoredState("owner pause omitted resume stage".to_string())
            })?;
            (
                resume_stage,
                None,
                None,
                lifecycle_status_for_orchestration_stage(resume_stage),
                orchestration_action_for_stage(resume_stage),
                state.active_processing_ms,
                Some(now_ms),
            )
        };
        let next_lifecycle_revision = lifecycle_revision.checked_add(1).ok_or_else(|| {
            MasterError::InvalidStoredState("lifecycle revision overflowed".to_string())
        })?;
        let next_orchestration_revision =
            state.orchestration_revision.checked_add(1).ok_or_else(|| {
                MasterError::InvalidStoredState("orchestration revision overflowed".to_string())
            })?;
        let decision = DerivedOrchestrationDecision {
            stage,
            action: checkpoint_action,
            reason: FeatureConveyorOrchestrationReason::CheckpointEffectFree,
            pause_kind,
            next_retry_at_ms: None,
            evidence_sha256: None,
            effect_possible: false,
        };
        let checkpoint_sha256 = orchestration_checkpoint_sha256(
            request.feature_id,
            next_orchestration_revision,
            next_lifecycle_revision,
            decision,
            state.replacement_candidates_used,
            active_processing_ms,
        );
        let checkpoint_id = orchestration_checkpoint_id(checkpoint_sha256);
        if tx.execute(
            "UPDATE feature_conveyor_features
             SET status=?1,lifecycle_revision=?2,effect_possible=0,updated_at_ms=?3
             WHERE feature_id=?4 AND status=?5 AND lifecycle_revision=?6",
            params![
                target_status.as_str(),
                u64_to_i64(next_lifecycle_revision)?,
                u64_to_i64(now_ms)?,
                request.feature_id.to_string(),
                current_status.as_str(),
                u64_to_i64(lifecycle_revision)?,
            ],
        )? != 1
        {
            return Err(MasterError::InvalidFeatureTransition);
        }
        tx.execute(
            "INSERT INTO feature_transition_evidence(
               feature_id,lifecycle_revision,from_status,to_status,recorded_at_ms
             ) VALUES(?1,?2,?3,?4,?5)",
            params![
                request.feature_id.to_string(),
                u64_to_i64(next_lifecycle_revision)?,
                current_status.as_str(),
                target_status.as_str(),
                u64_to_i64(now_ms)?,
            ],
        )?;
        tx.execute(
            "INSERT INTO feature_orchestration_checkpoints(
               checkpoint_id,feature_id,orchestration_revision,lifecycle_revision,stage,action,
               reason,checkpoint_sha256,evidence_sha256,replacement_candidates_used,
               active_processing_ms,effect_possible,recorded_at_ms
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,NULL,?9,?10,0,?11)",
            params![
                checkpoint_id.to_string(),
                request.feature_id.to_string(),
                u64_to_i64(next_orchestration_revision)?,
                u64_to_i64(next_lifecycle_revision)?,
                orchestration_stage_str(stage),
                orchestration_action_str(checkpoint_action),
                orchestration_reason_str(FeatureConveyorOrchestrationReason::CheckpointEffectFree),
                checkpoint_sha256.as_slice(),
                i64::from(state.replacement_candidates_used),
                u64_to_i64(active_processing_ms)?,
                u64_to_i64(now_ms)?,
            ],
        )?;
        if tx.execute(
            "UPDATE feature_orchestration_state SET
               orchestration_revision=?1,checkpoint_id=?2,stage=?3,resume_stage=?4,pause_kind=?5,
               active_processing_ms=?6,clock_started_at_ms=?7,next_retry_at_ms=NULL,
               effect_possible=0,updated_at_ms=?8
             WHERE feature_id=?9 AND orchestration_revision=?10",
            params![
                u64_to_i64(next_orchestration_revision)?,
                checkpoint_id.to_string(),
                orchestration_stage_str(stage),
                resume_stage.map(orchestration_stage_str),
                pause_kind.map(orchestration_pause_kind_str),
                u64_to_i64(active_processing_ms)?,
                clock_started_at_ms.map(u64_to_i64).transpose()?,
                u64_to_i64(now_ms)?,
                request.feature_id.to_string(),
                u64_to_i64(state.orchestration_revision)?,
            ],
        )? != 1
        {
            return Err(MasterError::StaleOrchestrationRevision {
                expected: state.orchestration_revision,
                found: state.orchestration_revision,
            });
        }
        tx.execute(
            "INSERT INTO feature_owner_orchestration_controls(
               request_sha256,action,feature_id,lifecycle_revision,orchestration_revision,
               queue_revision,owner_control_designation_revision,emergency_pause_revision,
               checkpoint_id,checkpoint_sha256,recorded_at_ms
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![
                request_sha256.as_slice(),
                action,
                request.feature_id.to_string(),
                u64_to_i64(next_lifecycle_revision)?,
                u64_to_i64(next_orchestration_revision)?,
                u64_to_i64(request.expected_queue_revision)?,
                u64_to_i64(request.expected_owner_control_designation_revision)?,
                u64_to_i64(request.expected_emergency_pause_revision)?,
                checkpoint_id.to_string(),
                checkpoint_sha256.as_slice(),
                u64_to_i64(now_ms)?,
            ],
        )?;
        append_feature_audit_tx(
            &tx,
            if pause {
                "feature_orchestration_owner_paused"
            } else {
                "feature_orchestration_owner_resumed"
            },
            Some(request.feature_id),
            now_ms,
            serde_json::json!({
                "lifecycle_revision": next_lifecycle_revision,
                "orchestration_revision": next_orchestration_revision,
                "queue_revision": request.expected_queue_revision,
                "owner_control_designation_revision": request.expected_owner_control_designation_revision,
                "emergency_pause_revision": request.expected_emergency_pause_revision,
                "checkpoint_digest_present": true,
                "effect_possible": false,
                "side_effect_executed": false
            }),
        )?;
        let receipt = FeatureConveyorOwnerOrchestrationControlReceipt {
            schema_version: FEATURE_CONVEYOR_OWNER_ACTIVATION_SCHEMA_VERSION,
            feature_id: request.feature_id,
            lifecycle_revision: next_lifecycle_revision,
            orchestration_revision: next_orchestration_revision,
            queue_revision: request.expected_queue_revision,
            owner_control_designation_revision: request.expected_owner_control_designation_revision,
            emergency_pause_revision: request.expected_emergency_pause_revision,
            checkpoint_id,
            checkpoint_sha256,
            status: if pause {
                FeatureConveyorOwnerOrchestrationControlStatus::Paused
            } else {
                FeatureConveyorOwnerOrchestrationControlStatus::Resumed
            },
        };
        receipt.validate()?;
        tx.commit()?;
        Ok(receipt)
    }

    /// Advances only the master-owned orchestration ledger. It remains inert
    /// until schema-v19 owner activation durably binds the complete admitted
    /// proof set; no caller supplies actions or evidence to this coordinator.
    pub fn coordinate_feature_orchestration(
        &mut self,
        feature_id: Uuid,
        expected_orchestration_revision: u64,
        now_ms: u64,
    ) -> Result<FeatureConveyorOrchestrationProjection, MasterError> {
        if feature_id.is_nil() || now_ms == 0 {
            return Err(MasterError::InvalidFeatureConveyorInput(
                "orchestration requires a non-nil feature and positive time".to_string(),
            ));
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        // Emergency pause dominates every orchestration concern, including
        // activation, lease lookup, CAS validation, and active-time accounting.
        // Returning from the open transaction leaves no checkpoint or audit.
        if emergency_paused_tx(&tx)? {
            return Err(MasterError::EmergencyPaused);
        }
        let activated: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM feature_orchestration_activation WHERE singleton=1)",
            [],
            |row| row.get(0),
        )?;
        if !activated {
            return Err(MasterError::OrchestrationInactive);
        }
        require_active_lease_tx(&tx, feature_id)?;
        let (current_status, mut lifecycle_revision) =
            feature_status_and_revision_tx(&tx, feature_id)?;
        // Owner resolution states are immutable to automatic coordination.
        // In particular, cancellation must remain available for the owner's
        // explicit reconciliation and abandonment action.
        if matches!(
            current_status,
            FeatureLifecycleStatus::Cancelled
                | FeatureLifecycleStatus::AttentionRequired
                | FeatureLifecycleStatus::Abandoned
                | FeatureLifecycleStatus::Succeeded
                | FeatureLifecycleStatus::Failed
        ) {
            return Err(MasterError::InvalidFeatureTransition);
        }
        let existing = load_orchestration_state_tx(&tx, feature_id)?;
        let current_revision = existing
            .as_ref()
            .map(|state| state.orchestration_revision)
            .unwrap_or(0);
        if current_revision != expected_orchestration_revision {
            return Err(MasterError::StaleOrchestrationRevision {
                expected: expected_orchestration_revision,
                found: current_revision,
            });
        }

        let mut active_processing_ms = existing
            .as_ref()
            .map(|state| state.active_processing_ms)
            .unwrap_or(0);
        let clock_started_at_ms = existing
            .as_ref()
            .and_then(|state| state.clock_started_at_ms);
        let elapsed = clock_started_at_ms
            .map(|started| now_ms.saturating_sub(started))
            .unwrap_or(0);
        let charged_active_processing_ms = active_processing_ms.saturating_add(elapsed);
        let budget_exhausted =
            charged_active_processing_ms >= MAX_FEATURE_CONVEYOR_ACTIVE_PROCESSING_MS;
        if budget_exhausted {
            active_processing_ms = MAX_FEATURE_CONVEYOR_ACTIVE_PROCESSING_MS;
        } else if existing.is_none() {
            active_processing_ms = 0;
        }

        let mut decision = derive_orchestration_decision_tx(
            &tx,
            feature_id,
            current_status,
            existing.as_ref(),
            now_ms,
        )?;
        if budget_exhausted
            && !matches!(
                decision.stage,
                FeatureConveyorOrchestrationStage::Succeeded
                    | FeatureConveyorOrchestrationStage::Quarantined
                    | FeatureConveyorOrchestrationStage::Failed
            )
        {
            decision = DerivedOrchestrationDecision {
                stage: FeatureConveyorOrchestrationStage::AttentionRequired,
                action: FeatureConveyorOrchestrationAction::OwnerAttentionRequired,
                reason: FeatureConveyorOrchestrationReason::ActiveProcessingBudgetExhausted,
                pause_kind: None,
                next_retry_at_ms: None,
                evidence_sha256: None,
                effect_possible: false,
            };
        }

        let starts_clock = !matches!(
            decision.stage,
            FeatureConveyorOrchestrationStage::Paused
                | FeatureConveyorOrchestrationStage::AttentionRequired
                | FeatureConveyorOrchestrationStage::Failed
                | FeatureConveyorOrchestrationStage::Succeeded
                | FeatureConveyorOrchestrationStage::Quarantined
        );

        if let Some(existing) = existing.as_ref() {
            let checkpoint = load_orchestration_checkpoint_tx(&tx, existing.checkpoint_id)?;
            if checkpoint.stage == decision.stage
                && checkpoint.action == decision.action
                && checkpoint.reason == decision.reason
                && existing.pause_kind == decision.pause_kind
                && existing.next_retry_at_ms == decision.next_retry_at_ms
                && elapsed == 0
                && (!starts_clock || existing.clock_started_at_ms.is_some())
            {
                let projection = orchestration_projection(feature_id, existing, &checkpoint);
                projection.validate()?;
                tx.commit()?;
                return Ok(projection);
            }
        }

        let target_status = lifecycle_status_for_orchestration_stage(decision.stage);
        if target_status != current_status {
            let next_lifecycle_revision = lifecycle_revision
                .checked_add(1)
                .ok_or_else(|| MasterError::InvalidStoredState("lifecycle overflow".to_string()))?;
            if tx.execute(
                "UPDATE feature_conveyor_features SET status=?1,lifecycle_revision=?2,
                   effect_possible=?3,updated_at_ms=?4
                 WHERE feature_id=?5 AND status=?6 AND lifecycle_revision=?7",
                params![
                    target_status.as_str(),
                    u64_to_i64(next_lifecycle_revision)?,
                    i64::from(decision.effect_possible),
                    u64_to_i64(now_ms)?,
                    feature_id.to_string(),
                    current_status.as_str(),
                    u64_to_i64(lifecycle_revision)?,
                ],
            )? != 1
            {
                return Err(MasterError::InvalidFeatureTransition);
            }
            tx.execute(
                "INSERT INTO feature_transition_evidence(
                   feature_id,lifecycle_revision,from_status,to_status,accepted_evidence_sha256,
                   recorded_at_ms
                 ) VALUES(?1,?2,?3,?4,?5,?6)",
                params![
                    feature_id.to_string(),
                    u64_to_i64(next_lifecycle_revision)?,
                    current_status.as_str(),
                    target_status.as_str(),
                    decision
                        .evidence_sha256
                        .as_ref()
                        .map(|digest| digest.as_slice()),
                    u64_to_i64(now_ms)?,
                ],
            )?;
            lifecycle_revision = next_lifecycle_revision;
        }

        let replacement_candidates_used = existing
            .as_ref()
            .map(|state| state.replacement_candidates_used)
            .unwrap_or(0);
        if replacement_candidates_used > MAX_FEATURE_CONVEYOR_REPLACEMENT_CANDIDATES {
            return Err(MasterError::InvalidStoredState(
                "orchestration repair budget exceeds protocol maximum".to_string(),
            ));
        }
        if existing.is_some() && !budget_exhausted {
            active_processing_ms = charged_active_processing_ms;
        }
        let orchestration_revision = current_revision
            .checked_add(1)
            .ok_or_else(|| MasterError::InvalidStoredState("orchestration overflow".to_string()))?;
        let checkpoint_sha256 = orchestration_checkpoint_sha256(
            feature_id,
            orchestration_revision,
            lifecycle_revision,
            decision,
            replacement_candidates_used,
            active_processing_ms,
        );
        let checkpoint_id = orchestration_checkpoint_id(checkpoint_sha256);
        let resume_stage = if decision.stage == FeatureConveyorOrchestrationStage::Paused {
            Some(orchestration_stage_for_lifecycle_status(current_status))
        } else {
            None
        };
        tx.execute(
            "INSERT INTO feature_orchestration_checkpoints(
               checkpoint_id,feature_id,orchestration_revision,lifecycle_revision,stage,action,
               reason,checkpoint_sha256,evidence_sha256,replacement_candidates_used,
               active_processing_ms,effect_possible,recorded_at_ms
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
            params![
                checkpoint_id.to_string(),
                feature_id.to_string(),
                u64_to_i64(orchestration_revision)?,
                u64_to_i64(lifecycle_revision)?,
                orchestration_stage_str(decision.stage),
                orchestration_action_str(decision.action),
                orchestration_reason_str(decision.reason),
                checkpoint_sha256.as_slice(),
                decision
                    .evidence_sha256
                    .as_ref()
                    .map(|digest| digest.as_slice()),
                i64::from(replacement_candidates_used),
                u64_to_i64(active_processing_ms)?,
                i64::from(decision.effect_possible),
                u64_to_i64(now_ms)?,
            ],
        )?;
        tx.execute(
            "INSERT INTO feature_orchestration_state(
               feature_id,orchestration_revision,checkpoint_id,stage,resume_stage,pause_kind,
               replacement_candidates_used,active_processing_ms,clock_started_at_ms,
               next_retry_at_ms,effect_possible,updated_at_ms
             ) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
             ON CONFLICT(feature_id) DO UPDATE SET
               orchestration_revision=excluded.orchestration_revision,
               checkpoint_id=excluded.checkpoint_id,stage=excluded.stage,
               resume_stage=excluded.resume_stage,pause_kind=excluded.pause_kind,
               replacement_candidates_used=excluded.replacement_candidates_used,
               active_processing_ms=excluded.active_processing_ms,
               clock_started_at_ms=excluded.clock_started_at_ms,
               next_retry_at_ms=excluded.next_retry_at_ms,
               effect_possible=excluded.effect_possible,updated_at_ms=excluded.updated_at_ms
             WHERE feature_orchestration_state.orchestration_revision=?13",
            params![
                feature_id.to_string(),
                u64_to_i64(orchestration_revision)?,
                checkpoint_id.to_string(),
                orchestration_stage_str(decision.stage),
                resume_stage.map(orchestration_stage_str),
                decision.pause_kind.map(orchestration_pause_kind_str),
                i64::from(replacement_candidates_used),
                u64_to_i64(active_processing_ms)?,
                starts_clock.then_some(u64_to_i64(now_ms)?),
                decision.next_retry_at_ms.map(u64_to_i64).transpose()?,
                i64::from(decision.effect_possible),
                u64_to_i64(now_ms)?,
                u64_to_i64(current_revision)?,
            ],
        )?;
        append_feature_audit_tx(
            &tx,
            "feature_orchestration_checkpointed",
            Some(feature_id),
            now_ms,
            serde_json::json!({
                "orchestration_revision": orchestration_revision,
                "lifecycle_revision": lifecycle_revision,
                "stage": orchestration_stage_str(decision.stage),
                "action": orchestration_action_str(decision.action),
                "reason": orchestration_reason_str(decision.reason),
                "checkpoint_digest_present": true,
                "evidence_digest_present": decision.evidence_sha256.is_some(),
                "replacement_candidates_used": replacement_candidates_used,
                "active_processing_ms": active_processing_ms,
                "active_processing_budget_ms": MAX_FEATURE_CONVEYOR_ACTIVE_PROCESSING_MS,
                "effect_possible": decision.effect_possible,
                "automatic_lease_release": false,
                "side_effect_executed": false
            }),
        )?;
        let state = load_orchestration_state_tx(&tx, feature_id)?.ok_or_else(|| {
            MasterError::InvalidStoredState("orchestration state was not stored".to_string())
        })?;
        let checkpoint = load_orchestration_checkpoint_tx(&tx, checkpoint_id)?;
        let projection = orchestration_projection(feature_id, &state, &checkpoint);
        projection.validate()?;
        tx.commit()?;
        Ok(projection)
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
                FeatureLifecycleStatus::Cancelled
                | FeatureLifecycleStatus::AttentionRequired
                | FeatureLifecycleStatus::Failed
                | FeatureLifecycleStatus::Quarantined => (
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
        let current_queue_revision = feature_queue_revision_tx(&tx)?;
        if current_queue_revision != expected_queue_revision {
            let replay_queue_revision = expected_queue_revision.checked_add(1);
            if let (
                Some((registration, designation_revision, emergency_pause_revision)),
                Some(replay_queue_revision),
            ) = (owner_binding, replay_queue_revision)
            {
                if current_queue_revision == replay_queue_revision {
                    require_owner_control_bridge_tx(&tx, registration, designation_revision)?;
                    require_unpaused_revision_tx(&tx, emergency_pause_revision)?;
                    require_grants_tx(
                        &tx,
                        specification.repository_id,
                        specification.grants,
                        now_ms,
                    )?;
                    if let Some(snapshot) = exact_owner_bridge_enqueue_replay_tx(
                        &tx,
                        specification,
                        &canonical_manifest,
                        expected_queue_revision,
                        replay_queue_revision,
                        (registration, designation_revision, emergency_pause_revision),
                    )? {
                        return Ok(snapshot);
                    }
                    return Err(MasterError::FeatureSpecificationImmutable);
                }
            }
            return Err(MasterError::StaleFeatureQueueRevision {
                expected: expected_queue_revision,
                found: current_queue_revision,
            });
        }
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
        let audit_metadata = feature_enqueue_audit_metadata(
            specification,
            expected_queue_revision,
            next_queue_revision,
            i64_to_u64(position)?,
            owner_binding,
        )?;
        append_feature_audit_tx(
            &tx,
            "feature_enqueued",
            Some(specification.feature_id),
            now_ms,
            audit_metadata,
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

    pub fn prepare_repository_snapshot_claim(
        &mut self,
        requested: &FeatureSnapshotClaimPlan,
        now_ms: u64,
    ) -> Result<FeatureSnapshotClaimPlan, MasterError> {
        validate_snapshot_claim_plan(requested, now_ms)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Deferred)?;
        require_snapshot_claim_plan_tx(&tx, requested, now_ms)?;
        tx.commit()?;
        Ok(requested.clone())
    }

    pub fn finalize_repository_snapshot_claim(
        &mut self,
        plan: &FeatureSnapshotClaimPlan,
        snapshot: &RepositorySnapshotEvidence,
        now_ms: u64,
    ) -> Result<FeatureClaim, MasterError> {
        validate_snapshot_claim_plan(plan, now_ms)?;
        if snapshot.snapshot_id.is_nil()
            || snapshot.snapshot_sha256 == [0; 32]
            || snapshot.base_commit != plan.base_commit
        {
            return Err(MasterError::InvalidFeatureConveyorInput(
                "snapshot claim requires an exact nonzero snapshot binding".to_string(),
            ));
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_snapshot_claim_plan_tx(&tx, plan, now_ms)?;
        let lease_id = Uuid::new_v4();
        tx.execute(
            "INSERT INTO feature_repository_snapshot_claims (
               snapshot_id, snapshot_sha256, feature_id, specification_revision,
               lease_id, base_commit, scope_sha256, provider_id, model_id,
               registration_grant_revision, cloud_disclosure_grant_revision,
               publication_grant_revision, emergency_pause_revision,
               queue_revision, claimed_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                snapshot.snapshot_id.to_string(),
                snapshot.snapshot_sha256.as_slice(),
                plan.feature_id.to_string(),
                u64_to_i64(plan.specification_revision)?,
                lease_id.to_string(),
                plan.base_commit,
                plan.scope_sha256.as_slice(),
                plan.provider_id,
                plan.model_id,
                u64_to_i64(plan.grants.registration)?,
                u64_to_i64(plan.grants.cloud_disclosure)?,
                u64_to_i64(plan.grants.autonomous_publication)?,
                u64_to_i64(plan.expected_emergency_pause_revision)?,
                u64_to_i64(plan.expected_queue_revision)?,
                u64_to_i64(now_ms)?,
            ],
        )?;
        tx.execute(
            "INSERT INTO feature_active_lease (
               singleton, feature_id, lease_id, claimed_at_ms, snapshot_id
             ) VALUES (1, ?1, ?2, ?3, ?4)",
            params![
                plan.feature_id.to_string(),
                lease_id.to_string(),
                u64_to_i64(now_ms)?,
                snapshot.snapshot_id.to_string(),
            ],
        )?;
        let changed = tx.execute(
            "UPDATE feature_conveyor_features
             SET status = 'implementing', lifecycle_revision = lifecycle_revision + 1,
                 updated_at_ms = ?1
             WHERE feature_id = ?2 AND status = 'queued'",
            params![u64_to_i64(now_ms)?, plan.feature_id.to_string()],
        )?;
        if changed != 1 {
            return Err(MasterError::InvalidFeatureTransition);
        }
        let lifecycle_revision = tx.query_row(
            "SELECT lifecycle_revision FROM feature_conveyor_features WHERE feature_id = ?1",
            [plan.feature_id.to_string()],
            |row| row.get::<_, i64>(0),
        )?;
        let next_queue_revision = increment_queue_revision_tx(&tx, plan.expected_queue_revision)?;
        append_feature_audit_tx(
            &tx,
            "feature_snapshot_claimed",
            Some(plan.feature_id),
            now_ms,
            serde_json::json!({
                "from_status": "queued",
                "to_status": "implementing",
                "lifecycle_revision": lifecycle_revision,
                "queue_revision": next_queue_revision,
                "lease_present": true,
                "provider_snapshot_present": true,
                "grant_snapshot_present": true,
                "repository_snapshot_id_present": true,
                "repository_snapshot_digest_present": true,
                "base_commit_present": true,
                "scope_digest_present": true,
                "emergency_pause_revision": plan.expected_emergency_pause_revision,
                "effect_possible": false,
                "side_effect_executed": false
            }),
        )?;
        tx.commit()?;
        Ok(FeatureClaim {
            feature_id: plan.feature_id,
            specification_revision: plan.specification_revision,
            lifecycle_revision: i64_to_u64(lifecycle_revision)?,
            lease_id,
            provider_id: plan.provider_id.clone(),
            model_id: plan.model_id.clone(),
            grants: plan.grants,
            snapshot_id: snapshot.snapshot_id,
            snapshot_sha256: snapshot.snapshot_sha256,
            base_commit: snapshot.base_commit.clone(),
        })
    }

    /// Legacy unbound claiming is deliberately unavailable in schema v9.
    pub fn claim_next_feature(
        &mut self,
        _expected_queue_revision: u64,
        _now_ms: u64,
    ) -> Result<FeatureClaim, MasterError> {
        Err(MasterError::InvalidFeatureConveyorInput(
            "repository snapshot evidence is required before feature claim".to_string(),
        ))
    }

    /// Atomically maps one explicit, snapshot-bound owner dispatch onto the
    /// existing durable distributed-step lane. No repository path or bytes are
    /// accepted, stored, audited, or returned by this transition.
    pub fn dispatch_feature_coding(
        &mut self,
        request: &FeatureConveyorCodingDispatchRequest,
        now_ms: u64,
    ) -> Result<FeatureConveyorCodingDispatchReceipt, MasterError> {
        request.validate()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        require_queue_revision_tx(&tx, request.expected_queue_revision)?;
        require_emergency_pause_revision_tx(&tx, request.expected_emergency_pause_revision)?;
        require_emergency_unpaused_tx(&tx)?;

        let binding = tx
            .query_row(
                "SELECT f.current_specification_revision, f.lifecycle_revision, f.status,
                        l.lease_id, l.snapshot_id, c.snapshot_sha256
                 FROM feature_conveyor_features f
                 JOIN feature_active_lease l ON l.feature_id = f.feature_id
                 JOIN feature_repository_snapshot_claims c
                   ON c.snapshot_id = l.snapshot_id AND c.feature_id = f.feature_id
                  AND c.lease_id = l.lease_id
                 WHERE f.feature_id = ?1",
                [request.feature_id.to_string()],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, Vec<u8>>(5)?,
                    ))
                },
            )
            .optional()?;
        let Some((
            specification_revision,
            lifecycle_revision,
            status,
            lease_id,
            snapshot_id,
            snapshot_sha256,
        )) = binding
        else {
            return Err(MasterError::FeatureCodingDispatchUnavailable);
        };
        if i64_to_u64(specification_revision)? != request.specification_revision
            || i64_to_u64(lifecycle_revision)? != request.expected_lifecycle_revision
            || status != FeatureLifecycleStatus::Implementing.as_str()
            || parse_uuid(&lease_id)? != request.feature_lease_id
            || parse_uuid(&snapshot_id)? != request.snapshot_id
            || digest_array(&snapshot_sha256)? != request.snapshot_sha256
        {
            return Err(MasterError::FeatureCodingDispatchUnavailable);
        }

        let (role_json, registry_revision, capabilities_json, revoked) = tx
            .query_row(
                "SELECT role_json, registry_revision, capabilities_json, revoked
                 FROM master_devices WHERE device_id = ?1",
                [request.device_id.0.to_string()],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, i64>(3)?,
                    ))
                },
            )
            .optional()?
            .ok_or(MasterError::FeatureCodingDispatchUnavailable)?;
        let role: DeviceRole = serde_json::from_str(&role_json)?;
        let capabilities: Vec<CapabilityDescriptor> = serde_json::from_str(&capabilities_json)?;
        if revoked != 0
            || role != DeviceRole::InferenceWorker
            || i64_to_u64(registry_revision)? != request.device_registry_revision
            || capabilities != vec![CapabilityDescriptor::local_coding()]
        {
            return Err(MasterError::FeatureCodingDispatchUnavailable);
        }
        let nonterminal: i64 = tx.query_row(
            "SELECT COUNT(*) FROM master_steps WHERE status IN ('queued', 'leased')",
            [],
            |row| row.get(0),
        )?;
        if i64_to_u64(nonterminal)? >= MAX_QUEUED_OR_LEASED_STEPS {
            return Err(MasterError::QueueFull);
        }

        let task_id = TaskId::new(Uuid::new_v4());
        let step_id = StepId::new(Uuid::new_v4());
        let context = serde_json::to_value(LocalCodingJobRequest {
            feature_id: request.feature_id,
            specification_revision: request.specification_revision,
            lifecycle_revision: request.expected_lifecycle_revision,
            feature_lease_id: request.feature_lease_id,
            snapshot_id: request.snapshot_id,
            snapshot_sha256: request.snapshot_sha256,
            work_packet_sha256: request.work_packet_sha256,
            work_packet: request.work_packet.clone(),
            device_id: request.device_id,
            device_registry_revision: request.device_registry_revision,
            queue_revision: request.expected_queue_revision,
            emergency_pause_revision: request.expected_emergency_pause_revision,
        })?;
        let context_json = serde_json::to_string(&context)?;
        let context_sha256 = json_sha256(&context)?;
        tx.execute(
            "INSERT INTO master_steps
             (task_id, step_id, status, capability_id, sensitivity_json, context_json,
              context_sha256, lease_duration_ms, deadline_after_ms, created_at_ms,
              accepted_payload_json, accepted_payload_sha256, completed_at_ms)
             VALUES (?1, ?2, 'queued', ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, NULL, NULL)",
            params![
                task_id.0.to_string(),
                step_id.0.to_string(),
                LOCAL_CODING_CAPABILITY_ID,
                serde_json::to_string(&Sensitivity::Workspace)?,
                context_json,
                context_sha256.as_slice(),
                u64_to_i64(MAX_LEASE_DURATION_MS)?,
                u64_to_i64(MAX_LEASE_DURATION_MS)?,
                u64_to_i64(now_ms)?,
            ],
        )?;
        tx.execute(
            "INSERT INTO feature_coding_dispatches (
               packet_id, feature_id, specification_revision, lifecycle_revision,
               feature_lease_id, snapshot_id, snapshot_sha256, work_packet_sha256,
               work_packet_metadata_json, device_id, device_registry_revision,
               queue_revision, emergency_pause_revision, task_id, step_id, dispatched_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
            params![
                request.work_packet.packet_id.to_string(),
                request.feature_id.to_string(),
                u64_to_i64(request.specification_revision)?,
                u64_to_i64(request.expected_lifecycle_revision)?,
                request.feature_lease_id.to_string(),
                request.snapshot_id.to_string(),
                request.snapshot_sha256.as_slice(),
                request.work_packet_sha256.as_slice(),
                serde_json::to_string(&request.work_packet)?,
                request.device_id.0.to_string(),
                u64_to_i64(request.device_registry_revision)?,
                u64_to_i64(request.expected_queue_revision)?,
                u64_to_i64(request.expected_emergency_pause_revision)?,
                task_id.0.to_string(),
                step_id.0.to_string(),
                u64_to_i64(now_ms)?,
            ],
        )?;
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
        append_feature_audit_tx(
            &tx,
            "feature_coding_dispatched",
            Some(request.feature_id),
            now_ms,
            serde_json::json!({
                "lifecycle_revision": request.expected_lifecycle_revision,
                "queue_revision": request.expected_queue_revision,
                "emergency_pause_revision": request.expected_emergency_pause_revision,
                "snapshot_id_present": true,
                "snapshot_digest_present": true,
                "work_packet_digest_present": true,
                "work_packet_metadata_present": true,
                "device_binding_present": true,
                "registration_revision": request.device_registry_revision,
                "queued_step_present": true,
                "repository_material_present": false,
                "effect_possible": false,
                "side_effect_executed": false
            }),
        )?;
        tx.commit()?;
        Ok(FeatureConveyorCodingDispatchReceipt {
            schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
            feature_id: request.feature_id,
            specification_revision: request.specification_revision,
            lifecycle_revision: request.expected_lifecycle_revision,
            feature_lease_id: request.feature_lease_id,
            snapshot_id: request.snapshot_id,
            snapshot_sha256: request.snapshot_sha256,
            work_packet_sha256: request.work_packet_sha256,
            packet_id: request.work_packet.packet_id,
            device_id: request.device_id,
            device_registry_revision: request.device_registry_revision,
            queue_revision: request.expected_queue_revision,
            emergency_pause_revision: request.expected_emergency_pause_revision,
            task_id,
            step_id,
            status: FeatureConveyorCodingDispatchStatus::Queued,
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
        require_feature_coding_work_terminal_tx(&tx, feature_id)?;
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
        expected_queue_revision: u64,
        expected_emergency_pause_revision: u64,
        now_ms: u64,
    ) -> Result<FeatureSnapshot, MasterError> {
        self.cancel_active_feature_bound(
            FeatureOwnerResolutionBinding {
                feature_id,
                expected_lifecycle_revision,
                expected_queue_revision,
                expected_emergency_pause_revision,
            },
            now_ms,
            None,
        )
    }

    pub fn cancel_active_feature_from_owner_bridge(
        &mut self,
        request: &FeatureConveyorRemoteCancelActiveFeatureRequest,
        registration: &DeviceRegistration,
        now_ms: u64,
    ) -> Result<FeatureSnapshot, MasterError> {
        self.cancel_active_feature_bound(
            FeatureOwnerResolutionBinding {
                feature_id: request.feature_id,
                expected_lifecycle_revision: request.expected_lifecycle_revision,
                expected_queue_revision: request.expected_queue_revision,
                expected_emergency_pause_revision: request.expected_emergency_pause_revision,
            },
            now_ms,
            Some(OwnerControlBridgeBinding {
                registration,
                expected_designation_revision: request.expected_owner_control_designation_revision,
            }),
        )
    }

    fn cancel_active_feature_bound(
        &mut self,
        binding: FeatureOwnerResolutionBinding,
        now_ms: u64,
        owner_binding: Option<OwnerControlBridgeBinding<'_>>,
    ) -> Result<FeatureSnapshot, MasterError> {
        let FeatureOwnerResolutionBinding {
            feature_id,
            expected_lifecycle_revision,
            expected_queue_revision,
            expected_emergency_pause_revision,
        } = binding;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(owner) = owner_binding {
            require_owner_control_bridge_tx(
                &tx,
                owner.registration,
                owner.expected_designation_revision,
            )?;
        }
        require_queue_revision_tx(&tx, expected_queue_revision)?;
        // Owner cancellation remains available during Emergency Pause, but it
        // must be bound to the exact pause epoch observed by the owner.
        require_emergency_pause_revision_tx(&tx, expected_emergency_pause_revision)?;
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
        tx.execute(
            "INSERT INTO feature_transition_evidence (
               feature_id, lifecycle_revision, from_status, to_status, recorded_at_ms
             ) VALUES (?1, ?2, ?3, 'cancelled', ?4)",
            params![
                feature_id.to_string(),
                u64_to_i64(revision + 1)?,
                status.as_str(),
                u64_to_i64(now_ms)?,
            ],
        )?;
        cancel_feature_coding_dispatches_tx(&tx, feature_id, now_ms)?;
        append_feature_audit_tx(
            &tx,
            "feature_cancelled",
            Some(feature_id),
            now_ms,
            serde_json::json!({
                "from_status": status.as_str(),
                "to_status": "cancelled",
                "lifecycle_revision": revision + 1,
                "queue_revision": expected_queue_revision,
                "emergency_pause_revision": expected_emergency_pause_revision,
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
        expected_emergency_pause_revision: u64,
        evidence: FeatureAbandonmentEvidence,
        now_ms: u64,
    ) -> Result<FeatureSnapshot, MasterError> {
        self.abandon_and_advance_bound(
            FeatureOwnerResolutionBinding {
                feature_id,
                expected_lifecycle_revision,
                expected_queue_revision,
                expected_emergency_pause_revision,
            },
            evidence,
            now_ms,
            None,
        )
    }

    pub fn abandon_and_advance_from_owner_bridge(
        &mut self,
        request: &FeatureConveyorRemoteAbandonAndAdvanceRequest,
        registration: &DeviceRegistration,
        now_ms: u64,
    ) -> Result<FeatureSnapshot, MasterError> {
        self.abandon_and_advance_bound(
            FeatureOwnerResolutionBinding {
                feature_id: request.feature_id,
                expected_lifecycle_revision: request.expected_lifecycle_revision,
                expected_queue_revision: request.expected_queue_revision,
                expected_emergency_pause_revision: request.expected_emergency_pause_revision,
            },
            FeatureAbandonmentEvidence {
                safe_reconciliation_sha256: request.evidence.safe_reconciliation_sha256,
                merged: request.evidence.merged,
                verified_healthy_main_sha256: request.evidence.verified_healthy_main_sha256,
            },
            now_ms,
            Some(OwnerControlBridgeBinding {
                registration,
                expected_designation_revision: request.expected_owner_control_designation_revision,
            }),
        )
    }

    fn abandon_and_advance_bound(
        &mut self,
        binding: FeatureOwnerResolutionBinding,
        evidence: FeatureAbandonmentEvidence,
        now_ms: u64,
        owner_binding: Option<OwnerControlBridgeBinding<'_>>,
    ) -> Result<FeatureSnapshot, MasterError> {
        let FeatureOwnerResolutionBinding {
            feature_id,
            expected_lifecycle_revision,
            expected_queue_revision,
            expected_emergency_pause_revision,
        } = binding;
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
        if let Some(owner) = owner_binding {
            require_owner_control_bridge_tx(
                &tx,
                owner.registration,
                owner.expected_designation_revision,
            )?;
        }
        require_queue_revision_tx(&tx, expected_queue_revision)?;
        // Resolution is permitted while paused only against the exact pause
        // revision; it does not resume work or create a new lease.
        require_emergency_pause_revision_tx(&tx, expected_emergency_pause_revision)?;
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
            FeatureLifecycleStatus::Cancelled
                | FeatureLifecycleStatus::Quarantined
                | FeatureLifecycleStatus::AttentionRequired
                | FeatureLifecycleStatus::Failed
        ) {
            return Err(MasterError::InvalidFeatureTransition);
        }
        let resolution_origin = feature_resolution_origin_tx(&tx, feature_id, status, revision)?;
        let publication_merge_possible = publication_merge_effect_possible_tx(&tx, feature_id)?;
        let merged_main_reconciliation_required = resolution_origin
            == FeatureLifecycleStatus::VerifyingMain
            || publication_merge_possible;
        if merged_main_reconciliation_required
            && (!evidence.merged
                || evidence
                    .verified_healthy_main_sha256
                    .filter(|digest| *digest != [0; 32])
                    .is_none())
        {
            return Err(MasterError::VerifiedHealthyMainRequired);
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
                "emergency_pause_revision": expected_emergency_pause_revision,
                "safe_reconciliation_digest_present": true,
                "merged": evidence.merged,
                "merged_required_by_durable_transition":
                    resolution_origin == FeatureLifecycleStatus::VerifyingMain,
                "merged_required_by_publication_intent": publication_merge_possible,
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

    /// Authorizes one read-only bundle chunk only while the exact local-coding
    /// attempt remains leased and every Feature Conveyor binding is current.
    /// This creates no reusable repository or worker authority.
    pub fn authorize_local_coding_snapshot_chunk(
        &self,
        authenticated_device_id: DeviceId,
        request: &LocalCodingSnapshotChunkRequest,
        now_ms: u64,
    ) -> Result<(), MasterError> {
        request.validate()?;
        let tx = self.connection.unchecked_transaction()?;
        require_emergency_unpaused_tx(&tx)?;
        let attempt = load_attempt(&tx, request.attempt_id)?
            .ok_or(MasterError::FeatureCodingDispatchUnavailable)?;
        if attempt.device_id != authenticated_device_id
            || attempt.status != AttemptStatus::Leased
            || attempt.lease_expires_at_ms <= now_ms
        {
            return Err(MasterError::FeatureCodingDispatchUnavailable);
        }
        let job: JobEnvelope = serde_json::from_str(&attempt.job_json)?;
        request.validate_for_job(&job)?;
        let step_status: String = tx
            .query_row(
                "SELECT status FROM master_steps WHERE step_id = ?1 AND task_id = ?2",
                params![job.step_id.0.to_string(), job.task_id.0.to_string()],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(MasterError::FeatureCodingDispatchUnavailable)?;
        if StepStatus::parse(&step_status)? != StepStatus::Leased {
            return Err(MasterError::FeatureCodingDispatchUnavailable);
        }
        let context = job.validate_local_coding()?;
        if !coding_job_binding_is_current_tx(&tx, &context, job.step_id, attempt.device_id)? {
            return Err(MasterError::FeatureCodingDispatchUnavailable);
        }
        Ok(())
    }

    /// Read-only phase of result-artifact admission. Callers perform bounded
    /// filesystem work only after this check, without retaining the SQLite
    /// transaction, then call `finalize_local_coding_result_artifact`.
    pub fn authorize_local_coding_result_artifact(
        &self,
        authenticated_device_id: DeviceId,
        admission: &LocalCodingResultArtifactAdmission,
        now_ms: u64,
    ) -> Result<bool, MasterError> {
        admission.validate()?;
        let tx = self.connection.unchecked_transaction()?;
        validate_result_artifact_admission_tx(&tx, authenticated_device_id, admission, now_ms)?;
        let already_admitted: bool = tx.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM feature_result_artifacts WHERE artifact_id = ?1
             )",
            [admission.artifact.artifact_id.to_string()],
            |row| row.get(0),
        )?;
        Ok(already_admitted)
    }

    /// Immediate-transaction phase. Every authority binding is rechecked and
    /// immutable metadata plus redacted audit commit atomically. Exact retry
    /// returns the original binding without adding a second row or audit event.
    pub fn finalize_local_coding_result_artifact(
        &mut self,
        authenticated_device_id: DeviceId,
        admission: &LocalCodingResultArtifactAdmission,
        now_ms: u64,
    ) -> Result<LocalCodingResultArtifactReceipt, MasterError> {
        admission.validate()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let job =
            validate_result_artifact_admission_tx(&tx, authenticated_device_id, admission, now_ms)?;
        let context = job.validate_local_coding()?;
        let existing: i64 = tx.query_row(
            "SELECT COUNT(*) FROM feature_result_artifacts
             WHERE artifact_id = ?1 AND artifact_sha256 = ?2
               AND artifact_size_bytes = ?3 AND device_id = ?4
               AND device_registry_revision = ?5 AND connection_epoch = ?6
               AND sequence = ?7 AND task_id = ?8 AND step_id = ?9
               AND attempt_id = ?10 AND lease_id = ?11 AND cancellation_id = ?12
               AND context_sha256 = ?13 AND feature_id = ?14
               AND feature_lease_id = ?15 AND snapshot_id = ?16
               AND snapshot_sha256 = ?17 AND work_packet_sha256 = ?18
               AND workspace_retained = 1 AND workspace_expires_at_ms = ?19",
            params![
                admission.artifact.artifact_id.to_string(),
                admission.artifact.artifact_sha256.as_slice(),
                u64_to_i64(admission.artifact.artifact_size_bytes)?,
                authenticated_device_id.0.to_string(),
                u64_to_i64(context.device_registry_revision)?,
                u64_to_i64(admission.connection_epoch)?,
                u64_to_i64(admission.sequence)?,
                admission.task_id.0.to_string(),
                admission.step_id.0.to_string(),
                admission.attempt_id.0.to_string(),
                admission.lease_id.0.to_string(),
                admission.cancellation_id.0.to_string(),
                admission.context_sha256.as_slice(),
                admission.feature_id.to_string(),
                admission.feature_lease_id.to_string(),
                admission.snapshot_id.to_string(),
                admission.snapshot_sha256.as_slice(),
                admission.work_packet_sha256.as_slice(),
                u64_to_i64(admission.workspace_expires_at_ms)?,
            ],
            |row| row.get(0),
        )?;
        if existing == 0 {
            let collision: i64 = tx.query_row(
                "SELECT COUNT(*) FROM feature_result_artifacts
                 WHERE artifact_id = ?1 OR attempt_id = ?2",
                params![
                    admission.artifact.artifact_id.to_string(),
                    admission.attempt_id.0.to_string()
                ],
                |row| row.get(0),
            )?;
            if collision != 0 {
                return Err(MasterError::ResultArtifactUnavailable);
            }
            tx.execute(
                "INSERT INTO feature_result_artifacts (
                   artifact_id, artifact_sha256, artifact_size_bytes, device_id,
                   device_registry_revision, connection_epoch, sequence, task_id,
                   step_id, attempt_id, lease_id, cancellation_id, context_sha256,
                   feature_id, feature_lease_id, snapshot_id, snapshot_sha256,
                   work_packet_sha256, admitted_at_ms, workspace_retained,
                   workspace_expires_at_ms
                 ) VALUES (
                   ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                   ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, 1, ?20
                 )",
                params![
                    admission.artifact.artifact_id.to_string(),
                    admission.artifact.artifact_sha256.as_slice(),
                    u64_to_i64(admission.artifact.artifact_size_bytes)?,
                    authenticated_device_id.0.to_string(),
                    u64_to_i64(context.device_registry_revision)?,
                    u64_to_i64(admission.connection_epoch)?,
                    u64_to_i64(admission.sequence)?,
                    admission.task_id.0.to_string(),
                    admission.step_id.0.to_string(),
                    admission.attempt_id.0.to_string(),
                    admission.lease_id.0.to_string(),
                    admission.cancellation_id.0.to_string(),
                    admission.context_sha256.as_slice(),
                    admission.feature_id.to_string(),
                    admission.feature_lease_id.to_string(),
                    admission.snapshot_id.to_string(),
                    admission.snapshot_sha256.as_slice(),
                    admission.work_packet_sha256.as_slice(),
                    u64_to_i64(now_ms)?,
                    u64_to_i64(admission.workspace_expires_at_ms)?,
                ],
            )?;
            append_feature_audit_tx(
                &tx,
                "result_artifact_admitted",
                Some(admission.feature_id),
                now_ms,
                serde_json::json!({
                    "artifact_id": admission.artifact.artifact_id,
                    "artifact_sha256": lower_hex(&admission.artifact.artifact_sha256),
                    "artifact_size_bytes": admission.artifact.artifact_size_bytes,
                    "attempt_id": admission.attempt_id,
                    "step_id": admission.step_id
                    ,"workspace_retained": true,
                    "workspace_expiry_present": true
                }),
            )?;
        }
        tx.commit()?;
        Ok(LocalCodingResultArtifactReceipt {
            protocol_version: admission.protocol_version,
            connection_epoch: admission.connection_epoch,
            sequence: admission.sequence,
            task_id: admission.task_id,
            step_id: admission.step_id,
            attempt_id: admission.attempt_id,
            lease_id: admission.lease_id,
            cancellation_id: admission.cancellation_id,
            artifact_id: admission.artifact.artifact_id,
            artifact_sha256: admission.artifact.artifact_sha256,
            artifact_size_bytes: admission.artifact.artifact_size_bytes,
            workspace_retained: admission.workspace_retained,
            workspace_expires_at_ms: admission.workspace_expires_at_ms,
            status: assemblywright_protocol::LOCAL_CODING_RESULT_ARTIFACT_STATUS.to_string(),
        })
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
        let mut selected = None;
        for step in queued {
            let Some(capability) = capabilities
                .iter()
                .find(|candidate| {
                    candidate.id == step.capability_id
                        && step.context_json.len() <= candidate.max_context_bytes as usize
                })
                .cloned()
            else {
                continue;
            };
            if capability.id == LOCAL_CODING_CAPABILITY_ID
                && (!matches!(remote_contract, Some(RemoteWorkContract::LocalCoding))
                    || !coding_step_binding_is_current_tx(&tx, &step, device_id)?)
            {
                continue;
            }
            selected = Some((step, capability));
            break;
        }
        let (step, capability) = selected.ok_or(MasterError::NoEligibleStep)?;

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
        let context_handling = if capability.id == LOCAL_CODING_CAPABILITY_ID {
            ContextHandlingPolicy::SealedUntilResolvedOrExpired
        } else {
            ContextHandlingPolicy::EphemeralNoRetention
        };
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
            context_handling,
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
        self.accept_result_bound(None, result, now_ms, None, None)
    }

    pub fn accept_result_from(
        &mut self,
        authenticated_device_id: DeviceId,
        result: &JobResultEnvelope,
        now_ms: u64,
    ) -> Result<AcceptedResult, MasterError> {
        self.accept_result_bound(Some(authenticated_device_id), result, now_ms, None, None)
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
            None,
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
            None,
        )
    }

    pub fn accept_remote_result_from_with_artifact(
        &mut self,
        authenticated_device_id: DeviceId,
        result: &JobResultEnvelope,
        now_ms: u64,
        contract: &RemoteWorkContract,
        store: &ResultArtifactStore,
        artifact: &mut VerifiedResultArtifact,
    ) -> Result<AcceptedResult, MasterError> {
        artifact
            .revalidate(store)
            .map_err(|_| MasterError::ResultArtifactUnavailable)?;
        self.accept_result_bound(
            Some(authenticated_device_id),
            result,
            now_ms,
            Some(contract),
            Some(artifact.reference()),
        )
    }

    fn accept_result_bound(
        &mut self,
        authenticated_device_id: Option<DeviceId>,
        result: &JobResultEnvelope,
        now_ms: u64,
        remote_contract: Option<&RemoteWorkContract>,
        artifact_evidence: Option<ResultArtifactReference>,
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
        if job.capability_id == LOCAL_CODING_CAPABILITY_ID
            && !matches!(remote_contract, Some(RemoteWorkContract::LocalCoding))
        {
            return Err(MasterError::InvalidRemoteWorkContract);
        }
        if let Some(contract) = remote_contract {
            contract.validate_result(result, &job)?;
            let registered = capability_for_device(&tx, attempt.device_id, &job.capability_id)?;
            if registered != contract.capability() {
                return Err(MasterError::InvalidRemoteWorkContract);
            }
        } else {
            result.validate_for_job(&job)?;
        }
        if job.capability_id == LOCAL_CODING_CAPABILITY_ID {
            let context = job.validate_local_coding()?;
            if !coding_job_binding_is_current_tx(&tx, &context, job.step_id, attempt.device_id)? {
                return Err(MasterError::FeatureCodingDispatchUnavailable);
            }
            let payload: LocalCodingJobResult = serde_json::from_value(result.payload.clone())?;
            let evidence = artifact_evidence.ok_or(MasterError::ResultArtifactUnavailable)?;
            if evidence
                != (ResultArtifactReference {
                    artifact_id: payload.artifact_id,
                    artifact_sha256: payload.artifact_sha256,
                    artifact_size_bytes: payload.artifact_size_bytes,
                })
            {
                return Err(MasterError::ResultArtifactUnavailable);
            }
            let artifact_matches: bool = tx.query_row(
                "SELECT EXISTS(
                   SELECT 1 FROM feature_result_artifacts
                   WHERE artifact_id = ?1 AND artifact_sha256 = ?2
                     AND artifact_size_bytes = ?3 AND device_id = ?4
                     AND connection_epoch = ?5 AND sequence = ?6
                     AND task_id = ?7 AND step_id = ?8 AND attempt_id = ?9
                     AND lease_id = ?10 AND cancellation_id = ?11
                     AND context_sha256 = ?12 AND feature_id = ?13
                     AND feature_lease_id = ?14 AND snapshot_id = ?15
                     AND snapshot_sha256 = ?16 AND work_packet_sha256 = ?17
                     AND workspace_retained = ?18 AND workspace_expires_at_ms = ?19
                 )",
                params![
                    payload.artifact_id.to_string(),
                    payload.artifact_sha256.as_slice(),
                    u64_to_i64(payload.artifact_size_bytes)?,
                    attempt.device_id.0.to_string(),
                    u64_to_i64(result.connection_epoch)?,
                    u64_to_i64(result.sequence)?,
                    result.task_id.0.to_string(),
                    result.step_id.0.to_string(),
                    result.attempt_id.0.to_string(),
                    result.lease_id.0.to_string(),
                    result.cancellation_id.0.to_string(),
                    result.context_sha256.as_slice(),
                    context.feature_id.to_string(),
                    context.feature_lease_id.to_string(),
                    context.snapshot_id.to_string(),
                    context.snapshot_sha256.as_slice(),
                    context.work_packet_sha256.as_slice(),
                    payload.workspace_retained,
                    u64_to_i64(payload.workspace_expires_at_ms)?,
                ],
                |row| row.get(0),
            )?;
            if !artifact_matches || payload.patch_sha256 != payload.artifact_sha256 {
                return Err(MasterError::ResultArtifactUnavailable);
            }
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

    /// Returns the independently provisioned planning-only catalog. Caller-carried
    /// catalogs are validated against this value before any durable admission.
    pub fn assembly_line_orchestrator_catalog(&self) -> OrchestratorCatalog {
        OrchestratorCatalog::default()
    }

    /// Pure durable projection. Execution components remain deliberately
    /// unavailable to product routes even when source-only capability bindings
    /// exist; this method never synthesizes an execution epoch.
    pub fn assembly_line_owner_projection(
        &self,
        observed_at_ms: u64,
    ) -> Result<AssemblyLineOwnerProjection, MasterError> {
        self.assembly_line_owner_projection_with_planning_runtime(observed_at_ms, None)
    }

    /// Adds only the two independently loaded planning components to the
    /// projection. Execution and broker components remain unavailable.
    pub fn assembly_line_owner_projection_with_planning_runtime(
        &self,
        observed_at_ms: u64,
        planning: Option<PlanningRuntimeStatus>,
    ) -> Result<AssemblyLineOwnerProjection, MasterError> {
        self.assembly_line_owner_projection_with_runtime(observed_at_ms, planning, None)
    }

    /// Projects only independently attested runtime boundaries. Durable source
    /// capability rows cannot make execution appear available without a live
    /// dispatcher status supplied by the hosting process.
    pub fn assembly_line_owner_projection_with_runtime(
        &self,
        observed_at_ms: u64,
        planning: Option<PlanningRuntimeStatus>,
        execution: Option<AssemblyLineExecutionRuntimeStatus>,
    ) -> Result<AssemblyLineOwnerProjection, MasterError> {
        if let Some(execution) = execution {
            execution.validate()?;
        }
        let capability_healthy = self
            .connection
            .query_row(
                "SELECT healthy FROM assembly_line_execution_capabilities
                 ORDER BY binding_revision DESC LIMIT 1",
                [],
                |row| row.get::<_, i64>(0),
            )
            .optional()?;
        if capability_healthy.is_some_and(|healthy| !matches!(healthy, 0 | 1)) {
            return Err(MasterError::InvalidStoredState(
                "assembly-line execution capability health is malformed".to_string(),
            ));
        }
        let execution = execution.filter(|_| capability_healthy == Some(1));
        let owner_control_revision: i64 = self.connection.query_row(
            "SELECT owner_control_revision FROM assembly_line_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        let mut repositories = Vec::new();
        let mut statement = self.connection.prepare(
            "SELECT repository_id,git_url,repository_revision,lifecycle_revision,visibility,
                    approved_specification_id,approved_specification_revision,
                    approved_specification_sha256,owner_approval_sha256,lifecycle,
                    effect_possible,creation_evidence_sha256
             FROM assembly_line_repositories ORDER BY git_url ASC",
        )?;
        let rows = statement.query_map([], repository_projection_row)?;
        for row in rows {
            repositories.push(row?);
        }
        drop(statement);

        let mut queue = Vec::new();
        let mut statement = self.connection.prepare(
            "SELECT feature_id,repository_id,specification_id,specification_revision,
                    specification_sha256,owner_approval_sha256,queue_position,
                    lifecycle_revision,lifecycle
             FROM assembly_line_queue ORDER BY queue_position ASC",
        )?;
        let rows = statement.query_map([], queue_projection_row)?;
        for row in rows {
            queue.push(row?);
        }
        drop(statement);

        let (emergency_paused, emergency_pause_revision) = self.emergency_pause_snapshot()?;
        let component = |name: &str| RuntimeComponentAvailability {
            binding_revision: 1,
            binding_sha256: Sha256::digest(
                [
                    b"assemblywright.schema-v20.inert-availability.v1\0".as_slice(),
                    name.as_bytes(),
                ]
                .concat(),
            )
            .into(),
            status: RuntimeAvailabilityStatus::Unavailable,
            unavailable_reason: Some(RuntimeUnavailableReason::NotConfigured),
        };
        let planning_component = |name: &str, sha256: [u8; 32]| RuntimeComponentAvailability {
            binding_revision: planning.map_or(1, |runtime| runtime.binding_revision),
            binding_sha256: planning.map_or_else(
                || component(name).binding_sha256,
                |runtime| {
                    Sha256::digest(
                        [
                            b"assemblywright.schema-v20.planning-availability.v1\0".as_slice(),
                            sha256.as_slice(),
                            runtime.catalog_sha256.as_slice(),
                        ]
                        .concat(),
                    )
                    .into()
                },
            ),
            status: if planning.is_some() {
                RuntimeAvailabilityStatus::Available
            } else {
                RuntimeAvailabilityStatus::Unavailable
            },
            unavailable_reason: planning
                .is_none()
                .then_some(RuntimeUnavailableReason::NotConfigured),
        };
        let execution_component = |name: &str| RuntimeComponentAvailability {
            binding_revision: execution.map_or(1, |runtime| runtime.binding_revision),
            binding_sha256: execution.map_or_else(
                || component(name).binding_sha256,
                |runtime| {
                    Sha256::digest(
                        [
                            b"assemblywright.schema-v21.execution-availability.v1\0".as_slice(),
                            name.as_bytes(),
                            runtime.dispatcher_sha256.as_slice(),
                        ]
                        .concat(),
                    )
                    .into()
                },
            ),
            status: if execution.is_some() {
                RuntimeAvailabilityStatus::Available
            } else {
                RuntimeAvailabilityStatus::Unavailable
            },
            unavailable_reason: execution
                .is_none()
                .then_some(RuntimeUnavailableReason::NotConfigured),
        };
        let projection = AssemblyLineOwnerProjection {
            schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
            owner_control_revision: i64_to_u64(owner_control_revision)?,
            emergency_pause_revision,
            emergency_paused,
            orchestrator_catalog: self.assembly_line_orchestrator_catalog(),
            repositories,
            queue: queue.clone(),
            assembly_line: assembly_line_state_connection(&self.connection)?,
            availability: AssemblyLineRuntimeAvailabilityProjection {
                schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
                availability_revision: 1,
                observed_at_ms,
                brainstorming_provider: planning_component(
                    "brainstorming_provider",
                    planning.map_or([0; 32], |runtime| runtime.brainstorming_sha256),
                ),
                github_creation: planning_component(
                    "github_creation",
                    planning.map_or([0; 32], |runtime| runtime.github_sha256),
                ),
                windows_executor: execution_component("windows_executor"),
                mac_executor: execution_component("mac_executor"),
                protected_brokers: execution_component("protected_brokers"),
            },
        };
        projection.validate()?;
        Ok(projection)
    }

    pub fn record_assembly_line_project_draft(
        &mut self,
        draft: &ProjectBrainstormingDraft,
        now_ms: u64,
    ) -> Result<(), MasterError> {
        let catalog = self.assembly_line_orchestrator_catalog();
        draft.validate_against_authoritative_catalog(&catalog)?;
        let request_sha256 = draft.canonical_sha256()?;
        let canonical_json = canonical_json(&serde_json::to_value(draft)?)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if assembly_line_request_replay_tx(&tx, "project_draft", draft.draft_id, request_sha256)? {
            return Ok(());
        }
        let registered: i64 = tx.query_row(
            "SELECT COUNT(*) FROM assembly_line_repositories WHERE repository_id=?1 OR git_url=?2",
            params![
                draft.repository.repository_id.to_string(),
                draft.repository.git_url.url
            ],
            |row| row.get(0),
        )?;
        if registered != 0 {
            return Err(MasterError::AssemblyLineRepositoryUnavailable);
        }
        insert_assembly_line_request_tx(
            &tx,
            "project_draft",
            draft.draft_id,
            request_sha256,
            None,
            now_ms,
        )?;
        tx.execute(
            "INSERT INTO assembly_line_project_drafts
             (draft_id,draft_revision,repository_id,git_url,visibility,request_sha256,canonical_json,recorded_at_ms)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![draft.draft_id.to_string(), u64_to_i64(draft.draft_revision)?,
                draft.repository.repository_id.to_string(), draft.repository.git_url.url,
                project_visibility_str(draft.visibility), request_sha256.as_slice(), canonical_json,
                u64_to_i64(now_ms)?],
        )?;
        advance_assembly_line_owner_revision_tx(&tx)?;
        append_assembly_line_audit_tx(
            &tx,
            "project_draft_recorded",
            now_ms,
            serde_json::json!({
                "draft_id": draft.draft_id, "draft_revision": draft.draft_revision,
                "request_sha256": request_sha256, "planning_only": true, "external_effect": false
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn record_assembly_line_feature_draft(
        &mut self,
        draft: &FeatureBrainstormingDraft,
        now_ms: u64,
    ) -> Result<(), MasterError> {
        let catalog = self.assembly_line_orchestrator_catalog();
        draft.validate_against_authoritative_catalog(&catalog)?;
        let request_sha256 = draft.canonical_sha256()?;
        let canonical_json = canonical_json(&serde_json::to_value(draft)?)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if assembly_line_request_replay_tx(&tx, "feature_draft", draft.draft_id, request_sha256)? {
            return Ok(());
        }
        require_created_assembly_line_repository_tx(
            &tx,
            &draft.repository,
            draft.expected_repository_revision,
        )?;
        insert_assembly_line_request_tx(
            &tx,
            "feature_draft",
            draft.draft_id,
            request_sha256,
            None,
            now_ms,
        )?;
        tx.execute(
            "INSERT INTO assembly_line_feature_drafts
             (draft_id,draft_revision,repository_id,expected_repository_revision,request_sha256,canonical_json,recorded_at_ms)
             VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![draft.draft_id.to_string(), u64_to_i64(draft.draft_revision)?,
                draft.repository.repository_id.to_string(), u64_to_i64(draft.expected_repository_revision)?,
                request_sha256.as_slice(), canonical_json, u64_to_i64(now_ms)?],
        )?;
        advance_assembly_line_owner_revision_tx(&tx)?;
        append_assembly_line_audit_tx(
            &tx,
            "feature_draft_recorded",
            now_ms,
            serde_json::json!({
                "draft_id": draft.draft_id, "draft_revision": draft.draft_revision,
                "request_sha256": request_sha256, "planning_only": true, "external_effect": false
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn record_assembly_line_frozen_specification(
        &mut self,
        frozen: &FrozenBrainstormingSpecification,
        now_ms: u64,
    ) -> Result<(), MasterError> {
        self.record_assembly_line_frozen_specification_inner(frozen, None, now_ms)
    }

    pub(crate) fn record_assembly_line_frozen_specification_with_provider(
        &mut self,
        frozen: &FrozenBrainstormingSpecification,
        binding: &crate::planning_effects::BrainstormingAdapterBinding,
        adapter_catalog_sha256: [u8; 32],
        now_ms: u64,
    ) -> Result<(), MasterError> {
        self.record_assembly_line_frozen_specification_inner(
            frozen,
            Some((binding, adapter_catalog_sha256)),
            now_ms,
        )
    }

    fn record_assembly_line_frozen_specification_inner(
        &mut self,
        frozen: &FrozenBrainstormingSpecification,
        provider: Option<(
            &crate::planning_effects::BrainstormingAdapterBinding,
            [u8; 32],
        )>,
        now_ms: u64,
    ) -> Result<(), MasterError> {
        frozen.validate()?;
        let request_json = serde_json::to_value(frozen)?;
        let frozen_canonical_json = canonical_json(&request_json)?;
        let request_sha256: [u8; 32] = Sha256::digest(frozen_canonical_json.as_bytes()).into();
        let catalog = self.assembly_line_orchestrator_catalog();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if assembly_line_request_replay_tx(
            &tx,
            "frozen_specification",
            frozen.specification_id,
            request_sha256,
        )? {
            if let Some((binding, adapter_catalog_sha256)) = provider {
                let expected = canonical_json(&serde_json::json!({
                    "target_kind": brainstorming_target_str(frozen.target_kind),
                    "draft_id": frozen.draft_id,
                    "specification_id": frozen.specification_id,
                    "specification_sha256": frozen.specification_sha256,
                    "provider_id": binding.profile.provider_id,
                    "model_id": binding.profile.model_id,
                    "adapter_sha256": binding.executable_sha256,
                    "adapter_catalog_sha256": adapter_catalog_sha256,
                    "planning_only": true,
                    "provider_output_retained_in_audit": false,
                    "external_effect_authorized": false
                }))?;
                let count: i64 = tx.query_row(
                    "SELECT COUNT(*) FROM assembly_line_audit
                     WHERE event_kind='brainstorming_provider_output_accepted'
                       AND redacted_metadata_json=?1",
                    [expected],
                    |row| row.get(0),
                )?;
                if count != 1 {
                    return Err(MasterError::AssemblyLineBrainstormingUnavailable);
                }
            }
            return Ok(());
        }
        match frozen.target_kind {
            BrainstormingTargetKind::Project => {
                let json: String = tx
                    .query_row(
                        "SELECT canonical_json FROM assembly_line_project_drafts WHERE draft_id=?1",
                        [frozen.draft_id.to_string()],
                        |row| row.get(0),
                    )
                    .optional()?
                    .ok_or(MasterError::AssemblyLinePlanningImmutable)?;
                let draft: ProjectBrainstormingDraft = serde_json::from_str(&json)?;
                frozen.validate_for_project_draft(&draft, &catalog)?;
            }
            BrainstormingTargetKind::Feature => {
                let json: String = tx
                    .query_row(
                        "SELECT canonical_json FROM assembly_line_feature_drafts WHERE draft_id=?1",
                        [frozen.draft_id.to_string()],
                        |row| row.get(0),
                    )
                    .optional()?
                    .ok_or(MasterError::AssemblyLinePlanningImmutable)?;
                let draft: FeatureBrainstormingDraft = serde_json::from_str(&json)?;
                frozen.validate_for_feature_draft(&draft, &catalog)?;
                require_created_assembly_line_repository_tx(
                    &tx,
                    &draft.repository,
                    draft.expected_repository_revision,
                )?;
            }
        }
        insert_assembly_line_request_tx(
            &tx,
            "frozen_specification",
            frozen.specification_id,
            request_sha256,
            None,
            now_ms,
        )?;
        tx.execute(
            "INSERT INTO assembly_line_frozen_specifications
             (specification_id,specification_revision,target_kind,draft_id,repository_id,
              specification_sha256,request_sha256,canonical_json,recorded_at_ms)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                frozen.specification_id.to_string(),
                u64_to_i64(frozen.specification_revision)?,
                brainstorming_target_str(frozen.target_kind),
                frozen.draft_id.to_string(),
                frozen.repository.repository_id.to_string(),
                frozen.specification_sha256.as_slice(),
                request_sha256.as_slice(),
                frozen_canonical_json,
                u64_to_i64(now_ms)?
            ],
        )?;
        advance_assembly_line_owner_revision_tx(&tx)?;
        append_assembly_line_audit_tx(
            &tx,
            "frozen_specification_recorded",
            now_ms,
            serde_json::json!({
                "specification_id": frozen.specification_id,
                "specification_revision": frozen.specification_revision,
                "specification_sha256": frozen.specification_sha256,
                "target_kind": brainstorming_target_str(frozen.target_kind),
                "provider_output_retained_in_audit": false, "external_effect": false
            }),
        )?;
        if let Some((binding, adapter_catalog_sha256)) = provider {
            append_assembly_line_audit_tx(
                &tx,
                "brainstorming_provider_output_accepted",
                now_ms,
                serde_json::json!({
                    "target_kind": brainstorming_target_str(frozen.target_kind),
                    "draft_id": frozen.draft_id,
                    "specification_id": frozen.specification_id,
                    "specification_sha256": frozen.specification_sha256,
                    "provider_id": binding.profile.provider_id,
                    "model_id": binding.profile.model_id,
                    "adapter_sha256": binding.executable_sha256,
                    "adapter_catalog_sha256": adapter_catalog_sha256,
                    "planning_only": true,
                    "provider_output_retained_in_audit": false,
                    "external_effect_authorized": false
                }),
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn approve_assembly_line_project(
        &mut self,
        approval: &BrainstormingOwnerApprovalBinding,
        now_ms: u64,
    ) -> Result<RepositoryCreationProjection, MasterError> {
        let request_json = serde_json::to_value(approval)?;
        let canonical_json = canonical_json(&request_json)?;
        let request_sha256: [u8; 32] = Sha256::digest(canonical_json.as_bytes()).into();
        let catalog = self.assembly_line_orchestrator_catalog();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if assembly_line_request_replay_tx(
            &tx,
            "project_approval",
            approval.approval_id,
            request_sha256,
        )? {
            return assembly_line_repository_projection_tx(&tx, approval.repository.repository_id);
        }
        let owner_revision = assembly_line_owner_revision_tx(&tx)?;
        if approval.owner_control_revision != owner_revision {
            return Err(MasterError::StaleAssemblyLineOwnerControlRevision {
                expected: approval.owner_control_revision,
                found: owner_revision,
            });
        }
        let draft_json: String = tx
            .query_row(
                "SELECT canonical_json FROM assembly_line_project_drafts WHERE draft_id=?1",
                [approval.draft_id.to_string()],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(MasterError::AssemblyLinePlanningImmutable)?;
        let frozen_json: String = tx.query_row(
            "SELECT canonical_json FROM assembly_line_frozen_specifications WHERE specification_id=?1 AND target_kind='project'",
            [approval.specification_id.to_string()], |row| row.get(0),
        ).optional()?.ok_or(MasterError::AssemblyLinePlanningImmutable)?;
        let draft: ProjectBrainstormingDraft = serde_json::from_str(&draft_json)?;
        let frozen: FrozenBrainstormingSpecification = serde_json::from_str(&frozen_json)?;
        approval.validate_for_project(&draft, &frozen, &catalog)?;
        let conflict: i64 = tx.query_row(
            "SELECT COUNT(*) FROM assembly_line_repositories WHERE repository_id=?1 OR git_url=?2",
            params![
                approval.repository.repository_id.to_string(),
                approval.repository.git_url.url
            ],
            |row| row.get(0),
        )?;
        if conflict != 0 {
            return Err(MasterError::AssemblyLinePlanningImmutable);
        }
        insert_assembly_line_request_tx(
            &tx,
            "project_approval",
            approval.approval_id,
            request_sha256,
            None,
            now_ms,
        )?;
        tx.execute(
            "INSERT INTO assembly_line_owner_approvals
             (approval_id,target_kind,specification_id,repository_id,owner_control_revision,
              owner_approval_sha256,request_sha256,approved_at_ms)
             VALUES(?1,'project',?2,?3,?4,?5,?6,?7)",
            params![
                approval.approval_id.to_string(),
                approval.specification_id.to_string(),
                approval.repository.repository_id.to_string(),
                u64_to_i64(approval.owner_control_revision)?,
                approval.owner_approval_sha256.as_slice(),
                request_sha256.as_slice(),
                u64_to_i64(now_ms)?
            ],
        )?;
        tx.execute(
            "INSERT INTO assembly_line_repositories
             (repository_id,git_url,repository_revision,lifecycle_revision,visibility,
              approved_specification_id,approved_specification_revision,
              approved_specification_sha256,owner_approval_sha256,lifecycle,effect_possible,
              creation_evidence_sha256,created_at_ms)
             VALUES(?1,?2,1,1,?3,?4,?5,?6,?7,'creation_pending',0,NULL,?8)",
            params![
                approval.repository.repository_id.to_string(),
                approval.repository.git_url.url,
                project_visibility_str(approval.visibility.ok_or_else(|| {
                    MasterError::InvalidAssemblyLinePlanningInput(
                        "project visibility is required".to_string(),
                    )
                })?),
                approval.specification_id.to_string(),
                u64_to_i64(approval.specification_revision)?,
                approval.specification_sha256.as_slice(),
                approval.owner_approval_sha256.as_slice(),
                u64_to_i64(now_ms)?
            ],
        )?;
        advance_assembly_line_owner_revision_tx(&tx)?;
        append_assembly_line_audit_tx(
            &tx,
            "project_creation_intent_recorded",
            now_ms,
            serde_json::json!({
                "approval_id": approval.approval_id, "specification_id": approval.specification_id,
                "owner_approval_sha256": approval.owner_approval_sha256,
                "lifecycle": "creation_pending", "github_called": false, "external_effect": false
            }),
        )?;
        let projection =
            assembly_line_repository_projection_tx(&tx, approval.repository.repository_id)?;
        tx.commit()?;
        Ok(projection)
    }

    pub fn approve_assembly_line_feature_and_enqueue(
        &mut self,
        approval: &BrainstormingOwnerApprovalBinding,
        now_ms: u64,
    ) -> Result<FeatureQueueEntryProjection, MasterError> {
        self.approve_assembly_line_feature_and_enqueue_inner(approval, None, now_ms)
    }

    pub(crate) fn approve_assembly_line_feature_and_enqueue_with_provider(
        &mut self,
        approval: &BrainstormingOwnerApprovalBinding,
        binding: &crate::planning_effects::BrainstormingAdapterBinding,
        adapter_catalog_sha256: [u8; 32],
        now_ms: u64,
    ) -> Result<FeatureQueueEntryProjection, MasterError> {
        self.approve_assembly_line_feature_and_enqueue_inner(
            approval,
            Some((binding, adapter_catalog_sha256)),
            now_ms,
        )
    }

    fn approve_assembly_line_feature_and_enqueue_inner(
        &mut self,
        approval: &BrainstormingOwnerApprovalBinding,
        provider: Option<(
            &crate::planning_effects::BrainstormingAdapterBinding,
            [u8; 32],
        )>,
        now_ms: u64,
    ) -> Result<FeatureQueueEntryProjection, MasterError> {
        let canonical_json = canonical_json(&serde_json::to_value(approval)?)?;
        let request_sha256: [u8; 32] = Sha256::digest(canonical_json.as_bytes()).into();
        let catalog = self.assembly_line_orchestrator_catalog();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if assembly_line_request_replay_tx(
            &tx,
            "feature_approval",
            approval.approval_id,
            request_sha256,
        )? {
            return assembly_line_queue_projection_tx(&tx, approval.approval_id);
        }
        let owner_revision = assembly_line_owner_revision_tx(&tx)?;
        if approval.owner_control_revision != owner_revision {
            return Err(MasterError::StaleAssemblyLineOwnerControlRevision {
                expected: approval.owner_control_revision,
                found: owner_revision,
            });
        }
        let expected_queue_revision = approval.expected_queue_revision.ok_or_else(|| {
            MasterError::InvalidAssemblyLinePlanningInput(
                "feature queue revision is required".to_string(),
            )
        })?;
        let prior_state = assembly_line_state_tx(&tx)?;
        if prior_state.queue_revision != expected_queue_revision {
            return Err(MasterError::StaleAssemblyLineQueueRevision {
                expected: expected_queue_revision,
                found: prior_state.queue_revision,
            });
        }
        let next_owner_revision = owner_revision
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        let next_state_revision = prior_state
            .state_revision
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        let next_queue_revision = prior_state
            .queue_revision
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        let next_queue_count = prior_state
            .queue_count
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        let next_owner_revision_i64 = u64_to_i64(next_owner_revision)?;
        let next_state_revision_i64 = u64_to_i64(next_state_revision)?;
        let next_queue_revision_i64 = u64_to_i64(next_queue_revision)?;
        let draft_json: String = tx
            .query_row(
                "SELECT canonical_json FROM assembly_line_feature_drafts WHERE draft_id=?1",
                [approval.draft_id.to_string()],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(MasterError::AssemblyLinePlanningImmutable)?;
        let frozen_json: String = tx.query_row(
            "SELECT canonical_json FROM assembly_line_frozen_specifications WHERE specification_id=?1 AND target_kind='feature'",
            [approval.specification_id.to_string()], |row| row.get(0),
        ).optional()?.ok_or(MasterError::AssemblyLinePlanningImmutable)?;
        let draft: FeatureBrainstormingDraft = serde_json::from_str(&draft_json)?;
        let frozen: FrozenBrainstormingSpecification = serde_json::from_str(&frozen_json)?;
        approval.validate_for_feature(&draft, &frozen, &catalog)?;
        require_accepted_feature_provider_provenance_tx(&tx, &frozen, provider)?;
        require_created_assembly_line_repository_tx(
            &tx,
            &draft.repository,
            approval
                .expected_repository_revision
                .ok_or(MasterError::AssemblyLineRepositoryUnavailable)?,
        )?;
        let count: i64 = tx.query_row("SELECT COUNT(*) FROM assembly_line_queue", [], |row| {
            row.get(0)
        })?;
        if count >= i64::from(MAX_ASSEMBLY_LINE_QUEUE_COUNT) {
            return Err(MasterError::AssemblyLineQueueFull);
        }
        let position = i64_to_u64(count)?
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        insert_assembly_line_request_tx(
            &tx,
            "feature_approval",
            approval.approval_id,
            request_sha256,
            None,
            now_ms,
        )?;
        tx.execute(
            "INSERT INTO assembly_line_owner_approvals
             (approval_id,target_kind,specification_id,repository_id,owner_control_revision,
              owner_approval_sha256,request_sha256,approved_at_ms)
             VALUES(?1,'feature',?2,?3,?4,?5,?6,?7)",
            params![
                approval.approval_id.to_string(),
                approval.specification_id.to_string(),
                approval.repository.repository_id.to_string(),
                u64_to_i64(approval.owner_control_revision)?,
                approval.owner_approval_sha256.as_slice(),
                request_sha256.as_slice(),
                u64_to_i64(now_ms)?
            ],
        )?;
        tx.execute(
            "INSERT INTO assembly_line_queue
             (feature_id,repository_id,specification_id,specification_revision,
              specification_sha256,owner_approval_sha256,queue_position,lifecycle_revision,
              lifecycle,enqueued_at_ms)
             VALUES(?1,?2,?3,?4,?5,?6,?7,1,'queued',?8)",
            params![
                approval.approval_id.to_string(),
                approval.repository.repository_id.to_string(),
                approval.specification_id.to_string(),
                u64_to_i64(approval.specification_revision)?,
                approval.specification_sha256.as_slice(),
                approval.owner_approval_sha256.as_slice(),
                u64_to_i64(position)?,
                u64_to_i64(now_ms)?
            ],
        )?;
        let changed = tx.execute(
            "UPDATE assembly_line_state
             SET owner_control_revision=?1,state_revision=?2,queue_revision=?3
             WHERE singleton=1 AND owner_control_revision=?4
               AND state_revision=?5 AND queue_revision=?6",
            params![
                next_owner_revision_i64,
                next_state_revision_i64,
                next_queue_revision_i64,
                u64_to_i64(owner_revision)?,
                u64_to_i64(prior_state.state_revision)?,
                u64_to_i64(prior_state.queue_revision)?
            ],
        )?;
        if changed != 1 {
            return Err(MasterError::InvalidStoredState(
                "assembly-line enqueue state CAS affected an unexpected row count".to_string(),
            ));
        }
        let resulting_owner_revision = assembly_line_owner_revision_tx(&tx)?;
        let resulting_state = assembly_line_state_tx(&tx)?;
        let mut expected_resulting_state = prior_state.clone();
        expected_resulting_state.state_revision = next_state_revision;
        expected_resulting_state.queue_revision = next_queue_revision;
        expected_resulting_state.queue_count = next_queue_count;
        if resulting_owner_revision != next_owner_revision
            || resulting_state != expected_resulting_state
        {
            return Err(MasterError::InvalidStoredState(
                "assembly-line enqueue state did not match its authoritative transition"
                    .to_string(),
            ));
        }
        append_assembly_line_audit_tx(
            &tx,
            "feature_queued",
            now_ms,
            serde_json::json!({
                "feature_id": approval.approval_id, "specification_id": approval.specification_id,
                "specification_sha256": approval.specification_sha256,
                "owner_approval_sha256": approval.owner_approval_sha256,
                "queue_position": position, "dispatch_created": false, "external_effect": false
            }),
        )?;
        let projection = assembly_line_queue_projection_tx(&tx, approval.approval_id)?;
        tx.commit()?;
        Ok(projection)
    }

    pub fn set_assembly_line_auto_run(
        &mut self,
        request: &AssemblyLineAutoRunRequest,
        now_ms: u64,
    ) -> Result<AssemblyLineAutoRunReceipt, MasterError> {
        request.validate()?;
        let request_json = canonical_json(&serde_json::to_value(request)?)?;
        let request_sha256: [u8; 32] = Sha256::digest(request_json.as_bytes()).into();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if assembly_line_request_replay_tx(&tx, "auto_run", request.request_id, request_sha256)? {
            let response: String = tx.query_row(
                "SELECT response_json FROM assembly_line_requests WHERE request_kind='auto_run' AND record_id=?1",
                [request.request_id.to_string()], |row| row.get(0),
            )?;
            return Ok(serde_json::from_str(&response)?);
        }
        let prior_state = assembly_line_state_tx(&tx)?;
        if prior_state.state_revision != request.expected_state_revision {
            return Err(MasterError::StaleAssemblyLineStateRevision {
                expected: request.expected_state_revision,
                found: prior_state.state_revision,
            });
        }
        let prior_owner_revision = assembly_line_owner_revision_tx(&tx)?;
        let next_owner_revision = prior_owner_revision
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        let next_state_revision = prior_state
            .state_revision
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        let changed = tx.execute(
            "UPDATE assembly_line_state
             SET owner_control_revision=?1,state_revision=?2,auto_run=?3
             WHERE singleton=1 AND owner_control_revision=?4 AND state_revision=?5",
            params![
                u64_to_i64(next_owner_revision)?,
                u64_to_i64(next_state_revision)?,
                if request.auto_run { 1_i64 } else { 0_i64 },
                u64_to_i64(prior_owner_revision)?,
                u64_to_i64(prior_state.state_revision)?
            ],
        )?;
        if changed != 1 {
            return Err(MasterError::InvalidStoredState(
                "assembly-line auto-run state CAS affected an unexpected row count".to_string(),
            ));
        }
        let resulting_state = assembly_line_state_tx(&tx)?;
        let resulting_owner_revision = assembly_line_owner_revision_tx(&tx)?;
        let mut expected_resulting_state = prior_state.clone();
        expected_resulting_state.state_revision = next_state_revision;
        expected_resulting_state.auto_run = request.auto_run;
        if resulting_owner_revision != next_owner_revision
            || resulting_state != expected_resulting_state
        {
            return Err(MasterError::InvalidStoredState(
                "assembly-line auto-run state did not match its authoritative transition"
                    .to_string(),
            ));
        }
        let receipt = AssemblyLineAutoRunReceipt {
            schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
            request_id: request.request_id,
            resulting_state,
        };
        receipt.validate_for_request_and_prior_state(request, &prior_state)?;
        let response_json = canonical_json(&serde_json::to_value(&receipt)?)?;
        insert_assembly_line_request_tx(
            &tx,
            "auto_run",
            request.request_id,
            request_sha256,
            Some(&response_json),
            now_ms,
        )?;
        append_assembly_line_audit_tx(
            &tx,
            "auto_run_changed",
            now_ms,
            serde_json::json!({
                "request_id": request.request_id, "state_revision": receipt.resulting_state.state_revision,
                "auto_run": request.auto_run, "execution_started": false, "external_effect": false
            }),
        )?;
        tx.commit()?;
        Ok(receipt)
    }

    /// Records a revisioned provisioning observation only. It does not install,
    /// launch, or expose the bound components to any product route.
    pub fn record_assembly_line_execution_capabilities(
        &mut self,
        binding: &AssemblyLineExecutionCapabilityBinding,
        now_ms: u64,
    ) -> Result<(), MasterError> {
        binding.validate()?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let state = assembly_line_state_tx(&tx)?;
        if state.state_revision != binding.expected_state_revision
            || !matches!(
                state.lifecycle,
                AssemblyLineLifecycleState::Stopped
                    | AssemblyLineLifecycleState::WaitingForOwnerStart
            )
        {
            return Err(MasterError::StaleAssemblyLineStateRevision {
                expected: binding.expected_state_revision,
                found: state.state_revision,
            });
        }
        require_emergency_pause_revision_tx(&tx, binding.expected_emergency_pause_revision)?;
        let current_revision: i64 = tx.query_row(
            "SELECT COALESCE(MAX(binding_revision),0)
             FROM assembly_line_execution_capabilities",
            [],
            |row| row.get(0),
        )?;
        let current_revision = i64_to_u64(current_revision)?;
        if binding.binding_revision != current_revision.saturating_add(1) {
            return Err(MasterError::AssemblyLineExecutionCapabilityUnavailable);
        }
        tx.execute(
            "INSERT INTO assembly_line_execution_capabilities
             (binding_revision,state_revision,emergency_pause_revision,
              windows_executor_id,windows_executor_revision,windows_executor_sha256,
              mac_executor_id,mac_executor_revision,mac_executor_sha256,
              windows_broker_id,windows_broker_revision,windows_broker_sha256,
              mac_broker_id,mac_broker_revision,mac_broker_sha256,
              protected_control_plane_sha256,windows_receipt_signer_key_id,
              windows_receipt_verifying_key,mac_receipt_signer_key_id,
              mac_receipt_verifying_key,healthy,provisioning_evidence_sha256,recorded_at_ms)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,
                    ?16,?17,?18,?19,?20,?21,?22,?23)",
            params![
                u64_to_i64(binding.binding_revision)?,
                u64_to_i64(binding.expected_state_revision)?,
                u64_to_i64(binding.expected_emergency_pause_revision)?,
                binding.windows_executor_id.to_string(),
                u64_to_i64(binding.windows_executor_revision)?,
                binding.windows_executor_sha256.as_slice(),
                binding.mac_executor_id.to_string(),
                u64_to_i64(binding.mac_executor_revision)?,
                binding.mac_executor_sha256.as_slice(),
                binding.windows_broker_id.to_string(),
                u64_to_i64(binding.windows_broker_revision)?,
                binding.windows_broker_sha256.as_slice(),
                binding.mac_broker_id.to_string(),
                u64_to_i64(binding.mac_broker_revision)?,
                binding.mac_broker_sha256.as_slice(),
                binding.protected_control_plane_sha256.as_slice(),
                binding.windows_receipt_signer_key_id,
                binding.windows_receipt_verifying_key.as_slice(),
                binding.mac_receipt_signer_key_id,
                binding.mac_receipt_verifying_key.as_slice(),
                i64::from(binding.healthy),
                binding.provisioning_evidence_sha256.as_slice(),
                u64_to_i64(now_ms)?,
            ],
        )?;
        append_assembly_line_audit_tx(
            &tx,
            "execution_capabilities_recorded",
            now_ms,
            serde_json::json!({
                "binding_revision": binding.binding_revision,
                "state_revision": binding.expected_state_revision,
                "emergency_pause_revision": binding.expected_emergency_pause_revision,
                "healthy": binding.healthy,
                "provisioning_evidence_sha256": binding.provisioning_evidence_sha256,
                "installed_or_started": false,
                "external_effect": false
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Creates durable authority in `starting`. The authority is effect-possible
    /// but cannot become `running` until both platform activation receipts are
    /// cryptographically admitted.
    pub fn start_assembly_line(
        &mut self,
        request: &AssemblyLineStartRequest,
        now_ms: u64,
    ) -> Result<AssemblyLineStartReceipt, MasterError> {
        request.validate()?;
        let request_json = canonical_json(&serde_json::to_value(request)?)?;
        let request_sha256: [u8; 32] = Sha256::digest(request_json.as_bytes()).into();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(response) = assembly_line_execution_request_replay_tx(
            &tx,
            "start",
            request.request_id,
            request_sha256,
        )? {
            let receipt: AssemblyLineStartReceipt = serde_json::from_str(&response)?;
            receipt.validate_for_request(request)?;
            return Ok(receipt);
        }
        let prior = assembly_line_state_tx(&tx)?;
        if prior.state_revision != request.expected_state_revision {
            return Err(MasterError::StaleAssemblyLineStateRevision {
                expected: request.expected_state_revision,
                found: prior.state_revision,
            });
        }
        if prior.queue_revision != request.expected_queue_revision {
            return Err(MasterError::StaleAssemblyLineQueueRevision {
                expected: request.expected_queue_revision,
                found: prior.queue_revision,
            });
        }
        require_unpaused_revision_tx(&tx, request.expected_emergency_pause_revision)?;
        if prior.queue_count == 0
            || prior.queue_count != request.queue_count
            || prior.auto_run != request.auto_run
            || prior.lifecycle != AssemblyLineLifecycleState::Stopped
        {
            return Err(MasterError::AssemblyLineExecutionControlUnavailable);
        }
        let capability = current_assembly_line_execution_capability_tx(&tx)?;
        if !capability.healthy
            || capability.expected_state_revision != prior.state_revision
            || capability.expected_emergency_pause_revision
                != request.expected_emergency_pause_revision
            || capability.windows_executor_id != request.windows_executor_id
            || capability.windows_executor_revision != request.windows_executor_revision
            || capability.mac_executor_id != request.mac_executor_id
            || capability.mac_executor_revision != request.mac_executor_revision
        {
            return Err(MasterError::AssemblyLineExecutionCapabilityUnavailable);
        }
        let (feature_id, repository_id, feature_lifecycle_revision): (String, String, i64) = tx
            .query_row(
                "SELECT feature_id,repository_id,lifecycle_revision
                 FROM assembly_line_queue ORDER BY queue_position ASC LIMIT 1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
        let feature_id = parse_uuid(&feature_id)?;
        let repository_id = parse_uuid(&repository_id)?;
        let session_id = Uuid::new_v4();
        let child_epoch_id = Uuid::new_v4();
        let next_state_revision = prior
            .state_revision
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        let prior_authority_revision: i64 = tx.query_row(
            "SELECT authority_revision FROM assembly_line_execution_authority WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        let next_authority_revision = i64_to_u64(prior_authority_revision)?
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        let session = AssemblyLineSessionEpoch {
            schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
            session_id,
            session_revision: 1,
            start_request_id: request.request_id,
            started_queue_count: request.queue_count,
            state_revision: next_state_revision,
            queue_revision: request.expected_queue_revision,
            emergency_pause_revision: request.expected_emergency_pause_revision,
            owner_start_approval_sha256: request.owner_start_approval_sha256,
            windows_executor_id: request.windows_executor_id,
            windows_executor_revision: request.windows_executor_revision,
            mac_executor_id: request.mac_executor_id,
            mac_executor_revision: request.mac_executor_revision,
            auto_run: request.auto_run,
        };
        session.validate_for_start(request)?;
        let child = AssemblyLineChildEpoch {
            schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
            child_epoch_id,
            child_epoch_revision: 1,
            session_id,
            session_revision: session.session_revision,
            feature_id,
            repository_id,
            feature_lifecycle_revision: i64_to_u64(feature_lifecycle_revision)?,
            queue_revision: request.expected_queue_revision,
            windows_executor_id: request.windows_executor_id,
            windows_executor_revision: request.windows_executor_revision,
            mac_executor_id: request.mac_executor_id,
            mac_executor_revision: request.mac_executor_revision,
        };
        child.validate_for_session(&session)?;
        tx.execute(
            "INSERT INTO assembly_line_execution_sessions
             (session_id,session_revision,start_request_id,started_queue_count,state_revision,
              queue_revision,emergency_pause_revision,owner_start_approval_sha256,
              capability_binding_revision,windows_executor_id,windows_executor_revision,
              mac_executor_id,mac_executor_revision,auto_run,started_at_ms)
             VALUES(?1,1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            params![
                session_id.to_string(),
                request.request_id.to_string(),
                i64::from(request.queue_count),
                u64_to_i64(next_state_revision)?,
                u64_to_i64(request.expected_queue_revision)?,
                u64_to_i64(request.expected_emergency_pause_revision)?,
                request.owner_start_approval_sha256.as_slice(),
                u64_to_i64(capability.binding_revision)?,
                request.windows_executor_id.to_string(),
                u64_to_i64(request.windows_executor_revision)?,
                request.mac_executor_id.to_string(),
                u64_to_i64(request.mac_executor_revision)?,
                i64::from(request.auto_run),
                u64_to_i64(now_ms)?,
            ],
        )?;
        tx.execute(
            "INSERT INTO assembly_line_child_epochs
             (child_epoch_id,child_epoch_revision,session_id,session_revision,feature_id,
              repository_id,feature_lifecycle_revision,queue_revision,authority_revision,
              lifecycle,effect_possible,started_at_ms)
             VALUES(?1,1,?2,1,?3,?4,?5,?6,?7,'starting',1,?8)",
            params![
                child_epoch_id.to_string(),
                session_id.to_string(),
                feature_id.to_string(),
                repository_id.to_string(),
                feature_lifecycle_revision,
                u64_to_i64(request.expected_queue_revision)?,
                u64_to_i64(next_authority_revision)?,
                u64_to_i64(now_ms)?,
            ],
        )?;
        if tx.execute(
            "UPDATE assembly_line_queue
             SET lifecycle='starting',lifecycle_revision=lifecycle_revision+1
             WHERE feature_id=?1 AND queue_position=1 AND lifecycle='queued'",
            [feature_id.to_string()],
        )? != 1
        {
            return Err(MasterError::AssemblyLineExecutionControlUnavailable);
        }
        if tx.execute(
            "UPDATE assembly_line_execution_authority
             SET authority_revision=?1,revoked=0,session_id=?2,child_epoch_id=?3,updated_at_ms=?4
             WHERE singleton=1 AND authority_revision=?5",
            params![
                u64_to_i64(next_authority_revision)?,
                session_id.to_string(),
                child_epoch_id.to_string(),
                u64_to_i64(now_ms)?,
                prior_authority_revision,
            ],
        )? != 1
        {
            return Err(MasterError::AssemblyLineExecutionControlUnavailable);
        }
        let prior_owner_revision = assembly_line_owner_revision_tx(&tx)?;
        let next_owner_revision = prior_owner_revision
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        if tx.execute(
            "UPDATE assembly_line_state
             SET owner_control_revision=?1,state_revision=?2,
                 lifecycle='starting',session_id=?3,active_child_epoch_id=?4,
                 active_feature_id=?5,effect_possible=1,authority_revision=?6
             WHERE singleton=1 AND owner_control_revision=?7 AND state_revision=?8
               AND queue_revision=?9
               AND lifecycle='stopped' AND session_id IS NULL",
            params![
                u64_to_i64(next_owner_revision)?,
                u64_to_i64(next_state_revision)?,
                session_id.to_string(),
                child_epoch_id.to_string(),
                feature_id.to_string(),
                u64_to_i64(next_authority_revision)?,
                u64_to_i64(prior_owner_revision)?,
                u64_to_i64(prior.state_revision)?,
                u64_to_i64(prior.queue_revision)?,
            ],
        )? != 1
        {
            return Err(MasterError::AssemblyLineExecutionControlUnavailable);
        }
        let resulting_state = assembly_line_state_tx(&tx)?;
        let receipt = AssemblyLineStartReceipt {
            schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
            request_id: request.request_id,
            owner_start_approval_sha256: request.owner_start_approval_sha256,
            resulting_state,
            session,
            child,
        };
        receipt.validate_for_request(request)?;
        let response_json = canonical_json(&serde_json::to_value(&receipt)?)?;
        insert_assembly_line_execution_request_tx(
            &tx,
            "start",
            request.request_id,
            request_sha256,
            &response_json,
            now_ms,
        )?;
        append_assembly_line_audit_tx(
            &tx,
            "execution_start_intent_recorded",
            now_ms,
            serde_json::json!({
                "request_id": request.request_id,
                "session_id": session_id,
                "child_epoch_id": child_epoch_id,
                "feature_id": feature_id,
                "state_revision": next_state_revision,
                "queue_revision": request.expected_queue_revision,
                "authority_revision": next_authority_revision,
                "capability_binding_revision": capability.binding_revision,
                "owner_control_revision": next_owner_revision,
                "audit_precedes_effect": true,
                "running_claimed": false,
                "external_effect_performed": false
            }),
        )?;
        tx.commit()?;
        Ok(receipt)
    }

    /// Claims the one effect-possible dispatch attempt for an exact durable
    /// Start intent. `None` means the identical intent was already claimed and
    /// must not be dispatched again automatically.
    pub fn claim_assembly_line_start_dispatch(
        &mut self,
        receipt: &AssemblyLineStartReceipt,
        now_ms: u64,
    ) -> Result<Option<AssemblyLineStartDispatchIntent>, MasterError> {
        receipt.validate()?;
        let response_json = canonical_json(&serde_json::to_value(receipt)?)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let stored_response: String = tx
            .query_row(
                "SELECT response_json FROM assembly_line_execution_requests
                 WHERE request_kind='start' AND request_id=?1",
                [receipt.request_id.to_string()],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(MasterError::AssemblyLineExecutionControlUnavailable)?;
        if stored_response != response_json {
            return Err(MasterError::AssemblyLineExecutionControlUnavailable);
        }
        let state = assembly_line_state_tx(&tx)?;
        if state.lifecycle != AssemblyLineLifecycleState::Starting
            || state.state_revision != receipt.resulting_state.state_revision
            || state.session_id != Some(receipt.session.session_id)
            || state.active_child_epoch_id != Some(receipt.child.child_epoch_id)
        {
            return Err(MasterError::AssemblyLineExecutionControlUnavailable);
        }
        let (authority_revision, revoked, authority_session, authority_child): (
            i64,
            i64,
            Option<String>,
            Option<String>,
        ) = tx.query_row(
            "SELECT authority_revision,revoked,session_id,child_epoch_id
             FROM assembly_line_execution_authority WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        if revoked != 0
            || authority_session.as_deref() != Some(receipt.session.session_id.to_string().as_str())
            || authority_child.as_deref() != Some(receipt.child.child_epoch_id.to_string().as_str())
        {
            return Err(MasterError::AssemblyLineExecutionControlUnavailable);
        }
        let intent = AssemblyLineStartDispatchIntent {
            request_id: receipt.request_id,
            session_id: receipt.session.session_id,
            child_epoch_id: receipt.child.child_epoch_id,
            authority_revision: i64_to_u64(authority_revision)?,
            state_revision: receipt.resulting_state.state_revision,
            queue_revision: receipt.resulting_state.queue_revision,
            owner_start_approval_sha256: receipt.owner_start_approval_sha256,
        };
        let intent_sha256: [u8; 32] =
            Sha256::digest(canonical_json(&serde_json::to_value(&intent)?)?.as_bytes()).into();
        if !claim_assembly_line_effect_dispatch_tx(
            &tx,
            "start",
            intent.request_id,
            intent_sha256,
            now_ms,
        )? {
            return Ok(None);
        }
        append_assembly_line_audit_tx(
            &tx,
            "execution_start_dispatch_claimed",
            now_ms,
            serde_json::json!({
                "request_id": intent.request_id,
                "session_id": intent.session_id,
                "child_epoch_id": intent.child_epoch_id,
                "authority_revision": intent.authority_revision,
                "intent_sha256": intent_sha256,
                "effect_possible": true,
                "external_effect_confirmed": false
            }),
        )?;
        tx.commit()?;
        Ok(Some(intent))
    }

    /// Verifies one platform activation acknowledgement. Only a complete pair
    /// from the session-pinned Windows and macOS keys promotes `starting` to
    /// `running`.
    pub fn record_assembly_line_activation_receipt(
        &mut self,
        receipt: &ExecutionActivationReceipt,
        now_ms: u64,
    ) -> Result<AssemblyLineState, MasterError> {
        receipt.validate()?;
        let receipt_json = canonical_json(&serde_json::to_value(receipt)?)?;
        let receipt_sha256: [u8; 32] = Sha256::digest(receipt_json.as_bytes()).into();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let binding = tx
            .query_row(
                "SELECT e.session_id,e.authority_revision,s.start_request_id,
                        s.windows_executor_id,s.windows_executor_revision,
                        s.mac_executor_id,s.mac_executor_revision,
                        c.windows_receipt_signer_key_id,c.windows_receipt_verifying_key,
                        c.mac_receipt_signer_key_id,c.mac_receipt_verifying_key
                 FROM assembly_line_child_epochs e
                 JOIN assembly_line_execution_sessions s ON s.session_id=e.session_id
                 JOIN assembly_line_execution_capabilities c
                   ON c.binding_revision=s.capability_binding_revision
                 WHERE e.child_epoch_id=?1",
                [receipt.child_epoch_id.to_string()],
                |row| {
                    Ok(AssemblyLineActivationVerificationBinding {
                        child_session: row.get(0)?,
                        child_authority_revision: row.get(1)?,
                        start_request_id: row.get(2)?,
                        windows_executor_id: row.get(3)?,
                        windows_executor_revision: row.get(4)?,
                        mac_executor_id: row.get(5)?,
                        mac_executor_revision: row.get(6)?,
                        windows_key_id: row.get(7)?,
                        windows_key: row.get(8)?,
                        mac_key_id: row.get(9)?,
                        mac_key: row.get(10)?,
                    })
                },
            )
            .optional()?
            .ok_or(MasterError::AssemblyLineExecutionReceiptMismatch)?;
        if binding.child_session != receipt.session_id.to_string()
            || i64_to_u64(binding.child_authority_revision)? != receipt.authority_revision
        {
            return Err(MasterError::AssemblyLineExecutionReceiptMismatch);
        }
        let dispatch_exists: i64 = tx.query_row(
            "SELECT COUNT(*) FROM assembly_line_effect_dispatches
             WHERE request_kind='start' AND request_id=?1",
            [binding.start_request_id],
            |row| row.get(0),
        )?;
        if dispatch_exists != 1 {
            return Err(MasterError::AssemblyLineExecutionReceiptMismatch);
        }
        let (expected_executor_id, expected_executor_revision, expected_key_id, expected_key) =
            match receipt.host_platform {
                ExecutionHostPlatform::Windows => (
                    binding.windows_executor_id,
                    binding.windows_executor_revision,
                    binding.windows_key_id,
                    binding.windows_key,
                ),
                ExecutionHostPlatform::Macos => (
                    binding.mac_executor_id,
                    binding.mac_executor_revision,
                    binding.mac_key_id,
                    binding.mac_key,
                ),
            };
        if expected_executor_id != receipt.executor_id.to_string()
            || i64_to_u64(expected_executor_revision)? != receipt.executor_revision
            || expected_key_id != receipt.signer_key_id
        {
            return Err(MasterError::AssemblyLineExecutionReceiptMismatch);
        }
        verify_execution_activation_receipt_signature(receipt, digest_array(&expected_key)?)?;
        let existing = tx
            .query_row(
                "SELECT session_id,receipt_sha256 FROM assembly_line_activation_receipts
                 WHERE receipt_id=?1",
                [receipt.receipt_id.to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()?;
        if let Some((session_id, existing_sha256)) = existing {
            if session_id != receipt.session_id.to_string()
                || digest_array(&existing_sha256)? != receipt_sha256
            {
                return Err(MasterError::AssemblyLineExecutionReceiptMismatch);
            }
            return assembly_line_state_tx(&tx);
        }
        let platform = match receipt.host_platform {
            ExecutionHostPlatform::Windows => "windows",
            ExecutionHostPlatform::Macos => "macos",
        };
        let platform_exists: i64 = tx.query_row(
            "SELECT COUNT(*) FROM assembly_line_activation_receipts
             WHERE session_id=?1 AND host_platform=?2",
            params![receipt.session_id.to_string(), platform],
            |row| row.get(0),
        )?;
        if platform_exists != 0 {
            return Err(MasterError::AssemblyLineExecutionReceiptMismatch);
        }
        let (authority_revision, revoked, authority_session, authority_child): (
            i64,
            i64,
            Option<String>,
            Option<String>,
        ) = tx.query_row(
            "SELECT authority_revision,revoked,session_id,child_epoch_id
             FROM assembly_line_execution_authority WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        if revoked != 0
            || i64_to_u64(authority_revision)? != receipt.authority_revision
            || authority_session.as_deref() != Some(receipt.session_id.to_string().as_str())
            || authority_child.as_deref() != Some(receipt.child_epoch_id.to_string().as_str())
        {
            return Err(MasterError::AssemblyLineExecutionReceiptMismatch);
        }
        tx.execute(
            "INSERT INTO assembly_line_activation_receipts
             (receipt_id,session_id,child_epoch_id,authority_revision,host_platform,
              signer_key_id,receipt_sha256,receipt_json,recorded_at_ms)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                receipt.receipt_id.to_string(),
                receipt.session_id.to_string(),
                receipt.child_epoch_id.to_string(),
                u64_to_i64(receipt.authority_revision)?,
                platform,
                receipt.signer_key_id,
                receipt_sha256.as_slice(),
                receipt_json,
                u64_to_i64(now_ms)?,
            ],
        )?;
        let receipt_count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM assembly_line_activation_receipts WHERE session_id=?1",
            [receipt.session_id.to_string()],
            |row| row.get(0),
        )?;
        if receipt_count == 2 {
            let state = assembly_line_state_tx(&tx)?;
            if state.lifecycle != AssemblyLineLifecycleState::Starting
                || state.session_id != Some(receipt.session_id)
                || state.active_child_epoch_id != Some(receipt.child_epoch_id)
            {
                return Err(MasterError::AssemblyLineExecutionReceiptMismatch);
            }
            let next_state_revision = state
                .state_revision
                .checked_add(1)
                .ok_or(MasterError::IntegerOutOfRange)?;
            let owner_revision = assembly_line_owner_revision_tx(&tx)?;
            let next_owner_revision = owner_revision
                .checked_add(1)
                .ok_or(MasterError::IntegerOutOfRange)?;
            if tx.execute(
                "UPDATE assembly_line_state
                 SET owner_control_revision=?1,state_revision=?2,lifecycle='running'
                 WHERE singleton=1 AND owner_control_revision=?3 AND state_revision=?4
                   AND lifecycle='starting' AND session_id=?5 AND active_child_epoch_id=?6",
                params![
                    u64_to_i64(next_owner_revision)?,
                    u64_to_i64(next_state_revision)?,
                    u64_to_i64(owner_revision)?,
                    u64_to_i64(state.state_revision)?,
                    receipt.session_id.to_string(),
                    receipt.child_epoch_id.to_string(),
                ],
            )? != 1
            {
                return Err(MasterError::AssemblyLineExecutionReceiptMismatch);
            }
            if tx.execute(
                "UPDATE assembly_line_child_epochs SET lifecycle='running'
                 WHERE child_epoch_id=?1 AND session_id=?2 AND lifecycle='starting'",
                params![
                    receipt.child_epoch_id.to_string(),
                    receipt.session_id.to_string()
                ],
            )? != 1
            {
                return Err(MasterError::AssemblyLineExecutionReceiptMismatch);
            }
            if tx.execute(
                "UPDATE assembly_line_queue SET lifecycle='active',lifecycle_revision=lifecycle_revision+1
                 WHERE feature_id=(SELECT active_feature_id FROM assembly_line_state WHERE singleton=1)
                   AND queue_position=1 AND lifecycle='starting'",
                [],
            )? != 1
            {
                return Err(MasterError::AssemblyLineExecutionReceiptMismatch);
            }
        }
        append_assembly_line_audit_tx(
            &tx,
            "execution_activation_receipt_recorded",
            now_ms,
            serde_json::json!({
                "receipt_id": receipt.receipt_id,
                "session_id": receipt.session_id,
                "child_epoch_id": receipt.child_epoch_id,
                "authority_revision": receipt.authority_revision,
                "host_platform": platform,
                "receipt_sha256": receipt_sha256,
                "complete_receipt_set": receipt_count == 2,
                "running_claimed": receipt_count == 2
            }),
        )?;
        let state = assembly_line_state_tx(&tx)?;
        tx.commit()?;
        Ok(state)
    }

    pub fn stop_assembly_line(
        &mut self,
        request: &AssemblyLineStopRequest,
        now_ms: u64,
    ) -> Result<AssemblyLineTerminationIntent, MasterError> {
        request.validate()?;
        self.record_assembly_line_termination_intent(
            AssemblyLineTerminationControlBinding {
                request_kind: "stop",
                request_id: request.request_id,
                session_id: request.session_id,
                child_epoch_id: request.expected_child_epoch_id,
                expected_state_revision: request.expected_state_revision,
                expected_emergency_pause_revision: None,
            },
            now_ms,
        )
    }

    pub fn emergency_pause_assembly_line(
        &mut self,
        request: &AssemblyLineEmergencyPauseRequest,
        now_ms: u64,
    ) -> Result<AssemblyLineTerminationIntent, MasterError> {
        request.validate()?;
        self.record_assembly_line_termination_intent(
            AssemblyLineTerminationControlBinding {
                request_kind: "emergency_pause",
                request_id: request.request_id,
                session_id: request.session_id,
                child_epoch_id: request.expected_child_epoch_id,
                expected_state_revision: request.expected_state_revision,
                expected_emergency_pause_revision: Some(request.expected_emergency_pause_revision),
            },
            now_ms,
        )
    }

    /// Claims the one host-effect dispatch attempt for an exact durable Stop or
    /// Emergency Pause intent. Authority has already been revoked by the time
    /// this method can succeed.
    pub fn claim_assembly_line_termination_dispatch(
        &mut self,
        intent: &AssemblyLineTerminationIntent,
        now_ms: u64,
    ) -> Result<bool, MasterError> {
        if intent.request_id.is_nil()
            || intent.session_id.is_nil()
            || intent.child_epoch_id.is_nil()
            || intent.authority_revision == 0
            || intent.checkpoint_id.is_nil()
            || intent.checkpoint_sha256 == [0; 32]
            || intent.external_effect_performed
        {
            return Err(MasterError::AssemblyLineExecutionControlUnavailable);
        }
        let request_kind = match intent.mode {
            ExecutionTerminationMode::Stop => "stop",
            ExecutionTerminationMode::EmergencyPause => "emergency_pause",
        };
        let response_json = canonical_json(&serde_json::to_value(intent)?)?;
        let intent_sha256: [u8; 32] = Sha256::digest(response_json.as_bytes()).into();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let stored_response: String = tx
            .query_row(
                "SELECT response_json FROM assembly_line_execution_requests
                 WHERE request_kind=?1 AND request_id=?2",
                params![request_kind, intent.request_id.to_string()],
                |row| row.get(0),
            )
            .optional()?
            .ok_or(MasterError::AssemblyLineExecutionControlUnavailable)?;
        if stored_response != response_json {
            return Err(MasterError::AssemblyLineExecutionControlUnavailable);
        }
        let (authority_revision, revoked, authority_session, authority_child): (
            i64,
            i64,
            Option<String>,
            Option<String>,
        ) = tx.query_row(
            "SELECT authority_revision,revoked,session_id,child_epoch_id
             FROM assembly_line_execution_authority
             WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        if revoked != 1
            || i64_to_u64(authority_revision)? != intent.authority_revision
            || authority_session.as_deref() != Some(intent.session_id.to_string().as_str())
            || authority_child.as_deref() != Some(intent.child_epoch_id.to_string().as_str())
        {
            return Err(MasterError::AssemblyLineExecutionControlUnavailable);
        }
        let claimed = claim_assembly_line_effect_dispatch_tx(
            &tx,
            request_kind,
            intent.request_id,
            intent_sha256,
            now_ms,
        )?;
        if claimed {
            append_assembly_line_audit_tx(
                &tx,
                "execution_termination_dispatch_claimed",
                now_ms,
                serde_json::json!({
                    "request_kind": request_kind,
                    "request_id": intent.request_id,
                    "session_id": intent.session_id,
                    "child_epoch_id": intent.child_epoch_id,
                    "authority_revision": intent.authority_revision,
                    "intent_sha256": intent_sha256,
                    "authority_revoked_before_dispatch": true,
                    "external_effect_confirmed": false
                }),
            )?;
        }
        tx.commit()?;
        Ok(claimed)
    }

    pub fn assembly_line_termination_pending(&self, request_id: Uuid) -> Result<bool, MasterError> {
        if request_id.is_nil() {
            return Err(MasterError::AssemblyLineExecutionControlUnavailable);
        }
        let (intent_exists, count): (i64, i64) = self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM assembly_line_control_intents WHERE request_id=?1),
                    (SELECT COUNT(*) FROM assembly_line_termination_receipts WHERE request_id=?1)",
            [request_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if intent_exists != 1 {
            return Err(MasterError::AssemblyLineExecutionControlUnavailable);
        }
        Ok(count < 2)
    }

    fn record_assembly_line_termination_intent(
        &mut self,
        control: AssemblyLineTerminationControlBinding,
        now_ms: u64,
    ) -> Result<AssemblyLineTerminationIntent, MasterError> {
        let AssemblyLineTerminationControlBinding {
            request_kind,
            request_id,
            session_id,
            child_epoch_id,
            expected_state_revision,
            expected_emergency_pause_revision,
        } = control;
        let request_value = serde_json::json!({
            "request_kind": request_kind,
            "request_id": request_id,
            "session_id": session_id,
            "child_epoch_id": child_epoch_id,
            "expected_state_revision": expected_state_revision,
            "expected_emergency_pause_revision": expected_emergency_pause_revision,
        });
        let request_json = canonical_json(&request_value)?;
        let request_sha256: [u8; 32] = Sha256::digest(request_json.as_bytes()).into();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if let Some(response) = assembly_line_execution_request_replay_tx(
            &tx,
            request_kind,
            request_id,
            request_sha256,
        )? {
            return Ok(serde_json::from_str(&response)?);
        }
        let prior = assembly_line_state_tx(&tx)?;
        if prior.state_revision != expected_state_revision {
            return Err(MasterError::StaleAssemblyLineStateRevision {
                expected: expected_state_revision,
                found: prior.state_revision,
            });
        }
        if !matches!(
            prior.lifecycle,
            AssemblyLineLifecycleState::Starting | AssemblyLineLifecycleState::Running
        ) || prior.session_id != Some(session_id)
            || prior.active_child_epoch_id != Some(child_epoch_id)
        {
            return Err(MasterError::AssemblyLineExecutionControlUnavailable);
        }
        let (authority_revision, revoked, authority_session, authority_child): (
            i64,
            i64,
            Option<String>,
            Option<String>,
        ) = tx.query_row(
            "SELECT authority_revision,revoked,session_id,child_epoch_id
                 FROM assembly_line_execution_authority WHERE singleton=1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        let session_text = session_id.to_string();
        let child_text = child_epoch_id.to_string();
        if revoked != 0
            || authority_session.as_deref() != Some(session_text.as_str())
            || authority_child.as_deref() != Some(child_text.as_str())
        {
            return Err(MasterError::AssemblyLineExecutionControlUnavailable);
        }
        if let Some(expected_pause_revision) = expected_emergency_pause_revision {
            require_unpaused_revision_tx(&tx, expected_pause_revision)?;
        }
        let next_authority_revision = i64_to_u64(authority_revision)?
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        let next_state_revision = expected_state_revision
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        let prior_owner_revision = assembly_line_owner_revision_tx(&tx)?;
        let next_owner_revision = prior_owner_revision
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        let checkpoint_id = Uuid::new_v4();
        let checkpoint_sha256: [u8; 32] = Sha256::digest(
            canonical_json(&serde_json::json!({
                "domain": "assemblywright.assembly-line.control-checkpoint-intent.v1",
                "request_id": request_id,
                "session_id": session_id,
                "child_epoch_id": child_epoch_id,
                "mode": request_kind,
                "authority_revision": next_authority_revision,
                "checkpoint_id": checkpoint_id,
            }))?
            .as_bytes(),
        )
        .into();
        let lifecycle = if request_kind == "emergency_pause" {
            "emergency_paused"
        } else {
            "stopping"
        };
        if let Some(expected_pause_revision) = expected_emergency_pause_revision {
            if tx.execute(
                "UPDATE master_metadata SET integer_value=1
                 WHERE key='emergency_paused' AND integer_value=0",
                [],
            )? != 1
                || tx.execute(
                    "UPDATE master_metadata SET integer_value=integer_value+1
                     WHERE key='emergency_pause_revision' AND integer_value=?1",
                    [u64_to_i64(expected_pause_revision)?],
                )? != 1
            {
                return Err(MasterError::StaleEmergencyPauseRevision {
                    expected: expected_pause_revision,
                    found: emergency_pause_revision_tx(&tx)?,
                });
            }
            request_active_remote_work_cancellations_tx(&tx, now_ms)?;
        }
        if tx.execute(
            "UPDATE assembly_line_execution_authority
             SET authority_revision=?1,revoked=1,updated_at_ms=?2
             WHERE singleton=1 AND authority_revision=?3 AND revoked=0",
            params![
                u64_to_i64(next_authority_revision)?,
                u64_to_i64(now_ms)?,
                authority_revision,
            ],
        )? != 1
        {
            return Err(MasterError::AssemblyLineExecutionControlUnavailable);
        }
        if tx.execute(
            "UPDATE assembly_line_state
             SET owner_control_revision=?1,state_revision=?2,
                 lifecycle=?3,effect_possible=1,authority_revision=?4
             WHERE singleton=1 AND owner_control_revision=?5 AND state_revision=?6
               AND lifecycle IN('starting','running')
               AND session_id=?7 AND active_child_epoch_id=?8",
            params![
                u64_to_i64(next_owner_revision)?,
                u64_to_i64(next_state_revision)?,
                lifecycle,
                u64_to_i64(next_authority_revision)?,
                u64_to_i64(prior_owner_revision)?,
                u64_to_i64(expected_state_revision)?,
                session_id.to_string(),
                child_epoch_id.to_string(),
            ],
        )? != 1
        {
            return Err(MasterError::AssemblyLineExecutionControlUnavailable);
        }
        if tx.execute(
            "UPDATE assembly_line_child_epochs
             SET authority_revision=?1,lifecycle=?2,effect_possible=1
             WHERE child_epoch_id=?3 AND session_id=?4 AND lifecycle IN('starting','running')",
            params![
                u64_to_i64(next_authority_revision)?,
                lifecycle,
                child_epoch_id.to_string(),
                session_id.to_string(),
            ],
        )? != 1
        {
            return Err(MasterError::AssemblyLineExecutionControlUnavailable);
        }
        if tx.execute(
            "UPDATE assembly_line_queue
             SET lifecycle=?1,lifecycle_revision=lifecycle_revision+1
             WHERE feature_id=(SELECT active_feature_id FROM assembly_line_state WHERE singleton=1)
               AND queue_position=1 AND lifecycle IN('starting','active')",
            [lifecycle],
        )? != 1
        {
            return Err(MasterError::AssemblyLineExecutionControlUnavailable);
        }
        tx.execute(
            "INSERT INTO assembly_line_control_intents
             (request_id,mode,session_id,child_epoch_id,authority_revision,checkpoint_id,
              checkpoint_sha256,request_sha256,state_revision,termination_pending,recorded_at_ms)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,1,?10)",
            params![
                request_id.to_string(),
                request_kind,
                session_id.to_string(),
                child_epoch_id.to_string(),
                u64_to_i64(next_authority_revision)?,
                checkpoint_id.to_string(),
                checkpoint_sha256.as_slice(),
                request_sha256.as_slice(),
                u64_to_i64(next_state_revision)?,
                u64_to_i64(now_ms)?,
            ],
        )?;
        let intent = AssemblyLineTerminationIntent {
            request_id,
            session_id,
            child_epoch_id,
            mode: if request_kind == "emergency_pause" {
                ExecutionTerminationMode::EmergencyPause
            } else {
                ExecutionTerminationMode::Stop
            },
            authority_revision: next_authority_revision,
            checkpoint_id,
            checkpoint_sha256,
            resulting_state: assembly_line_state_tx(&tx)?,
            external_effect_performed: false,
        };
        let response_json = canonical_json(&serde_json::to_value(&intent)?)?;
        insert_assembly_line_execution_request_tx(
            &tx,
            request_kind,
            request_id,
            request_sha256,
            &response_json,
            now_ms,
        )?;
        append_assembly_line_audit_tx(
            &tx,
            if request_kind == "emergency_pause" {
                "emergency_termination_intent_recorded"
            } else {
                "stop_checkpoint_termination_intent_recorded"
            },
            now_ms,
            serde_json::json!({
                "request_id": request_id,
                "session_id": session_id,
                "child_epoch_id": child_epoch_id,
                "authority_revision": next_authority_revision,
                "checkpoint_id": checkpoint_id,
                "checkpoint_sha256": checkpoint_sha256,
                "termination_pending": true,
                "no_new_actions": true,
                "audit_precedes_effect": true,
                "external_effect_performed": false
            }),
        )?;
        tx.commit()?;
        Ok(intent)
    }

    /// Verifies and admits one executor receipt against the platform key pinned
    /// by the session's durable capability binding and the exact control intent.
    /// This kernel does not perform process termination.
    pub fn record_assembly_line_termination_receipt(
        &mut self,
        request_id: Uuid,
        receipt: &ExecutionTerminationReceipt,
        now_ms: u64,
    ) -> Result<AssemblyLineState, MasterError> {
        receipt.validate()?;
        let receipt_json = canonical_json(&serde_json::to_value(receipt)?)?;
        let receipt_sha256: [u8; 32] = Sha256::digest(receipt_json.as_bytes()).into();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let existing = tx
            .query_row(
                "SELECT request_id,receipt_sha256 FROM assembly_line_termination_receipts
                 WHERE receipt_id=?1",
                [receipt.receipt_id.to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
            )
            .optional()?;
        if let Some((existing_request_id, existing)) = existing {
            if existing_request_id != request_id.to_string()
                || digest_array(&existing)? != receipt_sha256
            {
                return Err(MasterError::AssemblyLineExecutionReceiptMismatch);
            }
            return assembly_line_state_tx(&tx);
        }
        let (mode, child_epoch_id, checkpoint_sha256, session_id): (
            String,
            String,
            Vec<u8>,
            String,
        ) = tx
            .query_row(
                "SELECT mode,child_epoch_id,checkpoint_sha256,session_id
                     FROM assembly_line_control_intents WHERE request_id=?1",
                [request_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?
            .ok_or(MasterError::AssemblyLineExecutionReceiptMismatch)?;
        let expected_mode = if mode == "emergency_pause" {
            ExecutionTerminationMode::EmergencyPause
        } else {
            ExecutionTerminationMode::Stop
        };
        let dispatch_exists: i64 = tx.query_row(
            "SELECT COUNT(*) FROM assembly_line_effect_dispatches
             WHERE request_kind=?1 AND request_id=?2",
            params![mode, request_id.to_string()],
            |row| row.get(0),
        )?;
        if receipt.child_epoch_id.to_string() != child_epoch_id
            || receipt.mode != expected_mode
            || receipt.last_checkpoint_sha256 != digest_array(&checkpoint_sha256)?
            || dispatch_exists != 1
        {
            return Err(MasterError::AssemblyLineExecutionReceiptMismatch);
        }
        let (windows_key_id, windows_key, mac_key_id, mac_key): (String, Vec<u8>, String, Vec<u8>) =
            tx.query_row(
                "SELECT c.windows_receipt_signer_key_id,c.windows_receipt_verifying_key,
                    c.mac_receipt_signer_key_id,c.mac_receipt_verifying_key
             FROM assembly_line_execution_sessions s
             JOIN assembly_line_execution_capabilities c
               ON c.binding_revision=s.capability_binding_revision
             WHERE s.session_id=?1",
                [session_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;
        let (expected_key_id, expected_key) = match receipt.descendant_scope {
            ExecutionDescendantScope::WindowsJobObject => (windows_key_id, windows_key),
            ExecutionDescendantScope::MacosProcessGroup => (mac_key_id, mac_key),
        };
        if receipt.signer_key_id != expected_key_id {
            return Err(MasterError::AssemblyLineExecutionReceiptMismatch);
        }
        verify_execution_termination_receipt_signature(receipt, digest_array(&expected_key)?)?;
        tx.execute(
            "INSERT INTO assembly_line_termination_receipts
             (receipt_id,request_id,child_epoch_id,mode,signer_key_id,outcome,
              last_checkpoint_sha256,receipt_sha256,receipt_json,recorded_at_ms)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                receipt.receipt_id.to_string(),
                request_id.to_string(),
                receipt.child_epoch_id.to_string(),
                mode,
                receipt.signer_key_id,
                match receipt.outcome {
                    ExecutionTerminationOutcome::Reaped => "reaped",
                    ExecutionTerminationOutcome::Incomplete => "incomplete",
                },
                receipt.last_checkpoint_sha256.as_slice(),
                receipt_sha256.as_slice(),
                receipt_json,
                u64_to_i64(now_ms)?,
            ],
        )?;
        let (count, incomplete): (i64, i64) = tx.query_row(
            "SELECT COUNT(*),COALESCE(SUM(CASE WHEN outcome='incomplete' THEN 1 ELSE 0 END),0)
             FROM assembly_line_termination_receipts WHERE request_id=?1",
            [request_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if count == 2 {
            let final_lifecycle = if incomplete != 0 {
                "incomplete_termination"
            } else if expected_mode == ExecutionTerminationMode::Stop {
                "paused_at_checkpoint"
            } else {
                "emergency_paused"
            };
            tx.execute(
                "UPDATE assembly_line_state
                 SET state_revision=state_revision+1,lifecycle=?1,
                     effect_possible=CASE WHEN ?1='paused_at_checkpoint' OR ?1='emergency_paused'
                                          THEN 0 ELSE 1 END
                 WHERE singleton=1 AND active_child_epoch_id=?2
                   AND lifecycle IN('stopping','emergency_paused')",
                params![final_lifecycle, child_epoch_id],
            )?;
            tx.execute(
                "UPDATE assembly_line_child_epochs
                 SET lifecycle=?1,effect_possible=CASE WHEN ?1='paused_at_checkpoint'
                                                        OR ?1='emergency_paused' THEN 0 ELSE 1 END
                 WHERE child_epoch_id=?2",
                params![final_lifecycle, child_epoch_id],
            )?;
            tx.execute(
                "UPDATE assembly_line_queue
                 SET lifecycle=?1,lifecycle_revision=lifecycle_revision+1
                 WHERE feature_id=(SELECT active_feature_id FROM assembly_line_state WHERE singleton=1)
                   AND queue_position=1 AND lifecycle IN('stopping','emergency_paused')",
                [final_lifecycle],
            )?;
        }
        append_assembly_line_audit_tx(
            &tx,
            "termination_receipt_recorded",
            now_ms,
            serde_json::json!({
                "request_id": request_id,
                "receipt_id": receipt.receipt_id,
                "child_epoch_id": receipt.child_epoch_id,
                "mode": mode,
                "outcome": match receipt.outcome {
                    ExecutionTerminationOutcome::Reaped => "reaped",
                    ExecutionTerminationOutcome::Incomplete => "incomplete",
                },
                "receipt_sha256": receipt_sha256,
                "complete_receipt_set": count == 2,
                "external_effect_claimed_by_master": false
            }),
        )?;
        let state = assembly_line_state_tx(&tx)?;
        tx.commit()?;
        Ok(state)
    }

    /// Verifies and persists one checkpoint receipt against the action host's
    /// session-pinned platform key and exact durable action intent. The action
    /// issuance path is intentionally not exposed by this controller slice.
    pub fn record_assembly_line_checkpoint_receipt(
        &mut self,
        receipt: &ExecutionCheckpointReceipt,
        now_ms: u64,
    ) -> Result<(), MasterError> {
        receipt.validate()?;
        let receipt_json = canonical_json(&serde_json::to_value(receipt)?)?;
        let receipt_sha256: [u8; 32] = Sha256::digest(receipt_json.as_bytes()).into();
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let action: Option<(i64, String, String, String)> = tx
            .query_row(
                "SELECT action_sequence,child_epoch_id,session_id,host_platform
                 FROM assembly_line_action_ledger WHERE action_id=?1",
                [receipt.action_id.to_string()],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .optional()?;
        let Some((action_sequence, child_epoch_id, session_id, host_platform)) = action else {
            return Err(MasterError::AssemblyLineExecutionReceiptMismatch);
        };
        if i64_to_u64(action_sequence)? != receipt.action_sequence
            || child_epoch_id != receipt.child_epoch_id.to_string()
        {
            return Err(MasterError::AssemblyLineExecutionReceiptMismatch);
        }
        let (windows_key_id, windows_key, mac_key_id, mac_key): (String, Vec<u8>, String, Vec<u8>) =
            tx.query_row(
                "SELECT c.windows_receipt_signer_key_id,c.windows_receipt_verifying_key,
                    c.mac_receipt_signer_key_id,c.mac_receipt_verifying_key
             FROM assembly_line_execution_sessions s
             JOIN assembly_line_execution_capabilities c
               ON c.binding_revision=s.capability_binding_revision
             WHERE s.session_id=?1",
                [session_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;
        let (expected_key_id, expected_key) = match host_platform.as_str() {
            "windows" => (windows_key_id, windows_key),
            "macos" => (mac_key_id, mac_key),
            _ => return Err(MasterError::AssemblyLineExecutionReceiptMismatch),
        };
        if receipt.signer_key_id != expected_key_id {
            return Err(MasterError::AssemblyLineExecutionReceiptMismatch);
        }
        verify_execution_checkpoint_receipt_signature(receipt, digest_array(&expected_key)?)?;
        let phase = match receipt.phase {
            ExecutionCheckpointPhase::BeforeEffect => "before_effect",
            ExecutionCheckpointPhase::AfterEffect => "after_effect",
        };
        let existing = tx
            .query_row(
                "SELECT receipt_sha256 FROM assembly_line_checkpoint_receipts
                 WHERE action_id=?1 AND phase=?2",
                params![receipt.action_id.to_string(), phase],
                |row| row.get::<_, Vec<u8>>(0),
            )
            .optional()?;
        if let Some(existing) = existing {
            return if digest_array(&existing)? == receipt_sha256 {
                Ok(())
            } else {
                Err(MasterError::AssemblyLineExecutionReceiptMismatch)
            };
        }
        tx.execute(
            "INSERT INTO assembly_line_checkpoint_receipts
             (action_id,child_epoch_id,action_sequence,phase,checkpoint_sha256,
              receipt_sha256,receipt_json,recorded_at_ms)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                receipt.action_id.to_string(),
                receipt.child_epoch_id.to_string(),
                u64_to_i64(receipt.action_sequence)?,
                phase,
                receipt.checkpoint_sha256.as_slice(),
                receipt_sha256.as_slice(),
                receipt_json,
                u64_to_i64(now_ms)?,
            ],
        )?;
        append_assembly_line_audit_tx(
            &tx,
            "execution_checkpoint_receipt_recorded",
            now_ms,
            serde_json::json!({
                "action_id": receipt.action_id,
                "action_sequence": receipt.action_sequence,
                "child_epoch_id": receipt.child_epoch_id,
                "phase": phase,
                "checkpoint_sha256": receipt.checkpoint_sha256,
                "receipt_sha256": receipt_sha256,
                "external_effect_claimed_by_master": false
            }),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Remote planning actions share the same kernel but additionally require
    /// the exact current designated Mac owner-control identity.
    pub fn authorize_assembly_line_owner_bridge(
        &mut self,
        registration: &DeviceRegistration,
    ) -> Result<(), MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let designation_revision = owner_control_bridge_designation_connection(&tx)?
            .ok_or(MasterError::OwnerControlBridgeNotDesignated)?
            .designation_revision;
        require_owner_control_bridge_tx(&tx, registration, designation_revision)
    }

    fn migrate(&mut self) -> Result<(), MasterError> {
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
        if version == 8 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE feature_repository_snapshot_claims (
                   snapshot_id TEXT PRIMARY KEY NOT NULL,
                   snapshot_sha256 BLOB NOT NULL
                     CHECK (length(snapshot_sha256) = 32),
                   feature_id TEXT NOT NULL
                     REFERENCES feature_conveyor_features(feature_id),
                   specification_revision INTEGER NOT NULL
                     CHECK (specification_revision > 0),
                   lease_id TEXT NOT NULL UNIQUE,
                   base_commit TEXT NOT NULL CHECK (length(base_commit) = 40),
                   scope_sha256 BLOB NOT NULL CHECK (length(scope_sha256) = 32),
                   provider_id TEXT NOT NULL CHECK (length(provider_id) BETWEEN 1 AND 128),
                   model_id TEXT NOT NULL CHECK (length(model_id) BETWEEN 1 AND 128),
                   registration_grant_revision INTEGER NOT NULL
                     CHECK (registration_grant_revision > 0),
                   cloud_disclosure_grant_revision INTEGER NOT NULL
                     CHECK (cloud_disclosure_grant_revision > 0),
                   publication_grant_revision INTEGER NOT NULL
                     CHECK (publication_grant_revision > 0),
                   emergency_pause_revision INTEGER NOT NULL
                     CHECK (emergency_pause_revision >= 0),
                   queue_revision INTEGER NOT NULL CHECK (queue_revision >= 0),
                   claimed_at_ms INTEGER NOT NULL CHECK (claimed_at_ms > 0),
                   FOREIGN KEY (feature_id, specification_revision)
                     REFERENCES feature_specification_revisions(feature_id, revision)
                 );
                 CREATE TRIGGER feature_repository_snapshot_claims_no_update
                   BEFORE UPDATE ON feature_repository_snapshot_claims
                   BEGIN SELECT RAISE(ABORT, 'immutable repository snapshot claim'); END;
                 CREATE TRIGGER feature_repository_snapshot_claims_no_delete
                   BEFORE DELETE ON feature_repository_snapshot_claims
                   BEGIN SELECT RAISE(ABORT, 'durable repository snapshot claim evidence'); END;
                 ALTER TABLE feature_active_lease ADD COLUMN snapshot_id TEXT
                   REFERENCES feature_repository_snapshot_claims(snapshot_id);
                 CREATE TRIGGER feature_active_lease_requires_snapshot
                   BEFORE INSERT ON feature_active_lease
                   WHEN NEW.snapshot_id IS NULL OR NOT EXISTS (
                     SELECT 1 FROM feature_repository_snapshot_claims claim
                     WHERE claim.snapshot_id = NEW.snapshot_id
                       AND claim.feature_id = NEW.feature_id
                       AND claim.lease_id = NEW.lease_id
                   )
                   BEGIN SELECT RAISE(ABORT, 'active feature lease requires snapshot claim'); END;
                 PRAGMA user_version = 9;
                 COMMIT;",
            )?;
        }
        let version = self.schema_version()?;
        if version == 9 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE feature_coding_dispatches (
                   packet_id TEXT PRIMARY KEY NOT NULL,
                   feature_id TEXT NOT NULL REFERENCES feature_conveyor_features(feature_id),
                   specification_revision INTEGER NOT NULL CHECK (specification_revision > 0),
                   lifecycle_revision INTEGER NOT NULL CHECK (lifecycle_revision > 0),
                   feature_lease_id TEXT NOT NULL,
                   snapshot_id TEXT NOT NULL REFERENCES feature_repository_snapshot_claims(snapshot_id),
                   snapshot_sha256 BLOB NOT NULL CHECK (length(snapshot_sha256) = 32),
                   work_packet_sha256 BLOB NOT NULL CHECK (length(work_packet_sha256) = 32),
                   work_packet_metadata_json TEXT NOT NULL,
                   device_id TEXT NOT NULL REFERENCES master_devices(device_id),
                   device_registry_revision INTEGER NOT NULL CHECK (device_registry_revision > 0),
                   queue_revision INTEGER NOT NULL CHECK (queue_revision >= 0),
                   emergency_pause_revision INTEGER NOT NULL CHECK (emergency_pause_revision >= 0),
                   task_id TEXT NOT NULL,
                   step_id TEXT NOT NULL UNIQUE REFERENCES master_steps(step_id),
                   dispatched_at_ms INTEGER NOT NULL CHECK (dispatched_at_ms > 0),
                   FOREIGN KEY (feature_id, specification_revision)
                     REFERENCES feature_specification_revisions(feature_id, revision),
                   UNIQUE (feature_id, packet_id),
                   UNIQUE (task_id, step_id)
                 );
                 CREATE TRIGGER feature_coding_dispatches_no_update
                   BEFORE UPDATE ON feature_coding_dispatches
                   BEGIN SELECT RAISE(ABORT, 'immutable feature coding dispatch'); END;
                 CREATE TRIGGER feature_coding_dispatches_no_delete
                   BEFORE DELETE ON feature_coding_dispatches
                   BEGIN SELECT RAISE(ABORT, 'durable feature coding dispatch evidence'); END;
                 PRAGMA user_version = 10;
                 COMMIT;",
            )?;
        }
        let version = self.schema_version()?;
        if version == 10 {
            let tx = self
                .connection
                .transaction_with_behavior(TransactionBehavior::Immediate)?;
            backfill_legacy_feature_resolution_evidence_tx(&tx)?;
            tx.execute_batch("PRAGMA user_version = 11;")?;
            tx.commit()?;
        }
        let version = self.schema_version()?;
        if version == 11 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE feature_result_artifacts (
                   artifact_id TEXT PRIMARY KEY NOT NULL,
                   artifact_sha256 BLOB NOT NULL CHECK (length(artifact_sha256) = 32),
                   artifact_size_bytes INTEGER NOT NULL
                     CHECK (artifact_size_bytes BETWEEN 1 AND 65536),
                   device_id TEXT NOT NULL REFERENCES master_devices(device_id),
                   device_registry_revision INTEGER NOT NULL CHECK (device_registry_revision > 0),
                   connection_epoch INTEGER NOT NULL CHECK (connection_epoch > 0),
                   sequence INTEGER NOT NULL CHECK (sequence > 0),
                   task_id TEXT NOT NULL,
                   step_id TEXT NOT NULL REFERENCES master_steps(step_id),
                   attempt_id TEXT NOT NULL UNIQUE REFERENCES master_attempts(attempt_id),
                   lease_id TEXT NOT NULL,
                   cancellation_id TEXT NOT NULL,
                   context_sha256 BLOB NOT NULL CHECK (length(context_sha256) = 32),
                   feature_id TEXT NOT NULL REFERENCES feature_conveyor_features(feature_id),
                   feature_lease_id TEXT NOT NULL,
                   snapshot_id TEXT NOT NULL REFERENCES feature_repository_snapshot_claims(snapshot_id),
                   snapshot_sha256 BLOB NOT NULL CHECK (length(snapshot_sha256) = 32),
                   work_packet_sha256 BLOB NOT NULL CHECK (length(work_packet_sha256) = 32),
                   admitted_at_ms INTEGER NOT NULL CHECK (admitted_at_ms > 0),
                   UNIQUE (task_id, step_id, attempt_id, lease_id, cancellation_id)
                 );
                 CREATE TRIGGER feature_result_artifacts_no_update
                   BEFORE UPDATE ON feature_result_artifacts
                   BEGIN SELECT RAISE(ABORT, 'immutable feature result artifact'); END;
                 CREATE TRIGGER feature_result_artifacts_no_delete
                   BEFORE DELETE ON feature_result_artifacts
                   BEGIN SELECT RAISE(ABORT, 'durable feature result artifact evidence'); END;
                 PRAGMA user_version = 12;
                 COMMIT;",
            )?;
        }
        let version = self.schema_version()?;
        if version == 12 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 ALTER TABLE feature_result_artifacts
                   ADD COLUMN workspace_retained INTEGER NOT NULL DEFAULT 0
                     CHECK (workspace_retained IN (0, 1));
                 ALTER TABLE feature_result_artifacts
                   ADD COLUMN workspace_expires_at_ms INTEGER
                     CHECK (workspace_expires_at_ms IS NULL OR workspace_expires_at_ms > 0);
                 PRAGMA user_version = 13;
                 COMMIT;",
            )?;
        }
        let version = self.schema_version()?;
        if version == 13 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE feature_artifact_integrations (
                   integration_id TEXT PRIMARY KEY NOT NULL,
                   feature_id TEXT NOT NULL REFERENCES feature_conveyor_features(feature_id),
                   specification_revision INTEGER NOT NULL CHECK(specification_revision>0),
                   lifecycle_revision INTEGER NOT NULL CHECK(lifecycle_revision>0),
                   feature_lease_id TEXT NOT NULL,
                   snapshot_id TEXT NOT NULL REFERENCES feature_repository_snapshot_claims(snapshot_id),
                   snapshot_sha256 BLOB NOT NULL CHECK(length(snapshot_sha256)=32),
                   artifact_set_sha256 BLOB NOT NULL CHECK(length(artifact_set_sha256)=32),
                   candidate_commit TEXT NOT NULL CHECK(length(candidate_commit)=40),
                   candidate_tree TEXT NOT NULL CHECK(length(candidate_tree)=40),
                   base_commit TEXT NOT NULL CHECK(length(base_commit)=40),
                   queue_revision INTEGER NOT NULL CHECK(queue_revision>=0),
                   emergency_pause_revision INTEGER NOT NULL CHECK(emergency_pause_revision>=0),
                   registration_grant_revision INTEGER NOT NULL CHECK(registration_grant_revision>0),
                   cloud_disclosure_grant_revision INTEGER NOT NULL CHECK(cloud_disclosure_grant_revision>0),
                   publication_grant_revision INTEGER NOT NULL CHECK(publication_grant_revision>0),
                   integrated_at_ms INTEGER NOT NULL CHECK(integrated_at_ms>0),
                   UNIQUE(feature_id, specification_revision, feature_lease_id, snapshot_id)
                 );
                 CREATE TRIGGER feature_artifact_integrations_no_update BEFORE UPDATE ON feature_artifact_integrations
                   BEGIN SELECT RAISE(ABORT,'immutable artifact integration'); END;
                 CREATE TRIGGER feature_artifact_integrations_no_delete BEFORE DELETE ON feature_artifact_integrations
                   BEGIN SELECT RAISE(ABORT,'durable artifact integration evidence'); END;
                 CREATE TABLE feature_artifact_integration_artifacts (
                   integration_id TEXT NOT NULL REFERENCES feature_artifact_integrations(integration_id),
                   artifact_id TEXT NOT NULL REFERENCES feature_result_artifacts(artifact_id),
                   ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 1 AND 65535),
                   packet_id TEXT NOT NULL,
                   position INTEGER NOT NULL CHECK(position BETWEEN 1 AND 3),
                   PRIMARY KEY(integration_id, artifact_id),
                   UNIQUE(integration_id, ordinal),
                   UNIQUE(integration_id, packet_id),
                   UNIQUE(integration_id, position)
                 );
                 CREATE TRIGGER feature_artifact_integration_artifacts_no_update BEFORE UPDATE ON feature_artifact_integration_artifacts
                   BEGIN SELECT RAISE(ABORT,'immutable integrated artifact linkage'); END;
                 CREATE TRIGGER feature_artifact_integration_artifacts_no_delete BEFORE DELETE ON feature_artifact_integration_artifacts
                   BEGIN SELECT RAISE(ABORT,'durable integrated artifact linkage'); END;
                 CREATE TABLE feature_artifact_integration_conflicts (
                   integration_id TEXT NOT NULL,
                   feature_id TEXT NOT NULL,
                   lifecycle_revision INTEGER NOT NULL,
                   request_binding_sha256 BLOB NOT NULL CHECK(length(request_binding_sha256)=32),
                   reason_code TEXT NOT NULL CHECK(reason_code IN('content_cas_mismatch','overlapping_path','duplicate_ordinal')),
                   recorded_at_ms INTEGER NOT NULL CHECK(recorded_at_ms>0),
                   PRIMARY KEY(integration_id, reason_code)
                 );
                 CREATE TRIGGER feature_artifact_integration_conflicts_no_update BEFORE UPDATE ON feature_artifact_integration_conflicts
                   BEGIN SELECT RAISE(ABORT,'immutable artifact integration conflict'); END;
                 CREATE TRIGGER feature_artifact_integration_conflicts_no_delete BEFORE DELETE ON feature_artifact_integration_conflicts
                   BEGIN SELECT RAISE(ABORT,'durable artifact integration conflict'); END;
                 PRAGMA user_version=14;
                 COMMIT;"
            )?;
        }
        let version = self.schema_version()?;
        if version == 14 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE feature_validation_attempts (
                   validation_id TEXT PRIMARY KEY NOT NULL,
                   feature_id TEXT NOT NULL REFERENCES feature_conveyor_features(feature_id),
                   specification_revision INTEGER NOT NULL CHECK(specification_revision>0),
                   lifecycle_revision INTEGER NOT NULL CHECK(lifecycle_revision>0),
                   feature_lease_id TEXT NOT NULL,
                   snapshot_id TEXT NOT NULL REFERENCES feature_repository_snapshot_claims(snapshot_id),
                   snapshot_sha256 BLOB NOT NULL CHECK(length(snapshot_sha256)=32),
                   integration_id TEXT NOT NULL REFERENCES feature_artifact_integrations(integration_id),
                   artifact_set_sha256 BLOB NOT NULL CHECK(length(artifact_set_sha256)=32),
                   candidate_commit TEXT NOT NULL CHECK(length(candidate_commit)=40),
                   candidate_tree TEXT NOT NULL CHECK(length(candidate_tree)=40),
                   base_commit TEXT NOT NULL CHECK(length(base_commit)=40),
                   plan_sha256 BLOB NOT NULL CHECK(length(plan_sha256)=32),
                   command_ids_json TEXT NOT NULL CHECK(length(command_ids_json) BETWEEN 2 AND 1024),
                   queue_revision INTEGER NOT NULL CHECK(queue_revision>=0),
                   emergency_pause_revision INTEGER NOT NULL CHECK(emergency_pause_revision>=0),
                   registration_grant_revision INTEGER NOT NULL CHECK(registration_grant_revision>0),
                   cloud_disclosure_grant_revision INTEGER NOT NULL CHECK(cloud_disclosure_grant_revision>0),
                   publication_grant_revision INTEGER NOT NULL CHECK(publication_grant_revision>0),
                   request_binding_sha256 BLOB NOT NULL CHECK(length(request_binding_sha256)=32),
                   started_at_ms INTEGER NOT NULL CHECK(started_at_ms>0),
                   UNIQUE(feature_id,specification_revision,feature_lease_id,integration_id)
                 );
                 CREATE TRIGGER feature_validation_attempts_no_update
                   BEFORE UPDATE ON feature_validation_attempts
                   BEGIN SELECT RAISE(ABORT,'immutable validation attempt'); END;
                 CREATE TRIGGER feature_validation_attempts_no_delete
                   BEFORE DELETE ON feature_validation_attempts
                   BEGIN SELECT RAISE(ABORT,'durable validation attempt'); END;
                 CREATE TABLE feature_validation_command_evidence (
                   validation_id TEXT NOT NULL REFERENCES feature_validation_attempts(validation_id),
                   command_id TEXT NOT NULL CHECK(length(command_id) BETWEEN 3 AND 64),
                   passed INTEGER NOT NULL CHECK(passed IN(0,1)),
                   result_sha256 BLOB NOT NULL CHECK(length(result_sha256)=32),
                   duration_ms INTEGER NOT NULL CHECK(duration_ms>=0),
                   output_truncated INTEGER NOT NULL CHECK(output_truncated IN(0,1)),
                   PRIMARY KEY(validation_id,command_id)
                 );
                 CREATE TRIGGER feature_validation_command_evidence_no_update
                   BEFORE UPDATE ON feature_validation_command_evidence
                   BEGIN SELECT RAISE(ABORT,'immutable validation command evidence'); END;
                 CREATE TRIGGER feature_validation_command_evidence_no_delete
                   BEFORE DELETE ON feature_validation_command_evidence
                   BEGIN SELECT RAISE(ABORT,'durable validation command evidence'); END;
                 CREATE TABLE feature_validation_completions (
                   validation_id TEXT PRIMARY KEY NOT NULL REFERENCES feature_validation_attempts(validation_id),
                   passed INTEGER NOT NULL CHECK(passed IN(0,1)),
                   evidence_manifest_sha256 BLOB NOT NULL CHECK(length(evidence_manifest_sha256)=32),
                   lifecycle_revision INTEGER NOT NULL CHECK(lifecycle_revision>0),
                   completed_at_ms INTEGER NOT NULL CHECK(completed_at_ms>0)
                 );
                 CREATE TRIGGER feature_validation_completions_no_update
                   BEFORE UPDATE ON feature_validation_completions
                   BEGIN SELECT RAISE(ABORT,'immutable validation completion'); END;
                 CREATE TRIGGER feature_validation_completions_no_delete
                   BEFORE DELETE ON feature_validation_completions
                   BEGIN SELECT RAISE(ABORT,'durable validation completion'); END;
                 PRAGMA user_version=15;
                 COMMIT;",
            )?;
        }
        let version = self.schema_version()?;
        if version == 15 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE feature_review_calls (
                   review_call_id TEXT PRIMARY KEY NOT NULL,
                   feature_id TEXT NOT NULL REFERENCES feature_conveyor_features(feature_id),
                   specification_revision INTEGER NOT NULL CHECK(specification_revision>0),
                   lifecycle_revision INTEGER NOT NULL CHECK(lifecycle_revision>0),
                   feature_lease_id TEXT NOT NULL,
                   integration_id TEXT NOT NULL REFERENCES feature_artifact_integrations(integration_id),
                   validation_id TEXT NOT NULL REFERENCES feature_validation_attempts(validation_id),
                   candidate_commit TEXT NOT NULL CHECK(length(candidate_commit)=40),
                   candidate_tree TEXT NOT NULL CHECK(length(candidate_tree)=40),
                   base_commit TEXT NOT NULL CHECK(length(base_commit)=40),
                   candidate_diff_sha256 BLOB NOT NULL CHECK(length(candidate_diff_sha256)=32),
                   evidence_manifest_sha256 BLOB NOT NULL CHECK(length(evidence_manifest_sha256)=32),
                   review_packet_sha256 BLOB NOT NULL CHECK(length(review_packet_sha256)=32),
                   provider_id TEXT NOT NULL CHECK(length(provider_id) BETWEEN 1 AND 128),
                   model_id TEXT NOT NULL CHECK(length(model_id) BETWEEN 1 AND 128),
                   candidate_attempt INTEGER NOT NULL CHECK(candidate_attempt BETWEEN 1 AND 3),
                   feature_call INTEGER NOT NULL CHECK(feature_call BETWEEN 1 AND 12),
                   queue_revision INTEGER NOT NULL CHECK(queue_revision>=0),
                   emergency_pause_revision INTEGER NOT NULL CHECK(emergency_pause_revision>=0),
                   registration_grant_revision INTEGER NOT NULL CHECK(registration_grant_revision>0),
                   cloud_disclosure_grant_revision INTEGER NOT NULL CHECK(cloud_disclosure_grant_revision>0),
                   publication_grant_revision INTEGER NOT NULL CHECK(publication_grant_revision>0),
                   request_binding_sha256 BLOB NOT NULL CHECK(length(request_binding_sha256)=32),
                   started_at_ms INTEGER NOT NULL CHECK(started_at_ms>0)
                 );
                 CREATE INDEX feature_review_calls_candidate_idx
                   ON feature_review_calls(feature_id,candidate_commit,candidate_attempt);
                 CREATE TRIGGER feature_review_calls_no_update BEFORE UPDATE ON feature_review_calls
                   BEGIN SELECT RAISE(ABORT,'immutable review call'); END;
                 CREATE TRIGGER feature_review_calls_no_delete BEFORE DELETE ON feature_review_calls
                   BEGIN SELECT RAISE(ABORT,'durable review call evidence'); END;
                 CREATE TABLE feature_review_call_outcomes (
                   review_call_id TEXT PRIMARY KEY NOT NULL REFERENCES feature_review_calls(review_call_id),
                   outcome_kind TEXT NOT NULL CHECK(outcome_kind IN(
                     'provider_outage','malformed_output','incomplete_transport','interrupted','decision'
                   )),
                   outcome_sha256 BLOB NOT NULL CHECK(length(outcome_sha256)=32),
                   next_retry_at_ms INTEGER CHECK(next_retry_at_ms IS NULL OR next_retry_at_ms>0),
                   completed_at_ms INTEGER NOT NULL CHECK(completed_at_ms>0),
                   CHECK(
                     (outcome_kind IN('decision','interrupted') AND next_retry_at_ms IS NULL)
                     OR (outcome_kind NOT IN('decision','interrupted') AND next_retry_at_ms IS NOT NULL)
                   )
                 );
                 CREATE TRIGGER feature_review_call_outcomes_no_update BEFORE UPDATE ON feature_review_call_outcomes
                   BEGIN SELECT RAISE(ABORT,'immutable review call outcome'); END;
                 CREATE TRIGGER feature_review_call_outcomes_no_delete BEFORE DELETE ON feature_review_call_outcomes
                   BEGIN SELECT RAISE(ABORT,'durable review call outcome evidence'); END;
                 CREATE TABLE feature_review_decisions (
                   review_call_id TEXT PRIMARY KEY NOT NULL REFERENCES feature_review_calls(review_call_id),
                   feature_id TEXT NOT NULL,
                   candidate_commit TEXT NOT NULL CHECK(length(candidate_commit)=40),
                   decision TEXT NOT NULL CHECK(decision IN('approved','rejected')),
                   decision_sha256 BLOB NOT NULL CHECK(length(decision_sha256)=32),
                   structured_result_json TEXT NOT NULL CHECK(
                     length(CAST(structured_result_json AS BLOB)) BETWEEN 2 AND 65536
                   ),
                   lifecycle_revision INTEGER NOT NULL CHECK(lifecycle_revision>0),
                   decided_at_ms INTEGER NOT NULL CHECK(decided_at_ms>0),
                   UNIQUE(feature_id,candidate_commit)
                 );
                 CREATE TRIGGER feature_review_decisions_no_update BEFORE UPDATE ON feature_review_decisions
                   BEGIN SELECT RAISE(ABORT,'immutable review decision'); END;
                 CREATE TRIGGER feature_review_decisions_no_delete BEFORE DELETE ON feature_review_decisions
                   BEGIN SELECT RAISE(ABORT,'durable review decision evidence'); END;
                 PRAGMA user_version=16;
                 COMMIT;"
            )?;
        }
        let version = self.schema_version()?;
        if version == 16 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE feature_publications (
                   publication_id TEXT PRIMARY KEY NOT NULL,
                   feature_id TEXT NOT NULL REFERENCES feature_conveyor_features(feature_id),
                   specification_revision INTEGER NOT NULL CHECK(specification_revision>0),
                   lifecycle_revision INTEGER NOT NULL CHECK(lifecycle_revision>0),
                   feature_lease_id TEXT NOT NULL,
                   integration_id TEXT NOT NULL REFERENCES feature_artifact_integrations(integration_id),
                   validation_id TEXT NOT NULL REFERENCES feature_validation_attempts(validation_id),
                   review_call_id TEXT NOT NULL REFERENCES feature_review_calls(review_call_id),
                   candidate_commit TEXT NOT NULL CHECK(length(candidate_commit)=40),
                   candidate_tree TEXT NOT NULL CHECK(length(candidate_tree)=40),
                   candidate_diff_sha256 BLOB NOT NULL CHECK(length(candidate_diff_sha256)=32),
                   evidence_manifest_sha256 BLOB NOT NULL CHECK(length(evidence_manifest_sha256)=32),
                   review_decision_sha256 BLOB NOT NULL CHECK(length(review_decision_sha256)=32),
                   provider_id TEXT NOT NULL CHECK(length(provider_id) BETWEEN 1 AND 128),
                   model_id TEXT NOT NULL CHECK(length(model_id) BETWEEN 1 AND 128),
                   repository_id TEXT NOT NULL,
                   feature_branch TEXT NOT NULL CHECK(length(feature_branch) BETWEEN 1 AND 255),
                   base_branch TEXT NOT NULL CHECK(length(base_branch) BETWEEN 1 AND 255),
                   remote_base_commit TEXT NOT NULL CHECK(length(remote_base_commit)=40),
                   branch_policy_sha256 BLOB NOT NULL CHECK(length(branch_policy_sha256)=32),
                   required_checks_json TEXT NOT NULL CHECK(length(required_checks_json) BETWEEN 2 AND 8192),
                   merge_strategy TEXT NOT NULL CHECK(merge_strategy IN('merge','squash','rebase')),
                   post_merge_gate TEXT NOT NULL CHECK(post_merge_gate='release-local'),
                   queue_revision INTEGER NOT NULL CHECK(queue_revision>=0),
                   emergency_pause_revision INTEGER NOT NULL CHECK(emergency_pause_revision>=0),
                   registration_grant_revision INTEGER NOT NULL CHECK(registration_grant_revision>0),
                   cloud_disclosure_grant_revision INTEGER NOT NULL CHECK(cloud_disclosure_grant_revision>0),
                   publication_grant_revision INTEGER NOT NULL CHECK(publication_grant_revision>0),
                   request_binding_sha256 BLOB NOT NULL CHECK(length(request_binding_sha256)=32),
                   started_at_ms INTEGER NOT NULL CHECK(started_at_ms>0),
                   UNIQUE(feature_id,candidate_commit)
                 );
                 CREATE TRIGGER feature_publications_no_update BEFORE UPDATE ON feature_publications
                   BEGIN SELECT RAISE(ABORT,'immutable publication'); END;
                 CREATE TRIGGER feature_publications_no_delete BEFORE DELETE ON feature_publications
                   BEGIN SELECT RAISE(ABORT,'durable publication'); END;
                 CREATE TABLE feature_publication_action_intents (
                   publication_id TEXT NOT NULL REFERENCES feature_publications(publication_id),
                   ordinal INTEGER NOT NULL CHECK(ordinal BETWEEN 1 AND 7),
                   action_kind TEXT NOT NULL CHECK(action_kind IN(
                     'push_branch','upsert_pull_request','observe_required_checks',
                     'verify_pull_request_head','merge_pull_request','reconcile_remote_main',
                     'run_post_merge_gate'
                   )),
                   intent_sha256 BLOB NOT NULL CHECK(length(intent_sha256)=32),
                   created_at_ms INTEGER NOT NULL CHECK(created_at_ms>0),
                   PRIMARY KEY(publication_id,ordinal),
                   UNIQUE(publication_id,action_kind)
                 );
                 CREATE TRIGGER feature_publication_action_intents_no_update BEFORE UPDATE ON feature_publication_action_intents
                   BEGIN SELECT RAISE(ABORT,'immutable publication action intent'); END;
                 CREATE TRIGGER feature_publication_action_intents_no_delete BEFORE DELETE ON feature_publication_action_intents
                   BEGIN SELECT RAISE(ABORT,'durable publication action intent'); END;
                 CREATE TABLE feature_publication_action_outcomes (
                   publication_id TEXT NOT NULL,
                   ordinal INTEGER NOT NULL,
                   action_kind TEXT NOT NULL,
                   evidence_sha256 BLOB NOT NULL CHECK(length(evidence_sha256)=32),
                   pull_request_number INTEGER CHECK(pull_request_number IS NULL OR pull_request_number>0),
                   observed_commit TEXT CHECK(observed_commit IS NULL OR length(observed_commit)=40),
                   merge_commit TEXT CHECK(merge_commit IS NULL OR length(merge_commit)=40),
                   passed INTEGER NOT NULL CHECK(passed IN(0,1)),
                   structured_evidence_json TEXT NOT NULL CHECK(
                     length(CAST(structured_evidence_json AS BLOB)) BETWEEN 2 AND 32768
                   ),
                   completed_at_ms INTEGER NOT NULL CHECK(completed_at_ms>0),
                   PRIMARY KEY(publication_id,ordinal),
                   FOREIGN KEY(publication_id,ordinal)
                     REFERENCES feature_publication_action_intents(publication_id,ordinal)
                 );
                 CREATE TRIGGER feature_publication_action_outcomes_no_update BEFORE UPDATE ON feature_publication_action_outcomes
                   BEGIN SELECT RAISE(ABORT,'immutable publication action outcome'); END;
                 CREATE TRIGGER feature_publication_action_outcomes_no_delete BEFORE DELETE ON feature_publication_action_outcomes
                   BEGIN SELECT RAISE(ABORT,'durable publication action outcome'); END;
                 CREATE TABLE feature_publication_completions (
                   publication_id TEXT PRIMARY KEY NOT NULL REFERENCES feature_publications(publication_id),
                   merge_commit TEXT NOT NULL CHECK(length(merge_commit)=40),
                   remote_main_commit TEXT NOT NULL CHECK(length(remote_main_commit)=40),
                   post_merge_evidence_sha256 BLOB NOT NULL CHECK(length(post_merge_evidence_sha256)=32),
                   lifecycle_revision INTEGER NOT NULL CHECK(lifecycle_revision>0),
                   queue_revision INTEGER NOT NULL CHECK(queue_revision>0),
                   completed_at_ms INTEGER NOT NULL CHECK(completed_at_ms>0),
                   CHECK(merge_commit=remote_main_commit)
                 );
                 CREATE TRIGGER feature_publication_completions_no_update BEFORE UPDATE ON feature_publication_completions
                   BEGIN SELECT RAISE(ABORT,'immutable publication completion'); END;
                 CREATE TRIGGER feature_publication_completions_no_delete BEFORE DELETE ON feature_publication_completions
                   BEGIN SELECT RAISE(ABORT,'durable publication completion'); END;
                 PRAGMA user_version=17;
                 COMMIT;"
            )?;
        }
        let version = self.schema_version()?;
        if version == 17 {
            self.connection.execute_batch(
                "PRAGMA foreign_keys=OFF;
                 PRAGMA legacy_alter_table=ON;
                 BEGIN IMMEDIATE;
                 ALTER TABLE feature_conveyor_features RENAME TO feature_conveyor_features_v17;
                 CREATE TABLE feature_conveyor_features (
                   feature_id TEXT PRIMARY KEY NOT NULL,
                   current_specification_revision INTEGER NOT NULL
                     CHECK(current_specification_revision>0),
                   status TEXT NOT NULL CHECK(status IN(
                     'queued','implementing','validating','reviewing','publishing',
                     'verifying_main','repairing','paused','attention_required',
                     'failed','succeeded','cancelled','abandoned','quarantined'
                   )),
                   lifecycle_revision INTEGER NOT NULL CHECK(lifecycle_revision>0),
                   queue_position INTEGER NOT NULL CHECK(queue_position>0),
                   effect_possible INTEGER NOT NULL CHECK(effect_possible IN(0,1)),
                   created_at_ms INTEGER NOT NULL CHECK(created_at_ms>=0),
                   updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>=created_at_ms),
                   FOREIGN KEY(feature_id,current_specification_revision)
                     REFERENCES feature_specification_revisions(feature_id,revision)
                 );
                 INSERT INTO feature_conveyor_features
                   SELECT * FROM feature_conveyor_features_v17;
                 DROP TABLE feature_conveyor_features_v17;
                 CREATE INDEX feature_conveyor_features_status_idx
                   ON feature_conveyor_features(status,queue_position);
                 DROP TRIGGER IF EXISTS feature_orchestration_checkpoints_no_delete;
                 DROP TRIGGER IF EXISTS feature_orchestration_checkpoints_no_update;
                 DROP TRIGGER IF EXISTS feature_orchestration_activation_no_delete;
                 DROP TRIGGER IF EXISTS feature_orchestration_activation_no_update;
                 DROP TABLE IF EXISTS feature_orchestration_checkpoints;
                 DROP TABLE IF EXISTS feature_orchestration_state;
                 DROP TABLE IF EXISTS feature_orchestration_activation;
                 CREATE TABLE feature_orchestration_activation (
                   singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
                   activation_id TEXT NOT NULL UNIQUE,
                   owner_evidence_sha256 BLOB NOT NULL CHECK(length(owner_evidence_sha256)=32),
                   live_evidence_sha256 BLOB NOT NULL CHECK(length(live_evidence_sha256)=32),
                   activated_at_ms INTEGER NOT NULL CHECK(activated_at_ms>0)
                 );
                 CREATE TRIGGER feature_orchestration_activation_no_update
                   BEFORE UPDATE ON feature_orchestration_activation
                   BEGIN SELECT RAISE(ABORT,'immutable orchestration activation'); END;
                 CREATE TRIGGER feature_orchestration_activation_no_delete
                   BEFORE DELETE ON feature_orchestration_activation
                   BEGIN SELECT RAISE(ABORT,'durable orchestration activation'); END;
                 CREATE TABLE feature_orchestration_state (
                   feature_id TEXT PRIMARY KEY NOT NULL REFERENCES feature_conveyor_features(feature_id),
                   orchestration_revision INTEGER NOT NULL CHECK(orchestration_revision>0),
                   checkpoint_id TEXT NOT NULL UNIQUE,
                   stage TEXT NOT NULL CHECK(stage IN(
                     'implementing','validating','reviewing','publishing','verifying_main',
                     'repairing','paused','attention_required','failed','succeeded','quarantined'
                   )),
                   resume_stage TEXT CHECK(resume_stage IS NULL OR resume_stage IN(
                     'implementing','validating','reviewing','publishing','verifying_main','repairing'
                   )),
                   pause_kind TEXT CHECK(pause_kind IS NULL OR pause_kind IN(
                     'provider','worker','maintenance','owner'
                   )),
                   replacement_candidates_used INTEGER NOT NULL
                     CHECK(replacement_candidates_used BETWEEN 0 AND 3),
                   active_processing_ms INTEGER NOT NULL
                     CHECK(active_processing_ms BETWEEN 0 AND 86400000),
                   clock_started_at_ms INTEGER CHECK(clock_started_at_ms IS NULL OR clock_started_at_ms>0),
                   next_retry_at_ms INTEGER CHECK(next_retry_at_ms IS NULL OR next_retry_at_ms>0),
                   effect_possible INTEGER NOT NULL CHECK(effect_possible IN(0,1)),
                   updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>0),
                   CHECK((stage='paused')=(pause_kind IS NOT NULL)),
                   CHECK((stage='paused')=(resume_stage IS NOT NULL)),
                   CHECK(stage='paused' OR next_retry_at_ms IS NULL)
                 );
                 CREATE TABLE feature_orchestration_checkpoints (
                   checkpoint_id TEXT PRIMARY KEY NOT NULL,
                   feature_id TEXT NOT NULL REFERENCES feature_conveyor_features(feature_id),
                   orchestration_revision INTEGER NOT NULL CHECK(orchestration_revision>0),
                   lifecycle_revision INTEGER NOT NULL CHECK(lifecycle_revision>0),
                   stage TEXT NOT NULL CHECK(stage IN(
                     'implementing','validating','reviewing','publishing','verifying_main',
                     'repairing','paused','attention_required','failed','succeeded','quarantined'
                   )),
                   action TEXT NOT NULL CHECK(length(action) BETWEEN 3 AND 64),
                   reason TEXT NOT NULL CHECK(length(reason) BETWEEN 3 AND 64),
                   checkpoint_sha256 BLOB NOT NULL UNIQUE CHECK(length(checkpoint_sha256)=32),
                   evidence_sha256 BLOB CHECK(evidence_sha256 IS NULL OR length(evidence_sha256)=32),
                   replacement_candidates_used INTEGER NOT NULL
                     CHECK(replacement_candidates_used BETWEEN 0 AND 3),
                   active_processing_ms INTEGER NOT NULL
                     CHECK(active_processing_ms BETWEEN 0 AND 86400000),
                   effect_possible INTEGER NOT NULL CHECK(effect_possible IN(0,1)),
                   recorded_at_ms INTEGER NOT NULL CHECK(recorded_at_ms>0),
                   UNIQUE(feature_id,orchestration_revision)
                 );
                 CREATE TRIGGER feature_orchestration_checkpoints_no_update
                   BEFORE UPDATE ON feature_orchestration_checkpoints
                   BEGIN SELECT RAISE(ABORT,'immutable orchestration checkpoint'); END;
                 CREATE TRIGGER feature_orchestration_checkpoints_no_delete
                   BEFORE DELETE ON feature_orchestration_checkpoints
                   BEGIN SELECT RAISE(ABORT,'durable orchestration checkpoint'); END;
                 CREATE INDEX feature_orchestration_checkpoints_feature_idx
                   ON feature_orchestration_checkpoints(feature_id,orchestration_revision);
                 PRAGMA user_version=18;
                 COMMIT;
                 PRAGMA legacy_alter_table=OFF;
                 PRAGMA foreign_keys=ON;"
            )?;
            let violations: i64 = self.connection.query_row(
                "SELECT COUNT(*) FROM pragma_foreign_key_check",
                [],
                |row| row.get(0),
            )?;
            if violations != 0 {
                return Err(MasterError::InvalidStoredState(
                    "schema-v18 migration produced foreign-key violations".to_string(),
                ));
            }
        }
        let version = self.schema_version()?;
        if version == 18 {
            let legacy_activations: i64 = self.connection.query_row(
                "SELECT COUNT(*) FROM feature_orchestration_activation",
                [],
                |row| row.get(0),
            )?;
            if legacy_activations != 0 {
                return Err(MasterError::InvalidStoredState(
                    "schema-v18 orchestration activation had no authoritative writer".to_string(),
                ));
            }
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 DROP TRIGGER feature_orchestration_activation_no_update;
                 DROP TRIGGER feature_orchestration_activation_no_delete;
                 DROP TABLE feature_orchestration_activation;
                 CREATE TABLE feature_activation_evidence (
                   category TEXT NOT NULL CHECK(category IN(
                     'repository_gate_proof','restricted_worker_live','review_provider_live',
                     'github_publication_live','restart_recovery_live',
                     'mac_windows_control_event_streaming_live'
                   )),
                   revision INTEGER NOT NULL CHECK(revision>0),
                   evidence_id TEXT NOT NULL UNIQUE,
                   origin TEXT NOT NULL CHECK(origin IN(
                     'repository_gate_proof_controller','restricted_worker_proof_controller',
                     'review_provider_proof_controller','github_publication_proof_controller',
                     'restart_recovery_proof_controller',
                     'mac_windows_control_event_streaming_proof_controller'
                   )),
                   receipt_sha256 BLOB NOT NULL CHECK(length(receipt_sha256)=32),
                   observed_at_ms INTEGER NOT NULL CHECK(observed_at_ms>0),
                   emergency_pause_revision INTEGER NOT NULL CHECK(emergency_pause_revision>=0),
                   recorded_at_ms INTEGER NOT NULL CHECK(recorded_at_ms>0),
                   PRIMARY KEY(category,revision)
                 );
                 CREATE TRIGGER feature_activation_evidence_no_update
                   BEFORE UPDATE ON feature_activation_evidence
                   BEGIN SELECT RAISE(ABORT,'immutable activation evidence'); END;
                 CREATE TRIGGER feature_activation_evidence_no_delete
                   BEFORE DELETE ON feature_activation_evidence
                   BEGIN SELECT RAISE(ABORT,'durable activation evidence'); END;
                 CREATE TABLE feature_orchestration_activation (
                   singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
                   activation_id TEXT NOT NULL UNIQUE,
                   queue_revision INTEGER NOT NULL CHECK(queue_revision>=0),
                   owner_control_designation_revision INTEGER NOT NULL
                     CHECK(owner_control_designation_revision>0),
                   emergency_pause_revision INTEGER NOT NULL CHECK(emergency_pause_revision>=0),
                   repository_gate_evidence_id TEXT NOT NULL UNIQUE
                     REFERENCES feature_activation_evidence(evidence_id),
                   restricted_worker_evidence_id TEXT NOT NULL UNIQUE
                     REFERENCES feature_activation_evidence(evidence_id),
                   review_provider_evidence_id TEXT NOT NULL UNIQUE
                     REFERENCES feature_activation_evidence(evidence_id),
                   github_publication_evidence_id TEXT NOT NULL UNIQUE
                     REFERENCES feature_activation_evidence(evidence_id),
                   restart_recovery_evidence_id TEXT NOT NULL UNIQUE
                     REFERENCES feature_activation_evidence(evidence_id),
                   control_event_streaming_evidence_id TEXT NOT NULL UNIQUE
                     REFERENCES feature_activation_evidence(evidence_id),
                   activated_at_ms INTEGER NOT NULL CHECK(activated_at_ms>0)
                 );
                 CREATE TRIGGER feature_orchestration_activation_no_update
                   BEFORE UPDATE ON feature_orchestration_activation
                   BEGIN SELECT RAISE(ABORT,'immutable orchestration activation'); END;
                 CREATE TRIGGER feature_orchestration_activation_no_delete
                   BEFORE DELETE ON feature_orchestration_activation
                   BEGIN SELECT RAISE(ABORT,'durable orchestration activation'); END;
                 CREATE TABLE feature_owner_orchestration_controls (
                   request_sha256 BLOB PRIMARY KEY NOT NULL CHECK(length(request_sha256)=32),
                   action TEXT NOT NULL CHECK(action IN('pause','resume')),
                   feature_id TEXT NOT NULL,
                   lifecycle_revision INTEGER NOT NULL CHECK(lifecycle_revision>0),
                   orchestration_revision INTEGER NOT NULL CHECK(orchestration_revision>0),
                   queue_revision INTEGER NOT NULL CHECK(queue_revision>=0),
                   owner_control_designation_revision INTEGER NOT NULL
                     CHECK(owner_control_designation_revision>0),
                   emergency_pause_revision INTEGER NOT NULL CHECK(emergency_pause_revision>=0),
                   checkpoint_id TEXT NOT NULL UNIQUE,
                   checkpoint_sha256 BLOB NOT NULL UNIQUE CHECK(length(checkpoint_sha256)=32),
                   recorded_at_ms INTEGER NOT NULL CHECK(recorded_at_ms>0)
                 );
                 CREATE TRIGGER feature_owner_orchestration_controls_no_update
                   BEFORE UPDATE ON feature_owner_orchestration_controls
                   BEGIN SELECT RAISE(ABORT,'immutable owner orchestration control'); END;
                 CREATE TRIGGER feature_owner_orchestration_controls_no_delete
                   BEFORE DELETE ON feature_owner_orchestration_controls
                   BEGIN SELECT RAISE(ABORT,'durable owner orchestration control'); END;
                 PRAGMA user_version=19;
                 COMMIT;",
            )?;
        }
        let version = self.schema_version()?;
        if version == 19 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 CREATE TABLE assembly_line_state (
                   singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
                   owner_control_revision INTEGER NOT NULL CHECK(owner_control_revision>0),
                   state_revision INTEGER NOT NULL CHECK(state_revision>0),
                   queue_revision INTEGER NOT NULL CHECK(queue_revision>=0),
                   auto_run INTEGER NOT NULL CHECK(auto_run IN(0,1)),
                   lifecycle TEXT NOT NULL CHECK(lifecycle='stopped')
                 );
                 INSERT INTO assembly_line_state
                   (singleton,owner_control_revision,state_revision,queue_revision,auto_run,lifecycle)
                   VALUES(1,1,1,0,1,'stopped');
                 CREATE TABLE assembly_line_project_drafts (
                   draft_id TEXT PRIMARY KEY NOT NULL,
                   draft_revision INTEGER NOT NULL CHECK(draft_revision>0),
                   repository_id TEXT NOT NULL,
                   git_url TEXT NOT NULL,
                   visibility TEXT NOT NULL CHECK(visibility IN('public','private')),
                   request_sha256 BLOB NOT NULL UNIQUE CHECK(length(request_sha256)=32),
                   canonical_json TEXT NOT NULL CHECK(length(CAST(canonical_json AS BLOB)) BETWEEN 2 AND 16384),
                   recorded_at_ms INTEGER NOT NULL CHECK(recorded_at_ms>0)
                 );
                 CREATE TABLE assembly_line_feature_drafts (
                   draft_id TEXT PRIMARY KEY NOT NULL,
                   draft_revision INTEGER NOT NULL CHECK(draft_revision>0),
                   repository_id TEXT NOT NULL,
                   expected_repository_revision INTEGER NOT NULL CHECK(expected_repository_revision>0),
                   request_sha256 BLOB NOT NULL UNIQUE CHECK(length(request_sha256)=32),
                   canonical_json TEXT NOT NULL CHECK(length(CAST(canonical_json AS BLOB)) BETWEEN 2 AND 16384),
                   recorded_at_ms INTEGER NOT NULL CHECK(recorded_at_ms>0)
                 );
                 CREATE TABLE assembly_line_frozen_specifications (
                   specification_id TEXT PRIMARY KEY NOT NULL,
                   specification_revision INTEGER NOT NULL CHECK(specification_revision>0),
                   target_kind TEXT NOT NULL CHECK(target_kind IN('project','feature')),
                   draft_id TEXT NOT NULL,
                   repository_id TEXT NOT NULL,
                   specification_sha256 BLOB NOT NULL UNIQUE CHECK(length(specification_sha256)=32),
                   request_sha256 BLOB NOT NULL UNIQUE CHECK(length(request_sha256)=32),
                   canonical_json TEXT NOT NULL CHECK(length(CAST(canonical_json AS BLOB)) BETWEEN 2 AND 98304),
                   recorded_at_ms INTEGER NOT NULL CHECK(recorded_at_ms>0),
                   UNIQUE(draft_id,target_kind)
                 );
                 CREATE TABLE assembly_line_owner_approvals (
                   approval_id TEXT PRIMARY KEY NOT NULL,
                   target_kind TEXT NOT NULL CHECK(target_kind IN('project','feature')),
                   specification_id TEXT NOT NULL,
                   repository_id TEXT NOT NULL,
                   owner_control_revision INTEGER NOT NULL CHECK(owner_control_revision>0),
                   owner_approval_sha256 BLOB NOT NULL UNIQUE CHECK(length(owner_approval_sha256)=32),
                   request_sha256 BLOB NOT NULL UNIQUE CHECK(length(request_sha256)=32),
                   approved_at_ms INTEGER NOT NULL CHECK(approved_at_ms>0),
                   UNIQUE(target_kind,specification_id)
                 );
                 CREATE TABLE assembly_line_repositories (
                   repository_id TEXT PRIMARY KEY NOT NULL,
                   git_url TEXT NOT NULL UNIQUE,
                   repository_revision INTEGER NOT NULL CHECK(repository_revision>0),
                   lifecycle_revision INTEGER NOT NULL CHECK(lifecycle_revision>0),
                   visibility TEXT NOT NULL CHECK(visibility IN('public','private')),
                   approved_specification_id TEXT NOT NULL UNIQUE,
                   approved_specification_revision INTEGER NOT NULL CHECK(approved_specification_revision>0),
                   approved_specification_sha256 BLOB NOT NULL CHECK(length(approved_specification_sha256)=32),
                   owner_approval_sha256 BLOB NOT NULL CHECK(length(owner_approval_sha256)=32),
                   lifecycle TEXT NOT NULL CHECK(lifecycle IN(
                     'creation_pending','reconciling','created','conflict',
                     'reconciliation_required','failed'
                   )),
                   effect_possible INTEGER NOT NULL CHECK(effect_possible IN(0,1)),
                   creation_evidence_sha256 BLOB CHECK(
                     creation_evidence_sha256 IS NULL OR length(creation_evidence_sha256)=32
                   ),
                   created_at_ms INTEGER NOT NULL CHECK(created_at_ms>0)
                 );
                 CREATE TABLE assembly_line_queue (
                   feature_id TEXT PRIMARY KEY NOT NULL,
                   repository_id TEXT NOT NULL REFERENCES assembly_line_repositories(repository_id),
                   specification_id TEXT NOT NULL UNIQUE,
                   specification_revision INTEGER NOT NULL CHECK(specification_revision>0),
                   specification_sha256 BLOB NOT NULL CHECK(length(specification_sha256)=32),
                   owner_approval_sha256 BLOB NOT NULL CHECK(length(owner_approval_sha256)=32),
                   queue_position INTEGER NOT NULL UNIQUE CHECK(queue_position>0),
                   lifecycle_revision INTEGER NOT NULL CHECK(lifecycle_revision>0),
                   lifecycle TEXT NOT NULL CHECK(lifecycle='queued'),
                   enqueued_at_ms INTEGER NOT NULL CHECK(enqueued_at_ms>0)
                 );
                 CREATE TABLE assembly_line_requests (
                   request_kind TEXT NOT NULL CHECK(request_kind IN(
                     'project_draft','feature_draft','frozen_specification',
                     'project_approval','feature_approval','auto_run'
                   )),
                   record_id TEXT NOT NULL,
                   request_sha256 BLOB NOT NULL CHECK(length(request_sha256)=32),
                   response_json TEXT CHECK(
                     response_json IS NULL OR length(CAST(response_json AS BLOB)) BETWEEN 2 AND 98304
                   ),
                   recorded_at_ms INTEGER NOT NULL CHECK(recorded_at_ms>0),
                   PRIMARY KEY(request_kind,record_id)
                 );
                 CREATE TABLE assembly_line_audit (
                   audit_id INTEGER PRIMARY KEY AUTOINCREMENT,
                   event_kind TEXT NOT NULL CHECK(length(event_kind) BETWEEN 1 AND 96),
                   occurred_at_ms INTEGER NOT NULL CHECK(occurred_at_ms>0),
                   redacted_metadata_json TEXT NOT NULL CHECK(
                     length(CAST(redacted_metadata_json AS BLOB)) BETWEEN 2 AND 4096
                   )
                 );
                 CREATE TRIGGER assembly_line_project_drafts_no_update BEFORE UPDATE ON assembly_line_project_drafts
                   BEGIN SELECT RAISE(ABORT,'immutable assembly-line project draft'); END;
                 CREATE TRIGGER assembly_line_project_drafts_no_delete BEFORE DELETE ON assembly_line_project_drafts
                   BEGIN SELECT RAISE(ABORT,'durable assembly-line project draft'); END;
                 CREATE TRIGGER assembly_line_feature_drafts_no_update BEFORE UPDATE ON assembly_line_feature_drafts
                   BEGIN SELECT RAISE(ABORT,'immutable assembly-line feature draft'); END;
                 CREATE TRIGGER assembly_line_feature_drafts_no_delete BEFORE DELETE ON assembly_line_feature_drafts
                   BEGIN SELECT RAISE(ABORT,'durable assembly-line feature draft'); END;
                 CREATE TRIGGER assembly_line_frozen_specs_no_update BEFORE UPDATE ON assembly_line_frozen_specifications
                   BEGIN SELECT RAISE(ABORT,'immutable assembly-line frozen specification'); END;
                 CREATE TRIGGER assembly_line_frozen_specs_no_delete BEFORE DELETE ON assembly_line_frozen_specifications
                   BEGIN SELECT RAISE(ABORT,'durable assembly-line frozen specification'); END;
                 CREATE TRIGGER assembly_line_approvals_no_update BEFORE UPDATE ON assembly_line_owner_approvals
                   BEGIN SELECT RAISE(ABORT,'immutable assembly-line approval'); END;
                 CREATE TRIGGER assembly_line_approvals_no_delete BEFORE DELETE ON assembly_line_owner_approvals
                   BEGIN SELECT RAISE(ABORT,'durable assembly-line approval'); END;
                 CREATE TRIGGER assembly_line_requests_no_update BEFORE UPDATE ON assembly_line_requests
                   BEGIN SELECT RAISE(ABORT,'immutable assembly-line request'); END;
                 CREATE TRIGGER assembly_line_requests_no_delete BEFORE DELETE ON assembly_line_requests
                   BEGIN SELECT RAISE(ABORT,'durable assembly-line request'); END;
                 CREATE TRIGGER assembly_line_audit_no_update BEFORE UPDATE ON assembly_line_audit
                   BEGIN SELECT RAISE(ABORT,'append-only assembly-line audit'); END;
                 CREATE TRIGGER assembly_line_audit_no_delete BEFORE DELETE ON assembly_line_audit
                   BEGIN SELECT RAISE(ABORT,'append-only assembly-line audit'); END;
                 PRAGMA user_version=20;
                 COMMIT;",
            )?;
        }
        let version = self.schema_version()?;
        if version == 20 {
            self.connection.execute_batch(
                "BEGIN IMMEDIATE;
                 ALTER TABLE assembly_line_state RENAME TO assembly_line_state_v20;
                 CREATE TABLE assembly_line_state (
                   singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
                   owner_control_revision INTEGER NOT NULL CHECK(owner_control_revision>0),
                   state_revision INTEGER NOT NULL CHECK(state_revision>0),
                   queue_revision INTEGER NOT NULL CHECK(queue_revision>=0),
                   auto_run INTEGER NOT NULL CHECK(auto_run IN(0,1)),
                   lifecycle TEXT NOT NULL CHECK(lifecycle IN(
                     'stopped','running','stopping','paused_at_checkpoint','emergency_paused',
                     'waiting_for_host_reconnect','reconciliation_required',
                     'incomplete_termination','waiting_for_owner_start'
                   )),
                   session_id TEXT,
                   active_child_epoch_id TEXT,
                   active_feature_id TEXT,
                   effect_possible INTEGER NOT NULL CHECK(effect_possible IN(0,1)),
                   authority_revision INTEGER NOT NULL CHECK(authority_revision>=0),
                   CHECK((session_id IS NULL)=(active_child_epoch_id IS NULL)),
                   CHECK((active_child_epoch_id IS NULL)=(active_feature_id IS NULL))
                 );
                 INSERT INTO assembly_line_state
                   (singleton,owner_control_revision,state_revision,queue_revision,auto_run,
                    lifecycle,session_id,active_child_epoch_id,active_feature_id,effect_possible,
                    authority_revision)
                 SELECT singleton,owner_control_revision,state_revision,queue_revision,auto_run,
                        lifecycle,NULL,NULL,NULL,0,0
                 FROM assembly_line_state_v20;
                 DROP TABLE assembly_line_state_v20;
                 ALTER TABLE assembly_line_queue RENAME TO assembly_line_queue_v20;
                 CREATE TABLE assembly_line_queue (
                   feature_id TEXT PRIMARY KEY NOT NULL,
                   repository_id TEXT NOT NULL REFERENCES assembly_line_repositories(repository_id),
                   specification_id TEXT NOT NULL UNIQUE,
                   specification_revision INTEGER NOT NULL CHECK(specification_revision>0),
                   specification_sha256 BLOB NOT NULL CHECK(length(specification_sha256)=32),
                   owner_approval_sha256 BLOB NOT NULL CHECK(length(owner_approval_sha256)=32),
                   queue_position INTEGER NOT NULL UNIQUE CHECK(queue_position>0),
                   lifecycle_revision INTEGER NOT NULL CHECK(lifecycle_revision>0),
                   lifecycle TEXT NOT NULL CHECK(lifecycle IN(
                     'queued','active','stopping','paused_at_checkpoint','emergency_paused',
                     'waiting_for_host_reconnect','reconciliation_required','incomplete_termination'
                   )),
                   enqueued_at_ms INTEGER NOT NULL CHECK(enqueued_at_ms>0)
                 );
                 INSERT INTO assembly_line_queue
                 SELECT feature_id,repository_id,specification_id,specification_revision,
                        specification_sha256,owner_approval_sha256,queue_position,
                        lifecycle_revision,lifecycle,enqueued_at_ms
                 FROM assembly_line_queue_v20;
                 DROP TABLE assembly_line_queue_v20;
                 CREATE TABLE assembly_line_execution_capabilities (
                   binding_revision INTEGER PRIMARY KEY NOT NULL CHECK(binding_revision>0),
                   state_revision INTEGER NOT NULL CHECK(state_revision>0),
                   emergency_pause_revision INTEGER NOT NULL CHECK(emergency_pause_revision>=0),
                   windows_executor_id TEXT NOT NULL,
                   windows_executor_revision INTEGER NOT NULL CHECK(windows_executor_revision>0),
                   windows_executor_sha256 BLOB NOT NULL CHECK(length(windows_executor_sha256)=32),
                   mac_executor_id TEXT NOT NULL,
                   mac_executor_revision INTEGER NOT NULL CHECK(mac_executor_revision>0),
                   mac_executor_sha256 BLOB NOT NULL CHECK(length(mac_executor_sha256)=32),
                   windows_broker_id TEXT NOT NULL,
                   windows_broker_revision INTEGER NOT NULL CHECK(windows_broker_revision>0),
                   windows_broker_sha256 BLOB NOT NULL CHECK(length(windows_broker_sha256)=32),
                   mac_broker_id TEXT NOT NULL,
                   mac_broker_revision INTEGER NOT NULL CHECK(mac_broker_revision>0),
                   mac_broker_sha256 BLOB NOT NULL CHECK(length(mac_broker_sha256)=32),
                   protected_control_plane_sha256 BLOB NOT NULL CHECK(length(protected_control_plane_sha256)=32),
                   windows_receipt_signer_key_id TEXT NOT NULL,
                   windows_receipt_verifying_key BLOB NOT NULL CHECK(length(windows_receipt_verifying_key)=32),
                   mac_receipt_signer_key_id TEXT NOT NULL,
                   mac_receipt_verifying_key BLOB NOT NULL CHECK(length(mac_receipt_verifying_key)=32),
                   healthy INTEGER NOT NULL CHECK(healthy IN(0,1)),
                   provisioning_evidence_sha256 BLOB NOT NULL CHECK(length(provisioning_evidence_sha256)=32),
                   recorded_at_ms INTEGER NOT NULL CHECK(recorded_at_ms>0)
                 );
                 CREATE TABLE assembly_line_execution_sessions (
                   session_id TEXT PRIMARY KEY NOT NULL,
                   session_revision INTEGER NOT NULL CHECK(session_revision>0),
                   start_request_id TEXT NOT NULL UNIQUE,
                   started_queue_count INTEGER NOT NULL CHECK(started_queue_count>0),
                   state_revision INTEGER NOT NULL CHECK(state_revision>0),
                   queue_revision INTEGER NOT NULL CHECK(queue_revision>0),
                   emergency_pause_revision INTEGER NOT NULL CHECK(emergency_pause_revision>=0),
                   owner_start_approval_sha256 BLOB NOT NULL CHECK(length(owner_start_approval_sha256)=32),
                   capability_binding_revision INTEGER NOT NULL REFERENCES assembly_line_execution_capabilities(binding_revision),
                   windows_executor_id TEXT NOT NULL,
                   windows_executor_revision INTEGER NOT NULL CHECK(windows_executor_revision>0),
                   mac_executor_id TEXT NOT NULL,
                   mac_executor_revision INTEGER NOT NULL CHECK(mac_executor_revision>0),
                   auto_run INTEGER NOT NULL CHECK(auto_run IN(0,1)),
                   started_at_ms INTEGER NOT NULL CHECK(started_at_ms>0)
                 );
                 CREATE TABLE assembly_line_child_epochs (
                   child_epoch_id TEXT PRIMARY KEY NOT NULL,
                   child_epoch_revision INTEGER NOT NULL CHECK(child_epoch_revision>0),
                   session_id TEXT NOT NULL REFERENCES assembly_line_execution_sessions(session_id),
                   session_revision INTEGER NOT NULL CHECK(session_revision>0),
                   feature_id TEXT NOT NULL,
                   repository_id TEXT NOT NULL,
                   feature_lifecycle_revision INTEGER NOT NULL CHECK(feature_lifecycle_revision>0),
                   queue_revision INTEGER NOT NULL CHECK(queue_revision>0),
                   authority_revision INTEGER NOT NULL CHECK(authority_revision>0),
                   lifecycle TEXT NOT NULL CHECK(lifecycle IN(
                     'running','stopping','paused_at_checkpoint','emergency_paused',
                     'waiting_for_host_reconnect','reconciliation_required',
                     'incomplete_termination'
                   )),
                   effect_possible INTEGER NOT NULL CHECK(effect_possible IN(0,1)),
                   started_at_ms INTEGER NOT NULL CHECK(started_at_ms>0),
                   UNIQUE(session_id,child_epoch_revision)
                 );
                 CREATE TABLE assembly_line_execution_authority (
                   singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
                   authority_revision INTEGER NOT NULL CHECK(authority_revision>=0),
                   revoked INTEGER NOT NULL CHECK(revoked IN(0,1)),
                   session_id TEXT,
                   child_epoch_id TEXT,
                   updated_at_ms INTEGER NOT NULL CHECK(updated_at_ms>0),
                   CHECK((session_id IS NULL)=(child_epoch_id IS NULL))
                 );
                 INSERT INTO assembly_line_execution_authority
                   (singleton,authority_revision,revoked,session_id,child_epoch_id,updated_at_ms)
                 VALUES(1,0,1,NULL,NULL,1);
                 CREATE TABLE assembly_line_action_ledger (
                   action_id TEXT PRIMARY KEY NOT NULL,
                   action_sequence INTEGER NOT NULL CHECK(action_sequence>0),
                   session_id TEXT NOT NULL,
                   child_epoch_id TEXT NOT NULL,
                   host_platform TEXT NOT NULL CHECK(host_platform IN('windows','macos')),
                   authority_revision INTEGER NOT NULL CHECK(authority_revision>0),
                   envelope_sha256 BLOB NOT NULL UNIQUE CHECK(length(envelope_sha256)=32),
                   effect_possible INTEGER NOT NULL CHECK(effect_possible IN(0,1)),
                   reconciliation_strategy TEXT NOT NULL CHECK(length(reconciliation_strategy) BETWEEN 1 AND 64),
                   recorded_at_ms INTEGER NOT NULL CHECK(recorded_at_ms>0),
                   UNIQUE(child_epoch_id,action_sequence)
                 );
                 CREATE TABLE assembly_line_checkpoint_receipts (
                   action_id TEXT NOT NULL,
                   child_epoch_id TEXT NOT NULL,
                   action_sequence INTEGER NOT NULL CHECK(action_sequence>0),
                   phase TEXT NOT NULL CHECK(phase IN('before_effect','after_effect')),
                   checkpoint_sha256 BLOB NOT NULL CHECK(length(checkpoint_sha256)=32),
                   receipt_sha256 BLOB NOT NULL UNIQUE CHECK(length(receipt_sha256)=32),
                   receipt_json TEXT NOT NULL CHECK(length(CAST(receipt_json AS BLOB)) BETWEEN 2 AND 16384),
                   recorded_at_ms INTEGER NOT NULL CHECK(recorded_at_ms>0),
                   PRIMARY KEY(action_id,phase)
                 );
                 CREATE TABLE assembly_line_control_intents (
                   request_id TEXT PRIMARY KEY NOT NULL,
                   mode TEXT NOT NULL CHECK(mode IN('stop','emergency_pause')),
                   session_id TEXT NOT NULL,
                   child_epoch_id TEXT NOT NULL,
                   authority_revision INTEGER NOT NULL CHECK(authority_revision>0),
                   checkpoint_id TEXT NOT NULL UNIQUE,
                   checkpoint_sha256 BLOB NOT NULL CHECK(length(checkpoint_sha256)=32),
                   request_sha256 BLOB NOT NULL UNIQUE CHECK(length(request_sha256)=32),
                   state_revision INTEGER NOT NULL CHECK(state_revision>0),
                   termination_pending INTEGER NOT NULL CHECK(termination_pending=1),
                   recorded_at_ms INTEGER NOT NULL CHECK(recorded_at_ms>0)
                 );
                 CREATE TABLE assembly_line_termination_receipts (
                   receipt_id TEXT PRIMARY KEY NOT NULL,
                   request_id TEXT NOT NULL REFERENCES assembly_line_control_intents(request_id),
                   child_epoch_id TEXT NOT NULL,
                   mode TEXT NOT NULL CHECK(mode IN('stop','emergency_pause')),
                   signer_key_id TEXT NOT NULL,
                   outcome TEXT NOT NULL CHECK(outcome IN('reaped','incomplete')),
                   last_checkpoint_sha256 BLOB NOT NULL CHECK(length(last_checkpoint_sha256)=32),
                   receipt_sha256 BLOB NOT NULL UNIQUE CHECK(length(receipt_sha256)=32),
                   receipt_json TEXT NOT NULL CHECK(length(CAST(receipt_json AS BLOB)) BETWEEN 2 AND 16384),
                   recorded_at_ms INTEGER NOT NULL CHECK(recorded_at_ms>0),
                   UNIQUE(request_id,signer_key_id)
                 );
                 CREATE TABLE assembly_line_execution_requests (
                   request_kind TEXT NOT NULL CHECK(request_kind IN('start','stop','emergency_pause')),
                   request_id TEXT NOT NULL,
                   request_sha256 BLOB NOT NULL CHECK(length(request_sha256)=32),
                   response_json TEXT NOT NULL CHECK(length(CAST(response_json AS BLOB)) BETWEEN 2 AND 98304),
                   recorded_at_ms INTEGER NOT NULL CHECK(recorded_at_ms>0),
                   PRIMARY KEY(request_kind,request_id)
                 );
                 CREATE TRIGGER assembly_line_execution_capabilities_no_update
                   BEFORE UPDATE ON assembly_line_execution_capabilities
                   BEGIN SELECT RAISE(ABORT,'immutable assembly-line execution capability'); END;
                 CREATE TRIGGER assembly_line_execution_capabilities_no_delete
                   BEFORE DELETE ON assembly_line_execution_capabilities
                   BEGIN SELECT RAISE(ABORT,'durable assembly-line execution capability'); END;
                 CREATE TRIGGER assembly_line_execution_sessions_no_update
                   BEFORE UPDATE ON assembly_line_execution_sessions
                   BEGIN SELECT RAISE(ABORT,'immutable assembly-line execution session'); END;
                 CREATE TRIGGER assembly_line_execution_sessions_no_delete
                   BEFORE DELETE ON assembly_line_execution_sessions
                   BEGIN SELECT RAISE(ABORT,'durable assembly-line execution session'); END;
                 CREATE TRIGGER assembly_line_action_ledger_no_update
                   BEFORE UPDATE ON assembly_line_action_ledger
                   BEGIN SELECT RAISE(ABORT,'immutable assembly-line action intent'); END;
                 CREATE TRIGGER assembly_line_action_ledger_no_delete
                   BEFORE DELETE ON assembly_line_action_ledger
                   BEGIN SELECT RAISE(ABORT,'durable assembly-line action intent'); END;
                 CREATE TRIGGER assembly_line_checkpoint_receipts_no_update
                   BEFORE UPDATE ON assembly_line_checkpoint_receipts
                   BEGIN SELECT RAISE(ABORT,'immutable assembly-line checkpoint receipt'); END;
                 CREATE TRIGGER assembly_line_checkpoint_receipts_no_delete
                   BEFORE DELETE ON assembly_line_checkpoint_receipts
                   BEGIN SELECT RAISE(ABORT,'durable assembly-line checkpoint receipt'); END;
                 CREATE TRIGGER assembly_line_control_intents_no_update
                   BEFORE UPDATE ON assembly_line_control_intents
                   BEGIN SELECT RAISE(ABORT,'immutable assembly-line control intent'); END;
                 CREATE TRIGGER assembly_line_control_intents_no_delete
                   BEFORE DELETE ON assembly_line_control_intents
                   BEGIN SELECT RAISE(ABORT,'durable assembly-line control intent'); END;
                 CREATE TRIGGER assembly_line_termination_receipts_no_update
                   BEFORE UPDATE ON assembly_line_termination_receipts
                   BEGIN SELECT RAISE(ABORT,'immutable assembly-line termination receipt'); END;
                 CREATE TRIGGER assembly_line_termination_receipts_no_delete
                   BEFORE DELETE ON assembly_line_termination_receipts
                   BEGIN SELECT RAISE(ABORT,'durable assembly-line termination receipt'); END;
                 CREATE TRIGGER assembly_line_execution_requests_no_update
                   BEFORE UPDATE ON assembly_line_execution_requests
                   BEGIN SELECT RAISE(ABORT,'immutable assembly-line execution request'); END;
                 CREATE TRIGGER assembly_line_execution_requests_no_delete
                   BEFORE DELETE ON assembly_line_execution_requests
                   BEGIN SELECT RAISE(ABORT,'durable assembly-line execution request'); END;
                 PRAGMA user_version=21;
                 COMMIT;",
            )?;
        }
        let version = self.schema_version()?;
        if version == 21 {
            self.connection.execute_batch(
                "PRAGMA foreign_keys=OFF;
                 PRAGMA legacy_alter_table=ON;
                 BEGIN IMMEDIATE;
                 ALTER TABLE assembly_line_state RENAME TO assembly_line_state_v21;
                 CREATE TABLE assembly_line_state (
                   singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
                   owner_control_revision INTEGER NOT NULL CHECK(owner_control_revision>0),
                   state_revision INTEGER NOT NULL CHECK(state_revision>0),
                   queue_revision INTEGER NOT NULL CHECK(queue_revision>=0),
                   auto_run INTEGER NOT NULL CHECK(auto_run IN(0,1)),
                   lifecycle TEXT NOT NULL CHECK(lifecycle IN(
                     'stopped','starting','running','stopping','paused_at_checkpoint','emergency_paused',
                     'waiting_for_host_reconnect','reconciliation_required',
                     'incomplete_termination','waiting_for_owner_start'
                   )),
                   session_id TEXT,
                   active_child_epoch_id TEXT,
                   active_feature_id TEXT,
                   effect_possible INTEGER NOT NULL CHECK(effect_possible IN(0,1)),
                   authority_revision INTEGER NOT NULL CHECK(authority_revision>=0),
                   CHECK((session_id IS NULL)=(active_child_epoch_id IS NULL)),
                   CHECK((active_child_epoch_id IS NULL)=(active_feature_id IS NULL))
                 );
                 INSERT INTO assembly_line_state SELECT * FROM assembly_line_state_v21;
                 DROP TABLE assembly_line_state_v21;
                 ALTER TABLE assembly_line_queue RENAME TO assembly_line_queue_v21;
                 CREATE TABLE assembly_line_queue (
                   feature_id TEXT PRIMARY KEY NOT NULL,
                   repository_id TEXT NOT NULL REFERENCES assembly_line_repositories(repository_id),
                   specification_id TEXT NOT NULL UNIQUE,
                   specification_revision INTEGER NOT NULL CHECK(specification_revision>0),
                   specification_sha256 BLOB NOT NULL CHECK(length(specification_sha256)=32),
                   owner_approval_sha256 BLOB NOT NULL CHECK(length(owner_approval_sha256)=32),
                   queue_position INTEGER NOT NULL UNIQUE CHECK(queue_position>0),
                   lifecycle_revision INTEGER NOT NULL CHECK(lifecycle_revision>0),
                   lifecycle TEXT NOT NULL CHECK(lifecycle IN(
                     'queued','starting','active','stopping','paused_at_checkpoint','emergency_paused',
                     'waiting_for_host_reconnect','reconciliation_required','incomplete_termination'
                   )),
                   enqueued_at_ms INTEGER NOT NULL CHECK(enqueued_at_ms>0)
                 );
                 INSERT INTO assembly_line_queue SELECT * FROM assembly_line_queue_v21;
                 DROP TABLE assembly_line_queue_v21;
                 ALTER TABLE assembly_line_child_epochs RENAME TO assembly_line_child_epochs_v21;
                 CREATE TABLE assembly_line_child_epochs (
                   child_epoch_id TEXT PRIMARY KEY NOT NULL,
                   child_epoch_revision INTEGER NOT NULL CHECK(child_epoch_revision>0),
                   session_id TEXT NOT NULL REFERENCES assembly_line_execution_sessions(session_id),
                   session_revision INTEGER NOT NULL CHECK(session_revision>0),
                   feature_id TEXT NOT NULL,
                   repository_id TEXT NOT NULL,
                   feature_lifecycle_revision INTEGER NOT NULL CHECK(feature_lifecycle_revision>0),
                   queue_revision INTEGER NOT NULL CHECK(queue_revision>0),
                   authority_revision INTEGER NOT NULL CHECK(authority_revision>0),
                   lifecycle TEXT NOT NULL CHECK(lifecycle IN(
                     'starting','running','stopping','paused_at_checkpoint','emergency_paused',
                     'waiting_for_host_reconnect','reconciliation_required',
                     'incomplete_termination'
                   )),
                   effect_possible INTEGER NOT NULL CHECK(effect_possible IN(0,1)),
                   started_at_ms INTEGER NOT NULL CHECK(started_at_ms>0),
                   UNIQUE(session_id,child_epoch_revision)
                 );
                 INSERT INTO assembly_line_child_epochs SELECT * FROM assembly_line_child_epochs_v21;
                 DROP TABLE assembly_line_child_epochs_v21;
                 CREATE TABLE assembly_line_activation_receipts (
                   receipt_id TEXT PRIMARY KEY NOT NULL,
                   session_id TEXT NOT NULL REFERENCES assembly_line_execution_sessions(session_id),
                   child_epoch_id TEXT NOT NULL,
                   authority_revision INTEGER NOT NULL CHECK(authority_revision>0),
                   host_platform TEXT NOT NULL CHECK(host_platform IN('windows','macos')),
                   signer_key_id TEXT NOT NULL,
                   receipt_sha256 BLOB NOT NULL UNIQUE CHECK(length(receipt_sha256)=32),
                   receipt_json TEXT NOT NULL CHECK(length(CAST(receipt_json AS BLOB)) BETWEEN 2 AND 16384),
                   recorded_at_ms INTEGER NOT NULL CHECK(recorded_at_ms>0),
                   UNIQUE(session_id,host_platform)
                 );
                 CREATE TABLE assembly_line_effect_dispatches (
                   request_kind TEXT NOT NULL CHECK(request_kind IN('start','stop','emergency_pause')),
                   request_id TEXT NOT NULL,
                   intent_sha256 BLOB NOT NULL CHECK(length(intent_sha256)=32),
                   effect_possible INTEGER NOT NULL CHECK(effect_possible=1),
                   recorded_at_ms INTEGER NOT NULL CHECK(recorded_at_ms>0),
                   PRIMARY KEY(request_kind,request_id)
                 );
                 CREATE TRIGGER assembly_line_activation_receipts_no_update
                   BEFORE UPDATE ON assembly_line_activation_receipts
                   BEGIN SELECT RAISE(ABORT,'immutable assembly-line activation receipt'); END;
                 CREATE TRIGGER assembly_line_activation_receipts_no_delete
                   BEFORE DELETE ON assembly_line_activation_receipts
                   BEGIN SELECT RAISE(ABORT,'durable assembly-line activation receipt'); END;
                 CREATE TRIGGER assembly_line_effect_dispatches_no_update
                   BEFORE UPDATE ON assembly_line_effect_dispatches
                   BEGIN SELECT RAISE(ABORT,'immutable assembly-line effect dispatch'); END;
                 CREATE TRIGGER assembly_line_effect_dispatches_no_delete
                   BEFORE DELETE ON assembly_line_effect_dispatches
                   BEGIN SELECT RAISE(ABORT,'durable assembly-line effect dispatch'); END;
                 PRAGMA user_version=22;
                 COMMIT;
                 PRAGMA legacy_alter_table=OFF;
                 PRAGMA foreign_keys=ON;",
            )?;
            let violations: i64 = self.connection.query_row(
                "SELECT COUNT(*) FROM pragma_foreign_key_check",
                [],
                |row| row.get(0),
            )?;
            if violations != 0 {
                return Err(MasterError::InvalidStoredState(
                    "schema-v22 migration produced foreign-key violations".to_string(),
                ));
            }
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

    fn reconcile_assembly_line_startup(
        &mut self,
        now_ms: u64,
    ) -> Result<AssemblyLineStartupReconciliation, MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let state = assembly_line_state_tx(&tx)?;
        if matches!(
            state.lifecycle,
            AssemblyLineLifecycleState::Stopped
                | AssemblyLineLifecycleState::WaitingForOwnerStart
                | AssemblyLineLifecycleState::PausedAtCheckpoint
        ) {
            tx.commit()?;
            return Ok(AssemblyLineStartupReconciliation::default());
        }
        let Some(child_epoch_id) = state.active_child_epoch_id else {
            return Err(MasterError::InvalidStoredState(
                "active assembly-line restart state has no child epoch".to_string(),
            ));
        };
        let pending_termination_intent: i64 = tx.query_row(
            "SELECT COUNT(*) FROM assembly_line_control_intents i
             WHERE i.child_epoch_id=?1
               AND (SELECT COUNT(*) FROM assembly_line_termination_receipts r
                    WHERE r.request_id=i.request_id) < 2",
            [child_epoch_id.to_string()],
            |row| row.get(0),
        )?;
        let action_effect_possible: i64 = tx.query_row(
            "SELECT COUNT(*) FROM assembly_line_action_ledger
             WHERE child_epoch_id=?1 AND effect_possible=1",
            [child_epoch_id.to_string()],
            |row| row.get(0),
        )?;
        let state_effect_possible: i64 = tx.query_row(
            "SELECT effect_possible FROM assembly_line_state WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        if !matches!(state_effect_possible, 0 | 1) {
            return Err(MasterError::InvalidStoredState(
                "assembly-line restart effect marker is malformed".to_string(),
            ));
        }
        let effect_possible = state_effect_possible == 1 || action_effect_possible != 0;
        let lifecycle = if pending_termination_intent != 0 {
            "incomplete_termination"
        } else if effect_possible {
            "reconciliation_required"
        } else {
            "waiting_for_host_reconnect"
        };
        let prior_authority_revision: i64 = tx.query_row(
            "SELECT authority_revision FROM assembly_line_execution_authority WHERE singleton=1",
            [],
            |row| row.get(0),
        )?;
        let next_authority_revision = i64_to_u64(prior_authority_revision)?
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        let prior_owner_revision = assembly_line_owner_revision_tx(&tx)?;
        let next_owner_revision = prior_owner_revision
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        let next_state_revision = state
            .state_revision
            .checked_add(1)
            .ok_or(MasterError::IntegerOutOfRange)?;
        if tx.execute(
            "UPDATE assembly_line_execution_authority
             SET authority_revision=?1,revoked=1,updated_at_ms=?2
             WHERE singleton=1 AND authority_revision=?3",
            params![
                u64_to_i64(next_authority_revision)?,
                u64_to_i64(now_ms)?,
                prior_authority_revision,
            ],
        )? != 1
        {
            return Err(MasterError::AssemblyLineExecutionControlUnavailable);
        }
        if tx.execute(
            "UPDATE assembly_line_state
             SET owner_control_revision=?1,state_revision=?2,lifecycle=?3,
                 effect_possible=?4,authority_revision=?5
             WHERE singleton=1 AND owner_control_revision=?6 AND state_revision=?7
               AND active_child_epoch_id=?8",
            params![
                u64_to_i64(next_owner_revision)?,
                u64_to_i64(next_state_revision)?,
                lifecycle,
                i64::from(effect_possible || pending_termination_intent != 0),
                u64_to_i64(next_authority_revision)?,
                u64_to_i64(prior_owner_revision)?,
                u64_to_i64(state.state_revision)?,
                child_epoch_id.to_string(),
            ],
        )? != 1
        {
            return Err(MasterError::AssemblyLineExecutionControlUnavailable);
        }
        if tx.execute(
            "UPDATE assembly_line_child_epochs
             SET lifecycle=?1,effect_possible=?2,authority_revision=?3
             WHERE child_epoch_id=?4",
            params![
                lifecycle,
                i64::from(effect_possible || pending_termination_intent != 0),
                u64_to_i64(next_authority_revision)?,
                child_epoch_id.to_string(),
            ],
        )? != 1
        {
            return Err(MasterError::AssemblyLineExecutionControlUnavailable);
        }
        if tx.execute(
            "UPDATE assembly_line_queue
             SET lifecycle=?1,lifecycle_revision=lifecycle_revision+1
             WHERE feature_id=(SELECT active_feature_id FROM assembly_line_state WHERE singleton=1)
               AND queue_position=1
               AND lifecycle IN('starting','active','stopping','emergency_paused','waiting_for_host_reconnect',
                                'reconciliation_required','incomplete_termination')",
            [lifecycle],
        )? != 1
        {
            return Err(MasterError::AssemblyLineExecutionControlUnavailable);
        }
        append_assembly_line_audit_tx(
            &tx,
            "execution_restart_reconciled",
            now_ms,
            serde_json::json!({
                "child_epoch_id": child_epoch_id,
                "state_revision": next_state_revision,
                "authority_revision": next_authority_revision,
                "authority_revoked": true,
                "effect_possible": effect_possible,
                "pending_termination_intent": pending_termination_intent != 0,
                "resulting_lifecycle": lifecycle,
                "automatic_retry": false,
                "external_effect": false
            }),
        )?;
        tx.commit()?;
        Ok(AssemblyLineStartupReconciliation {
            quarantined_effect_possible_session: effect_possible,
            pending_termination_intent: pending_termination_intent != 0,
        })
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
            let feature_uuid = parse_uuid(&feature_id)?;
            let resumable = status == FeatureLifecycleStatus::Paused
                && orchestration_paused_checkpoint_is_restart_safe_tx(
                    &tx,
                    feature_uuid,
                    i64_to_u64(revision)?,
                )?;
            if status.is_active_execution() && !resumable {
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
                tx.execute(
                    "INSERT INTO feature_transition_evidence (
                       feature_id, lifecycle_revision, from_status, to_status, recorded_at_ms
                     ) VALUES (?1, ?2, ?3, 'quarantined', ?4)",
                    params![
                        feature_id,
                        revision + 1,
                        status.as_str(),
                        u64_to_i64(now_ms)?,
                    ],
                )?;
                append_feature_audit_tx(
                    &tx,
                    "feature_startup_quarantined",
                    Some(feature_uuid),
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

fn require_accepted_feature_provider_provenance_tx(
    tx: &Transaction<'_>,
    frozen: &FrozenBrainstormingSpecification,
    provider: Option<(
        &crate::planning_effects::BrainstormingAdapterBinding,
        [u8; 32],
    )>,
) -> Result<(), MasterError> {
    let default_profile = OrchestratorProfile::default();
    let (provider_id, model_id, adapter_sha256, adapter_catalog_sha256) = match provider {
        Some((binding, catalog_sha256)) => (
            binding.profile.provider_id.as_str(),
            binding.profile.model_id.as_str(),
            Some(binding.executable_sha256),
            Some(catalog_sha256),
        ),
        None => (
            default_profile.provider_id.as_str(),
            default_profile.model_id.as_str(),
            None,
            None,
        ),
    };
    let mut statement = tx.prepare(
        "SELECT redacted_metadata_json FROM assembly_line_audit
         WHERE event_kind='brainstorming_provider_output_accepted' ORDER BY audit_id ASC",
    )?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    let mut matches = 0_usize;
    for row in rows {
        let value: Value = serde_json::from_str(&row?)?;
        let accepted: crate::planning_effects::BrainstormingAcceptedAudit =
            serde_json::from_value(value)?;
        if accepted.target_kind == "feature"
            && accepted.draft_id == frozen.draft_id
            && accepted.specification_id == frozen.specification_id
            && accepted.specification_sha256 == frozen.specification_sha256
            && accepted.provider_id == provider_id
            && accepted.model_id == model_id
            && adapter_sha256.is_none_or(|expected| accepted.adapter_sha256 == expected)
            && adapter_catalog_sha256
                .is_none_or(|expected| accepted.adapter_catalog_sha256 == expected)
        {
            if accepted.adapter_sha256 == [0; 32]
                || accepted.adapter_catalog_sha256 == [0; 32]
                || !accepted.planning_only
                || accepted.provider_output_retained_in_audit
                || accepted.external_effect_authorized
            {
                return Err(MasterError::InvalidStoredState(
                    "accepted feature brainstorming provenance audit is malformed".to_string(),
                ));
            }
            matches += 1;
        }
    }
    if matches != 1 {
        return Err(MasterError::AssemblyLineBrainstormingUnavailable);
    }
    Ok(())
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
    if step.capability_id == LOCAL_CODING_CAPABILITY_ID {
        return Err(MasterError::FeatureCodingDispatchUnavailable);
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

fn cancel_feature_coding_dispatches_tx(
    tx: &Transaction<'_>,
    feature_id: Uuid,
    now_ms: u64,
) -> Result<(), MasterError> {
    let mut statement = tx.prepare(
        "SELECT d.task_id, d.step_id, s.status
         FROM feature_coding_dispatches d
         JOIN master_steps s ON s.step_id = d.step_id
         WHERE d.feature_id = ?1 AND s.status IN ('queued', 'leased')
         ORDER BY d.dispatched_at_ms, d.step_id",
    )?;
    let dispatches = statement
        .query_map([feature_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(statement);
    for (task_id, step_id, status) in dispatches {
        let task_id = TaskId::new(parse_uuid(&task_id)?);
        let step_id = StepId::new(parse_uuid(&step_id)?);
        match status.as_str() {
            "queued" => {
                tx.execute(
                    "UPDATE master_steps SET status = 'cancelled', completed_at_ms = ?1
                     WHERE step_id = ?2 AND status = 'queued'",
                    params![u64_to_i64(now_ms)?, step_id.0.to_string()],
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
            "leased" => request_leased_step_cancellation_tx(tx, task_id, step_id, now_ms)?,
            _ => {
                return Err(MasterError::InvalidStoredState(
                    "feature coding dispatch had an unexpected nonterminal status".to_string(),
                ));
            }
        }
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
    let mut queued_statement = tx.prepare(
        "SELECT task_id, step_id FROM master_steps
         WHERE capability_id = ?1 AND status = 'queued'
         ORDER BY created_at_ms, step_id",
    )?;
    let queued_coding = queued_statement
        .query_map([LOCAL_CODING_CAPABILITY_ID], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(queued_statement);
    for (task_id, step_id) in queued_coding {
        let task_id = TaskId::new(parse_uuid(&task_id)?);
        let step_id = StepId::new(parse_uuid(&step_id)?);
        let changed = tx.execute(
            "UPDATE master_steps SET status = 'cancelled', completed_at_ms = ?1
             WHERE step_id = ?2 AND status = 'queued'",
            params![u64_to_i64(now_ms)?, step_id.0.to_string()],
        )?;
        if changed != 1 {
            return Err(MasterError::InvalidStoredState(
                "queued coding step changed before emergency-pause cancellation".to_string(),
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
    }
    let mut statement = tx.prepare(
        "SELECT s.task_id, s.step_id FROM master_steps s\n\
         JOIN master_attempts a ON a.step_id = s.step_id\n\
         WHERE s.capability_id IN (?1, ?2, ?3) AND s.status = 'leased' AND a.status = 'leased'\n\
         ORDER BY a.leased_at_ms ASC",
    )?;
    let remote_steps = statement
        .query_map(
            [
                FIXTURE_REASONING_CAPABILITY_ID,
                MLX_REASONING_CAPABILITY_ID,
                LOCAL_CODING_CAPABILITY_ID,
            ],
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

fn coding_step_binding_is_current_tx(
    tx: &Transaction<'_>,
    step: &StoredStep,
    leasing_device_id: DeviceId,
) -> Result<bool, MasterError> {
    let context: LocalCodingJobRequest = match serde_json::from_str(&step.context_json) {
        Ok(context) => context,
        Err(_) => return Ok(false),
    };
    if context.validate().is_err() || context.device_id != leasing_device_id {
        return Ok(false);
    }
    coding_job_binding_is_current_tx(tx, &context, step.step_id, leasing_device_id)
}

fn coding_job_binding_is_current_tx(
    tx: &Transaction<'_>,
    context: &LocalCodingJobRequest,
    step_id: StepId,
    device_id: DeviceId,
) -> Result<bool, MasterError> {
    if context.validate().is_err() || context.device_id != device_id {
        return Ok(false);
    }
    let mapped: bool = tx.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM feature_coding_dispatches d
           JOIN master_devices device ON device.device_id = d.device_id
           JOIN feature_conveyor_features f ON f.feature_id = d.feature_id
           JOIN feature_active_lease l
             ON l.feature_id = d.feature_id AND l.lease_id = d.feature_lease_id
            AND l.snapshot_id = d.snapshot_id
           JOIN feature_repository_snapshot_claims c
             ON c.snapshot_id = d.snapshot_id AND c.snapshot_sha256 = d.snapshot_sha256
           JOIN feature_conveyor_state q ON q.singleton = 1
           JOIN master_metadata pause ON pause.key = 'emergency_pause_revision'
           WHERE d.step_id = ?1 AND d.device_id = ?2
             AND d.device_registry_revision = ?3 AND device.registry_revision = ?3
             AND device.revoked = 0
             AND d.feature_id = ?4 AND d.specification_revision = ?5
             AND f.current_specification_revision = ?5
             AND d.lifecycle_revision = ?6 AND f.lifecycle_revision = ?6
             AND f.status = 'implementing'
             AND d.feature_lease_id = ?7 AND d.snapshot_id = ?8
             AND d.snapshot_sha256 = ?9 AND d.work_packet_sha256 = ?10
             AND d.queue_revision = ?11 AND q.queue_revision = ?11
             AND d.emergency_pause_revision = ?12 AND pause.integer_value = ?12
         )",
        params![
            step_id.0.to_string(),
            device_id.0.to_string(),
            u64_to_i64(context.device_registry_revision)?,
            context.feature_id.to_string(),
            u64_to_i64(context.specification_revision)?,
            u64_to_i64(context.lifecycle_revision)?,
            context.feature_lease_id.to_string(),
            context.snapshot_id.to_string(),
            context.snapshot_sha256.as_slice(),
            context.work_packet_sha256.as_slice(),
            u64_to_i64(context.queue_revision)?,
            u64_to_i64(context.emergency_pause_revision)?,
        ],
        |row| row.get(0),
    )?;
    Ok(mapped)
}

fn validate_result_artifact_admission_tx(
    tx: &Transaction<'_>,
    authenticated_device_id: DeviceId,
    admission: &LocalCodingResultArtifactAdmission,
    now_ms: u64,
) -> Result<JobEnvelope, MasterError> {
    if admission.workspace_expires_at_ms <= now_ms
        || admission.workspace_expires_at_ms
            > now_ms.saturating_add(MAX_RETAINED_CODING_WORKSPACE_MS)
    {
        return Err(MasterError::ResultArtifactUnavailable);
    }
    require_emergency_unpaused_tx(tx)?;
    let attempt =
        load_attempt(tx, admission.attempt_id)?.ok_or(MasterError::ResultArtifactUnavailable)?;
    if attempt.device_id != authenticated_device_id
        || attempt.status != AttemptStatus::Leased
        || attempt.lease_expires_at_ms <= now_ms
    {
        return Err(MasterError::ResultArtifactUnavailable);
    }
    let job: JobEnvelope = serde_json::from_str(&attempt.job_json)?;
    admission.validate_for_job(&job)?;
    let connection = connection_state(tx, authenticated_device_id)?;
    if !connection.active
        || connection.epoch != admission.connection_epoch
        || admission.sequence <= connection.last_sequence
    {
        return Err(MasterError::ResultArtifactUnavailable);
    }
    if step_status_tx(tx, admission.step_id)? != StepStatus::Leased {
        return Err(MasterError::ResultArtifactUnavailable);
    }
    let context = job.validate_local_coding()?;
    if !coding_job_binding_is_current_tx(tx, &context, job.step_id, authenticated_device_id)? {
        return Err(MasterError::ResultArtifactUnavailable);
    }
    let capability =
        capability_for_device(tx, authenticated_device_id, LOCAL_CODING_CAPABILITY_ID)?;
    if capability != RemoteWorkContract::LocalCoding.capability() {
        return Err(MasterError::ResultArtifactUnavailable);
    }
    let collision: i64 = tx.query_row(
        "SELECT COUNT(*) FROM feature_result_artifacts
         WHERE artifact_id = ?1 OR attempt_id = ?2",
        params![
            admission.artifact.artifact_id.to_string(),
            admission.attempt_id.0.to_string()
        ],
        |row| row.get(0),
    )?;
    if collision != 0 {
        let exact: i64 = tx.query_row(
            "SELECT COUNT(*) FROM feature_result_artifacts
             WHERE artifact_id = ?1 AND artifact_sha256 = ?2
               AND artifact_size_bytes = ?3 AND device_id = ?4
               AND device_registry_revision = ?5 AND connection_epoch = ?6
               AND sequence = ?7 AND task_id = ?8 AND step_id = ?9
               AND attempt_id = ?10 AND lease_id = ?11 AND cancellation_id = ?12
               AND context_sha256 = ?13 AND feature_id = ?14
               AND feature_lease_id = ?15 AND snapshot_id = ?16
               AND snapshot_sha256 = ?17 AND work_packet_sha256 = ?18
               AND workspace_retained = 1 AND workspace_expires_at_ms = ?19",
            params![
                admission.artifact.artifact_id.to_string(),
                admission.artifact.artifact_sha256.as_slice(),
                u64_to_i64(admission.artifact.artifact_size_bytes)?,
                authenticated_device_id.0.to_string(),
                u64_to_i64(context.device_registry_revision)?,
                u64_to_i64(admission.connection_epoch)?,
                u64_to_i64(admission.sequence)?,
                admission.task_id.0.to_string(),
                admission.step_id.0.to_string(),
                admission.attempt_id.0.to_string(),
                admission.lease_id.0.to_string(),
                admission.cancellation_id.0.to_string(),
                admission.context_sha256.as_slice(),
                admission.feature_id.to_string(),
                admission.feature_lease_id.to_string(),
                admission.snapshot_id.to_string(),
                admission.snapshot_sha256.as_slice(),
                admission.work_packet_sha256.as_slice(),
                u64_to_i64(admission.workspace_expires_at_ms)?,
            ],
            |row| row.get(0),
        )?;
        if exact != 1 || collision != 1 {
            return Err(MasterError::ResultArtifactUnavailable);
        }
    }
    Ok(job)
}

fn require_feature_coding_work_terminal_tx(
    tx: &Transaction<'_>,
    feature_id: Uuid,
) -> Result<(), MasterError> {
    let outstanding: bool = tx.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM feature_coding_dispatches d
           JOIN master_steps s ON s.step_id = d.step_id
           LEFT JOIN master_attempts a ON a.step_id = d.step_id
           WHERE d.feature_id = ?1
             AND (
               s.status IN ('queued', 'leased')
               OR a.status = 'cancellation_pending'
             )
         )",
        [feature_id.to_string()],
        |row| row.get(0),
    )?;
    if outstanding {
        Err(MasterError::FeatureCodingWorkOutstanding)
    } else {
        Ok(())
    }
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

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
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
    validate_review_safe_manifest_schema(&specification.manifest)?;
    validate_review_disclosure_value(&specification.manifest)?;
    approved_requirement_ids(&specification.manifest)?;
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

fn validate_review_disclosure_value(value: &Value) -> Result<(), MasterError> {
    const FORBIDDEN_KEY_COMPONENTS: &[&str] = &[
        "authorization",
        "auth",
        "authentication",
        "bearer",
        "chat",
        "conversation",
        "cookie",
        "credential",
        "credentials",
        "dialogue",
        "history",
        "memory",
        "message",
        "messages",
        "oauth",
        "passwd",
        "password",
        "prompt",
        "prompts",
        "raw",
        "secret",
        "secrets",
        "transcript",
        "token",
        "turn",
        "turns",
    ];
    const FORBIDDEN_COMPOUND_KEYS: &[&str] = &[
        "access_token",
        "api_key",
        "apikey",
        "auth_token",
        "bearer_token",
        "id_token",
        "private_key",
        "refresh_token",
        "session_token",
    ];
    match value {
        Value::Object(object) => {
            for (key, nested) in object {
                let mut normalized = String::with_capacity(key.len());
                let mut separator = false;
                for character in key.chars().flat_map(char::to_lowercase) {
                    if character.is_ascii_alphanumeric() {
                        normalized.push(character);
                        separator = false;
                    } else if !separator && !normalized.is_empty() {
                        normalized.push('_');
                        separator = true;
                    }
                }
                let normalized = normalized.trim_matches('_');
                let compact = normalized.replace('_', "");
                let forbidden_component = normalized
                    .split('_')
                    .any(|component| FORBIDDEN_KEY_COMPONENTS.contains(&component))
                    || FORBIDDEN_KEY_COMPONENTS
                        .iter()
                        .any(|component| compact.contains(component));
                let forbidden_compound = FORBIDDEN_COMPOUND_KEYS.iter().any(|forbidden| {
                    normalized == *forbidden
                        || normalized.starts_with(&format!("{forbidden}_"))
                        || normalized.ends_with(&format!("_{forbidden}"))
                        || compact == forbidden.replace('_', "")
                });
                if forbidden_component || forbidden_compound {
                    return Err(MasterError::InvalidFeatureConveyorInput(
                        "approved specification contains review-forbidden sensitive context"
                            .to_string(),
                    ));
                }
                validate_review_disclosure_value(nested)?;
            }
        }
        Value::Array(values) => {
            for nested in values {
                validate_review_disclosure_value(nested)?;
            }
        }
        Value::String(string) => {
            if contains_embedded_secret_shape(string.trim()) {
                return Err(MasterError::InvalidFeatureConveyorInput(
                    "approved specification contains review-forbidden secret-shaped content"
                        .to_string(),
                ));
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
    Ok(())
}

fn contains_embedded_secret_shape(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if lower.contains("-----begin ")
        || lower.contains("bearer ")
        || lower.contains("basic ")
        || value.contains("ghp_")
        || value.contains("github_pat_")
        || contains_prefixed_token(value, "sk-", 17)
        || contains_aws_access_key(value)
        || contains_embedded_jwt(value)
    {
        return true;
    }
    value.match_indices("://").any(|(offset, _)| {
        let authority = &value[offset + 3..];
        let authority = authority
            .split(|character: char| {
                character.is_ascii_whitespace() || matches!(character, '/' | '?' | '#')
            })
            .next()
            .unwrap_or_default();
        authority.contains('@')
    })
}

fn contains_prefixed_token(value: &str, prefix: &str, minimum_suffix: usize) -> bool {
    value.match_indices(prefix).any(|(offset, _)| {
        value[offset + prefix.len()..]
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
            .count()
            >= minimum_suffix
    })
}

fn contains_aws_access_key(value: &str) -> bool {
    value.match_indices("AKIA").any(|(offset, _)| {
        value.as_bytes()[offset + 4..]
            .iter()
            .take(16)
            .copied()
            .filter(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
            .count()
            == 16
    })
}

fn contains_embedded_jwt(value: &str) -> bool {
    value.match_indices("eyJ").any(|(offset, _)| {
        let candidate = &value[offset..];
        let mut segments = candidate.splitn(3, '.');
        let Some(header) = segments.next() else {
            return false;
        };
        let Some(payload) = segments.next() else {
            return false;
        };
        let Some(signature_and_suffix) = segments.next() else {
            return false;
        };
        if !header
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
            || payload.is_empty()
            || !payload
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return false;
        }
        let signature_len = signature_and_suffix
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
            .count();
        signature_len > 0 && header.len() + payload.len() + signature_len + 2 >= 32
    })
}

fn exact_owner_bridge_enqueue_replay_tx(
    tx: &Transaction<'_>,
    specification: &ApprovedFeatureSpecification,
    canonical_manifest: &str,
    original_expected_queue_revision: u64,
    replay_queue_revision: u64,
    owner_binding: (&DeviceRegistration, u64, u64),
) -> Result<Option<FeatureSnapshot>, MasterError> {
    let (registration, designation_revision, emergency_pause_revision) = owner_binding;
    let specification_count: i64 = tx.query_row(
        "SELECT COUNT(*) FROM feature_specification_revisions WHERE feature_id = ?1",
        [specification.feature_id.to_string()],
        |row| row.get(0),
    )?;
    if specification_count != 1 {
        return Ok(None);
    }
    let stored_specification = tx
        .query_row(
            "SELECT repository_id,canonical_manifest_json,manifest_sha256,design_sha256,
                    brainstorming_sha256,owner_approval_sha256,
                    registration_grant_revision,cloud_disclosure_grant_revision,
                    publication_grant_revision,provider_id,model_id,approved_at_ms
             FROM feature_specification_revisions
             WHERE feature_id = ?1 AND revision = ?2",
            params![
                specification.feature_id.to_string(),
                u64_to_i64(specification.revision)?
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                    row.get::<_, Vec<u8>>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, i64>(11)?,
                ))
            },
        )
        .optional()?;
    let Some(stored_specification) = stored_specification else {
        return Ok(None);
    };
    if parse_uuid(&stored_specification.0)? != specification.repository_id
        || stored_specification.1 != canonical_manifest
        || stored_specification.2.as_slice() != specification.manifest_sha256
        || stored_specification.3.as_slice() != specification.design_sha256
        || stored_specification.4.as_slice() != specification.brainstorming_sha256
        || stored_specification.5.as_slice() != specification.owner_approval_sha256
        || i64_to_u64(stored_specification.6)? != specification.grants.registration
        || i64_to_u64(stored_specification.7)? != specification.grants.cloud_disclosure
        || i64_to_u64(stored_specification.8)? != specification.grants.autonomous_publication
        || stored_specification.9 != specification.provider_id
        || stored_specification.10 != specification.model_id
    {
        return Ok(None);
    }
    let approved_at_ms = i64_to_u64(stored_specification.11)?;

    let stored_feature = tx
        .query_row(
            "SELECT f.current_specification_revision,f.status,f.lifecycle_revision,
                    f.queue_position,f.effect_possible,f.created_at_ms,f.updated_at_ms,
                    q.queue_position,l.lease_id
             FROM feature_conveyor_features f
             LEFT JOIN feature_conveyor_queue q ON q.feature_id = f.feature_id
             LEFT JOIN feature_active_lease l ON l.feature_id = f.feature_id
             WHERE f.feature_id = ?1",
            [specification.feature_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, Option<i64>>(7)?,
                    row.get::<_, Option<String>>(8)?,
                ))
            },
        )
        .optional()?;
    let Some(stored_feature) = stored_feature else {
        return Ok(None);
    };
    let queue_position = i64_to_u64(stored_feature.3)?;
    let queue_row_position = stored_feature.7.map(i64_to_u64).transpose()?;
    let maximum_queue_position: i64 = tx.query_row(
        "SELECT COALESCE(MAX(queue_position),0) FROM feature_conveyor_queue",
        [],
        |row| row.get(0),
    )?;
    if i64_to_u64(stored_feature.0)? != specification.revision
        || stored_feature.1 != FeatureLifecycleStatus::Queued.as_str()
        || i64_to_u64(stored_feature.2)? != 1
        || parse_stored_boolean(stored_feature.4, "feature effect_possible")?
        || i64_to_u64(stored_feature.5)? != approved_at_ms
        || i64_to_u64(stored_feature.6)? != approved_at_ms
        || queue_row_position != Some(queue_position)
        || i64_to_u64(maximum_queue_position)? != queue_position
        || stored_feature.8.is_some()
    {
        return Ok(None);
    }

    let mut dependency_statement = tx.prepare(
        "SELECT dependency_feature_id FROM feature_dependencies
         WHERE feature_id = ?1 ORDER BY dependency_feature_id",
    )?;
    let stored_dependencies = dependency_statement
        .query_map([specification.feature_id.to_string()], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(|dependency| parse_uuid(&dependency))
        .collect::<Result<Vec<_>, _>>()?;
    drop(dependency_statement);
    let mut expected_dependencies = specification.dependencies.clone();
    expected_dependencies.sort_unstable();
    if stored_dependencies != expected_dependencies {
        return Ok(None);
    }

    let mut audit_statement = tx.prepare(
        "SELECT event_kind,occurred_at_ms,redacted_metadata_json
         FROM feature_conveyor_audit WHERE feature_id = ?1 ORDER BY audit_id",
    )?;
    let audit_rows = audit_statement
        .query_map([specification.feature_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    drop(audit_statement);
    let expected_audit = canonical_json(&feature_enqueue_audit_metadata(
        specification,
        original_expected_queue_revision,
        replay_queue_revision,
        queue_position,
        Some((registration, designation_revision, emergency_pause_revision)),
    )?)?;
    if audit_rows.len() != 1
        || audit_rows[0].0 != "feature_enqueued"
        || i64_to_u64(audit_rows[0].1)? != approved_at_ms
        || audit_rows[0].2 != expected_audit
    {
        return Ok(None);
    }

    Ok(Some(FeatureSnapshot {
        feature_id: specification.feature_id,
        specification_revision: specification.revision,
        status: FeatureLifecycleStatus::Queued,
        lifecycle_revision: 1,
        queue_position,
        active_lease_id: None,
        effect_possible: false,
    }))
}

fn feature_enqueue_audit_metadata(
    specification: &ApprovedFeatureSpecification,
    expected_queue_revision: u64,
    queue_revision: u64,
    queue_position: u64,
    owner_binding: Option<(&DeviceRegistration, u64, u64)>,
) -> Result<Value, MasterError> {
    let mut metadata = serde_json::json!({
        "specification_revision": specification.revision,
        "queue_revision": queue_revision,
        "queue_position": queue_position,
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
    });
    if let Some((registration, designation_revision, emergency_pause_revision)) = owner_binding {
        let device_id = registration.device_id.0.to_string();
        let device_sha256 = lower_hex(&Sha256::digest(device_id.as_bytes()));
        let request_binding = serde_json::json!({
            "schema_version": 1,
            "operation": "owner_bridge_enqueue",
            "expected_queue_revision": expected_queue_revision,
            "owner_control_device_id": device_id,
            "owner_control_registry_revision": registration.registry_revision,
            "owner_control_designation_revision": designation_revision,
            "emergency_pause_revision": emergency_pause_revision,
            "specification": specification
        });
        let request_sha256 = lower_hex(&Sha256::digest(
            canonical_json(&request_binding)?.as_bytes(),
        ));
        let object = metadata.as_object_mut().ok_or_else(|| {
            MasterError::InvalidStoredState(
                "feature enqueue audit metadata was not an object".to_string(),
            )
        })?;
        object.insert(
            "owner_control_device_sha256".to_string(),
            Value::String(device_sha256),
        );
        object.insert(
            "owner_control_registry_revision".to_string(),
            Value::from(registration.registry_revision),
        );
        object.insert(
            "owner_control_designation_revision".to_string(),
            Value::from(designation_revision),
        );
        object.insert(
            "emergency_pause_revision".to_string(),
            Value::from(emergency_pause_revision),
        );
        object.insert(
            "owner_bridge_enqueue_request_sha256".to_string(),
            Value::String(request_sha256),
        );
    }
    Ok(metadata)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewSafeApprovedManifest {
    acceptance: Vec<String>,
    outcome: String,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    scope: Option<String>,
    #[serde(default)]
    allowed_paths: Vec<String>,
    #[serde(default)]
    validation_gate: Option<StoredValidationGateManifest>,
    #[serde(default)]
    assumptions: Vec<String>,
    #[serde(default)]
    risks: Vec<String>,
    #[serde(default)]
    non_goals: Vec<String>,
    #[serde(default)]
    decisions: Vec<String>,
    #[serde(default)]
    required_capabilities: Vec<String>,
    #[serde(default)]
    unit_test_obligations: Vec<String>,
    #[serde(default)]
    e2e_scenarios: Vec<String>,
    #[serde(default)]
    documentation_obligations: Vec<String>,
    #[serde(default)]
    knowledge_base_obligations: Vec<String>,
    #[serde(default)]
    prohibited_data: Vec<String>,
    #[serde(default)]
    publication_checks: Vec<String>,
    #[serde(default)]
    base_branch: Option<String>,
    #[serde(default)]
    security_classification: Option<String>,
    #[serde(default)]
    merge_strategy: Option<String>,
    #[serde(default)]
    post_merge_gate: Option<String>,
}

fn validate_review_safe_manifest_schema(value: &Value) -> Result<(), MasterError> {
    let manifest: ReviewSafeApprovedManifest =
        serde_json::from_value(value.clone()).map_err(|_| {
            MasterError::InvalidFeatureConveyorInput(
                "approved specification is not the strict review-safe manifest schema".to_string(),
            )
        })?;
    let mut strings = Vec::new();
    strings.push(manifest.outcome.as_str());
    strings.extend(manifest.title.as_deref());
    strings.extend(manifest.scope.as_deref());
    strings.extend(manifest.acceptance.iter().map(String::as_str));
    strings.extend(manifest.allowed_paths.iter().map(String::as_str));
    strings.extend(manifest.assumptions.iter().map(String::as_str));
    strings.extend(manifest.risks.iter().map(String::as_str));
    strings.extend(manifest.non_goals.iter().map(String::as_str));
    strings.extend(manifest.decisions.iter().map(String::as_str));
    strings.extend(manifest.required_capabilities.iter().map(String::as_str));
    strings.extend(manifest.unit_test_obligations.iter().map(String::as_str));
    strings.extend(manifest.e2e_scenarios.iter().map(String::as_str));
    strings.extend(
        manifest
            .documentation_obligations
            .iter()
            .map(String::as_str),
    );
    strings.extend(
        manifest
            .knowledge_base_obligations
            .iter()
            .map(String::as_str),
    );
    strings.extend(manifest.prohibited_data.iter().map(String::as_str));
    strings.extend(manifest.publication_checks.iter().map(String::as_str));
    strings.extend(manifest.base_branch.as_deref());
    strings.extend(manifest.security_classification.as_deref());
    strings.extend(manifest.merge_strategy.as_deref());
    strings.extend(manifest.post_merge_gate.as_deref());
    if strings
        .iter()
        .any(|value| value.is_empty() || value.len() > 4096)
        || manifest.allowed_paths.len() > 256
        || manifest.assumptions.len() > 128
        || manifest.risks.len() > 128
        || manifest.non_goals.len() > 128
        || manifest.decisions.len() > 128
        || manifest.required_capabilities.len() > 128
        || manifest.unit_test_obligations.len() > 128
        || manifest.e2e_scenarios.len() > 128
        || manifest.documentation_obligations.len() > 128
        || manifest.knowledge_base_obligations.len() > 128
        || manifest.prohibited_data.len() > 128
        || manifest.publication_checks.len() > 128
    {
        return Err(MasterError::InvalidFeatureConveyorInput(
            "approved specification review-safe fields exceed fixed bounds".to_string(),
        ));
    }
    if let Some(gate) = manifest.validation_gate {
        if gate.schema_version != FEATURE_CONVEYOR_VALIDATION_GATE_SCHEMA_VERSION
            || gate.command_ids != FeatureConveyorValidationCommandId::REQUIRED
        {
            return Err(MasterError::InvalidFeatureConveyorInput(
                "approved specification validation gate is not exact".to_string(),
            ));
        }
    }
    Ok(())
}

fn approved_requirement_ids(manifest: &Value) -> Result<Vec<String>, MasterError> {
    let acceptance = manifest
        .as_object()
        .and_then(|object| object.get("acceptance"))
        .and_then(Value::as_array)
        .ok_or_else(|| {
            MasterError::InvalidFeatureConveyorInput(
                "approved specification requires an ordered acceptance array".to_string(),
            )
        })?;
    if acceptance.is_empty() || acceptance.len() > MAX_FEATURE_CONVEYOR_REVIEW_REQUIREMENT_COVERAGE
    {
        return Err(MasterError::InvalidFeatureConveyorInput(
            "approved specification acceptance count is outside the review envelope".to_string(),
        ));
    }
    let mut ids = Vec::with_capacity(acceptance.len());
    let mut unique = HashSet::new();
    for value in acceptance {
        let id = value.as_str().ok_or_else(|| {
            MasterError::InvalidFeatureConveyorInput(
                "approved specification acceptance IDs must be strings".to_string(),
            )
        })?;
        validate_bounded_feature_identifier(id, "acceptance criterion")?;
        if !unique.insert(id) {
            return Err(MasterError::InvalidFeatureConveyorInput(
                "approved specification acceptance IDs must be unique".to_string(),
            ));
        }
        ids.push(id.to_string());
    }
    Ok(ids)
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

fn activation_evidence_category_str(
    category: FeatureConveyorActivationEvidenceCategory,
) -> &'static str {
    match category {
        FeatureConveyorActivationEvidenceCategory::RepositoryGateProof => "repository_gate_proof",
        FeatureConveyorActivationEvidenceCategory::RestrictedWorkerLive => "restricted_worker_live",
        FeatureConveyorActivationEvidenceCategory::ReviewProviderLive => "review_provider_live",
        FeatureConveyorActivationEvidenceCategory::GithubPublicationLive => {
            "github_publication_live"
        }
        FeatureConveyorActivationEvidenceCategory::RestartRecoveryLive => "restart_recovery_live",
        FeatureConveyorActivationEvidenceCategory::MacWindowsControlEventStreamingLive => {
            "mac_windows_control_event_streaming_live"
        }
    }
}

fn parse_activation_evidence_category(
    category: &str,
) -> Result<FeatureConveyorActivationEvidenceCategory, MasterError> {
    match category {
        "repository_gate_proof" => {
            Ok(FeatureConveyorActivationEvidenceCategory::RepositoryGateProof)
        }
        "restricted_worker_live" => {
            Ok(FeatureConveyorActivationEvidenceCategory::RestrictedWorkerLive)
        }
        "review_provider_live" => Ok(FeatureConveyorActivationEvidenceCategory::ReviewProviderLive),
        "github_publication_live" => {
            Ok(FeatureConveyorActivationEvidenceCategory::GithubPublicationLive)
        }
        "restart_recovery_live" => {
            Ok(FeatureConveyorActivationEvidenceCategory::RestartRecoveryLive)
        }
        "mac_windows_control_event_streaming_live" => {
            Ok(FeatureConveyorActivationEvidenceCategory::MacWindowsControlEventStreamingLive)
        }
        _ => Err(MasterError::InvalidStoredState(
            "unknown activation evidence category".to_string(),
        )),
    }
}

fn activation_evidence_origin_str(origin: FeatureConveyorActivationEvidenceOrigin) -> &'static str {
    match origin {
        FeatureConveyorActivationEvidenceOrigin::RepositoryGateProofController => {
            "repository_gate_proof_controller"
        }
        FeatureConveyorActivationEvidenceOrigin::RestrictedWorkerProofController => {
            "restricted_worker_proof_controller"
        }
        FeatureConveyorActivationEvidenceOrigin::ReviewProviderProofController => {
            "review_provider_proof_controller"
        }
        FeatureConveyorActivationEvidenceOrigin::GithubPublicationProofController => {
            "github_publication_proof_controller"
        }
        FeatureConveyorActivationEvidenceOrigin::RestartRecoveryProofController => {
            "restart_recovery_proof_controller"
        }
        FeatureConveyorActivationEvidenceOrigin::MacWindowsControlEventStreamingProofController => {
            "mac_windows_control_event_streaming_proof_controller"
        }
    }
}

fn parse_activation_evidence_origin(
    origin: &str,
) -> Result<FeatureConveyorActivationEvidenceOrigin, MasterError> {
    match origin {
        "repository_gate_proof_controller" => {
            Ok(FeatureConveyorActivationEvidenceOrigin::RepositoryGateProofController)
        }
        "restricted_worker_proof_controller" => {
            Ok(FeatureConveyorActivationEvidenceOrigin::RestrictedWorkerProofController)
        }
        "review_provider_proof_controller" => {
            Ok(FeatureConveyorActivationEvidenceOrigin::ReviewProviderProofController)
        }
        "github_publication_proof_controller" => {
            Ok(FeatureConveyorActivationEvidenceOrigin::GithubPublicationProofController)
        }
        "restart_recovery_proof_controller" => {
            Ok(FeatureConveyorActivationEvidenceOrigin::RestartRecoveryProofController)
        }
        "mac_windows_control_event_streaming_proof_controller" => Ok(
            FeatureConveyorActivationEvidenceOrigin::MacWindowsControlEventStreamingProofController,
        ),
        _ => Err(MasterError::InvalidStoredState(
            "unknown activation evidence origin".to_string(),
        )),
    }
}

fn activation_evidence_receipt(
    request: &FeatureConveyorActivationEvidenceAdmissionRequest,
) -> FeatureConveyorActivationEvidenceAdmissionReceipt {
    FeatureConveyorActivationEvidenceAdmissionReceipt {
        schema_version: FEATURE_CONVEYOR_OWNER_ACTIVATION_SCHEMA_VERSION,
        category: request.category,
        origin: request.origin,
        evidence: FeatureConveyorActivationEvidenceReference {
            evidence_id: request.evidence_id,
            revision: request.revision,
            receipt_sha256: request.receipt_sha256,
        },
        observed_at_ms: request.observed_at_ms,
        emergency_pause_revision: request.expected_emergency_pause_revision,
    }
}

fn activation_evidence_by_id_tx(
    tx: &Transaction<'_>,
    evidence_id: Uuid,
) -> Result<Option<FeatureConveyorActivationEvidenceAdmissionReceipt>, MasterError> {
    tx.query_row(
        "SELECT category,revision,origin,receipt_sha256,observed_at_ms,emergency_pause_revision
         FROM feature_activation_evidence WHERE evidence_id=?1",
        [evidence_id.to_string()],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Vec<u8>>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
            ))
        },
    )
    .optional()?
    .map(
        |(category, revision, origin, digest, observed_at_ms, pause_revision)| {
            Ok(FeatureConveyorActivationEvidenceAdmissionReceipt {
                schema_version: FEATURE_CONVEYOR_OWNER_ACTIVATION_SCHEMA_VERSION,
                category: parse_activation_evidence_category(&category)?,
                origin: parse_activation_evidence_origin(&origin)?,
                evidence: FeatureConveyorActivationEvidenceReference {
                    evidence_id,
                    revision: i64_to_u64(revision)?,
                    receipt_sha256: digest_array(&digest)?,
                },
                observed_at_ms: i64_to_u64(observed_at_ms)?,
                emergency_pause_revision: i64_to_u64(pause_revision)?,
            })
        },
    )
    .transpose()
}

fn current_activation_evidence_reference_tx(
    tx: &Transaction<'_>,
    category: FeatureConveyorActivationEvidenceCategory,
) -> Result<Option<FeatureConveyorActivationEvidenceReference>, MasterError> {
    tx.query_row(
        "SELECT evidence_id,revision,receipt_sha256 FROM feature_activation_evidence
         WHERE category=?1 ORDER BY revision DESC LIMIT 1",
        [activation_evidence_category_str(category)],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Vec<u8>>(2)?,
            ))
        },
    )
    .optional()?
    .map(|(evidence_id, revision, digest)| {
        Ok(FeatureConveyorActivationEvidenceReference {
            evidence_id: parse_uuid(&evidence_id)?,
            revision: i64_to_u64(revision)?,
            receipt_sha256: digest_array(&digest)?,
        })
    })
    .transpose()
}

fn activation_evidence_projection_tx(
    tx: &Transaction<'_>,
) -> Result<FeatureConveyorActivationEvidenceProjection, MasterError> {
    Ok(FeatureConveyorActivationEvidenceProjection {
        repository_gate_proof: current_activation_evidence_reference_tx(
            tx,
            FeatureConveyorActivationEvidenceCategory::RepositoryGateProof,
        )?,
        restricted_worker_live: current_activation_evidence_reference_tx(
            tx,
            FeatureConveyorActivationEvidenceCategory::RestrictedWorkerLive,
        )?,
        review_provider_live: current_activation_evidence_reference_tx(
            tx,
            FeatureConveyorActivationEvidenceCategory::ReviewProviderLive,
        )?,
        github_publication_live: current_activation_evidence_reference_tx(
            tx,
            FeatureConveyorActivationEvidenceCategory::GithubPublicationLive,
        )?,
        restart_recovery_live: current_activation_evidence_reference_tx(
            tx,
            FeatureConveyorActivationEvidenceCategory::RestartRecoveryLive,
        )?,
        mac_windows_control_event_streaming_live: current_activation_evidence_reference_tx(
            tx,
            FeatureConveyorActivationEvidenceCategory::MacWindowsControlEventStreamingLive,
        )?,
    })
}

fn require_current_activation_evidence_tx(
    tx: &Transaction<'_>,
    requested: &FeatureConveyorActivationEvidenceSet,
) -> Result<(), MasterError> {
    requested.validate()?;
    let current = activation_evidence_projection_tx(tx)?
        .complete()
        .ok_or(MasterError::FeatureActivationEvidenceUnavailable)?;
    if current != *requested {
        return Err(MasterError::FeatureActivationEvidenceUnavailable);
    }
    Ok(())
}

fn load_feature_activation_tx(
    tx: &Transaction<'_>,
) -> Result<Option<FeatureConveyorActivationReceipt>, MasterError> {
    tx.query_row(
        "SELECT activation_id,queue_revision,owner_control_designation_revision,
                emergency_pause_revision,repository_gate_evidence_id,
                restricted_worker_evidence_id,review_provider_evidence_id,
                github_publication_evidence_id,restart_recovery_evidence_id,
                control_event_streaming_evidence_id,activated_at_ms
         FROM feature_orchestration_activation WHERE singleton=1",
        [],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, String>(8)?,
                row.get::<_, String>(9)?,
                row.get::<_, i64>(10)?,
            ))
        },
    )
    .optional()?
    .map(|row| {
        let receipt = FeatureConveyorActivationReceipt {
            schema_version: FEATURE_CONVEYOR_OWNER_ACTIVATION_SCHEMA_VERSION,
            activation_id: parse_uuid(&row.0)?,
            queue_revision: i64_to_u64(row.1)?,
            owner_control_designation_revision: i64_to_u64(row.2)?,
            emergency_pause_revision: i64_to_u64(row.3)?,
            evidence: FeatureConveyorActivationEvidenceSet {
                repository_gate_proof: activation_evidence_reference_for_role_tx(
                    tx,
                    &row.4,
                    FeatureConveyorActivationEvidenceCategory::RepositoryGateProof,
                    FeatureConveyorActivationEvidenceOrigin::RepositoryGateProofController,
                )?,
                restricted_worker_live: activation_evidence_reference_for_role_tx(
                    tx,
                    &row.5,
                    FeatureConveyorActivationEvidenceCategory::RestrictedWorkerLive,
                    FeatureConveyorActivationEvidenceOrigin::RestrictedWorkerProofController,
                )?,
                review_provider_live: activation_evidence_reference_for_role_tx(
                    tx,
                    &row.6,
                    FeatureConveyorActivationEvidenceCategory::ReviewProviderLive,
                    FeatureConveyorActivationEvidenceOrigin::ReviewProviderProofController,
                )?,
                github_publication_live: activation_evidence_reference_for_role_tx(
                    tx,
                    &row.7,
                    FeatureConveyorActivationEvidenceCategory::GithubPublicationLive,
                    FeatureConveyorActivationEvidenceOrigin::GithubPublicationProofController,
                )?,
                restart_recovery_live: activation_evidence_reference_for_role_tx(
                    tx,
                    &row.8,
                    FeatureConveyorActivationEvidenceCategory::RestartRecoveryLive,
                    FeatureConveyorActivationEvidenceOrigin::RestartRecoveryProofController,
                )?,
                mac_windows_control_event_streaming_live:
                    activation_evidence_reference_for_role_tx(
                        tx,
                        &row.9,
                        FeatureConveyorActivationEvidenceCategory::MacWindowsControlEventStreamingLive,
                        FeatureConveyorActivationEvidenceOrigin::MacWindowsControlEventStreamingProofController,
                    )?,
            },
            activated_at_ms: i64_to_u64(row.10)?,
            status: FeatureConveyorActivationStatus::Active,
        };
        receipt.validate()?;
        Ok(receipt)
    })
    .transpose()
}

fn activation_evidence_reference_for_role_tx(
    tx: &Transaction<'_>,
    evidence_id: &str,
    expected_category: FeatureConveyorActivationEvidenceCategory,
    expected_origin: FeatureConveyorActivationEvidenceOrigin,
) -> Result<FeatureConveyorActivationEvidenceReference, MasterError> {
    let receipt = activation_evidence_by_id_tx(tx, parse_uuid(evidence_id)?)?.ok_or_else(|| {
        MasterError::InvalidStoredState("activation evidence is missing".to_string())
    })?;
    receipt.validate()?;
    if receipt.category != expected_category || receipt.origin != expected_origin {
        return Err(MasterError::InvalidStoredState(
            "activation evidence role binding is invalid".to_string(),
        ));
    }
    Ok(receipt.evidence)
}

fn activation_receipt_matches_request(
    receipt: &FeatureConveyorActivationReceipt,
    request: &FeatureConveyorActivationRequest,
) -> bool {
    receipt.queue_revision == request.expected_queue_revision
        && receipt.owner_control_designation_revision
            == request.expected_owner_control_designation_revision
        && receipt.emergency_pause_revision == request.expected_emergency_pause_revision
        && receipt.evidence == request.evidence
}

fn activation_id_for_request(request: &FeatureConveyorActivationRequest) -> Uuid {
    let mut hasher = Sha256::new();
    hasher.update(b"assemblywright.feature-orchestration-activation.v1\0");
    hasher.update(serde_json::to_vec(request).expect("validated activation request serializes"));
    let digest: [u8; 32] = hasher.finalize().into();
    orchestration_checkpoint_id(digest)
}

fn owner_orchestration_control_request_sha256(
    action: &str,
    request: &FeatureConveyorOwnerOrchestrationControlRequest,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(b"assemblywright.owner-orchestration-control.v1\0");
    hasher.update(action.as_bytes());
    hasher.update([0]);
    hasher.update(serde_json::to_vec(request).expect("validated owner control serializes"));
    hasher.finalize().into()
}

fn load_owner_orchestration_control_tx(
    tx: &Transaction<'_>,
    request_sha256: &[u8; 32],
) -> Result<Option<FeatureConveyorOwnerOrchestrationControlReceipt>, MasterError> {
    tx.query_row(
        "SELECT action,feature_id,lifecycle_revision,orchestration_revision,queue_revision,
                owner_control_designation_revision,emergency_pause_revision,checkpoint_id,
                checkpoint_sha256
         FROM feature_owner_orchestration_controls WHERE request_sha256=?1",
        [request_sha256.as_slice()],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, String>(7)?,
                row.get::<_, Vec<u8>>(8)?,
            ))
        },
    )
    .optional()?
    .map(|row| {
        let status = match row.0.as_str() {
            "pause" => FeatureConveyorOwnerOrchestrationControlStatus::Paused,
            "resume" => FeatureConveyorOwnerOrchestrationControlStatus::Resumed,
            _ => {
                return Err(MasterError::InvalidStoredState(
                    "unknown owner orchestration control".to_string(),
                ))
            }
        };
        let receipt = FeatureConveyorOwnerOrchestrationControlReceipt {
            schema_version: FEATURE_CONVEYOR_OWNER_ACTIVATION_SCHEMA_VERSION,
            feature_id: parse_uuid(&row.1)?,
            lifecycle_revision: i64_to_u64(row.2)?,
            orchestration_revision: i64_to_u64(row.3)?,
            queue_revision: i64_to_u64(row.4)?,
            owner_control_designation_revision: i64_to_u64(row.5)?,
            emergency_pause_revision: i64_to_u64(row.6)?,
            checkpoint_id: parse_uuid(&row.7)?,
            checkpoint_sha256: digest_array(&row.8)?,
            status,
        };
        receipt.validate()?;
        Ok(receipt)
    })
    .transpose()
}

fn orchestration_action_for_stage(
    stage: FeatureConveyorOrchestrationStage,
) -> FeatureConveyorOrchestrationAction {
    match stage {
        FeatureConveyorOrchestrationStage::Implementing => {
            FeatureConveyorOrchestrationAction::AwaitImplementationEvidence
        }
        FeatureConveyorOrchestrationStage::Validating => {
            FeatureConveyorOrchestrationAction::AwaitValidationEvidence
        }
        FeatureConveyorOrchestrationStage::Reviewing => {
            FeatureConveyorOrchestrationAction::AwaitReviewDecision
        }
        FeatureConveyorOrchestrationStage::Publishing => {
            FeatureConveyorOrchestrationAction::AwaitPublicationEvidence
        }
        FeatureConveyorOrchestrationStage::VerifyingMain => {
            FeatureConveyorOrchestrationAction::AwaitMainVerification
        }
        FeatureConveyorOrchestrationStage::Repairing => {
            FeatureConveyorOrchestrationAction::ReplacementCandidateRequired
        }
        FeatureConveyorOrchestrationStage::Paused => {
            FeatureConveyorOrchestrationAction::OwnerAttentionRequired
        }
        FeatureConveyorOrchestrationStage::AttentionRequired => {
            FeatureConveyorOrchestrationAction::OwnerAttentionRequired
        }
        FeatureConveyorOrchestrationStage::Failed
        | FeatureConveyorOrchestrationStage::Succeeded => {
            FeatureConveyorOrchestrationAction::Terminal
        }
        FeatureConveyorOrchestrationStage::Quarantined => {
            FeatureConveyorOrchestrationAction::ReconcileQuarantine
        }
    }
}

fn feature_queue_revision_tx(tx: &Transaction<'_>) -> Result<u64, MasterError> {
    let revision: i64 = tx.query_row(
        "SELECT queue_revision FROM feature_conveyor_state WHERE singleton=1",
        [],
        |row| row.get(0),
    )?;
    i64_to_u64(revision)
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

fn validate_snapshot_claim_plan(
    plan: &FeatureSnapshotClaimPlan,
    now_ms: u64,
) -> Result<(), MasterError> {
    let valid_commit = plan.base_commit.len() == 40
        && plan
            .base_commit
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    let valid_provider = |value: &str| {
        !value.is_empty()
            && value.len() <= 128
            && value.trim() == value
            && value.bytes().all(|byte| byte.is_ascii_graphic())
    };
    if plan.feature_id.is_nil()
        || plan.repository_id.is_nil()
        || plan.specification_revision == 0
        || plan.scope_sha256 == [0; 32]
        || plan.grants.registration == 0
        || plan.grants.cloud_disclosure == 0
        || plan.grants.autonomous_publication == 0
        || !valid_commit
        || !valid_provider(&plan.provider_id)
        || !valid_provider(&plan.model_id)
        || now_ms == 0
    {
        return Err(MasterError::InvalidFeatureConveyorInput(
            "snapshot claim plan requires exact bounded nonzero bindings".to_string(),
        ));
    }
    Ok(())
}

fn require_snapshot_claim_plan_tx(
    tx: &Transaction<'_>,
    plan: &FeatureSnapshotClaimPlan,
    now_ms: u64,
) -> Result<(), MasterError> {
    require_queue_revision_tx(tx, plan.expected_queue_revision)?;
    require_unpaused_revision_tx(tx, plan.expected_emergency_pause_revision)?;
    require_repository_preflight_binding(
        tx,
        plan.repository_id,
        plan.grants.registration,
        &plan.scope_sha256,
        plan.expected_emergency_pause_revision,
        now_ms,
    )?;
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
    if parse_uuid(&feature_id)? != plan.feature_id
        || i64_to_u64(specification_revision)? != plan.specification_revision
    {
        return Err(MasterError::InvalidFeatureConveyorInput(
            "snapshot claim does not bind the exact strict queue head".to_string(),
        ));
    }
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
    let stored: (String, i64, i64, i64, String, String) = tx.query_row(
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
    let stored_grants = FeatureGrantRevisions {
        registration: i64_to_u64(stored.1)?,
        cloud_disclosure: i64_to_u64(stored.2)?,
        autonomous_publication: i64_to_u64(stored.3)?,
    };
    if parse_uuid(&stored.0)? != plan.repository_id
        || stored_grants != plan.grants
        || stored.4 != plan.provider_id
        || stored.5 != plan.model_id
    {
        return Err(MasterError::InvalidFeatureConveyorInput(
            "snapshot claim binding differs from the approved specification".to_string(),
        ));
    }
    require_grants_tx(tx, plan.repository_id, plan.grants, now_ms)
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyFeatureCancellationAudit {
    from_status: String,
    to_status: String,
    lifecycle_revision: u64,
    #[serde(default)]
    queue_revision: Option<u64>,
    #[serde(default)]
    emergency_pause_revision: Option<u64>,
    lease_retained: bool,
    advancement_authorized: bool,
    effect_possible: bool,
    side_effect_executed: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyFeatureStartupQuarantineAudit {
    from_status: String,
    to_status: String,
    lifecycle_revision: u64,
    lease_retained: bool,
    automatic_retry_authorized: bool,
    effect_possible: bool,
    side_effect_executed: bool,
}

fn backfill_legacy_feature_resolution_evidence_tx(tx: &Transaction<'_>) -> Result<(), MasterError> {
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
    let Some((feature_id, status, lifecycle_revision)) = active else {
        return Ok(());
    };
    let status = FeatureLifecycleStatus::parse(&status)?;
    if !matches!(
        status,
        FeatureLifecycleStatus::Cancelled
            | FeatureLifecycleStatus::Quarantined
            | FeatureLifecycleStatus::AttentionRequired
            | FeatureLifecycleStatus::Failed
    ) {
        return Ok(());
    }
    let feature_uuid = parse_uuid(&feature_id)?;
    let lifecycle_revision = i64_to_u64(lifecycle_revision)?;
    let receipt_exists: bool = tx.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM feature_transition_evidence
           WHERE feature_id = ?1 AND lifecycle_revision = ?2
         )",
        params![feature_id, u64_to_i64(lifecycle_revision)?],
        |row| row.get(0),
    )?;
    if receipt_exists {
        feature_resolution_origin_tx(tx, feature_uuid, status, lifecycle_revision)?;
        return Ok(());
    }

    // Attention/failure were introduced with immutable transition evidence.
    // There is no legacy audit shape from which an exact origin can safely be
    // inferred, so a missing receipt is corruption rather than a backfill.
    if matches!(
        status,
        FeatureLifecycleStatus::AttentionRequired | FeatureLifecycleStatus::Failed
    ) {
        return Err(MasterError::InvalidStoredState(
            "feature resolution transition evidence is missing".to_string(),
        ));
    }

    let event_kind = match status {
        FeatureLifecycleStatus::Cancelled => "feature_cancelled",
        FeatureLifecycleStatus::Quarantined => "feature_startup_quarantined",
        _ => unreachable!("resolution status checked above"),
    };
    let mut statement = tx.prepare(
        "SELECT occurred_at_ms, redacted_metadata_json
         FROM feature_conveyor_audit
         WHERE event_kind = ?1 AND feature_id = ?2
         ORDER BY audit_id",
    )?;
    let candidates = statement
        .query_map(params![event_kind, feature_id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if candidates.len() != 1 {
        return Err(MasterError::InvalidStoredState(
            "legacy feature resolution audit evidence is missing or ambiguous".to_string(),
        ));
    }
    let (recorded_at_ms, metadata_json) = &candidates[0];
    let from_status = match status {
        FeatureLifecycleStatus::Cancelled => {
            let metadata: LegacyFeatureCancellationAudit = serde_json::from_str(metadata_json)
                .map_err(|_| {
                    MasterError::InvalidStoredState(
                        "legacy feature cancellation audit evidence is malformed".to_string(),
                    )
                })?;
            if metadata.to_status != FeatureLifecycleStatus::Cancelled.as_str()
                || metadata.lifecycle_revision != lifecycle_revision
                || !metadata.lease_retained
                || metadata.advancement_authorized
                || !metadata.effect_possible
                || metadata.side_effect_executed
                || metadata.queue_revision.is_some() != metadata.emergency_pause_revision.is_some()
            {
                return Err(MasterError::InvalidStoredState(
                    "legacy feature cancellation audit evidence is malformed".to_string(),
                ));
            }
            metadata.from_status
        }
        FeatureLifecycleStatus::Quarantined => {
            let metadata: LegacyFeatureStartupQuarantineAudit = serde_json::from_str(metadata_json)
                .map_err(|_| {
                    MasterError::InvalidStoredState(
                        "legacy feature quarantine audit evidence is malformed".to_string(),
                    )
                })?;
            if metadata.to_status != FeatureLifecycleStatus::Quarantined.as_str()
                || metadata.lifecycle_revision != lifecycle_revision
                || !metadata.lease_retained
                || metadata.automatic_retry_authorized
                || !metadata.effect_possible
                || metadata.side_effect_executed
            {
                return Err(MasterError::InvalidStoredState(
                    "legacy feature quarantine audit evidence is malformed".to_string(),
                ));
            }
            metadata.from_status
        }
        _ => unreachable!("resolution status checked above"),
    };
    let from_status = FeatureLifecycleStatus::parse(&from_status)?;
    if !from_status.is_active_execution() {
        return Err(MasterError::InvalidStoredState(
            "legacy feature resolution origin is not active execution".to_string(),
        ));
    }
    tx.execute(
        "INSERT INTO feature_transition_evidence (
           feature_id, lifecycle_revision, from_status, to_status, recorded_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            feature_id,
            u64_to_i64(lifecycle_revision)?,
            from_status.as_str(),
            status.as_str(),
            recorded_at_ms,
        ],
    )?;
    feature_resolution_origin_tx(tx, feature_uuid, status, lifecycle_revision)?;
    Ok(())
}

fn feature_resolution_origin_tx(
    tx: &Transaction<'_>,
    feature_id: Uuid,
    resolution_status: FeatureLifecycleStatus,
    lifecycle_revision: u64,
) -> Result<FeatureLifecycleStatus, MasterError> {
    let evidence = tx
        .query_row(
            "SELECT from_status, to_status,
                    repository_snapshot_sha256, accepted_evidence_sha256,
                    verified_main_commit_sha256, post_merge_evidence_sha256,
                    safe_reconciliation_sha256, verified_healthy_main_sha256
             FROM feature_transition_evidence
             WHERE feature_id = ?1 AND lifecycle_revision = ?2",
            params![feature_id.to_string(), u64_to_i64(lifecycle_revision)?,],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<Vec<u8>>>(2)?,
                    row.get::<_, Option<Vec<u8>>>(3)?,
                    row.get::<_, Option<Vec<u8>>>(4)?,
                    row.get::<_, Option<Vec<u8>>>(5)?,
                    row.get::<_, Option<Vec<u8>>>(6)?,
                    row.get::<_, Option<Vec<u8>>>(7)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| {
            MasterError::InvalidStoredState(
                "feature resolution transition evidence is missing".to_string(),
            )
        })?;
    let from_status = FeatureLifecycleStatus::parse(&evidence.0)?;
    if !matches!(
        resolution_status,
        FeatureLifecycleStatus::Cancelled
            | FeatureLifecycleStatus::Quarantined
            | FeatureLifecycleStatus::AttentionRequired
            | FeatureLifecycleStatus::Failed
    ) || evidence.1 != resolution_status.as_str()
        || !from_status.is_active_execution()
        || evidence.2.is_some()
        || evidence.3.is_some()
        || evidence.4.is_some()
        || evidence.5.is_some()
        || evidence.6.is_some()
        || evidence.7.is_some()
    {
        return Err(MasterError::InvalidStoredState(
            "feature resolution transition evidence is malformed".to_string(),
        ));
    }
    if from_status == FeatureLifecycleStatus::VerifyingMain {
        let prior_revision = lifecycle_revision.checked_sub(1).ok_or_else(|| {
            MasterError::InvalidStoredState(
                "verified-main resolution evidence revision underflowed".to_string(),
            )
        })?;
        let prior = tx
            .query_row(
                "SELECT from_status, to_status,
                        repository_snapshot_sha256, accepted_evidence_sha256,
                        verified_main_commit_sha256, post_merge_evidence_sha256,
                        safe_reconciliation_sha256, verified_healthy_main_sha256
                 FROM feature_transition_evidence
                 WHERE feature_id = ?1 AND lifecycle_revision = ?2",
                params![feature_id.to_string(), u64_to_i64(prior_revision)?,],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, Option<Vec<u8>>>(2)?,
                        row.get::<_, Option<Vec<u8>>>(3)?,
                        row.get::<_, Option<Vec<u8>>>(4)?,
                        row.get::<_, Option<Vec<u8>>>(5)?,
                        row.get::<_, Option<Vec<u8>>>(6)?,
                        row.get::<_, Option<Vec<u8>>>(7)?,
                    ))
                },
            )
            .optional()?
            .ok_or_else(|| {
                MasterError::InvalidStoredState(
                    "verified-main transition evidence is missing".to_string(),
                )
            })?;
        let snapshot_digest = prior.2.as_deref().map(digest_array).transpose()?;
        let accepted_digest = prior.3.as_deref().map(digest_array).transpose()?;
        if prior.0 != FeatureLifecycleStatus::Publishing.as_str()
            || prior.1 != FeatureLifecycleStatus::VerifyingMain.as_str()
            || snapshot_digest.is_none_or(|digest| digest == [0; 32])
            || accepted_digest.is_none_or(|digest| digest == [0; 32])
            || prior.4.is_some()
            || prior.5.is_some()
            || prior.6.is_some()
            || prior.7.is_some()
        {
            return Err(MasterError::InvalidStoredState(
                "verified-main transition evidence is malformed".to_string(),
            ));
        }
    }
    Ok(from_status)
}

fn publication_merge_effect_possible_tx(
    tx: &Transaction<'_>,
    feature_id: Uuid,
) -> Result<bool, MasterError> {
    tx.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM feature_publications p
           JOIN feature_publication_action_intents i
             ON i.publication_id=p.publication_id
           WHERE p.feature_id=?1 AND i.action_kind='merge_pull_request'
         )",
        [feature_id.to_string()],
        |row| row.get(0),
    )
    .map_err(MasterError::from)
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

fn validate_integration_binding_tx(
    tx: &Transaction<'_>,
    request: &FeatureConveyorArtifactIntegrationRequest,
    now_ms: u64,
) -> Result<(), MasterError> {
    require_unpaused_revision_tx(tx, request.expected_emergency_pause_revision)?;
    require_queue_revision_tx(tx, request.expected_queue_revision)?;
    let binding = tx
        .query_row(
            "SELECT f.current_specification_revision, f.lifecycle_revision, f.status,
                l.lease_id, l.snapshot_id, c.snapshot_sha256, c.base_commit,
                c.registration_grant_revision, c.cloud_disclosure_grant_revision,
                c.publication_grant_revision, s.repository_id
         FROM feature_conveyor_features f
         JOIN feature_active_lease l ON l.feature_id=f.feature_id
         JOIN feature_repository_snapshot_claims c ON c.snapshot_id=l.snapshot_id
         JOIN feature_specification_revisions s
           ON s.feature_id=f.feature_id AND s.revision=f.current_specification_revision
         WHERE f.feature_id=?1",
            [request.feature_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Vec<u8>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, String>(10)?,
                ))
            },
        )
        .optional()?
        .ok_or(MasterError::ArtifactIntegrationUnavailable)?;
    let repository_id = parse_uuid(&binding.10)?;
    if i64_to_u64(binding.0)? != request.specification_revision
        || i64_to_u64(binding.1)? != request.expected_lifecycle_revision
        || binding.2 != "implementing"
        || parse_uuid(&binding.3)? != request.feature_lease_id
        || parse_uuid(&binding.4)? != request.snapshot_id
        || digest_array(&binding.5)? != request.snapshot_sha256
        || binding.6 != request.base_commit
        || i64_to_u64(binding.7)? != request.grants.registration
        || i64_to_u64(binding.8)? != request.grants.cloud_disclosure
        || i64_to_u64(binding.9)? != request.grants.autonomous_publication
    {
        return Err(MasterError::ArtifactIntegrationUnavailable);
    }
    require_grants_tx(
        tx,
        repository_id,
        FeatureGrantRevisions {
            registration: request.grants.registration,
            cloud_disclosure: request.grants.cloud_disclosure,
            autonomous_publication: request.grants.autonomous_publication,
        },
        now_ms,
    )
    .map_err(|_| MasterError::ArtifactIntegrationUnavailable)
}

fn load_complete_integration_artifacts_tx(
    tx: &Transaction<'_>,
    request: &FeatureConveyorArtifactIntegrationRequest,
    enforce_requested_ids: bool,
) -> Result<Vec<IntegrationArtifact>, MasterError> {
    let total: i64 = tx.query_row(
        "SELECT COUNT(*) FROM feature_coding_dispatches WHERE feature_id=?1",
        [request.feature_id.to_string()],
        |row| row.get(0),
    )?;
    if !(1..=3).contains(&total) {
        return Err(MasterError::ArtifactIntegrationUnavailable);
    }
    let mut statement = tx.prepare(
        "SELECT a.artifact_id, a.artifact_sha256, a.artifact_size_bytes,
                d.work_packet_metadata_json, s.accepted_payload_json
         FROM feature_coding_dispatches d
         JOIN master_steps s ON s.step_id=d.step_id AND s.status='succeeded'
         JOIN feature_result_artifacts a ON a.step_id=d.step_id AND a.feature_id=d.feature_id
         WHERE d.feature_id=?1 AND d.specification_revision=?2 AND d.lifecycle_revision=?3
           AND d.feature_lease_id=?4 AND d.snapshot_id=?5 AND d.snapshot_sha256=?6",
    )?;
    let rows = statement.query_map(
        params![
            request.feature_id.to_string(),
            u64_to_i64(request.specification_revision)?,
            u64_to_i64(request.expected_lifecycle_revision)?,
            request.feature_lease_id.to_string(),
            request.snapshot_id.to_string(),
            request.snapshot_sha256.as_slice()
        ],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        },
    )?;
    let mut artifacts = Vec::new();
    let mut ordinals = HashSet::new();
    for row in rows {
        let (id, digest, size, packet_json, payload_json) = row?;
        let packet: FeatureConveyorCodingWorkPacketMetadata = serde_json::from_str(&packet_json)?;
        packet.validate()?;
        if !ordinals.insert(packet.ordinal) {
            return Err(MasterError::ArtifactIntegrationConflict);
        }
        let payload: LocalCodingJobResult = serde_json::from_str(&payload_json)?;
        let reference = ResultArtifactReference {
            artifact_id: parse_uuid(&id)?,
            artifact_sha256: digest_array(&digest)?,
            artifact_size_bytes: i64_to_u64(size)?,
        };
        if payload.artifact_id != reference.artifact_id
            || payload.artifact_sha256 != reference.artifact_sha256
            || payload.artifact_size_bytes != reference.artifact_size_bytes
            || payload.ambiguous
            || payload.status != assemblywright_protocol::LOCAL_CODING_COMPLETED_STATUS
        {
            return Err(MasterError::ArtifactIntegrationUnavailable);
        }
        artifacts.push(IntegrationArtifact { reference, packet });
    }
    if artifacts.len() as i64 != total {
        return Err(MasterError::ArtifactIntegrationUnavailable);
    }
    let mut ids: Vec<_> = artifacts.iter().map(|a| a.reference.artifact_id).collect();
    ids.sort();
    if enforce_requested_ids && ids != request.artifact_ids {
        return Err(MasterError::ArtifactIntegrationUnavailable);
    }
    artifacts.sort_by(|left, right| {
        left.packet
            .ordinal
            .cmp(&right.packet.ordinal)
            .then_with(|| left.packet.packet_id.cmp(&right.packet.packet_id))
    });
    Ok(artifacts)
}

fn load_complete_integration_artifacts_tx_without_requested_ids(
    tx: &Transaction<'_>,
    request: &FeatureConveyorArtifactIntegrationRequest,
) -> Result<Vec<IntegrationArtifact>, MasterError> {
    load_complete_integration_artifacts_tx(tx, request, false)
}

fn integration_receipt(
    request: &FeatureConveyorArtifactIntegrationRequest,
    candidate: &CandidateEvidence,
    lifecycle_revision: u64,
) -> FeatureConveyorArtifactIntegrationReceipt {
    FeatureConveyorArtifactIntegrationReceipt {
        schema_version:
            assemblywright_protocol::FEATURE_CONVEYOR_ARTIFACT_INTEGRATION_SCHEMA_VERSION,
        integration_id: request.integration_id,
        feature_id: request.feature_id,
        specification_revision: request.specification_revision,
        lifecycle_revision,
        feature_lease_id: request.feature_lease_id,
        snapshot_id: request.snapshot_id,
        snapshot_sha256: request.snapshot_sha256,
        artifact_set_sha256: candidate.artifact_set_sha256,
        candidate_commit: candidate.candidate_commit.clone(),
        candidate_tree: candidate.candidate_tree.clone(),
        base_commit: candidate.base_commit.clone(),
        queue_revision: request.expected_queue_revision,
        emergency_pause_revision: request.expected_emergency_pause_revision,
        grants: request.grants,
        status: FeatureConveyorArtifactIntegrationStatus::CandidateFrozen,
    }
}

fn load_integration_receipt_tx(
    tx: &Transaction<'_>,
    integration_id: Uuid,
) -> Result<Option<FeatureConveyorArtifactIntegrationReceipt>, MasterError> {
    let row = tx
        .query_row(
            "SELECT feature_id,specification_revision,lifecycle_revision,feature_lease_id,
        snapshot_id,snapshot_sha256,artifact_set_sha256,candidate_commit,candidate_tree,base_commit,
        queue_revision,emergency_pause_revision,registration_grant_revision,
        cloud_disclosure_grant_revision,publication_grant_revision
        FROM feature_artifact_integrations WHERE integration_id=?1",
            [integration_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Vec<u8>>(5)?,
                    row.get::<_, Vec<u8>>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, i64>(11)?,
                    row.get::<_, i64>(12)?,
                    row.get::<_, i64>(13)?,
                    row.get::<_, i64>(14)?,
                ))
            },
        )
        .optional()?;
    row.map(|r| {
        Ok(FeatureConveyorArtifactIntegrationReceipt {
            schema_version:
                assemblywright_protocol::FEATURE_CONVEYOR_ARTIFACT_INTEGRATION_SCHEMA_VERSION,
            integration_id,
            feature_id: parse_uuid(&r.0)?,
            specification_revision: i64_to_u64(r.1)?,
            lifecycle_revision: i64_to_u64(r.2)?
                .checked_add(1)
                .ok_or(MasterError::ArtifactIntegrationUnavailable)?,
            feature_lease_id: parse_uuid(&r.3)?,
            snapshot_id: parse_uuid(&r.4)?,
            snapshot_sha256: digest_array(&r.5)?,
            artifact_set_sha256: digest_array(&r.6)?,
            candidate_commit: r.7,
            candidate_tree: r.8,
            base_commit: r.9,
            queue_revision: i64_to_u64(r.10)?,
            emergency_pause_revision: i64_to_u64(r.11)?,
            grants: assemblywright_protocol::FeatureConveyorGrantRevisions {
                registration: i64_to_u64(r.12)?,
                cloud_disclosure: i64_to_u64(r.13)?,
                autonomous_publication: i64_to_u64(r.14)?,
            },
            status: FeatureConveyorArtifactIntegrationStatus::CandidateFrozen,
        })
    })
    .transpose()
}

fn integration_artifact_ids_tx(
    tx: &Transaction<'_>,
    integration_id: Uuid,
) -> Result<Vec<Uuid>, MasterError> {
    let mut statement = tx.prepare(
        "SELECT artifact_id FROM feature_artifact_integration_artifacts
         WHERE integration_id=?1 ORDER BY artifact_id",
    )?;
    let ids = statement
        .query_map([integration_id.to_string()], |row| row.get::<_, String>(0))?
        .map(|value| parse_uuid(&value?))
        .collect();
    ids
}

fn validation_work_packet_scope_tx(
    tx: &Transaction<'_>,
    integration_id: Uuid,
) -> Result<(Vec<String>, u64), MasterError> {
    let mut statement = tx.prepare(
        "SELECT d.work_packet_metadata_json
         FROM feature_artifact_integration_artifacts ia
         JOIN feature_result_artifacts a ON a.artifact_id=ia.artifact_id
         JOIN feature_coding_dispatches d ON d.step_id=a.step_id
         WHERE ia.integration_id=?1 ORDER BY ia.ordinal,ia.packet_id",
    )?;
    let rows = statement.query_map([integration_id.to_string()], |row| row.get::<_, String>(0))?;
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    let mut acceptance_criteria_count = 0u64;
    for row in rows {
        let packet: FeatureConveyorCodingWorkPacketMetadata = serde_json::from_str(&row?)?;
        packet.validate()?;
        acceptance_criteria_count = acceptance_criteria_count
            .checked_add(u64::from(packet.acceptance_criteria_count))
            .ok_or(MasterError::ValidationGateUnavailable)?;
        for path in packet.allowed_paths {
            if !seen.insert(path.clone()) {
                return Err(MasterError::ValidationGateUnavailable);
            }
            paths.push(path);
        }
    }
    paths.sort();
    if paths.is_empty() || acceptance_criteria_count == 0 {
        return Err(MasterError::ValidationGateUnavailable);
    }
    Ok((paths, acceptance_criteria_count))
}

fn validation_requirements_sha256_tx(
    tx: &Transaction<'_>,
    feature_id: Uuid,
    specification_revision: u64,
) -> Result<[u8; 32], MasterError> {
    let digest: Vec<u8> = tx.query_row(
        "SELECT design_sha256 FROM feature_specification_revisions
         WHERE feature_id=?1 AND revision=?2",
        params![feature_id.to_string(), u64_to_i64(specification_revision)?],
        |row| row.get(0),
    )?;
    let digest = digest_array(&digest)?;
    if digest == [0; 32] {
        return Err(MasterError::ValidationGateUnavailable);
    }
    Ok(digest)
}

fn integration_receipt_matches_request(
    receipt: &FeatureConveyorArtifactIntegrationReceipt,
    request: &FeatureConveyorArtifactIntegrationRequest,
) -> bool {
    receipt.integration_id == request.integration_id
        && receipt.feature_id == request.feature_id
        && receipt.specification_revision == request.specification_revision
        && receipt.lifecycle_revision == request.expected_lifecycle_revision.saturating_add(1)
        && receipt.feature_lease_id == request.feature_lease_id
        && receipt.snapshot_id == request.snapshot_id
        && receipt.snapshot_sha256 == request.snapshot_sha256
        && receipt.base_commit == request.base_commit
        && receipt.queue_revision == request.expected_queue_revision
        && receipt.emergency_pause_revision == request.expected_emergency_pause_revision
        && receipt.grants == request.grants
}

fn project_visibility_str(visibility: ProjectVisibility) -> &'static str {
    match visibility {
        ProjectVisibility::Public => "public",
        ProjectVisibility::Private => "private",
    }
}

fn parse_project_visibility(value: &str) -> Result<ProjectVisibility, MasterError> {
    match value {
        "public" => Ok(ProjectVisibility::Public),
        "private" => Ok(ProjectVisibility::Private),
        _ => Err(MasterError::InvalidStoredState(
            "assembly-line project visibility is invalid".to_string(),
        )),
    }
}

fn brainstorming_target_str(target: BrainstormingTargetKind) -> &'static str {
    match target {
        BrainstormingTargetKind::Project => "project",
        BrainstormingTargetKind::Feature => "feature",
    }
}

fn parse_repository_creation_lifecycle(
    value: &str,
) -> Result<RepositoryCreationLifecycle, MasterError> {
    match value {
        "creation_pending" => Ok(RepositoryCreationLifecycle::CreationPending),
        "reconciling" => Ok(RepositoryCreationLifecycle::Reconciling),
        "created" => Ok(RepositoryCreationLifecycle::Created),
        "conflict" => Ok(RepositoryCreationLifecycle::Conflict),
        "reconciliation_required" => Ok(RepositoryCreationLifecycle::ReconciliationRequired),
        "failed" => Ok(RepositoryCreationLifecycle::Failed),
        _ => Err(MasterError::InvalidStoredState(
            "assembly-line repository lifecycle is invalid".to_string(),
        )),
    }
}

fn repository_projection_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<RepositoryCreationProjection> {
    let repository_id = row.get::<_, String>(0)?;
    let git_url = row.get::<_, String>(1)?;
    let visibility = row.get::<_, String>(4)?;
    let specification_id = row.get::<_, String>(5)?;
    let specification_sha256 = row.get::<_, Vec<u8>>(7)?;
    let owner_approval_sha256 = row.get::<_, Vec<u8>>(8)?;
    let lifecycle = row.get::<_, String>(9)?;
    let effect_possible = row.get::<_, i64>(10)?;
    let creation_evidence = row.get::<_, Option<Vec<u8>>>(11)?;
    let conversion_error = |message: &str| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            message.to_string().into(),
        )
    };
    Ok(RepositoryCreationProjection {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        repository: AssemblyLineRepositoryIdentity {
            repository_id: Uuid::parse_str(&repository_id)
                .map_err(|_| conversion_error("invalid repository UUID"))?,
            git_url: assemblywright_protocol::CanonicalGitHubRepositoryUrl::parse(&git_url)
                .map_err(|_| conversion_error("invalid canonical GitHub URL"))?,
        },
        repository_revision: u64::try_from(row.get::<_, i64>(2)?)
            .map_err(|_| conversion_error("invalid repository revision"))?,
        lifecycle_revision: u64::try_from(row.get::<_, i64>(3)?)
            .map_err(|_| conversion_error("invalid repository lifecycle revision"))?,
        visibility: parse_project_visibility(&visibility)
            .map_err(|_| conversion_error("invalid project visibility"))?,
        approved_specification_id: Uuid::parse_str(&specification_id)
            .map_err(|_| conversion_error("invalid specification UUID"))?,
        approved_specification_revision: u64::try_from(row.get::<_, i64>(6)?)
            .map_err(|_| conversion_error("invalid specification revision"))?,
        approved_specification_sha256: specification_sha256
            .try_into()
            .map_err(|_| conversion_error("invalid specification digest"))?,
        owner_approval_sha256: owner_approval_sha256
            .try_into()
            .map_err(|_| conversion_error("invalid approval digest"))?,
        lifecycle: parse_repository_creation_lifecycle(&lifecycle)
            .map_err(|_| conversion_error("invalid repository lifecycle"))?,
        effect_possible: match effect_possible {
            0 => false,
            1 => true,
            _ => return Err(conversion_error("invalid effect-possible bit")),
        },
        creation_evidence_sha256: creation_evidence
            .map(|value| {
                value
                    .try_into()
                    .map_err(|_| conversion_error("invalid creation digest"))
            })
            .transpose()?,
    })
}

fn queue_projection_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<FeatureQueueEntryProjection> {
    let conversion_error = |message: &str| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Text,
            message.to_string().into(),
        )
    };
    let lifecycle = row.get::<_, String>(8)?;
    let lifecycle = match lifecycle.as_str() {
        "queued" => FeatureQueueLifecycle::Queued,
        "starting" => FeatureQueueLifecycle::Starting,
        "active" => FeatureQueueLifecycle::Active,
        "stopping" => FeatureQueueLifecycle::Stopping,
        "paused_at_checkpoint" => FeatureQueueLifecycle::PausedAtCheckpoint,
        "emergency_paused" => FeatureQueueLifecycle::EmergencyPaused,
        "waiting_for_host_reconnect" => FeatureQueueLifecycle::WaitingForHostReconnect,
        "reconciliation_required" => FeatureQueueLifecycle::ReconciliationRequired,
        "incomplete_termination" => FeatureQueueLifecycle::IncompleteTermination,
        _ => return Err(conversion_error("invalid assembly-line queue lifecycle")),
    };
    Ok(FeatureQueueEntryProjection {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        feature_id: Uuid::parse_str(&row.get::<_, String>(0)?)
            .map_err(|_| conversion_error("invalid feature UUID"))?,
        repository_id: Uuid::parse_str(&row.get::<_, String>(1)?)
            .map_err(|_| conversion_error("invalid repository UUID"))?,
        specification_id: Uuid::parse_str(&row.get::<_, String>(2)?)
            .map_err(|_| conversion_error("invalid specification UUID"))?,
        specification_revision: u64::try_from(row.get::<_, i64>(3)?)
            .map_err(|_| conversion_error("invalid specification revision"))?,
        specification_sha256: row
            .get::<_, Vec<u8>>(4)?
            .try_into()
            .map_err(|_| conversion_error("invalid specification digest"))?,
        owner_approval_sha256: row
            .get::<_, Vec<u8>>(5)?
            .try_into()
            .map_err(|_| conversion_error("invalid approval digest"))?,
        position: u16::try_from(row.get::<_, i64>(6)?)
            .map_err(|_| conversion_error("invalid queue position"))?,
        lifecycle_revision: u64::try_from(row.get::<_, i64>(7)?)
            .map_err(|_| conversion_error("invalid lifecycle revision"))?,
        lifecycle,
    })
}

fn assembly_line_request_replay_tx(
    tx: &Transaction<'_>,
    request_kind: &str,
    record_id: Uuid,
    request_sha256: [u8; 32],
) -> Result<bool, MasterError> {
    let stored = tx
        .query_row(
            "SELECT request_sha256 FROM assembly_line_requests
             WHERE request_kind=?1 AND record_id=?2",
            params![request_kind, record_id.to_string()],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()?;
    match stored {
        None => Ok(false),
        Some(stored) if digest_array(&stored)? == request_sha256 => Ok(true),
        Some(_) => Err(MasterError::AssemblyLinePlanningImmutable),
    }
}

fn assembly_line_execution_request_replay_tx(
    tx: &Transaction<'_>,
    request_kind: &str,
    request_id: Uuid,
    request_sha256: [u8; 32],
) -> Result<Option<String>, MasterError> {
    let stored = tx
        .query_row(
            "SELECT request_sha256,response_json FROM assembly_line_execution_requests
             WHERE request_kind=?1 AND request_id=?2",
            params![request_kind, request_id.to_string()],
            |row| Ok((row.get::<_, Vec<u8>>(0)?, row.get::<_, String>(1)?)),
        )
        .optional()?;
    match stored {
        None => Ok(None),
        Some((stored, response)) if digest_array(&stored)? == request_sha256 => Ok(Some(response)),
        Some(_) => Err(MasterError::AssemblyLinePlanningImmutable),
    }
}

fn insert_assembly_line_execution_request_tx(
    tx: &Transaction<'_>,
    request_kind: &str,
    request_id: Uuid,
    request_sha256: [u8; 32],
    response_json: &str,
    now_ms: u64,
) -> Result<(), MasterError> {
    tx.execute(
        "INSERT INTO assembly_line_execution_requests
         (request_kind,request_id,request_sha256,response_json,recorded_at_ms)
         VALUES(?1,?2,?3,?4,?5)",
        params![
            request_kind,
            request_id.to_string(),
            request_sha256.as_slice(),
            response_json,
            u64_to_i64(now_ms)?,
        ],
    )?;
    Ok(())
}

fn claim_assembly_line_effect_dispatch_tx(
    tx: &Transaction<'_>,
    request_kind: &str,
    request_id: Uuid,
    intent_sha256: [u8; 32],
    now_ms: u64,
) -> Result<bool, MasterError> {
    let existing = tx
        .query_row(
            "SELECT intent_sha256 FROM assembly_line_effect_dispatches
             WHERE request_kind=?1 AND request_id=?2",
            params![request_kind, request_id.to_string()],
            |row| row.get::<_, Vec<u8>>(0),
        )
        .optional()?;
    match existing {
        Some(existing) if digest_array(&existing)? == intent_sha256 => Ok(false),
        Some(_) => Err(MasterError::AssemblyLineExecutionControlUnavailable),
        None => {
            tx.execute(
                "INSERT INTO assembly_line_effect_dispatches
                 (request_kind,request_id,intent_sha256,effect_possible,recorded_at_ms)
                 VALUES(?1,?2,?3,1,?4)",
                params![
                    request_kind,
                    request_id.to_string(),
                    intent_sha256.as_slice(),
                    u64_to_i64(now_ms)?,
                ],
            )?;
            Ok(true)
        }
    }
}

fn current_assembly_line_execution_capability_tx(
    tx: &Transaction<'_>,
) -> Result<AssemblyLineExecutionCapabilityBinding, MasterError> {
    let row = tx
        .query_row(
            "SELECT binding_revision,state_revision,emergency_pause_revision,
                    windows_executor_id,windows_executor_revision,windows_executor_sha256,
                    mac_executor_id,mac_executor_revision,mac_executor_sha256,
                    windows_broker_id,windows_broker_revision,windows_broker_sha256,
                    mac_broker_id,mac_broker_revision,mac_broker_sha256,
                    protected_control_plane_sha256,windows_receipt_signer_key_id,
                    windows_receipt_verifying_key,mac_receipt_signer_key_id,
                    mac_receipt_verifying_key,healthy,provisioning_evidence_sha256
             FROM assembly_line_execution_capabilities
             ORDER BY binding_revision DESC LIMIT 1",
            [],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, Vec<u8>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, Vec<u8>>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, Vec<u8>>(11)?,
                    row.get::<_, String>(12)?,
                    row.get::<_, i64>(13)?,
                    row.get::<_, Vec<u8>>(14)?,
                    row.get::<_, Vec<u8>>(15)?,
                    row.get::<_, String>(16)?,
                    row.get::<_, Vec<u8>>(17)?,
                    row.get::<_, String>(18)?,
                    row.get::<_, Vec<u8>>(19)?,
                    row.get::<_, i64>(20)?,
                    row.get::<_, Vec<u8>>(21)?,
                ))
            },
        )
        .optional()?
        .ok_or(MasterError::AssemblyLineExecutionCapabilityUnavailable)?;
    let healthy = match row.20 {
        0 => false,
        1 => true,
        _ => return Err(MasterError::AssemblyLineExecutionCapabilityUnavailable),
    };
    let binding = AssemblyLineExecutionCapabilityBinding {
        binding_revision: i64_to_u64(row.0)?,
        expected_state_revision: i64_to_u64(row.1)?,
        expected_emergency_pause_revision: i64_to_u64(row.2)?,
        windows_executor_id: parse_uuid(&row.3)?,
        windows_executor_revision: i64_to_u64(row.4)?,
        windows_executor_sha256: digest_array(&row.5)?,
        mac_executor_id: parse_uuid(&row.6)?,
        mac_executor_revision: i64_to_u64(row.7)?,
        mac_executor_sha256: digest_array(&row.8)?,
        windows_broker_id: parse_uuid(&row.9)?,
        windows_broker_revision: i64_to_u64(row.10)?,
        windows_broker_sha256: digest_array(&row.11)?,
        mac_broker_id: parse_uuid(&row.12)?,
        mac_broker_revision: i64_to_u64(row.13)?,
        mac_broker_sha256: digest_array(&row.14)?,
        protected_control_plane_sha256: digest_array(&row.15)?,
        windows_receipt_signer_key_id: row.16,
        windows_receipt_verifying_key: digest_array(&row.17)?,
        mac_receipt_signer_key_id: row.18,
        mac_receipt_verifying_key: digest_array(&row.19)?,
        healthy,
        provisioning_evidence_sha256: digest_array(&row.21)?,
    };
    binding.validate()?;
    Ok(binding)
}

fn insert_assembly_line_request_tx(
    tx: &Transaction<'_>,
    request_kind: &str,
    record_id: Uuid,
    request_sha256: [u8; 32],
    response_json: Option<&str>,
    now_ms: u64,
) -> Result<(), MasterError> {
    tx.execute(
        "INSERT INTO assembly_line_requests
         (request_kind,record_id,request_sha256,response_json,recorded_at_ms)
         VALUES(?1,?2,?3,?4,?5)",
        params![
            request_kind,
            record_id.to_string(),
            request_sha256.as_slice(),
            response_json,
            u64_to_i64(now_ms)?
        ],
    )?;
    Ok(())
}

fn assembly_line_owner_revision_tx(tx: &Transaction<'_>) -> Result<u64, MasterError> {
    let revision: i64 = tx.query_row(
        "SELECT owner_control_revision FROM assembly_line_state WHERE singleton=1",
        [],
        |row| row.get(0),
    )?;
    i64_to_u64(revision)
}

fn advance_assembly_line_owner_revision_tx(tx: &Transaction<'_>) -> Result<(), MasterError> {
    let prior_state = assembly_line_state_tx(tx)?;
    let prior_revision = assembly_line_owner_revision_tx(tx)?;
    let next_revision = prior_revision
        .checked_add(1)
        .ok_or(MasterError::IntegerOutOfRange)?;
    let changed = tx.execute(
        "UPDATE assembly_line_state SET owner_control_revision=?1
         WHERE singleton=1 AND owner_control_revision=?2",
        params![u64_to_i64(next_revision)?, u64_to_i64(prior_revision)?],
    )?;
    if changed != 1 {
        return Err(MasterError::InvalidStoredState(
            "assembly-line owner revision CAS affected an unexpected row count".to_string(),
        ));
    }
    if assembly_line_owner_revision_tx(tx)? != next_revision
        || assembly_line_state_tx(tx)? != prior_state
    {
        return Err(MasterError::InvalidStoredState(
            "assembly-line owner revision did not match its authoritative transition".to_string(),
        ));
    }
    Ok(())
}

fn require_created_assembly_line_repository_tx(
    tx: &Transaction<'_>,
    repository: &AssemblyLineRepositoryIdentity,
    expected_revision: u64,
) -> Result<(), MasterError> {
    let stored = tx
        .query_row(
            "SELECT git_url,repository_revision,lifecycle,effect_possible,creation_evidence_sha256
             FROM assembly_line_repositories
             WHERE repository_id=?1",
            [repository.repository_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<Vec<u8>>>(4)?,
                ))
            },
        )
        .optional()?;
    match stored {
        Some((url, revision, lifecycle, 1, Some(evidence)))
            if url == repository.git_url.url
                && i64_to_u64(revision)? == expected_revision
                && lifecycle == "created"
                && digest_array(&evidence)? != [0; 32] =>
        {
            Ok(())
        }
        _ => Err(MasterError::AssemblyLineRepositoryUnavailable),
    }
}

fn assembly_line_repository_projection_tx(
    tx: &Transaction<'_>,
    repository_id: Uuid,
) -> Result<RepositoryCreationProjection, MasterError> {
    tx.query_row(
        "SELECT repository_id,git_url,repository_revision,lifecycle_revision,visibility,
                approved_specification_id,approved_specification_revision,
                approved_specification_sha256,owner_approval_sha256,lifecycle,
                effect_possible,creation_evidence_sha256
         FROM assembly_line_repositories WHERE repository_id=?1",
        [repository_id.to_string()],
        repository_projection_row,
    )
    .optional()?
    .ok_or(MasterError::AssemblyLineRepositoryUnavailable)
}

fn assembly_line_queue_projection_tx(
    tx: &Transaction<'_>,
    feature_id: Uuid,
) -> Result<FeatureQueueEntryProjection, MasterError> {
    tx.query_row(
        "SELECT feature_id,repository_id,specification_id,specification_revision,
                specification_sha256,owner_approval_sha256,queue_position,
                lifecycle_revision,lifecycle
         FROM assembly_line_queue WHERE feature_id=?1",
        [feature_id.to_string()],
        queue_projection_row,
    )
    .optional()?
    .ok_or(MasterError::AssemblyLinePlanningImmutable)
}

fn assembly_line_state_tx(tx: &Transaction<'_>) -> Result<AssemblyLineState, MasterError> {
    assembly_line_state_connection(tx)
}

fn assembly_line_state_connection(
    connection: &Connection,
) -> Result<AssemblyLineState, MasterError> {
    let (
        state_revision,
        queue_revision,
        auto_run,
        lifecycle,
        session_id,
        child_epoch_id,
        feature_id,
    ): (
        i64,
        i64,
        i64,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    ) = connection.query_row(
        "SELECT state_revision,queue_revision,auto_run,lifecycle,session_id,
                    active_child_epoch_id,active_feature_id
             FROM assembly_line_state WHERE singleton=1",
        [],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
            ))
        },
    )?;
    let count: i64 =
        connection.query_row("SELECT COUNT(*) FROM assembly_line_queue", [], |row| {
            row.get(0)
        })?;
    if !matches!(auto_run, 0 | 1) {
        return Err(MasterError::InvalidStoredState(
            "assembly-line auto-run state is malformed".to_string(),
        ));
    }
    let state = AssemblyLineState {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        state_revision: i64_to_u64(state_revision)?,
        queue_revision: i64_to_u64(queue_revision)?,
        queue_count: u16::try_from(count).map_err(|_| MasterError::IntegerOutOfRange)?,
        auto_run: auto_run == 1,
        lifecycle: assembly_line_lifecycle_from_str(&lifecycle)?,
        session_id: session_id.as_deref().map(parse_uuid).transpose()?,
        active_child_epoch_id: child_epoch_id.as_deref().map(parse_uuid).transpose()?,
        active_feature_id: feature_id.as_deref().map(parse_uuid).transpose()?,
    };
    state.validate()?;
    Ok(state)
}

fn assembly_line_lifecycle_from_str(
    lifecycle: &str,
) -> Result<AssemblyLineLifecycleState, MasterError> {
    match lifecycle {
        "stopped" => Ok(AssemblyLineLifecycleState::Stopped),
        "starting" => Ok(AssemblyLineLifecycleState::Starting),
        "running" => Ok(AssemblyLineLifecycleState::Running),
        "stopping" => Ok(AssemblyLineLifecycleState::Stopping),
        "paused_at_checkpoint" => Ok(AssemblyLineLifecycleState::PausedAtCheckpoint),
        "emergency_paused" => Ok(AssemblyLineLifecycleState::EmergencyPaused),
        "waiting_for_host_reconnect" => Ok(AssemblyLineLifecycleState::WaitingForHostReconnect),
        "reconciliation_required" => Ok(AssemblyLineLifecycleState::ReconciliationRequired),
        "incomplete_termination" => Ok(AssemblyLineLifecycleState::IncompleteTermination),
        "waiting_for_owner_start" => Ok(AssemblyLineLifecycleState::WaitingForOwnerStart),
        _ => Err(MasterError::InvalidStoredState(
            "assembly-line lifecycle is malformed".to_string(),
        )),
    }
}

fn valid_execution_signer_key_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value.is_ascii()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn verify_execution_checkpoint_receipt_signature(
    receipt: &ExecutionCheckpointReceipt,
    verifying_key: [u8; 32],
) -> Result<(), MasterError> {
    let verifying_key = verifying_key
        .as_slice()
        .try_into()
        .map_err(|_| MasterError::AssemblyLineExecutionReceiptMismatch)?;
    receipt
        .verify_signature(&verifying_key)
        .map_err(|_| MasterError::AssemblyLineExecutionReceiptMismatch)
}

fn verify_execution_activation_receipt_signature(
    receipt: &ExecutionActivationReceipt,
    verifying_key: [u8; 32],
) -> Result<(), MasterError> {
    let verifying_key = verifying_key
        .as_slice()
        .try_into()
        .map_err(|_| MasterError::AssemblyLineExecutionReceiptMismatch)?;
    receipt
        .verify_signature(&verifying_key)
        .map_err(|_| MasterError::AssemblyLineExecutionReceiptMismatch)
}

fn verify_execution_termination_receipt_signature(
    receipt: &ExecutionTerminationReceipt,
    verifying_key: [u8; 32],
) -> Result<(), MasterError> {
    let verifying_key = verifying_key
        .as_slice()
        .try_into()
        .map_err(|_| MasterError::AssemblyLineExecutionReceiptMismatch)?;
    receipt
        .verify_signature(&verifying_key)
        .map_err(|_| MasterError::AssemblyLineExecutionReceiptMismatch)
}

fn append_assembly_line_audit_tx(
    tx: &Transaction<'_>,
    event_kind: &str,
    occurred_at_ms: u64,
    redacted_metadata: Value,
) -> Result<(), MasterError> {
    if event_kind.is_empty() || event_kind.len() > 96 || !redacted_metadata.is_object() {
        return Err(MasterError::InvalidAssemblyLinePlanningInput(
            "assembly-line audit metadata is invalid".to_string(),
        ));
    }
    let metadata_json = canonical_json(&redacted_metadata)?;
    if metadata_json.len() > 4096 {
        return Err(MasterError::InvalidAssemblyLinePlanningInput(
            "assembly-line audit metadata exceeds the redacted bound".to_string(),
        ));
    }
    tx.execute(
        "INSERT INTO assembly_line_audit(event_kind,occurred_at_ms,redacted_metadata_json)
         VALUES(?1,?2,?3)",
        params![event_kind, u64_to_i64(occurred_at_ms)?, metadata_json],
    )?;
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredValidationGateManifest {
    schema_version: u16,
    command_ids: Vec<FeatureConveyorValidationCommandId>,
}

fn validation_commands_from_manifest(
    canonical_manifest_json: &str,
) -> Result<Vec<FeatureConveyorValidationCommandId>, MasterError> {
    let manifest: Value = serde_json::from_str(canonical_manifest_json)?;
    let gate = manifest
        .get("validation_gate")
        .cloned()
        .ok_or(MasterError::ValidationGateUnavailable)?;
    let gate: StoredValidationGateManifest =
        serde_json::from_value(gate).map_err(|_| MasterError::ValidationGateUnavailable)?;
    if gate.schema_version != FEATURE_CONVEYOR_VALIDATION_GATE_SCHEMA_VERSION
        || gate.command_ids != FeatureConveyorValidationCommandId::REQUIRED
    {
        return Err(MasterError::ValidationGateUnavailable);
    }
    Ok(gate.command_ids)
}

fn validate_validation_gate_binding_tx(
    tx: &Transaction<'_>,
    request: &FeatureConveyorValidationGateRequest,
    now_ms: u64,
) -> Result<CandidateEvidence, MasterError> {
    validate_validation_gate_binding_state_tx(tx, request, None, now_ms)
}

fn validate_completed_validation_gate_binding_tx(
    tx: &Transaction<'_>,
    request: &FeatureConveyorValidationGateRequest,
    completed_lifecycle_revision: u64,
    now_ms: u64,
) -> Result<CandidateEvidence, MasterError> {
    if completed_lifecycle_revision
        != request
            .expected_lifecycle_revision
            .checked_add(1)
            .ok_or(MasterError::ValidationGateUnavailable)?
    {
        return Err(MasterError::ValidationGateUnavailable);
    }
    validate_validation_gate_binding_state_tx(
        tx,
        request,
        Some(completed_lifecycle_revision),
        now_ms,
    )
}

fn validate_validation_gate_binding_state_tx(
    tx: &Transaction<'_>,
    request: &FeatureConveyorValidationGateRequest,
    completed_lifecycle_revision: Option<u64>,
    now_ms: u64,
) -> Result<CandidateEvidence, MasterError> {
    if emergency_paused_tx(tx)? {
        return Err(MasterError::EmergencyPaused);
    }
    let pause_revision = emergency_pause_revision_tx(tx)?;
    if pause_revision != request.expected_emergency_pause_revision {
        return Err(MasterError::StaleEmergencyPauseRevision {
            expected: request.expected_emergency_pause_revision,
            found: pause_revision,
        });
    }
    let queue_revision: i64 = tx.query_row(
        "SELECT queue_revision FROM feature_conveyor_state WHERE singleton=1",
        [],
        |row| row.get(0),
    )?;
    let queue_revision = i64_to_u64(queue_revision)?;
    if queue_revision != request.expected_queue_revision {
        return Err(MasterError::StaleFeatureQueueRevision {
            expected: request.expected_queue_revision,
            found: queue_revision,
        });
    }
    require_active_lease_tx(tx, request.feature_id)?;
    require_current_feature_grants_tx(tx, request.feature_id, now_ms)?;
    let row = tx
        .query_row(
            "SELECT f.current_specification_revision,f.lifecycle_revision,f.status,
                    l.lease_id,l.snapshot_id,c.snapshot_sha256,c.base_commit,
                    s.canonical_manifest_json,s.registration_grant_revision,
                    s.cloud_disclosure_grant_revision,s.publication_grant_revision,
                    i.feature_id,i.specification_revision,i.lifecycle_revision,
                    i.feature_lease_id,i.snapshot_id,i.snapshot_sha256,
                    i.artifact_set_sha256,i.candidate_commit,i.candidate_tree,i.base_commit,
                    i.queue_revision,i.emergency_pause_revision,
                    i.registration_grant_revision,i.cloud_disclosure_grant_revision,
                    i.publication_grant_revision
             FROM feature_conveyor_features f
             JOIN feature_active_lease l ON l.feature_id=f.feature_id AND l.singleton=1
             JOIN feature_repository_snapshot_claims c ON c.snapshot_id=l.snapshot_id
             JOIN feature_specification_revisions s
               ON s.feature_id=f.feature_id AND s.revision=f.current_specification_revision
             JOIN feature_artifact_integrations i ON i.integration_id=?2
             WHERE f.feature_id=?1",
            params![
                request.feature_id.to_string(),
                request.integration_id.to_string()
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, Vec<u8>>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, String>(11)?,
                    row.get::<_, i64>(12)?,
                    row.get::<_, i64>(13)?,
                    row.get::<_, String>(14)?,
                    row.get::<_, String>(15)?,
                    row.get::<_, Vec<u8>>(16)?,
                    row.get::<_, Vec<u8>>(17)?,
                    row.get::<_, String>(18)?,
                    row.get::<_, String>(19)?,
                    row.get::<_, String>(20)?,
                    row.get::<_, i64>(21)?,
                    row.get::<_, i64>(22)?,
                    row.get::<_, i64>(23)?,
                    row.get::<_, i64>(24)?,
                    row.get::<_, i64>(25)?,
                ))
            },
        )
        .optional()?
        .ok_or(MasterError::ValidationGateUnavailable)?;
    let integration_lifecycle = i64_to_u64(row.13)?;
    let expected_feature_lifecycle =
        completed_lifecycle_revision.unwrap_or(request.expected_lifecycle_revision);
    let expected_status = if completed_lifecycle_revision.is_some() {
        FeatureLifecycleStatus::Reviewing
    } else {
        FeatureLifecycleStatus::Validating
    };
    if i64_to_u64(row.0)? != request.specification_revision
        || i64_to_u64(row.1)? != expected_feature_lifecycle
        || row.2 != expected_status.as_str()
        || parse_uuid(&row.3)? != request.feature_lease_id
        || parse_uuid(&row.4)? != request.snapshot_id
        || digest_array(&row.5)? != request.snapshot_sha256
        || row.6 != request.base_commit
        || i64_to_u64(row.8)? != request.grants.registration
        || i64_to_u64(row.9)? != request.grants.cloud_disclosure
        || i64_to_u64(row.10)? != request.grants.autonomous_publication
        || parse_uuid(&row.11)? != request.feature_id
        || i64_to_u64(row.12)? != request.specification_revision
        || integration_lifecycle.checked_add(1) != Some(request.expected_lifecycle_revision)
        || parse_uuid(&row.14)? != request.feature_lease_id
        || parse_uuid(&row.15)? != request.snapshot_id
        || digest_array(&row.16)? != request.snapshot_sha256
        || digest_array(&row.17)? != request.artifact_set_sha256
        || row.18 != request.candidate_commit
        || row.19 != request.candidate_tree
        || row.20 != request.base_commit
        || i64_to_u64(row.21)? != request.expected_queue_revision
        || i64_to_u64(row.22)? != request.expected_emergency_pause_revision
        || i64_to_u64(row.23)? != request.grants.registration
        || i64_to_u64(row.24)? != request.grants.cloud_disclosure
        || i64_to_u64(row.25)? != request.grants.autonomous_publication
    {
        return Err(MasterError::ValidationGateUnavailable);
    }
    let commands = validation_commands_from_manifest(&row.7)?;
    if commands != request.command_ids
        || assemblywright_protocol::feature_conveyor_validation_plan_sha256(&commands)?
            != request.plan_sha256
    {
        return Err(MasterError::ValidationGateUnavailable);
    }
    let artifact_ids = integration_artifact_ids_tx(tx, request.integration_id)?;
    Ok(CandidateEvidence {
        integration_id: request.integration_id,
        artifact_set_sha256: request.artifact_set_sha256,
        candidate_commit: request.candidate_commit.clone(),
        candidate_tree: request.candidate_tree.clone(),
        base_commit: request.base_commit.clone(),
        artifact_ids,
    })
}

fn validate_validation_gate_evidence(
    request: &FeatureConveyorValidationGateRequest,
    evidence: &ValidationGateEvidence,
) -> Result<(), MasterError> {
    if evidence.commands.len() != request.command_ids.len() {
        return Err(MasterError::ValidationGateUnavailable);
    }
    for (expected, command) in request.command_ids.iter().zip(&evidence.commands) {
        if command.command_id != *expected
            || command.result_sha256 == [0; 32]
            || (command.passed && command.output_truncated)
        {
            return Err(MasterError::ValidationGateUnavailable);
        }
    }
    Ok(())
}

fn review_gateway_plan_tx(
    tx: &Transaction<'_>,
    request: &FeatureConveyorReviewGatewayRequest,
    now_ms: u64,
) -> Result<ReviewGatewayExecutionPlan, MasterError> {
    if emergency_paused_tx(tx)? {
        return Err(MasterError::EmergencyPaused);
    }
    let pause_revision = emergency_pause_revision_tx(tx)?;
    if pause_revision != request.expected_emergency_pause_revision {
        return Err(MasterError::StaleEmergencyPauseRevision {
            expected: request.expected_emergency_pause_revision,
            found: pause_revision,
        });
    }
    let queue_revision = i64_to_u64(tx.query_row(
        "SELECT queue_revision FROM feature_conveyor_state WHERE singleton=1",
        [],
        |row| row.get::<_, i64>(0),
    )?)?;
    if queue_revision != request.expected_queue_revision {
        return Err(MasterError::StaleFeatureQueueRevision {
            expected: request.expected_queue_revision,
            found: queue_revision,
        });
    }
    require_active_lease_tx(tx, request.feature_id)?;
    require_current_feature_grants_tx(tx, request.feature_id, now_ms)?;
    let feature = tx
        .query_row(
            "SELECT current_specification_revision,lifecycle_revision,status
             FROM feature_conveyor_features WHERE feature_id=?1",
            [request.feature_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?
        .ok_or(MasterError::ReviewGatewayUnavailable)?;
    if i64_to_u64(feature.0)? != request.specification_revision
        || i64_to_u64(feature.1)? != request.expected_lifecycle_revision
        || feature.2 != "reviewing"
    {
        return Err(MasterError::ReviewGatewayUnavailable);
    }
    let lease_id = tx.query_row(
        "SELECT lease_id FROM feature_active_lease WHERE singleton=1 AND feature_id=?1",
        [request.feature_id.to_string()],
        |row| row.get::<_, String>(0),
    )?;
    if parse_uuid(&lease_id)? != request.feature_lease_id {
        return Err(MasterError::ReviewGatewayUnavailable);
    }
    let specification = tx.query_row(
        "SELECT canonical_manifest_json,manifest_sha256,design_sha256,provider_id,model_id,
                registration_grant_revision,cloud_disclosure_grant_revision,
                publication_grant_revision
         FROM feature_specification_revisions WHERE feature_id=?1 AND revision=?2",
        params![
            request.feature_id.to_string(),
            u64_to_i64(request.specification_revision)?
        ],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, i64>(7)?,
            ))
        },
    )?;
    if specification.3 != request.provider_id
        || specification.4 != request.model_id
        || i64_to_u64(specification.5)? != request.grants.registration
        || i64_to_u64(specification.6)? != request.grants.cloud_disclosure
        || i64_to_u64(specification.7)? != request.grants.autonomous_publication
    {
        return Err(MasterError::ReviewGatewayUnavailable);
    }
    let integration = tx
        .query_row(
            "SELECT feature_id,specification_revision,lifecycle_revision,feature_lease_id,
                    artifact_set_sha256,candidate_commit,candidate_tree,base_commit,
                    queue_revision,emergency_pause_revision,registration_grant_revision,
                    cloud_disclosure_grant_revision,publication_grant_revision
             FROM feature_artifact_integrations WHERE integration_id=?1",
            [request.integration_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Vec<u8>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, i64>(11)?,
                    row.get::<_, i64>(12)?,
                ))
            },
        )
        .optional()?
        .ok_or(MasterError::ReviewGatewayUnavailable)?;
    if parse_uuid(&integration.0)? != request.feature_id
        || i64_to_u64(integration.1)? != request.specification_revision
        || i64_to_u64(integration.2)?.checked_add(2) != Some(request.expected_lifecycle_revision)
        || parse_uuid(&integration.3)? != request.feature_lease_id
        || integration.5 != request.candidate_commit
        || integration.6 != request.candidate_tree
        || integration.7 != request.base_commit
        || i64_to_u64(integration.8)? != request.expected_queue_revision
        || i64_to_u64(integration.9)? != request.expected_emergency_pause_revision
        || i64_to_u64(integration.10)? != request.grants.registration
        || i64_to_u64(integration.11)? != request.grants.cloud_disclosure
        || i64_to_u64(integration.12)? != request.grants.autonomous_publication
    {
        return Err(MasterError::ReviewGatewayUnavailable);
    }
    let validation = tx
        .query_row(
            "SELECT a.feature_id,a.specification_revision,a.lifecycle_revision,
                    a.feature_lease_id,a.integration_id,a.candidate_commit,a.candidate_tree,
                    a.base_commit,a.queue_revision,a.emergency_pause_revision,
                    a.registration_grant_revision,a.cloud_disclosure_grant_revision,
                    a.publication_grant_revision,a.command_ids_json,c.passed,
                    c.evidence_manifest_sha256,c.lifecycle_revision
             FROM feature_validation_attempts a
             JOIN feature_validation_completions c ON c.validation_id=a.validation_id
             WHERE a.validation_id=?1",
            [request.validation_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, i64>(8)?,
                    row.get::<_, i64>(9)?,
                    row.get::<_, i64>(10)?,
                    row.get::<_, i64>(11)?,
                    row.get::<_, i64>(12)?,
                    row.get::<_, String>(13)?,
                    row.get::<_, i64>(14)?,
                    row.get::<_, Vec<u8>>(15)?,
                    row.get::<_, i64>(16)?,
                ))
            },
        )
        .optional()?
        .ok_or(MasterError::ReviewGatewayUnavailable)?;
    if parse_uuid(&validation.0)? != request.feature_id
        || i64_to_u64(validation.1)? != request.specification_revision
        || i64_to_u64(validation.2)?.checked_add(1) != Some(request.expected_lifecycle_revision)
        || parse_uuid(&validation.3)? != request.feature_lease_id
        || parse_uuid(&validation.4)? != request.integration_id
        || validation.5 != request.candidate_commit
        || validation.6 != request.candidate_tree
        || validation.7 != request.base_commit
        || i64_to_u64(validation.8)? != request.expected_queue_revision
        || i64_to_u64(validation.9)? != request.expected_emergency_pause_revision
        || i64_to_u64(validation.10)? != request.grants.registration
        || i64_to_u64(validation.11)? != request.grants.cloud_disclosure
        || i64_to_u64(validation.12)? != request.grants.autonomous_publication
        || !parse_stored_boolean(validation.14, "review validation pass")?
        || digest_array(&validation.15)? != request.evidence_manifest_sha256
        || i64_to_u64(validation.16)? != request.expected_lifecycle_revision
    {
        return Err(MasterError::ReviewGatewayUnavailable);
    }
    let command_ids: Vec<FeatureConveyorValidationCommandId> =
        serde_json::from_str(&validation.13)?;
    if command_ids != FeatureConveyorValidationCommandId::REQUIRED {
        return Err(MasterError::ReviewGatewayUnavailable);
    }
    let mut evidence_digests = Vec::with_capacity(command_ids.len() + 1);
    evidence_digests.push(request.evidence_manifest_sha256);
    for command_id in command_ids {
        let digest: Vec<u8> = tx.query_row(
            "SELECT result_sha256 FROM feature_validation_command_evidence
             WHERE validation_id=?1 AND command_id=?2 AND passed=1",
            params![
                request.validation_id.to_string(),
                serde_json::to_string(&command_id)?
            ],
            |row| row.get(0),
        )?;
        evidence_digests.push(digest_array(&digest)?);
    }
    let approved_specification: Value = serde_json::from_str(&specification.0)?;
    validate_review_safe_manifest_schema(&approved_specification)
        .map_err(|_| MasterError::ReviewGatewayUnavailable)?;
    validate_review_disclosure_value(&approved_specification)?;
    let requirement_ids = approved_requirement_ids(&approved_specification)
        .map_err(|_| MasterError::ReviewGatewayUnavailable)?;
    let (_, work_packet_acceptance_count) =
        validation_work_packet_scope_tx(tx, request.integration_id)?;
    if usize::try_from(work_packet_acceptance_count).ok() != Some(requirement_ids.len()) {
        return Err(MasterError::ReviewGatewayUnavailable);
    }
    let existing_call = tx
        .query_row(
            "SELECT candidate_attempt,feature_call FROM feature_review_calls
             WHERE review_call_id=?1",
            [request.review_call_id.to_string()],
            |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?)),
        )
        .optional()?;
    let (candidate_attempt, feature_call) = if let Some((candidate_attempt, feature_call)) =
        existing_call
    {
        (
            u8::try_from(candidate_attempt).map_err(|_| MasterError::ReviewGatewayUnavailable)?,
            u8::try_from(feature_call).map_err(|_| MasterError::ReviewGatewayUnavailable)?,
        )
    } else {
        let decided: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM feature_review_decisions
             WHERE feature_id=?1 AND candidate_commit=?2)",
            params![request.feature_id.to_string(), request.candidate_commit],
            |row| row.get(0),
        )?;
        if decided {
            return Err(MasterError::ReviewGatewayUnavailable);
        }
        let candidate_calls = i64_to_u64(tx.query_row(
            "SELECT COUNT(*) FROM feature_review_calls
             WHERE feature_id=?1 AND candidate_commit=?2",
            params![request.feature_id.to_string(), request.candidate_commit],
            |row| row.get::<_, i64>(0),
        )?)?;
        let feature_calls = i64_to_u64(tx.query_row(
            "SELECT COUNT(*) FROM feature_review_calls WHERE feature_id=?1",
            [request.feature_id.to_string()],
            |row| row.get::<_, i64>(0),
        )?)?;
        if candidate_calls
            >= u64::from(MAX_FEATURE_CONVEYOR_REVIEW_TRANSPORT_ATTEMPTS_PER_CANDIDATE)
            || feature_calls >= u64::from(MAX_FEATURE_CONVEYOR_REVIEW_CALLS_PER_FEATURE)
        {
            return Err(MasterError::ReviewBudgetExhausted);
        }
        let next_retry = tx.query_row(
            "SELECT MAX(o.next_retry_at_ms) FROM feature_review_call_outcomes o
             JOIN feature_review_calls c ON c.review_call_id=o.review_call_id
             WHERE c.feature_id=?1 AND c.candidate_commit=?2 AND o.outcome_kind<>'decision'",
            params![request.feature_id.to_string(), request.candidate_commit],
            |row| row.get::<_, Option<i64>>(0),
        )?;
        if let Some(next_retry) = next_retry {
            let next_retry_at_ms = i64_to_u64(next_retry)?;
            if now_ms < next_retry_at_ms {
                return Err(MasterError::ReviewRetryNotReady { next_retry_at_ms });
            }
        }
        (
            u8::try_from(candidate_calls + 1).map_err(|_| MasterError::ReviewGatewayUnavailable)?,
            u8::try_from(feature_calls + 1).map_err(|_| MasterError::ReviewGatewayUnavailable)?,
        )
    };
    let candidate = CandidateEvidence {
        integration_id: request.integration_id,
        artifact_set_sha256: digest_array(&integration.4)?,
        candidate_commit: request.candidate_commit.clone(),
        candidate_tree: request.candidate_tree.clone(),
        base_commit: request.base_commit.clone(),
        artifact_ids: integration_artifact_ids_tx(tx, request.integration_id)?,
    };
    Ok(ReviewGatewayExecutionPlan {
        request: request.clone(),
        candidate,
        approved_specification,
        approved_specification_sha256: digest_array(&specification.1)?,
        requirements_sha256: digest_array(&specification.2)?,
        requirement_ids,
        evidence_digests,
        candidate_attempt,
        feature_call,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredOrchestrationState {
    orchestration_revision: u64,
    checkpoint_id: Uuid,
    stage: FeatureConveyorOrchestrationStage,
    resume_stage: Option<FeatureConveyorOrchestrationStage>,
    pause_kind: Option<FeatureConveyorOrchestrationPauseKind>,
    replacement_candidates_used: u8,
    active_processing_ms: u64,
    clock_started_at_ms: Option<u64>,
    next_retry_at_ms: Option<u64>,
    effect_possible: bool,
}

fn orchestration_stage_str(stage: FeatureConveyorOrchestrationStage) -> &'static str {
    match stage {
        FeatureConveyorOrchestrationStage::Implementing => "implementing",
        FeatureConveyorOrchestrationStage::Validating => "validating",
        FeatureConveyorOrchestrationStage::Reviewing => "reviewing",
        FeatureConveyorOrchestrationStage::Publishing => "publishing",
        FeatureConveyorOrchestrationStage::VerifyingMain => "verifying_main",
        FeatureConveyorOrchestrationStage::Repairing => "repairing",
        FeatureConveyorOrchestrationStage::Paused => "paused",
        FeatureConveyorOrchestrationStage::AttentionRequired => "attention_required",
        FeatureConveyorOrchestrationStage::Failed => "failed",
        FeatureConveyorOrchestrationStage::Succeeded => "succeeded",
        FeatureConveyorOrchestrationStage::Quarantined => "quarantined",
    }
}

fn parse_orchestration_stage(
    value: &str,
) -> Result<FeatureConveyorOrchestrationStage, MasterError> {
    match value {
        "implementing" => Ok(FeatureConveyorOrchestrationStage::Implementing),
        "validating" => Ok(FeatureConveyorOrchestrationStage::Validating),
        "reviewing" => Ok(FeatureConveyorOrchestrationStage::Reviewing),
        "publishing" => Ok(FeatureConveyorOrchestrationStage::Publishing),
        "verifying_main" => Ok(FeatureConveyorOrchestrationStage::VerifyingMain),
        "repairing" => Ok(FeatureConveyorOrchestrationStage::Repairing),
        "paused" => Ok(FeatureConveyorOrchestrationStage::Paused),
        "attention_required" => Ok(FeatureConveyorOrchestrationStage::AttentionRequired),
        "failed" => Ok(FeatureConveyorOrchestrationStage::Failed),
        "succeeded" => Ok(FeatureConveyorOrchestrationStage::Succeeded),
        "quarantined" => Ok(FeatureConveyorOrchestrationStage::Quarantined),
        _ => Err(MasterError::InvalidStoredState(
            "unknown orchestration stage".to_string(),
        )),
    }
}

fn orchestration_action_str(action: FeatureConveyorOrchestrationAction) -> &'static str {
    match action {
        FeatureConveyorOrchestrationAction::Inactive => "inactive",
        FeatureConveyorOrchestrationAction::AwaitImplementationEvidence => {
            "await_implementation_evidence"
        }
        FeatureConveyorOrchestrationAction::AwaitValidationEvidence => "await_validation_evidence",
        FeatureConveyorOrchestrationAction::AwaitReviewDecision => "await_review_decision",
        FeatureConveyorOrchestrationAction::RetryReviewTransport => "retry_review_transport",
        FeatureConveyorOrchestrationAction::AwaitPublicationEvidence => {
            "await_publication_evidence"
        }
        FeatureConveyorOrchestrationAction::AwaitMainVerification => "await_main_verification",
        FeatureConveyorOrchestrationAction::ReplacementCandidateRequired => {
            "replacement_candidate_required"
        }
        FeatureConveyorOrchestrationAction::OwnerAttentionRequired => "owner_attention_required",
        FeatureConveyorOrchestrationAction::ReconcileQuarantine => "reconcile_quarantine",
        FeatureConveyorOrchestrationAction::Terminal => "terminal",
    }
}

fn parse_orchestration_action(
    value: &str,
) -> Result<FeatureConveyorOrchestrationAction, MasterError> {
    match value {
        "inactive" => Ok(FeatureConveyorOrchestrationAction::Inactive),
        "await_implementation_evidence" => {
            Ok(FeatureConveyorOrchestrationAction::AwaitImplementationEvidence)
        }
        "await_validation_evidence" => {
            Ok(FeatureConveyorOrchestrationAction::AwaitValidationEvidence)
        }
        "await_review_decision" => Ok(FeatureConveyorOrchestrationAction::AwaitReviewDecision),
        "retry_review_transport" => Ok(FeatureConveyorOrchestrationAction::RetryReviewTransport),
        "await_publication_evidence" => {
            Ok(FeatureConveyorOrchestrationAction::AwaitPublicationEvidence)
        }
        "await_main_verification" => Ok(FeatureConveyorOrchestrationAction::AwaitMainVerification),
        "replacement_candidate_required" => {
            Ok(FeatureConveyorOrchestrationAction::ReplacementCandidateRequired)
        }
        "owner_attention_required" => {
            Ok(FeatureConveyorOrchestrationAction::OwnerAttentionRequired)
        }
        "reconcile_quarantine" => Ok(FeatureConveyorOrchestrationAction::ReconcileQuarantine),
        "terminal" => Ok(FeatureConveyorOrchestrationAction::Terminal),
        _ => Err(MasterError::InvalidStoredState(
            "unknown orchestration action".to_string(),
        )),
    }
}

fn orchestration_reason_str(reason: FeatureConveyorOrchestrationReason) -> &'static str {
    match reason {
        FeatureConveyorOrchestrationReason::CapabilityInactive => "capability_inactive",
        FeatureConveyorOrchestrationReason::CheckpointEffectFree => "checkpoint_effect_free",
        FeatureConveyorOrchestrationReason::ExistingEffectAmbiguous => "existing_effect_ambiguous",
        FeatureConveyorOrchestrationReason::ValidationFailed => "validation_failed",
        FeatureConveyorOrchestrationReason::ReviewRejected => "review_rejected",
        FeatureConveyorOrchestrationReason::ReviewTransportBackoff => "review_transport_backoff",
        FeatureConveyorOrchestrationReason::ReviewBudgetExhausted => "review_budget_exhausted",
        FeatureConveyorOrchestrationReason::PublicationFailed => "publication_failed",
        FeatureConveyorOrchestrationReason::ReplacementCandidateContractUnavailable => {
            "replacement_candidate_contract_unavailable"
        }
        FeatureConveyorOrchestrationReason::RepairBudgetExhausted => "repair_budget_exhausted",
        FeatureConveyorOrchestrationReason::ActiveProcessingBudgetExhausted => {
            "active_processing_budget_exhausted"
        }
        FeatureConveyorOrchestrationReason::Cancelled => "cancelled",
        FeatureConveyorOrchestrationReason::Failed => "failed",
        FeatureConveyorOrchestrationReason::Succeeded => "succeeded",
    }
}

fn parse_orchestration_reason(
    value: &str,
) -> Result<FeatureConveyorOrchestrationReason, MasterError> {
    match value {
        "capability_inactive" => Ok(FeatureConveyorOrchestrationReason::CapabilityInactive),
        "checkpoint_effect_free" => Ok(FeatureConveyorOrchestrationReason::CheckpointEffectFree),
        "existing_effect_ambiguous" => {
            Ok(FeatureConveyorOrchestrationReason::ExistingEffectAmbiguous)
        }
        "validation_failed" => Ok(FeatureConveyorOrchestrationReason::ValidationFailed),
        "review_rejected" => Ok(FeatureConveyorOrchestrationReason::ReviewRejected),
        "review_transport_backoff" => {
            Ok(FeatureConveyorOrchestrationReason::ReviewTransportBackoff)
        }
        "review_budget_exhausted" => Ok(FeatureConveyorOrchestrationReason::ReviewBudgetExhausted),
        "publication_failed" => Ok(FeatureConveyorOrchestrationReason::PublicationFailed),
        "replacement_candidate_contract_unavailable" => {
            Ok(FeatureConveyorOrchestrationReason::ReplacementCandidateContractUnavailable)
        }
        "repair_budget_exhausted" => Ok(FeatureConveyorOrchestrationReason::RepairBudgetExhausted),
        "active_processing_budget_exhausted" => {
            Ok(FeatureConveyorOrchestrationReason::ActiveProcessingBudgetExhausted)
        }
        "cancelled" => Ok(FeatureConveyorOrchestrationReason::Cancelled),
        "failed" => Ok(FeatureConveyorOrchestrationReason::Failed),
        "succeeded" => Ok(FeatureConveyorOrchestrationReason::Succeeded),
        _ => Err(MasterError::InvalidStoredState(
            "unknown orchestration reason".to_string(),
        )),
    }
}

fn orchestration_pause_kind_str(kind: FeatureConveyorOrchestrationPauseKind) -> &'static str {
    match kind {
        FeatureConveyorOrchestrationPauseKind::Provider => "provider",
        FeatureConveyorOrchestrationPauseKind::Worker => "worker",
        FeatureConveyorOrchestrationPauseKind::Maintenance => "maintenance",
        FeatureConveyorOrchestrationPauseKind::Owner => "owner",
    }
}

fn parse_orchestration_pause_kind(
    value: &str,
) -> Result<FeatureConveyorOrchestrationPauseKind, MasterError> {
    match value {
        "provider" => Ok(FeatureConveyorOrchestrationPauseKind::Provider),
        "worker" => Ok(FeatureConveyorOrchestrationPauseKind::Worker),
        "maintenance" => Ok(FeatureConveyorOrchestrationPauseKind::Maintenance),
        "owner" => Ok(FeatureConveyorOrchestrationPauseKind::Owner),
        _ => Err(MasterError::InvalidStoredState(
            "unknown orchestration pause kind".to_string(),
        )),
    }
}

fn orchestration_stage_for_lifecycle_status(
    status: FeatureLifecycleStatus,
) -> FeatureConveyorOrchestrationStage {
    match status {
        FeatureLifecycleStatus::Queued | FeatureLifecycleStatus::Implementing => {
            FeatureConveyorOrchestrationStage::Implementing
        }
        FeatureLifecycleStatus::Validating => FeatureConveyorOrchestrationStage::Validating,
        FeatureLifecycleStatus::Reviewing => FeatureConveyorOrchestrationStage::Reviewing,
        FeatureLifecycleStatus::Publishing => FeatureConveyorOrchestrationStage::Publishing,
        FeatureLifecycleStatus::VerifyingMain => FeatureConveyorOrchestrationStage::VerifyingMain,
        FeatureLifecycleStatus::Repairing => FeatureConveyorOrchestrationStage::Repairing,
        FeatureLifecycleStatus::Paused => FeatureConveyorOrchestrationStage::Paused,
        FeatureLifecycleStatus::AttentionRequired => {
            FeatureConveyorOrchestrationStage::AttentionRequired
        }
        FeatureLifecycleStatus::Failed | FeatureLifecycleStatus::Cancelled => {
            FeatureConveyorOrchestrationStage::Failed
        }
        FeatureLifecycleStatus::Succeeded | FeatureLifecycleStatus::Abandoned => {
            FeatureConveyorOrchestrationStage::Succeeded
        }
        FeatureLifecycleStatus::Quarantined => FeatureConveyorOrchestrationStage::Quarantined,
    }
}

fn owner_lifecycle_status(
    status: FeatureLifecycleStatus,
) -> Result<FeatureConveyorOwnerLifecycleStatus, MasterError> {
    match status {
        FeatureLifecycleStatus::Implementing => {
            Ok(FeatureConveyorOwnerLifecycleStatus::Implementing)
        }
        FeatureLifecycleStatus::Validating => Ok(FeatureConveyorOwnerLifecycleStatus::Validating),
        FeatureLifecycleStatus::Reviewing => Ok(FeatureConveyorOwnerLifecycleStatus::Reviewing),
        FeatureLifecycleStatus::Publishing => Ok(FeatureConveyorOwnerLifecycleStatus::Publishing),
        FeatureLifecycleStatus::VerifyingMain => {
            Ok(FeatureConveyorOwnerLifecycleStatus::VerifyingMain)
        }
        FeatureLifecycleStatus::Repairing => Ok(FeatureConveyorOwnerLifecycleStatus::Repairing),
        FeatureLifecycleStatus::Paused => Ok(FeatureConveyorOwnerLifecycleStatus::Paused),
        FeatureLifecycleStatus::AttentionRequired => {
            Ok(FeatureConveyorOwnerLifecycleStatus::AttentionRequired)
        }
        FeatureLifecycleStatus::Failed => Ok(FeatureConveyorOwnerLifecycleStatus::Failed),
        FeatureLifecycleStatus::Cancelled => Ok(FeatureConveyorOwnerLifecycleStatus::Cancelled),
        FeatureLifecycleStatus::Quarantined => Ok(FeatureConveyorOwnerLifecycleStatus::Quarantined),
        FeatureLifecycleStatus::Queued
        | FeatureLifecycleStatus::Succeeded
        | FeatureLifecycleStatus::Abandoned => Err(MasterError::InvalidStoredState(
            "active lease has an invalid owner-control lifecycle".to_string(),
        )),
    }
}

fn lifecycle_status_for_orchestration_stage(
    stage: FeatureConveyorOrchestrationStage,
) -> FeatureLifecycleStatus {
    match stage {
        FeatureConveyorOrchestrationStage::Implementing => FeatureLifecycleStatus::Implementing,
        FeatureConveyorOrchestrationStage::Validating => FeatureLifecycleStatus::Validating,
        FeatureConveyorOrchestrationStage::Reviewing => FeatureLifecycleStatus::Reviewing,
        FeatureConveyorOrchestrationStage::Publishing => FeatureLifecycleStatus::Publishing,
        FeatureConveyorOrchestrationStage::VerifyingMain => FeatureLifecycleStatus::VerifyingMain,
        FeatureConveyorOrchestrationStage::Repairing => FeatureLifecycleStatus::Repairing,
        FeatureConveyorOrchestrationStage::Paused => FeatureLifecycleStatus::Paused,
        FeatureConveyorOrchestrationStage::AttentionRequired => {
            FeatureLifecycleStatus::AttentionRequired
        }
        FeatureConveyorOrchestrationStage::Failed => FeatureLifecycleStatus::Failed,
        FeatureConveyorOrchestrationStage::Succeeded => FeatureLifecycleStatus::Succeeded,
        FeatureConveyorOrchestrationStage::Quarantined => FeatureLifecycleStatus::Quarantined,
    }
}

fn orchestration_checkpoint_sha256(
    feature_id: Uuid,
    orchestration_revision: u64,
    lifecycle_revision: u64,
    decision: DerivedOrchestrationDecision,
    replacement_candidates_used: u8,
    active_processing_ms: u64,
) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"assemblywright.orchestration-checkpoint.v1\0");
    digest.update(feature_id.as_bytes());
    digest.update(orchestration_revision.to_be_bytes());
    digest.update(lifecycle_revision.to_be_bytes());
    digest.update(orchestration_stage_str(decision.stage).as_bytes());
    digest.update([0]);
    digest.update(orchestration_action_str(decision.action).as_bytes());
    digest.update([0]);
    digest.update(orchestration_reason_str(decision.reason).as_bytes());
    digest.update([replacement_candidates_used]);
    digest.update(active_processing_ms.to_be_bytes());
    digest.update([u8::from(decision.effect_possible)]);
    if let Some(kind) = decision.pause_kind {
        digest.update(orchestration_pause_kind_str(kind).as_bytes());
    }
    digest.update(decision.next_retry_at_ms.unwrap_or(0).to_be_bytes());
    digest.update(decision.evidence_sha256.unwrap_or([0; 32]));
    digest.finalize().into()
}

fn orchestration_checkpoint_id(digest: [u8; 32]) -> Uuid {
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StoredOrchestrationCheckpoint {
    checkpoint_id: Uuid,
    lifecycle_revision: u64,
    stage: FeatureConveyorOrchestrationStage,
    action: FeatureConveyorOrchestrationAction,
    reason: FeatureConveyorOrchestrationReason,
    checkpoint_sha256: [u8; 32],
}

fn load_orchestration_state_tx(
    tx: &Transaction<'_>,
    feature_id: Uuid,
) -> Result<Option<StoredOrchestrationState>, MasterError> {
    tx.query_row(
        "SELECT orchestration_revision,checkpoint_id,stage,resume_stage,pause_kind,
                replacement_candidates_used,active_processing_ms,clock_started_at_ms,
                next_retry_at_ms,effect_possible
         FROM feature_orchestration_state WHERE feature_id=?1",
        [feature_id.to_string()],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, i64>(5)?,
                row.get::<_, i64>(6)?,
                row.get::<_, Option<i64>>(7)?,
                row.get::<_, Option<i64>>(8)?,
                row.get::<_, i64>(9)?,
            ))
        },
    )
    .optional()?
    .map(
        |(
            orchestration_revision,
            checkpoint_id,
            stage,
            resume_stage,
            pause_kind,
            replacement_candidates_used,
            active_processing_ms,
            clock_started_at_ms,
            next_retry_at_ms,
            effect_possible,
        )| {
            Ok(StoredOrchestrationState {
                orchestration_revision: i64_to_u64(orchestration_revision)?,
                checkpoint_id: parse_uuid(&checkpoint_id)?,
                stage: parse_orchestration_stage(&stage)?,
                resume_stage: resume_stage
                    .as_deref()
                    .map(parse_orchestration_stage)
                    .transpose()?,
                pause_kind: pause_kind
                    .as_deref()
                    .map(parse_orchestration_pause_kind)
                    .transpose()?,
                replacement_candidates_used: u8::try_from(replacement_candidates_used).map_err(
                    |_| {
                        MasterError::InvalidStoredState(
                            "invalid orchestration repair count".to_string(),
                        )
                    },
                )?,
                active_processing_ms: i64_to_u64(active_processing_ms)?,
                clock_started_at_ms: clock_started_at_ms.map(i64_to_u64).transpose()?,
                next_retry_at_ms: next_retry_at_ms.map(i64_to_u64).transpose()?,
                effect_possible: parse_stored_boolean(
                    effect_possible,
                    "orchestration effect_possible",
                )?,
            })
        },
    )
    .transpose()
}

fn orchestration_paused_checkpoint_is_restart_safe_tx(
    tx: &Transaction<'_>,
    feature_id: Uuid,
    lifecycle_revision: u64,
) -> Result<bool, MasterError> {
    let safe: bool = tx.query_row(
        "SELECT EXISTS(
           SELECT 1 FROM feature_orchestration_activation a
           JOIN feature_orchestration_state s ON s.feature_id=?1
           JOIN feature_orchestration_checkpoints c ON c.checkpoint_id=s.checkpoint_id
           WHERE a.singleton=1 AND s.stage='paused' AND s.pause_kind IS NOT NULL
             AND s.resume_stage IS NOT NULL AND s.clock_started_at_ms IS NULL
             AND s.effect_possible=0 AND c.effect_possible=0
             AND c.lifecycle_revision=?2 AND c.orchestration_revision=s.orchestration_revision
             AND c.stage='paused'
         )",
        params![feature_id.to_string(), u64_to_i64(lifecycle_revision)?],
        |row| row.get(0),
    )?;
    Ok(safe)
}

fn load_orchestration_checkpoint_tx(
    tx: &Transaction<'_>,
    checkpoint_id: Uuid,
) -> Result<StoredOrchestrationCheckpoint, MasterError> {
    tx.query_row(
        "SELECT lifecycle_revision,stage,action,reason,checkpoint_sha256
         FROM feature_orchestration_checkpoints WHERE checkpoint_id=?1",
        [checkpoint_id.to_string()],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Vec<u8>>(4)?,
            ))
        },
    )
    .optional()?
    .map(
        |(lifecycle_revision, stage, action, reason, checkpoint_sha256)| {
            Ok::<StoredOrchestrationCheckpoint, MasterError>(StoredOrchestrationCheckpoint {
                checkpoint_id,
                lifecycle_revision: i64_to_u64(lifecycle_revision)?,
                stage: parse_orchestration_stage(&stage)?,
                action: parse_orchestration_action(&action)?,
                reason: parse_orchestration_reason(&reason)?,
                checkpoint_sha256: digest_array(&checkpoint_sha256)?,
            })
        },
    )
    .transpose()?
    .ok_or_else(|| {
        MasterError::InvalidStoredState("orchestration checkpoint is missing".to_string())
    })
}

fn derive_orchestration_decision_tx(
    tx: &Transaction<'_>,
    feature_id: Uuid,
    status: FeatureLifecycleStatus,
    state: Option<&StoredOrchestrationState>,
    now_ms: u64,
) -> Result<DerivedOrchestrationDecision, MasterError> {
    if matches!(
        status,
        FeatureLifecycleStatus::Publishing | FeatureLifecycleStatus::VerifyingMain
    ) {
        let ambiguous: bool = tx.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM feature_publication_action_intents i
               JOIN feature_publications p ON p.publication_id=i.publication_id
               LEFT JOIN feature_publication_action_outcomes o
                 ON o.publication_id=i.publication_id AND o.ordinal=i.ordinal
               WHERE p.feature_id=?1 AND o.publication_id IS NULL
             )",
            [feature_id.to_string()],
            |row| row.get(0),
        )?;
        if ambiguous {
            return Ok(DerivedOrchestrationDecision {
                stage: FeatureConveyorOrchestrationStage::Quarantined,
                action: FeatureConveyorOrchestrationAction::ReconcileQuarantine,
                reason: FeatureConveyorOrchestrationReason::ExistingEffectAmbiguous,
                pause_kind: None,
                next_retry_at_ms: None,
                evidence_sha256: None,
                effect_possible: true,
            });
        }
    }
    if status == FeatureLifecycleStatus::Reviewing {
        let ambiguous: bool = tx.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM feature_review_calls c
               LEFT JOIN feature_review_call_outcomes o ON o.review_call_id=c.review_call_id
               WHERE c.feature_id=?1 AND o.review_call_id IS NULL
             )",
            [feature_id.to_string()],
            |row| row.get(0),
        )?;
        if ambiguous {
            return Ok(DerivedOrchestrationDecision {
                stage: FeatureConveyorOrchestrationStage::Quarantined,
                action: FeatureConveyorOrchestrationAction::ReconcileQuarantine,
                reason: FeatureConveyorOrchestrationReason::ExistingEffectAmbiguous,
                pause_kind: None,
                next_retry_at_ms: None,
                evidence_sha256: None,
                effect_possible: true,
            });
        }
    }
    if status == FeatureLifecycleStatus::Paused {
        let state = state.ok_or_else(|| {
            MasterError::InvalidStoredState(
                "paused lifecycle lacks orchestration state".to_string(),
            )
        })?;
        if state.pause_kind == Some(FeatureConveyorOrchestrationPauseKind::Provider)
            && state.next_retry_at_ms.is_some_and(|retry| now_ms >= retry)
        {
            let stage = state.resume_stage.ok_or_else(|| {
                MasterError::InvalidStoredState("paused lifecycle lacks resume stage".to_string())
            })?;
            return Ok(DerivedOrchestrationDecision {
                stage,
                action: FeatureConveyorOrchestrationAction::RetryReviewTransport,
                reason: FeatureConveyorOrchestrationReason::CheckpointEffectFree,
                pause_kind: None,
                next_retry_at_ms: None,
                evidence_sha256: None,
                effect_possible: false,
            });
        }
        return Ok(DerivedOrchestrationDecision {
            stage: FeatureConveyorOrchestrationStage::Paused,
            action: FeatureConveyorOrchestrationAction::AwaitReviewDecision,
            reason: FeatureConveyorOrchestrationReason::ReviewTransportBackoff,
            pause_kind: state.pause_kind,
            next_retry_at_ms: state.next_retry_at_ms,
            evidence_sha256: None,
            effect_possible: false,
        });
    }
    let ordinary = |stage, action| DerivedOrchestrationDecision {
        stage,
        action,
        reason: FeatureConveyorOrchestrationReason::CheckpointEffectFree,
        pause_kind: None,
        next_retry_at_ms: None,
        evidence_sha256: None,
        effect_possible: false,
    };
    match status {
        FeatureLifecycleStatus::Implementing => Ok(ordinary(
            FeatureConveyorOrchestrationStage::Implementing,
            FeatureConveyorOrchestrationAction::AwaitImplementationEvidence,
        )),
        FeatureLifecycleStatus::Validating => {
            let failed = tx
                .query_row(
                    "SELECT c.evidence_manifest_sha256
                     FROM feature_validation_completions c
                     JOIN feature_validation_attempts a ON a.validation_id=c.validation_id
                     WHERE a.feature_id=?1 AND c.passed=0
                     ORDER BY c.completed_at_ms DESC LIMIT 1",
                    [feature_id.to_string()],
                    |row| row.get::<_, Vec<u8>>(0),
                )
                .optional()?;
            if let Some(evidence) = failed {
                return Ok(DerivedOrchestrationDecision {
                    stage: FeatureConveyorOrchestrationStage::Repairing,
                    action: FeatureConveyorOrchestrationAction::ReplacementCandidateRequired,
                    reason: FeatureConveyorOrchestrationReason::ValidationFailed,
                    pause_kind: None,
                    next_retry_at_ms: None,
                    evidence_sha256: Some(digest_array(&evidence)?),
                    effect_possible: false,
                });
            }
            Ok(ordinary(
                FeatureConveyorOrchestrationStage::Validating,
                FeatureConveyorOrchestrationAction::AwaitValidationEvidence,
            ))
        }
        FeatureLifecycleStatus::Reviewing => {
            let decision = tx
                .query_row(
                    "SELECT d.decision,d.decision_sha256
                     FROM feature_review_decisions d
                     WHERE d.feature_id=?1 ORDER BY d.decided_at_ms DESC LIMIT 1",
                    [feature_id.to_string()],
                    |row| Ok((row.get::<_, String>(0)?, row.get::<_, Vec<u8>>(1)?)),
                )
                .optional()?;
            if let Some((decision, evidence)) = decision {
                if decision == "rejected" {
                    return Ok(DerivedOrchestrationDecision {
                        stage: FeatureConveyorOrchestrationStage::Repairing,
                        action: FeatureConveyorOrchestrationAction::ReplacementCandidateRequired,
                        reason: FeatureConveyorOrchestrationReason::ReviewRejected,
                        pause_kind: None,
                        next_retry_at_ms: None,
                        evidence_sha256: Some(digest_array(&evidence)?),
                        effect_possible: false,
                    });
                }
            }
            let latest_candidate = tx
                .query_row(
                    "SELECT candidate_commit FROM feature_review_calls
                     WHERE feature_id=?1
                     ORDER BY feature_call DESC,started_at_ms DESC,review_call_id DESC
                     LIMIT 1",
                    [feature_id.to_string()],
                    |row| row.get::<_, String>(0),
                )
                .optional()?;
            if let Some(candidate_commit) = latest_candidate {
                let candidate_calls = i64_to_u64(tx.query_row(
                    "SELECT COUNT(*) FROM feature_review_calls c
                     JOIN feature_review_call_outcomes o
                       ON o.review_call_id=c.review_call_id
                     WHERE c.feature_id=?1 AND c.candidate_commit=?2",
                    params![feature_id.to_string(), candidate_commit],
                    |row| row.get::<_, i64>(0),
                )?)?;
                let feature_calls = i64_to_u64(tx.query_row(
                    "SELECT COUNT(*) FROM feature_review_calls c
                     JOIN feature_review_call_outcomes o
                       ON o.review_call_id=c.review_call_id
                     WHERE c.feature_id=?1",
                    [feature_id.to_string()],
                    |row| row.get::<_, i64>(0),
                )?)?;
                if candidate_calls
                    >= u64::from(MAX_FEATURE_CONVEYOR_REVIEW_TRANSPORT_ATTEMPTS_PER_CANDIDATE)
                    || feature_calls >= u64::from(MAX_FEATURE_CONVEYOR_REVIEW_CALLS_PER_FEATURE)
                {
                    return Ok(DerivedOrchestrationDecision {
                        stage: FeatureConveyorOrchestrationStage::AttentionRequired,
                        action: FeatureConveyorOrchestrationAction::OwnerAttentionRequired,
                        reason: FeatureConveyorOrchestrationReason::ReviewBudgetExhausted,
                        pause_kind: None,
                        next_retry_at_ms: None,
                        evidence_sha256: None,
                        effect_possible: false,
                    });
                }
            }
            let retry = tx
                .query_row(
                    "SELECT o.next_retry_at_ms,o.outcome_sha256
                     FROM feature_review_call_outcomes o
                     JOIN feature_review_calls c ON c.review_call_id=o.review_call_id
                     WHERE c.feature_id=?1 AND o.next_retry_at_ms IS NOT NULL
                     ORDER BY o.completed_at_ms DESC LIMIT 1",
                    [feature_id.to_string()],
                    |row| Ok((row.get::<_, i64>(0)?, row.get::<_, Vec<u8>>(1)?)),
                )
                .optional()?;
            if let Some((retry_at, evidence)) = retry {
                let retry_at = i64_to_u64(retry_at)?;
                if now_ms < retry_at {
                    return Ok(DerivedOrchestrationDecision {
                        stage: FeatureConveyorOrchestrationStage::Paused,
                        action: FeatureConveyorOrchestrationAction::AwaitReviewDecision,
                        reason: FeatureConveyorOrchestrationReason::ReviewTransportBackoff,
                        pause_kind: Some(FeatureConveyorOrchestrationPauseKind::Provider),
                        next_retry_at_ms: Some(retry_at),
                        evidence_sha256: Some(digest_array(&evidence)?),
                        effect_possible: false,
                    });
                }
                return Ok(DerivedOrchestrationDecision {
                    stage: FeatureConveyorOrchestrationStage::Reviewing,
                    action: FeatureConveyorOrchestrationAction::RetryReviewTransport,
                    reason: FeatureConveyorOrchestrationReason::CheckpointEffectFree,
                    pause_kind: None,
                    next_retry_at_ms: None,
                    evidence_sha256: Some(digest_array(&evidence)?),
                    effect_possible: false,
                });
            }
            Ok(ordinary(
                FeatureConveyorOrchestrationStage::Reviewing,
                FeatureConveyorOrchestrationAction::AwaitReviewDecision,
            ))
        }
        FeatureLifecycleStatus::Publishing => Ok(ordinary(
            FeatureConveyorOrchestrationStage::Publishing,
            FeatureConveyorOrchestrationAction::AwaitPublicationEvidence,
        )),
        FeatureLifecycleStatus::VerifyingMain => Ok(ordinary(
            FeatureConveyorOrchestrationStage::VerifyingMain,
            FeatureConveyorOrchestrationAction::AwaitMainVerification,
        )),
        FeatureLifecycleStatus::Repairing => Ok(DerivedOrchestrationDecision {
            stage: FeatureConveyorOrchestrationStage::AttentionRequired,
            action: FeatureConveyorOrchestrationAction::OwnerAttentionRequired,
            reason: FeatureConveyorOrchestrationReason::ReplacementCandidateContractUnavailable,
            pause_kind: None,
            next_retry_at_ms: None,
            evidence_sha256: None,
            effect_possible: false,
        }),
        FeatureLifecycleStatus::AttentionRequired => Ok(DerivedOrchestrationDecision {
            stage: FeatureConveyorOrchestrationStage::AttentionRequired,
            action: FeatureConveyorOrchestrationAction::OwnerAttentionRequired,
            reason: FeatureConveyorOrchestrationReason::ReplacementCandidateContractUnavailable,
            pause_kind: None,
            next_retry_at_ms: None,
            evidence_sha256: None,
            effect_possible: false,
        }),
        FeatureLifecycleStatus::Failed | FeatureLifecycleStatus::Cancelled => {
            Ok(DerivedOrchestrationDecision {
                stage: FeatureConveyorOrchestrationStage::Failed,
                action: FeatureConveyorOrchestrationAction::Terminal,
                reason: if status == FeatureLifecycleStatus::Cancelled {
                    FeatureConveyorOrchestrationReason::Cancelled
                } else {
                    FeatureConveyorOrchestrationReason::Failed
                },
                pause_kind: None,
                next_retry_at_ms: None,
                evidence_sha256: None,
                effect_possible: true,
            })
        }
        FeatureLifecycleStatus::Succeeded | FeatureLifecycleStatus::Abandoned => {
            Ok(DerivedOrchestrationDecision {
                stage: FeatureConveyorOrchestrationStage::Succeeded,
                action: FeatureConveyorOrchestrationAction::Terminal,
                reason: FeatureConveyorOrchestrationReason::Succeeded,
                pause_kind: None,
                next_retry_at_ms: None,
                evidence_sha256: None,
                effect_possible: false,
            })
        }
        FeatureLifecycleStatus::Quarantined => Ok(DerivedOrchestrationDecision {
            stage: FeatureConveyorOrchestrationStage::Quarantined,
            action: FeatureConveyorOrchestrationAction::ReconcileQuarantine,
            reason: FeatureConveyorOrchestrationReason::ExistingEffectAmbiguous,
            pause_kind: None,
            next_retry_at_ms: None,
            evidence_sha256: None,
            effect_possible: true,
        }),
        FeatureLifecycleStatus::Queued | FeatureLifecycleStatus::Paused => {
            Err(MasterError::InvalidFeatureTransition)
        }
    }
}

fn orchestration_projection(
    feature_id: Uuid,
    state: &StoredOrchestrationState,
    checkpoint: &StoredOrchestrationCheckpoint,
) -> FeatureConveyorOrchestrationProjection {
    FeatureConveyorOrchestrationProjection {
        schema_version: FEATURE_CONVEYOR_ORCHESTRATION_SCHEMA_VERSION,
        feature_id,
        lifecycle_revision: checkpoint.lifecycle_revision,
        orchestration_revision: state.orchestration_revision,
        stage: checkpoint.stage,
        action: checkpoint.action,
        reason: checkpoint.reason,
        checkpoint_id: checkpoint.checkpoint_id,
        checkpoint_sha256: checkpoint.checkpoint_sha256,
        replacement_candidates_used: state.replacement_candidates_used,
        active_processing_ms: state.active_processing_ms,
        active_processing_budget_ms: MAX_FEATURE_CONVEYOR_ACTIVE_PROCESSING_MS,
        pause_kind: state.pause_kind,
        next_retry_at_ms: state.next_retry_at_ms,
        effect_possible: state.effect_possible,
        activated: true,
    }
}

fn publication_plan_tx(
    tx: &Transaction<'_>,
    request: &FeatureConveyorPublicationRequest,
    now_ms: u64,
) -> Result<PublicationExecutionPlan, MasterError> {
    request.validate()?;
    require_emergency_unpaused_tx(tx)?;
    require_emergency_pause_revision_tx(tx, request.expected_emergency_pause_revision)?;
    require_queue_revision_tx(tx, request.expected_queue_revision)?;
    require_active_lease_tx(tx, request.feature_id)?;
    require_current_feature_grants_tx(tx, request.feature_id, now_ms)?;
    let (status, revision) = feature_status_and_revision_tx(tx, request.feature_id)?;
    let lifecycle_matches = (status == FeatureLifecycleStatus::Publishing
        && revision == request.expected_lifecycle_revision)
        || (status == FeatureLifecycleStatus::VerifyingMain
            && request.expected_lifecycle_revision.checked_add(1) == Some(revision));
    if !lifecycle_matches {
        return Err(MasterError::PublicationCoordinatorUnavailable);
    }
    let lease_id: String = tx.query_row(
        "SELECT lease_id FROM feature_active_lease WHERE singleton=1 AND feature_id=?1",
        [request.feature_id.to_string()],
        |row| row.get(0),
    )?;
    if parse_uuid(&lease_id)? != request.feature_lease_id {
        return Err(MasterError::PublicationCoordinatorUnavailable);
    }
    let row = tx
        .query_row(
            "SELECT s.repository_id,s.canonical_manifest_json,s.provider_id,s.model_id,
                    s.registration_grant_revision,s.cloud_disclosure_grant_revision,
                    s.publication_grant_revision,i.feature_lease_id,i.candidate_commit,
                    i.candidate_tree,i.base_commit,v.evidence_manifest_sha256,
                    c.candidate_diff_sha256,c.review_packet_sha256,d.decision,
                    d.decision_sha256,d.lifecycle_revision
             FROM feature_specification_revisions s
             JOIN feature_artifact_integrations i ON i.integration_id=?3
             JOIN feature_validation_completions v ON v.validation_id=?4
             JOIN feature_review_calls c ON c.review_call_id=?5
             JOIN feature_review_decisions d ON d.review_call_id=c.review_call_id
             WHERE s.feature_id=?1 AND s.revision=?2
               AND i.feature_id=s.feature_id AND i.specification_revision=s.revision
               AND v.validation_id=c.validation_id AND c.integration_id=i.integration_id
               AND c.feature_id=s.feature_id",
            params![
                request.feature_id.to_string(),
                u64_to_i64(request.specification_revision)?,
                request.integration_id.to_string(),
                request.validation_id.to_string(),
                request.review_call_id.to_string(),
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, String>(7)?,
                    row.get::<_, String>(8)?,
                    row.get::<_, String>(9)?,
                    row.get::<_, String>(10)?,
                    row.get::<_, Vec<u8>>(11)?,
                    row.get::<_, Vec<u8>>(12)?,
                    row.get::<_, Vec<u8>>(13)?,
                    row.get::<_, String>(14)?,
                    row.get::<_, Vec<u8>>(15)?,
                    row.get::<_, i64>(16)?,
                ))
            },
        )
        .optional()?
        .ok_or(MasterError::PublicationCoordinatorUnavailable)?;
    if row.2 != request.provider_id
        || row.3 != request.model_id
        || i64_to_u64(row.4)? != request.grants.registration
        || i64_to_u64(row.5)? != request.grants.cloud_disclosure
        || i64_to_u64(row.6)? != request.grants.autonomous_publication
        || parse_uuid(&row.7)? != request.feature_lease_id
        || row.8 != request.candidate_commit
        || row.9 != request.candidate_tree
        || row.10 != request.remote_base_commit
        || digest_array(&row.11)? != request.evidence_manifest_sha256
        || digest_array(&row.12)? != request.candidate_diff_sha256
        || row.14 != "approved"
        || digest_array(&row.15)? != request.review_decision_sha256
        || i64_to_u64(row.16)? != request.expected_lifecycle_revision
    {
        return Err(MasterError::PublicationCoordinatorUnavailable);
    }
    let manifest_value: Value = serde_json::from_str(&row.1)?;
    validate_review_safe_manifest_schema(&manifest_value)
        .map_err(|_| MasterError::PublicationCoordinatorUnavailable)?;
    validate_review_disclosure_value(&manifest_value)?;
    let manifest: ReviewSafeApprovedManifest = serde_json::from_value(manifest_value)
        .map_err(|_| MasterError::PublicationCoordinatorUnavailable)?;
    let base_branch = manifest
        .base_branch
        .filter(|value| valid_publication_token(value, 255))
        .ok_or(MasterError::PublicationCoordinatorUnavailable)?;
    let merge_strategy = manifest
        .merge_strategy
        .filter(|value| matches!(value.as_str(), "merge" | "squash" | "rebase"))
        .ok_or(MasterError::PublicationCoordinatorUnavailable)?;
    let post_merge_gate = manifest
        .post_merge_gate
        .filter(|value| value == "release-local")
        .ok_or(MasterError::PublicationCoordinatorUnavailable)?;
    if manifest.publication_checks.is_empty()
        || manifest.publication_checks.len()
            > assemblywright_protocol::MAX_FEATURE_CONVEYOR_PUBLICATION_CHECKS
        || manifest
            .publication_checks
            .iter()
            .any(|check| !valid_publication_token(check, 128))
    {
        return Err(MasterError::PublicationCoordinatorUnavailable);
    }
    let mut required_checks = manifest.publication_checks;
    let original_len = required_checks.len();
    required_checks.sort();
    required_checks.dedup();
    if required_checks.len() != original_len {
        return Err(MasterError::PublicationCoordinatorUnavailable);
    }
    let repository_id = parse_uuid(&row.0)?;
    let feature_branch = format!("assemblywright-{}", request.feature_id);
    if publication_branch_policy_sha256(
        repository_id,
        request.feature_id,
        &base_branch,
        &required_checks,
        &merge_strategy,
        &post_merge_gate,
    )? != request.branch_policy_sha256
    {
        return Err(MasterError::PublicationCoordinatorUnavailable);
    }
    Ok(PublicationExecutionPlan {
        request: request.clone(),
        repository_id,
        feature_branch,
        base_branch,
        required_checks,
        merge_strategy,
        post_merge_gate,
    })
}

fn valid_publication_token(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value.trim() == value
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'/'))
        && !value.contains("..")
        && !value.starts_with('/')
        && !value.ends_with('/')
}

fn insert_publication_action_intent_tx(
    tx: &Transaction<'_>,
    publication_id: Uuid,
    action: PublicationActionKind,
    ordinal: u64,
    now_ms: u64,
) -> Result<(), MasterError> {
    let mut digest = Sha256::new();
    digest.update(b"assemblywright.publication-action-intent.v1\0");
    digest.update(publication_id.as_bytes());
    digest.update(ordinal.to_be_bytes());
    digest.update(action.as_str().as_bytes());
    let intent_sha256: [u8; 32] = digest.finalize().into();
    tx.execute(
        "INSERT INTO feature_publication_action_intents (
           publication_id,ordinal,action_kind,intent_sha256,created_at_ms
         ) VALUES (?1,?2,?3,?4,?5)",
        params![
            publication_id.to_string(),
            u64_to_i64(ordinal)?,
            action.as_str(),
            intent_sha256.as_slice(),
            u64_to_i64(now_ms)?,
        ],
    )?;
    Ok(())
}

fn next_publication_action_tx(
    tx: &Transaction<'_>,
    publication_id: Uuid,
) -> Result<Option<PublicationActionKind>, MasterError> {
    let value = tx
        .query_row(
            "SELECT i.action_kind FROM feature_publication_action_intents i
             LEFT JOIN feature_publication_action_outcomes o
               ON o.publication_id=i.publication_id AND o.ordinal=i.ordinal
             WHERE i.publication_id=?1 AND o.publication_id IS NULL
             ORDER BY i.ordinal LIMIT 1",
            [publication_id.to_string()],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    value
        .map(|value| match value.as_str() {
            "push_branch" => Ok(PublicationActionKind::PushBranch),
            "upsert_pull_request" => Ok(PublicationActionKind::UpsertPullRequest),
            "observe_required_checks" => Ok(PublicationActionKind::ObserveRequiredChecks),
            "verify_pull_request_head" => Ok(PublicationActionKind::VerifyPullRequestHead),
            "merge_pull_request" => Ok(PublicationActionKind::MergePullRequest),
            "reconcile_remote_main" => Ok(PublicationActionKind::ReconcileRemoteMain),
            "run_post_merge_gate" => Ok(PublicationActionKind::RunPostMergeGate),
            _ => Err(MasterError::InvalidStoredState(
                "unknown publication action kind".to_string(),
            )),
        })
        .transpose()
}

fn validate_publication_action_evidence(
    plan: &PublicationExecutionPlan,
    evidence: &PublicationActionEvidence,
) -> Result<(), MasterError> {
    evidence.validate()?;
    let expected_checks =
        feature_conveyor_publication_required_checks_sha256(&plan.required_checks)?;
    let checks_expected = !matches!(
        evidence.action,
        PublicationActionKind::PushBranch | PublicationActionKind::UpsertPullRequest
    );
    let merge_expected = matches!(
        evidence.action,
        PublicationActionKind::MergePullRequest
            | PublicationActionKind::ReconcileRemoteMain
            | PublicationActionKind::RunPostMergeGate
    );
    if evidence.publication_id != plan.request.publication_id
        || evidence.remote_base_commit != plan.request.remote_base_commit
        || evidence.candidate_commit != plan.request.candidate_commit
        || evidence.feature_branch != plan.feature_branch
        || evidence.base_branch != plan.base_branch
        || (checks_expected
            && (evidence.required_checks_sha256 != Some(expected_checks)
                || usize::from(evidence.required_check_count) != plan.required_checks.len()
                || !evidence.required_checks_passed))
        || (!checks_expected
            && (evidence.required_checks_sha256.is_some()
                || evidence.required_check_count != 0
                || evidence.required_checks_passed))
        || (merge_expected
            && evidence.merge_strategy.as_deref() != Some(plan.merge_strategy.as_str()))
        || (!merge_expected && evidence.merge_strategy.is_some())
        || (evidence.action == PublicationActionKind::RunPostMergeGate
            && (evidence.post_merge_gate_id.as_deref() != Some(plan.post_merge_gate.as_str())
                || !evidence.post_merge_gate_passed))
        || (evidence.action != PublicationActionKind::RunPostMergeGate
            && (evidence.post_merge_gate_id.is_some() || evidence.post_merge_gate_passed))
    {
        return Err(MasterError::PublicationCoordinatorUnavailable);
    }
    let candidate_head = matches!(
        evidence.action,
        PublicationActionKind::PushBranch
            | PublicationActionKind::UpsertPullRequest
            | PublicationActionKind::ObserveRequiredChecks
            | PublicationActionKind::VerifyPullRequestHead
            | PublicationActionKind::MergePullRequest
    );
    if candidate_head && evidence.observed_head_commit != plan.request.candidate_commit {
        return Err(MasterError::PublicationCoordinatorUnavailable);
    }
    Ok(())
}

fn validate_publication_action_evidence_tx(
    tx: &Transaction<'_>,
    plan: &PublicationExecutionPlan,
    evidence: &PublicationActionEvidence,
) -> Result<(), MasterError> {
    if matches!(
        evidence.action,
        PublicationActionKind::ObserveRequiredChecks
            | PublicationActionKind::VerifyPullRequestHead
            | PublicationActionKind::MergePullRequest
    ) {
        let pull_request: i64 = tx.query_row(
            "SELECT pull_request_number FROM feature_publication_action_outcomes
             WHERE publication_id=?1 AND action_kind='upsert_pull_request'",
            [plan.request.publication_id.to_string()],
            |row| row.get(0),
        )?;
        if evidence.pull_request_number.map(u64_to_i64).transpose()? != Some(pull_request) {
            return Err(MasterError::PublicationCoordinatorUnavailable);
        }
    }
    if matches!(
        evidence.action,
        PublicationActionKind::ReconcileRemoteMain | PublicationActionKind::RunPostMergeGate
    ) {
        let merge_commit: String = tx.query_row(
            "SELECT merge_commit FROM feature_publication_action_outcomes
             WHERE publication_id=?1 AND action_kind='merge_pull_request' AND passed=1",
            [plan.request.publication_id.to_string()],
            |row| row.get(0),
        )?;
        if evidence.observed_head_commit != merge_commit
            || evidence.resulting_main_commit.as_deref() != Some(merge_commit.as_str())
        {
            return Err(MasterError::PublicationCoordinatorUnavailable);
        }
    }
    Ok(())
}

fn publication_merge_commit_tx(
    tx: &Transaction<'_>,
    publication_id: Uuid,
) -> Result<String, MasterError> {
    let merge_commit: String = tx.query_row(
        "SELECT merge_commit FROM feature_publication_action_outcomes
         WHERE publication_id=?1 AND action_kind='merge_pull_request' AND passed=1",
        [publication_id.to_string()],
        |row| row.get(0),
    )?;
    let remote_main: String = tx.query_row(
        "SELECT observed_commit FROM feature_publication_action_outcomes
         WHERE publication_id=?1 AND action_kind='reconcile_remote_main' AND passed=1",
        [publication_id.to_string()],
        |row| row.get(0),
    )?;
    let post_merge: String = tx.query_row(
        "SELECT observed_commit FROM feature_publication_action_outcomes
         WHERE publication_id=?1 AND action_kind='run_post_merge_gate' AND passed=1",
        [publication_id.to_string()],
        |row| row.get(0),
    )?;
    if merge_commit != remote_main || merge_commit != post_merge {
        return Err(MasterError::PublicationCoordinatorUnavailable);
    }
    Ok(merge_commit)
}

fn publication_receipt(
    plan: &PublicationExecutionPlan,
    lifecycle_revision: u64,
    queue_revision: u64,
    merge_commit: &str,
    post_merge_evidence_sha256: [u8; 32],
) -> FeatureConveyorPublicationReceipt {
    FeatureConveyorPublicationReceipt {
        schema_version: FEATURE_CONVEYOR_PUBLICATION_COORDINATOR_SCHEMA_VERSION,
        publication_id: plan.request.publication_id,
        feature_id: plan.request.feature_id,
        specification_revision: plan.request.specification_revision,
        lifecycle_revision,
        candidate_commit: plan.request.candidate_commit.clone(),
        merge_commit: merge_commit.to_string(),
        remote_main_commit: merge_commit.to_string(),
        post_merge_evidence_sha256,
        branch_policy_sha256: plan.request.branch_policy_sha256,
        queue_revision,
        emergency_pause_revision: plan.request.expected_emergency_pause_revision,
        grants: plan.request.grants,
        status: FeatureConveyorPublicationStatus::Succeeded,
    }
}

fn load_publication_receipt_tx(
    tx: &Transaction<'_>,
    request: &FeatureConveyorPublicationRequest,
) -> Result<Option<FeatureConveyorPublicationReceipt>, MasterError> {
    tx.query_row(
        "SELECT merge_commit,remote_main_commit,post_merge_evidence_sha256,
                lifecycle_revision,queue_revision
         FROM feature_publication_completions WHERE publication_id=?1",
        [request.publication_id.to_string()],
        |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Vec<u8>>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
            ))
        },
    )
    .optional()?
    .map(|row| {
        if row.0 != row.1 {
            return Err(MasterError::InvalidStoredState(
                "publication main reconciliation is inconsistent".to_string(),
            ));
        }
        Ok(FeatureConveyorPublicationReceipt {
            schema_version: FEATURE_CONVEYOR_PUBLICATION_COORDINATOR_SCHEMA_VERSION,
            publication_id: request.publication_id,
            feature_id: request.feature_id,
            specification_revision: request.specification_revision,
            lifecycle_revision: i64_to_u64(row.3)?,
            candidate_commit: request.candidate_commit.clone(),
            merge_commit: row.0.clone(),
            remote_main_commit: row.1,
            post_merge_evidence_sha256: digest_array(&row.2)?,
            branch_policy_sha256: request.branch_policy_sha256,
            queue_revision: i64_to_u64(row.4)?,
            emergency_pause_revision: request.expected_emergency_pause_revision,
            grants: request.grants,
            status: FeatureConveyorPublicationStatus::Succeeded,
        })
    })
    .transpose()
}

fn validate_review_packet_plan(
    plan: &ReviewGatewayExecutionPlan,
    packet: &FeatureConveyorReviewPacket,
) -> Result<(), MasterError> {
    packet.validate()?;
    if packet.feature_id != plan.request.feature_id
        || packet.specification_revision != plan.request.specification_revision
        || packet.approved_specification != plan.approved_specification
        || packet.approved_specification_sha256 != plan.approved_specification_sha256
        || packet.candidate_commit != plan.request.candidate_commit
        || packet.candidate_tree != plan.request.candidate_tree
        || packet.base_commit != plan.request.base_commit
        || packet.candidate_diff_sha256 != plan.request.candidate_diff_sha256
        || packet.evidence_manifest_sha256 != plan.request.evidence_manifest_sha256
        || packet.evidence_digests != plan.evidence_digests
        || packet.requirements_sha256 != plan.requirements_sha256
        || packet.requirement_ids != plan.requirement_ids
        || packet.provider_id != plan.request.provider_id
        || packet.model_id != plan.request.model_id
        || packet.grants != plan.request.grants
        || packet.sha256()? != plan.request.review_packet_sha256
    {
        return Err(MasterError::ReviewGatewayUnavailable);
    }
    Ok(())
}

fn validate_started_review_call_tx(
    tx: &Transaction<'_>,
    plan: &ReviewGatewayExecutionPlan,
) -> Result<(), MasterError> {
    let row = tx
        .query_row(
            "SELECT request_binding_sha256,candidate_attempt,feature_call,
                    EXISTS(SELECT 1 FROM feature_review_call_outcomes o
                           WHERE o.review_call_id=c.review_call_id)
             FROM feature_review_calls c WHERE review_call_id=?1",
            [plan.request.review_call_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, Vec<u8>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, bool>(3)?,
                ))
            },
        )
        .optional()?
        .ok_or(MasterError::ReviewGatewayUnavailable)?;
    if digest_array(&row.0)? != feature_conveyor_review_request_binding_sha256(&plan.request)?
        || row.1 != i64::from(plan.candidate_attempt)
        || row.2 != i64::from(plan.feature_call)
        || row.3
    {
        return Err(MasterError::ReviewGatewayUnavailable);
    }
    Ok(())
}

fn review_gateway_receipt(
    plan: &ReviewGatewayExecutionPlan,
    lifecycle_revision: u64,
    decision_sha256: [u8; 32],
    approved: bool,
) -> FeatureConveyorReviewGatewayReceipt {
    FeatureConveyorReviewGatewayReceipt {
        schema_version: FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION,
        review_call_id: plan.request.review_call_id,
        feature_id: plan.request.feature_id,
        specification_revision: plan.request.specification_revision,
        lifecycle_revision,
        feature_lease_id: plan.request.feature_lease_id,
        integration_id: plan.request.integration_id,
        validation_id: plan.request.validation_id,
        candidate_commit: plan.request.candidate_commit.clone(),
        candidate_diff_sha256: plan.request.candidate_diff_sha256,
        evidence_manifest_sha256: plan.request.evidence_manifest_sha256,
        review_packet_sha256: plan.request.review_packet_sha256,
        provider_id: plan.request.provider_id.clone(),
        model_id: plan.request.model_id.clone(),
        candidate_attempt: plan.candidate_attempt,
        feature_call: plan.feature_call,
        decision_sha256,
        queue_revision: plan.request.expected_queue_revision,
        emergency_pause_revision: plan.request.expected_emergency_pause_revision,
        grants: plan.request.grants,
        status: if approved {
            FeatureConveyorReviewGatewayStatus::Approved
        } else {
            FeatureConveyorReviewGatewayStatus::Rejected
        },
    }
}

fn load_review_gateway_receipt_tx(
    tx: &Transaction<'_>,
    request: &FeatureConveyorReviewGatewayRequest,
) -> Result<FeatureConveyorReviewGatewayReceipt, MasterError> {
    let row = tx
        .query_row(
            "SELECT c.candidate_attempt,c.feature_call,d.decision,d.decision_sha256,
                    d.lifecycle_revision
             FROM feature_review_calls c JOIN feature_review_decisions d
               ON d.review_call_id=c.review_call_id WHERE c.review_call_id=?1",
            [request.review_call_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Vec<u8>>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .optional()?
        .ok_or(MasterError::ReviewGatewayUnavailable)?;
    let plan = ReviewGatewayExecutionPlan {
        request: request.clone(),
        candidate: CandidateEvidence {
            integration_id: request.integration_id,
            artifact_set_sha256: [0; 32],
            candidate_commit: request.candidate_commit.clone(),
            candidate_tree: request.candidate_tree.clone(),
            base_commit: request.base_commit.clone(),
            artifact_ids: Vec::new(),
        },
        approved_specification: Value::Null,
        approved_specification_sha256: [0; 32],
        requirements_sha256: [0; 32],
        requirement_ids: Vec::new(),
        evidence_digests: Vec::new(),
        candidate_attempt: u8::try_from(row.0)
            .map_err(|_| MasterError::ReviewGatewayUnavailable)?,
        feature_call: u8::try_from(row.1).map_err(|_| MasterError::ReviewGatewayUnavailable)?,
    };
    match row.2.as_str() {
        "approved" => Ok(review_gateway_receipt(
            &plan,
            i64_to_u64(row.4)?,
            digest_array(&row.3)?,
            true,
        )),
        "rejected" => Ok(review_gateway_receipt(
            &plan,
            i64_to_u64(row.4)?,
            digest_array(&row.3)?,
            false,
        )),
        _ => Err(MasterError::ReviewGatewayUnavailable),
    }
}

fn load_validation_completion_tx(
    tx: &Transaction<'_>,
    validation_id: Uuid,
) -> Result<Option<(bool, [u8; 32], u64)>, MasterError> {
    tx.query_row(
        "SELECT passed,evidence_manifest_sha256,lifecycle_revision
         FROM feature_validation_completions WHERE validation_id=?1",
        [validation_id.to_string()],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Vec<u8>>(1)?,
                row.get::<_, i64>(2)?,
            ))
        },
    )
    .optional()?
    .map(|(passed, digest, lifecycle)| {
        Ok((
            parse_stored_boolean(passed, "validation completion")?,
            digest_array(&digest)?,
            i64_to_u64(lifecycle)?,
        ))
    })
    .transpose()
}

fn validation_gate_receipt(
    request: &FeatureConveyorValidationGateRequest,
    lifecycle_revision: u64,
    evidence_manifest_sha256: [u8; 32],
) -> FeatureConveyorValidationGateReceipt {
    FeatureConveyorValidationGateReceipt {
        schema_version: FEATURE_CONVEYOR_VALIDATION_GATE_SCHEMA_VERSION,
        validation_id: request.validation_id,
        feature_id: request.feature_id,
        specification_revision: request.specification_revision,
        lifecycle_revision,
        feature_lease_id: request.feature_lease_id,
        integration_id: request.integration_id,
        candidate_commit: request.candidate_commit.clone(),
        candidate_tree: request.candidate_tree.clone(),
        evidence_manifest_sha256,
        plan_sha256: request.plan_sha256,
        queue_revision: request.expected_queue_revision,
        emergency_pause_revision: request.expected_emergency_pause_revision,
        grants: request.grants,
        status: FeatureConveyorValidationGateStatus::EvidenceAccepted,
    }
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
            "acceptance": ["canonical-json-remains-bound"],
            "outcome": "canonical JSON remains bound",
            "assumptions": ["quoted value"]
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

        for sensitive_manifest in [
            json!({"acceptance": ["a"], "transcript": "owner brainstorming"}),
            json!({"acceptance": ["a"], "nested": {"api-key": "not-for-review"}}),
            json!({"acceptance": ["a"], "chat_history": ["private discussion"]}),
            json!({"acceptance": ["a"], "chatHistory": ["private discussion"]}),
            json!({"acceptance": ["a"], "messages": [{"role": "owner"}]}),
            json!({"acceptance": ["a"], "access_token": "opaque"}),
            json!({"acceptance": ["a"], "accessToken": "opaque"}),
            json!({"acceptance": ["a"], "token": "opaque"}),
            json!({"acceptance": ["a"], "api_token": "opaque"}),
            json!({"acceptance": ["a"], "client_token": "opaque"}),
            json!({"acceptance": ["a"], "outcome": "safe", "meeting_notes": [{"speaker": "owner", "text": "private discussion"}]}),
            json!({"acceptance": ["a"], "notes": "-----BEGIN PRIVATE KEY-----"}),
            json!({"acceptance": ["a"], "endpoint": "https://owner:password@example.invalid/path"}),
        ] {
            let mut invalid = valid.clone();
            invalid.manifest = sensitive_manifest;
            invalid.manifest_sha256 =
                Sha256::digest(canonical_json(&invalid.manifest).unwrap().as_bytes()).into();
            assert_invalid_specification(&invalid);
        }

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
    fn feature_conveyor_unit_specification_rejects_embedded_secret_shapes() {
        let valid = valid_specification();
        let forbidden = [
            "prefix -----BEGIN PRIVATE KEY----- suffix",
            "authorization: bEaReR abcdefghijklmnop",
            "authorization: BaSiC dXNlcjpwYXNzd29yZA==",
            "embedded ghp_123456789012345678901234567890123456 token",
            "embedded github_pat_11AA22BB33CC44DD55EE66FF token",
            "embedded sk-1234567890abcdefg secret",
            "access key AKIA1234567890ABCDEF12 here",
            "jwt eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0In0.signature1234 here",
            "endpoint https://owner:password@example.invalid/path",
        ];
        for disclosure in forbidden {
            let mut invalid = valid.clone();
            invalid.manifest = json!({"acceptance": ["a"], "outcome": disclosure});
            invalid.manifest_sha256 =
                Sha256::digest(canonical_json(&invalid.manifest).unwrap().as_bytes()).into();
            assert_invalid_specification(&invalid);
        }
    }

    #[test]
    fn feature_conveyor_unit_specification_allows_noncredential_prose() {
        let valid = valid_specification();
        for disclosure in [
            "Use token-free authorization metadata",
            "The basic-authentication lane remains disabled",
            "The bearer-token header must be redacted",
            "Reject the literal short example sk-example",
            "Reject AWS access-key identifiers without including one",
            "Semantic version 1.2.3 is not a JWT",
            "Browse https://example.invalid/path for public documentation",
            "A GitHub personal access token is prohibited",
        ] {
            let mut allowed = valid.clone();
            allowed.manifest = json!({"acceptance": ["a"], "outcome": disclosure});
            allowed.manifest_sha256 =
                Sha256::digest(canonical_json(&allowed.manifest).unwrap().as_bytes()).into();
            assert!(
                validate_approved_specification(&allowed).is_ok(),
                "{disclosure}"
            );
        }
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
