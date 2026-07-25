use anyhow::{bail, Context};
use assemblywright_agent::{
    AgentCursorSnapshot, AgentCursorStore, FixtureJobRuntime, FixtureRuntimeError, MlxJobRuntime,
    MlxRuntimeConfig, MlxRuntimeError, AGENT_SCHEMA_VERSION,
};
use assemblywright_core::{
    serve_router_unix_socket_with_peer_identity, validate_peer_code_requirement,
    validate_unix_socket_path, PeerIdentityProfile, MAX_PEER_CODE_REQUIREMENT_BYTES,
};
use assemblywright_protocol::{
    CancellationAcknowledgement, CancellationInstruction, DistributedEventBatch, JobEnvelope,
    JobResultEnvelope, MAX_WIRE_FRAME_BYTES, PROTOCOL_VERSION,
};
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::io::{self, Read};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use zeroize::{Zeroize, Zeroizing};

const AGENT_STARTUP_VERSION: u16 = 2;
const LEGACY_AGENT_STARTUP_VERSION: u16 = 1;
const MAX_AGENT_STARTUP_BYTES: usize = 16 * 1024;
const IPC_BEARER_TOKEN_BYTES: usize = 32;
const IPC_BEARER_TOKEN_LENGTH: usize = 43;

#[derive(Debug, Parser)]
#[command(
    name = "assemblywright-agent",
    version,
    about = "App-supervised Mac relay for Assemblywright Developer Mode"
)]
struct Cli {
    /// Owner-only directory containing the durable local event cursor.
    #[arg(long)]
    data_dir: PathBuf,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Serve the authenticated app-local relay using a bounded startup document from stdin.
    Serve,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AgentStartupDocument {
    version: u16,
    supervised_parent_pid: u32,
    socket_path: PathBuf,
    peer_code_requirement: String,
    peer_identity_profile: PeerIdentityProfile,
    bearer_token: String,
    #[serde(default)]
    fixture_jobs_enabled: bool,
    #[serde(default)]
    mlx_jobs_enabled: bool,
    #[serde(default)]
    mlx_executable_path: Option<PathBuf>,
    #[serde(default)]
    mlx_model_path: Option<PathBuf>,
    #[serde(default)]
    mlx_model_id: Option<String>,
}

#[derive(Clone)]
struct AgentState {
    store: Arc<Mutex<AgentCursorStore>>,
    token_sha256: [u8; 32],
    fixture_runtime: FixtureJobRuntime,
    fixture_jobs_enabled: bool,
    mlx_runtime: MlxJobRuntime,
    mlx_jobs_enabled: bool,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct AgentHealth {
    status: &'static str,
    mode: &'static str,
    protocol_version: u16,
    schema_version: i64,
    cursor: AgentCursorSnapshot,
    boundary: &'static str,
    fixture_jobs_enabled: bool,
    mlx_jobs_enabled: bool,
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct AcceptedBatch {
    status: &'static str,
    cursor: AgentCursorSnapshot,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_target(false)
        .without_time()
        .try_init()
        .ok();
    let cli = Cli::parse();
    match cli.command {
        Command::Serve => serve(cli.data_dir).await,
    }
}

async fn serve(data_dir: PathBuf) -> anyhow::Result<()> {
    #[cfg(not(target_os = "macos"))]
    {
        let _ = data_dir;
        bail!("jarvis-agent app supervision is supported only on macOS");
    }

    #[cfg(target_os = "macos")]
    {
        let mut startup_bytes = read_bounded_stdin()?;
        let mut startup: AgentStartupDocument = serde_json::from_slice(&startup_bytes)
            .context("decode bounded jarvis-agent startup document")?;
        startup_bytes.zeroize();
        validate_startup(&startup)?;
        verify_supervised_parent(startup.supervised_parent_pid)?;
        let mlx_config = mlx_config_from_startup(&startup)?;

        let token_sha256 = digest_bearer_token(&startup.bearer_token)?;
        startup.bearer_token.zeroize();

        let store = AgentCursorStore::open(data_dir).context("open durable agent cursor")?;
        let state = AgentState {
            store: Arc::new(Mutex::new(store)),
            token_sha256,
            fixture_runtime: FixtureJobRuntime::new(startup.fixture_jobs_enabled),
            fixture_jobs_enabled: startup.fixture_jobs_enabled,
            mlx_runtime: MlxJobRuntime::new(mlx_config),
            mlx_jobs_enabled: startup.mlx_jobs_enabled,
        };
        let app = Router::new()
            .route("/health", get(health))
            .route("/v1/events/accept", post(accept_events))
            .route("/v1/jobs/execute", post(execute_fixture_job))
            .route("/v1/jobs/cancel", post(cancel_fixture_job))
            .route("/v1/mlx/jobs/execute", post(execute_mlx_job))
            .route("/v1/mlx/jobs/cancel", post(cancel_mlx_job))
            .layer(DefaultBodyLimit::max(MAX_WIRE_FRAME_BYTES))
            .layer(middleware::from_fn_with_state(
                state.clone(),
                enforce_bearer,
            ))
            .with_state(state.clone());

        spawn_parent_watcher(startup.supervised_parent_pid);
        let serve_result = serve_router_unix_socket_with_peer_identity(
            &startup.socket_path,
            app,
            &startup.peer_code_requirement,
            startup.peer_identity_profile,
        )
        .await;
        state
            .mlx_runtime
            .shutdown_active()
            .await
            .context("reap active MLX backend during agent shutdown")?;
        serve_result
    }
}

fn read_bounded_stdin() -> anyhow::Result<Zeroizing<Vec<u8>>> {
    let mut bytes = Zeroizing::new(Vec::new());
    io::stdin()
        .take((MAX_AGENT_STARTUP_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .context("read jarvis-agent startup document")?;
    if bytes.is_empty() || bytes.len() > MAX_AGENT_STARTUP_BYTES {
        bail!("jarvis-agent startup document must contain 1 to {MAX_AGENT_STARTUP_BYTES} bytes");
    }
    Ok(bytes)
}

fn validate_startup(startup: &AgentStartupDocument) -> anyhow::Result<()> {
    if startup.version != AGENT_STARTUP_VERSION && startup.version != LEGACY_AGENT_STARTUP_VERSION {
        bail!("unsupported jarvis-agent startup document version");
    }
    if startup.version == LEGACY_AGENT_STARTUP_VERSION
        && (startup.mlx_jobs_enabled
            || startup.mlx_executable_path.is_some()
            || startup.mlx_model_path.is_some()
            || startup.mlx_model_id.is_some())
    {
        bail!("legacy jarvis-agent startup documents cannot configure MLX jobs");
    }
    if startup.fixture_jobs_enabled && startup.mlx_jobs_enabled {
        bail!("fixture and MLX job runtimes cannot be enabled together");
    }
    if startup.supervised_parent_pid == 0 {
        bail!("jarvis-agent requires a nonzero supervised parent PID");
    }
    validate_unix_socket_path(&startup.socket_path).map_err(anyhow::Error::new)?;
    if startup.peer_code_requirement.is_empty()
        || startup.peer_code_requirement.len() > MAX_PEER_CODE_REQUIREMENT_BYTES
        || startup.peer_code_requirement.as_bytes().contains(&0)
    {
        bail!("jarvis-agent peer code requirement is invalid");
    }
    validate_peer_code_requirement(
        &startup.peer_code_requirement,
        startup.peer_identity_profile,
    )
    .map_err(anyhow::Error::new)?;
    let _ = digest_bearer_token(&startup.bearer_token)?;
    let _ = mlx_config_from_startup(startup)?;
    Ok(())
}

fn mlx_config_from_startup(
    startup: &AgentStartupDocument,
) -> anyhow::Result<Option<MlxRuntimeConfig>> {
    match (
        startup.mlx_jobs_enabled,
        startup.mlx_executable_path.clone(),
        startup.mlx_model_path.clone(),
        startup.mlx_model_id.clone(),
    ) {
        (false, None, None, None) => Ok(None),
        (true, Some(executable), Some(model), Some(model_id)) => {
            MlxRuntimeConfig::validate(executable, model, model_id)
                .map(Some)
                .map_err(|_| anyhow::anyhow!("jarvis-agent MLX startup configuration is invalid"))
        }
        _ => bail!(
            "jarvis-agent MLX paths and model identifier are required iff MLX jobs are enabled"
        ),
    }
}

fn digest_bearer_token(token: &str) -> anyhow::Result<[u8; 32]> {
    if token.len() != IPC_BEARER_TOKEN_LENGTH
        || !token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        bail!("jarvis-agent bearer token is invalid");
    }
    let decoded = Zeroizing::new(
        URL_SAFE_NO_PAD
            .decode(token)
            .context("jarvis-agent bearer token is invalid")?,
    );
    if decoded.len() != IPC_BEARER_TOKEN_BYTES {
        bail!("jarvis-agent bearer token is invalid");
    }
    Ok(Sha256::digest(token.as_bytes()).into())
}

#[cfg(target_os = "macos")]
fn verify_supervised_parent(expected_parent_pid: u32) -> anyhow::Result<()> {
    let actual_parent_pid = unsafe { libc::getppid() };
    if actual_parent_pid <= 0 || actual_parent_pid as u32 != expected_parent_pid {
        bail!("jarvis-agent direct parent does not match the startup document");
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn spawn_parent_watcher(expected_parent_pid: u32) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(500));
        loop {
            interval.tick().await;
            let actual_parent_pid = unsafe { libc::getppid() };
            if actual_parent_pid <= 0 || actual_parent_pid as u32 != expected_parent_pid {
                unsafe {
                    libc::kill(libc::getpid(), libc::SIGTERM);
                }
                break;
            }
        }
    });
}

async fn enforce_bearer(
    State(state): State<AgentState>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let mut values = headers.get_all(header::AUTHORIZATION).iter();
    let authorized = match (values.next(), values.next()) {
        (Some(value), None) => value
            .to_str()
            .ok()
            .and_then(|value| value.strip_prefix("Bearer "))
            .is_some_and(|candidate| {
                candidate.len() == IPC_BEARER_TOKEN_LENGTH
                    && constant_time_equal(
                        &Sha256::digest(candidate.as_bytes()).into(),
                        &state.token_sha256,
                    )
            }),
        _ => false,
    };
    if authorized {
        return next.run(request).await;
    }
    (
        StatusCode::UNAUTHORIZED,
        [(header::WWW_AUTHENTICATE, "Bearer")],
        Json(json!({"error":"unauthorized"})),
    )
        .into_response()
}

async fn health(
    State(state): State<AgentState>,
) -> Result<Json<AgentHealth>, (StatusCode, Json<serde_json::Value>)> {
    let cursor = state
        .store
        .lock()
        .map_err(|_| internal_error())?
        .snapshot()
        .map_err(|_| internal_error())?;
    Ok(Json(AgentHealth {
        status: "ok",
        mode: "developer_event_relay",
        protocol_version: PROTOCOL_VERSION,
        schema_version: AGENT_SCHEMA_VERSION,
        cursor,
        boundary: if state.fixture_jobs_enabled {
            "metadata_cursor_plus_in_memory_public_fixture_jobs_no_retention"
        } else if state.mlx_jobs_enabled {
            "metadata_cursor_plus_bounded_public_mlx_jobs_no_retention"
        } else {
            "metadata_only_no_authoritative_state"
        },
        fixture_jobs_enabled: state.fixture_jobs_enabled,
        mlx_jobs_enabled: state.mlx_jobs_enabled,
    }))
}

async fn execute_mlx_job(
    State(state): State<AgentState>,
    Json(job): Json<JobEnvelope>,
) -> Result<Json<JobResultEnvelope>, (StatusCode, Json<serde_json::Value>)> {
    state
        .mlx_runtime
        .execute(job)
        .await
        .map(Json)
        .map_err(mlx_error)
}

async fn cancel_mlx_job(
    State(state): State<AgentState>,
    Json(instruction): Json<CancellationInstruction>,
) -> Result<Json<CancellationAcknowledgement>, (StatusCode, Json<serde_json::Value>)> {
    state
        .mlx_runtime
        .cancel(&instruction)
        .await
        .map(Json)
        .map_err(mlx_error)
}

fn mlx_error(error: MlxRuntimeError) -> (StatusCode, Json<serde_json::Value>) {
    match error {
        MlxRuntimeError::Disabled => (
            StatusCode::FORBIDDEN,
            Json(json!({"error":"mlx_jobs_disabled"})),
        ),
        MlxRuntimeError::Cancelled => {
            (StatusCode::CONFLICT, Json(json!({"error":"job_cancelled"})))
        }
        MlxRuntimeError::NotActive => (
            StatusCode::NOT_FOUND,
            Json(json!({"error":"job_not_active"})),
        ),
        MlxRuntimeError::AlreadyActive => (
            StatusCode::CONFLICT,
            Json(json!({"error":"job_already_active"})),
        ),
        MlxRuntimeError::Protocol(_) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error":"mlx_contract_rejected"})),
        ),
        MlxRuntimeError::Timeout => (
            StatusCode::GATEWAY_TIMEOUT,
            Json(json!({"error":"mlx_backend_timeout"})),
        ),
        MlxRuntimeError::InvalidOutput | MlxRuntimeError::BackendFailed => (
            StatusCode::BAD_GATEWAY,
            Json(json!({"error":"mlx_backend_failed"})),
        ),
        MlxRuntimeError::InvalidConfiguration
        | MlxRuntimeError::CleanupFailed
        | MlxRuntimeError::Unavailable => internal_error(),
    }
}

async fn execute_fixture_job(
    State(state): State<AgentState>,
    Json(job): Json<JobEnvelope>,
) -> Result<Json<JobResultEnvelope>, (StatusCode, Json<serde_json::Value>)> {
    state
        .fixture_runtime
        .execute(job)
        .await
        .map(Json)
        .map_err(fixture_error)
}

async fn cancel_fixture_job(
    State(state): State<AgentState>,
    Json(instruction): Json<CancellationInstruction>,
) -> Result<Json<CancellationAcknowledgement>, (StatusCode, Json<serde_json::Value>)> {
    state
        .fixture_runtime
        .cancel(&instruction)
        .map(Json)
        .map_err(fixture_error)
}

fn fixture_error(error: FixtureRuntimeError) -> (StatusCode, Json<serde_json::Value>) {
    match error {
        FixtureRuntimeError::Disabled => (
            StatusCode::FORBIDDEN,
            Json(json!({"error":"fixture_jobs_disabled"})),
        ),
        FixtureRuntimeError::Cancelled => {
            (StatusCode::CONFLICT, Json(json!({"error":"job_cancelled"})))
        }
        FixtureRuntimeError::NotActive => (
            StatusCode::NOT_FOUND,
            Json(json!({"error":"job_not_active"})),
        ),
        FixtureRuntimeError::AlreadyActive => (
            StatusCode::CONFLICT,
            Json(json!({"error":"job_already_active"})),
        ),
        FixtureRuntimeError::Protocol(_) => (
            StatusCode::UNPROCESSABLE_ENTITY,
            Json(json!({"error":"fixture_contract_rejected"})),
        ),
        FixtureRuntimeError::Unavailable | FixtureRuntimeError::Serialization => internal_error(),
    }
}

async fn accept_events(
    State(state): State<AgentState>,
    Json(batch): Json<DistributedEventBatch>,
) -> Result<Json<AcceptedBatch>, (StatusCode, Json<serde_json::Value>)> {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| internal_error())?
        .as_millis()
        .try_into()
        .map_err(|_| internal_error())?;
    let cursor = state
        .store
        .lock()
        .map_err(|_| internal_error())?
        .accept_batch(&batch, now_ms)
        .map_err(|error| match error {
            assemblywright_agent::AgentError::EventStreamMismatch
            | assemblywright_agent::AgentError::EventCursorGap
            | assemblywright_agent::AgentError::Protocol(_) => (
                StatusCode::CONFLICT,
                Json(json!({"error":"event_cursor_rejected"})),
            ),
            _ => internal_error(),
        })?;
    Ok(Json(AcceptedBatch {
        status: "accepted",
        cursor,
    }))
}

fn internal_error() -> (StatusCode, Json<serde_json::Value>) {
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error":"agent_unavailable"})),
    )
}

fn constant_time_equal(left: &[u8; 32], right: &[u8; 32]) -> bool {
    left.iter()
        .zip(right.iter())
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}
