use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use crate::storage::{
    EmergencyPauseState as StoredEmergencyPauseState, NewMemoryItem, SqliteRepository,
};
use crate::{
    ApprovalDecision, ApprovalStatus, AuditEntry, CapabilityScope, ConversationRuntime,
    FakeLocalModel, JarvisError, JarvisResult, ModelRoute, ModelRouteRequest, ModelRouter,
    PermissionEngine, PluginCallRequest, PluginCallResult, PluginCallStatus, PluginHost,
    PluginManifest, PluginPermission, PolicyRequest, RuntimeCommandRequest, RuntimeCommandStore,
    RuntimeConfig, RuntimeControl, RuntimeStep, Scheduler, SchedulerJob, SchedulerJobSpec,
    Sensitivity, TaskRecord, TaskStatus, TriggerKind,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub started_at: DateTime<Utc>,
    pub emergency_paused: bool,
    pub emergency_pause_reason: Option<String>,
    pub emergency_pause_updated_at: Option<DateTime<Utc>>,
    pub scheduler_jobs: usize,
    pub command_runtime: String,
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
    #[serde(default)]
    pub sensitivity: Option<Sensitivity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResponse {
    pub accepted: bool,
    pub task: TaskRecord,
    pub audit_entry: AuditEntry,
    pub audit_entries: Vec<AuditEntry>,
    pub route: Option<ModelRoute>,
    pub steps: Vec<RuntimeStep>,
    pub plugin_results: Vec<PluginCallResult>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMemoryItemRequest {
    pub category: String,
    pub key: String,
    pub value: String,
    pub provenance: String,
    pub sensitivity: Sensitivity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMemoryItemRequest {
    pub value: String,
    pub provenance: String,
    pub sensitivity: Sensitivity,
}

#[derive(Debug, Clone, Default)]
struct EmergencyPauseState {
    paused: bool,
    reason: Option<String>,
    paused_at: Option<DateTime<Utc>>,
    resumed_at: Option<DateTime<Utc>>,
}

impl EmergencyPauseState {
    fn from_stored(stored: StoredEmergencyPauseState) -> Self {
        Self {
            paused: stored.paused,
            reason: stored.reason,
            paused_at: stored.paused.then_some(stored.updated_at),
            resumed_at: (!stored.paused).then_some(stored.updated_at),
        }
    }

    fn updated_at(&self) -> Option<DateTime<Utc>> {
        self.paused_at.or(self.resumed_at)
    }
}

#[derive(Clone)]
pub struct IpcState {
    version: String,
    started_at: DateTime<Utc>,
    scheduler: Scheduler,
    runtime_control: RuntimeControl,
    emergency_pause: Arc<Mutex<EmergencyPauseState>>,
    repository: Option<Arc<Mutex<SqliteRepository>>>,
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
            runtime_control: RuntimeControl::default(),
            emergency_pause: Arc::new(Mutex::new(EmergencyPauseState::default())),
            repository: None,
        }
    }

    pub fn with_repository(repository: SqliteRepository) -> JarvisResult<Self> {
        let stored_pause = repository.emergency_pause_state()?;
        if stored_pause.paused {
            let control = RuntimeControl::default();
            control.emergency_pause();
            Ok(Self {
                version: env!("CARGO_PKG_VERSION").to_string(),
                started_at: Utc::now(),
                scheduler: Scheduler::new(),
                runtime_control: control,
                emergency_pause: Arc::new(Mutex::new(EmergencyPauseState::from_stored(
                    stored_pause,
                ))),
                repository: Some(Arc::new(Mutex::new(repository))),
            })
        } else {
            Ok(Self {
                version: env!("CARGO_PKG_VERSION").to_string(),
                started_at: Utc::now(),
                scheduler: Scheduler::new(),
                runtime_control: RuntimeControl::default(),
                emergency_pause: Arc::new(Mutex::new(EmergencyPauseState::from_stored(
                    stored_pause,
                ))),
                repository: Some(Arc::new(Mutex::new(repository))),
            })
        }
    }

    pub fn scheduler(&self) -> Scheduler {
        self.scheduler.clone()
    }

    pub fn health(&self) -> HealthResponse {
        let pause = self.pause_snapshot();
        HealthResponse {
            status: "ok".to_string(),
            version: self.version.clone(),
            started_at: self.started_at,
            emergency_paused: pause.paused,
            emergency_pause_reason: pause.reason.clone(),
            emergency_pause_updated_at: pause.updated_at(),
            scheduler_jobs: self.scheduler.list().len(),
            command_runtime: "routed-fake-local-model+first-party-plugins".to_string(),
        }
    }

    pub async fn submit_command(&self, request: CommandRequest) -> JarvisResult<CommandResponse> {
        if request.input.trim().is_empty() {
            return Err(JarvisError::Validation(
                "command input cannot be empty".to_string(),
            ));
        }

        let command_store = self.command_store();
        let runtime = ConversationRuntime::with_storage_parts(
            RuntimeConfig::default(),
            self.runtime_control.clone(),
            FakeLocalModel::default(),
            crate::NoopRuntimeHooks,
            command_store.clone(),
        );
        let sensitivity = request
            .sensitivity
            .or_else(|| sensitivity_from_context(&request.context))
            .unwrap_or(Sensitivity::Personal);
        let runtime_response = runtime
            .execute_command(
                RuntimeCommandRequest::new(request.input.clone())
                    .with_session_id(request.session_id.unwrap_or_else(Uuid::new_v4))
                    .with_sensitivity(sensitivity),
            )
            .await?;

        let mut audit_entries = runtime_response.audit_entries;
        let plugin_results = if runtime_response.task.status == TaskStatus::Completed {
            let route_record = ModelRouter::route(&ModelRouteRequest {
                task_id: Some(runtime_response.task.id),
                user_intent: request.input.clone(),
                sensitivity,
                required_scopes: vec![CapabilityScope::Conversation, CapabilityScope::LocalModel],
                granted_scopes: vec![CapabilityScope::Conversation, CapabilityScope::LocalModel],
                local_available: true,
                local_sufficient: true,
                chatgpt_enabled: false,
                emergency_paused: self.runtime_control.is_emergency_paused(),
                approval: None,
                context_preview: context_preview(&request.context),
            });
            let route_audit = AuditEntry::new(
                Some(runtime_response.task.id),
                "model_route_selected",
                "model router selected the command route",
                json!({
                    "outcome": route_record.outcome,
                    "selected_provider": route_record.selected_provider,
                    "reason": route_record.reason,
                    "sensitivity": route_record.sensitivity,
                    "approval_status": route_record.approval_status,
                    "redaction_applied": route_record.redaction_applied,
                }),
            );
            command_store.append_audit_entry(&route_audit)?;
            audit_entries.push(route_audit);
            self.maybe_execute_first_party_plugin(
                runtime_response.task.id,
                &request.input,
                sensitivity,
                request.dry_run,
                &mut audit_entries,
                &command_store,
            )?
        } else {
            Vec::new()
        };

        let accepted = runtime_response.task.status == TaskStatus::Completed;
        let audit_entry = audit_entries.last().cloned().unwrap_or_else(|| {
            AuditEntry::new(
                Some(runtime_response.task.id),
                "command_runtime_empty",
                "command runtime returned no audit entries",
                json!({
                    "dry_run": request.dry_run,
                    "context": request.context,
                }),
            )
        });

        Ok(CommandResponse {
            accepted,
            task: runtime_response.task,
            audit_entry,
            audit_entries,
            route: runtime_response.route,
            steps: runtime_response.steps,
            plugin_results,
            message: runtime_response.message,
        })
    }

    fn command_store(&self) -> SharedCommandStore {
        SharedCommandStore {
            repository: self.repository.clone(),
        }
    }

    fn maybe_execute_first_party_plugin(
        &self,
        task_id: Uuid,
        input: &str,
        sensitivity: Sensitivity,
        dry_run: bool,
        audit_entries: &mut Vec<AuditEntry>,
        command_store: &SharedCommandStore,
    ) -> JarvisResult<Vec<PluginCallResult>> {
        let Some(mut plugin_request) = first_party_plugin_request(input) else {
            return Ok(Vec::new());
        };
        let host = PluginHost::with_first_party_plugins()?;
        let manifest = host.manifest(&plugin_request.plugin_id)?;
        let action = manifest.action(&plugin_request.action).ok_or_else(|| {
            JarvisError::Plugin(format!(
                "plugin {} does not declare action {}",
                plugin_request.plugin_id, plugin_request.action
            ))
        })?;
        let requested_scopes = plugin_scopes(&action.permissions);
        let mut granted_scopes = requested_scopes.clone();
        granted_scopes.push(CapabilityScope::Conversation);
        let policy_request = PolicyRequest {
            task_id: Some(task_id),
            action: format!("{}.{}", plugin_request.plugin_id, plugin_request.action),
            requested_scopes,
            granted_scopes,
            risk_tier: action.risk_tier,
            sensitivity,
            emergency_paused: self.runtime_control.is_emergency_paused(),
            approval: None,
        };
        let policy = PermissionEngine::evaluate(&policy_request);
        let policy_audit = AuditEntry::new(
            Some(task_id),
            "plugin_policy_evaluated",
            "policy evaluated first-party plugin action",
            json!({
                "plugin_id": plugin_request.plugin_id,
                "action": plugin_request.action,
                "decision": policy.decision,
                "reason": policy.reason,
                "risk_tier": policy.risk_tier,
                "approval_status": policy.approval_status,
                "missing_scopes": policy.missing_scopes,
                "dry_run": dry_run,
            }),
        );
        command_store.append_audit_entry(&policy_audit)?;
        audit_entries.push(policy_audit);

        if policy.decision == ApprovalDecision::Blocked {
            return Err(JarvisError::PolicyBlocked(policy.reason));
        }

        if policy.decision == ApprovalDecision::RequireConfirmation {
            plugin_request.approval_status = ApprovalStatus::Pending;
        }

        if dry_run {
            let dry_run_audit = AuditEntry::new(
                Some(task_id),
                "plugin_dry_run",
                "dry run skipped first-party plugin execution",
                json!({
                    "plugin_id": plugin_request.plugin_id,
                    "action": plugin_request.action,
                    "approval_status": plugin_request.approval_status,
                }),
            );
            command_store.append_audit_entry(&dry_run_audit)?;
            audit_entries.push(dry_run_audit);
            return Ok(Vec::new());
        }

        let result = host.execute(plugin_request)?;
        let event_type = match result.status {
            PluginCallStatus::Completed => "plugin_completed",
            PluginCallStatus::ApprovalRequired => "plugin_approval_required",
            PluginCallStatus::TimedOut => "plugin_timed_out",
            PluginCallStatus::Cancelled => "plugin_cancelled",
            PluginCallStatus::Failed => "plugin_failed",
        };
        let plugin_audit = AuditEntry::new(
            Some(task_id),
            event_type,
            "first-party plugin action finished",
            json!({
                "plugin_id": result.metadata.plugin_id,
                "action": result.metadata.action,
                "status": result.status,
                "risk_tier": result.metadata.risk_tier,
                "approval_status": result.metadata.approval_status,
                "proactive": result.metadata.proactive,
                "timeout_ms": result.metadata.timeout_ms,
            }),
        );
        command_store.append_audit_entry(&plugin_audit)?;
        audit_entries.push(plugin_audit);
        Ok(vec![result])
    }

    pub fn pause(&self, reason: impl Into<String>) -> JarvisResult<EmergencyPauseResponse> {
        let reason = reason.into();
        let stored_pause = self.persist_pause(true, Some(&reason))?;
        let cancelled = self
            .scheduler
            .cancel_active(format!("emergency pause: {reason}"));
        self.runtime_control.emergency_pause();
        let paused_at = stored_pause
            .as_ref()
            .map(|pause| pause.updated_at)
            .unwrap_or_else(Utc::now);
        let mut pause = self
            .emergency_pause
            .lock()
            .expect("emergency pause lock poisoned");

        pause.paused = true;
        pause.reason = Some(reason);
        pause.paused_at = Some(paused_at);
        pause.resumed_at = None;

        Ok(EmergencyPauseResponse {
            paused: pause.paused,
            reason: pause.reason.clone(),
            paused_at: pause.paused_at,
            resumed_at: pause.resumed_at,
            cancelled_scheduler_jobs: cancelled,
        })
    }

    pub fn resume(&self) -> JarvisResult<EmergencyPauseResponse> {
        let stored_pause = self.persist_pause(false, None)?;
        let resumed_at = stored_pause
            .as_ref()
            .map(|pause| pause.updated_at)
            .unwrap_or_else(Utc::now);
        let mut pause = self
            .emergency_pause
            .lock()
            .expect("emergency pause lock poisoned");
        self.runtime_control.resume();
        pause.paused = false;
        pause.reason = None;
        pause.resumed_at = Some(resumed_at);

        Ok(EmergencyPauseResponse {
            paused: pause.paused,
            reason: pause.reason.clone(),
            paused_at: pause.paused_at,
            resumed_at: pause.resumed_at,
            cancelled_scheduler_jobs: 0,
        })
    }

    pub fn pause_status(&self) -> EmergencyPauseResponse {
        let pause = self.pause_snapshot();
        EmergencyPauseResponse {
            paused: pause.paused,
            reason: pause.reason,
            paused_at: pause.paused_at,
            resumed_at: pause.resumed_at,
            cancelled_scheduler_jobs: 0,
        }
    }

    fn persist_pause(
        &self,
        paused: bool,
        reason: Option<&str>,
    ) -> JarvisResult<Option<StoredEmergencyPauseState>> {
        self.repository
            .as_ref()
            .map(|repository| {
                repository
                    .lock()
                    .expect("IPC repository lock poisoned")
                    .set_emergency_pause(paused, reason, Some("ipc"))
            })
            .transpose()
    }

    fn pause_snapshot(&self) -> EmergencyPauseState {
        self.emergency_pause
            .lock()
            .expect("emergency pause lock poisoned")
            .clone()
    }

    fn using_repository<T>(
        &self,
        operation: impl FnOnce(&SqliteRepository) -> JarvisResult<T>,
    ) -> JarvisResult<T> {
        let repository = self.repository.as_ref().ok_or_else(|| {
            JarvisError::Storage(
                "this endpoint requires IpcState with SqliteRepository backing".to_string(),
            )
        })?;
        let repository = repository.lock().expect("IPC repository lock poisoned");
        operation(&repository)
    }
}

#[derive(Clone)]
struct SharedCommandStore {
    repository: Option<Arc<Mutex<SqliteRepository>>>,
}

impl RuntimeCommandStore for SharedCommandStore {
    fn create_task(&self, session_id: Uuid, user_input: String) -> JarvisResult<TaskRecord> {
        match &self.repository {
            Some(repository) => repository
                .lock()
                .expect("IPC repository lock poisoned")
                .create_task(session_id, user_input),
            None => crate::NoopRuntimeCommandStore.create_task(session_id, user_input),
        }
    }

    fn update_task_status(&self, task: &mut TaskRecord, status: TaskStatus) -> JarvisResult<()> {
        match &self.repository {
            Some(repository) => {
                *task = repository
                    .lock()
                    .expect("IPC repository lock poisoned")
                    .update_task_status(task.id, status)?;
                Ok(())
            }
            None => crate::NoopRuntimeCommandStore.update_task_status(task, status),
        }
    }

    fn append_audit_entry(&self, entry: &AuditEntry) -> JarvisResult<()> {
        match &self.repository {
            Some(repository) => repository
                .lock()
                .expect("IPC repository lock poisoned")
                .append_audit_entry(entry),
            None => crate::NoopRuntimeCommandStore.append_audit_entry(entry),
        }
    }
}

fn sensitivity_from_context(context: &serde_json::Value) -> Option<Sensitivity> {
    let value = context.get("sensitivity")?.as_str()?;
    match value {
        "public" => Some(Sensitivity::Public),
        "workspace" => Some(Sensitivity::Workspace),
        "personal" => Some(Sensitivity::Personal),
        "private" => Some(Sensitivity::Private),
        "credential_adjacent" => Some(Sensitivity::CredentialAdjacent),
        "restricted" => Some(Sensitivity::Restricted),
        _ => None,
    }
}

fn context_preview(context: &serde_json::Value) -> String {
    match context {
        serde_json::Value::Null => String::new(),
        serde_json::Value::String(value) => value.chars().take(512).collect(),
        value => value.to_string().chars().take(512).collect(),
    }
}

fn first_party_plugin_request(input: &str) -> Option<PluginCallRequest> {
    let trimmed = input.trim();
    if let Some(message) = trimmed.strip_prefix("plugin echo ") {
        return Some(PluginCallRequest::reactive(
            "fake_echo",
            "echo",
            json!({ "message": message.trim() }),
        ));
    }

    if matches!(
        trimmed,
        "plugin status" | "core status" | "jarvis status" | "status"
    ) {
        return Some(PluginCallRequest::reactive(
            "fake_status",
            "status",
            json!({}),
        ));
    }

    None
}

fn plugin_scopes(permissions: &[PluginPermission]) -> Vec<CapabilityScope> {
    let mut scopes = vec![CapabilityScope::PluginRun];
    for permission in permissions {
        let scope = match permission {
            PluginPermission::ReadWorkspace => CapabilityScope::FileRead,
            PluginPermission::WriteWorkspace => CapabilityScope::FileWrite,
            PluginPermission::ReadMemory => CapabilityScope::MemoryRead,
            PluginPermission::WriteMemory => CapabilityScope::MemoryWrite,
            PluginPermission::CallModel => CapabilityScope::LocalModel,
            PluginPermission::ProactiveRun => CapabilityScope::SchedulerRun,
            PluginPermission::Network => CapabilityScope::NetworkAccess,
            PluginPermission::SystemStatus => CapabilityScope::Conversation,
        };
        if !scopes.contains(&scope) {
            scopes.push(scope);
        }
    }
    scopes
}

pub fn router(state: IpcState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/commands", post(command))
        .route("/tasks", get(list_tasks))
        .route("/tasks/:id", get(get_task))
        .route("/tasks/:id/audit", get(list_task_audit_entries))
        .route("/audit", get(list_audit_entries))
        .route("/memory", get(list_memory_items).post(create_memory_item))
        .route(
            "/memory/:id",
            get(get_memory_item)
                .patch(update_memory_item)
                .delete(delete_memory_item),
        )
        .route("/memory/:id/review", post(review_memory_item))
        .route("/plugins/manifests", get(list_plugin_manifests))
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
    serve_listener(listener, state).await
}

pub async fn serve_listener(listener: TcpListener, state: IpcState) -> anyhow::Result<()> {
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
        .await
        .map(Json)
        .map_err(error_response)
}

async fn list_tasks(
    State(state): State<IpcState>,
) -> Result<Json<Vec<TaskRecord>>, (StatusCode, Json<ErrorResponse>)> {
    state
        .using_repository(SqliteRepository::list_tasks)
        .map(Json)
        .map_err(error_response)
}

async fn get_task(
    State(state): State<IpcState>,
    Path(id): Path<Uuid>,
) -> Result<Json<TaskRecord>, (StatusCode, Json<ErrorResponse>)> {
    state
        .using_repository(|repository| {
            repository
                .get_task(id)?
                .ok_or_else(|| JarvisError::Storage(format!("task not found: {id}")))
        })
        .map(Json)
        .map_err(error_response)
}

async fn list_task_audit_entries(
    State(state): State<IpcState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<AuditEntry>>, (StatusCode, Json<ErrorResponse>)> {
    state
        .using_repository(|repository| {
            if repository.get_task(id)?.is_none() {
                return Err(JarvisError::Storage(format!("task not found: {id}")));
            }
            repository.list_audit_entries(Some(id))
        })
        .map(Json)
        .map_err(error_response)
}

async fn list_audit_entries(
    State(state): State<IpcState>,
) -> Result<Json<Vec<AuditEntry>>, (StatusCode, Json<ErrorResponse>)> {
    state
        .using_repository(|repository| repository.list_audit_entries(None))
        .map(Json)
        .map_err(error_response)
}

async fn list_memory_items(
    State(state): State<IpcState>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Vec<crate::MemoryItem>>, (StatusCode, Json<ErrorResponse>)> {
    let include_deleted = query
        .get("include_deleted")
        .is_some_and(|value| value == "true" || value == "1");
    state
        .using_repository(|repository| repository.list_memory_items(include_deleted))
        .map(Json)
        .map_err(error_response)
}

async fn create_memory_item(
    State(state): State<IpcState>,
    Json(request): Json<CreateMemoryItemRequest>,
) -> Result<Json<crate::MemoryItem>, (StatusCode, Json<ErrorResponse>)> {
    state
        .using_repository(|repository| {
            repository.create_memory_item(NewMemoryItem {
                category: request.category,
                key: request.key,
                value: request.value,
                provenance: request.provenance,
                sensitivity: request.sensitivity,
            })
        })
        .map(Json)
        .map_err(error_response)
}

async fn get_memory_item(
    State(state): State<IpcState>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::MemoryItem>, (StatusCode, Json<ErrorResponse>)> {
    state
        .using_repository(|repository| {
            repository
                .get_memory_item(id)?
                .ok_or_else(|| JarvisError::Storage(format!("memory item not found: {id}")))
        })
        .map(Json)
        .map_err(error_response)
}

async fn update_memory_item(
    State(state): State<IpcState>,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateMemoryItemRequest>,
) -> Result<Json<crate::MemoryItem>, (StatusCode, Json<ErrorResponse>)> {
    state
        .using_repository(|repository| {
            repository.update_memory_item(
                id,
                request.value,
                request.provenance,
                request.sensitivity,
            )
        })
        .map(Json)
        .map_err(error_response)
}

async fn review_memory_item(
    State(state): State<IpcState>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::MemoryItem>, (StatusCode, Json<ErrorResponse>)> {
    state
        .using_repository(|repository| repository.mark_memory_reviewed(id))
        .map(Json)
        .map_err(error_response)
}

async fn delete_memory_item(
    State(state): State<IpcState>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::MemoryItem>, (StatusCode, Json<ErrorResponse>)> {
    state
        .using_repository(|repository| repository.delete_memory_item(id))
        .map(Json)
        .map_err(error_response)
}

async fn list_plugin_manifests(
    State(_state): State<IpcState>,
) -> Result<Json<Vec<PluginManifest>>, (StatusCode, Json<ErrorResponse>)> {
    PluginHost::with_first_party_plugins()
        .and_then(|host| host.manifests())
        .map(Json)
        .map_err(error_response)
}

async fn pause_status(State(state): State<IpcState>) -> Json<EmergencyPauseResponse> {
    Json(state.pause_status())
}

async fn pause(
    State(state): State<IpcState>,
    Json(request): Json<EmergencyPauseRequest>,
) -> Result<Json<EmergencyPauseResponse>, (StatusCode, Json<ErrorResponse>)> {
    state
        .pause(request.reason)
        .map(Json)
        .map_err(error_response)
}

async fn resume(
    State(state): State<IpcState>,
) -> Result<Json<EmergencyPauseResponse>, (StatusCode, Json<ErrorResponse>)> {
    state.resume().map(Json).map_err(error_response)
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
        assert_eq!(health.emergency_pause_reason, None);
        assert_eq!(health.emergency_pause_updated_at, None);
        assert_eq!(
            health.command_runtime,
            "routed-fake-local-model+first-party-plugins"
        );
    }

    #[tokio::test]
    async fn command_schema_executes_fake_local_runtime() {
        let state = IpcState::new();
        let response = state
            .submit_command(CommandRequest {
                input: "what is next".to_string(),
                session_id: None,
                context: json!({"surface": "test"}),
                dry_run: true,
                sensitivity: None,
            })
            .await
            .expect("command");

        assert!(response.accepted);
        assert_eq!(response.task.status, TaskStatus::Completed);
        assert_eq!(response.audit_entry.event_type, "model_route_selected");
        assert_eq!(response.steps.len(), 1);
        assert!(response.message.contains("what is next"));
        assert_eq!(
            response.route.expect("fake local route").model,
            "fake-local-model"
        );
        assert!(response
            .audit_entries
            .iter()
            .any(|entry| entry.event_type == "model_step_completed"));
        assert!(response
            .audit_entries
            .iter()
            .any(|entry| entry.event_type == "model_route_selected"));
    }

    #[tokio::test]
    async fn command_schema_executes_first_party_plugin_with_policy_audit() {
        let state = IpcState::new();
        let response = state
            .submit_command(CommandRequest {
                input: "plugin echo hello from ipc".to_string(),
                session_id: None,
                context: json!({"surface": "test", "sensitivity": "workspace"}),
                dry_run: false,
                sensitivity: None,
            })
            .await
            .expect("plugin command");

        assert!(response.accepted);
        assert_eq!(response.plugin_results.len(), 1);
        assert_eq!(
            response.plugin_results[0].status,
            PluginCallStatus::Completed
        );
        assert_eq!(
            response.plugin_results[0].output,
            json!({ "message": "hello from ipc" })
        );
        assert!(response
            .audit_entries
            .iter()
            .any(|entry| entry.event_type == "plugin_policy_evaluated"));
        assert_eq!(response.audit_entry.event_type, "plugin_completed");
    }

    #[tokio::test]
    async fn command_dry_run_skips_first_party_plugin_execution() {
        let state = IpcState::new();
        let response = state
            .submit_command(CommandRequest {
                input: "plugin echo dry run".to_string(),
                session_id: None,
                context: json!({"surface": "test"}),
                dry_run: true,
                sensitivity: Some(Sensitivity::Workspace),
            })
            .await
            .expect("dry run command");

        assert!(response.accepted);
        assert!(response.plugin_results.is_empty());
        assert_eq!(response.audit_entry.event_type, "plugin_dry_run");
        assert!(response
            .audit_entries
            .iter()
            .any(|entry| entry.event_type == "plugin_policy_evaluated"));
    }

    #[tokio::test]
    async fn repository_backed_command_persists_ipc_task_and_audit_entries() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("jarvis.sqlite");
        let repository = SqliteRepository::open(&db_path).unwrap();
        let state = IpcState::with_repository(repository).expect("state");

        let response = state
            .submit_command(CommandRequest {
                input: "plugin echo persist me".to_string(),
                session_id: None,
                context: json!({"surface": "test"}),
                dry_run: false,
                sensitivity: Some(Sensitivity::Workspace),
            })
            .await
            .expect("persisted command");

        assert_eq!(response.task.status, TaskStatus::Completed);
        drop(state);

        let repository = SqliteRepository::open(db_path).unwrap();
        let task = repository
            .get_task(response.task.id)
            .expect("task query")
            .expect("persisted task");
        assert_eq!(task.status, TaskStatus::Completed);

        let audit_entries = repository
            .list_audit_entries(Some(response.task.id))
            .expect("audit query");
        let event_types = audit_entries
            .iter()
            .map(|entry| entry.event_type.as_str())
            .collect::<Vec<_>>();
        assert!(event_types.contains(&"task_created"));
        assert!(event_types.contains(&"model_route_selected"));
        assert!(event_types.contains(&"plugin_policy_evaluated"));
        assert!(event_types.contains(&"plugin_completed"));
    }

    #[tokio::test]
    async fn repository_backed_state_endpoints_expose_tasks_and_audit() {
        let repository = SqliteRepository::in_memory().unwrap();
        let state = IpcState::with_repository(repository).expect("state");
        let response = state
            .submit_command(CommandRequest {
                input: "plugin echo inspect state".to_string(),
                session_id: None,
                context: json!({"surface": "test"}),
                dry_run: false,
                sensitivity: Some(Sensitivity::Workspace),
            })
            .await
            .expect("command");

        let Json(tasks) = list_tasks(State(state.clone())).await.expect("tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, response.task.id);

        let Json(task) = get_task(State(state.clone()), Path(response.task.id))
            .await
            .expect("task");
        assert_eq!(task.status, TaskStatus::Completed);

        let Json(entries) = list_task_audit_entries(State(state.clone()), Path(response.task.id))
            .await
            .expect("task audit");
        assert!(entries
            .iter()
            .any(|entry| entry.event_type == "plugin_completed"));

        let Json(all_entries) = list_audit_entries(State(state)).await.expect("audit");
        assert_eq!(all_entries.len(), entries.len());
    }

    #[tokio::test]
    async fn repository_backed_memory_endpoints_cover_create_update_review_delete() {
        let repository = SqliteRepository::in_memory().unwrap();
        let state = IpcState::with_repository(repository).expect("state");

        let Json(created) = create_memory_item(
            State(state.clone()),
            Json(CreateMemoryItemRequest {
                category: "workflow".to_string(),
                key: "release-gate".to_string(),
                value: "run local gate before PR".to_string(),
                provenance: "test".to_string(),
                sensitivity: Sensitivity::Workspace,
            }),
        )
        .await
        .expect("create memory");
        assert_eq!(created.key, "release-gate");

        let Json(listed) = list_memory_items(State(state.clone()), Query(HashMap::new()))
            .await
            .expect("list memory");
        assert_eq!(listed.len(), 1);

        let Json(updated) = update_memory_item(
            State(state.clone()),
            Path(created.id),
            Json(UpdateMemoryItemRequest {
                value: "run full local gate before PR".to_string(),
                provenance: "test update".to_string(),
                sensitivity: Sensitivity::Workspace,
            }),
        )
        .await
        .expect("update memory");
        assert_eq!(updated.value, "run full local gate before PR");

        let Json(reviewed) = review_memory_item(State(state.clone()), Path(created.id))
            .await
            .expect("review memory");
        assert!(reviewed.reviewed_at.is_some());

        let Json(deleted) = delete_memory_item(State(state.clone()), Path(created.id))
            .await
            .expect("delete memory");
        assert!(deleted.deleted_at.is_some());

        let Json(active) = list_memory_items(State(state), Query(HashMap::new()))
            .await
            .expect("list active memory");
        assert!(active.is_empty());
    }

    #[tokio::test]
    async fn plugin_manifest_endpoint_lists_first_party_plugins() {
        let Json(manifests) = list_plugin_manifests(State(IpcState::new()))
            .await
            .expect("plugin manifests");

        assert!(manifests.iter().any(|manifest| manifest.id == "fake_echo"));
        assert!(manifests
            .iter()
            .any(|manifest| manifest.id == "fake_status"));
    }

    #[tokio::test]
    async fn emergency_pause_blocks_commands_and_cancels_scheduler_jobs() {
        let state = IpcState::new();
        state
            .scheduler()
            .schedule(SchedulerJobSpec {
                name: "routine".to_string(),
                command: "run routine".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("schedule");

        let pause = state.pause("testing").expect("pause");
        assert!(pause.paused);
        assert_eq!(pause.cancelled_scheduler_jobs, 1);
        assert!(state.health().emergency_paused);
        assert_eq!(
            state.health().emergency_pause_reason.as_deref(),
            Some("testing")
        );

        let response = state
            .submit_command(CommandRequest {
                input: "continue".to_string(),
                session_id: None,
                context: serde_json::Value::Null,
                dry_run: false,
                sensitivity: None,
            })
            .await
            .expect("blocked command is still represented");
        assert!(!response.accepted);
        assert_eq!(response.task.status, TaskStatus::Blocked);
        assert_eq!(response.audit_entry.event_type, "emergency_pause_blocked");

        let resume = state.resume().expect("resume");
        assert!(!resume.paused);
    }

    #[tokio::test]
    async fn emergency_pause_loads_and_updates_persistent_state() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("jarvis.sqlite");
        let repository = SqliteRepository::open(&db_path).unwrap();
        let state = IpcState::with_repository(repository).expect("state");
        state
            .scheduler()
            .schedule(SchedulerJobSpec {
                name: "persisted pause job".to_string(),
                command: "run persisted pause job".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("schedule");

        let pause = state.pause("maintenance window").expect("pause");
        assert!(pause.paused);
        assert_eq!(pause.reason.as_deref(), Some("maintenance window"));
        assert_eq!(pause.cancelled_scheduler_jobs, 1);
        assert!(pause.paused_at.is_some());
        assert!(state.health().emergency_paused);

        drop(state);

        let repository = SqliteRepository::open(&db_path).unwrap();
        let restarted = IpcState::with_repository(repository).expect("restarted state");
        let status = restarted.pause_status();
        assert!(status.paused);
        assert_eq!(status.reason.as_deref(), Some("maintenance window"));
        assert!(restarted.health().emergency_paused);
        assert_eq!(
            restarted.health().emergency_pause_reason.as_deref(),
            Some("maintenance window")
        );

        let response = restarted
            .submit_command(CommandRequest {
                input: "still blocked".to_string(),
                session_id: None,
                context: serde_json::Value::Null,
                dry_run: false,
                sensitivity: None,
            })
            .await
            .expect("blocked command");
        assert!(!response.accepted);
        assert_eq!(response.task.status, TaskStatus::Blocked);

        let resume = restarted.resume().expect("resume");
        assert!(!resume.paused);
        assert!(resume.resumed_at.is_some());

        drop(restarted);

        let repository = SqliteRepository::open(db_path).unwrap();
        let stored = repository.emergency_pause_state().unwrap();
        assert!(!stored.paused);
        assert_eq!(stored.reason, None);
        assert_eq!(stored.updated_by.as_deref(), Some("ipc"));
    }
}
