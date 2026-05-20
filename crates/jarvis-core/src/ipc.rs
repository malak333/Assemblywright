use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use crate::{
    AuditEntry, JarvisError, JarvisResult, Scheduler, SchedulerJob, SchedulerJobSpec, TaskRecord,
    TaskStatus, TriggerKind,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub started_at: DateTime<Utc>,
    pub emergency_paused: bool,
    pub scheduler_jobs: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRequest {
    pub input: String,
    #[serde(default)]
    pub session_id: Option<Uuid>,
    #[serde(default)]
    pub context: serde_json::Value,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResponse {
    pub accepted: bool,
    pub task: TaskRecord,
    pub audit_entry: AuditEntry,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergencyPauseRequest {
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergencyPauseResponse {
    pub paused: bool,
    pub reason: Option<String>,
    pub paused_at: Option<DateTime<Utc>>,
    pub resumed_at: Option<DateTime<Utc>>,
    pub cancelled_scheduler_jobs: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSchedulerJobRequest {
    pub name: String,
    pub command: String,
    pub trigger: TriggerKind,
}

#[derive(Debug, Clone)]
struct EmergencyPauseState {
    paused: bool,
    reason: Option<String>,
    paused_at: Option<DateTime<Utc>>,
    resumed_at: Option<DateTime<Utc>>,
}

impl Default for EmergencyPauseState {
    fn default() -> Self {
        Self {
            paused: false,
            reason: None,
            paused_at: None,
            resumed_at: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct IpcState {
    version: String,
    started_at: DateTime<Utc>,
    scheduler: Scheduler,
    emergency_pause: Arc<Mutex<EmergencyPauseState>>,
}

impl Default for IpcState {
    fn default() -> Self {
        Self::new()
    }
}

impl IpcState {
    pub fn new() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            started_at: Utc::now(),
            scheduler: Scheduler::new(),
            emergency_pause: Arc::new(Mutex::new(EmergencyPauseState::default())),
        }
    }

    pub fn scheduler(&self) -> Scheduler {
        self.scheduler.clone()
    }

    pub fn health(&self) -> HealthResponse {
        HealthResponse {
            status: "ok".to_string(),
            version: self.version.clone(),
            started_at: self.started_at,
            emergency_paused: self.is_paused(),
            scheduler_jobs: self.scheduler.list().len(),
        }
    }

    pub fn submit_command(&self, request: CommandRequest) -> JarvisResult<CommandResponse> {
        if request.input.trim().is_empty() {
            return Err(JarvisError::Validation(
                "command input cannot be empty".to_string(),
            ));
        }

        let now = Utc::now();
        let mut task = TaskRecord {
            id: Uuid::new_v4(),
            session_id: request.session_id.unwrap_or_else(Uuid::new_v4),
            user_input: request.input.clone(),
            status: TaskStatus::Created,
            created_at: now,
            updated_at: now,
        };

        if self.is_paused() {
            task.status = TaskStatus::Blocked;
            task.updated_at = Utc::now();
            let audit_entry = AuditEntry::new(
                Some(task.id),
                "command.blocked",
                "command blocked by emergency pause",
                json!({
                    "input": request.input,
                    "dry_run": request.dry_run,
                    "context": request.context,
                }),
            );

            return Ok(CommandResponse {
                accepted: false,
                task,
                audit_entry,
                message: "emergency pause is active".to_string(),
            });
        }

        task.status = TaskStatus::Completed;
        task.updated_at = Utc::now();
        let audit_entry = AuditEntry::new(
            Some(task.id),
            "command.accepted",
            "command accepted by IPC stub",
            json!({
                "input": request.input,
                "dry_run": request.dry_run,
                "context": request.context,
                "execution": "stub",
            }),
        );

        Ok(CommandResponse {
            accepted: true,
            task,
            audit_entry,
            message: "command accepted; execution pipeline is stubbed".to_string(),
        })
    }

    pub fn pause(&self, reason: impl Into<String>) -> EmergencyPauseResponse {
        let reason = reason.into();
        let cancelled = self
            .scheduler
            .cancel_active(format!("emergency pause: {reason}"));
        let paused_at = Utc::now();
        let mut pause = self
            .emergency_pause
            .lock()
            .expect("emergency pause lock poisoned");

        pause.paused = true;
        pause.reason = Some(reason);
        pause.paused_at = Some(paused_at);
        pause.resumed_at = None;

        EmergencyPauseResponse {
            paused: pause.paused,
            reason: pause.reason.clone(),
            paused_at: pause.paused_at,
            resumed_at: pause.resumed_at,
            cancelled_scheduler_jobs: cancelled,
        }
    }

    pub fn resume(&self) -> EmergencyPauseResponse {
        let mut pause = self
            .emergency_pause
            .lock()
            .expect("emergency pause lock poisoned");
        pause.paused = false;
        pause.reason = None;
        pause.resumed_at = Some(Utc::now());

        EmergencyPauseResponse {
            paused: pause.paused,
            reason: pause.reason.clone(),
            paused_at: pause.paused_at,
            resumed_at: pause.resumed_at,
            cancelled_scheduler_jobs: 0,
        }
    }

    pub fn pause_status(&self) -> EmergencyPauseResponse {
        let pause = self
            .emergency_pause
            .lock()
            .expect("emergency pause lock poisoned");
        EmergencyPauseResponse {
            paused: pause.paused,
            reason: pause.reason.clone(),
            paused_at: pause.paused_at,
            resumed_at: pause.resumed_at,
            cancelled_scheduler_jobs: 0,
        }
    }

    fn is_paused(&self) -> bool {
        self.emergency_pause
            .lock()
            .expect("emergency pause lock poisoned")
            .paused
    }
}

pub fn router(state: IpcState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/commands", post(command))
        .route(
            "/emergency-pause",
            get(pause_status).post(pause).delete(resume),
        )
        .route(
            "/scheduler/jobs",
            get(list_scheduler_jobs).post(create_scheduler_job),
        )
        .route("/scheduler/jobs/:id", delete(cancel_scheduler_job))
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

pub async fn serve(bind: SocketAddr, state: IpcState) -> anyhow::Result<()> {
    let listener = TcpListener::bind(bind).await?;
    axum::serve(listener, router(state)).await?;
    Ok(())
}

async fn health(State(state): State<IpcState>) -> Json<HealthResponse> {
    Json(state.health())
}

async fn command(
    State(state): State<IpcState>,
    Json(request): Json<CommandRequest>,
) -> Result<Json<CommandResponse>, (StatusCode, Json<ErrorResponse>)> {
    state
        .submit_command(request)
        .map(Json)
        .map_err(error_response)
}

async fn pause_status(State(state): State<IpcState>) -> Json<EmergencyPauseResponse> {
    Json(state.pause_status())
}

async fn pause(
    State(state): State<IpcState>,
    Json(request): Json<EmergencyPauseRequest>,
) -> Json<EmergencyPauseResponse> {
    Json(state.pause(request.reason))
}

async fn resume(State(state): State<IpcState>) -> Json<EmergencyPauseResponse> {
    Json(state.resume())
}

async fn list_scheduler_jobs(State(state): State<IpcState>) -> Json<Vec<SchedulerJob>> {
    Json(state.scheduler().list())
}

async fn create_scheduler_job(
    State(state): State<IpcState>,
    Json(request): Json<CreateSchedulerJobRequest>,
) -> Result<Json<SchedulerJob>, (StatusCode, Json<ErrorResponse>)> {
    state
        .scheduler()
        .schedule(SchedulerJobSpec {
            name: request.name,
            command: request.command,
            trigger: request.trigger,
        })
        .map(Json)
        .map_err(error_response)
}

async fn cancel_scheduler_job(
    State(state): State<IpcState>,
    Path(id): Path<Uuid>,
) -> Result<Json<SchedulerJob>, (StatusCode, Json<ErrorResponse>)> {
    state
        .scheduler()
        .cancel(id, "cancelled through IPC")
        .map(Json)
        .map_err(error_response)
}

fn error_response(error: JarvisError) -> (StatusCode, Json<ErrorResponse>) {
    let status = match error {
        JarvisError::Validation(_) => StatusCode::BAD_REQUEST,
        JarvisError::PolicyBlocked(_) => StatusCode::FORBIDDEN,
        JarvisError::ApprovalRequired(_) => StatusCode::ACCEPTED,
        JarvisError::Storage(_) | JarvisError::Plugin(_) | JarvisError::Other(_) => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    };

    (
        status,
        Json(ErrorResponse {
            error: error.to_string(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn health_reports_pause_and_scheduler_counts() {
        let state = IpcState::new();
        state
            .scheduler()
            .schedule(SchedulerJobSpec {
                name: "daily".to_string(),
                command: "review calendar".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("schedule");

        let health = state.health();
        assert_eq!(health.status, "ok");
        assert_eq!(health.scheduler_jobs, 1);
        assert!(!health.emergency_paused);
    }

    #[test]
    fn command_schema_accepts_stubbed_command() {
        let state = IpcState::new();
        let response = state
            .submit_command(CommandRequest {
                input: "what is next".to_string(),
                session_id: None,
                context: json!({"surface": "test"}),
                dry_run: true,
            })
            .expect("command");

        assert!(response.accepted);
        assert_eq!(response.task.status, TaskStatus::Completed);
        assert_eq!(response.audit_entry.event_type, "command.accepted");
    }

    #[test]
    fn emergency_pause_blocks_commands_and_cancels_scheduler_jobs() {
        let state = IpcState::new();
        state
            .scheduler()
            .schedule(SchedulerJobSpec {
                name: "routine".to_string(),
                command: "run routine".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("schedule");

        let pause = state.pause("testing");
        assert!(pause.paused);
        assert_eq!(pause.cancelled_scheduler_jobs, 1);

        let response = state
            .submit_command(CommandRequest {
                input: "continue".to_string(),
                session_id: None,
                context: serde_json::Value::Null,
                dry_run: false,
            })
            .expect("blocked command is still represented");
        assert!(!response.accepted);
        assert_eq!(response.task.status, TaskStatus::Blocked);

        let resume = state.resume();
        assert!(!resume.paused);
    }
}
