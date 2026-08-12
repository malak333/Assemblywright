use anyhow::{bail, Context};
use assemblywright_master::{
    current_time_ms, AcceptedCancellation, AcceptedResult, ApprovedFeatureSpecification,
    ArtifactIntegrationAuthorization, ArtifactIntegrationError, CapabilityRebindAcknowledgement,
    DeviceRegistration, EnrollmentGrantSpec, EnrollmentRequest, EphemeralServerIdentity,
    FeatureAbandonmentEvidence, FeatureConveyorStatus, FeatureGrantRevisions,
    FeatureSnapshotClaimPlan, IdentityAuthority, MasterHealthSnapshot, MasterProcess, NewStep,
    PlatformSecretProtector, RemoteWorkContract, RepositoryGrantKind, RepositoryGrantRevision,
    RepositorySnapshotEvidence, RepositorySnapshotStore, ResultArtifactReference,
    StartupReconciliation,
};
#[cfg(test)]
use assemblywright_protocol::CapabilityKind;
use assemblywright_protocol::{
    feature_conveyor_provider_binding_sha256, repository_preflight_fingerprint_sha256,
    AuthenticatedHandshakeRequest, CancellationAcknowledgement, CancellationPollRequest,
    CapabilityDescriptor, DeviceId, DeviceRole, DistributedEventBatch,
    DistributedEventBatchRequest, EnrollmentCsrReply, EnrollmentInvitation,
    FeatureConveyorAbandonAndAdvanceReceipt, FeatureConveyorAbandonAndAdvanceRequest,
    FeatureConveyorAbandonAndAdvanceStatus, FeatureConveyorApprovedFeatureReceipt,
    FeatureConveyorApprovedFeatureRequest, FeatureConveyorApprovedFeatureStatus,
    FeatureConveyorArtifactIntegrationPlan, FeatureConveyorArtifactIntegrationRequest,
    FeatureConveyorCancelActiveFeatureReceipt, FeatureConveyorCancelActiveFeatureRequest,
    FeatureConveyorCancelActiveFeatureStatus, FeatureConveyorCodingDispatchReceipt,
    FeatureConveyorCodingDispatchRequest, FeatureConveyorOwnerBridgeDesignationReceipt,
    FeatureConveyorOwnerBridgeDesignationRequest, FeatureConveyorOwnerBridgeDesignationStatus,
    FeatureConveyorRepositoryGrantKind, FeatureConveyorRepositoryGrantReceipt,
    FeatureConveyorRepositoryGrantRequest, FeatureConveyorRepositoryGrantSet,
    FeatureConveyorRepositoryGrantStatus, FeatureConveyorRepositoryPreflightReceipt,
    FeatureConveyorRepositoryPreflightRequest, FeatureConveyorRepositoryPreflightStatus,
    FeatureConveyorRepositorySnapshotClaimReceipt, FeatureConveyorRepositorySnapshotClaimRequest,
    FeatureConveyorRepositorySnapshotClaimStatus, FixtureJobResult, HandshakeRequest,
    HandshakeResponse, HandshakeStatus, JobEnvelope, JobResultEnvelope, JobResultStatus,
    LocalCodingJobResult, LocalCodingResultArtifactAdmission, LocalCodingResultArtifactReceipt,
    LocalCodingSnapshotChunk, LocalCodingSnapshotChunkRequest, Sensitivity, StepId, TaskId,
    ENROLLMENT_INVITATION_READY_STATUS, ENROLLMENT_PAIRING_SCHEMA_VERSION,
    FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION, MAX_ENROLLMENT_PAIRING_FRAME_BYTES,
    MAX_FEATURE_CONVEYOR_CODING_DISPATCH_REQUEST_BYTES,
    MAX_FEATURE_CONVEYOR_OWNER_RESOLUTION_REQUEST_BYTES,
    MAX_FEATURE_CONVEYOR_REPOSITORY_PREFLIGHT_REQUEST_BYTES,
    MAX_FEATURE_CONVEYOR_SNAPSHOT_CLAIM_REQUEST_BYTES, MAX_LOCAL_CODING_SNAPSHOT_CHUNK_BYTES,
    MAX_WIRE_FRAME_BYTES, PROTOCOL_VERSION,
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
use std::sync::{Arc, Mutex};
use std::time::Duration;
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
        EnrollmentCommand::Pair { .. } | EnrollmentCommand::RebindPair { .. } => {
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
    let state = AppState {
        process: Arc::new(Mutex::new(process)),
        token_sha256: Sha256::digest(token.as_bytes()).into(),
        started_at_ms: current_time_ms()?,
        lifecycle,
        repository_snapshot_claim_reservation: Arc::new(tokio::sync::Mutex::new(())),
        artifact_integration_reservation: Arc::new(tokio::sync::Mutex::new(())),
    };

    let app = Router::new()
        .route("/health", get(get_health))
        .route(
            "/v1/feature-conveyor/status",
            get(get_feature_conveyor_status),
        )
        .route(
            "/v1/feature-conveyor/owner-control-bridge",
            post(designate_owner_control_bridge),
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
            "/v1/distributed/feature-conveyor/approved-features",
            post(remote_enqueue_approved_feature),
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
    process
        .kernel()
        .authenticate_device_certificate(
            session.registration.device_id,
            &session.certificate_serial_hex,
            &session.certificate_sha256,
            current_time_ms().map_err(api_error)?,
        )
        .map_err(|_| unauthorized())
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
    lock_process(&state)?
        .kernel_mut()
        .set_emergency_paused(true)
        .map_err(api_error)?;
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

    #[test]
    fn constant_time_token_comparison_requires_exact_digest() {
        assert!(constant_time_equal(&[7; 32], &[7; 32]));
        assert!(!constant_time_equal(&[7; 32], &[8; 32]));
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
