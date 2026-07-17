use anyhow::{bail, Context};
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use clap::{Parser, Subcommand, ValueEnum};
use hyper::server::conn::http1;
use hyper_util::rt::TokioIo;
use hyper_util::service::TowerToHyperService;
use jarvis_master::{
    current_time_ms, AcceptedResult, DeviceRegistration, EnrollmentGrantSpec, EnrollmentRequest,
    EphemeralServerIdentity, IdentityAuthority, MasterHealthSnapshot, MasterProcess, NewStep,
    PlatformSecretProtector, StartupReconciliation,
};
use jarvis_protocol::{
    AuthenticatedHandshakeRequest, CapabilityDescriptor, CapabilityKind, DeviceId, DeviceRole,
    HandshakeRequest, HandshakeResponse, HandshakeStatus, JobEnvelope, JobResultEnvelope,
    JobResultStatus, Sensitivity, StepId, TaskId, MAX_WIRE_FRAME_BYTES, PROTOCOL_VERSION,
};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use rustls::server::WebPkiClientVerifier;
use rustls::{RootCertStore, ServerConfig};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_rustls::TlsAcceptor;
use tracing::info;
use uuid::Uuid;
use x509_parser::extensions::GeneralName;
use x509_parser::prelude::FromDer;
use zeroize::Zeroize;

#[cfg(windows)]
mod windows_service_host;

const DEFAULT_BIND: &str = "127.0.0.1:7791";
const TLS_EXPORTER_LABEL: &[u8] = b"EXPORTER-Jarvis-Developer-Mode-v1";
const TLS_EXPORTER_BYTES: usize = 32;
const DEVELOPMENT_TOKEN_FILE: &str = "development.token";
const MIN_TOKEN_BYTES: usize = 32;
const MAX_TOKEN_BYTES: usize = 256;
const DEFAULT_SERVICE_NAME: &str = "JarvisMaster";
const MAINTENANCE_MARKER_FILE: &str = "maintenance-mode.json";

#[derive(Debug, Parser)]
#[command(
    name = "jarvis-master",
    version,
    about = "Headless Windows master foundation for Jarvis Developer Mode"
)]
struct Cli {
    /// Master state directory. Defaults to %LOCALAPPDATA%\Jarvis\master on Windows.
    #[arg(long, env = "JARVIS_MASTER_DATA_DIR", global = true)]
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
    fn as_str(self) -> &'static str {
        match self {
            Self::OperatorRequest => "operator_request",
            Self::Upgrade => "upgrade",
            Self::Restore => "restore",
            Self::Recovery => "recovery",
        }
    }
}

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
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("jarvis_master=info")),
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
        .context("create the Jarvis master service runtime")?;
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
                    .context("report the ready Jarvis master service state")
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
        .context("service installation requires prior `jarvis-master setup`")?;
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
        "--data-dir or JARVIS_MASTER_DATA_DIR is required when LOCALAPPDATA is unavailable",
    )?;
    Ok(PathBuf::from(local_app_data).join("Jarvis").join("master"))
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
    let mut bytes = Vec::new();
    std::io::stdin()
        .take((MAX_WIRE_FRAME_BYTES + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_WIRE_FRAME_BYTES {
        bail!("stdin document exceeds the wire-frame limit");
    }
    if bytes.is_empty() {
        bail!("stdin document is empty");
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
    };

    let app = Router::new()
        .route("/health", get(get_health))
        .route("/v1/development/devices/register", post(register_device))
        .route("/v1/development/connections/accept", post(accept_handshake))
        .route("/v1/development/steps", post(enqueue_step))
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
            GeneralName::URI(uri) => uri.strip_prefix("urn:jarvis:device:"),
            _ => None,
        });
    let device_id = device_ids
        .next()
        .context("enrolled device certificate omitted its Jarvis device URI")?;
    if device_ids.next().is_some() {
        bail!("enrolled device certificate contains multiple Jarvis device URIs");
    }
    let device_id =
        DeviceId::new(Uuid::parse_str(device_id).context("parse certificate device ID")?);
    Ok((device_id, hex(certificate.raw_serial())))
}

fn remote_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(remote_get_health))
        .route(
            "/v1/distributed/connections/accept",
            post(remote_accept_handshake),
        )
        .route("/v1/distributed/steps", post(remote_enqueue_step))
        .route("/v1/distributed/leases/next", post(remote_lease_next))
        .route("/v1/distributed/results", post(remote_accept_result))
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
    revalidate_remote_session(&state, &session)?;
    let (maintenance_active, maintenance_reason) = state.lifecycle.maintenance_snapshot();
    let process = lock_process(&state)?;
    Ok(Json(HealthResponse {
        status: if maintenance_active { "maintenance" } else { "ok" }.to_string(),
        mode: "developer_remote_master".to_string(),
        host_mode: state.lifecycle.host_mode.clone(),
        service_identity: state.lifecycle.service_identity.clone(),
        maintenance_active,
        maintenance_reason,
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

async fn remote_enqueue_step(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    Json(step): Json<NewStep>,
) -> ApiResult<AcceptedResponse> {
    require_work_admission(&state)?;
    let registration = require_remote_application_session(&state, &session, None)?;
    if registration.role != DeviceRole::MacBridge {
        return Err(unauthorized());
    }
    lock_process(&state)?
        .kernel_mut()
        .enqueue_step(&step, current_time_ms().map_err(api_error)?)
        .map_err(api_error)?;
    Ok(Json(AcceptedResponse { accepted: true }))
}

async fn remote_lease_next(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    Json(request): Json<LeaseRequest>,
) -> ApiResult<JobEnvelope> {
    require_work_admission(&state)?;
    let registration =
        require_remote_application_session(&state, &session, Some(request.connection_epoch))?;
    if request.device_id != registration.device_id {
        return Err(unauthorized());
    }
    let job = lock_process(&state)?
        .kernel_mut()
        .lease_next_step(
            registration.device_id,
            request.connection_epoch,
            current_time_ms().map_err(api_error)?,
        )
        .map_err(api_error)?;
    Ok(Json(job))
}

async fn remote_accept_result(
    State(state): State<AppState>,
    Extension(session): Extension<RemoteSession>,
    Json(result): Json<JobResultEnvelope>,
) -> ApiResult<AcceptedResult> {
    require_remote_application_session(&state, &session, Some(result.connection_epoch))?;
    let accepted = lock_process(&state)?
        .kernel_mut()
        .accept_result(&result, current_time_ms().map_err(api_error)?)
        .map_err(api_error)?;
    Ok(Json(accepted))
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
    Ok(Json(HealthResponse {
        status: if maintenance_active { "maintenance" } else { "ok" }.to_string(),
        mode: "developer_foundation".to_string(),
        host_mode: state.lifecycle.host_mode.clone(),
        service_identity: state.lifecycle.service_identity.clone(),
        maintenance_active,
        maintenance_reason,
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

async fn fetch_health_value(data_dir: &Path, endpoint: SocketAddr) -> anyhow::Result<Value> {
    Ok(serde_json::to_value(
        fetch_health(data_dir, endpoint).await?,
    )?)
}

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
    let capability = CapabilityDescriptor {
        id: "fixture.reasoning".to_string(),
        kind: CapabilityKind::LocalInference,
        provider: "cross-process-fixture".to_string(),
        model: "deterministic-fixture".to_string(),
        max_context_bytes: 262_144,
        max_result_bytes: 786_432,
    };
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
        sensitivity: Sensitivity::Workspace,
        context: json!({"prompt": prompt, "retain": false}),
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
    let payload: Value = json!({
        "summary": "cross-process fixture completed",
        "context_sha256": hex(&job.context_sha256)
    });
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
            "read development token at {}; run jarvis-master setup first",
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
}
