use anyhow::{bail, Context};
use assemblywright_master::validation_containment::{
    run_internal_validation_check, run_validation_command, validation_command_execution,
    ValidationCancellation, ValidationCommandExecution, ValidationToolchainConfig,
    VerifiedValidationCopy,
};
use assemblywright_master::{
    current_time_ms, execute_github_publication_live_proof, execute_review_provider_live_proof,
    invoke_review_provider, prepare_review_provider_call, AcceptedCancellation, AcceptedResult,
    ApprovedFeatureSpecification, ArtifactIntegrationAuthorization, ArtifactIntegrationError,
    AssemblyLineEffectDispatcher, BrainstormingCloudAuthorization, BrainstormingDraft,
    CapabilityRebindAcknowledgement, DeviceRegistration, EnrollmentGrantSpec, EnrollmentRequest,
    EphemeralServerIdentity, FeatureAbandonmentEvidence, FeatureConveyorStatus,
    FeatureGrantRevisions, FeatureSnapshotClaimPlan, IdentityAuthority, IssuedDeviceCertificate,
    MasterError, MasterHealthSnapshot, MasterKernel, MasterProcess, NewStep, PlanningEffectControl,
    PlanningRuntime, PlanningRuntimeStatus, PlatformSecretProtector, ProcessGithubPublication,
    ProcessReviewProvider, PublicationAdapter, PublicationExecutionControl, RemoteWorkContract,
    RepositoryGrantKind, RepositoryGrantRevision, RepositorySnapshotEvidence,
    RepositorySnapshotStore, ResultArtifactReference, ReviewGatewayAuthorization, ReviewProvider,
    ReviewProviderInvocationError, ReviewTransportFailure, StartupReconciliation,
    UnavailableAssemblyLineEffectDispatcher, UnavailableReviewProvider, ValidationCommandEvidence,
    ValidationGateAuthorization, ValidationGateEvidence, ValidationGateExecutionPlan,
};
use assemblywright_protocol::{
    feature_conveyor_provider_binding_sha256, repository_preflight_fingerprint_sha256,
    AssemblyLineAutoRunReceipt, AssemblyLineAutoRunRequest, AssemblyLineEmergencyPauseRequest,
    AssemblyLineLifecycleState, AssemblyLineOwnerProjection, AssemblyLineStartRequest,
    AssemblyLineStopRequest, AuthenticatedHandshakeRequest, BrainstormingOwnerApprovalBinding,
    CancellationAcknowledgement, CancellationPollRequest, CapabilityDescriptor, DeviceId,
    DeviceRole, DistributedEventBatch, DistributedEventBatchRequest, EnrollmentCsrReply,
    EnrollmentInvitation, FeatureBrainstormingCloudRequest, FeatureBrainstormingDraft,
    FeatureConveyorAbandonAndAdvanceReceipt, FeatureConveyorAbandonAndAdvanceRequest,
    FeatureConveyorAbandonAndAdvanceStatus, FeatureConveyorActivationEvidenceAdmissionProjection,
    FeatureConveyorActivationEvidenceAdmissionReceipt,
    FeatureConveyorActivationEvidenceAdmissionRequest, FeatureConveyorActivationReceipt,
    FeatureConveyorActivationRequest, FeatureConveyorApprovedFeatureReceipt,
    FeatureConveyorApprovedFeatureRequest, FeatureConveyorApprovedFeatureStatus,
    FeatureConveyorArtifactIntegrationPlan, FeatureConveyorArtifactIntegrationRequest,
    FeatureConveyorCancelActiveFeatureReceipt, FeatureConveyorCancelActiveFeatureRequest,
    FeatureConveyorCancelActiveFeatureStatus, FeatureConveyorCodingDispatchReceipt,
    FeatureConveyorCodingDispatchRequest, FeatureConveyorOwnerBridgeDesignationReceipt,
    FeatureConveyorOwnerBridgeDesignationRequest, FeatureConveyorOwnerBridgeDesignationStatus,
    FeatureConveyorOwnerControlProjection, FeatureConveyorOwnerOrchestrationControlReceipt,
    FeatureConveyorOwnerOrchestrationControlRequest, FeatureConveyorPublicationRequest,
    FeatureConveyorRemoteAbandonAndAdvanceRequest, FeatureConveyorRemoteCancelActiveFeatureRequest,
    FeatureConveyorRepositoryGrantKind, FeatureConveyorRepositoryGrantReceipt,
    FeatureConveyorRepositoryGrantRequest, FeatureConveyorRepositoryGrantSet,
    FeatureConveyorRepositoryGrantStatus, FeatureConveyorRepositoryPreflightReceipt,
    FeatureConveyorRepositoryPreflightRequest, FeatureConveyorRepositoryPreflightStatus,
    FeatureConveyorRepositorySnapshotClaimReceipt, FeatureConveyorRepositorySnapshotClaimRequest,
    FeatureConveyorRepositorySnapshotClaimStatus, FeatureConveyorReviewGatewayRequest,
    FeatureConveyorReviewPacket, FeatureConveyorValidationGateRequest, FeatureQueueEntryProjection,
    FixtureJobResult, FrozenBrainstormingSpecification, HandshakeRequest, HandshakeResponse,
    HandshakeStatus, JobEnvelope, JobResultEnvelope, JobResultStatus, LocalCodingJobResult,
    LocalCodingResultArtifactAdmission, LocalCodingResultArtifactReceipt, LocalCodingSnapshotChunk,
    LocalCodingSnapshotChunkRequest, LocalModelSelectionProjection, LocalModelSelectionReceipt,
    LocalModelSelectionRequest, ProjectBrainstormingCloudRequest, ProjectBrainstormingDraft,
    RepositoryCreationProjection, Sensitivity, StepId, TaskId, ENROLLMENT_INVITATION_READY_STATUS,
    ENROLLMENT_PAIRING_SCHEMA_VERSION, FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
    MAX_ASSEMBLY_LINE_OWNER_PROJECTION_BYTES, MAX_ENROLLMENT_PAIRING_FRAME_BYTES,
    MAX_FEATURE_CONVEYOR_CODING_DISPATCH_REQUEST_BYTES,
    MAX_FEATURE_CONVEYOR_OWNER_ACTIVATION_FRAME_BYTES,
    MAX_FEATURE_CONVEYOR_OWNER_RESOLUTION_REQUEST_BYTES,
    MAX_FEATURE_CONVEYOR_REPOSITORY_PREFLIGHT_REQUEST_BYTES,
    MAX_FEATURE_CONVEYOR_SNAPSHOT_CLAIM_REQUEST_BYTES, MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
    MAX_LOCAL_CODING_SNAPSHOT_CHUNK_BYTES, MAX_LOCAL_MODEL_SELECTION_FRAME_BYTES,
    MAX_WIRE_FRAME_BYTES, PROTOCOL_VERSION,
};
#[cfg(test)]
use assemblywright_protocol::{
    CapabilityKind, ExecutionDescendantScope, ExecutionHostPlatform, ExecutionTerminationMode,
    ExecutionTerminationOutcome, FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
};
use axum::body::Bytes;
use axum::extract::rejection::BytesRejection;
use axum::extract::{DefaultBodyLimit, Path as AxumPath, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use clap::{Parser, Subcommand, ValueEnum};
use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use hyper_util::service::TowerToHyperService;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::json;
#[cfg(any(windows, test))]
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;
use std::time::Instant;
use tokio_rustls::TlsAcceptor;
use tracing::info;
use uuid::Uuid;
use x509_parser::extensions::GeneralName;
use x509_parser::prelude::FromDer;
use zeroize::{Zeroize, Zeroizing};

#[cfg(windows)]
mod windows_service_host;

const DEFAULT_BIND: &str = "127.0.0.1:7791";
const TLS_EXPORTER_LABEL: &[u8] = b"EXPORTER-Assemblywright-Developer-Mode-v1";
const TLS_EXPORTER_BYTES: usize = 32;
const DEVELOPMENT_TOKEN_FILE: &str = "development.token";
const MIN_TOKEN_BYTES: usize = 32;
const MAX_TOKEN_BYTES: usize = 256;
const DEFAULT_SERVICE_NAME: &str = "AssemblywrightMaster";
const MASTER_STATE_NAMESPACE: &str = "Assemblywright";
/// Pre-rename state namespace, read once by `adopt_legacy_master_state` so an
/// already-enrolled host keeps its durable kernel. Never written.
const LEGACY_MASTER_STATE_NAMESPACE: &str = "Jarvis";
const MAINTENANCE_MARKER_FILE: &str = "maintenance-mode.json";
const ROTATION_RECOVERY_DIRECTORY: &str = "rotation-recovery-v1";
const MAX_ROTATION_RECOVERY_RECEIPT_BYTES: usize = 64 * 1_024;
const REPOSITORY_FILESYSTEM_OBSERVATION_TIMEOUT: Duration = Duration::from_secs(5);
// This bounds API wait, not OS-thread execution: spawn_blocking cannot be
// forcibly cancelled safely. The singleton reservation remains owned by a
// timed-out task until it exits and drops any staged snapshot.
const REPOSITORY_SNAPSHOT_CLAIM_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Parser)]
#[command(
    name = "assemblywright-master",
    version,
    about = "Headless Windows master foundation for Assemblywright Developer Mode"
)]
struct Cli {
    /// Master state directory. Defaults to %LOCALAPPDATA%\Assemblywright\master on Windows.
    #[arg(long, env = "ASSEMBLYWRIGHT_MASTER_DATA_DIR", global = true)]
    data_dir: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Initialize the durable master database and local development token.
    Setup,
    /// Run the single-owner master process on an authenticated loopback socket.
    Serve {
        #[arg(long, default_value = DEFAULT_BIND)]
        bind: SocketAddr,
        /// Optional concrete IP endpoint for TLS 1.3 enrolled-device traffic.
        #[arg(long)]
        remote_bind: Option<SocketAddr>,
    },
    /// Query the running master health boundary.
    Health {
        #[arg(long, default_value = DEFAULT_BIND)]
        endpoint: SocketAddr,
    },
    /// Complete one bounded fake inference job through the cross-process development boundary.
    FixtureWorker {
        #[arg(long, default_value = DEFAULT_BIND)]
        endpoint: SocketAddr,
        #[arg(long, default_value = "prove the Windows master process boundary")]
        prompt: String,
    },
    /// Run the fixed selected-provider approval/rejection live proof without queue mutation.
    ReviewProviderProof {
        #[arg(long)]
        confirm: bool,
    },
    /// Run the fixed protected-GitHub publication live proof without queue/database mutation.
    GithubPublicationProof {
        #[arg(long)]
        confirm: bool,
        /// Exact authenticated main commit from the clean published Windows checkout.
        #[arg(long)]
        expected_source_head: String,
    },
    /// Validate the provisioned capability-separated planning runtime without invoking providers.
    PlanningRuntimeCheck {
        #[arg(long)]
        confirm: bool,
    },
    /// Run the fixed real-Codex probe inside the exact stopped production planning boundary.
    PlanningProviderNativeProbe {
        #[arg(long, default_value = DEFAULT_SERVICE_NAME)]
        service_name: String,
        #[arg(long)]
        confirm: bool,
    },
    /// Manage the Windows enrollment identity and short-lived device grants.
    Enrollment {
        #[command(subcommand)]
        command: EnrollmentCommand,
    },
    /// Install and operate the Windows Service Control Manager host.
    Service {
        #[command(subcommand)]
        command: ServiceCommand,
    },
    /// Internal Service Control Manager entry point. Do not invoke directly.
    #[command(hide = true)]
    ServiceRun {
        #[arg(long, default_value = DEFAULT_SERVICE_NAME)]
        service_name: String,
        #[arg(long, default_value = DEFAULT_BIND)]
        bind: SocketAddr,
        #[arg(long)]
        remote_bind: Option<SocketAddr>,
        #[arg(long)]
        service_identity: String,
    },
}

#[derive(Debug, Subcommand)]
enum ServiceCommand {
    /// Install an automatic-start Windows service with bounded crash recovery.
    Install {
        #[arg(long, default_value = DEFAULT_SERVICE_NAME)]
        service_name: String,
        #[arg(long, default_value = DEFAULT_BIND)]
        bind: SocketAddr,
        #[arg(long)]
        remote_bind: Option<SocketAddr>,
        #[arg(long, value_enum, default_value_t = CliServiceIdentity::OwnerAccount)]
        identity: CliServiceIdentity,
        /// Read owner-account name and password from one bounded JSON document on stdin.
        #[arg(long)]
        credentials_stdin: bool,
        #[arg(long)]
        confirm: bool,
    },
    /// Start the installed service and wait for its SCM state to settle.
    Start {
        #[arg(long, default_value = DEFAULT_SERVICE_NAME)]
        service_name: String,
    },
    /// Stop the installed service gracefully and wait for it to stop.
    Stop {
        #[arg(long, default_value = DEFAULT_SERVICE_NAME)]
        service_name: String,
    },
    /// Inspect SCM state, configured identity, runtime health, and maintenance state.
    Status {
        #[arg(long, default_value = DEFAULT_SERVICE_NAME)]
        service_name: String,
        #[arg(long, default_value = DEFAULT_BIND)]
        endpoint: SocketAddr,
    },
    /// Enter durable maintenance mode and block new enqueue and lease work.
    MaintenanceEnter {
        #[arg(long, default_value = DEFAULT_SERVICE_NAME)]
        service_name: String,
        #[arg(long, value_enum)]
        reason: CliMaintenanceReason,
        #[arg(long)]
        confirm: bool,
    },
    /// Exit durable maintenance mode after operator validation.
    MaintenanceExit {
        #[arg(long, default_value = DEFAULT_SERVICE_NAME)]
        service_name: String,
        #[arg(long)]
        confirm: bool,
    },
    /// Restart the service through durable startup reconciliation and verify health.
    Recover {
        #[arg(long, default_value = DEFAULT_SERVICE_NAME)]
        service_name: String,
        #[arg(long, default_value = DEFAULT_BIND)]
        endpoint: SocketAddr,
        #[arg(long)]
        confirm: bool,
    },
    /// Stop and remove the installed service registration without deleting master data.
    Uninstall {
        #[arg(long, default_value = DEFAULT_SERVICE_NAME)]
        service_name: String,
        #[arg(long)]
        confirm: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliServiceIdentity {
    OwnerAccount,
    LocalSystem,
}

#[derive(Debug, Clone, Copy, ValueEnum, Serialize)]
#[serde(rename_all = "snake_case")]
enum CliMaintenanceReason {
    OperatorRequest,
    Upgrade,
    Restore,
    Recovery,
}

impl CliMaintenanceReason {
    #[cfg(windows)]
    fn as_str(self) -> &'static str {
        match self {
            Self::OperatorRequest => "operator_request",
            Self::Upgrade => "upgrade",
            Self::Restore => "restore",
            Self::Recovery => "recovery",
        }
    }
}

#[cfg(windows)]
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ServiceCredentialsDocument {
    account_name: String,
    password: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MaintenanceMarker {
    schema_version: u16,
    active: bool,
    reason: String,
}

#[derive(Debug, Subcommand)]
enum EnrollmentCommand {
    /// Initialize or verify the DPAPI-protected master enrollment CA.
    Initialize,
    /// Create one ten-minute, single-use enrollment grant.
    Grant {
        #[arg(long)]
        device_name: String,
        #[arg(long, value_enum)]
        role: CliDeviceRole,
        /// JSON file containing an array of capability descriptors.
        #[arg(long)]
        capabilities_file: PathBuf,
        /// Confirm this local operator enrollment action.
        #[arg(long)]
        confirm: bool,
    },
    /// Pair one Mac without exposing the short-lived enrollment secret.
    Pair {
        #[arg(long)]
        device_name: String,
        #[arg(long, value_enum)]
        role: CliDeviceRole,
        /// JSON file containing an array of capability descriptors.
        #[arg(long)]
        capabilities_file: PathBuf,
        /// Concrete local or private-overlay IP endpoint used by remote mTLS.
        #[arg(long)]
        master_endpoint: SocketAddr,
        /// Confirm this local operator enrollment action.
        #[arg(long)]
        confirm: bool,
    },
    /// Stage a same-device capability replacement without disabling its active certificate.
    RebindPair {
        #[arg(long)]
        device_id: Uuid,
        /// JSON file containing the exact singleton mlx.reasoning descriptor.
        #[arg(long)]
        capabilities_file: PathBuf,
        /// Existing concrete local or private-overlay master endpoint.
        #[arg(long)]
        master_endpoint: SocketAddr,
        /// Confirm this local operator capability-rebind staging action.
        #[arg(long)]
        confirm: bool,
    },
    /// Rotate one standard MacBridge certificate without changing its registration.
    RotatePair {
        #[arg(long)]
        device_id: Uuid,
        /// Existing concrete local or private-overlay master endpoint.
        #[arg(long)]
        master_endpoint: SocketAddr,
        /// Confirm this local operator certificate-rotation action.
        #[arg(long)]
        confirm: bool,
    },
    /// Re-emit one exact committed rotation receipt from its owner-private journal.
    RotateRecover {
        #[arg(long)]
        grant_id: Uuid,
        /// Confirm this local operator recovery action.
        #[arg(long)]
        confirm: bool,
    },
    /// Acknowledge delivery and remove one exact committed rotation recovery journal.
    RotateRecoverAcknowledge {
        #[arg(long)]
        grant_id: Uuid,
        /// Confirm removal of the exact validated recovery journal.
        #[arg(long)]
        confirm: bool,
    },
    /// Atomically activate a Mac-staged capability replacement acknowledgement from stdin.
    RebindActivate {
        /// Required acknowledgement that the public staged-certificate binding is on stdin.
        #[arg(long)]
        acknowledgement_stdin: bool,
        /// Confirm this local operator activation action.
        #[arg(long)]
        confirm: bool,
    },
    /// Abort an unactivated capability replacement without changing the active identity.
    RebindAbort {
        #[arg(long)]
        grant_id: Uuid,
        /// Confirm this local operator abort action.
        #[arg(long)]
        confirm: bool,
    },
    /// Create a key-rotation grant for an existing non-revoked device.
    RotateGrant {
        #[arg(long)]
        device_id: Uuid,
        /// Confirm this local operator rotation action.
        #[arg(long)]
        confirm: bool,
    },
    /// Verify a CSR and consume one grant from a bounded JSON document on stdin.
    Issue {
        /// Required acknowledgement that the secret-bearing request is supplied on stdin.
        #[arg(long)]
        request_stdin: bool,
    },
    /// Revoke a device and all active certificates immediately.
    Revoke {
        #[arg(long)]
        device_id: Uuid,
        #[arg(long)]
        reason: String,
        /// Confirm this local operator revocation action.
        #[arg(long)]
        confirm: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliDeviceRole {
    MacBridge,
    InferenceWorker,
}

impl From<CliDeviceRole> for DeviceRole {
    fn from(value: CliDeviceRole) -> Self {
        match value {
            CliDeviceRole::MacBridge => Self::MacBridge,
            CliDeviceRole::InferenceWorker => Self::InferenceWorker,
        }
    }
}

#[derive(Clone)]
struct AppState {
    process: Arc<Mutex<MasterProcess>>,
    token_sha256: [u8; 32],
    started_at_ms: u64,
    lifecycle: RuntimeLifecycle,
    repository_snapshot_claim_reservation: Arc<tokio::sync::Mutex<()>>,
    artifact_integration_reservation: Arc<tokio::sync::Mutex<()>>,
    validation_gate_reservation: Arc<tokio::sync::Mutex<()>>,
    review_gateway_reservation: Arc<tokio::sync::Mutex<()>>,
    publication_reservation: Arc<tokio::sync::Mutex<()>>,
    review_provider: Arc<dyn ReviewProvider>,
    github_publication: Option<Arc<ProcessGithubPublication>>,
    planning_runtime: Option<Arc<Mutex<PlanningRuntime>>>,
    planning_runtime_status: Option<PlanningRuntimeStatus>,
    planning_database_path: PathBuf,
    active_planning_calls: Arc<Mutex<Vec<Weak<AtomicBool>>>>,
    assembly_line_effect_dispatcher: Arc<dyn AssemblyLineEffectDispatcher>,
    validation_runtime: ValidationRuntime,
}

#[derive(Clone)]
#[cfg_attr(not(windows), allow(dead_code))]
enum ValidationRuntime {
    Disabled,
    Ready(ValidationToolchainConfig),
}

impl ValidationRuntime {
    fn load(data_dir: &Path) -> anyhow::Result<Self> {
        #[cfg(windows)]
        {
            let root = data_dir.join("validation-runner");
            let toolchain = root.join("toolchain");
            let cache = root.join("dependency-cache-seed");
            if !toolchain.exists() && !cache.exists() {
                return Ok(Self::Disabled);
            }
            if !toolchain.exists() || !cache.exists() {
                bail!("validation runner provisioning is incomplete");
            }
            ValidationToolchainConfig::resolve(&toolchain, &cache)
                .map(Self::Ready)
                .map_err(|_| anyhow::anyhow!("validation runner provisioning is invalid"))
        }
        #[cfg(not(windows))]
        {
            let _ = data_dir;
            Ok(Self::Disabled)
        }
    }

    fn toolchain(&self) -> Option<&ValidationToolchainConfig> {
        match self {
            Self::Disabled => None,
            Self::Ready(toolchain) => Some(toolchain),
        }
    }
}

#[derive(Clone)]
struct RuntimeLifecycle {
    host_mode: String,
    service_identity: String,
    maintenance_active: Arc<AtomicBool>,
    maintenance_reason: Arc<Mutex<Option<String>>>,
}

type ReadyCallback =
    Box<dyn FnOnce(SocketAddr, Option<SocketAddr>, &RuntimeLifecycle) -> anyhow::Result<()> + Send>;

#[derive(Clone)]
struct RemoteSession {
    registration: DeviceRegistration,
    certificate_serial_hex: String,
    certificate_sha256: [u8; 32],
    tls_exporter_sha256: [u8; 32],
    accepted_epoch: Arc<Mutex<Option<u64>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HealthResponse {
    status: String,
    mode: String,
    host_mode: String,
    service_identity: String,
    maintenance_active: bool,
    maintenance_reason: Option<String>,
    emergency_paused: bool,
    protocol_version: u16,
    schema_version: i64,
    process_id: u32,
    started_at_ms: u64,
    startup_reconciliation: StartupReconciliation,
    state: MasterHealthSnapshot,
    boundary: String,
}

#[derive(Debug, Serialize)]
struct SetupReceipt {
    status: &'static str,
    protocol_version: u16,
    schema_version: i64,
    data_dir: PathBuf,
    database_path: PathBuf,
    development_token_file: PathBuf,
    boundary: &'static str,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LeaseRequest {
    device_id: DeviceId,
    connection_epoch: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AcceptedResponse {
    accepted: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmergencyPauseResponse {
    emergency_paused: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EmergencyPauseActionRequest {}

#[derive(Debug, Serialize, Deserialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Serialize)]
struct FixtureReceipt {
    status: &'static str,
    task_id: TaskId,
    step_id: StepId,
    accepted_result: AcceptedResult,
}

type ApiError = (StatusCode, Json<ErrorResponse>);
type ApiResult<T> = Result<Json<T>, ApiError>;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    #[cfg(windows)]
    if let Some(exit_code) = assemblywright_master::github_publication_launcher_exit_code() {
        std::process::exit(exit_code);
    }
    #[cfg(windows)]
    if let Some(exit_code) = assemblywright_master::review_provider_launcher_exit_code() {
        std::process::exit(exit_code);
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("assemblywright_master=info")
            }),
        )
        .with_target(false)
        .init();

    let cli = Cli::parse();
    let data_dir = resolve_data_dir(cli.data_dir)?;
    match cli.command {
        Command::Setup => setup(&data_dir),
        Command::Serve { bind, remote_bind } => serve(&data_dir, bind, remote_bind).await,
        Command::Health { endpoint } => health(&data_dir, endpoint).await,
        Command::FixtureWorker { endpoint, prompt } => {
            fixture_worker(&data_dir, endpoint, prompt).await
        }
        Command::ReviewProviderProof { confirm } => {
            require_operator_confirmation(confirm, "selected review-provider live proof")?;
            let provider = ProcessReviewProvider::load(&data_dir)?
                .context("selected review provider is not provisioned")?;
            if !provider.is_pinned_codex_adapter() {
                bail!("selected review provider is not the pinned Codex adapter");
            }
            let receipt = execute_review_provider_live_proof(&provider, current_time_ms()?)
                .map_err(|_| anyhow::anyhow!("selected review-provider live proof failed"))?;
            println!("{}", serde_json::to_string(&receipt)?);
            Ok(())
        }
        Command::GithubPublicationProof {
            confirm,
            expected_source_head,
        } => {
            require_operator_confirmation(confirm, "protected GitHub publication live proof")?;
            let runtime = ProcessGithubPublication::load(&data_dir)?
                .context("GitHub publication adapter is not provisioned")?;
            let receipt = execute_github_publication_live_proof(
                &runtime,
                &expected_source_head,
                current_time_ms()?,
            )
            .map_err(|_| {
                anyhow::anyhow!(
                    "GitHub publication proof failed; credential reauthentication may be required"
                )
            })?;
            receipt
                .validate()
                .map_err(|_| anyhow::anyhow!("GitHub publication proof receipt was invalid"))?;
            println!("{}", serde_json::to_string(&receipt)?);
            Ok(())
        }
        Command::PlanningRuntimeCheck { confirm } => {
            require_operator_confirmation(confirm, "planning runtime trust-boundary check")?;
            let runtime =
                PlanningRuntime::load(&data_dir)?.context("planning runtime is not provisioned")?;
            if runtime.validated_status().is_none() {
                bail!("planning runtime trust boundary is invalid");
            }
            println!(
                "{{\"status\":\"planning_runtime_validated\",\"live_evidence_required\":true}}"
            );
            Ok(())
        }
        Command::PlanningProviderNativeProbe {
            service_name,
            confirm,
        } => {
            require_operator_confirmation(confirm, "native planning-provider containment probe")?;
            #[cfg(not(windows))]
            {
                let _ = service_name;
                bail!("native planning-provider containment probe is available only on Windows");
            }
            #[cfg(windows)]
            {
                let runtime = PlanningRuntime::load(&data_dir)?
                    .context("planning runtime is not provisioned")?;
                let receipt = runtime.run_provider_native_probe(&data_dir, &service_name)?;
                println!("{}", serde_json::to_string(&receipt)?);
                Ok(())
            }
        }
        Command::Enrollment { command } => enrollment(&data_dir, command),
        Command::Service { command } => service_command(&data_dir, command).await,
        Command::ServiceRun {
            service_name,
            bind,
            remote_bind,
            service_identity,
        } => run_service_host(data_dir, service_name, bind, remote_bind, service_identity),
    }
}

async fn service_command(data_dir: &Path, command: ServiceCommand) -> anyhow::Result<()> {
    #[cfg(not(windows))]
    {
        let _ = (data_dir, command);
        bail!("Windows service management is available only on Windows");
    }

    #[cfg(windows)]
    {
        let receipt = match command {
            ServiceCommand::Install {
                service_name,
                bind,
                remote_bind,
                identity,
                credentials_stdin,
                confirm,
            } => {
                require_operator_confirmation(confirm, "Windows service installation")?;
                require_loopback(bind)?;
                if let Some(remote_bind) = remote_bind {
                    require_concrete_remote_bind(remote_bind)?;
                }
                validate_initialized_service_data(data_dir, identity, remote_bind)?;
                match identity {
                    CliServiceIdentity::OwnerAccount => {
                        if !credentials_stdin {
                            bail!(
                                "owner-account service installation requires --credentials-stdin; passwords must not be passed in argv"
                            );
                        }
                        let mut bytes = read_bounded_stdin()?;
                        let mut credentials: ServiceCredentialsDocument =
                            match serde_json::from_slice(&bytes) {
                                Ok(credentials) => credentials,
                                Err(error) => {
                                    bytes.zeroize();
                                    return Err(error)
                                        .context("decode strict service credentials document");
                                }
                            };
                        bytes.zeroize();
                        if credentials.account_name.trim().is_empty()
                            || credentials.account_name.len() > 256
                            || credentials.password.is_empty()
                            || credentials.password.len() > 1024
                            || credentials.account_name.contains('\0')
                            || credentials.password.contains('\0')
                        {
                            credentials.password.zeroize();
                            bail!("service credentials violate the bounded input contract");
                        }
                        let result = windows_service_host::install(
                            &service_name,
                            data_dir,
                            bind,
                            remote_bind,
                            Some(&credentials.account_name),
                            Some(&credentials.password),
                        );
                        credentials.password.zeroize();
                        result?
                    }
                    CliServiceIdentity::LocalSystem => {
                        if credentials_stdin {
                            bail!("LocalSystem installation does not accept credentials stdin");
                        }
                        if remote_bind.is_some() {
                            bail!(
                                "LocalSystem service identity is loopback-only because it cannot use the owner's DPAPI-current-user enrollment authority"
                            );
                        }
                        windows_service_host::install(
                            &service_name,
                            data_dir,
                            bind,
                            None,
                            None,
                            None,
                        )?
                    }
                }
            }
            ServiceCommand::Start { service_name } => windows_service_host::start(&service_name)?,
            ServiceCommand::Stop { service_name } => windows_service_host::stop(&service_name)?,
            ServiceCommand::Status {
                service_name,
                endpoint,
            } => {
                let service = windows_service_host::status(&service_name)?;
                let runtime = fetch_health_value(data_dir, endpoint).await.ok();
                let runtime_health_available = runtime.is_some();
                json!({
                    "status": "service_status",
                    "service": service,
                    "runtime_health": runtime,
                    "runtime_health_available": runtime_health_available
                })
            }
            ServiceCommand::MaintenanceEnter {
                service_name,
                reason,
                confirm,
            } => {
                require_operator_confirmation(confirm, "enter maintenance mode")?;
                write_maintenance_marker(data_dir, reason.as_str())?;
                match windows_service_host::pause(&service_name) {
                    Ok(mut receipt) => {
                        if let Some(object) = receipt.as_object_mut() {
                            object.insert("reason".to_string(), json!(reason.as_str()));
                        }
                        receipt
                    }
                    Err(error) => {
                        let _ = clear_maintenance_marker(data_dir);
                        return Err(error);
                    }
                }
            }
            ServiceCommand::MaintenanceExit {
                service_name,
                confirm,
            } => {
                require_operator_confirmation(confirm, "exit maintenance mode")?;
                let receipt = windows_service_host::resume(&service_name)?;
                clear_maintenance_marker(data_dir)?;
                receipt
            }
            ServiceCommand::Recover {
                service_name,
                endpoint,
                confirm,
            } => {
                require_operator_confirmation(confirm, "Windows service recovery")?;
                let service = windows_service_host::recover(&service_name)?;
                let runtime =
                    wait_for_runtime_health(data_dir, endpoint, Duration::from_secs(30)).await?;
                json!({
                    "status": "service_recovered",
                    "service": service,
                    "runtime_health": runtime,
                    "maintenance_preserved": maintenance_snapshot(data_dir).0
                })
            }
            ServiceCommand::Uninstall {
                service_name,
                confirm,
            } => {
                require_operator_confirmation(confirm, "Windows service uninstallation")?;
                windows_service_host::uninstall(&service_name)?
            }
        };
        println!("{}", serde_json::to_string(&receipt)?);
        Ok(())
    }
}

fn run_service_host(
    data_dir: PathBuf,
    service_name: String,
    bind: SocketAddr,
    remote_bind: Option<SocketAddr>,
    service_identity: String,
) -> anyhow::Result<()> {
    #[cfg(not(windows))]
    {
        let _ = (data_dir, service_name, bind, remote_bind, service_identity);
        bail!("Windows service runtime is available only on Windows");
    }
    #[cfg(windows)]
    {
        windows_service_host::run_dispatcher(windows_service_host::ServiceRuntimeConfig {
            service_name,
            data_dir,
            bind,
            remote_bind,
            service_identity,
        })
    }
}

impl RuntimeLifecycle {
    fn load(data_dir: &Path, host_mode: &str, service_identity: &str) -> anyhow::Result<Self> {
        let (maintenance_active, maintenance_reason) = maintenance_snapshot(data_dir);
        Ok(Self {
            host_mode: host_mode.to_string(),
            service_identity: service_identity.to_string(),
            maintenance_active: Arc::new(AtomicBool::new(maintenance_active)),
            maintenance_reason: Arc::new(Mutex::new(maintenance_reason)),
        })
    }

    fn maintenance_snapshot(&self) -> (bool, Option<String>) {
        let active = self.maintenance_active.load(Ordering::SeqCst);
        let reason = self
            .maintenance_reason
            .lock()
            .ok()
            .and_then(|reason| reason.clone());
        (active, reason)
    }

    #[cfg(windows)]
    fn enter_maintenance(&self, data_dir: &Path) {
        let (marker_active, marker_reason) = maintenance_snapshot(data_dir);
        let reason = marker_reason.unwrap_or_else(|| "operator_request".to_string());
        if !marker_active && write_maintenance_marker(data_dir, &reason).is_err() {
            self.maintenance_active.store(true, Ordering::SeqCst);
            if let Ok(mut current) = self.maintenance_reason.lock() {
                *current = Some("persistence_error".to_string());
            }
            return;
        }
        self.maintenance_active.store(true, Ordering::SeqCst);
        if let Ok(mut current) = self.maintenance_reason.lock() {
            *current = Some(reason);
        }
    }

    #[cfg(windows)]
    fn exit_maintenance(&self, data_dir: &Path) -> anyhow::Result<()> {
        clear_maintenance_marker(data_dir)?;
        self.maintenance_active.store(false, Ordering::SeqCst);
        *self
            .maintenance_reason
            .lock()
            .map_err(|_| anyhow::anyhow!("maintenance state lock poisoned"))? = None;
        Ok(())
    }
}

fn maintenance_snapshot(data_dir: &Path) -> (bool, Option<String>) {
    let path = data_dir.join(MAINTENANCE_MARKER_FILE);
    let Ok(metadata) = fs::symlink_metadata(&path) else {
        return (false, None);
    };
    if !metadata.file_type().is_file() || metadata.len() > 4096 {
        return (true, Some("invalid_marker".to_string()));
    }
    match fs::read(&path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<MaintenanceMarker>(&bytes).ok())
    {
        Some(marker)
            if marker.schema_version == 1
                && marker.active
                && is_valid_maintenance_reason(&marker.reason) =>
        {
            (true, Some(marker.reason))
        }
        _ => (true, Some("invalid_marker".to_string())),
    }
}

#[cfg(any(windows, test))]
fn write_maintenance_marker(data_dir: &Path, reason: &str) -> anyhow::Result<()> {
    if !is_valid_maintenance_reason(reason) {
        bail!("invalid maintenance reason");
    }
    fs::create_dir_all(data_dir)?;
    let path = data_dir.join(MAINTENANCE_MARKER_FILE);
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&path)
        .with_context(|| format!("write maintenance marker {}", path.display()))?;
    serde_json::to_writer(
        &mut file,
        &MaintenanceMarker {
            schema_version: 1,
            active: true,
            reason: reason.to_string(),
        },
    )?;
    file.write_all(b"\n")?;
    file.flush()?;
    file.sync_all()?;
    Ok(())
}

#[cfg(any(windows, test))]
fn clear_maintenance_marker(data_dir: &Path) -> anyhow::Result<()> {
    let path = data_dir.join(MAINTENANCE_MARKER_FILE);
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("remove maintenance marker {}", path.display()))
        }
    }
}

fn is_valid_maintenance_reason(reason: &str) -> bool {
    matches!(
        reason,
        "operator_request" | "upgrade" | "restore" | "recovery"
    )
}

#[cfg(windows)]
fn run_windows_service_runtime(
    config: windows_service_host::ServiceRuntimeConfig,
    mut control_rx: tokio::sync::mpsc::UnboundedReceiver<
        windows_service_host::ServiceRuntimeControl,
    >,
    status_handle: windows_service::service_control_handler::ServiceStatusHandle,
) -> anyhow::Result<()> {
    use windows_service::service::{ServiceControlAccept, ServiceExitCode, ServiceState};

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("create the Assemblywright master service runtime")?;
    let lifecycle = RuntimeLifecycle::load(
        &config.data_dir,
        "windows_service",
        &config.service_identity,
    )?;
    let control_lifecycle = lifecycle.clone();
    let control_data_dir = config.data_dir.clone();
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    runtime.block_on(async move {
        tokio::spawn(async move {
            while let Some(control) = control_rx.recv().await {
                match control {
                    windows_service_host::ServiceRuntimeControl::Stop => {
                        let _ =
                            status_handle.set_service_status(windows_service_host::service_status(
                                ServiceState::StopPending,
                                ServiceControlAccept::empty(),
                                ServiceExitCode::NO_ERROR,
                                1,
                                Duration::from_secs(30),
                            ));
                        let _ = shutdown_tx.send(true);
                        break;
                    }
                    windows_service_host::ServiceRuntimeControl::EnterMaintenance => {
                        control_lifecycle.enter_maintenance(&control_data_dir);
                        let _ =
                            status_handle.set_service_status(windows_service_host::service_status(
                                ServiceState::Paused,
                                ServiceControlAccept::STOP
                                    | ServiceControlAccept::SHUTDOWN
                                    | ServiceControlAccept::PAUSE_CONTINUE,
                                ServiceExitCode::NO_ERROR,
                                0,
                                Duration::default(),
                            ));
                    }
                    windows_service_host::ServiceRuntimeControl::ExitMaintenance => {
                        if control_lifecycle
                            .exit_maintenance(&control_data_dir)
                            .is_ok()
                        {
                            let _ = status_handle.set_service_status(
                                windows_service_host::service_status(
                                    ServiceState::Running,
                                    ServiceControlAccept::STOP
                                        | ServiceControlAccept::SHUTDOWN
                                        | ServiceControlAccept::PAUSE_CONTINUE,
                                    ServiceExitCode::NO_ERROR,
                                    0,
                                    Duration::default(),
                                ),
                            );
                        }
                    }
                }
            }
        });

        let ready_status_handle = status_handle;
        let ready_callback = Box::new(
            move |_local_addr: SocketAddr,
                  _remote_addr: Option<SocketAddr>,
                  lifecycle: &RuntimeLifecycle| {
                let state = if lifecycle.maintenance_snapshot().0 {
                    ServiceState::Paused
                } else {
                    ServiceState::Running
                };
                ready_status_handle
                    .set_service_status(windows_service_host::service_status(
                        state,
                        ServiceControlAccept::STOP
                            | ServiceControlAccept::SHUTDOWN
                            | ServiceControlAccept::PAUSE_CONTINUE,
                        ServiceExitCode::NO_ERROR,
                        0,
                        Duration::default(),
                    ))
                    .context("report the ready Assemblywright master service state")
            },
        );
        serve_runtime(
            &config.data_dir,
            config.bind,
            config.remote_bind,
            lifecycle,
            shutdown_rx,
            Some(ready_callback),
        )
        .await
    })
}

#[cfg(windows)]
fn validate_initialized_service_data(
    data_dir: &Path,
    identity: CliServiceIdentity,
    remote_bind: Option<SocketAddr>,
) -> anyhow::Result<()> {
    let mut process = MasterProcess::acquire(data_dir)
        .context("validate exclusive access to initialized master data before installation")?;
    let token_path = process.data_dir().join(DEVELOPMENT_TOKEN_FILE);
    let _ = read_development_token(&token_path)
        .context("service installation requires prior `assemblywright-master setup`")?;
    if remote_bind.is_some() {
        if !matches!(identity, CliServiceIdentity::OwnerAccount) {
            bail!("remote mTLS requires the owner-account service identity");
        }
        let now_ms = current_time_ms()?;
        let protector = PlatformSecretProtector;
        let authority = IdentityAuthority::open_existing(process.data_dir(), &protector, now_ms)
            .context("validate owner DPAPI enrollment authority before service installation")?;
        process
            .kernel_mut()
            .record_identity_authority(authority.receipt())?;
    }
    Ok(())
}

fn resolve_data_dir(explicit: Option<PathBuf>) -> anyhow::Result<PathBuf> {
    if let Some(path) = explicit {
        return Ok(path);
    }
    let local_app_data = std::env::var_os("LOCALAPPDATA").context(
        "--data-dir or ASSEMBLYWRIGHT_MASTER_DATA_DIR is required when LOCALAPPDATA is unavailable",
    )?;
    let root = PathBuf::from(local_app_data);
    let current = root.join(MASTER_STATE_NAMESPACE).join("master");
    adopt_legacy_master_state(&root, &current)?;
    Ok(current)
}

/// Move a pre-rename `Jarvis\master` state directory to the current namespace.
///
/// The rename changed the state namespace, and this directory holds the only
/// copy of the durable kernel: `master.sqlite3`, the DPAPI-protected enrollment
/// authority, and the owner lock. Leaving it behind would silently strand an
/// enrolled host on an empty database rather than failing, so adopt it once.
///
/// This is deliberately a move and not a copy: two directories both claiming to
/// be the authority is exactly the ambiguity the safety rules say to refuse. If
/// both exist the legacy one is left untouched and the current one wins, because
/// guessing which is authoritative is not safe.
fn adopt_legacy_master_state(root: &Path, current: &Path) -> anyhow::Result<()> {
    if current.exists() {
        return Ok(());
    }
    let legacy = root.join(LEGACY_MASTER_STATE_NAMESPACE).join("master");
    if !legacy.is_dir() {
        return Ok(());
    }
    let parent = current
        .parent()
        .context("resolve the master state namespace directory")?;
    std::fs::create_dir_all(parent)
        .with_context(|| format!("create the master state namespace at {}", parent.display()))?;
    std::fs::rename(&legacy, current).with_context(|| {
        format!(
            "adopt the pre-rename master state directory {} as {}; move it by hand if the \
             volume differs",
            legacy.display(),
            current.display()
        )
    })?;
    Ok(())
}

fn setup(data_dir: &Path) -> anyhow::Result<()> {
    let process = MasterProcess::acquire(data_dir)?;
    let token_path = process.data_dir().join(DEVELOPMENT_TOKEN_FILE);
    ensure_development_token(&token_path)?;
    let receipt = SetupReceipt {
        status: "setup_complete",
        protocol_version: PROTOCOL_VERSION,
        schema_version: process.kernel().schema_version()?,
        data_dir: process.data_dir().to_path_buf(),
        database_path: process.database_path().to_path_buf(),
        development_token_file: token_path,
        boundary:
            "loopback development transport is default; optional remote TLS 1.3 mTLS requires an initialized enrollment authority and explicit --remote-bind",
    };
    println!("{}", serde_json::to_string(&receipt)?);
    Ok(())
}

fn enrollment(data_dir: &Path, command: EnrollmentCommand) -> anyhow::Result<()> {
    let command = match command {
        EnrollmentCommand::Pair {
            device_name,
            role,
            capabilities_file,
            master_endpoint,
            confirm,
        } => {
            return enrollment_pair(
                data_dir,
                device_name,
                role,
                capabilities_file,
                master_endpoint,
                confirm,
            );
        }
        EnrollmentCommand::RebindPair {
            device_id,
            capabilities_file,
            master_endpoint,
            confirm,
        } => {
            return enrollment_rebind_pair(
                data_dir,
                DeviceId::new(device_id),
                capabilities_file,
                master_endpoint,
                confirm,
            );
        }
        EnrollmentCommand::RotatePair {
            device_id,
            master_endpoint,
            confirm,
        } => {
            return enrollment_rotate_pair(
                data_dir,
                DeviceId::new(device_id),
                master_endpoint,
                confirm,
            );
        }
        EnrollmentCommand::RotateRecover { grant_id, confirm } => {
            return enrollment_rotate_recover(data_dir, grant_id, confirm);
        }
        EnrollmentCommand::RotateRecoverAcknowledge { grant_id, confirm } => {
            return enrollment_rotate_recover_acknowledge(data_dir, grant_id, confirm);
        }
        command => command,
    };
    let now_ms = current_time_ms()?;
    let mut process = MasterProcess::acquire(data_dir)?;
    let protector = PlatformSecretProtector;
    let authority = if process.kernel().identity_authority_recorded()? {
        IdentityAuthority::open_existing(process.data_dir(), &protector, now_ms)?
    } else {
        IdentityAuthority::open_or_initialize(process.data_dir(), &protector, now_ms)?
    };
    process
        .kernel_mut()
        .record_identity_authority(authority.receipt())?;

    match command {
        EnrollmentCommand::Initialize => {
            println!("{}", serde_json::to_string(authority.receipt())?);
        }
        EnrollmentCommand::Grant {
            device_name,
            role,
            capabilities_file,
            confirm,
        } => {
            require_operator_confirmation(confirm, "device enrollment grant")?;
            let capabilities_bytes = read_bounded_file(&capabilities_file)?;
            let capabilities: Vec<CapabilityDescriptor> =
                serde_json::from_slice(&capabilities_bytes).with_context(|| {
                    format!(
                        "decode capability array from {}",
                        capabilities_file.display()
                    )
                })?;
            let receipt = process.kernel_mut().create_enrollment_grant(
                EnrollmentGrantSpec {
                    device_name,
                    role: role.into(),
                    capabilities,
                },
                now_ms,
            )?;
            println!("{}", serde_json::to_string(&receipt)?);
        }
        EnrollmentCommand::RotateGrant { device_id, confirm } => {
            require_operator_confirmation(confirm, "device certificate rotation grant")?;
            let receipt = process
                .kernel_mut()
                .create_rotation_grant(DeviceId::new(device_id), now_ms)?;
            println!("{}", serde_json::to_string(&receipt)?);
        }
        EnrollmentCommand::Issue { request_stdin } => {
            if !request_stdin {
                bail!(
                    "enrollment issue requires --request-stdin; grant secrets must not be passed in argv"
                );
            }
            let request_bytes = read_bounded_stdin()?;
            let request: EnrollmentRequest = serde_json::from_slice(&request_bytes)
                .context("decode strict enrollment request from stdin")?;
            let certificate = process
                .kernel_mut()
                .issue_device_certificate(&authority, &request, now_ms)?;
            println!("{}", serde_json::to_string(&certificate)?);
        }
        EnrollmentCommand::RebindActivate {
            acknowledgement_stdin,
            confirm,
        } => {
            require_operator_confirmation(confirm, "capability rebind activation")?;
            if !acknowledgement_stdin {
                bail!("rebind activation requires --acknowledgement-stdin");
            }
            let bytes = read_bounded_stdin()?;
            let acknowledgement: CapabilityRebindAcknowledgement =
                serde_json::from_slice(&bytes)
                    .context("decode strict capability rebind acknowledgement from stdin")?;
            let activation = process.kernel_mut().activate_capability_rebind(
                &authority,
                &acknowledgement,
                now_ms,
            )?;
            println!("{}", serde_json::to_string(&activation)?);
        }
        EnrollmentCommand::RebindAbort { grant_id, confirm } => {
            require_operator_confirmation(confirm, "capability rebind abort")?;
            process
                .kernel_mut()
                .abort_capability_rebind(grant_id, now_ms)?;
            println!(
                "{}",
                serde_json::to_string(&json!({
                    "status": "capability_rebind_aborted",
                    "grant_id": grant_id,
                    "aborted_at_ms": now_ms,
                }))?
            );
        }
        EnrollmentCommand::Pair { .. }
        | EnrollmentCommand::RebindPair { .. }
        | EnrollmentCommand::RotatePair { .. }
        | EnrollmentCommand::RotateRecover { .. }
        | EnrollmentCommand::RotateRecoverAcknowledge { .. } => {
            unreachable!("pairing commands are handled before authority acquisition")
        }
        EnrollmentCommand::Revoke {
            device_id,
            reason,
            confirm,
        } => {
            require_operator_confirmation(confirm, "device revocation")?;
            process.kernel_mut().revoke_device_with_reason(
                DeviceId::new(device_id),
                now_ms,
                &reason,
            )?;
            println!(
                "{}",
                serde_json::to_string(&json!({
                    "status": "device_revoked",
                    "device_id": device_id,
                    "revoked_at_ms": now_ms,
                    "reason": reason,
                }))?
            );
        }
    }
    Ok(())
}

fn enrollment_rotate_pair(
    data_dir: &Path,
    device_id: DeviceId,
    master_endpoint: SocketAddr,
    confirm: bool,
) -> anyhow::Result<()> {
    require_operator_confirmation(confirm, "same-device Mac certificate rotation")?;
    require_concrete_remote_bind(master_endpoint)
        .context("validate the existing master endpoint before certificate rotation")?;

    // Holding the single-owner process lease proves the durable Windows authority
    // is stopped for the entire grant, invitation, CSR, and issuance ceremony.
    let started_at_ms = current_time_ms()?;
    let mut process = MasterProcess::acquire(data_dir)
        .context("acquire the stopped Windows master authority for certificate rotation")?;
    let protector = PlatformSecretProtector;
    let authority = IdentityAuthority::open_existing(process.data_dir(), &protector, started_at_ms)
        .context("open the existing Windows enrollment authority for certificate rotation")?;
    process
        .kernel_mut()
        .record_identity_authority(authority.receipt())?;

    let (mut grant, registration) = process
        .kernel_mut()
        .create_rotation_pairing_grant(device_id, started_at_ms)
        .context("validate the current non-revoked device registration for rotation")?;
    let mut grant_secret = Zeroizing::new(std::mem::take(&mut grant.grant_secret));
    let invitation = EnrollmentInvitation {
        schema_version: ENROLLMENT_PAIRING_SCHEMA_VERSION,
        status: ENROLLMENT_INVITATION_READY_STATUS.to_string(),
        grant_id: grant.grant_id,
        device_id: registration.device_id,
        device_name: registration.device_name,
        role: registration.role,
        registry_revision: registration.registry_revision,
        expires_at_ms: grant.expires_at_ms,
        capabilities: registration.capabilities,
        master_endpoint,
        ca_fingerprint_sha256: authority.receipt().ca_fingerprint_sha256.clone(),
    };
    invitation.validate_at(started_at_ms)?;

    write_json_line(std::io::stdout().lock(), &invitation)?;
    eprintln!(
        "rotation invitation ready: interruption before CSR acceptance issues no certificate; after CSR submission, a missing receipt is ambiguous and requires device-registry inspection before recovery"
    );
    let reply_bytes = read_bounded_stdin_with_limit(
        MAX_ENROLLMENT_PAIRING_FRAME_BYTES,
        "certificate rotation CSR reply",
    )?;
    let reply = EnrollmentCsrReply::decode_frame(&reply_bytes)
        .context("decode strict certificate rotation CSR reply from stdin")?;
    let issue_at_ms = current_time_ms()?;
    validate_pairing_reply(&invitation, &reply, issue_at_ms)?;

    let mut request = EnrollmentRequest {
        grant_id: reply.grant_id,
        grant_secret: std::mem::take(&mut *grant_secret),
        csr_pem: reply.csr_pem,
    };
    let recovery_path = rotation_recovery_receipt_path(process.data_dir(), request.grant_id)?;
    let certificate = process
        .kernel_mut()
        .issue_device_certificate_with_precommit(&authority, &request, issue_at_ms, |receipt| {
            write_rotation_recovery_receipt(&recovery_path, receipt)
        });
    request.grant_secret.zeroize();
    let certificate = certificate?;
    write_json_line(std::io::stdout().lock(), &certificate).context(
        "certificate rotation committed but stdout failed; preserve the Mac stage and run confirmed rotate-recover for this grant",
    )?;
    eprintln!(
        "rotation receipt delivered; after the Mac installs this exact receipt, run confirmed rotate-recover-acknowledge for the grant"
    );
    Ok(())
}

fn enrollment_rotate_recover(data_dir: &Path, grant_id: Uuid, confirm: bool) -> anyhow::Result<()> {
    require_operator_confirmation(confirm, "committed certificate rotation receipt recovery")?;
    let now_ms = current_time_ms()?;
    let mut process = MasterProcess::acquire(data_dir)
        .context("acquire the stopped Windows master authority for rotation recovery")?;
    let recovery_path = rotation_recovery_receipt_path(process.data_dir(), grant_id)?;
    let temporary_path = recovery_path.with_extension("json.tmp");
    if !recovery_path.exists() && temporary_path.exists() {
        fs::remove_file(&temporary_path).with_context(|| {
            format!(
                "remove stale precommit journal {}",
                temporary_path.display()
            )
        })?;
        bail!("rotation recovery found only a stale precommit journal; stale journal removed");
    }
    let receipt = read_rotation_recovery_receipt(&recovery_path)?;
    if receipt.grant_id != Some(grant_id) {
        let _ = fs::remove_file(&recovery_path);
        bail!("rotation recovery journal grant_id mismatch; stale journal removed");
    }
    if let Err(error) = process
        .kernel_mut()
        .validate_rotation_recovery_receipt(&receipt, now_ms)
    {
        let _ = fs::remove_file(&recovery_path);
        bail!("rotation recovery journal is not an exact committed active rotation; stale journal removed: {error}");
    }
    write_json_line(std::io::stdout().lock(), &receipt)
        .context("write exact recovered rotation receipt")?;
    Ok(())
}

fn enrollment_rotate_recover_acknowledge(
    data_dir: &Path,
    grant_id: Uuid,
    confirm: bool,
) -> anyhow::Result<()> {
    require_operator_confirmation(
        confirm,
        "acknowledge delivery of a committed certificate rotation receipt",
    )?;
    let now_ms = current_time_ms()?;
    let mut process = MasterProcess::acquire(data_dir)
        .context("acquire the stopped Windows master authority for rotation acknowledgement")?;
    let recovery_path = rotation_recovery_receipt_path(process.data_dir(), grant_id)?;
    let receipt = read_rotation_recovery_receipt(&recovery_path)?;
    if receipt.grant_id != Some(grant_id) {
        bail!("rotation recovery acknowledgement grant_id mismatch");
    }
    process
        .kernel_mut()
        .validate_rotation_recovery_receipt(&receipt, now_ms)
        .context("validate the exact committed active rotation before acknowledgement")?;
    write_json_line(
        std::io::stdout().lock(),
        &json!({
            "status": "rotation_recovery_acknowledged",
            "grant_id": grant_id,
            "device_id": receipt.device_id,
            "serial_hex": receipt.serial_hex,
        }),
    )
    .context("write rotation recovery acknowledgement before cleanup")?;
    fs::remove_file(&recovery_path).with_context(|| {
        format!(
            "remove acknowledged rotation recovery journal {}",
            recovery_path.display()
        )
    })?;
    sync_rotation_recovery_directory(
        recovery_path
            .parent()
            .expect("rotation recovery receipt has parent"),
    )?;
    Ok(())
}

fn rotation_recovery_receipt_path(data_dir: &Path, grant_id: Uuid) -> anyhow::Result<PathBuf> {
    if grant_id.is_nil() {
        bail!("rotation recovery grant_id must not be nil");
    }
    let directory = data_dir.join(ROTATION_RECOVERY_DIRECTORY);
    ensure_rotation_recovery_directory(&directory)?;
    Ok(directory.join(format!("{grant_id}.json")))
}

fn ensure_rotation_recovery_directory(directory: &Path) -> anyhow::Result<()> {
    if !directory.exists() {
        fs::create_dir(directory).with_context(|| {
            format!("create rotation recovery directory {}", directory.display())
        })?;
        restrict_rotation_recovery_directory(directory)?;
    }
    let metadata = fs::symlink_metadata(directory)?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        bail!("rotation recovery path must be an ordinary directory");
    }
    validate_rotation_recovery_not_reparse(&metadata)?;
    validate_rotation_recovery_directory_permissions(directory, &metadata)?;
    Ok(())
}

fn write_rotation_recovery_receipt(
    path: &Path,
    receipt: &IssuedDeviceCertificate,
) -> Result<(), assemblywright_master::MasterError> {
    let bytes = serde_json::to_vec(receipt)?;
    if bytes.len() > MAX_ROTATION_RECOVERY_RECEIPT_BYTES {
        return Err(assemblywright_master::MasterError::InvalidStoredState(
            "rotation recovery receipt exceeds its fixed bound".to_string(),
        ));
    }
    let temporary = path.with_extension("json.tmp");
    if path.exists() || temporary.exists() {
        return Err(assemblywright_master::MasterError::InvalidStoredState(
            "rotation recovery journal already exists".to_string(),
        ));
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    restrict_rotation_recovery_file(&temporary).map_err(|error| {
        assemblywright_master::MasterError::InvalidStoredState(error.to_string())
    })?;
    file.write_all(&bytes)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    fs::rename(&temporary, path)?;
    sync_rotation_recovery_directory(path.parent().expect("recovery receipt has parent"))?;
    Ok(())
}

fn read_rotation_recovery_receipt(path: &Path) -> anyhow::Result<IssuedDeviceCertificate> {
    let metadata = fs::symlink_metadata(path).with_context(|| {
        format!(
            "read rotation recovery journal metadata at {}",
            path.display()
        )
    })?;
    if !metadata.file_type().is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() > MAX_ROTATION_RECOVERY_RECEIPT_BYTES as u64
    {
        bail!("rotation recovery journal is not one bounded ordinary file");
    }
    validate_rotation_recovery_not_reparse(&metadata)?;
    validate_rotation_recovery_file_permissions(path, &metadata)?;
    let bytes = fs::read(path)?;
    serde_json::from_slice(&bytes).context("decode strict rotation recovery receipt")
}

#[cfg(unix)]
fn validate_rotation_recovery_file_permissions(
    _path: &Path,
    metadata: &fs::Metadata,
) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if metadata.permissions().mode() & 0o077 != 0 {
        bail!("rotation recovery journal must be owner-private");
    }
    Ok(())
}

#[cfg(all(not(unix), not(windows)))]
fn validate_rotation_recovery_file_permissions(
    _path: &Path,
    _metadata: &fs::Metadata,
) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(windows)]
fn validate_rotation_recovery_file_permissions(
    path: &Path,
    _metadata: &fs::Metadata,
) -> anyhow::Result<()> {
    validate_private_windows_rotation_acl(path)
}

#[cfg(unix)]
fn restrict_rotation_recovery_file(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(windows)]
fn validate_rotation_recovery_not_reparse(metadata: &fs::Metadata) -> anyhow::Result<()> {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
    if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        bail!("rotation recovery path must not be a Windows reparse point");
    }
    Ok(())
}

#[cfg(not(windows))]
fn validate_rotation_recovery_not_reparse(_metadata: &fs::Metadata) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(all(not(unix), not(windows)))]
fn restrict_rotation_recovery_file(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(windows)]
fn restrict_rotation_recovery_file(path: &Path) -> anyhow::Result<()> {
    restrict_private_windows_rotation_acl(path, false)
}

#[cfg(unix)]
fn restrict_rotation_recovery_directory(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(all(not(unix), not(windows)))]
fn restrict_rotation_recovery_directory(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(windows)]
fn restrict_rotation_recovery_directory(path: &Path) -> anyhow::Result<()> {
    restrict_private_windows_rotation_acl(path, true)
}

#[cfg(unix)]
fn validate_rotation_recovery_directory_permissions(
    _path: &Path,
    metadata: &fs::Metadata,
) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    if metadata.permissions().mode() & 0o077 != 0 {
        bail!("rotation recovery directory must be owner-private");
    }
    Ok(())
}

#[cfg(all(not(unix), not(windows)))]
fn validate_rotation_recovery_directory_permissions(
    _path: &Path,
    _metadata: &fs::Metadata,
) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(windows)]
fn validate_rotation_recovery_directory_permissions(
    path: &Path,
    _metadata: &fs::Metadata,
) -> anyhow::Result<()> {
    validate_private_windows_rotation_acl(path)
}

#[cfg(windows)]
fn restrict_private_windows_rotation_acl(path: &Path, directory: bool) -> anyhow::Result<()> {
    use std::mem::{size_of, zeroed};
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree};
    use windows_sys::Win32::Security::Authorization::{
        SetEntriesInAclW, SetNamedSecurityInfoW, EXPLICIT_ACCESS_W, SET_ACCESS, SE_FILE_OBJECT,
        TRUSTEE_IS_SID, TRUSTEE_IS_USER, TRUSTEE_IS_WELL_KNOWN_GROUP,
    };
    use windows_sys::Win32::Security::{
        CreateWellKnownSid, GetTokenInformation, TokenUser, WinLocalSystemSid,
        CONTAINER_INHERIT_ACE, DACL_SECURITY_INFORMATION, OBJECT_INHERIT_ACE,
        OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, SECURITY_MAX_SID_SIZE,
        TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::Storage::FileSystem::FILE_ALL_ACCESS;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut token = null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        bail!("open current-user token for rotation recovery ACL");
    }
    let result = (|| {
        let mut token_bytes = 0_u32;
        unsafe { GetTokenInformation(token, TokenUser, null_mut(), 0, &mut token_bytes) };
        if token_bytes < size_of::<TOKEN_USER>() as u32 {
            bail!("read current-user SID size for rotation recovery ACL");
        }
        let mut token_buffer = vec![0_usize; (token_bytes as usize).div_ceil(size_of::<usize>())];
        if unsafe {
            GetTokenInformation(
                token,
                TokenUser,
                token_buffer.as_mut_ptr().cast(),
                token_bytes,
                &mut token_bytes,
            )
        } == 0
        {
            bail!("read current-user SID for rotation recovery ACL");
        }
        let user = unsafe { &*(token_buffer.as_ptr().cast::<TOKEN_USER>()) };
        let mut system_sid = [0_u8; SECURITY_MAX_SID_SIZE as usize];
        let mut system_sid_bytes = SECURITY_MAX_SID_SIZE;
        if unsafe {
            CreateWellKnownSid(
                WinLocalSystemSid,
                null_mut(),
                system_sid.as_mut_ptr().cast(),
                &mut system_sid_bytes,
            )
        } == 0
        {
            bail!("create LocalSystem SID for rotation recovery ACL");
        }
        let inheritance = if directory {
            CONTAINER_INHERIT_ACE | OBJECT_INHERIT_ACE
        } else {
            0
        };
        let mut entries: [EXPLICIT_ACCESS_W; 2] = unsafe { zeroed() };
        entries[0].grfAccessPermissions = FILE_ALL_ACCESS;
        entries[0].grfAccessMode = SET_ACCESS;
        entries[0].grfInheritance = inheritance;
        entries[0].Trustee.TrusteeForm = TRUSTEE_IS_SID;
        entries[0].Trustee.TrusteeType = TRUSTEE_IS_USER;
        entries[0].Trustee.ptstrName = user.User.Sid.cast();
        entries[1].grfAccessPermissions = FILE_ALL_ACCESS;
        entries[1].grfAccessMode = SET_ACCESS;
        entries[1].grfInheritance = inheritance;
        entries[1].Trustee.TrusteeForm = TRUSTEE_IS_SID;
        entries[1].Trustee.TrusteeType = TRUSTEE_IS_WELL_KNOWN_GROUP;
        entries[1].Trustee.ptstrName = system_sid.as_mut_ptr().cast();
        let mut dacl = null_mut();
        let status = unsafe { SetEntriesInAclW(2, entries.as_ptr(), null_mut(), &mut dacl) };
        if status != 0 || dacl.is_null() {
            bail!("construct protected rotation recovery ACL");
        }
        let mut wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
        wide.push(0);
        let status = unsafe {
            SetNamedSecurityInfoW(
                wide.as_ptr(),
                SE_FILE_OBJECT,
                OWNER_SECURITY_INFORMATION
                    | DACL_SECURITY_INFORMATION
                    | PROTECTED_DACL_SECURITY_INFORMATION,
                user.User.Sid,
                null_mut(),
                dacl,
                null_mut(),
            )
        };
        unsafe { LocalFree(dacl.cast()) };
        if status != 0 {
            bail!("install protected rotation recovery ACL");
        }
        Ok(())
    })();
    unsafe { CloseHandle(token) };
    result?;
    validate_private_windows_rotation_acl(path)
}

#[cfg(windows)]
fn validate_private_windows_rotation_acl(path: &Path) -> anyhow::Result<()> {
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::{addr_of, null_mut};
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree};
    use windows_sys::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
    use windows_sys::Win32::Security::{
        AclSizeInformation, CreateWellKnownSid, EqualSid, GetAce, GetAclInformation,
        GetSecurityDescriptorControl, GetTokenInformation, TokenUser, WinLocalSystemSid,
        ACCESS_ALLOWED_ACE, ACL_SIZE_INFORMATION, DACL_SECURITY_INFORMATION, INHERITED_ACE,
        OWNER_SECURITY_INFORMATION, PSID, SECURITY_MAX_SID_SIZE, SE_DACL_PROTECTED, TOKEN_QUERY,
        TOKEN_USER,
    };
    use windows_sys::Win32::Storage::FileSystem::FILE_ALL_ACCESS;
    use windows_sys::Win32::System::SystemServices::ACCESS_ALLOWED_ACE_TYPE;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
    wide.push(0);
    let mut owner: PSID = null_mut();
    let mut dacl = null_mut();
    let mut descriptor = null_mut();
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut dacl,
            null_mut(),
            &mut descriptor,
        )
    };
    if status != 0 || owner.is_null() || dacl.is_null() || descriptor.is_null() {
        if !descriptor.is_null() {
            unsafe { LocalFree(descriptor) };
        }
        bail!("rotation recovery ACL is unavailable");
    }
    let result = (|| {
        let mut token = null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            bail!("open current-user token while validating rotation recovery ACL");
        }
        let token_result = (|| {
            let mut token_bytes = 0_u32;
            unsafe { GetTokenInformation(token, TokenUser, null_mut(), 0, &mut token_bytes) };
            if token_bytes < size_of::<TOKEN_USER>() as u32 {
                bail!("read current-user SID size while validating rotation recovery ACL");
            }
            let mut token_buffer =
                vec![0_usize; (token_bytes as usize).div_ceil(size_of::<usize>())];
            if unsafe {
                GetTokenInformation(
                    token,
                    TokenUser,
                    token_buffer.as_mut_ptr().cast(),
                    token_bytes,
                    &mut token_bytes,
                )
            } == 0
            {
                bail!("read current-user SID while validating rotation recovery ACL");
            }
            let user = unsafe { &*(token_buffer.as_ptr().cast::<TOKEN_USER>()) };
            if unsafe { EqualSid(owner, user.User.Sid) } == 0 {
                bail!("rotation recovery ACL owner is not the current Windows user");
            }
            let mut control = 0_u16;
            let mut revision = 0_u32;
            if unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } == 0
                || control & SE_DACL_PROTECTED == 0
            {
                bail!("rotation recovery DACL is not protected");
            }
            let mut acl_info: ACL_SIZE_INFORMATION = unsafe { zeroed() };
            if unsafe {
                GetAclInformation(
                    dacl,
                    (&mut acl_info as *mut ACL_SIZE_INFORMATION).cast(),
                    size_of::<ACL_SIZE_INFORMATION>() as u32,
                    AclSizeInformation,
                )
            } == 0
                || acl_info.AceCount != 2
            {
                bail!("rotation recovery DACL must contain exactly owner and LocalSystem");
            }
            let mut system_sid = [0_u8; SECURITY_MAX_SID_SIZE as usize];
            let mut system_sid_bytes = SECURITY_MAX_SID_SIZE;
            if unsafe {
                CreateWellKnownSid(
                    WinLocalSystemSid,
                    null_mut(),
                    system_sid.as_mut_ptr().cast(),
                    &mut system_sid_bytes,
                )
            } == 0
            {
                bail!("create LocalSystem SID while validating rotation recovery ACL");
            }
            let mut saw_user = false;
            let mut saw_system = false;
            for index in 0..acl_info.AceCount {
                let mut raw_ace: *mut c_void = null_mut();
                if unsafe { GetAce(dacl, index, &mut raw_ace) } == 0 || raw_ace.is_null() {
                    bail!("read rotation recovery DACL entry");
                }
                let ace = unsafe { &*(raw_ace.cast::<ACCESS_ALLOWED_ACE>()) };
                if u32::from(ace.Header.AceType) != ACCESS_ALLOWED_ACE_TYPE
                    || u32::from(ace.Header.AceFlags) & INHERITED_ACE != 0
                    || ace.Mask & FILE_ALL_ACCESS != FILE_ALL_ACCESS
                {
                    bail!("rotation recovery DACL entry is not explicit full control");
                }
                let sid = addr_of!(ace.SidStart) as PSID;
                if unsafe { EqualSid(sid, user.User.Sid) } != 0 && !saw_user {
                    saw_user = true;
                } else if unsafe { EqualSid(sid, system_sid.as_mut_ptr().cast()) } != 0
                    && !saw_system
                {
                    saw_system = true;
                } else {
                    bail!("rotation recovery DACL contains an unexpected principal");
                }
            }
            if !saw_user || !saw_system {
                bail!("rotation recovery DACL omitted owner or LocalSystem");
            }
            Ok(())
        })();
        unsafe { CloseHandle(token) };
        token_result
    })();
    unsafe { LocalFree(descriptor) };
    result
}

#[cfg(unix)]
fn sync_rotation_recovery_directory(path: &Path) -> Result<(), assemblywright_master::MasterError> {
    fs::File::open(path)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
fn sync_rotation_recovery_directory(
    _path: &Path,
) -> Result<(), assemblywright_master::MasterError> {
    Ok(())
}

fn enrollment_pair(
    data_dir: &Path,
    device_name: String,
    role: CliDeviceRole,
    capabilities_file: PathBuf,
    master_endpoint: SocketAddr,
    confirm: bool,
) -> anyhow::Result<()> {
    require_operator_confirmation(confirm, "Mac enrollment pairing")?;
    let role: DeviceRole = role.into();
    require_concrete_remote_bind(master_endpoint)
        .context("validate the advertised master endpoint before creating an enrollment grant")?;

    // Validate all operator-controlled planning input before acquiring authority
    // and creating the durable grant.
    let capabilities_bytes = read_bounded_file(&capabilities_file)?;
    let capabilities: Vec<CapabilityDescriptor> = serde_json::from_slice(&capabilities_bytes)
        .with_context(|| {
            format!(
                "decode capability array from {}",
                capabilities_file.display()
            )
        })?;
    if role == DeviceRole::InferenceWorker
        && capabilities.as_slice() != [CapabilityDescriptor::local_coding()]
    {
        bail!("inference-worker enrollment requires exact local.coding.v1 capability");
    }
    if role == DeviceRole::MacBridge
        && capabilities.iter().any(|capability| {
            capability.id == assemblywright_protocol::LOCAL_CODING_CAPABILITY_ID
                || capability.kind == assemblywright_protocol::CapabilityKind::LocalCoding
        })
    {
        bail!("mac-bridge enrollment cannot carry local.coding.v1 capability");
    }
    HandshakeRequest {
        protocol_version: PROTOCOL_VERSION,
        device_id: DeviceId::new(Uuid::from_u128(1)),
        device_name: device_name.clone(),
        role,
        registry_revision: 1,
        capabilities: capabilities.clone(),
    }
    .validate()
    .context("validate Mac enrollment pairing inputs")?;

    // The single-owner database lease also proves the service is stopped. No
    // online authority can race this interactive pairing transaction.
    let started_at_ms = current_time_ms()?;
    let mut process = MasterProcess::acquire(data_dir)
        .context("acquire the stopped Windows master authority for enrollment pairing")?;
    let protector = PlatformSecretProtector;
    let authority = if process.kernel().identity_authority_recorded()? {
        IdentityAuthority::open_existing(process.data_dir(), &protector, started_at_ms)?
    } else {
        IdentityAuthority::open_or_initialize(process.data_dir(), &protector, started_at_ms)?
    };
    process
        .kernel_mut()
        .record_identity_authority(authority.receipt())?;

    let mut grant = process.kernel_mut().create_enrollment_grant(
        EnrollmentGrantSpec {
            device_name,
            role,
            capabilities: capabilities.clone(),
        },
        started_at_ms,
    )?;
    let mut grant_secret = Zeroizing::new(std::mem::take(&mut grant.grant_secret));
    let invitation = EnrollmentInvitation {
        schema_version: ENROLLMENT_PAIRING_SCHEMA_VERSION,
        status: ENROLLMENT_INVITATION_READY_STATUS.to_string(),
        grant_id: grant.grant_id,
        device_id: grant.device_id,
        device_name: grant.device_name,
        role: grant.role,
        registry_revision: grant.registry_revision,
        expires_at_ms: grant.expires_at_ms,
        capabilities,
        master_endpoint,
        ca_fingerprint_sha256: authority.receipt().ca_fingerprint_sha256.clone(),
    };
    invitation.validate_at(started_at_ms)?;

    // stdout is the machine-readable two-document bridge. Flush the invitation
    // before blocking for exactly one bounded reply. Ctrl-C/EOF before issuance
    // drops and zeroizes the only raw secret; its digest-only row expires.
    write_json_line(std::io::stdout().lock(), &invitation)?;
    eprintln!(
        "pairing invitation ready: interruption before CSR acceptance issues no certificate; after CSR submission, a missing receipt is ambiguous and requires device-registry inspection or revocation before retry"
    );
    let reply_bytes =
        read_bounded_stdin_with_limit(MAX_ENROLLMENT_PAIRING_FRAME_BYTES, "enrollment CSR reply")?;
    let reply = EnrollmentCsrReply::decode_frame(&reply_bytes)
        .context("decode strict enrollment CSR reply from stdin")?;
    let issue_at_ms = current_time_ms()?;
    validate_pairing_reply(&invitation, &reply, issue_at_ms)?;

    let mut request = EnrollmentRequest {
        grant_id: reply.grant_id,
        grant_secret: std::mem::take(&mut *grant_secret),
        csr_pem: reply.csr_pem,
    };
    let certificate =
        process
            .kernel_mut()
            .issue_device_certificate(&authority, &request, issue_at_ms);
    request.grant_secret.zeroize();
    let certificate = certificate?;
    write_json_line(std::io::stdout().lock(), &certificate).context(
        "certificate issuance committed but the receipt could not be written; treat enrollment as ambiguous and inspect or revoke the device before retrying",
    )?;
    Ok(())
}

fn enrollment_rebind_pair(
    data_dir: &Path,
    device_id: DeviceId,
    capabilities_file: PathBuf,
    master_endpoint: SocketAddr,
    confirm: bool,
) -> anyhow::Result<()> {
    require_operator_confirmation(confirm, "Mac capability rebind staging")?;
    require_concrete_remote_bind(master_endpoint)
        .context("validate the existing master endpoint before capability rebind")?;
    let capabilities_bytes = read_bounded_file(&capabilities_file)?;
    let capabilities: Vec<CapabilityDescriptor> = serde_json::from_slice(&capabilities_bytes)
        .with_context(|| {
            format!(
                "decode capability array from {}",
                capabilities_file.display()
            )
        })?;
    let started_at_ms = current_time_ms()?;
    let mut process = MasterProcess::acquire(data_dir)
        .context("acquire the stopped Windows master authority for capability rebind")?;
    let protector = PlatformSecretProtector;
    let authority =
        IdentityAuthority::open_existing(process.data_dir(), &protector, started_at_ms)?;
    process
        .kernel_mut()
        .record_identity_authority(authority.receipt())?;
    let mut grant = process.kernel_mut().create_capability_rebind_grant(
        device_id,
        capabilities.clone(),
        started_at_ms,
    )?;
    let mut grant_secret = Zeroizing::new(std::mem::take(&mut grant.grant_secret));
    let invitation = EnrollmentInvitation {
        schema_version: ENROLLMENT_PAIRING_SCHEMA_VERSION,
        status: ENROLLMENT_INVITATION_READY_STATUS.to_string(),
        grant_id: grant.grant_id,
        device_id: grant.device_id,
        device_name: grant.device_name,
        role: grant.role,
        registry_revision: grant.registry_revision,
        expires_at_ms: grant.expires_at_ms,
        capabilities,
        master_endpoint,
        ca_fingerprint_sha256: authority.receipt().ca_fingerprint_sha256.clone(),
    };
    invitation.validate_at(started_at_ms)?;
    write_json_line(std::io::stdout().lock(), &invitation)?;
    eprintln!(
        "capability rebind invitation ready: the active registration and certificate remain unchanged until a separately confirmed rebind-activate"
    );
    let reply_bytes = read_bounded_stdin_with_limit(
        MAX_ENROLLMENT_PAIRING_FRAME_BYTES,
        "capability rebind CSR reply",
    )?;
    let reply = EnrollmentCsrReply::decode_frame(&reply_bytes)
        .context("decode strict capability rebind CSR reply from stdin")?;
    let issue_at_ms = current_time_ms()?;
    validate_pairing_reply(&invitation, &reply, issue_at_ms)?;
    let mut request = EnrollmentRequest {
        grant_id: reply.grant_id,
        grant_secret: std::mem::take(&mut *grant_secret),
        csr_pem: reply.csr_pem,
    };
    let pending =
        process
            .kernel_mut()
            .issue_pending_capability_rebind(&authority, &request, issue_at_ms);
    request.grant_secret.zeroize();
    let pending = pending?;
    write_json_line(std::io::stdout().lock(), &pending).context(
        "pending rebind certificate committed but its receipt could not be written; the active identity is unchanged and the owner must inspect or abort the pending rebind",
    )?;
    Ok(())
}

fn validate_pairing_reply(
    invitation: &EnrollmentInvitation,
    reply: &EnrollmentCsrReply,
    now_ms: u64,
) -> anyhow::Result<()> {
    invitation.validate_at(now_ms)?;
    reply.validate()?;
    if reply.grant_id != invitation.grant_id {
        bail!("enrollment CSR reply grant_id does not match the invitation");
    }
    if reply.device_id != invitation.device_id {
        bail!("enrollment CSR reply device_id does not match the invitation");
    }
    Ok(())
}

fn write_json_line(mut writer: impl Write, value: &impl Serialize) -> anyhow::Result<()> {
    serde_json::to_writer(&mut writer, value)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    Ok(())
}

fn require_operator_confirmation(confirmed: bool, action: &str) -> anyhow::Result<()> {
    if !confirmed {
        bail!("{action} requires explicit --confirm");
    }
    Ok(())
}

fn read_bounded_file(path: &Path) -> anyhow::Result<Vec<u8>> {
    let metadata = fs::metadata(path)
        .with_context(|| format!("inspect bounded input file {}", path.display()))?;
    if metadata.len() > MAX_WIRE_FRAME_BYTES as u64 {
        bail!("input file exceeds the wire-frame limit");
    }
    fs::read(path).with_context(|| format!("read bounded input file {}", path.display()))
}

fn read_bounded_stdin() -> anyhow::Result<Vec<u8>> {
    read_bounded_stdin_with_limit(MAX_WIRE_FRAME_BYTES, "stdin document")
}

fn read_bounded_stdin_with_limit(limit: usize, label: &str) -> anyhow::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        bail!("{label} exceeds the {limit}-byte limit");
    }
    if bytes.is_empty() {
        bail!("{label} is empty");
    }
    Ok(bytes)
}

async fn serve(
    data_dir: &Path,
    bind: SocketAddr,
    remote_bind: Option<SocketAddr>,
) -> anyhow::Result<()> {
    let lifecycle = RuntimeLifecycle::load(data_dir, "foreground", "interactive_operator")?;
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    tokio::spawn(async move {
        shutdown_signal().await;
        let _ = shutdown_tx.send(true);
    });
    serve_runtime(data_dir, bind, remote_bind, lifecycle, shutdown_rx, None).await
}

async fn serve_runtime(
    data_dir: &Path,
    bind: SocketAddr,
    remote_bind: Option<SocketAddr>,
    lifecycle: RuntimeLifecycle,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ready_callback: Option<ReadyCallback>,
) -> anyhow::Result<()> {
    require_loopback(bind)?;
    if let Some(remote_bind) = remote_bind {
        require_concrete_remote_bind(remote_bind)?;
    }
    let mut process = MasterProcess::acquire(data_dir)?;
    let token = read_development_token(&process.data_dir().join(DEVELOPMENT_TOKEN_FILE))?;
    let remote_acceptor = if let Some(remote_bind) = remote_bind {
        let now_ms = current_time_ms()?;
        let protector = PlatformSecretProtector;
        let authority = IdentityAuthority::open_existing(process.data_dir(), &protector, now_ms)
            .context("open the initialized Windows enrollment authority for remote TLS")?;
        process
            .kernel_mut()
            .record_identity_authority(authority.receipt())?;
        let identity = authority.issue_ephemeral_server_identity(remote_bind.ip(), now_ms)?;
        Some(build_tls_acceptor(&identity)?)
    } else {
        None
    };
    let validation_runtime = ValidationRuntime::load(process.data_dir())?;
    let review_provider: Arc<dyn ReviewProvider> =
        match ProcessReviewProvider::load(process.data_dir())? {
            Some(provider) => Arc::new(provider),
            None => Arc::new(UnavailableReviewProvider),
        };
    let github_publication = ProcessGithubPublication::load(process.data_dir())?.map(Arc::new);
    let planning_runtime = PlanningRuntime::load(process.data_dir())?;
    let planning_runtime_status = planning_runtime.as_ref().map(PlanningRuntime::status);
    let planning_database_path = process.database_path().to_path_buf();
    let state = AppState {
        process: Arc::new(Mutex::new(process)),
        token_sha256: Sha256::digest(token.as_bytes()).into(),
        started_at_ms: current_time_ms()?,
        lifecycle,
        repository_snapshot_claim_reservation: Arc::new(tokio::sync::Mutex::new(())),
        artifact_integration_reservation: Arc::new(tokio::sync::Mutex::new(())),
        validation_gate_reservation: Arc::new(tokio::sync::Mutex::new(())),
        review_gateway_reservation: Arc::new(tokio::sync::Mutex::new(())),
        publication_reservation: Arc::new(tokio::sync::Mutex::new(())),
        review_provider,
        github_publication,
        planning_runtime: planning_runtime.map(|runtime| Arc::new(Mutex::new(runtime))),
        planning_runtime_status,
        planning_database_path,
        active_planning_calls: Arc::new(Mutex::new(Vec::new())),
        assembly_line_effect_dispatcher: Arc::new(UnavailableAssemblyLineEffectDispatcher),
        validation_runtime,
    };

    let app = Router::new()
        .route("/health", get(get_health))
        .route(
            "/v1/feature-conveyor/status",
            get(get_feature_conveyor_status),
        )
        .route(
            "/v1/assembly-line",
            get(get_assembly_line_owner_projection).layer(DefaultBodyLimit::max(
                MAX_ASSEMBLY_LINE_OWNER_PROJECTION_BYTES,
            )),
        )
        .route(
            "/v1/assembly-line/project-drafts",
            post(record_assembly_line_project_draft).layer(DefaultBodyLimit::max(
                MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/assembly-line/feature-drafts",
            post(record_assembly_line_feature_draft).layer(DefaultBodyLimit::max(
                MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/assembly-line/project-brainstorms",
            post(run_assembly_line_project_brainstorm).layer(DefaultBodyLimit::max(
                MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/assembly-line/feature-brainstorms",
            post(run_assembly_line_feature_brainstorm).layer(DefaultBodyLimit::max(
                MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/assembly-line/repositories/:repository_id/create",
            post(run_assembly_line_github_creation).layer(DefaultBodyLimit::max(
                MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/assembly-line/frozen-specifications",
            post(record_assembly_line_frozen_specification).layer(DefaultBodyLimit::max(
                MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/assembly-line/project-approvals",
            post(approve_assembly_line_project).layer(DefaultBodyLimit::max(
                MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/assembly-line/feature-approvals",
            post(approve_assembly_line_feature).layer(DefaultBodyLimit::max(
                MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/assembly-line/auto-run",
            post(set_assembly_line_auto_run).layer(DefaultBodyLimit::max(
                MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/assembly-line/start",
            post(start_assembly_line).layer(DefaultBodyLimit::max(
                MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/assembly-line/stop",
            post(stop_assembly_line).layer(DefaultBodyLimit::max(
                MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/assembly-line/emergency-pause",
            post(emergency_pause_assembly_line).layer(DefaultBodyLimit::max(
                MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/feature-conveyor/owner-control-bridge",
            post(designate_owner_control_bridge),
        )
        .route(
            "/v1/feature-conveyor/activation-evidence",
            get(get_feature_activation_evidence_admission_projection)
                .post(admit_feature_activation_evidence)
                .layer(DefaultBodyLimit::max(
                    MAX_FEATURE_CONVEYOR_OWNER_ACTIVATION_FRAME_BYTES,
                )),
        )
        .route(
            "/v1/feature-conveyor/repository-grants",
            post(record_repository_grant),
        )
        .route(
            "/v1/feature-conveyor/repository-preflight",
            post(repository_preflight).layer(DefaultBodyLimit::max(
                MAX_FEATURE_CONVEYOR_REPOSITORY_PREFLIGHT_REQUEST_BYTES,
            )),
        )
        .route(
            "/v1/feature-conveyor/repository-snapshot-claims",
            post(repository_snapshot_claim).layer(DefaultBodyLimit::max(
                MAX_FEATURE_CONVEYOR_SNAPSHOT_CLAIM_REQUEST_BYTES,
            )),
        )
        .route(
            "/v1/feature-conveyor/coding-dispatches",
            post(feature_coding_dispatch).layer(DefaultBodyLimit::max(
                MAX_FEATURE_CONVEYOR_CODING_DISPATCH_REQUEST_BYTES,
            )),
        )
        .route(
            "/v1/feature-conveyor/artifact-integrations",
            post(feature_artifact_integration),
        )
        .route(
            "/v1/feature-conveyor/test-evidence-gates",
            post(feature_validation_gate),
        )
        .route(
            "/v1/feature-conveyor/review-gateway",
            post(feature_review_gateway),
        )
        .route(
            "/v1/feature-conveyor/publications",
            post(feature_publication),
        )
        .route(
            "/v1/feature-conveyor/features/:feature_id/integration-plan",
            get(feature_artifact_integration_plan),
        )
        .route(
            "/v1/feature-conveyor/cancel-active-feature",
            post(cancel_active_feature).layer(DefaultBodyLimit::max(
                MAX_FEATURE_CONVEYOR_OWNER_RESOLUTION_REQUEST_BYTES,
            )),
        )
        .route(
            "/v1/feature-conveyor/abandon-and-advance",
            post(abandon_and_advance).layer(DefaultBodyLimit::max(
                MAX_FEATURE_CONVEYOR_OWNER_RESOLUTION_REQUEST_BYTES,
            )),
        )
        .route(
            "/v1/feature-conveyor/repositories/:repository_id/grants",
            get(get_repository_grants),
        )
        .route("/v1/development/devices/register", post(register_device))
        .route("/v1/development/connections/accept", post(accept_handshake))
        .route("/v1/development/events/next", post(development_events_next))
        .route("/v1/development/steps", post(enqueue_step))
        .route(
            "/v1/development/steps/:step_id/cancel",
            post(cancel_development_step),
        )
        .route(
            "/v1/development/emergency-pause/activate",
            post(activate_emergency_pause),
        )
        .route(
            "/v1/development/emergency-pause/resume",
            post(resume_emergency_pause),
        )
        .route("/v1/development/leases/next", post(lease_next))
        .route("/v1/development/results", post(accept_result))
        .layer(DefaultBodyLimit::max(MAX_WIRE_FRAME_BYTES))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind(bind).await?;
    let local_addr = listener.local_addr()?;
    let remote_listener = if let Some(remote_bind) = remote_bind {
        Some(tokio::net::TcpListener::bind(remote_bind).await?)
    } else {
        None
    };
    let remote_addr = remote_listener
        .as_ref()
        .map(tokio::net::TcpListener::local_addr)
        .transpose()?;
    if let Some(ready_callback) = ready_callback {
        ready_callback(local_addr, remote_addr, &state.lifecycle)?;
    }
    println!(
        "{}",
        serde_json::to_string(&json!({
            "status": "ready",
            "endpoint": local_addr.to_string(),
            "remote_endpoint": remote_addr.map(|address| address.to_string()),
            "process_id": std::process::id(),
            "boundary": if remote_addr.is_some() {
                "authenticated_loopback_plus_tls13_mtls_enrolled_devices"
            } else {
                "authenticated_loopback_development_only"
            }
        }))?
    );
    std::io::stdout().flush()?;
    info!(endpoint = %local_addr, remote_endpoint = ?remote_addr, "Windows master process ready");
    if let (Some(remote_listener), Some(remote_acceptor)) = (remote_listener, remote_acceptor) {
        tokio::select! {
            result = axum::serve(listener, app) => result?,
            result = serve_remote(remote_listener, remote_acceptor, state) => result?,
            _ = wait_for_shutdown(shutdown_rx) => {}
        }
    } else {
        axum::serve(listener, app)
            .with_graceful_shutdown(wait_for_shutdown(shutdown_rx))
            .await?;
    }
    Ok(())
}

async fn wait_for_shutdown(mut shutdown_rx: tokio::sync::watch::Receiver<bool>) {
    if *shutdown_rx.borrow() {
        return;
    }
    while shutdown_rx.changed().await.is_ok() {
        if *shutdown_rx.borrow() {
            return;
        }
    }
}

fn build_tls_acceptor(identity: &EphemeralServerIdentity) -> anyhow::Result<TlsAcceptor> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let certificate_chain = identity
        .certificate_chain_der()
        .iter()
        .cloned()
        .map(CertificateDer::from)
        .collect::<Vec<_>>();
    let ca_certificate = certificate_chain
        .last()
        .cloned()
        .context("ephemeral server identity omitted its enrollment CA")?;
    let mut roots = RootCertStore::empty();
    roots
        .add(ca_certificate)
        .context("add enrollment CA to the remote client trust store")?;
    let client_verifier = WebPkiClientVerifier::builder(Arc::new(roots))
        .build()
        .context("build enrolled-device client certificate verifier")?;
    let private_key = PrivateKeyDer::from(PrivatePkcs8KeyDer::from(
        identity.private_key_der().to_vec(),
    ));
    let mut config = ServerConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
        .with_client_cert_verifier(client_verifier)
        .with_single_cert(certificate_chain, private_key)
        .context("build TLS 1.3 remote server configuration")?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok(TlsAcceptor::from(Arc::new(config)))
}

async fn serve_remote(
    listener: tokio::net::TcpListener,
    acceptor: TlsAcceptor,
    state: AppState,
) -> anyhow::Result<()> {
    loop {
        let (stream, peer) = listener.accept().await?;
        let acceptor = acceptor.clone();
        let state = state.clone();
        tokio::spawn(async move {
            let result = serve_remote_connection(stream, acceptor, state).await;
            if let Err(error) = result {
                info!(peer = %peer, error = %error, "remote TLS connection rejected or closed");
            }
        });
    }
}

async fn serve_remote_connection(
    stream: tokio::net::TcpStream,
    acceptor: TlsAcceptor,
    state: AppState,
) -> anyhow::Result<()> {
    let tls_stream = acceptor
        .accept(stream)
        .await
        .context("complete TLS 1.3 mutual-authentication handshake")?;
    let connection = tls_stream.get_ref().1;
    if connection.protocol_version() != Some(rustls::ProtocolVersion::TLSv1_3) {
        bail!("remote connection did not negotiate TLS 1.3");
    }
    let peer_certificate = connection
        .peer_certificates()
        .and_then(|certificates| certificates.first())
        .context("remote connection omitted its enrolled device certificate")?;
    let (device_id, certificate_serial_hex) = parse_device_certificate(peer_certificate.as_ref())?;
    let certificate_sha256: [u8; 32] = Sha256::digest(peer_certificate.as_ref()).into();
    let mut exporter = [0_u8; TLS_EXPORTER_BYTES];
    connection
        .export_keying_material(&mut exporter, TLS_EXPORTER_LABEL, None)
        .context("derive the TLS channel exporter")?;
    let tls_exporter_sha256: [u8; 32] = Sha256::digest(exporter).into();
    let registration = {
        let process =
            lock_process(&state).map_err(|(_, Json(error))| anyhow::anyhow!(error.error))?;
        process.kernel().authenticate_device_certificate(
            device_id,
            &certificate_serial_hex,
            &certificate_sha256,
            current_time_ms()?,
        )?
    };
    let accepted_epoch = Arc::new(Mutex::new(None));
    let session = RemoteSession {
        registration,
        certificate_serial_hex,
        certificate_sha256,
        tls_exporter_sha256,
        accepted_epoch: accepted_epoch.clone(),
    };
    let service = remote_router(state.clone()).layer(Extension(session.clone()));
    let result = http1::Builder::new()
        .serve_connection(TokioIo::new(tls_stream), TowerToHyperService::new(service))
        .await;

    let epoch = accepted_epoch.lock().ok().and_then(|guard| *guard);
    if let Some(epoch) = epoch {
        if let Ok(mut process) = state.process.lock() {
            let _ = process.kernel_mut().disconnect_device(
                session.registration.device_id,
                epoch,
                current_time_ms().unwrap_or(u64::MAX),
            );
        }
    }
    result.context("serve authenticated remote HTTP connection")
}

fn parse_device_certificate(certificate_der: &[u8]) -> anyhow::Result<(DeviceId, String)> {
    let (_, certificate) = x509_parser::certificate::X509Certificate::from_der(certificate_der)
        .map_err(|_| anyhow::anyhow!("parse enrolled device certificate"))?;
    let san = certificate
        .subject_alternative_name()
        .map_err(|_| anyhow::anyhow!("parse enrolled device certificate SAN"))?
        .context("enrolled device certificate omitted its SAN")?;
    let mut device_ids = san
        .value
        .general_names
        .iter()
        .filter_map(|name| match name {
            GeneralName::URI(uri) => uri.strip_prefix("urn:assemblywright:device:"),
            _ => None,
        });
    let device_id = device_ids
        .next()
        .context("enrolled device certificate omitted its Assemblywright device URI")?;
    if device_ids.next().is_some() {
        bail!("enrolled device certificate contains multiple Assemblywright device URIs");
    }
    let device_id =
        DeviceId::new(Uuid::parse_str(device_id).context("parse certificate device ID")?);
    Ok((device_id, hex(certificate.raw_serial())))
}

fn remote_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(remote_get_health))
        .route(
            "/v1/distributed/feature-conveyor/status",
            get(remote_get_feature_conveyor_status),
        )
        .route(
            "/v1/distributed/feature-conveyor/owner-control",
            get(remote_get_feature_conveyor_owner_control),
        )
        .route(
            "/v1/distributed/assembly-line",
            get(remote_get_assembly_line_owner_projection).layer(DefaultBodyLimit::max(
                MAX_ASSEMBLY_LINE_OWNER_PROJECTION_BYTES,
            )),
        )
        .route(
            "/v1/distributed/assembly-line/project-drafts",
            post(remote_record_assembly_line_project_draft).layer(DefaultBodyLimit::max(
                MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/distributed/assembly-line/feature-drafts",
            post(remote_record_assembly_line_feature_draft).layer(DefaultBodyLimit::max(
                MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/distributed/assembly-line/project-brainstorms",
            post(remote_run_assembly_line_project_brainstorm).layer(DefaultBodyLimit::max(
                MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/distributed/assembly-line/feature-brainstorms",
            post(remote_run_assembly_line_feature_brainstorm).layer(DefaultBodyLimit::max(
                MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/distributed/assembly-line/repositories/:repository_id/create",
            post(remote_run_assembly_line_github_creation).layer(DefaultBodyLimit::max(
                MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/distributed/assembly-line/frozen-specifications",
            post(remote_record_assembly_line_frozen_specification).layer(DefaultBodyLimit::max(
                MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/distributed/assembly-line/project-approvals",
            post(remote_approve_assembly_line_project).layer(DefaultBodyLimit::max(
                MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/distributed/assembly-line/feature-approvals",
            post(remote_approve_assembly_line_feature).layer(DefaultBodyLimit::max(
                MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/distributed/assembly-line/auto-run",
            post(remote_set_assembly_line_auto_run).layer(DefaultBodyLimit::max(
                MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/distributed/assembly-line/start",
            post(remote_start_assembly_line).layer(DefaultBodyLimit::max(
                MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/distributed/assembly-line/stop",
            post(remote_stop_assembly_line).layer(DefaultBodyLimit::max(
                MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/distributed/assembly-line/emergency-pause",
            post(remote_emergency_pause_assembly_line).layer(DefaultBodyLimit::max(
                MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/distributed/feature-conveyor/activation",
            post(remote_activate_feature_orchestration).layer(DefaultBodyLimit::max(
                MAX_FEATURE_CONVEYOR_OWNER_ACTIVATION_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/distributed/feature-conveyor/orchestration/pause",
            post(remote_pause_feature_orchestration).layer(DefaultBodyLimit::max(
                MAX_FEATURE_CONVEYOR_OWNER_ACTIVATION_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/distributed/feature-conveyor/orchestration/resume",
            post(remote_resume_feature_orchestration).layer(DefaultBodyLimit::max(
                MAX_FEATURE_CONVEYOR_OWNER_ACTIVATION_FRAME_BYTES,
            )),
        )
        .route(
            "/v1/distributed/feature-conveyor/cancel-active-feature",
            post(remote_cancel_active_feature).layer(DefaultBodyLimit::max(
                MAX_FEATURE_CONVEYOR_OWNER_RESOLUTION_REQUEST_BYTES,
            )),
        )
        .route(
            "/v1/distributed/feature-conveyor/abandon-and-advance",
            post(remote_abandon_and_advance).layer(DefaultBodyLimit::max(
                MAX_FEATURE_CONVEYOR_OWNER_RESOLUTION_REQUEST_BYTES,
            )),
        )
        .route(
            "/v1/distributed/feature-conveyor/approved-features",
            post(remote_enqueue_approved_feature),
        )
        .route(
            "/v1/distributed/local-model/selection",
            get(remote_local_model_selection)
                .post(remote_select_local_model)
                .layer(DefaultBodyLimit::max(MAX_LOCAL_MODEL_SELECTION_FRAME_BYTES)),
        )
        .route(
            "/v1/distributed/connections/accept",
            post(remote_accept_handshake),
        )
        .route("/v1/distributed/leases/next", post(remote_lease_next))
        .route(
            "/v1/distributed/feature-conveyor/snapshot-chunks",
            post(remote_local_coding_snapshot_chunk),
        )
        .route(
            "/v1/distributed/feature-conveyor/result-artifacts",
            post(remote_local_coding_result_artifact),
        )
        .route("/v1/distributed/results", post(remote_accept_result))
        .route(
            "/v1/distributed/cancellations/next",
            post(remote_cancellation_next),
        )
        .route(
            "/v1/distributed/cancellations/ack",
            post(remote_cancellation_ack),
        )
        .route("/v1/distributed/events/next", post(remote_events_next))
        .layer(DefaultBodyLimit::max(MAX_WIRE_FRAME_BYTES))
        .with_state(state)
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

async fn remote_get_health(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
) -> ApiResult<HealthResponse> {
    require_remote_application_session(&state, &session, None)?;
    let (maintenance_active, maintenance_reason) = state.lifecycle.maintenance_snapshot();
    let process = lock_process(&state)?;
    let emergency_paused = process.kernel().emergency_paused().map_err(api_error)?;
    Ok(Json(HealthResponse {
        status: if maintenance_active {
            "maintenance"
        } else if emergency_paused {
            "paused"
        } else {
            "ok"
        }
        .to_string(),
        mode: "developer_remote_master".to_string(),
        host_mode: state.lifecycle.host_mode.clone(),
        service_identity: state.lifecycle.service_identity.clone(),
        maintenance_active,
        maintenance_reason,
        emergency_paused,
        protocol_version: PROTOCOL_VERSION,
        schema_version: process.kernel().schema_version().map_err(api_error)?,
        process_id: std::process::id(),
        started_at_ms: state.started_at_ms,
        startup_reconciliation: process.kernel().startup_reconciliation(),
        state: process.kernel().health_snapshot().map_err(api_error)?,
        boundary: "TLS 1.3 mutual authentication with enrolled-device certificate and durable revocation checks"
            .to_string(),
    }))
}

async fn remote_get_feature_conveyor_status(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
) -> ApiResult<FeatureConveyorStatus> {
    let registration = require_remote_application_session(&state, &session, None)?;
    if registration.role != DeviceRole::MacBridge {
        return Err(unauthorized());
    }
    let status = lock_process(&state)?
        .kernel()
        .feature_conveyor_status()
        .map_err(api_error)?;
    Ok(Json(status))
}

async fn remote_get_feature_conveyor_owner_control(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
) -> ApiResult<FeatureConveyorOwnerControlProjection> {
    let registration = require_remote_application_session(&state, &session, None)?;
    if registration.role != DeviceRole::MacBridge {
        return Err(unauthorized());
    }
    let projection = lock_process(&state)?
        .kernel_mut()
        .feature_conveyor_owner_control_projection(&registration)
        .map_err(|_| unauthorized())?;
    Ok(Json(projection))
}

fn require_remote_assembly_line_owner(
    state: &AppState,
    session: &RemoteSession,
) -> Result<DeviceRegistration, ApiError> {
    let registration = require_remote_application_session(state, session, None)?;
    if registration.role != DeviceRole::MacBridge {
        return Err(unauthorized());
    }
    Ok(registration)
}

fn current_planning_runtime_status(state: &AppState) -> Option<PlanningRuntimeStatus> {
    match state.planning_runtime.as_ref()?.try_lock() {
        Ok(runtime) => runtime.validated_status(),
        Err(std::sync::TryLockError::WouldBlock) => state.planning_runtime_status,
        Err(std::sync::TryLockError::Poisoned(_)) => None,
    }
}

async fn remote_get_assembly_line_owner_projection(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
) -> ApiResult<AssemblyLineOwnerProjection> {
    let registration = require_remote_assembly_line_owner(&state, &session)?;
    let mut process = lock_process(&state)?;
    process
        .kernel_mut()
        .authorize_assembly_line_owner_bridge(&registration)
        .map_err(|_| unauthorized())?;
    let projection = process
        .kernel()
        .assembly_line_owner_projection_with_runtime(
            current_time_ms().map_err(api_error)?,
            current_planning_runtime_status(&state),
            state.assembly_line_effect_dispatcher.runtime_status(),
        )
        .map_err(assembly_line_api_error)?;
    Ok(Json(projection))
}

async fn remote_record_assembly_line_project_draft(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<AssemblyLineOwnerProjection> {
    let registration = require_remote_assembly_line_owner(&state, &session)?;
    let body = assembly_line_body(body)?;
    let draft = ProjectBrainstormingDraft::decode_frame(&body)
        .map_err(|_| assembly_line_request_rejected())?;
    let now_ms = current_time_ms().map_err(api_error)?;
    let mut process = lock_process(&state)?;
    process
        .kernel_mut()
        .authorize_assembly_line_owner_bridge(&registration)
        .map_err(|_| unauthorized())?;
    process
        .kernel_mut()
        .record_assembly_line_project_draft(&draft, now_ms)
        .map_err(assembly_line_api_error)?;
    Ok(Json(
        process
            .kernel()
            .assembly_line_owner_projection(now_ms)
            .map_err(assembly_line_api_error)?,
    ))
}

async fn remote_record_assembly_line_feature_draft(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<AssemblyLineOwnerProjection> {
    let registration = require_remote_assembly_line_owner(&state, &session)?;
    let body = assembly_line_body(body)?;
    let draft = FeatureBrainstormingDraft::decode_frame(&body)
        .map_err(|_| assembly_line_request_rejected())?;
    let now_ms = current_time_ms().map_err(api_error)?;
    let mut process = lock_process(&state)?;
    process
        .kernel_mut()
        .authorize_assembly_line_owner_bridge(&registration)
        .map_err(|_| unauthorized())?;
    process
        .kernel_mut()
        .record_assembly_line_feature_draft(&draft, now_ms)
        .map_err(assembly_line_api_error)?;
    Ok(Json(
        process
            .kernel()
            .assembly_line_owner_projection(now_ms)
            .map_err(assembly_line_api_error)?,
    ))
}

async fn remote_run_assembly_line_project_brainstorm(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<FrozenBrainstormingSpecification> {
    let registration = require_remote_assembly_line_owner(&state, &session)?;
    let body = assembly_line_body(body)?;
    let request = ProjectBrainstormingCloudRequest::decode_frame(&body)
        .map_err(|_| assembly_line_request_rejected())?;
    run_planning_brainstorm(
        state,
        BrainstormingDraft::Project(request.draft),
        BrainstormingCloudAuthorization {
            owner_cloud_disclosure_sha256: request.owner_cloud_disclosure_sha256,
        },
        Some(registration),
    )
    .await
}

async fn remote_run_assembly_line_feature_brainstorm(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<FrozenBrainstormingSpecification> {
    let registration = require_remote_assembly_line_owner(&state, &session)?;
    let body = assembly_line_body(body)?;
    let request = FeatureBrainstormingCloudRequest::decode_frame(&body)
        .map_err(|_| assembly_line_request_rejected())?;
    run_planning_brainstorm(
        state,
        BrainstormingDraft::Feature(request.draft),
        BrainstormingCloudAuthorization {
            owner_cloud_disclosure_sha256: request.owner_cloud_disclosure_sha256,
        },
        Some(registration),
    )
    .await
}

async fn remote_run_assembly_line_github_creation(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    AxumPath(repository_id): AxumPath<String>,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<RepositoryCreationProjection> {
    let registration = require_remote_assembly_line_owner(&state, &session)?;
    let body = assembly_line_body(body)?;
    if !body.is_empty() {
        return Err(assembly_line_request_rejected());
    }
    let repository_id = Uuid::parse_str(&repository_id)
        .ok()
        .filter(|identifier| !identifier.is_nil())
        .ok_or_else(assembly_line_request_rejected)?;
    run_planning_github_creation(state, repository_id, Some(registration)).await
}

async fn remote_record_assembly_line_frozen_specification(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<AssemblyLineOwnerProjection> {
    let registration = require_remote_assembly_line_owner(&state, &session)?;
    let body = assembly_line_body(body)?;
    let frozen = FrozenBrainstormingSpecification::decode_frame(&body)
        .map_err(|_| assembly_line_request_rejected())?;
    let now_ms = current_time_ms().map_err(api_error)?;
    let mut process = lock_process(&state)?;
    process
        .kernel_mut()
        .authorize_assembly_line_owner_bridge(&registration)
        .map_err(|_| unauthorized())?;
    process
        .kernel_mut()
        .record_assembly_line_frozen_specification(&frozen, now_ms)
        .map_err(assembly_line_api_error)?;
    Ok(Json(
        process
            .kernel()
            .assembly_line_owner_projection(now_ms)
            .map_err(assembly_line_api_error)?,
    ))
}

async fn remote_approve_assembly_line_project(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<RepositoryCreationProjection> {
    let registration = require_remote_assembly_line_owner(&state, &session)?;
    let body = assembly_line_body(body)?;
    let approval = BrainstormingOwnerApprovalBinding::decode_frame(&body)
        .map_err(|_| assembly_line_request_rejected())?;
    let mut process = lock_process(&state)?;
    process
        .kernel_mut()
        .authorize_assembly_line_owner_bridge(&registration)
        .map_err(|_| unauthorized())?;
    let projection = process
        .kernel_mut()
        .approve_assembly_line_project(&approval, current_time_ms().map_err(api_error)?)
        .map_err(assembly_line_api_error)?;
    Ok(Json(projection))
}

async fn remote_approve_assembly_line_feature(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<FeatureQueueEntryProjection> {
    let registration = require_remote_assembly_line_owner(&state, &session)?;
    let body = assembly_line_body(body)?;
    let approval = BrainstormingOwnerApprovalBinding::decode_frame(&body)
        .map_err(|_| assembly_line_request_rejected())?;
    let runtime = state
        .planning_runtime
        .as_ref()
        .ok_or_else(planning_runtime_unavailable)?
        .clone();
    let mut process = lock_process(&state)?;
    process
        .kernel_mut()
        .authorize_assembly_line_owner_bridge(&registration)
        .map_err(|_| unauthorized())?;
    let projection = runtime
        .lock()
        .map_err(|_| planning_runtime_unavailable())?
        .approve_feature_and_enqueue(
            process.kernel_mut(),
            &approval,
            current_time_ms().map_err(api_error)?,
        )
        .map_err(planning_effect_api_error)?;
    Ok(Json(projection))
}

async fn remote_set_assembly_line_auto_run(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<AssemblyLineAutoRunReceipt> {
    let registration = require_remote_assembly_line_owner(&state, &session)?;
    let body = assembly_line_body(body)?;
    let request = AssemblyLineAutoRunRequest::decode_frame(&body)
        .map_err(|_| assembly_line_request_rejected())?;
    let mut process = lock_process(&state)?;
    process
        .kernel_mut()
        .authorize_assembly_line_owner_bridge(&registration)
        .map_err(|_| unauthorized())?;
    let receipt = process
        .kernel_mut()
        .set_assembly_line_auto_run(&request, current_time_ms().map_err(api_error)?)
        .map_err(assembly_line_api_error)?;
    Ok(Json(receipt))
}

async fn remote_start_assembly_line(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    body: Result<Bytes, BytesRejection>,
) -> Result<(StatusCode, Json<AssemblyLineOwnerProjection>), ApiError> {
    let registration = require_remote_assembly_line_owner(&state, &session)?;
    if state
        .assembly_line_effect_dispatcher
        .runtime_status()
        .is_none()
    {
        return Err(assembly_line_effect_routes_unavailable());
    }
    let request = AssemblyLineStartRequest::decode_frame(&assembly_line_body(body)?)
        .map_err(|_| assembly_line_request_rejected())?;
    execute_assembly_line_start(&state, &request, Some(&registration))
}

async fn remote_stop_assembly_line(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    body: Result<Bytes, BytesRejection>,
) -> Result<(StatusCode, Json<AssemblyLineOwnerProjection>), ApiError> {
    let registration = require_remote_assembly_line_owner(&state, &session)?;
    if state
        .assembly_line_effect_dispatcher
        .runtime_status()
        .is_none()
    {
        return Err(assembly_line_effect_routes_unavailable());
    }
    let request = AssemblyLineStopRequest::decode_frame(&assembly_line_body(body)?)
        .map_err(|_| assembly_line_request_rejected())?;
    execute_assembly_line_termination(&state, Some(&request), None, Some(&registration))
}

async fn remote_emergency_pause_assembly_line(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    body: Result<Bytes, BytesRejection>,
) -> Result<(StatusCode, Json<AssemblyLineOwnerProjection>), ApiError> {
    let registration = require_remote_assembly_line_owner(&state, &session)?;
    if state
        .assembly_line_effect_dispatcher
        .runtime_status()
        .is_none()
    {
        return Err(assembly_line_effect_routes_unavailable());
    }
    let request = AssemblyLineEmergencyPauseRequest::decode_frame(&assembly_line_body(body)?)
        .map_err(|_| assembly_line_request_rejected())?;
    execute_assembly_line_termination(&state, None, Some(&request), Some(&registration))
}

async fn remote_activate_feature_orchestration(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<FeatureConveyorActivationReceipt> {
    let registration = require_remote_application_session(&state, &session, None)?;
    if registration.role != DeviceRole::MacBridge {
        return Err(unauthorized());
    }
    let body = body.map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "feature_activation_request_rejected",
        )
    })?;
    let request = FeatureConveyorActivationRequest::decode_frame(&body).map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "feature_activation_request_rejected",
        )
    })?;
    let receipt = lock_process(&state)?
        .kernel_mut()
        .activate_feature_orchestration_from_owner_bridge(
            &request,
            &registration,
            current_time_ms()
                .map_err(|_| fixed_error(StatusCode::CONFLICT, "feature_activation_rejected"))?,
        )
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "feature_activation_rejected"))?;
    Ok(Json(receipt))
}

async fn remote_pause_feature_orchestration(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<FeatureConveyorOwnerOrchestrationControlReceipt> {
    remote_owner_orchestration_control(state, session, body, true).await
}

async fn remote_resume_feature_orchestration(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<FeatureConveyorOwnerOrchestrationControlReceipt> {
    remote_owner_orchestration_control(state, session, body, false).await
}

async fn remote_owner_orchestration_control(
    state: AppState,
    session: RemoteSession,
    body: Result<Bytes, BytesRejection>,
    pause: bool,
) -> ApiResult<FeatureConveyorOwnerOrchestrationControlReceipt> {
    let registration = require_remote_application_session(&state, &session, None)?;
    if registration.role != DeviceRole::MacBridge {
        return Err(unauthorized());
    }
    let body = body.map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "feature_orchestration_control_request_rejected",
        )
    })?;
    let request =
        FeatureConveyorOwnerOrchestrationControlRequest::decode_frame(&body).map_err(|_| {
            fixed_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "feature_orchestration_control_request_rejected",
            )
        })?;
    let now_ms = current_time_ms().map_err(|_| {
        fixed_error(
            StatusCode::CONFLICT,
            "feature_orchestration_control_rejected",
        )
    })?;
    let receipt = if pause {
        lock_process(&state)?
            .kernel_mut()
            .pause_feature_orchestration_from_owner_bridge(&request, &registration, now_ms)
    } else {
        lock_process(&state)?
            .kernel_mut()
            .resume_feature_orchestration_from_owner_bridge(&request, &registration, now_ms)
    }
    .map_err(|_| {
        fixed_error(
            StatusCode::CONFLICT,
            "feature_orchestration_control_rejected",
        )
    })?;
    Ok(Json(receipt))
}

async fn remote_cancel_active_feature(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<FeatureConveyorCancelActiveFeatureReceipt> {
    let registration = require_remote_application_session(&state, &session, None)?;
    if registration.role != DeviceRole::MacBridge {
        return Err(unauthorized());
    }
    let body = body.map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "feature_cancel_request_rejected",
        )
    })?;
    let request =
        FeatureConveyorRemoteCancelActiveFeatureRequest::decode_frame(&body).map_err(|_| {
            fixed_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "feature_cancel_request_rejected",
            )
        })?;
    let snapshot = lock_process(&state)?
        .kernel_mut()
        .cancel_active_feature_from_owner_bridge(
            &request,
            &registration,
            current_time_ms()
                .map_err(|_| fixed_error(StatusCode::CONFLICT, "feature_cancel_rejected"))?,
        )
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "feature_cancel_rejected"))?;
    let receipt = FeatureConveyorCancelActiveFeatureReceipt {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        feature_id: snapshot.feature_id,
        lifecycle_revision: snapshot.lifecycle_revision,
        queue_revision: request.expected_queue_revision,
        emergency_pause_revision: request.expected_emergency_pause_revision,
        lease_retained: snapshot.active_lease_id.is_some(),
        advancement_authorized: false,
        status: FeatureConveyorCancelActiveFeatureStatus::Cancelled,
    };
    receipt
        .validate()
        .map_err(|_| fixed_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"))?;
    Ok(Json(receipt))
}

async fn remote_abandon_and_advance(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<FeatureConveyorAbandonAndAdvanceReceipt> {
    let registration = require_remote_application_session(&state, &session, None)?;
    if registration.role != DeviceRole::MacBridge {
        return Err(unauthorized());
    }
    let body = body.map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "feature_abandonment_request_rejected",
        )
    })?;
    let request =
        FeatureConveyorRemoteAbandonAndAdvanceRequest::decode_frame(&body).map_err(|_| {
            fixed_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "feature_abandonment_request_rejected",
            )
        })?;
    let snapshot = lock_process(&state)?
        .kernel_mut()
        .abandon_and_advance_from_owner_bridge(
            &request,
            &registration,
            current_time_ms()
                .map_err(|_| fixed_error(StatusCode::CONFLICT, "feature_abandonment_rejected"))?,
        )
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "feature_abandonment_rejected"))?;
    let receipt = FeatureConveyorAbandonAndAdvanceReceipt {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        feature_id: snapshot.feature_id,
        lifecycle_revision: snapshot.lifecycle_revision,
        queue_revision: request.expected_queue_revision + 1,
        emergency_pause_revision: request.expected_emergency_pause_revision,
        lease_released: snapshot.active_lease_id.is_none(),
        status: FeatureConveyorAbandonAndAdvanceStatus::Abandoned,
    };
    receipt
        .validate()
        .map_err(|_| fixed_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"))?;
    Ok(Json(receipt))
}

async fn remote_enqueue_approved_feature(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<FeatureConveyorApprovedFeatureReceipt> {
    let registration = require_remote_application_session(&state, &session, None)?;
    if registration.role != DeviceRole::MacBridge {
        return Err(unauthorized());
    }
    if state.lifecycle.maintenance_active.load(Ordering::SeqCst) {
        return Err(fixed_error(
            StatusCode::CONFLICT,
            "approved_feature_enqueue_rejected",
        ));
    }
    let body = body.map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "approved_feature_request_rejected",
        )
    })?;
    let request = FeatureConveyorApprovedFeatureRequest::decode_frame(&body).map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "approved_feature_request_rejected",
        )
    })?;
    let specification: ApprovedFeatureSpecification = request.specification.into();
    let mut process = lock_process(&state)?;
    let snapshot = process
        .kernel_mut()
        .enqueue_approved_feature_from_owner_bridge(
            &specification,
            request.expected_queue_revision,
            request.owner_control_designation_revision,
            request.emergency_pause_revision,
            &registration,
            current_time_ms().map_err(|_| {
                fixed_error(StatusCode::CONFLICT, "approved_feature_enqueue_rejected")
            })?,
        )
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "approved_feature_enqueue_rejected"))?;
    let queue_revision = process.kernel().feature_queue_revision().map_err(|_| {
        fixed_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "approved_feature_enqueue_unavailable",
        )
    })?;
    Ok(Json(FeatureConveyorApprovedFeatureReceipt {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        feature_id: snapshot.feature_id,
        specification_revision: snapshot.specification_revision,
        lifecycle_revision: snapshot.lifecycle_revision,
        queue_revision,
        owner_control_designation_revision: request.owner_control_designation_revision,
        emergency_pause_revision: request.emergency_pause_revision,
        status: FeatureConveyorApprovedFeatureStatus::Queued,
    }))
}

async fn remote_local_model_selection(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
) -> ApiResult<LocalModelSelectionProjection> {
    let registration = require_remote_application_session(&state, &session, None)?;
    let projection = lock_process(&state)?
        .kernel()
        .local_model_selection_projection(&registration)
        .map_err(|_| unauthorized())?;
    Ok(Json(projection))
}

async fn remote_select_local_model(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<LocalModelSelectionReceipt> {
    if state.lifecycle.maintenance_active.load(Ordering::SeqCst) {
        return Err(fixed_error(
            StatusCode::CONFLICT,
            "local_model_selection_rejected",
        ));
    }
    let registration = require_remote_application_session(&state, &session, None)?;
    let accepted_epoch = session
        .accepted_epoch
        .lock()
        .map_err(|_| local_model_selection_internal_error())?
        .ok_or_else(unauthorized)?;
    let body = body.map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "local_model_selection_request_rejected",
        )
    })?;
    let request = LocalModelSelectionRequest::decode_frame(&body).map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "local_model_selection_request_rejected",
        )
    })?;
    let receipt = lock_process(&state)
        .map_err(|_| local_model_selection_internal_error())?
        .kernel_mut()
        .select_local_model_from_owner_bridge(
            &request,
            &registration,
            accepted_epoch,
            current_time_ms().map_err(|_| local_model_selection_internal_error())?,
        )
        .map_err(local_model_selection_api_error)?;
    *session
        .accepted_epoch
        .lock()
        .map_err(|_| local_model_selection_internal_error())? = None;
    Ok(Json(receipt))
}

async fn remote_accept_handshake(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    Json(request): Json<AuthenticatedHandshakeRequest>,
) -> ApiResult<HandshakeResponse> {
    request.validate().map_err(api_error)?;
    let registration = revalidate_remote_session(&state, &session)?;
    if request.handshake.device_id != registration.device_id
        || request.handshake.device_name != registration.device_name
        || request.handshake.role != registration.role
        || request.handshake.registry_revision != registration.registry_revision
        || request.handshake.capabilities != registration.capabilities
    {
        return Err(unauthorized());
    }
    if !constant_time_equal(&request.tls_exporter_sha256, &session.tls_exporter_sha256) {
        return Err(unauthorized());
    }
    {
        let accepted = session
            .accepted_epoch
            .lock()
            .map_err(|_| internal_error())?;
        if accepted.is_some() {
            return Err(api_error(
                "this TLS connection already accepted a handshake",
            ));
        }
    }
    let response = lock_process(&state)?
        .kernel_mut()
        .accept_handshake(&request.handshake, current_time_ms().map_err(api_error)?)
        .map_err(api_error)?;
    if response.status == HandshakeStatus::Accepted {
        *session
            .accepted_epoch
            .lock()
            .map_err(|_| internal_error())? = Some(response.connection_epoch);
    }
    Ok(Json(response))
}

async fn remote_lease_next(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    Json(request): Json<LeaseRequest>,
) -> Result<Response, ApiError> {
    require_work_admission(&state)?;
    let registration =
        require_remote_application_session(&state, &session, Some(request.connection_epoch))?;
    if request.device_id != registration.device_id {
        return Err(unauthorized());
    }
    let contract =
        RemoteWorkContract::from_registration(&registration).map_err(|_| unauthorized())?;
    let job = lock_process(&state)?.kernel_mut().lease_next_remote_step(
        registration.device_id,
        request.connection_epoch,
        current_time_ms().map_err(api_error)?,
        &contract,
    );
    match job {
        Ok(job) => Ok(Json(job).into_response()),
        Err(assemblywright_master::MasterError::NoEligibleStep) => {
            Ok(StatusCode::NO_CONTENT.into_response())
        }
        Err(error) => Err(api_error(error)),
    }
}

async fn remote_local_coding_snapshot_chunk(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    Json(request): Json<LocalCodingSnapshotChunkRequest>,
) -> ApiResult<LocalCodingSnapshotChunk> {
    require_work_admission(&state)?;
    let registration =
        require_remote_application_session(&state, &session, Some(request.connection_epoch))?;
    if !matches!(
        RemoteWorkContract::from_registration(&registration),
        Ok(RemoteWorkContract::LocalCoding)
    ) {
        return Err(unauthorized());
    }
    let now_ms = current_time_ms().map_err(api_error)?;
    let data_dir = {
        let process = lock_process(&state)?;
        process
            .kernel()
            .authorize_local_coding_snapshot_chunk(registration.device_id, &request, now_ms)
            .map_err(snapshot_transfer_error)?;
        process.data_dir().to_path_buf()
    };
    let store = RepositorySnapshotStore::open(&data_dir).map_err(|_| {
        snapshot_transfer_error(
            assemblywright_master::MasterError::FeatureCodingDispatchUnavailable,
        )
    })?;
    let chunk = store
        .read_bundle_chunk(
            request.snapshot_id,
            request.offset,
            MAX_LOCAL_CODING_SNAPSHOT_CHUNK_BYTES,
        )
        .map_err(|_| {
            snapshot_transfer_error(
                assemblywright_master::MasterError::FeatureCodingDispatchUnavailable,
            )
        })?;
    // Recheck immediately after filesystem work so pause, cancellation,
    // registration drift, lifecycle departure, and lease expiry dominate the
    // response rather than leaving a reusable read authorization.
    lock_process(&state)?
        .kernel()
        .authorize_local_coding_snapshot_chunk(
            registration.device_id,
            &request,
            current_time_ms().map_err(api_error)?,
        )
        .map_err(snapshot_transfer_error)?;
    let content_sha256: [u8; 32] = Sha256::digest(&chunk.content).into();
    Ok(Json(LocalCodingSnapshotChunk {
        protocol_version: request.protocol_version,
        connection_epoch: request.connection_epoch,
        task_id: request.task_id,
        step_id: request.step_id,
        attempt_id: request.attempt_id,
        lease_id: request.lease_id,
        cancellation_id: request.cancellation_id,
        snapshot_id: request.snapshot_id,
        snapshot_sha256: request.snapshot_sha256,
        offset: chunk.offset,
        total_bytes: chunk.total_bytes,
        content_sha256,
        content_hex: hex(&chunk.content),
        complete: chunk.offset + chunk.content.len() as u64 == chunk.total_bytes,
    }))
}

fn snapshot_transfer_error(_error: assemblywright_master::MasterError) -> ApiError {
    fixed_error(StatusCode::CONFLICT, "snapshot_transfer_rejected")
}

async fn remote_local_coding_result_artifact(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<LocalCodingResultArtifactReceipt> {
    require_work_admission(&state)?;
    let body = body.map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "result_artifact_admission_rejected",
        )
    })?;
    let admission = LocalCodingResultArtifactAdmission::decode_frame(&body).map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "result_artifact_admission_rejected",
        )
    })?;
    let registration =
        require_remote_application_session(&state, &session, Some(admission.connection_epoch))?;
    if !matches!(
        RemoteWorkContract::from_registration(&registration),
        Ok(RemoteWorkContract::LocalCoding)
    ) {
        return Err(unauthorized());
    }
    let now_ms = current_time_ms().map_err(api_error)?;
    let (store, artifact_bytes, already_admitted) = {
        let process = lock_process(&state)?;
        let already_admitted = process
            .kernel()
            .authorize_local_coding_result_artifact(registration.device_id, &admission, now_ms)
            .map_err(result_artifact_error)?;
        (
            process.result_artifact_store(),
            admission.artifact.validate().map_err(|_| {
                result_artifact_error(assemblywright_master::MasterError::ResultArtifactUnavailable)
            })?,
            already_admitted,
        )
    };
    let reference = ResultArtifactReference {
        artifact_id: admission.artifact.artifact_id,
        artifact_sha256: admission.artifact.artifact_sha256,
        artifact_size_bytes: admission.artifact.artifact_size_bytes,
    };
    let mut existing = if already_admitted {
        Some(store.open_verified(reference).map_err(|_| {
            result_artifact_error(assemblywright_master::MasterError::ResultArtifactUnavailable)
        })?)
    } else {
        None
    };
    let mut prepared = if already_admitted {
        None
    } else {
        Some(
            store
                .prepare(
                    admission.artifact.artifact_id,
                    admission.artifact.artifact_sha256,
                    &artifact_bytes,
                )
                .map_err(|_| {
                    result_artifact_error(
                        assemblywright_master::MasterError::ResultArtifactUnavailable,
                    )
                })?,
        )
    };
    if let Some(verified) = existing.as_mut() {
        verified.revalidate(&store).map_err(|_| {
            result_artifact_error(assemblywright_master::MasterError::ResultArtifactUnavailable)
        })?;
    }
    if let Some(prepared) = prepared.as_mut() {
        prepared.verified_mut().revalidate(&store).map_err(|_| {
            result_artifact_error(assemblywright_master::MasterError::ResultArtifactUnavailable)
        })?;
    }
    let finalized = lock_process(&state)?
        .kernel_mut()
        .finalize_local_coding_result_artifact(
            registration.device_id,
            &admission,
            current_time_ms().map_err(api_error)?,
        );
    match finalized {
        Ok(receipt) => {
            if let Some(prepared) = prepared.as_mut() {
                prepared.mark_committed().map_err(|_| {
                    result_artifact_error(
                        assemblywright_master::MasterError::ResultArtifactUnavailable,
                    )
                })?;
            }
            Ok(Json(receipt))
        }
        Err(error) => {
            if let Some(prepared) = prepared {
                let referenced = lock_process(&state)
                    .ok()
                    .and_then(|process| process.kernel().result_artifact_ids().ok())
                    .is_some_and(|ids| ids.contains(&admission.artifact.artifact_id));
                prepared.cleanup_if_unreferenced(referenced).map_err(|_| {
                    result_artifact_error(
                        assemblywright_master::MasterError::ResultArtifactUnavailable,
                    )
                })?;
            }
            Err(result_artifact_error(error))
        }
    }
}

fn result_artifact_error(_error: assemblywright_master::MasterError) -> ApiError {
    fixed_error(StatusCode::CONFLICT, "result_artifact_admission_rejected")
}

async fn remote_accept_result(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    Json(result): Json<JobResultEnvelope>,
) -> ApiResult<AcceptedResult> {
    require_work_admission(&state)?;
    let registration =
        require_remote_application_session(&state, &session, Some(result.connection_epoch))?;
    let contract =
        RemoteWorkContract::from_registration(&registration).map_err(|_| unauthorized())?;
    let artifact_reference = if matches!(contract, RemoteWorkContract::LocalCoding) {
        result.validate().map_err(|_| {
            result_artifact_error(assemblywright_master::MasterError::ResultArtifactUnavailable)
        })?;
        let payload: LocalCodingJobResult = serde_json::from_value(result.payload.clone())
            .map_err(|_| {
                result_artifact_error(assemblywright_master::MasterError::ResultArtifactUnavailable)
            })?;
        Some(ResultArtifactReference {
            artifact_id: payload.artifact_id,
            artifact_sha256: payload.artifact_sha256,
            artifact_size_bytes: payload.artifact_size_bytes,
        })
    } else {
        None
    };
    let mut process = lock_process(&state)?;
    let store = process.result_artifact_store();
    let mut verified = artifact_reference
        .map(|reference| store.open_verified(reference))
        .transpose()
        .map_err(|_| {
            result_artifact_error(assemblywright_master::MasterError::ResultArtifactUnavailable)
        })?;
    let now_ms = current_time_ms().map_err(api_error)?;
    let accepted = if let Some(verified) = verified.as_mut() {
        // The kernel re-hashes this no-follow handle before opening SQLite and
        // keeps the borrowed file/directory handles live through commit.
        process
            .kernel_mut()
            .accept_remote_result_from_with_artifact(
                registration.device_id,
                &result,
                now_ms,
                &contract,
                &store,
                verified,
            )
    } else {
        process.kernel_mut().accept_remote_result_from(
            registration.device_id,
            &result,
            now_ms,
            &contract,
        )
    }
    .map_err(bound_worker_error)?;
    Ok(Json(accepted))
}

async fn remote_cancellation_next(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    Json(request): Json<CancellationPollRequest>,
) -> Result<Response, ApiError> {
    request.validate().map_err(api_error)?;
    let registration =
        require_remote_application_session(&state, &session, Some(request.connection_epoch))?;
    let contract =
        RemoteWorkContract::from_registration(&registration).map_err(|_| unauthorized())?;
    match lock_process(&state)?
        .kernel_mut()
        .next_remote_cancellation(
            registration.device_id,
            request.connection_epoch,
            current_time_ms().map_err(api_error)?,
            &contract,
        )
        .map_err(bound_worker_error)?
    {
        Some(instruction) => Ok(Json(instruction).into_response()),
        None => Ok(Json(json!({"status": "no_cancellation"})).into_response()),
    }
}

async fn remote_cancellation_ack(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    Json(acknowledgement): Json<CancellationAcknowledgement>,
) -> ApiResult<AcceptedCancellation> {
    let registration = require_remote_application_session(
        &state,
        &session,
        Some(acknowledgement.connection_epoch),
    )?;
    let contract =
        RemoteWorkContract::from_registration(&registration).map_err(|_| unauthorized())?;
    let accepted = lock_process(&state)?
        .kernel_mut()
        .accept_remote_cancellation_ack_from(
            registration.device_id,
            &acknowledgement,
            current_time_ms().map_err(api_error)?,
            &contract,
        )
        .map_err(bound_worker_error)?;
    Ok(Json(accepted))
}

fn bound_worker_error(error: assemblywright_master::MasterError) -> ApiError {
    match error {
        assemblywright_master::MasterError::ResultDeviceMismatch => unauthorized(),
        assemblywright_master::MasterError::CancellationExpired
        | assemblywright_master::MasterError::ResultNotAccepting(_)
        | assemblywright_master::MasterError::ConnectionNotActive
        | assemblywright_master::MasterError::ConnectionEpochMismatch
        | assemblywright_master::MasterError::SequenceReplay => (
            StatusCode::CONFLICT,
            Json(ErrorResponse {
                error: "distributed_work_rejected".to_string(),
            }),
        ),
        assemblywright_master::MasterError::EmergencyPaused => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "emergency_pause_blocks_work".to_string(),
            }),
        ),
        other => api_error(other),
    }
}

#[cfg(test)]
fn registration_can_execute_fixture(registration: &DeviceRegistration) -> bool {
    matches!(
        RemoteWorkContract::from_registration(registration),
        Ok(RemoteWorkContract::Fixture)
    )
}

async fn remote_events_next(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    Json(request): Json<DistributedEventBatchRequest>,
) -> ApiResult<DistributedEventBatch> {
    request.validate().map_err(api_error)?;
    let registration =
        require_remote_application_session(&state, &session, Some(request.connection_epoch))?;
    if registration.role != DeviceRole::MacBridge {
        return Err(unauthorized());
    }
    let events = lock_process(&state)?
        .kernel()
        .distributed_events(&request)
        .map_err(distributed_event_error)?;
    Ok(Json(events))
}

fn distributed_event_error(error: assemblywright_master::MasterError) -> ApiError {
    let code = match error {
        assemblywright_master::MasterError::EventCursorStreamMismatch => {
            "event_cursor_stream_mismatch"
        }
        assemblywright_master::MasterError::EventCursorAhead => "event_cursor_ahead",
        assemblywright_master::MasterError::Protocol(_) => "invalid_event_cursor_request",
        _ => return internal_error(),
    };
    (
        StatusCode::CONFLICT,
        Json(ErrorResponse {
            error: code.to_string(),
        }),
    )
}

fn revalidate_remote_session(
    state: &AppState,
    session: &RemoteSession,
) -> Result<DeviceRegistration, ApiError> {
    let process = lock_process(state)?;
    let registration = process
        .kernel()
        .authenticate_device_certificate(
            session.registration.device_id,
            &session.certificate_serial_hex,
            &session.certificate_sha256,
            current_time_ms().map_err(api_error)?,
        )
        .map_err(|_| unauthorized())?;
    if registration != session.registration {
        return Err(unauthorized());
    }
    Ok(registration)
}

fn require_remote_application_session(
    state: &AppState,
    session: &RemoteSession,
    requested_epoch: Option<u64>,
) -> Result<DeviceRegistration, ApiError> {
    let registration = revalidate_remote_session(state, session)?;
    let epoch = session
        .accepted_epoch
        .lock()
        .map_err(|_| internal_error())?
        .ok_or_else(unauthorized)?;
    if requested_epoch.is_some_and(|requested| requested != epoch) {
        return Err(unauthorized());
    }
    Ok(registration)
}

async fn get_health(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<HealthResponse> {
    authorize(&headers, &state)?;
    let (maintenance_active, maintenance_reason) = state.lifecycle.maintenance_snapshot();
    let process = lock_process(&state)?;
    let emergency_paused = process.kernel().emergency_paused().map_err(api_error)?;
    Ok(Json(HealthResponse {
        status: if maintenance_active {
            "maintenance"
        } else if emergency_paused {
            "paused"
        } else {
            "ok"
        }
        .to_string(),
        mode: "developer_foundation".to_string(),
        host_mode: state.lifecycle.host_mode.clone(),
        service_identity: state.lifecycle.service_identity.clone(),
        maintenance_active,
        maintenance_reason,
        emergency_paused,
        protocol_version: PROTOCOL_VERSION,
        schema_version: process.kernel().schema_version().map_err(api_error)?,
        process_id: std::process::id(),
        started_at_ms: state.started_at_ms,
        startup_reconciliation: process.kernel().startup_reconciliation(),
        state: process.kernel().health_snapshot().map_err(api_error)?,
        boundary: "authenticated loopback development transport; not mTLS or enrolled-device authentication"
            .to_string(),
    }))
}

async fn get_feature_conveyor_status(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<FeatureConveyorStatus> {
    authorize(&headers, &state)?;
    let status = lock_process(&state)?
        .kernel()
        .feature_conveyor_status()
        .map_err(api_error)?;
    Ok(Json(status))
}

fn assembly_line_body(body: Result<Bytes, BytesRejection>) -> Result<Bytes, ApiError> {
    let body = body.map_err(|_| fixed_error(StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large"))?;
    if body.len() > MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES {
        return Err(fixed_error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "payload_too_large",
        ));
    }
    Ok(body)
}

fn assembly_line_request_rejected() -> ApiError {
    fixed_error(
        StatusCode::UNPROCESSABLE_ENTITY,
        "assembly_line_request_rejected",
    )
}

fn assembly_line_api_error(error: MasterError) -> ApiError {
    match error {
        MasterError::Protocol(_)
        | MasterError::InvalidAssemblyLinePlanningInput(_)
        | MasterError::AssemblyLinePlanningImmutable
        | MasterError::StaleAssemblyLineOwnerControlRevision { .. }
        | MasterError::StaleAssemblyLineStateRevision { .. }
        | MasterError::StaleAssemblyLineQueueRevision { .. }
        | MasterError::AssemblyLineRepositoryUnavailable
        | MasterError::AssemblyLineQueueFull
        | MasterError::AssemblyLineExecutionCapabilityUnavailable
        | MasterError::AssemblyLineExecutionControlUnavailable
        | MasterError::AssemblyLineExecutionReceiptMismatch => {
            fixed_error(StatusCode::CONFLICT, "assembly_line_request_rejected")
        }
        _ => internal_error(),
    }
}

fn assembly_line_execution_unavailable() -> ApiError {
    fixed_error(
        StatusCode::SERVICE_UNAVAILABLE,
        "assembly_line_execution_unavailable",
    )
}

fn assembly_line_effect_routes_unavailable() -> ApiError {
    fixed_error(StatusCode::NOT_FOUND, "not_found")
}

fn assembly_line_execution_projection(
    state: &AppState,
    process: &MasterProcess,
    now_ms: u64,
) -> Result<AssemblyLineOwnerProjection, ApiError> {
    process
        .kernel()
        .assembly_line_owner_projection_with_runtime(
            now_ms,
            current_planning_runtime_status(state),
            state.assembly_line_effect_dispatcher.runtime_status(),
        )
        .map_err(assembly_line_api_error)
}

fn execute_assembly_line_start(
    state: &AppState,
    request: &AssemblyLineStartRequest,
    registration: Option<&DeviceRegistration>,
) -> Result<(StatusCode, Json<AssemblyLineOwnerProjection>), ApiError> {
    if state
        .assembly_line_effect_dispatcher
        .runtime_status()
        .is_none()
    {
        return Err(assembly_line_execution_unavailable());
    }
    let now_ms = current_time_ms().map_err(api_error)?;
    let dispatch = {
        let mut process = lock_process(state)?;
        if let Some(registration) = registration {
            process
                .kernel_mut()
                .authorize_assembly_line_owner_bridge(registration)
                .map_err(|_| unauthorized())?;
        }
        let receipt = process
            .kernel_mut()
            .start_assembly_line(request, now_ms)
            .map_err(assembly_line_api_error)?;
        let dispatch = process
            .kernel_mut()
            .claim_assembly_line_start_dispatch(&receipt, now_ms)
            .map_err(assembly_line_api_error)?;
        dispatch
    };
    if let Some(dispatch) = dispatch {
        let receipts = state
            .assembly_line_effect_dispatcher
            .dispatch_start(&dispatch)
            .map_err(|_| assembly_line_execution_unavailable())?;
        let mut process = lock_process(state)?;
        for activation in receipts {
            process
                .kernel_mut()
                .record_assembly_line_activation_receipt(
                    &activation,
                    current_time_ms().map_err(api_error)?,
                )
                .map_err(assembly_line_api_error)?;
        }
    }
    let now_ms = current_time_ms().map_err(api_error)?;
    let process = lock_process(state)?;
    let projection = assembly_line_execution_projection(state, &process, now_ms)?;
    let status = if projection.assembly_line.lifecycle == AssemblyLineLifecycleState::Running {
        StatusCode::OK
    } else {
        StatusCode::ACCEPTED
    };
    Ok((status, Json(projection)))
}

fn execute_assembly_line_termination(
    state: &AppState,
    stop: Option<&AssemblyLineStopRequest>,
    pause: Option<&AssemblyLineEmergencyPauseRequest>,
    registration: Option<&DeviceRegistration>,
) -> Result<(StatusCode, Json<AssemblyLineOwnerProjection>), ApiError> {
    if stop.is_some() == pause.is_some() {
        return Err(assembly_line_request_rejected());
    }
    if state
        .assembly_line_effect_dispatcher
        .runtime_status()
        .is_none()
    {
        return Err(assembly_line_execution_unavailable());
    }
    let now_ms = current_time_ms().map_err(api_error)?;
    let intent = {
        let mut process = lock_process(state)?;
        if let Some(registration) = registration {
            process
                .kernel_mut()
                .authorize_assembly_line_owner_bridge(registration)
                .map_err(|_| unauthorized())?;
        }
        if let Some(request) = stop {
            process
                .kernel_mut()
                .stop_assembly_line(request, now_ms)
                .map_err(assembly_line_api_error)?
        } else {
            process
                .kernel_mut()
                .emergency_pause_assembly_line(pause.expect("checked pause request"), now_ms)
                .map_err(assembly_line_api_error)?
        }
    };
    let claimed = lock_process(state)?
        .kernel_mut()
        .claim_assembly_line_termination_dispatch(&intent, current_time_ms().map_err(api_error)?)
        .map_err(assembly_line_api_error)?;
    if claimed {
        let receipts = state
            .assembly_line_effect_dispatcher
            .dispatch_termination(&intent)
            .map_err(|_| assembly_line_execution_unavailable())?;
        let mut process = lock_process(state)?;
        for termination in receipts {
            process
                .kernel_mut()
                .record_assembly_line_termination_receipt(
                    intent.request_id,
                    &termination,
                    current_time_ms().map_err(api_error)?,
                )
                .map_err(assembly_line_api_error)?;
        }
    }
    let now_ms = current_time_ms().map_err(api_error)?;
    let process = lock_process(state)?;
    let pending = process
        .kernel()
        .assembly_line_termination_pending(intent.request_id)
        .map_err(assembly_line_api_error)?;
    let projection = assembly_line_execution_projection(state, &process, now_ms)?;
    Ok((
        if pending {
            StatusCode::ACCEPTED
        } else {
            StatusCode::OK
        },
        Json(projection),
    ))
}

async fn get_assembly_line_owner_projection(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<AssemblyLineOwnerProjection> {
    authorize(&headers, &state)?;
    let projection = lock_process(&state)?
        .kernel()
        .assembly_line_owner_projection_with_runtime(
            current_time_ms().map_err(api_error)?,
            current_planning_runtime_status(&state),
            state.assembly_line_effect_dispatcher.runtime_status(),
        )
        .map_err(assembly_line_api_error)?;
    Ok(Json(projection))
}

async fn record_assembly_line_project_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<AssemblyLineOwnerProjection> {
    authorize(&headers, &state)?;
    let body = assembly_line_body(body)?;
    let draft = ProjectBrainstormingDraft::decode_frame(&body)
        .map_err(|_| assembly_line_request_rejected())?;
    let now_ms = current_time_ms().map_err(api_error)?;
    let mut process = lock_process(&state)?;
    process
        .kernel_mut()
        .record_assembly_line_project_draft(&draft, now_ms)
        .map_err(assembly_line_api_error)?;
    Ok(Json(
        process
            .kernel()
            .assembly_line_owner_projection(now_ms)
            .map_err(assembly_line_api_error)?,
    ))
}

async fn record_assembly_line_feature_draft(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<AssemblyLineOwnerProjection> {
    authorize(&headers, &state)?;
    let body = assembly_line_body(body)?;
    let draft = FeatureBrainstormingDraft::decode_frame(&body)
        .map_err(|_| assembly_line_request_rejected())?;
    let now_ms = current_time_ms().map_err(api_error)?;
    let mut process = lock_process(&state)?;
    process
        .kernel_mut()
        .record_assembly_line_feature_draft(&draft, now_ms)
        .map_err(assembly_line_api_error)?;
    Ok(Json(
        process
            .kernel()
            .assembly_line_owner_projection(now_ms)
            .map_err(assembly_line_api_error)?,
    ))
}

struct PlanningCallCancellation {
    cancelled: Arc<AtomicBool>,
    completed: bool,
}

impl PlanningCallCancellation {
    fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
            completed: false,
        }
    }
}

impl Drop for PlanningCallCancellation {
    fn drop(&mut self) {
        if !self.completed {
            self.cancelled.store(true, Ordering::Release);
        }
    }
}

fn planning_effect_control(
    state: &AppState,
    cancelled: Arc<AtomicBool>,
) -> Result<PlanningEffectControl, ApiError> {
    let mut active = state
        .active_planning_calls
        .lock()
        .map_err(|_| internal_error())?;
    active.retain(|call| call.strong_count() != 0);
    let kernel = MasterKernel::open_planning_runtime_connection(&state.planning_database_path)
        .map_err(|_| planning_runtime_unavailable())?;
    let (paused, expected_revision) = kernel
        .planning_effect_pause_snapshot()
        .map_err(|_| planning_runtime_unavailable())?;
    if paused {
        return Err(planning_runtime_unavailable());
    }
    active.push(Arc::downgrade(&cancelled));
    let database_path = state.planning_database_path.clone();
    let authority_current = Arc::new(move || {
        MasterKernel::open_planning_runtime_connection(&database_path)
            .and_then(|kernel| kernel.planning_effect_pause_snapshot())
            .is_ok_and(|(paused, revision)| !paused && revision == expected_revision)
    });
    Ok(PlanningEffectControl::new(
        cancelled,
        Instant::now() + assemblywright_master::PLANNING_EFFECT_DEADLINE,
    )
    .with_authority(authority_current))
}

fn planning_runtime_unavailable() -> ApiError {
    fixed_error(
        StatusCode::SERVICE_UNAVAILABLE,
        "planning_runtime_unavailable",
    )
}

fn planning_effect_api_error(error: MasterError) -> ApiError {
    match error {
        MasterError::AssemblyLineBrainstormingRejected => {
            fixed_error(StatusCode::CONFLICT, "brainstorming_rejected")
        }
        MasterError::AssemblyLineBrainstormingUnavailable => fixed_error(
            StatusCode::CONFLICT,
            "brainstorming_reconciliation_required",
        ),
        MasterError::AssemblyLineGithubCreationConflict => {
            fixed_error(StatusCode::CONFLICT, "github_creation_conflict")
        }
        MasterError::AssemblyLineGithubCreationReconciliationRequired => fixed_error(
            StatusCode::CONFLICT,
            "github_creation_reconciliation_required",
        ),
        MasterError::AssemblyLineGithubCreationUnavailable => fixed_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "github_creation_unavailable",
        ),
        error => assembly_line_api_error(error),
    }
}

async fn run_planning_brainstorm(
    state: AppState,
    draft: BrainstormingDraft,
    authorization: BrainstormingCloudAuthorization,
    registration: Option<DeviceRegistration>,
) -> ApiResult<FrozenBrainstormingSpecification> {
    let runtime = state
        .planning_runtime
        .clone()
        .ok_or_else(planning_runtime_unavailable)?;
    let database_path = state.planning_database_path.clone();
    let mut cancellation = PlanningCallCancellation::new();
    let control = planning_effect_control(&state, cancellation.cancelled.clone())?;
    let result = tokio::task::spawn_blocking(move || {
        let mut kernel = MasterKernel::open_planning_runtime_connection(database_path)?;
        if let Some(registration) = registration.as_ref() {
            kernel.authorize_assembly_line_owner_bridge(registration)?;
        }
        if !control.poll() {
            return Err(MasterError::AssemblyLineBrainstormingUnavailable);
        }
        let mut runtime = runtime.lock().map_err(|_| {
            MasterError::InvalidStoredState("planning runtime lock is poisoned".to_string())
        })?;
        runtime.run_brainstorming(&mut kernel, draft, authorization, &control)
    })
    .await
    .map_err(|_| internal_error())?;
    cancellation.completed = true;
    Ok(Json(result.map_err(planning_effect_api_error)?))
}

async fn run_planning_github_creation(
    state: AppState,
    repository_id: Uuid,
    registration: Option<DeviceRegistration>,
) -> ApiResult<RepositoryCreationProjection> {
    let runtime = state
        .planning_runtime
        .clone()
        .ok_or_else(planning_runtime_unavailable)?;
    let database_path = state.planning_database_path.clone();
    let mut cancellation = PlanningCallCancellation::new();
    let control = planning_effect_control(&state, cancellation.cancelled.clone())?;
    let result = tokio::task::spawn_blocking(move || {
        let mut kernel = MasterKernel::open_planning_runtime_connection(database_path)?;
        if let Some(registration) = registration.as_ref() {
            kernel.authorize_assembly_line_owner_bridge(registration)?;
        }
        if !control.poll() {
            return Err(MasterError::AssemblyLineGithubCreationUnavailable);
        }
        let mut runtime = runtime.lock().map_err(|_| {
            MasterError::InvalidStoredState("planning runtime lock is poisoned".to_string())
        })?;
        runtime.run_github_creation(&mut kernel, repository_id, &control)
    })
    .await
    .map_err(|_| internal_error())?;
    cancellation.completed = true;
    Ok(Json(result.map_err(planning_effect_api_error)?))
}

async fn run_assembly_line_project_brainstorm(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<FrozenBrainstormingSpecification> {
    authorize(&headers, &state)?;
    let body = assembly_line_body(body)?;
    let request = ProjectBrainstormingCloudRequest::decode_frame(&body)
        .map_err(|_| assembly_line_request_rejected())?;
    run_planning_brainstorm(
        state,
        BrainstormingDraft::Project(request.draft),
        BrainstormingCloudAuthorization {
            owner_cloud_disclosure_sha256: request.owner_cloud_disclosure_sha256,
        },
        None,
    )
    .await
}

async fn run_assembly_line_feature_brainstorm(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<FrozenBrainstormingSpecification> {
    authorize(&headers, &state)?;
    let body = assembly_line_body(body)?;
    let request = FeatureBrainstormingCloudRequest::decode_frame(&body)
        .map_err(|_| assembly_line_request_rejected())?;
    run_planning_brainstorm(
        state,
        BrainstormingDraft::Feature(request.draft),
        BrainstormingCloudAuthorization {
            owner_cloud_disclosure_sha256: request.owner_cloud_disclosure_sha256,
        },
        None,
    )
    .await
}

async fn run_assembly_line_github_creation(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(repository_id): AxumPath<String>,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<RepositoryCreationProjection> {
    authorize(&headers, &state)?;
    let body = assembly_line_body(body)?;
    if !body.is_empty() {
        return Err(assembly_line_request_rejected());
    }
    let repository_id = Uuid::parse_str(&repository_id)
        .ok()
        .filter(|identifier| !identifier.is_nil())
        .ok_or_else(assembly_line_request_rejected)?;
    run_planning_github_creation(state, repository_id, None).await
}

async fn record_assembly_line_frozen_specification(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<AssemblyLineOwnerProjection> {
    authorize(&headers, &state)?;
    let body = assembly_line_body(body)?;
    let frozen = FrozenBrainstormingSpecification::decode_frame(&body)
        .map_err(|_| assembly_line_request_rejected())?;
    let now_ms = current_time_ms().map_err(api_error)?;
    let mut process = lock_process(&state)?;
    process
        .kernel_mut()
        .record_assembly_line_frozen_specification(&frozen, now_ms)
        .map_err(assembly_line_api_error)?;
    Ok(Json(
        process
            .kernel()
            .assembly_line_owner_projection(now_ms)
            .map_err(assembly_line_api_error)?,
    ))
}

async fn approve_assembly_line_project(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<RepositoryCreationProjection> {
    authorize(&headers, &state)?;
    let body = assembly_line_body(body)?;
    let approval = BrainstormingOwnerApprovalBinding::decode_frame(&body)
        .map_err(|_| assembly_line_request_rejected())?;
    let projection = lock_process(&state)?
        .kernel_mut()
        .approve_assembly_line_project(&approval, current_time_ms().map_err(api_error)?)
        .map_err(assembly_line_api_error)?;
    Ok(Json(projection))
}

async fn approve_assembly_line_feature(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<FeatureQueueEntryProjection> {
    authorize(&headers, &state)?;
    let body = assembly_line_body(body)?;
    let approval = BrainstormingOwnerApprovalBinding::decode_frame(&body)
        .map_err(|_| assembly_line_request_rejected())?;
    let runtime = state
        .planning_runtime
        .as_ref()
        .ok_or_else(planning_runtime_unavailable)?
        .clone();
    let mut process = lock_process(&state)?;
    let projection = runtime
        .lock()
        .map_err(|_| planning_runtime_unavailable())?
        .approve_feature_and_enqueue(
            process.kernel_mut(),
            &approval,
            current_time_ms().map_err(api_error)?,
        )
        .map_err(planning_effect_api_error)?;
    Ok(Json(projection))
}

async fn set_assembly_line_auto_run(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<AssemblyLineAutoRunReceipt> {
    authorize(&headers, &state)?;
    let body = assembly_line_body(body)?;
    let request = AssemblyLineAutoRunRequest::decode_frame(&body)
        .map_err(|_| assembly_line_request_rejected())?;
    let receipt = lock_process(&state)?
        .kernel_mut()
        .set_assembly_line_auto_run(&request, current_time_ms().map_err(api_error)?)
        .map_err(assembly_line_api_error)?;
    Ok(Json(receipt))
}

async fn start_assembly_line(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<(StatusCode, Json<AssemblyLineOwnerProjection>), ApiError> {
    authorize(&headers, &state)?;
    if state
        .assembly_line_effect_dispatcher
        .runtime_status()
        .is_none()
    {
        return Err(assembly_line_effect_routes_unavailable());
    }
    let request = AssemblyLineStartRequest::decode_frame(&assembly_line_body(body)?)
        .map_err(|_| assembly_line_request_rejected())?;
    execute_assembly_line_start(&state, &request, None)
}

async fn stop_assembly_line(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<(StatusCode, Json<AssemblyLineOwnerProjection>), ApiError> {
    authorize(&headers, &state)?;
    if state
        .assembly_line_effect_dispatcher
        .runtime_status()
        .is_none()
    {
        return Err(assembly_line_effect_routes_unavailable());
    }
    let request = AssemblyLineStopRequest::decode_frame(&assembly_line_body(body)?)
        .map_err(|_| assembly_line_request_rejected())?;
    execute_assembly_line_termination(&state, Some(&request), None, None)
}

async fn emergency_pause_assembly_line(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<(StatusCode, Json<AssemblyLineOwnerProjection>), ApiError> {
    authorize(&headers, &state)?;
    if state
        .assembly_line_effect_dispatcher
        .runtime_status()
        .is_none()
    {
        return Err(assembly_line_effect_routes_unavailable());
    }
    let request = AssemblyLineEmergencyPauseRequest::decode_frame(&assembly_line_body(body)?)
        .map_err(|_| assembly_line_request_rejected())?;
    execute_assembly_line_termination(&state, None, Some(&request), None)
}

async fn designate_owner_control_bridge(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<FeatureConveyorOwnerBridgeDesignationReceipt> {
    authorize(&headers, &state)?;
    let body = body.map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "owner_control_designation_request_rejected",
        )
    })?;
    let request =
        FeatureConveyorOwnerBridgeDesignationRequest::decode_frame(&body).map_err(|_| {
            fixed_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "owner_control_designation_request_rejected",
            )
        })?;
    let designation = lock_process(&state)?
        .kernel_mut()
        .designate_owner_control_bridge(
            request.device_id,
            request.expected_designation_revision,
            current_time_ms().map_err(|_| {
                fixed_error(StatusCode::CONFLICT, "owner_control_designation_rejected")
            })?,
        )
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "owner_control_designation_rejected"))?;
    Ok(Json(FeatureConveyorOwnerBridgeDesignationReceipt {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        device_id: designation.device_id,
        registry_revision: designation.registry_revision,
        designation_revision: designation.designation_revision,
        status: FeatureConveyorOwnerBridgeDesignationStatus::Designated,
    }))
}

async fn get_feature_activation_evidence_admission_projection(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<FeatureConveyorActivationEvidenceAdmissionProjection> {
    authorize(&headers, &state)?;
    let projection = lock_process(&state)?
        .kernel_mut()
        .feature_conveyor_activation_evidence_admission_projection()
        .map_err(api_error)?;
    Ok(Json(projection))
}

async fn admit_feature_activation_evidence(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<FeatureConveyorActivationEvidenceAdmissionReceipt> {
    authorize(&headers, &state)?;
    let body = body.map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "feature_activation_evidence_request_rejected",
        )
    })?;
    let request =
        FeatureConveyorActivationEvidenceAdmissionRequest::decode_frame(&body).map_err(|_| {
            fixed_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "feature_activation_evidence_request_rejected",
            )
        })?;
    let receipt = lock_process(&state)?
        .kernel_mut()
        .admit_feature_activation_evidence(
            &request,
            current_time_ms().map_err(|_| {
                fixed_error(StatusCode::CONFLICT, "feature_activation_evidence_rejected")
            })?,
        )
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "feature_activation_evidence_rejected"))?;
    Ok(Json(receipt))
}

async fn record_repository_grant(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<FeatureConveyorRepositoryGrantReceipt> {
    authorize(&headers, &state)?;
    let body = body.map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "repository_grant_request_rejected",
        )
    })?;
    let request = FeatureConveyorRepositoryGrantRequest::decode_frame(&body).map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "repository_grant_request_rejected",
        )
    })?;
    let grant = RepositoryGrantRevision {
        repository_id: request.grant.repository_id,
        kind: match request.grant.kind {
            FeatureConveyorRepositoryGrantKind::Registration => RepositoryGrantKind::Registration,
            FeatureConveyorRepositoryGrantKind::CloudDisclosure => {
                RepositoryGrantKind::CloudDisclosure
            }
            FeatureConveyorRepositoryGrantKind::AutonomousPublication => {
                RepositoryGrantKind::AutonomousPublication
            }
        },
        revision: request.grant.revision,
        scope_sha256: request.grant.scope_sha256,
        owner_approval_sha256: request.grant.owner_approval_sha256,
        expires_at_ms: request.grant.expires_at_ms,
        revoked: request.grant.revoked,
    };
    let now_ms = current_time_ms()
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "repository_grant_recording_rejected"))?;
    lock_process(&state)?
        .kernel_mut()
        .record_repository_grant_revision(
            &grant,
            request.expected_current_revision,
            request.expected_emergency_pause_revision,
            now_ms,
        )
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "repository_grant_recording_rejected"))?;
    Ok(Json(FeatureConveyorRepositoryGrantReceipt {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        repository_id: request.grant.repository_id,
        kind: request.grant.kind,
        revision: request.grant.revision,
        scope_sha256: request.grant.scope_sha256,
        owner_approval_sha256: request.grant.owner_approval_sha256,
        expires_at_ms: request.grant.expires_at_ms,
        revoked: request.grant.revoked,
        emergency_pause_revision: request.expected_emergency_pause_revision,
        status: FeatureConveyorRepositoryGrantStatus::Recorded,
    }))
}

async fn repository_preflight(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<FeatureConveyorRepositoryPreflightReceipt> {
    authorize(&headers, &state)?;
    let body = body.map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "repository_preflight_request_rejected",
        )
    })?;
    let request = FeatureConveyorRepositoryPreflightRequest::decode_frame(&body).map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "repository_preflight_request_rejected",
        )
    })?;
    let before_ms = current_time_ms()
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "repository_preflight_rejected"))?;
    lock_process(&state)?
        .kernel()
        .authorize_repository_preflight(
            request.scope.repository_id,
            request.registration_grant_revision,
            &request.scope_sha256,
            request.expected_emergency_pause_revision,
            before_ms,
        )
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "repository_preflight_rejected"))?;
    let repository_path = request.scope.repository_path.clone();
    let expected_base_branch = request.scope.expected_base_branch.clone();
    let expected_head_commit = request.scope.expected_head_commit.clone();
    // Each blocking filesystem-observation await is independently bounded to
    // five seconds. The grant precheck above and atomic grant/pause/audit
    // recheck below intentionally remain outside these timeouts rather than
    // converting database-lock contention into filesystem-timeout semantics.
    let observation_guard = tokio::time::timeout(
        REPOSITORY_FILESYSTEM_OBSERVATION_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            observe_standard_repository_identity(
                &repository_path,
                &expected_base_branch,
                &expected_head_commit,
            )
        }),
    )
    .await
    .map_err(|_| fixed_error(StatusCode::CONFLICT, "repository_preflight_rejected"))?
    .map_err(|_| fixed_error(StatusCode::CONFLICT, "repository_preflight_rejected"))?
    .map_err(|_| fixed_error(StatusCode::CONFLICT, "repository_preflight_rejected"))?;
    let observation_guard = tokio::time::timeout(
        REPOSITORY_FILESYSTEM_OBSERVATION_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            let mut observation_guard = observation_guard;
            observation_guard.revalidate()?;
            Ok::<_, ()>(observation_guard)
        }),
    )
    .await
    .map_err(|_| fixed_error(StatusCode::CONFLICT, "repository_preflight_rejected"))?
    .map_err(|_| fixed_error(StatusCode::CONFLICT, "repository_preflight_rejected"))?
    .map_err(|_| fixed_error(StatusCode::CONFLICT, "repository_preflight_rejected"))?;
    let observed_at_ms = current_time_ms()
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "repository_preflight_rejected"))?;
    lock_process(&state)?
        .kernel_mut()
        .record_repository_preflight(
            request.scope.repository_id,
            request.registration_grant_revision,
            &request.scope_sha256,
            request.expected_emergency_pause_revision,
            observed_at_ms,
        )
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "repository_preflight_rejected"))?;
    let preflight_fingerprint_sha256 = repository_preflight_fingerprint_sha256(
        request.scope.repository_id,
        request.registration_grant_revision,
        &request.scope_sha256,
        request.expected_emergency_pause_revision,
        &request.scope.expected_base_branch,
        &request.scope.expected_head_commit,
        observed_at_ms,
    );
    let receipt = FeatureConveyorRepositoryPreflightReceipt {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        repository_id: request.scope.repository_id,
        registration_grant_revision: request.registration_grant_revision,
        scope_sha256: request.scope_sha256,
        emergency_pause_revision: request.expected_emergency_pause_revision,
        base_branch: request.scope.expected_base_branch,
        head_commit: request.scope.expected_head_commit,
        preflight_fingerprint_sha256,
        observed_at_ms,
        status: FeatureConveyorRepositoryPreflightStatus::IdentityEligible,
    };
    receipt
        .validate()
        .map_err(|_| fixed_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"))?;
    drop(observation_guard);
    Ok(Json(receipt))
}

async fn repository_snapshot_claim(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<FeatureConveyorRepositorySnapshotClaimReceipt> {
    authorize(&headers, &state)?;
    let body = body.map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "repository_snapshot_claim_request_rejected",
        )
    })?;
    let request =
        FeatureConveyorRepositorySnapshotClaimRequest::decode_frame(&body).map_err(|_| {
            fixed_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "repository_snapshot_claim_request_rejected",
            )
        })?;
    let reservation = state
        .repository_snapshot_claim_reservation
        .clone()
        .try_lock_owned()
        .map_err(|_| {
            fixed_error(
                StatusCode::CONFLICT,
                "repository_snapshot_claim_in_progress",
            )
        })?;
    let plan = FeatureSnapshotClaimPlan {
        feature_id: request.expected_feature_id,
        specification_revision: request.expected_specification_revision,
        repository_id: request.scope.repository_id,
        expected_queue_revision: request.expected_queue_revision,
        expected_emergency_pause_revision: request.expected_emergency_pause_revision,
        scope_sha256: request.scope_sha256,
        provider_id: request.provider_id.clone(),
        model_id: request.model_id.clone(),
        grants: FeatureGrantRevisions {
            registration: request.grants.registration,
            cloud_disclosure: request.grants.cloud_disclosure,
            autonomous_publication: request.grants.autonomous_publication,
        },
        base_commit: request.scope.expected_head_commit.clone(),
    };
    let before_ms = current_time_ms()
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "repository_snapshot_claim_rejected"))?;
    let data_dir = {
        let mut process = lock_process(&state)?;
        process
            .kernel_mut()
            .prepare_repository_snapshot_claim(&plan, before_ms)
            .map_err(|_| fixed_error(StatusCode::CONFLICT, "repository_snapshot_claim_rejected"))?;
        process.data_dir().to_path_buf()
    };
    let repository_path = request.scope.repository_path.clone();
    let expected_base_branch = request.scope.expected_base_branch.clone();
    let expected_head_commit = request.scope.expected_head_commit.clone();
    let source_path = PathBuf::from(&request.scope.repository_path);
    let base_commit = request.scope.expected_head_commit.clone();
    let (reservation, observation_guard, prepared) = tokio::time::timeout(
        REPOSITORY_SNAPSHOT_CLAIM_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            let reservation = reservation;
            let mut observation_guard = observe_standard_repository_identity(
                &repository_path,
                &expected_base_branch,
                &expected_head_commit,
            )?;
            observation_guard.revalidate()?;
            let store = RepositorySnapshotStore::open(&data_dir).map_err(|_| ())?;
            let prepared = store.prepare(&source_path, &base_commit).map_err(|_| ())?;
            observation_guard.revalidate()?;
            Ok::<_, ()>((reservation, observation_guard, prepared))
        }),
    )
    .await
    .map_err(|_| fixed_error(StatusCode::CONFLICT, "repository_snapshot_claim_rejected"))?
    .map_err(|_| fixed_error(StatusCode::CONFLICT, "repository_snapshot_claim_rejected"))?
    .map_err(|_| fixed_error(StatusCode::CONFLICT, "repository_snapshot_claim_rejected"))?;
    let (reservation, observation_guard, prepared) = tokio::time::timeout(
        REPOSITORY_FILESYSTEM_OBSERVATION_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            let reservation = reservation;
            let mut observation_guard = observation_guard;
            observation_guard.revalidate()?;
            Ok::<_, ()>((reservation, observation_guard, prepared))
        }),
    )
    .await
    .map_err(|_| fixed_error(StatusCode::CONFLICT, "repository_snapshot_claim_rejected"))?
    .map_err(|_| fixed_error(StatusCode::CONFLICT, "repository_snapshot_claim_rejected"))?
    .map_err(|_| fixed_error(StatusCode::CONFLICT, "repository_snapshot_claim_rejected"))?;
    let finalized_at_ms = current_time_ms()
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "repository_snapshot_claim_rejected"))?;
    let snapshot = RepositorySnapshotEvidence {
        snapshot_id: prepared.snapshot_id,
        snapshot_sha256: prepared.snapshot_sha256,
        base_commit: prepared.base_commit.clone(),
    };
    let claim = lock_process(&state)?
        .kernel_mut()
        .finalize_repository_snapshot_claim(&plan, &snapshot, finalized_at_ms)
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "repository_snapshot_claim_rejected"))?;
    drop(observation_guard);
    prepared.retain();
    drop(reservation);
    let receipt = FeatureConveyorRepositorySnapshotClaimReceipt {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        feature_id: claim.feature_id,
        specification_revision: claim.specification_revision,
        lifecycle_revision: claim.lifecycle_revision,
        queue_revision: plan.expected_queue_revision + 1,
        emergency_pause_revision: plan.expected_emergency_pause_revision,
        lease_id: claim.lease_id,
        snapshot_id: claim.snapshot_id,
        snapshot_sha256: claim.snapshot_sha256,
        base_commit: claim.base_commit,
        grants: request.grants,
        provider_binding_sha256: feature_conveyor_provider_binding_sha256(
            &claim.provider_id,
            &claim.model_id,
        ),
        status: FeatureConveyorRepositorySnapshotClaimStatus::SnapshotBound,
    };
    receipt
        .validate()
        .map_err(|_| fixed_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"))?;
    Ok(Json(receipt))
}

async fn feature_coding_dispatch(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<FeatureConveyorCodingDispatchReceipt> {
    authorize(&headers, &state)?;
    let body = body.map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "feature_coding_dispatch_request_rejected",
        )
    })?;
    let request = FeatureConveyorCodingDispatchRequest::decode_frame(&body).map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "feature_coding_dispatch_request_rejected",
        )
    })?;
    let receipt = lock_process(&state)?
        .kernel_mut()
        .dispatch_feature_coding(
            &request,
            current_time_ms().map_err(|_| {
                fixed_error(StatusCode::CONFLICT, "feature_coding_dispatch_rejected")
            })?,
        )
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "feature_coding_dispatch_rejected"))?;
    receipt
        .validate()
        .map_err(|_| fixed_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"))?;
    Ok(Json(receipt))
}

async fn feature_artifact_integration(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, ApiError> {
    authorize(&headers, &state)?;
    require_work_admission(&state)?;
    let body = body.map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "artifact_integration_request_rejected",
        )
    })?;
    let request = FeatureConveyorArtifactIntegrationRequest::decode_frame(&body).map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "artifact_integration_request_rejected",
        )
    })?;
    let reservation = state
        .artifact_integration_reservation
        .clone()
        .try_lock_owned()
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "artifact_integration_in_progress"))?;
    let state_for_work = state.clone();
    let work = spawn_reserved_blocking(reservation, move || {
        perform_feature_artifact_integration(&state_for_work, request)
    });
    work.await
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "artifact_integration_rejected"))?
}

async fn feature_validation_gate(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, ApiError> {
    authorize(&headers, &state)?;
    require_work_admission(&state)?;
    let body = body.map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation_gate_request_rejected",
        )
    })?;
    let request = FeatureConveyorValidationGateRequest::decode_frame(&body).map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation_gate_request_rejected",
        )
    })?;
    let reservation = state
        .validation_gate_reservation
        .clone()
        .try_lock_owned()
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "validation_gate_in_progress"))?;
    let state_for_work = state.clone();
    spawn_reserved_blocking(reservation, move || {
        perform_feature_validation_gate(&state_for_work, request)
    })
    .await
    .map_err(|_| fixed_error(StatusCode::CONFLICT, "validation_gate_rejected"))?
}

fn perform_feature_validation_gate(
    state: &AppState,
    request: FeatureConveyorValidationGateRequest,
) -> Result<Response, ApiError> {
    let authorization = lock_process(state)?
        .kernel_mut()
        .prepare_validation_gate(
            &request,
            current_time_ms()
                .map_err(|_| fixed_error(StatusCode::CONFLICT, "validation_gate_rejected"))?,
        )
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "validation_gate_rejected"))?;
    let plan = match authorization {
        ValidationGateAuthorization::ExistingPassed { receipt, candidate } => {
            let store = lock_process(state)?.artifact_integration_store();
            store
                .open_verified_candidate(&candidate)
                .map_err(|_| fixed_error(StatusCode::CONFLICT, "validation_gate_rejected"))?;
            receipt
                .validate()
                .map_err(|_| fixed_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"))?;
            return Ok(Json(receipt).into_response());
        }
        ValidationGateAuthorization::ExistingFailed => {
            return Err(fixed_error(StatusCode::CONFLICT, "validation_gate_failed"));
        }
        ValidationGateAuthorization::Planned(plan) => plan,
    };
    let toolchain = state
        .validation_runtime
        .toolchain()
        .ok_or_else(|| fixed_error(StatusCode::CONFLICT, "validation_runner_unavailable"))?;
    toolchain
        .revalidate()
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "validation_runner_unavailable"))?;
    let store = lock_process(state)?.artifact_integration_store();
    let mut candidate = store
        .open_verified_candidate(&plan.candidate)
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "validation_gate_rejected"))?;
    let mut scratch = store
        .prepare_validation_copy(&plan.candidate)
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "validation_gate_rejected"))?;
    VerifiedValidationCopy::verify(
        scratch.root(),
        &plan.candidate.candidate_commit,
        &plan.candidate.candidate_tree,
    )
    .map_err(|_| fixed_error(StatusCode::CONFLICT, "validation_gate_rejected"))?;
    scratch
        .verify_after(&store)
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "validation_gate_rejected"))?;
    let persisted = lock_process(state)?
        .kernel_mut()
        .plan_validation_gate(
            &request,
            current_time_ms()
                .map_err(|_| fixed_error(StatusCode::CONFLICT, "validation_gate_rejected"))?,
        )
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "validation_gate_rejected"))?;
    let persisted = match persisted {
        ValidationGateAuthorization::Planned(persisted) if persisted == plan => persisted,
        _ => {
            return Err(fixed_error(
                StatusCode::CONFLICT,
                "validation_gate_rejected",
            ))
        }
    };
    let evidence = execute_validation_plan(state, &persisted, scratch)?;
    candidate
        .revalidate(&store)
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "validation_gate_rejected"))?;
    let receipt = lock_process(state)?
        .kernel_mut()
        .finalize_validation_gate(
            &plan,
            &evidence,
            current_time_ms()
                .map_err(|_| fixed_error(StatusCode::CONFLICT, "validation_gate_rejected"))?,
        )
        .map_err(|error| match error {
            assemblywright_master::MasterError::ValidationGateFailed => {
                fixed_error(StatusCode::CONFLICT, "validation_gate_failed")
            }
            _ => fixed_error(StatusCode::CONFLICT, "validation_gate_rejected"),
        })?;
    receipt
        .validate()
        .map_err(|_| fixed_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"))?;
    Ok(Json(receipt).into_response())
}

fn execute_validation_plan(
    state: &AppState,
    plan: &ValidationGateExecutionPlan,
    mut scratch: assemblywright_master::ValidationCandidateScratch,
) -> Result<ValidationGateEvidence, ApiError> {
    let toolchain = state
        .validation_runtime
        .toolchain()
        .ok_or_else(|| fixed_error(StatusCode::CONFLICT, "validation_runner_unavailable"))?;
    let store = lock_process(state)?.artifact_integration_store();
    let candidate = VerifiedValidationCopy::verify(
        scratch.root(),
        &plan.candidate.candidate_commit,
        &plan.candidate.candidate_tree,
    )
    .map_err(|_| fixed_error(StatusCode::CONFLICT, "validation_gate_rejected"))?;

    let cancelled = Arc::new(AtomicBool::new(false));
    let monitor_done = Arc::new(AtomicBool::new(false));
    let monitor_state = state.clone();
    let monitor_request = plan.request.clone();
    let monitor_cancelled = cancelled.clone();
    let monitor_finished = monitor_done.clone();
    let monitor = std::thread::spawn(move || {
        while !monitor_finished.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(25));
            if monitor_finished.load(Ordering::Acquire) {
                break;
            }
            let current = current_time_ms()
                .ok()
                .and_then(|now_ms| {
                    monitor_state.process.lock().ok().and_then(|process| {
                        process
                            .kernel()
                            .validation_gate_execution_is_current(&monitor_request, now_ms)
                            .ok()
                    })
                })
                .unwrap_or(false);
            if !current {
                monitor_cancelled.store(true, Ordering::Release);
                break;
            }
        }
    });
    let cancellation = ValidationCancellation::new(cancelled.clone());
    let execution = (|| {
        let mut commands = Vec::with_capacity(plan.request.command_ids.len());
        for command_id in &plan.request.command_ids {
            let still_current = lock_process(state)?
                .kernel()
                .validation_gate_execution_is_current(
                    &plan.request,
                    current_time_ms().map_err(|_| {
                        fixed_error(StatusCode::CONFLICT, "validation_gate_rejected")
                    })?,
                )
                .map_err(|_| fixed_error(StatusCode::CONFLICT, "validation_gate_rejected"))?;
            if !still_current || cancelled.load(Ordering::Acquire) {
                return Err(fixed_error(
                    StatusCode::CONFLICT,
                    "validation_gate_cancelled",
                ));
            }
            let started = Instant::now();
            let evidence = match validation_command_execution(*command_id) {
                ValidationCommandExecution::InternalDeterministicCheck => {
                    let result = run_internal_validation_check(
                        *command_id,
                        &candidate,
                        &plan.candidate.base_commit,
                        &plan.approved_paths,
                        plan.acceptance_criteria_count,
                        plan.requirements_sha256,
                    )
                    .map_err(|_| fixed_error(StatusCode::CONFLICT, "validation_gate_rejected"))?;
                    ValidationCommandEvidence {
                        command_id: result.command_id,
                        passed: result.passed,
                        result_sha256: result.result_sha256,
                        duration_ms: u64::try_from(started.elapsed().as_millis())
                            .unwrap_or(u64::MAX),
                        output_truncated: false,
                    }
                }
                ValidationCommandExecution::ContainedProcess => {
                    let result = run_validation_command(
                        *command_id,
                        &candidate,
                        toolchain,
                        Duration::from_secs(30 * 60),
                        &cancellation,
                    )
                    .map_err(|_| fixed_error(StatusCode::CONFLICT, "validation_gate_rejected"))?;
                    let mut digest = Sha256::new();
                    digest.update(b"assemblywright.contained-validation-result.v1\0");
                    digest.update([
                        assemblywright_protocol::FEATURE_CONVEYOR_MINIMUM_LINE_COVERAGE_PERCENT,
                    ]);
                    digest.update(serde_json::to_vec(command_id).map_err(|_| {
                        fixed_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
                    })?);
                    digest.update(result.exit_code.to_be_bytes());
                    digest.update((result.stdout_len as u64).to_be_bytes());
                    digest.update(result.stdout_sha256);
                    digest.update((result.stderr_len as u64).to_be_bytes());
                    digest.update(result.stderr_sha256);
                    digest.update([u8::from(result.timed_out)]);
                    ValidationCommandEvidence {
                        command_id: *command_id,
                        passed: result.exit_code == 0 && !result.timed_out,
                        result_sha256: digest.finalize().into(),
                        duration_ms: u64::try_from(started.elapsed().as_millis())
                            .unwrap_or(u64::MAX),
                        output_truncated: false,
                    }
                }
                ValidationCommandExecution::ExternalPlatformEvidence => {
                    return Err(fixed_error(
                        StatusCode::CONFLICT,
                        "validation_runner_unavailable",
                    ));
                }
            };
            scratch
                .verify_after(&store)
                .map_err(|_| fixed_error(StatusCode::CONFLICT, "validation_gate_rejected"))?;
            commands.push(evidence);
        }
        scratch
            .finish(&store)
            .map_err(|_| fixed_error(StatusCode::CONFLICT, "validation_gate_rejected"))?;
        Ok(ValidationGateEvidence { commands })
    })();
    monitor_done.store(true, Ordering::Release);
    if monitor.join().is_err() {
        return Err(fixed_error(
            StatusCode::CONFLICT,
            "validation_gate_rejected",
        ));
    }
    execution
}

async fn feature_review_gateway(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, ApiError> {
    authorize(&headers, &state)?;
    require_work_admission(&state)?;
    let body = body.map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "review_gateway_request_rejected",
        )
    })?;
    let request = FeatureConveyorReviewGatewayRequest::decode_frame(&body).map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "review_gateway_request_rejected",
        )
    })?;
    let reservation = state
        .review_gateway_reservation
        .clone()
        .try_lock_owned()
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "review_gateway_in_progress"))?;
    let state_for_work = state.clone();
    spawn_reserved_blocking(reservation, move || {
        perform_feature_review_gateway(&state_for_work, request)
    })
    .await
    .map_err(|_| fixed_error(StatusCode::CONFLICT, "review_gateway_rejected"))?
}

async fn feature_publication(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> Result<Response, ApiError> {
    authorize(&headers, &state)?;
    require_work_admission(&state)?;
    let body = body.map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "publication_request_rejected",
        )
    })?;
    let request = FeatureConveyorPublicationRequest::decode_frame(&body).map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "publication_request_rejected",
        )
    })?;
    let reservation = state
        .publication_reservation
        .clone()
        .try_lock_owned()
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "publication_in_progress"))?;
    let state_for_work = state.clone();
    spawn_reserved_blocking(reservation, move || {
        perform_feature_publication(&state_for_work, request)
    })
    .await
    .map_err(|_| fixed_error(StatusCode::CONFLICT, "publication_rejected"))?
}

fn perform_feature_publication(
    state: &AppState,
    request: FeatureConveyorPublicationRequest,
) -> Result<Response, ApiError> {
    let authorization = lock_process(state)?
        .kernel_mut()
        .prepare_publication(
            &request,
            current_time_ms()
                .map_err(|_| fixed_error(StatusCode::CONFLICT, "publication_rejected"))?,
        )
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "publication_rejected"))?;
    let plan = match authorization {
        assemblywright_master::PublicationAuthorization::Existing(receipt) => {
            return Ok(Json(*receipt).into_response())
        }
        assemblywright_master::PublicationAuthorization::Planned(plan) => *plan,
    };
    let runtime = state.github_publication.as_ref().ok_or_else(|| {
        fixed_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "publication_adapter_unavailable",
        )
    })?;
    let (store, candidate) = {
        let process = lock_process(state)?;
        let candidate = process
            .kernel()
            .candidate_references()
            .map_err(|_| fixed_error(StatusCode::CONFLICT, "publication_rejected"))?
            .into_iter()
            .find(|candidate| candidate.integration_id == request.integration_id)
            .ok_or_else(|| fixed_error(StatusCode::CONFLICT, "publication_rejected"))?;
        (process.artifact_integration_store(), candidate)
    };
    let mut adapter = runtime
        .bind_candidate(&plan, store, candidate)
        .map_err(|_| {
            fixed_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "publication_adapter_unavailable",
            )
        })?;
    if !adapter.is_available() {
        return Err(fixed_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "publication_adapter_unavailable",
        ));
    }
    runtime.preflight_for_plan(&plan).map_err(|_| {
        fixed_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "publication_credential_reauthentication_required",
        )
    })?;
    lock_process(state)?
        .kernel_mut()
        .begin_publication(
            &plan,
            current_time_ms()
                .map_err(|_| fixed_error(StatusCode::CONFLICT, "publication_rejected"))?,
        )
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "publication_rejected"))?;

    for action in assemblywright_master::PublicationActionKind::ORDERED {
        let current = lock_process(state)?
            .kernel()
            .publication_execution_is_current(
                &request,
                action,
                current_time_ms().map_err(|_| {
                    fixed_error(StatusCode::CONFLICT, "publication_effect_ambiguous")
                })?,
            )
            .unwrap_or(false);
        if !current {
            quarantine_publication(state, &plan, action)?;
            return Err(fixed_error(
                StatusCode::CONFLICT,
                "publication_effect_ambiguous",
            ));
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        let monitor_state = state.clone();
        let monitor_request = request.clone();
        let control = PublicationExecutionControl::new(
            cancelled,
            Instant::now() + assemblywright_master::GITHUB_PUBLICATION_ACTION_DEADLINE,
            Arc::new(move || {
                let (maintenance, _) = monitor_state.lifecycle.maintenance_snapshot();
                !maintenance
                    && current_time_ms()
                        .ok()
                        .and_then(|now_ms| {
                            monitor_state.process.lock().ok().and_then(|process| {
                                process
                                    .kernel()
                                    .publication_execution_is_current(
                                        &monitor_request,
                                        action,
                                        now_ms,
                                    )
                                    .ok()
                            })
                        })
                        .unwrap_or(false)
            }),
        );
        let evidence = match adapter.execute(&plan, action, &control) {
            Ok(evidence) => evidence,
            Err(_) => {
                quarantine_publication(state, &plan, action)?;
                return Err(fixed_error(
                    StatusCode::CONFLICT,
                    "publication_effect_ambiguous",
                ));
            }
        };
        if control.poll().is_err() {
            quarantine_publication(state, &plan, action)?;
            return Err(fixed_error(
                StatusCode::CONFLICT,
                "publication_effect_ambiguous",
            ));
        }
        let completion = lock_process(state)?
            .kernel_mut()
            .complete_publication_action(
                &plan,
                &evidence,
                current_time_ms().map_err(|_| {
                    fixed_error(StatusCode::CONFLICT, "publication_effect_ambiguous")
                })?,
            );
        match completion {
            Ok(Some(receipt)) => {
                receipt.validate().map_err(|_| {
                    fixed_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
                })?;
                return Ok(Json(receipt).into_response());
            }
            Ok(None) => {}
            Err(_) => {
                quarantine_publication(state, &plan, action)?;
                return Err(fixed_error(
                    StatusCode::CONFLICT,
                    "publication_effect_ambiguous",
                ));
            }
        }
    }
    quarantine_publication(
        state,
        &plan,
        assemblywright_master::PublicationActionKind::RunPostMergeGate,
    )?;
    Err(fixed_error(
        StatusCode::CONFLICT,
        "publication_effect_ambiguous",
    ))
}

fn quarantine_publication(
    state: &AppState,
    plan: &assemblywright_master::PublicationExecutionPlan,
    action: assemblywright_master::PublicationActionKind,
) -> Result<(), ApiError> {
    lock_process(state)?
        .kernel_mut()
        .quarantine_ambiguous_publication(
            plan,
            action,
            current_time_ms()
                .map_err(|_| fixed_error(StatusCode::CONFLICT, "publication_effect_ambiguous"))?,
        )
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "publication_effect_ambiguous"))
}

fn perform_feature_review_gateway(
    state: &AppState,
    request: FeatureConveyorReviewGatewayRequest,
) -> Result<Response, ApiError> {
    let authorization = lock_process(state)?
        .kernel_mut()
        .prepare_review_gateway(
            &request,
            current_time_ms()
                .map_err(|_| fixed_error(StatusCode::CONFLICT, "review_gateway_rejected"))?,
        )
        .map_err(review_gateway_api_error)?;
    let plan = match authorization {
        ReviewGatewayAuthorization::ExistingDecision(receipt) => {
            let receipt = *receipt;
            let process = lock_process(state)?;
            let store = process.artifact_integration_store();
            let candidate = process
                .kernel()
                .candidate_references()
                .map_err(|_| fixed_error(StatusCode::CONFLICT, "review_gateway_rejected"))?
                .into_iter()
                .find(|candidate| candidate.integration_id == receipt.integration_id)
                .ok_or_else(|| fixed_error(StatusCode::CONFLICT, "review_gateway_rejected"))?;
            store
                .open_verified_candidate(&candidate)
                .map_err(|_| fixed_error(StatusCode::CONFLICT, "review_gateway_rejected"))?;
            receipt
                .validate()
                .map_err(|_| fixed_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"))?;
            return Ok(Json(receipt).into_response());
        }
        ReviewGatewayAuthorization::ExistingTransportFailure { .. } => {
            return Err(fixed_error(
                StatusCode::CONFLICT,
                "review_transport_attempt_failed",
            ));
        }
        ReviewGatewayAuthorization::Planned(plan) => *plan,
    };
    let store = lock_process(state)?.artifact_integration_store();
    let mut candidate = store
        .open_verified_candidate(&plan.candidate)
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "review_gateway_rejected"))?;
    let candidate_diff = store
        .review_diff(&plan.candidate)
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "review_gateway_rejected"))?;
    let packet = FeatureConveyorReviewPacket {
        schema_version: assemblywright_protocol::FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION,
        feature_id: request.feature_id,
        specification_revision: request.specification_revision,
        approved_specification: plan.approved_specification.clone(),
        approved_specification_sha256: plan.approved_specification_sha256,
        candidate_commit: request.candidate_commit.clone(),
        candidate_tree: request.candidate_tree.clone(),
        base_commit: request.base_commit.clone(),
        candidate_diff_sha256: Sha256::digest(candidate_diff.as_bytes()).into(),
        candidate_diff,
        evidence_manifest_sha256: request.evidence_manifest_sha256,
        evidence_digests: plan.evidence_digests.clone(),
        requirements_sha256: plan.requirements_sha256,
        requirement_ids: plan.requirement_ids.clone(),
        provider_id: request.provider_id.clone(),
        model_id: request.model_id.clone(),
        grants: request.grants,
    };
    let prepared = prepare_review_provider_call(state.review_provider.as_ref(), &request, &packet)
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "review_provider_unavailable"))?;
    candidate
        .revalidate(&store)
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "review_gateway_rejected"))?;
    let persisted = lock_process(state)?
        .kernel_mut()
        .begin_review_gateway(
            &request,
            &packet,
            current_time_ms()
                .map_err(|_| fixed_error(StatusCode::CONFLICT, "review_gateway_rejected"))?,
        )
        .map_err(review_gateway_api_error)?;
    let persisted = match persisted {
        ReviewGatewayAuthorization::Planned(persisted) => {
            let persisted = *persisted;
            if persisted != plan {
                terminalize_interrupted_review(state, &persisted)?;
                return Err(fixed_error(StatusCode::CONFLICT, "review_gateway_rejected"));
            }
            persisted
        }
        ReviewGatewayAuthorization::ExistingDecision(receipt) => {
            return Ok(Json(*receipt).into_response())
        }
        _ => return Err(fixed_error(StatusCode::CONFLICT, "review_gateway_rejected")),
    };
    let cancelled = Arc::new(AtomicBool::new(false));
    let monitor_done = Arc::new(AtomicBool::new(false));
    let monitor_state = state.clone();
    let monitor_request = request.clone();
    let monitor_cancelled = cancelled.clone();
    let monitor_finished = monitor_done.clone();
    let monitor = std::thread::spawn(move || {
        while !monitor_finished.load(Ordering::Acquire) {
            std::thread::sleep(Duration::from_millis(25));
            if monitor_finished.load(Ordering::Acquire) {
                break;
            }
            let current = current_time_ms()
                .ok()
                .and_then(|now_ms| {
                    monitor_state.process.lock().ok().and_then(|process| {
                        process
                            .kernel()
                            .review_gateway_execution_is_current(&monitor_request, now_ms)
                            .ok()
                    })
                })
                .unwrap_or(false);
            if !current {
                monitor_cancelled.store(true, Ordering::Release);
                break;
            }
        }
    });
    let provider_result = invoke_review_provider(
        state.review_provider.as_ref(),
        &request,
        &prepared,
        &cancelled,
    );
    monitor_done.store(true, Ordering::Release);
    if monitor.join().is_err() {
        terminalize_interrupted_review(state, &persisted)?;
        return Err(fixed_error(StatusCode::CONFLICT, "review_gateway_rejected"));
    }
    let output = match provider_result {
        Ok(output) => output,
        Err(ReviewProviderInvocationError::Outage) => {
            lock_process(state)?
                .kernel_mut()
                .finalize_review_transport_failure(
                    &persisted,
                    ReviewTransportFailure::ProviderOutage,
                    current_time_ms().map_err(|_| {
                        fixed_error(StatusCode::CONFLICT, "review_gateway_rejected")
                    })?,
                )
                .map_err(review_gateway_api_error)?;
            return Err(fixed_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "review_provider_outage",
            ));
        }
        Err(ReviewProviderInvocationError::IncompleteTransport) => {
            lock_process(state)?
                .kernel_mut()
                .finalize_review_transport_failure(
                    &persisted,
                    ReviewTransportFailure::IncompleteTransport,
                    current_time_ms().map_err(|_| {
                        fixed_error(StatusCode::CONFLICT, "review_gateway_rejected")
                    })?,
                )
                .map_err(review_gateway_api_error)?;
            return Err(fixed_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "review_transport_incomplete",
            ));
        }
        Err(ReviewProviderInvocationError::MalformedOutput) => {
            lock_process(state)?
                .kernel_mut()
                .finalize_review_transport_failure(
                    &persisted,
                    ReviewTransportFailure::MalformedOutput,
                    current_time_ms().map_err(|_| {
                        fixed_error(StatusCode::CONFLICT, "review_gateway_rejected")
                    })?,
                )
                .map_err(review_gateway_api_error)?;
            return Err(fixed_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "review_output_malformed",
            ));
        }
        Err(ReviewProviderInvocationError::Cancelled) => {
            terminalize_interrupted_review(state, &persisted)?;
            return Err(fixed_error(
                StatusCode::CONFLICT,
                "review_gateway_cancelled",
            ));
        }
        Err(ReviewProviderInvocationError::Unavailable) => {
            return Err(fixed_error(StatusCode::CONFLICT, "review_gateway_rejected"))
        }
    };
    if candidate.revalidate(&store).is_err() {
        terminalize_interrupted_review(state, &persisted)?;
        return Err(fixed_error(StatusCode::CONFLICT, "review_gateway_rejected"));
    }
    let receipt = match lock_process(state)?.kernel_mut().finalize_review_decision(
        &persisted,
        &packet,
        &output,
        current_time_ms()
            .map_err(|_| fixed_error(StatusCode::CONFLICT, "review_gateway_rejected"))?,
    ) {
        Ok(receipt) => receipt,
        Err(error) => {
            terminalize_interrupted_review(state, &persisted)?;
            return Err(review_gateway_api_error(error));
        }
    };
    receipt
        .validate()
        .map_err(|_| fixed_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"))?;
    Ok(Json(receipt).into_response())
}

fn terminalize_interrupted_review(
    state: &AppState,
    plan: &assemblywright_master::ReviewGatewayExecutionPlan,
) -> Result<(), ApiError> {
    lock_process(state)?
        .kernel_mut()
        .finalize_interrupted_review_call(
            plan,
            current_time_ms()
                .map_err(|_| fixed_error(StatusCode::CONFLICT, "review_gateway_rejected"))?,
        )
        .map_err(review_gateway_api_error)
}

fn review_gateway_api_error(error: assemblywright_master::MasterError) -> ApiError {
    match error {
        assemblywright_master::MasterError::ReviewRetryNotReady { .. } => {
            fixed_error(StatusCode::TOO_MANY_REQUESTS, "review_backoff_active")
        }
        assemblywright_master::MasterError::ReviewBudgetExhausted => {
            fixed_error(StatusCode::CONFLICT, "review_budget_exhausted")
        }
        _ => fixed_error(StatusCode::CONFLICT, "review_gateway_rejected"),
    }
}

fn spawn_reserved_blocking<T, F>(
    reservation: tokio::sync::OwnedMutexGuard<()>,
    work: F,
) -> tokio::task::JoinHandle<T>
where
    T: Send + 'static,
    F: FnOnce() -> T + Send + 'static,
{
    tokio::task::spawn_blocking(move || {
        let _reservation = reservation;
        work()
    })
}

fn perform_feature_artifact_integration(
    state: &AppState,
    request: FeatureConveyorArtifactIntegrationRequest,
) -> Result<Response, ApiError> {
    let now_ms = current_time_ms()
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "artifact_integration_rejected"))?;
    let authorization = {
        let process = lock_process(state)?;
        process
            .kernel()
            .prepare_artifact_integration(&request, now_ms)
    };
    let plan = match authorization {
        Ok(ArtifactIntegrationAuthorization::Existing(receipt)) => {
            let evidence = assemblywright_master::CandidateEvidence {
                integration_id: receipt.integration_id,
                artifact_set_sha256: receipt.artifact_set_sha256,
                candidate_commit: receipt.candidate_commit.clone(),
                candidate_tree: receipt.candidate_tree.clone(),
                base_commit: receipt.base_commit.clone(),
                artifact_ids: request.artifact_ids.clone(),
            };
            let process = lock_process(state)?;
            let store = process.artifact_integration_store();
            let artifact_store = process.result_artifact_store();
            let references = process
                .kernel()
                .integration_artifact_references(receipt.integration_id)
                .map_err(|_| fixed_error(StatusCode::CONFLICT, "artifact_integration_rejected"))?;
            drop(process);
            let _verified = store
                .open_verified_candidate(&evidence)
                .map_err(|_| fixed_error(StatusCode::CONFLICT, "artifact_integration_rejected"))?;
            let mut artifact_guards = Vec::with_capacity(request.artifact_ids.len());
            let mut exact_references = Vec::with_capacity(request.artifact_ids.len());
            for artifact_id in &request.artifact_ids {
                let reference = references
                    .iter()
                    .find(|reference| reference.artifact_id == *artifact_id)
                    .copied()
                    .ok_or_else(|| {
                        fixed_error(StatusCode::CONFLICT, "artifact_integration_rejected")
                    })?;
                let mut guard = artifact_store.open_verified(reference).map_err(|_| {
                    fixed_error(StatusCode::CONFLICT, "artifact_integration_rejected")
                })?;
                guard.revalidate(&artifact_store).map_err(|_| {
                    fixed_error(StatusCode::CONFLICT, "artifact_integration_rejected")
                })?;
                exact_references.push(reference);
                artifact_guards.push(guard);
            }
            if integration_artifact_set_sha256(&exact_references) != receipt.artifact_set_sha256 {
                return Err(fixed_error(
                    StatusCode::CONFLICT,
                    "artifact_integration_rejected",
                ));
            }
            receipt
                .validate()
                .map_err(|_| fixed_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"))?;
            return Ok(Json(receipt).into_response());
        }
        Ok(ArtifactIntegrationAuthorization::Planned(plan)) => plan,
        Err(assemblywright_master::MasterError::ArtifactIntegrationConflict) => {
            lock_process(state)?
                .kernel_mut()
                .record_artifact_integration_conflict(&request, "duplicate_ordinal", now_ms)
                .map_err(|_| fixed_error(StatusCode::CONFLICT, "artifact_integration_rejected"))?;
            return Err(fixed_error(
                StatusCode::CONFLICT,
                "artifact_integration_conflict",
            ));
        }
        Err(_) => {
            return Err(fixed_error(
                StatusCode::CONFLICT,
                "artifact_integration_rejected",
            ))
        }
    };
    let store = lock_process(state)?.artifact_integration_store();
    let mut prepared = match store.prepare(
        plan.request.integration_id,
        plan.request.snapshot_id,
        &plan.request.base_commit,
        &plan.artifacts,
    ) {
        Ok(prepared) => prepared,
        Err(
            error @ (ArtifactIntegrationError::OverlappingPath
            | ArtifactIntegrationError::ContentCasMismatch),
        ) => {
            let reason = if matches!(error, ArtifactIntegrationError::OverlappingPath) {
                "overlapping_path"
            } else {
                "content_cas_mismatch"
            };
            let _ = lock_process(state)?
                .kernel_mut()
                .record_artifact_integration_conflict(&request, reason, now_ms);
            return Err(fixed_error(
                StatusCode::CONFLICT,
                "artifact_integration_conflict",
            ));
        }
        Err(_) => {
            return Err(fixed_error(
                StatusCode::CONFLICT,
                "artifact_integration_rejected",
            ))
        }
    };
    let finalized_at_ms = current_time_ms()
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "artifact_integration_rejected"))?;
    let mut process = lock_process(state)?;
    prepared
        .revalidate_artifacts(&process.result_artifact_store())
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "artifact_integration_rejected"))?;
    prepared
        .revalidate_candidate(&store)
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "artifact_integration_rejected"))?;
    let receipt = process
        .kernel_mut()
        .finalize_artifact_integration(&plan, &prepared.evidence, finalized_at_ms)
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "artifact_integration_rejected"))?;
    receipt
        .validate()
        .map_err(|_| fixed_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"))?;
    prepared.retain();
    Ok(Json(receipt).into_response())
}

fn integration_artifact_set_sha256(references: &[ResultArtifactReference]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"assemblywright.artifact-set.v1\0");
    digest.update((references.len() as u64).to_be_bytes());
    let mut entries = references.to_vec();
    entries.sort_by_key(|reference| reference.artifact_id);
    for reference in entries {
        digest.update(reference.artifact_id.as_bytes());
        digest.update(reference.artifact_sha256);
        digest.update(reference.artifact_size_bytes.to_be_bytes());
    }
    digest.finalize().into()
}

async fn feature_artifact_integration_plan(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(feature_id): AxumPath<String>,
) -> ApiResult<FeatureConveyorArtifactIntegrationPlan> {
    authorize(&headers, &state)?;
    let feature_id = Uuid::parse_str(&feature_id)
        .map_err(|_| fixed_error(StatusCode::NOT_FOUND, "integration_plan_unavailable"))?;
    let plan = lock_process(&state)?
        .kernel()
        .artifact_integration_plan(
            feature_id,
            current_time_ms()
                .map_err(|_| fixed_error(StatusCode::CONFLICT, "integration_plan_unavailable"))?,
        )
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "integration_plan_unavailable"))?;
    Ok(Json(plan))
}

async fn cancel_active_feature(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<FeatureConveyorCancelActiveFeatureReceipt> {
    authorize(&headers, &state)?;
    let body = body.map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "feature_cancel_request_rejected",
        )
    })?;
    let request = FeatureConveyorCancelActiveFeatureRequest::decode_frame(&body).map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "feature_cancel_request_rejected",
        )
    })?;
    let snapshot = lock_process(&state)?
        .kernel_mut()
        .cancel_active_feature(
            request.feature_id,
            request.expected_lifecycle_revision,
            request.expected_queue_revision,
            request.expected_emergency_pause_revision,
            current_time_ms()
                .map_err(|_| fixed_error(StatusCode::CONFLICT, "feature_cancel_rejected"))?,
        )
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "feature_cancel_rejected"))?;
    let receipt = FeatureConveyorCancelActiveFeatureReceipt {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        feature_id: snapshot.feature_id,
        lifecycle_revision: snapshot.lifecycle_revision,
        queue_revision: request.expected_queue_revision,
        emergency_pause_revision: request.expected_emergency_pause_revision,
        lease_retained: snapshot.active_lease_id.is_some(),
        advancement_authorized: false,
        status: FeatureConveyorCancelActiveFeatureStatus::Cancelled,
    };
    receipt
        .validate()
        .map_err(|_| fixed_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"))?;
    Ok(Json(receipt))
}

async fn abandon_and_advance(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Result<Bytes, BytesRejection>,
) -> ApiResult<FeatureConveyorAbandonAndAdvanceReceipt> {
    authorize(&headers, &state)?;
    let body = body.map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "feature_abandonment_request_rejected",
        )
    })?;
    let request = FeatureConveyorAbandonAndAdvanceRequest::decode_frame(&body).map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "feature_abandonment_request_rejected",
        )
    })?;
    let snapshot = lock_process(&state)?
        .kernel_mut()
        .abandon_and_advance(
            request.feature_id,
            request.expected_lifecycle_revision,
            request.expected_queue_revision,
            request.expected_emergency_pause_revision,
            FeatureAbandonmentEvidence {
                safe_reconciliation_sha256: request.evidence.safe_reconciliation_sha256,
                merged: request.evidence.merged,
                verified_healthy_main_sha256: request.evidence.verified_healthy_main_sha256,
            },
            current_time_ms()
                .map_err(|_| fixed_error(StatusCode::CONFLICT, "feature_abandonment_rejected"))?,
        )
        .map_err(|_| fixed_error(StatusCode::CONFLICT, "feature_abandonment_rejected"))?;
    let receipt = FeatureConveyorAbandonAndAdvanceReceipt {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        feature_id: snapshot.feature_id,
        lifecycle_revision: snapshot.lifecycle_revision,
        queue_revision: request.expected_queue_revision + 1,
        emergency_pause_revision: request.expected_emergency_pause_revision,
        lease_released: snapshot.active_lease_id.is_none(),
        status: FeatureConveyorAbandonAndAdvanceStatus::Abandoned,
    };
    receipt
        .validate()
        .map_err(|_| fixed_error(StatusCode::INTERNAL_SERVER_ERROR, "internal_error"))?;
    Ok(Json(receipt))
}

#[cfg(not(windows))]
struct RepositoryIdentityObservationGuard;

#[cfg(not(windows))]
impl Drop for RepositoryIdentityObservationGuard {
    fn drop(&mut self) {}
}

#[cfg(not(windows))]
impl RepositoryIdentityObservationGuard {
    fn revalidate(&mut self) -> Result<(), ()> {
        Ok(())
    }
}

#[cfg(windows)]
struct RepositoryIdentityObservationGuard {
    held: WindowsHeldHandleSet,
    requested: String,
    expected_base_branch: String,
    expected_head_commit: String,
    path_revalidation: Option<WindowsHeldHandleSet>,
}

#[cfg(windows)]
impl Drop for RepositoryIdentityObservationGuard {
    fn drop(&mut self) {}
}

#[cfg(windows)]
impl RepositoryIdentityObservationGuard {
    fn revalidate(&mut self) -> Result<(), ()> {
        self.held.revalidate()?;
        let path_revalidation = open_windows_repository_identity_handles(
            &self.requested,
            &self.expected_base_branch,
            &self.expected_head_commit,
        )?;
        if path_revalidation.identities != self.held.identities {
            return Err(());
        }
        self.path_revalidation = Some(path_revalidation);
        Ok(())
    }
}

#[cfg(windows)]
#[derive(Clone, Copy, PartialEq, Eq)]
enum WindowsHandleKind {
    Directory,
    File { maximum_bytes: u64 },
}

#[cfg(windows)]
#[derive(Clone, Copy, PartialEq, Eq)]
struct WindowsHandleIdentity {
    volume_serial: u32,
    file_index: u64,
    attributes: u32,
    size: u64,
}

#[cfg(windows)]
struct WindowsHeldHandleSet {
    handles: Vec<fs::File>,
    identities: Vec<(WindowsHandleIdentity, WindowsHandleKind)>,
}

#[cfg(windows)]
impl WindowsHeldHandleSet {
    fn new() -> Self {
        Self {
            handles: Vec::new(),
            identities: Vec::new(),
        }
    }

    fn hold(&mut self, handle: fs::File, kind: WindowsHandleKind) -> Result<usize, ()> {
        let identity = windows_handle_identity(&handle, kind)?;
        if self
            .identities
            .first()
            .is_some_and(|(root, _)| root.volume_serial != identity.volume_serial)
        {
            return Err(());
        }
        self.handles.push(handle);
        self.identities.push((identity, kind));
        Ok(self.handles.len() - 1)
    }

    fn revalidate(&self) -> Result<(), ()> {
        for (handle, (expected, kind)) in self.handles.iter().zip(&self.identities) {
            if windows_handle_identity(handle, *kind)? != *expected {
                return Err(());
            }
        }
        Ok(())
    }
}

#[cfg(windows)]
fn observe_standard_repository_identity(
    requested: &str,
    expected_base_branch: &str,
    expected_head_commit: &str,
) -> Result<RepositoryIdentityObservationGuard, ()> {
    let held = open_windows_repository_identity_handles(
        requested,
        expected_base_branch,
        expected_head_commit,
    )?;
    Ok(RepositoryIdentityObservationGuard {
        held,
        requested: requested.to_string(),
        expected_base_branch: expected_base_branch.to_string(),
        expected_head_commit: expected_head_commit.to_string(),
        path_revalidation: None,
    })
}

#[cfg(windows)]
fn open_windows_repository_identity_handles(
    requested: &str,
    expected_base_branch: &str,
    expected_head_commit: &str,
) -> Result<WindowsHeldHandleSet, ()> {
    let requested_path = PathBuf::from(requested);
    let mut held = open_windows_fixed_directory_chain(requested)?;
    require_windows_handle_path_matches_requested(held.handles.last().ok_or(())?, &requested_path)?;
    let git_directory = requested_path.join(".git");
    held.hold(
        open_windows_stable_handle(&git_directory, WindowsHandleKind::Directory)?,
        WindowsHandleKind::Directory,
    )?;
    held.hold(
        open_windows_stable_handle(&git_directory.join("objects"), WindowsHandleKind::Directory)?,
        WindowsHandleKind::Directory,
    )?;
    let refs_directory = git_directory.join("refs");
    held.hold(
        open_windows_stable_handle(&refs_directory, WindowsHandleKind::Directory)?,
        WindowsHandleKind::Directory,
    )?;
    let heads_directory = refs_directory.join("heads");
    held.hold(
        open_windows_stable_handle(&heads_directory, WindowsHandleKind::Directory)?,
        WindowsHandleKind::Directory,
    )?;

    reject_existing_repository_control_path(&requested_path.join(".gitmodules"))?;
    for forbidden in [
        "modules",
        "worktrees",
        "commondir",
        "gitdir",
        "config.worktree",
    ] {
        reject_existing_repository_control_path(&git_directory.join(forbidden))?;
    }

    let branch_path = Path::new(expected_base_branch);
    if branch_path.components().count() != 1
        || !matches!(branch_path.components().next(), Some(Component::Normal(_)))
    {
        return Err(());
    }
    let head_kind = WindowsHandleKind::File { maximum_bytes: 512 };
    let head_index = held.hold(
        open_windows_stable_handle(&git_directory.join("HEAD"), head_kind)?,
        head_kind,
    )?;
    let branch_index = held.hold(
        open_windows_stable_handle(&heads_directory.join(branch_path), head_kind)?,
        head_kind,
    )?;
    let expected_head = format!("ref: refs/heads/{expected_base_branch}\n");
    let expected_ref = format!("{expected_head_commit}\n");
    if read_windows_held_file(&mut held.handles[head_index], 512)? != expected_head.as_bytes()
        || read_windows_held_file(&mut held.handles[branch_index], 512)? != expected_ref.as_bytes()
        || read_windows_held_file(&mut held.handles[head_index], 512)? != expected_head.as_bytes()
        || read_windows_held_file(&mut held.handles[branch_index], 512)? != expected_ref.as_bytes()
    {
        return Err(());
    }
    held.revalidate()?;
    Ok(held)
}

#[cfg(windows)]
fn open_windows_fixed_directory_chain(requested: &str) -> Result<WindowsHeldHandleSet, ()> {
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;

    // Win32 GetDriveTypeW returns 3 for DRIVE_FIXED. Keep this local instead
    // of enabling the much broader WindowsProgramming feature only for the
    // generated constant.
    const DRIVE_TYPE_FIXED: u32 = 3;

    let bytes = requested.as_bytes();
    if requested.starts_with(r"\\")
        || requested.starts_with("//")
        || bytes.len() < 3
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || !matches!(bytes[2], b'/' | b'\\')
    {
        return Err(());
    }
    let drive = bytes[0].to_ascii_uppercase();
    let root = [u16::from(drive), b':' as u16, b'\\' as u16, 0];
    if unsafe { GetDriveTypeW(root.as_ptr()) } != DRIVE_TYPE_FIXED {
        return Err(());
    }

    let mut held = WindowsHeldHandleSet::new();
    let mut current = PathBuf::from(format!("{}:\\", char::from(drive)));
    held.hold(
        open_windows_stable_handle(&current, WindowsHandleKind::Directory)?,
        WindowsHandleKind::Directory,
    )?;
    for component in Path::new(requested).components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {}
            Component::Normal(name) => {
                current.push(name);
                held.hold(
                    open_windows_stable_handle(&current, WindowsHandleKind::Directory)?,
                    WindowsHandleKind::Directory,
                )?;
            }
            Component::CurDir | Component::ParentDir => return Err(()),
        }
    }
    if !repository_paths_match(&current, Path::new(requested)) {
        return Err(());
    }
    Ok(held)
}

#[cfg(windows)]
fn require_windows_handle_path_matches_requested(
    handle: &fs::File,
    requested: &Path,
) -> Result<(), ()> {
    let resolved = windows_final_dos_path_by_handle(handle)?;
    if !repository_paths_match(&resolved, requested) {
        return Err(());
    }
    Ok(())
}

#[cfg(windows)]
fn windows_final_dos_path_by_handle(handle: &fs::File) -> Result<PathBuf, ()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFinalPathNameByHandleW, FILE_NAME_NORMALIZED, VOLUME_NAME_DOS,
    };

    const MAX_FINAL_PATH_CHARS: usize = 32_768;
    let mut buffer = vec![0_u16; MAX_FINAL_PATH_CHARS];
    let length = unsafe {
        GetFinalPathNameByHandleW(
            handle.as_raw_handle().cast(),
            buffer.as_mut_ptr(),
            buffer.len() as u32,
            FILE_NAME_NORMALIZED | VOLUME_NAME_DOS,
        )
    };
    let length = usize::try_from(length).map_err(|_| ())?;
    if length == 0 || length >= buffer.len() {
        return Err(());
    }
    let resolved = String::from_utf16(&buffer[..length]).map_err(|_| ())?;
    let dos_path = if let Some(rest) = resolved.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = resolved.strip_prefix(r"\\?\") {
        rest.to_string()
    } else {
        resolved
    };
    Ok(PathBuf::from(dos_path))
}

#[cfg(windows)]
fn open_windows_stable_handle(path: &Path, kind: WindowsHandleKind) -> Result<fs::File, ()> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
        FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    let mut options = OpenOptions::new();
    match kind {
        WindowsHandleKind::Directory => {
            options
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
                .access_mode(FILE_READ_ATTRIBUTES)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS);
        }
        WindowsHandleKind::File { .. } => {
            options
                .share_mode(FILE_SHARE_READ)
                .read(true)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        }
    }
    let handle = options.open(path).map_err(|_| ())?;
    windows_handle_identity(&handle, kind)?;
    Ok(handle)
}

#[cfg(windows)]
fn windows_handle_identity(
    handle: &fs::File,
    kind: WindowsHandleKind,
) -> Result<WindowsHandleIdentity, ()> {
    use std::mem::MaybeUninit;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY,
        FILE_ATTRIBUTE_REPARSE_POINT,
    };

    let mut information = MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::zeroed();
    if unsafe {
        GetFileInformationByHandle(handle.as_raw_handle().cast(), information.as_mut_ptr())
    } == 0
    {
        return Err(());
    }
    let information = unsafe { information.assume_init() };
    let is_directory = information.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
    let size = (u64::from(information.nFileSizeHigh) << 32) | u64::from(information.nFileSizeLow);
    if information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || information.nNumberOfLinks == 0
        || is_directory != matches!(kind, WindowsHandleKind::Directory)
        || matches!(kind, WindowsHandleKind::File { maximum_bytes } if size > maximum_bytes)
    {
        return Err(());
    }
    Ok(WindowsHandleIdentity {
        volume_serial: information.dwVolumeSerialNumber,
        file_index: (u64::from(information.nFileIndexHigh) << 32)
            | u64::from(information.nFileIndexLow),
        attributes: information.dwFileAttributes,
        size,
    })
}

#[cfg(windows)]
fn read_windows_held_file(handle: &mut fs::File, maximum: u64) -> Result<Vec<u8>, ()> {
    use std::io::{Seek, SeekFrom};

    handle.seek(SeekFrom::Start(0)).map_err(|_| ())?;
    let mut bytes = Vec::new();
    std::io::Read::by_ref(handle)
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.len() as u64 > maximum {
        return Err(());
    }
    Ok(bytes)
}

#[cfg(not(windows))]
fn observe_standard_repository_identity(
    requested: &str,
    expected_base_branch: &str,
    expected_head_commit: &str,
) -> Result<RepositoryIdentityObservationGuard, ()> {
    validate_windows_fixed_local_repository_path(requested)?;
    let requested = PathBuf::from(requested);
    if !requested.is_absolute()
        || requested
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(());
    }
    let metadata = fs::symlink_metadata(&requested).map_err(|_| ())?;
    if metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
        return Err(());
    }
    let canonical = fs::canonicalize(&requested).map_err(|_| ())?;
    if !repository_paths_match(&canonical, &requested) {
        return Err(());
    }
    let git_directory = requested.join(".git");
    require_directory_without_symlink(&git_directory)?;
    require_directory_without_symlink(&git_directory.join("objects"))?;
    reject_existing_repository_control_path(&requested.join(".gitmodules"))?;
    for forbidden in [
        "modules",
        "worktrees",
        "commondir",
        "gitdir",
        "config.worktree",
    ] {
        reject_existing_repository_control_path(&git_directory.join(forbidden))?;
    }

    let expected_head = format!("ref: refs/heads/{expected_base_branch}\n");
    let head_path = git_directory.join("HEAD");
    if read_bounded_regular_file(&head_path, 512)? != expected_head.as_bytes() {
        return Err(());
    }
    let heads_directory = git_directory.join("refs").join("heads");
    require_directory_without_symlink(&heads_directory)?;
    let branch_path = heads_directory.join(expected_base_branch);
    validate_windows_fixed_local_repository_path(branch_path.to_str().ok_or(())?)?;
    let canonical_branch = fs::canonicalize(&branch_path).map_err(|_| ())?;
    if !canonical_branch.starts_with(fs::canonicalize(&heads_directory).map_err(|_| ())?)
        || !repository_paths_match(&canonical_branch, &branch_path)
    {
        return Err(());
    }
    let expected_ref = format!("{expected_head_commit}\n");
    if read_bounded_regular_file(&canonical_branch, 512)? != expected_ref.as_bytes() {
        return Err(());
    }

    // Revalidate the complete identity after reading it. This remains a
    // point-in-time observation and grants no later repository authority.
    let revalidated = fs::canonicalize(&requested).map_err(|_| ())?;
    let revalidated_metadata = fs::symlink_metadata(&requested).map_err(|_| ())?;
    if !repository_paths_match(&revalidated, &canonical)
        || metadata_is_link_or_reparse(&revalidated_metadata)
        || read_bounded_regular_file(&head_path, 512)? != expected_head.as_bytes()
        || read_bounded_regular_file(&canonical_branch, 512)? != expected_ref.as_bytes()
    {
        return Err(());
    }
    Ok(RepositoryIdentityObservationGuard)
}

#[cfg(not(windows))]
fn repository_paths_match(left: &Path, right: &Path) -> bool {
    left == right
}

#[cfg(windows)]
fn repository_paths_match(left: &Path, right: &Path) -> bool {
    fn normalized(path: &Path) -> String {
        let mut value = path.to_string_lossy().replace('\\', "/");
        if let Some(rest) = value.strip_prefix("//?/UNC/") {
            value = format!("//{rest}");
        } else if let Some(rest) = value.strip_prefix("//?/") {
            value = rest.to_string();
        }
        value.to_ascii_lowercase().trim_end_matches('/').to_string()
    }
    normalized(left) == normalized(right)
}

#[cfg(not(windows))]
fn require_directory_without_symlink(path: &Path) -> Result<(), ()> {
    let metadata = fs::symlink_metadata(path).map_err(|_| ())?;
    if metadata_is_link_or_reparse(&metadata) || !metadata.is_dir() {
        return Err(());
    }
    Ok(())
}

fn reject_existing_repository_control_path(path: &Path) -> Result<(), ()> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(()),
    }
}

#[cfg(not(windows))]
fn read_bounded_regular_file(path: &Path, maximum: u64) -> Result<Vec<u8>, ()> {
    let metadata = fs::symlink_metadata(path).map_err(|_| ())?;
    if metadata_is_link_or_reparse(&metadata) || !metadata.is_file() || metadata.len() > maximum {
        return Err(());
    }
    let mut file = OpenOptions::new().read(true).open(path).map_err(|_| ())?;
    let opened = file.metadata().map_err(|_| ())?;
    if !opened.is_file() || opened.len() > maximum {
        return Err(());
    }
    let mut bytes = Vec::new();
    std::io::Read::by_ref(&mut file)
        .take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| ())?;
    if bytes.len() as u64 > maximum
        || metadata_is_link_or_reparse(&fs::symlink_metadata(path).map_err(|_| ())?)
    {
        return Err(());
    }
    Ok(bytes)
}

#[cfg(not(windows))]
fn metadata_is_link_or_reparse(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(not(windows))]
fn validate_windows_fixed_local_repository_path(_requested: &str) -> Result<(), ()> {
    Ok(())
}

#[cfg(all(windows, test))]
fn validate_windows_fixed_local_repository_path(requested: &str) -> Result<(), ()> {
    open_windows_fixed_directory_chain(requested).map(|_| ())
}

async fn get_repository_grants(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(repository_id): AxumPath<String>,
) -> ApiResult<FeatureConveyorRepositoryGrantSet> {
    authorize(&headers, &state)?;
    let repository_id = Uuid::parse_str(&repository_id).map_err(|_| {
        fixed_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "repository_grant_status_request_rejected",
        )
    })?;
    let now_ms = current_time_ms().map_err(|_| internal_error())?;
    let grants = lock_process(&state)?
        .kernel()
        .repository_grant_set(repository_id, now_ms)
        .map_err(|_| {
            fixed_error(
                StatusCode::UNPROCESSABLE_ENTITY,
                "repository_grant_status_request_rejected",
            )
        })?;
    Ok(Json(grants))
}

async fn register_device(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(registration): Json<DeviceRegistration>,
) -> ApiResult<AcceptedResponse> {
    authorize(&headers, &state)?;
    lock_process(&state)?
        .kernel_mut()
        .register_device(&registration)
        .map_err(api_error)?;
    Ok(Json(AcceptedResponse { accepted: true }))
}

async fn accept_handshake(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<HandshakeRequest>,
) -> ApiResult<HandshakeResponse> {
    authorize(&headers, &state)?;
    let now_ms = current_time_ms().map_err(api_error)?;
    let response = lock_process(&state)?
        .kernel_mut()
        .accept_handshake(&request, now_ms)
        .map_err(api_error)?;
    Ok(Json(response))
}

async fn enqueue_step(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(step): Json<NewStep>,
) -> ApiResult<AcceptedResponse> {
    authorize(&headers, &state)?;
    require_work_admission(&state)?;
    let now_ms = current_time_ms().map_err(api_error)?;
    lock_process(&state)?
        .kernel_mut()
        .enqueue_step(&step, now_ms)
        .map_err(api_error)?;
    Ok(Json(AcceptedResponse { accepted: true }))
}

async fn development_events_next(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<DistributedEventBatchRequest>,
) -> ApiResult<DistributedEventBatch> {
    authorize(&headers, &state)?;
    request.validate().map_err(api_error)?;
    let events = lock_process(&state)?
        .kernel()
        .distributed_events(&request)
        .map_err(distributed_event_error)?;
    Ok(Json(events))
}

async fn cancel_development_step(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(step_id): AxumPath<String>,
) -> ApiResult<AcceptedResponse> {
    authorize(&headers, &state)?;
    let step_id = Uuid::parse_str(&step_id).map(StepId::new).map_err(|_| {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(ErrorResponse {
                error: "invalid_step_id".to_string(),
            }),
        )
    })?;
    lock_process(&state)?
        .kernel_mut()
        .cancel_step(step_id, current_time_ms().map_err(api_error)?)
        .map_err(api_error)?;
    schedule_cancellation_deadline_reconciliation(&state);
    Ok(Json(AcceptedResponse { accepted: true }))
}

async fn activate_emergency_pause(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(_request): Json<EmergencyPauseActionRequest>,
) -> ApiResult<EmergencyPauseResponse> {
    authorize(&headers, &state)?;
    let mut active = state
        .active_planning_calls
        .lock()
        .map_err(|_| internal_error())?;
    active.retain(|call| {
        if let Some(call) = call.upgrade() {
            call.store(true, Ordering::Release);
            true
        } else {
            false
        }
    });
    lock_process(&state)?
        .kernel_mut()
        .set_emergency_paused(true)
        .map_err(api_error)?;
    for call in active.iter().filter_map(Weak::upgrade) {
        call.store(true, Ordering::Release);
    }
    drop(active);
    schedule_cancellation_deadline_reconciliation(&state);
    Ok(Json(EmergencyPauseResponse {
        emergency_paused: true,
    }))
}

async fn resume_emergency_pause(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(_request): Json<EmergencyPauseActionRequest>,
) -> ApiResult<EmergencyPauseResponse> {
    authorize(&headers, &state)?;
    lock_process(&state)?
        .kernel_mut()
        .set_emergency_paused(false)
        .map_err(api_error)?;
    Ok(Json(EmergencyPauseResponse {
        emergency_paused: false,
    }))
}

fn schedule_cancellation_deadline_reconciliation(state: &AppState) {
    let process = state.process.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(
            assemblywright_protocol::CANCELLATION_ACK_DEADLINE_MS,
        ))
        .await;
        if let Ok(mut process) = process.lock() {
            let _ = process
                .kernel_mut()
                .reconcile_cancellation_deadlines(current_time_ms().unwrap_or(u64::MAX));
        }
    });
}

async fn lease_next(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(request): Json<LeaseRequest>,
) -> ApiResult<JobEnvelope> {
    authorize(&headers, &state)?;
    require_work_admission(&state)?;
    let now_ms = current_time_ms().map_err(api_error)?;
    let job = lock_process(&state)?
        .kernel_mut()
        .lease_next_step(request.device_id, request.connection_epoch, now_ms)
        .map_err(api_error)?;
    Ok(Json(job))
}

async fn accept_result(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(result): Json<JobResultEnvelope>,
) -> ApiResult<AcceptedResult> {
    authorize(&headers, &state)?;
    require_work_admission(&state)?;
    let now_ms = current_time_ms().map_err(api_error)?;
    let accepted = lock_process(&state)?
        .kernel_mut()
        .accept_result(&result, now_ms)
        .map_err(api_error)?;
    Ok(Json(accepted))
}

fn authorize(headers: &HeaderMap, state: &AppState) -> Result<(), ApiError> {
    let Some(value) = headers.get(axum::http::header::AUTHORIZATION) else {
        return Err(unauthorized());
    };
    let Ok(value) = value.to_str() else {
        return Err(unauthorized());
    };
    let Some(token) = value.strip_prefix("Bearer ") else {
        return Err(unauthorized());
    };
    let candidate: [u8; 32] = Sha256::digest(token.as_bytes()).into();
    if !constant_time_equal(&candidate, &state.token_sha256) {
        return Err(unauthorized());
    }
    Ok(())
}

fn require_work_admission(state: &AppState) -> Result<(), ApiError> {
    if state.lifecycle.maintenance_active.load(Ordering::SeqCst) {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "maintenance_mode_blocks_new_work".to_string(),
            }),
        ));
    }
    if lock_process(state)?
        .kernel()
        .emergency_paused()
        .map_err(api_error)?
    {
        return Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(ErrorResponse {
                error: "emergency_pause_blocks_work".to_string(),
            }),
        ));
    }
    Ok(())
}

fn constant_time_equal(left: &[u8; 32], right: &[u8; 32]) -> bool {
    left.iter()
        .zip(right.iter())
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn unauthorized() -> ApiError {
    (
        StatusCode::UNAUTHORIZED,
        Json(ErrorResponse {
            error: "unauthorized".to_string(),
        }),
    )
}

fn fixed_error(status: StatusCode, code: &'static str) -> ApiError {
    (
        status,
        Json(ErrorResponse {
            error: code.to_string(),
        }),
    )
}

fn local_model_selection_api_error(error: MasterError) -> ApiError {
    match error {
        MasterError::LocalModelSelectionRejected
        | MasterError::StaleOwnerControlDesignationRevision { .. }
        | MasterError::StaleEmergencyPauseRevision { .. }
        | MasterError::OwnerControlBridgeNotDesignated
        | MasterError::OwnerControlBridgeUnauthorized
        | MasterError::EmergencyPaused
        | MasterError::ConnectionNotActive
        | MasterError::ConnectionEpochMismatch
        | MasterError::InvalidRemoteWorkContract => {
            fixed_error(StatusCode::CONFLICT, "local_model_selection_rejected")
        }
        _ => local_model_selection_internal_error(),
    }
}

fn local_model_selection_internal_error() -> ApiError {
    fixed_error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "local_model_selection_internal_error",
    )
}

fn api_error(error: impl std::fmt::Display) -> ApiError {
    (
        StatusCode::CONFLICT,
        Json(ErrorResponse {
            error: error.to_string(),
        }),
    )
}

fn internal_error() -> ApiError {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(ErrorResponse {
            error: "master process state is unavailable".to_string(),
        }),
    )
}

fn lock_process(state: &AppState) -> Result<std::sync::MutexGuard<'_, MasterProcess>, ApiError> {
    state.process.lock().map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse {
                error: "master process state is unavailable".to_string(),
            }),
        )
    })
}

async fn health(data_dir: &Path, endpoint: SocketAddr) -> anyhow::Result<()> {
    let response = fetch_health(data_dir, endpoint).await?;
    println!("{}", serde_json::to_string(&response)?);
    Ok(())
}

async fn fetch_health(data_dir: &Path, endpoint: SocketAddr) -> anyhow::Result<HealthResponse> {
    require_loopback(endpoint)?;
    let token = read_development_token(&data_dir.join(DEVELOPMENT_TOKEN_FILE))?;
    get_json(endpoint, "/health", &token).await
}

#[cfg(windows)]
async fn fetch_health_value(data_dir: &Path, endpoint: SocketAddr) -> anyhow::Result<Value> {
    Ok(serde_json::to_value(
        fetch_health(data_dir, endpoint).await?,
    )?)
}

#[cfg(windows)]
async fn wait_for_runtime_health(
    data_dir: &Path,
    endpoint: SocketAddr,
    timeout: Duration,
) -> anyhow::Result<Value> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Ok(health) = fetch_health_value(data_dir, endpoint).await {
            return Ok(health);
        }
        if std::time::Instant::now() >= deadline {
            bail!("service reached its SCM state but runtime health did not become available");
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

async fn fixture_worker(
    data_dir: &Path,
    endpoint: SocketAddr,
    prompt: String,
) -> anyhow::Result<()> {
    require_loopback(endpoint)?;
    let token = read_development_token(&data_dir.join(DEVELOPMENT_TOKEN_FILE))?;
    let device_id = DeviceId::new(Uuid::new_v4());
    let capability = CapabilityDescriptor::fixture_reasoning();
    let registration = DeviceRegistration {
        device_id,
        device_name: "cross-process-fixture-worker".to_string(),
        role: DeviceRole::InferenceWorker,
        registry_revision: 1,
        capabilities: vec![capability.clone()],
    };
    let _: AcceptedResponse = post_json(
        endpoint,
        "/v1/development/devices/register",
        &token,
        &registration,
    )
    .await?;
    let handshake: HandshakeResponse = post_json(
        endpoint,
        "/v1/development/connections/accept",
        &token,
        &HandshakeRequest {
            protocol_version: PROTOCOL_VERSION,
            device_id,
            device_name: registration.device_name.clone(),
            role: registration.role,
            registry_revision: registration.registry_revision,
            capabilities: vec![capability],
        },
    )
    .await?;
    if handshake.status != HandshakeStatus::Accepted {
        bail!(
            "fixture worker handshake was rejected: {}",
            handshake.reason_code.as_deref().unwrap_or("unknown")
        );
    }

    let task_id = TaskId::new(Uuid::new_v4());
    let step_id = StepId::new(Uuid::new_v4());
    let step = NewStep {
        task_id,
        step_id,
        capability_id: "fixture.reasoning".to_string(),
        sensitivity: Sensitivity::Public,
        context: json!({"operation":"synthetic_echo","input":prompt,"delay_ms":0}),
        lease_duration_ms: 60_000,
        deadline_after_ms: 300_000,
    };
    let _: AcceptedResponse = post_json(endpoint, "/v1/development/steps", &token, &step).await?;
    let job: JobEnvelope = post_json(
        endpoint,
        "/v1/development/leases/next",
        &token,
        &LeaseRequest {
            device_id,
            connection_epoch: handshake.connection_epoch,
        },
    )
    .await?;
    let payload = serde_json::to_value(FixtureJobResult::synthetic_echo(
        job.context["input"]
            .as_str()
            .unwrap_or_default()
            .to_string(),
    ))?;
    let result = JobResultEnvelope {
        protocol_version: PROTOCOL_VERSION,
        connection_epoch: job.connection_epoch,
        sequence: job.sequence + 1,
        task_id: job.task_id,
        step_id: job.step_id,
        attempt_id: job.attempt_id,
        lease_id: job.lease_id,
        cancellation_id: job.cancellation_id,
        status: JobResultStatus::Completed,
        context_sha256: job.context_sha256,
        payload_sha256: Sha256::digest(serde_json::to_vec(&payload)?).into(),
        payload,
    };
    let accepted_result: AcceptedResult =
        post_json(endpoint, "/v1/development/results", &token, &result).await?;
    println!(
        "{}",
        serde_json::to_string(&FixtureReceipt {
            status: "fixture_complete",
            task_id,
            step_id,
            accepted_result,
        })?
    );
    Ok(())
}

async fn get_json<T: DeserializeOwned>(
    endpoint: SocketAddr,
    path: &str,
    token: &str,
) -> anyhow::Result<T> {
    let response = http_client()?
        .get(endpoint_url(endpoint, path))
        .bearer_auth(token)
        .send()
        .await?;
    decode_response(response).await
}

async fn post_json<TRequest: Serialize + ?Sized, TResponse: DeserializeOwned>(
    endpoint: SocketAddr,
    path: &str,
    token: &str,
    request: &TRequest,
) -> anyhow::Result<TResponse> {
    let response = http_client()?
        .post(endpoint_url(endpoint, path))
        .bearer_auth(token)
        .json(request)
        .send()
        .await?;
    decode_response(response).await
}

async fn decode_response<T: DeserializeOwned>(
    mut response: reqwest::Response,
) -> anyhow::Result<T> {
    let status = response.status();
    if response
        .content_length()
        .is_some_and(|length| length > MAX_WIRE_FRAME_BYTES as u64)
    {
        bail!("master response exceeds the wire-frame limit");
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        if bytes.len().saturating_add(chunk.len()) > MAX_WIRE_FRAME_BYTES {
            bail!("master response exceeds the wire-frame limit");
        }
        bytes.extend_from_slice(&chunk);
    }
    if !status.is_success() {
        let detail = serde_json::from_slice::<ErrorResponse>(&bytes)
            .map(|value| value.error)
            .unwrap_or_else(|_| "invalid error response".to_string());
        bail!("master returned {status}: {detail}");
    }
    serde_json::from_slice(&bytes).context("decode master response")
}

fn http_client() -> anyhow::Result<reqwest::Client> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(5))
        .build()
        .context("build bounded master client")
}

fn endpoint_url(endpoint: SocketAddr, path: &str) -> String {
    format!("http://{endpoint}{path}")
}

fn require_loopback(address: SocketAddr) -> anyhow::Result<()> {
    if !address.ip().is_loopback() {
        bail!("Windows master development transport must use a loopback address");
    }
    Ok(())
}

fn require_concrete_remote_bind(address: SocketAddr) -> anyhow::Result<()> {
    if address.ip().is_unspecified() || address.ip().is_multicast() {
        bail!(
            "remote TLS bind must use a concrete local or private-overlay IP so the server certificate has an exact IP SAN"
        );
    }
    Ok(())
}

fn ensure_development_token(path: &Path) -> anyhow::Result<()> {
    if path.exists() {
        read_development_token(path)?;
        return Ok(());
    }

    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|error| anyhow::anyhow!("generate development token: {error}"))?;
    let token = hex(&bytes);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("create development token at {}", path.display()))?;
    file.write_all(token.as_bytes())?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    restrict_token_permissions(path)?;
    Ok(())
}

#[cfg(unix)]
fn restrict_token_permissions(path: &Path) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_token_permissions(_path: &Path) -> anyhow::Result<()> {
    Ok(())
}

fn read_development_token(path: &Path) -> anyhow::Result<String> {
    let raw = fs::read_to_string(path).with_context(|| {
        format!(
            "read development token at {}; run assemblywright-master setup first",
            path.display()
        )
    })?;
    let token = raw.trim();
    if token.len() < MIN_TOKEN_BYTES
        || token.len() > MAX_TOKEN_BYTES
        || !token.bytes().all(|byte| byte.is_ascii_graphic())
    {
        bail!("development token must contain 32-256 visible ASCII bytes");
    }
    Ok(token.to_string())
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_WINDOWS_SEED: [u8; 32] = [71; 32];
    const TEST_MAC_SEED: [u8; 32] = [72; 32];

    struct NativeBoundaryDispatcher {
        database_path: PathBuf,
        windows_executor_id: Uuid,
        mac_executor_id: Uuid,
        available: Arc<std::sync::atomic::AtomicBool>,
        observations: Arc<Mutex<Vec<String>>>,
    }

    impl AssemblyLineEffectDispatcher for NativeBoundaryDispatcher {
        fn runtime_status(
            &self,
        ) -> Option<assemblywright_master::AssemblyLineExecutionRuntimeStatus> {
            if !self.available.load(std::sync::atomic::Ordering::SeqCst) {
                return None;
            }
            Some(assemblywright_master::AssemblyLineExecutionRuntimeStatus {
                binding_revision: 1,
                dispatcher_sha256: [73; 32],
            })
        }

        fn dispatch_start(
            &self,
            intent: &assemblywright_master::AssemblyLineStartDispatchIntent,
        ) -> Result<
            Vec<assemblywright_protocol::ExecutionActivationReceipt>,
            assemblywright_master::AssemblyLineEffectDispatchError,
        > {
            let lifecycle: String = rusqlite::Connection::open(&self.database_path)
                .unwrap()
                .query_row(
                    "SELECT lifecycle FROM assembly_line_state WHERE singleton=1",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            self.observations
                .lock()
                .unwrap()
                .push(format!("start:{lifecycle}"));
            Ok(vec![
                signed_activation_receipt(
                    intent,
                    ExecutionHostPlatform::Windows,
                    self.windows_executor_id,
                    "windows.executor.receipts.v1",
                    TEST_WINDOWS_SEED,
                ),
                signed_activation_receipt(
                    intent,
                    ExecutionHostPlatform::Macos,
                    self.mac_executor_id,
                    "mac.executor.receipts.v1",
                    TEST_MAC_SEED,
                ),
            ])
        }

        fn dispatch_termination(
            &self,
            intent: &assemblywright_master::AssemblyLineTerminationIntent,
        ) -> Result<
            Vec<assemblywright_protocol::ExecutionTerminationReceipt>,
            assemblywright_master::AssemblyLineEffectDispatchError,
        > {
            let revoked: i64 = rusqlite::Connection::open(&self.database_path)
                .unwrap()
                .query_row(
                    "SELECT revoked FROM assembly_line_execution_authority WHERE singleton=1",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            self.observations
                .lock()
                .unwrap()
                .push(format!("termination:{revoked}"));
            Ok(vec![
                signed_termination_receipt(
                    intent,
                    ExecutionDescendantScope::WindowsJobObject,
                    "windows.executor.receipts.v1",
                    TEST_WINDOWS_SEED,
                ),
                signed_termination_receipt(
                    intent,
                    ExecutionDescendantScope::MacosProcessGroup,
                    "mac.executor.receipts.v1",
                    TEST_MAC_SEED,
                ),
            ])
        }
    }

    fn signed_activation_receipt(
        intent: &assemblywright_master::AssemblyLineStartDispatchIntent,
        host_platform: ExecutionHostPlatform,
        executor_id: Uuid,
        signer_key_id: &str,
        seed: [u8; 32],
    ) -> assemblywright_protocol::ExecutionActivationReceipt {
        let mut receipt = assemblywright_protocol::ExecutionActivationReceipt {
            schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
            receipt_id: Uuid::new_v4(),
            session_id: intent.session_id,
            child_epoch_id: intent.child_epoch_id,
            authority_revision: intent.authority_revision,
            host_platform,
            executor_id,
            executor_revision: 1,
            observed_at_ms: 1,
            signer_key_id: signer_key_id.to_string(),
            signature: Vec::new(),
        };
        let signing_key = seed.into();
        receipt.sign(&signing_key).unwrap();
        receipt
    }

    fn signed_termination_receipt(
        intent: &assemblywright_master::AssemblyLineTerminationIntent,
        descendant_scope: ExecutionDescendantScope,
        signer_key_id: &str,
        seed: [u8; 32],
    ) -> assemblywright_protocol::ExecutionTerminationReceipt {
        let mut receipt = assemblywright_protocol::ExecutionTerminationReceipt {
            schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
            receipt_id: Uuid::new_v4(),
            child_epoch_id: intent.child_epoch_id,
            mode: intent.mode,
            outcome: ExecutionTerminationOutcome::Reaped,
            tracked_root_process_count: 1,
            graceful_root_termination_count: 1,
            forced_root_termination_count: 0,
            reaped_root_process_count: 1,
            survivor_root_process_count: 0,
            descendant_scope,
            descendants_reaped: true,
            last_checkpoint_sha256: intent.checkpoint_sha256,
            observed_at_ms: 2,
            signer_key_id: signer_key_id.to_string(),
            signature: Vec::new(),
        };
        let signing_key = seed.into();
        receipt.sign(&signing_key).unwrap();
        receipt
    }

    fn test_verifying_key(seed: [u8; 32]) -> [u8; 32] {
        let signing_key = seed.into();
        let mut receipt = assemblywright_protocol::ExecutionTerminationReceipt {
            schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
            receipt_id: Uuid::new_v4(),
            child_epoch_id: Uuid::new_v4(),
            mode: ExecutionTerminationMode::Stop,
            outcome: ExecutionTerminationOutcome::Reaped,
            tracked_root_process_count: 1,
            graceful_root_termination_count: 1,
            forced_root_termination_count: 0,
            reaped_root_process_count: 1,
            survivor_root_process_count: 0,
            descendant_scope: ExecutionDescendantScope::WindowsJobObject,
            descendants_reaped: true,
            last_checkpoint_sha256: [1; 32],
            observed_at_ms: 1,
            signer_key_id: "key".to_string(),
            signature: Vec::new(),
        };
        receipt.sign(&signing_key).unwrap();
        signing_key.verifying_key().to_bytes()
    }

    #[test]
    fn native_execution_boundary_waits_for_signed_activation_and_revokes_before_termination() {
        let directory = tempfile::tempdir().unwrap();
        let lifecycle = RuntimeLifecycle::load(directory.path(), "test", "test-owner").unwrap();
        let process = MasterProcess::acquire(directory.path().join("master")).unwrap();
        let database_path = process.database_path().to_path_buf();
        let repository_id = Uuid::new_v4();
        let feature_id = Uuid::new_v4();
        let specification_id = Uuid::new_v4();
        let connection = rusqlite::Connection::open(&database_path).unwrap();
        connection
            .execute(
                "INSERT INTO assembly_line_repositories
                 (repository_id,git_url,repository_revision,lifecycle_revision,visibility,
                  approved_specification_id,approved_specification_revision,
                  approved_specification_sha256,owner_approval_sha256,lifecycle,effect_possible,
                  creation_evidence_sha256,created_at_ms)
                 VALUES(?1,'https://github.com/owner/native-boundary',1,2,'public',?2,1,
                        ?3,?4,'created',1,?5,1)",
                rusqlite::params![
                    repository_id.to_string(),
                    specification_id.to_string(),
                    [1_u8; 32].as_slice(),
                    [2_u8; 32].as_slice(),
                    [3_u8; 32].as_slice(),
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO assembly_line_queue
                 (feature_id,repository_id,specification_id,specification_revision,
                  specification_sha256,owner_approval_sha256,queue_position,
                  lifecycle_revision,lifecycle,enqueued_at_ms)
                 VALUES(?1,?2,?3,1,?4,?5,1,1,'queued',1)",
                rusqlite::params![
                    feature_id.to_string(),
                    repository_id.to_string(),
                    specification_id.to_string(),
                    [4_u8; 32].as_slice(),
                    [5_u8; 32].as_slice(),
                ],
            )
            .unwrap();
        connection
            .execute(
                "UPDATE assembly_line_state SET queue_revision=1 WHERE singleton=1",
                [],
            )
            .unwrap();
        drop(connection);

        let windows_executor_id = Uuid::new_v4();
        let mac_executor_id = Uuid::new_v4();
        let dispatcher_available = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let observations = Arc::new(Mutex::new(Vec::new()));
        let dispatcher = Arc::new(NativeBoundaryDispatcher {
            database_path: database_path.clone(),
            windows_executor_id,
            mac_executor_id,
            available: dispatcher_available.clone(),
            observations: observations.clone(),
        });
        let state = AppState {
            process: Arc::new(Mutex::new(process)),
            token_sha256: [1; 32],
            started_at_ms: 1,
            lifecycle,
            repository_snapshot_claim_reservation: Arc::new(tokio::sync::Mutex::new(())),
            artifact_integration_reservation: Arc::new(tokio::sync::Mutex::new(())),
            validation_gate_reservation: Arc::new(tokio::sync::Mutex::new(())),
            review_gateway_reservation: Arc::new(tokio::sync::Mutex::new(())),
            publication_reservation: Arc::new(tokio::sync::Mutex::new(())),
            review_provider: Arc::new(UnavailableReviewProvider),
            github_publication: None,
            planning_runtime: None,
            planning_runtime_status: None,
            planning_database_path: database_path.clone(),
            active_planning_calls: Arc::new(Mutex::new(Vec::new())),
            assembly_line_effect_dispatcher: dispatcher,
            validation_runtime: ValidationRuntime::Disabled,
        };
        state
            .process
            .lock()
            .unwrap()
            .kernel_mut()
            .record_assembly_line_execution_capabilities(
                &assemblywright_master::AssemblyLineExecutionCapabilityBinding {
                    binding_revision: 1,
                    expected_state_revision: 1,
                    expected_emergency_pause_revision: 0,
                    windows_executor_id,
                    windows_executor_revision: 1,
                    windows_executor_sha256: [11; 32],
                    mac_executor_id,
                    mac_executor_revision: 1,
                    mac_executor_sha256: [12; 32],
                    windows_broker_id: Uuid::new_v4(),
                    windows_broker_revision: 1,
                    windows_broker_sha256: [13; 32],
                    mac_broker_id: Uuid::new_v4(),
                    mac_broker_revision: 1,
                    mac_broker_sha256: [14; 32],
                    protected_control_plane_sha256: [15; 32],
                    windows_receipt_signer_key_id: "windows.executor.receipts.v1".to_string(),
                    windows_receipt_verifying_key: test_verifying_key(TEST_WINDOWS_SEED),
                    mac_receipt_signer_key_id: "mac.executor.receipts.v1".to_string(),
                    mac_receipt_verifying_key: test_verifying_key(TEST_MAC_SEED),
                    healthy: true,
                    provisioning_evidence_sha256: [16; 32],
                },
                2,
            )
            .unwrap();
        let mut start = AssemblyLineStartRequest {
            schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
            request_id: Uuid::new_v4(),
            expected_state_revision: 1,
            expected_queue_revision: 1,
            expected_emergency_pause_revision: 0,
            queue_count: 1,
            windows_executor_id,
            windows_executor_revision: 1,
            mac_executor_id,
            mac_executor_revision: 1,
            auto_run: true,
            owner_start_approval_sha256: [0; 32],
        };
        start.owner_start_approval_sha256 = start.canonical_owner_start_approval_sha256().unwrap();
        let (status, Json(running)) = execute_assembly_line_start(&state, &start, None).unwrap();
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            running.assembly_line.lifecycle,
            AssemblyLineLifecycleState::Running
        );

        let stop = AssemblyLineStopRequest {
            schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
            request_id: Uuid::new_v4(),
            session_id: running.assembly_line.session_id.unwrap(),
            expected_state_revision: running.assembly_line.state_revision,
            expected_child_epoch_id: running.assembly_line.active_child_epoch_id.unwrap(),
        };
        dispatcher_available.store(false, std::sync::atomic::Ordering::SeqCst);
        assert!(execute_assembly_line_termination(&state, Some(&stop), None, None).is_err());
        let lifecycle: String = rusqlite::Connection::open(&database_path)
            .unwrap()
            .query_row(
                "SELECT lifecycle FROM assembly_line_state WHERE singleton=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(lifecycle, "running");
        dispatcher_available.store(true, std::sync::atomic::Ordering::SeqCst);
        let (status, Json(paused)) =
            execute_assembly_line_termination(&state, Some(&stop), None, None).unwrap();
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            paused.assembly_line.lifecycle,
            AssemblyLineLifecycleState::PausedAtCheckpoint
        );
        assert_eq!(
            observations.lock().unwrap().as_slice(),
            ["start:starting", "termination:1"]
        );
    }

    fn pairing_invitation() -> EnrollmentInvitation {
        EnrollmentInvitation {
            schema_version: ENROLLMENT_PAIRING_SCHEMA_VERSION,
            status: ENROLLMENT_INVITATION_READY_STATUS.to_string(),
            grant_id: Uuid::parse_str("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa").unwrap(),
            device_id: DeviceId::new(
                Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            ),
            device_name: "owner-mac-bridge".to_string(),
            role: DeviceRole::MacBridge,
            registry_revision: 1,
            expires_at_ms: 2_000_000,
            capabilities: vec![CapabilityDescriptor {
                id: "mlx.reasoning".to_string(),
                kind: CapabilityKind::LocalInference,
                provider: "mlx".to_string(),
                model: "test-model".to_string(),
                max_context_bytes: 262_144,
                max_result_bytes: 786_432,
            }],
            master_endpoint: "100.64.23.14:7792".parse().unwrap(),
            ca_fingerprint_sha256: "ab".repeat(32),
        }
    }

    fn rotation_receipt() -> IssuedDeviceCertificate {
        serde_json::from_value(json!({
            "status": "device_certificate_issued",
            "operation": "rotate",
            "grant_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "device_id": "11111111-1111-4111-8111-111111111111",
            "device_name": "owner-mac-bridge",
            "role": "mac_bridge",
            "registry_revision": 1,
            "serial_hex": "01",
            "issued_at_ms": 1_000,
            "not_after_ms": 2_000,
            "certificate_sha256": "ab".repeat(32),
            "certificate_pem": "-----BEGIN CERTIFICATE-----\nZmFrZQ==\n-----END CERTIFICATE-----\n",
            "ca_certificate_pem": "-----BEGIN CERTIFICATE-----\nZmFrZQ==\n-----END CERTIFICATE-----\n"
        }))
        .unwrap()
    }

    #[test]
    fn constant_time_token_comparison_requires_exact_digest() {
        assert!(constant_time_equal(&[7; 32], &[7; 32]));
        assert!(!constant_time_equal(&[7; 32], &[8; 32]));
    }

    #[test]
    fn local_model_selection_route_classifies_only_deterministic_rejections_as_terminal() {
        for error in [
            MasterError::LocalModelSelectionRejected,
            MasterError::StaleOwnerControlDesignationRevision {
                expected: 1,
                found: 2,
            },
            MasterError::StaleEmergencyPauseRevision {
                expected: 1,
                found: 2,
            },
            MasterError::OwnerControlBridgeNotDesignated,
            MasterError::OwnerControlBridgeUnauthorized,
            MasterError::EmergencyPaused,
            MasterError::ConnectionNotActive,
            MasterError::ConnectionEpochMismatch,
            MasterError::InvalidRemoteWorkContract,
        ] {
            let (status, Json(body)) = local_model_selection_api_error(error);
            assert_eq!(status, StatusCode::CONFLICT);
            assert_eq!(body.error, "local_model_selection_rejected");
        }

        for error in [
            MasterError::IntegerOutOfRange,
            MasterError::InvalidStoredState("corrupt".to_string()),
            MasterError::InvalidSystemClock,
            MasterError::Storage(rusqlite::Error::InvalidQuery),
            MasterError::Json(serde_json::from_str::<Value>("{").unwrap_err()),
            MasterError::InvalidFeatureConveyorInput("audit".to_string()),
        ] {
            let (status, Json(body)) = local_model_selection_api_error(error);
            assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
            assert_eq!(body.error, "local_model_selection_internal_error");
        }
    }

    #[test]
    fn rotation_recovery_journal_is_bounded_private_strict_and_secret_free() {
        let directory = tempfile::tempdir().unwrap();
        let receipt = rotation_receipt();
        let grant_id = receipt.grant_id.unwrap();
        let path = rotation_recovery_receipt_path(directory.path(), grant_id).unwrap();
        write_rotation_recovery_receipt(&path, &receipt).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert!(!bytes.windows(12).any(|window| window == b"grant_secret"));
        let first = read_rotation_recovery_receipt(&path).unwrap();
        let second = read_rotation_recovery_receipt(&path).unwrap();
        assert_eq!(first, receipt);
        assert_eq!(second, first);
        let mut first_emission = Vec::new();
        let mut second_emission = Vec::new();
        write_json_line(&mut first_emission, &first).unwrap();
        write_json_line(&mut second_emission, &second).unwrap();
        assert_eq!(second_emission, first_emission);
        assert!(
            path.exists(),
            "recovery emission must not acknowledge cleanup"
        );
        assert!(write_rotation_recovery_receipt(&path, &receipt).is_err());

        let mut unknown: Value = serde_json::from_slice(&bytes).unwrap();
        unknown["unexpected"] = json!(true);
        fs::remove_file(&path).unwrap();
        fs::write(&path, serde_json::to_vec(&unknown).unwrap()).unwrap();
        restrict_rotation_recovery_file(&path).unwrap();
        assert!(read_rotation_recovery_receipt(&path).is_err());

        #[cfg(windows)]
        {
            validate_private_windows_rotation_acl(path.parent().unwrap()).unwrap();
            validate_private_windows_rotation_acl(&path).unwrap();
            let inherited = directory.path().join("not-owner-private.json");
            fs::write(&inherited, b"{}\n").unwrap();
            assert!(validate_private_windows_rotation_acl(&inherited).is_err());
        }
    }

    #[test]
    fn external_bind_is_rejected() {
        assert!(require_loopback("0.0.0.0:7791".parse().unwrap()).is_err());
        assert!(require_loopback("127.0.0.1:7791".parse().unwrap()).is_ok());
    }

    #[test]
    fn remote_bind_requires_a_concrete_certificate_identity() {
        assert!(require_concrete_remote_bind("0.0.0.0:7792".parse().unwrap()).is_err());
        assert!(require_concrete_remote_bind("127.0.0.1:7792".parse().unwrap()).is_ok());
        assert!(require_concrete_remote_bind("100.64.0.10:7792".parse().unwrap()).is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn repository_identity_preflight_rejects_windows_device_network_and_reparse_paths() {
        for forbidden in [
            r"\\server\share\repository",
            r"\\?\C:\repository",
            r"\\.\C:\repository",
            "//server/share/repository",
        ] {
            assert!(validate_windows_fixed_local_repository_path(forbidden).is_err());
        }
        let directory = tempfile::tempdir().unwrap();
        let held = open_windows_fixed_directory_chain(directory.path().to_str().unwrap()).unwrap();
        held.revalidate().unwrap();
        let directory_path =
            windows_final_dos_path_by_handle(held.handles.last().unwrap()).unwrap();
        require_windows_handle_path_matches_requested(
            held.handles.last().unwrap(),
            &directory_path,
        )
        .unwrap();
        drop(held);
        let long_name = directory_path.join("Repository Identity Long Name");
        std::fs::create_dir(&long_name).unwrap();
        if let Some(short_name) = windows_short_path(&long_name) {
            if !repository_paths_match(&short_name, &long_name) {
                let held =
                    open_windows_fixed_directory_chain(short_name.to_str().unwrap()).unwrap();
                assert!(require_windows_handle_path_matches_requested(
                    held.handles.last().unwrap(),
                    &short_name,
                )
                .is_err());
            }
        }
        let target = directory_path.join("target");
        let link = directory_path.join("junction-or-symlink");
        std::fs::create_dir(&target).unwrap();
        if std::os::windows::fs::symlink_dir(&target, &link).is_ok() {
            assert!(validate_windows_fixed_local_repository_path(link.to_str().unwrap()).is_err());
        }
    }

    #[cfg(windows)]
    #[test]
    fn repository_identity_handles_revalidate_pathnames_and_identity_files() {
        let repository = tempfile::tempdir().unwrap();
        let initial =
            open_windows_fixed_directory_chain(repository.path().to_str().unwrap()).unwrap();
        let repository_path =
            windows_final_dos_path_by_handle(initial.handles.last().unwrap()).unwrap();
        drop(initial);
        let git = repository_path.join(".git");
        std::fs::create_dir(&git).unwrap();
        std::fs::create_dir(git.join("objects")).unwrap();
        std::fs::create_dir_all(git.join("refs").join("heads")).unwrap();
        let commit = "1234567890abcdef1234567890abcdef12345678";
        std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
        std::fs::write(
            git.join("refs").join("heads").join("main"),
            format!("{commit}\n"),
        )
        .unwrap();
        let mut guard =
            observe_standard_repository_identity(repository_path.to_str().unwrap(), "main", commit)
                .unwrap();
        guard.revalidate().unwrap();

        let moved_repository = repository_path.with_extension("moved");
        if std::fs::rename(&repository_path, &moved_repository).is_ok() {
            assert!(guard.revalidate().is_err());
            drop(guard);
            std::fs::rename(&moved_repository, &repository_path).unwrap();
        } else {
            guard.revalidate().unwrap();
            drop(guard);
        }

        let mut guard =
            observe_standard_repository_identity(repository_path.to_str().unwrap(), "main", commit)
                .unwrap();
        let moved_head = git.join("HEAD.moved");
        if std::fs::rename(git.join("HEAD"), &moved_head).is_ok() {
            std::fs::write(git.join("HEAD"), "ref: refs/heads/main\n").unwrap();
            assert!(guard.revalidate().is_err());
            std::fs::remove_file(git.join("HEAD")).unwrap();
            drop(guard);
            std::fs::rename(&moved_head, git.join("HEAD")).unwrap();
        } else {
            guard.revalidate().unwrap();
            drop(guard);
        }
    }

    #[cfg(windows)]
    fn windows_short_path(path: &Path) -> Option<PathBuf> {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::GetShortPathNameW;

        let input = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        let mut output = vec![0_u16; 32_768];
        let length =
            unsafe { GetShortPathNameW(input.as_ptr(), output.as_mut_ptr(), output.len() as u32) }
                as usize;
        if length == 0 || length >= output.len() {
            return None;
        }
        Some(PathBuf::from(String::from_utf16(&output[..length]).ok()?))
    }

    #[test]
    fn setup_token_is_generated_without_printing_it() {
        let directory = tempfile::tempdir().unwrap();
        let token_path = directory.path().join(DEVELOPMENT_TOKEN_FILE);
        ensure_development_token(&token_path).unwrap();
        let token = read_development_token(&token_path).unwrap();
        assert_eq!(token.len(), 64);
        assert!(token.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    #[test]
    fn pairing_invitation_output_is_flushed_json_without_a_grant_secret() {
        let mut output = Vec::new();
        write_json_line(&mut output, &pairing_invitation()).unwrap();
        assert!(output.ends_with(b"\n"));
        let document: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(document["status"], ENROLLMENT_INVITATION_READY_STATUS);
        assert!(document.get("grant_secret").is_none());
        assert!(!String::from_utf8(output).unwrap().contains("grant_secret"));
    }

    #[test]
    fn pairing_reply_mismatches_and_expiry_fail_closed() {
        let invitation = pairing_invitation();
        let reply = EnrollmentCsrReply {
            schema_version: ENROLLMENT_PAIRING_SCHEMA_VERSION,
            status: assemblywright_protocol::ENROLLMENT_CSR_READY_STATUS.to_string(),
            grant_id: invitation.grant_id,
            device_id: invitation.device_id,
            csr_pem: "-----BEGIN CERTIFICATE REQUEST-----\npublic-key-only\n-----END CERTIFICATE REQUEST-----".to_string(),
        };
        validate_pairing_reply(&invitation, &reply, invitation.expires_at_ms - 1).unwrap();

        let mut wrong_grant = reply.clone();
        wrong_grant.grant_id = Uuid::new_v4();
        assert!(
            validate_pairing_reply(&invitation, &wrong_grant, invitation.expires_at_ms - 1)
                .unwrap_err()
                .to_string()
                .contains("grant_id")
        );

        let mut wrong_device = reply.clone();
        wrong_device.device_id = DeviceId::new(Uuid::new_v4());
        assert!(
            validate_pairing_reply(&invitation, &wrong_device, invitation.expires_at_ms - 1)
                .unwrap_err()
                .to_string()
                .contains("device_id")
        );

        assert!(
            validate_pairing_reply(&invitation, &reply, invitation.expires_at_ms)
                .unwrap_err()
                .to_string()
                .contains("expired")
        );
    }

    #[test]
    fn maintenance_marker_is_durable_and_invalid_state_fails_closed() {
        let directory = tempfile::tempdir().unwrap();
        assert_eq!(maintenance_snapshot(directory.path()), (false, None));

        write_maintenance_marker(directory.path(), "upgrade").unwrap();
        assert_eq!(
            maintenance_snapshot(directory.path()),
            (true, Some("upgrade".to_string()))
        );
        let lifecycle =
            RuntimeLifecycle::load(directory.path(), "windows_service", "LocalSystem").unwrap();
        assert_eq!(
            lifecycle.maintenance_snapshot(),
            (true, Some("upgrade".to_string()))
        );

        clear_maintenance_marker(directory.path()).unwrap();
        assert_eq!(maintenance_snapshot(directory.path()), (false, None));
        fs::write(directory.path().join(MAINTENANCE_MARKER_FILE), b"invalid").unwrap();
        assert_eq!(
            maintenance_snapshot(directory.path()),
            (true, Some("invalid_marker".to_string()))
        );
    }

    #[test]
    fn authoritative_pause_blocks_shared_work_admission_guard() {
        let directory = tempfile::tempdir().unwrap();
        let lifecycle =
            RuntimeLifecycle::load(directory.path(), "test", "test-owner").expect("load pause");
        let mut process = MasterProcess::acquire(directory.path().join("master")).unwrap();
        process
            .kernel_mut()
            .set_emergency_paused(true)
            .expect("activate authoritative pause");
        let state = AppState {
            process: Arc::new(Mutex::new(process)),
            token_sha256: [1; 32],
            started_at_ms: 1,
            lifecycle,
            repository_snapshot_claim_reservation: Arc::new(tokio::sync::Mutex::new(())),
            artifact_integration_reservation: Arc::new(tokio::sync::Mutex::new(())),
            validation_gate_reservation: Arc::new(tokio::sync::Mutex::new(())),
            review_gateway_reservation: Arc::new(tokio::sync::Mutex::new(())),
            publication_reservation: Arc::new(tokio::sync::Mutex::new(())),
            review_provider: Arc::new(UnavailableReviewProvider),
            github_publication: None,
            planning_runtime: None,
            planning_runtime_status: None,
            planning_database_path: directory.path().join("master/master.sqlite3"),
            active_planning_calls: Arc::new(Mutex::new(Vec::new())),
            assembly_line_effect_dispatcher: Arc::new(UnavailableAssemblyLineEffectDispatcher),
            validation_runtime: ValidationRuntime::Disabled,
        };
        let rejection = require_work_admission(&state).expect_err("pause must dominate work");
        assert_eq!(rejection.0, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(rejection.1.error, "emergency_pause_blocks_work");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn snapshot_claim_reservation_survives_blocking_task_timeout() {
        let reservation = Arc::new(tokio::sync::Mutex::new(()));
        let guard = reservation.clone().try_lock_owned().unwrap();
        assert!(reservation.clone().try_lock_owned().is_err());

        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let blocking = tokio::task::spawn_blocking(move || {
            let _guard = guard;
            let _ = started_tx.send(());
            release_rx.recv().unwrap();
        });
        started_rx.await.unwrap();
        assert!(tokio::time::timeout(Duration::from_millis(10), blocking)
            .await
            .is_err());
        assert!(
            reservation.clone().try_lock_owned().is_err(),
            "dropping a timed-out JoinHandle must not release its task-owned reservation"
        );

        release_tx.send(()).unwrap();
        let reacquired = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if let Ok(guard) = reservation.clone().try_lock_owned() {
                    break guard;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("blocking task eventually releases reservation");
        drop(reacquired);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn artifact_integration_reservation_survives_request_cancellation() {
        let reservation = Arc::new(tokio::sync::Mutex::new(()));
        let guard = reservation.clone().try_lock_owned().unwrap();
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let work = spawn_reserved_blocking(guard, move || {
            let _ = started_tx.send(());
            release_rx.recv().unwrap();
        });
        started_rx.await.unwrap();
        work.abort();
        tokio::task::yield_now().await;
        assert!(
            reservation.clone().try_lock_owned().is_err(),
            "cancelling the HTTP await must not release detached integration authority"
        );
        release_tx.send(()).unwrap();
        let reacquired = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                if let Ok(guard) = reservation.clone().try_lock_owned() {
                    break guard;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("detached integration work eventually releases reservation");
        drop(reacquired);
    }

    #[test]
    fn remote_fixture_registration_must_be_exact_and_fixture_only() {
        let mut registration = DeviceRegistration {
            device_id: DeviceId::new(Uuid::new_v4()),
            device_name: "fixture-worker".to_string(),
            role: DeviceRole::MacBridge,
            registry_revision: 1,
            capabilities: vec![CapabilityDescriptor::fixture_reasoning()],
        };
        assert!(registration_can_execute_fixture(&registration));

        registration.capabilities.push(CapabilityDescriptor {
            id: "mlx.reasoning".to_string(),
            kind: CapabilityKind::LocalInference,
            provider: "mlx".to_string(),
            model: "real-model".to_string(),
            max_context_bytes: 8_192,
            max_result_bytes: 8_192,
        });
        assert!(!registration_can_execute_fixture(&registration));
        registration.capabilities = vec![CapabilityDescriptor::fixture_reasoning()];
        registration.capabilities[0].model = "assemblywright-fixture-v2".to_string();
        assert!(!registration_can_execute_fixture(&registration));
    }

    // The rename to Assemblywright deliberately left these two identifiers
    // alone. The service name is what an installed Windows service is
    // registered under, and the exporter label is a wire contract the Mac
    // bridge asserts on its own side, so changing either orphans installed
    // state or silently breaks channel binding against an older peer. Pin them
    // so a future cosmetic rename pass fails here instead of in the field.
    #[test]
    fn windows_service_name_is_a_frozen_installed_state_contract() {
        assert_eq!(DEFAULT_SERVICE_NAME, "AssemblywrightMaster");
    }

    #[test]
    fn tls_exporter_label_is_a_frozen_wire_contract() {
        assert_eq!(
            TLS_EXPORTER_LABEL,
            b"EXPORTER-Assemblywright-Developer-Mode-v1"
        );
        assert_eq!(TLS_EXPORTER_BYTES, 32);
    }

    #[test]
    fn an_explicit_data_dir_overrides_the_installed_state_namespace() {
        let resolved = resolve_data_dir(Some(PathBuf::from("/explicit/override")))
            .expect("explicit data dir wins");
        assert_eq!(resolved, PathBuf::from("/explicit/override"));
    }

    #[test]
    fn a_pre_rename_state_directory_is_adopted_once() {
        let root = tempfile::tempdir().unwrap();
        let legacy = root
            .path()
            .join(LEGACY_MASTER_STATE_NAMESPACE)
            .join("master");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("master.sqlite3"), b"durable-kernel").unwrap();

        let current = root.path().join(MASTER_STATE_NAMESPACE).join("master");
        adopt_legacy_master_state(root.path(), &current).expect("adopt legacy state");

        // The durable kernel moved rather than being stranded or duplicated.
        assert_eq!(
            std::fs::read(current.join("master.sqlite3")).unwrap(),
            b"durable-kernel"
        );
        assert!(!legacy.exists(), "the legacy directory must not linger");

        // Idempotent: a second run is a no-op, not a second move.
        adopt_legacy_master_state(root.path(), &current).expect("second adopt is inert");
        assert!(current.join("master.sqlite3").is_file());
    }

    #[test]
    fn an_existing_current_state_directory_is_never_overwritten_by_the_legacy_one() {
        let root = tempfile::tempdir().unwrap();
        let legacy = root
            .path()
            .join(LEGACY_MASTER_STATE_NAMESPACE)
            .join("master");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("master.sqlite3"), b"stale").unwrap();
        let current = root.path().join(MASTER_STATE_NAMESPACE).join("master");
        std::fs::create_dir_all(&current).unwrap();
        std::fs::write(current.join("master.sqlite3"), b"authoritative").unwrap();

        adopt_legacy_master_state(root.path(), &current).expect("ambiguity must not clobber");

        // Two directories claiming authority is ambiguous, so the current one
        // wins untouched and the legacy one is left for the owner to inspect.
        assert_eq!(
            std::fs::read(current.join("master.sqlite3")).unwrap(),
            b"authoritative"
        );
        assert!(legacy.join("master.sqlite3").is_file());
    }

    #[test]
    fn a_missing_legacy_state_directory_is_not_an_error() {
        let root = tempfile::tempdir().unwrap();
        let current = root.path().join(MASTER_STATE_NAMESPACE).join("master");
        adopt_legacy_master_state(root.path(), &current).expect("nothing to adopt");
        assert!(!current.exists());
    }
}
