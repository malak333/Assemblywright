use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use crate::storage::{
    EmergencyPauseState as StoredEmergencyPauseState, NewMemoryItem, NewPendingApproval,
    PendingApproval, SqliteRepository,
};
use crate::{
    plugin_permission_scopes, ApprovalDecision, ApprovalStatus, AuditEntry, CapabilityScope,
    ConversationRuntime, FakeLocalModel, JarvisError, JarvisResult, LocalModelExecutor,
    LocalModelProviderKind, ModelRoute, ModelRouteRecord, PermissionEngine, PluginCallRequest,
    PluginCallResult, PluginCallStatus, PluginHost, PluginManifest, PolicyRequest, ProviderConfig,
    RuntimeCommandRequest, RuntimeCommandStore, RuntimeConfig, RuntimeControl, RuntimeStep,
    Scheduler, SchedulerJob, SchedulerJobSpec, SchedulerJobStatus, Sensitivity, TaskRecord,
    TaskStatus, TriggerKind,
};

pub const IPC_CONTRACT_VERSION: u16 = 1;
pub const IPC_CONTRACT_NAME: &str = "jarvis.local-ipc";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractMetadata {
    pub name: String,
    pub version: u16,
    pub core_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractEndpoint {
    pub method: String,
    pub path: String,
    pub repository_required: bool,
    pub redacted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractResponse {
    pub contract: ContractMetadata,
    pub endpoints: Vec<ContractEndpoint>,
    pub safe_inspection_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub contract: ContractMetadata,
    pub started_at: DateTime<Utc>,
    pub emergency_paused: bool,
    pub emergency_pause_reason: Option<String>,
    pub emergency_pause_updated_at: Option<DateTime<Utc>>,
    pub scheduler_jobs: usize,
    pub command_runtime: String,
    pub local_model_provider: LocalModelProviderKind,
    pub local_model: String,
    pub local_endpoint_configured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsExport {
    pub generated_at: DateTime<Utc>,
    pub redaction: String,
    pub health: HealthResponse,
    pub scheduler_jobs: Vec<DiagnosticSchedulerJob>,
    pub repository_backed: bool,
    pub schema_version: Option<i64>,
    pub task_count: Option<usize>,
    pub audit_entry_count: Option<usize>,
    pub active_memory_item_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticSchedulerJob {
    pub id: Uuid,
    pub name: String,
    pub trigger: TriggerKind,
    pub status: SchedulerJobStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub cancellation_reason_present: bool,
}

impl From<SchedulerJob> for DiagnosticSchedulerJob {
    fn from(job: SchedulerJob) -> Self {
        Self {
            id: job.id,
            name: job.name,
            trigger: job.trigger,
            status: job.status,
            created_at: job.created_at,
            updated_at: job.updated_at,
            cancelled_at: job.cancelled_at,
            cancellation_reason_present: job.cancellation_reason.is_some(),
        }
    }
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
    pub route_evidence: Option<ModelRouteRecord>,
    pub steps: Vec<RuntimeStep>,
    pub plugin_results: Vec<PluginCallResult>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalDecisionRequest {
    #[serde(default = "default_decided_by")]
    pub decided_by: String,
    #[serde(default)]
    pub reason: Option<String>,
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
    provider_config: ProviderConfig,
}

impl Default for IpcState {
    fn default() -> Self {
        Self::new()
    }
}

impl IpcState {
    pub fn new() -> Self {
        Self::with_provider_config(ProviderConfig::default())
    }

    pub fn with_provider_config(provider_config: ProviderConfig) -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            started_at: Utc::now(),
            scheduler: Scheduler::new(),
            runtime_control: RuntimeControl::default(),
            emergency_pause: Arc::new(Mutex::new(EmergencyPauseState::default())),
            repository: None,
            provider_config,
        }
    }

    pub fn with_repository(repository: SqliteRepository) -> JarvisResult<Self> {
        Self::with_repository_and_provider_config(repository, ProviderConfig::default())
    }

    pub fn with_repository_and_provider_config(
        repository: SqliteRepository,
        provider_config: ProviderConfig,
    ) -> JarvisResult<Self> {
        let stored_pause = repository.emergency_pause_state()?;
        let stored_scheduler_jobs = repository.list_scheduler_jobs()?;
        let scheduler = Scheduler::with_jobs(stored_scheduler_jobs);
        if stored_pause.paused {
            let control = RuntimeControl::default();
            control.emergency_pause();
            Ok(Self {
                version: env!("CARGO_PKG_VERSION").to_string(),
                started_at: Utc::now(),
                scheduler,
                runtime_control: control,
                emergency_pause: Arc::new(Mutex::new(EmergencyPauseState::from_stored(
                    stored_pause,
                ))),
                repository: Some(Arc::new(Mutex::new(repository))),
                provider_config,
            })
        } else {
            Ok(Self {
                version: env!("CARGO_PKG_VERSION").to_string(),
                started_at: Utc::now(),
                scheduler,
                runtime_control: RuntimeControl::default(),
                emergency_pause: Arc::new(Mutex::new(EmergencyPauseState::from_stored(
                    stored_pause,
                ))),
                repository: Some(Arc::new(Mutex::new(repository))),
                provider_config,
            })
        }
    }

    pub fn scheduler(&self) -> Scheduler {
        self.scheduler.clone()
    }

    pub fn contract(&self) -> ContractResponse {
        ContractResponse {
            contract: self.contract_metadata(),
            endpoints: contract_endpoints(),
            safe_inspection_paths: vec![
                "/health".to_string(),
                "/contract".to_string(),
                "/diagnostics/export".to_string(),
                "/plugins/manifests".to_string(),
                "/plugins/manifests/:id".to_string(),
                "/scheduler/jobs".to_string(),
                "/scheduler/jobs/:id".to_string(),
                "/memory".to_string(),
                "/memory/:id".to_string(),
                "/approvals".to_string(),
                "/approvals/:id".to_string(),
            ],
        }
    }

    pub fn health(&self) -> HealthResponse {
        let pause = self.pause_snapshot();
        HealthResponse {
            status: "ok".to_string(),
            version: self.version.clone(),
            contract: self.contract_metadata(),
            started_at: self.started_at,
            emergency_paused: pause.paused,
            emergency_pause_reason: pause.reason.clone(),
            emergency_pause_updated_at: pause.updated_at(),
            scheduler_jobs: self.scheduler.list().len(),
            command_runtime: self.command_runtime_label(),
            local_model_provider: self.provider_config.local.provider,
            local_model: self.provider_config.local.model.clone(),
            local_endpoint_configured: self.provider_config.local.base_url.is_some(),
        }
    }

    fn command_runtime_label(&self) -> String {
        match self.provider_config.local.provider {
            LocalModelProviderKind::Fake => "routed-fake-local-model+first-party-plugins",
            LocalModelProviderKind::Ollama => "routed-ollama-local-model+first-party-plugins",
        }
        .to_string()
    }

    fn contract_metadata(&self) -> ContractMetadata {
        ContractMetadata {
            name: IPC_CONTRACT_NAME.to_string(),
            version: IPC_CONTRACT_VERSION,
            core_version: self.version.clone(),
        }
    }

    pub fn diagnostics_export(&self) -> JarvisResult<DiagnosticsExport> {
        let (schema_version, task_count, audit_entry_count, active_memory_item_count) =
            match &self.repository {
                Some(_) => self.using_repository(|repository| {
                    Ok((
                        Some(repository.schema_version()?),
                        Some(repository.list_tasks()?.len()),
                        Some(repository.list_audit_entries(None)?.len()),
                        Some(repository.list_memory_items(false)?.len()),
                    ))
                })?,
                None => (None, None, None, None),
            };

        Ok(DiagnosticsExport {
            generated_at: Utc::now(),
            redaction:
                "diagnostics export omits command bodies, scheduler commands, audit payloads, memory values, and cancellation reason text"
                    .to_string(),
            health: self.health(),
            scheduler_jobs: self
                .scheduler
                .list()
                .into_iter()
                .map(DiagnosticSchedulerJob::from)
                .collect(),
            repository_backed: self.repository.is_some(),
            schema_version,
            task_count,
            audit_entry_count,
            active_memory_item_count,
        })
    }

    pub fn list_approvals(
        &self,
        status: Option<ApprovalStatus>,
    ) -> JarvisResult<Vec<PendingApproval>> {
        self.using_repository(|repository| repository.list_pending_approvals(status))
    }

    pub fn get_approval(&self, id: Uuid) -> JarvisResult<PendingApproval> {
        self.using_repository(|repository| {
            repository
                .get_pending_approval(id)?
                .ok_or_else(|| JarvisError::Storage(format!("pending approval not found: {id}")))
        })
    }

    pub fn approve_approval(
        &self,
        id: Uuid,
        decided_by: String,
        reason: Option<String>,
    ) -> JarvisResult<PendingApproval> {
        self.decide_approval(id, ApprovalStatus::Approved, decided_by, reason)
    }

    pub fn deny_approval(
        &self,
        id: Uuid,
        decided_by: String,
        reason: Option<String>,
    ) -> JarvisResult<PendingApproval> {
        self.decide_approval(id, ApprovalStatus::Denied, decided_by, reason)
    }

    fn decide_approval(
        &self,
        id: Uuid,
        status: ApprovalStatus,
        decided_by: String,
        reason: Option<String>,
    ) -> JarvisResult<PendingApproval> {
        self.using_repository(|repository| {
            let approval = repository.decide_pending_approval(id, status, decided_by, reason)?;
            let event_type = match status {
                ApprovalStatus::Approved => "approval_granted",
                ApprovalStatus::Denied => "approval_denied",
                _ => "approval_decision",
            };
            repository.append_audit_entry(&AuditEntry::new(
                Some(approval.task_id),
                event_type,
                "pending approval was decided; side effect remains unexecuted until retried with an approval grant",
                json!({
                    "approval_id": approval.id,
                    "action": approval.action,
                    "status": approval.status,
                    "risk_tier": approval.risk_tier,
                    "sensitivity": approval.sensitivity,
                    "requested_scopes": approval.requested_scopes,
                    "decided_by": approval.decided_by,
                    "decision_reason": approval.decision_reason,
                    "side_effect_executed": false,
                }),
            ))?;
            Ok(approval)
        })
    }

    pub async fn submit_command(&self, request: CommandRequest) -> JarvisResult<CommandResponse> {
        if request.input.trim().is_empty() {
            return Err(JarvisError::Validation(
                "command input cannot be empty".to_string(),
            ));
        }

        let command_store = self.command_store();
        let local_model = if self.provider_config.local.enabled {
            LocalModelExecutor::from_config(&self.provider_config.local)?
        } else {
            LocalModelExecutor::Fake(FakeLocalModel::default())
        };
        let runtime = ConversationRuntime::with_storage_parts(
            RuntimeConfig::default().with_provider_config(self.provider_config.clone()),
            self.runtime_control.clone(),
            local_model,
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
        let plugin_dispatch = if runtime_response.task.status == TaskStatus::Completed {
            self.maybe_execute_first_party_plugin(
                runtime_response.task.id,
                &request.input,
                sensitivity,
                request.dry_run,
                &mut audit_entries,
                &command_store,
            )?
        } else {
            PluginDispatch::default()
        };

        let mut task = runtime_response.task;
        if plugin_dispatch.waiting_for_approval {
            command_store.update_task_status(&mut task, TaskStatus::WaitingForApproval)?;
        }

        let accepted = task.status == TaskStatus::Completed;
        let audit_entry = audit_entries.last().cloned().unwrap_or_else(|| {
            AuditEntry::new(
                Some(task.id),
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
            task,
            audit_entry,
            audit_entries,
            route: runtime_response.route,
            route_evidence: runtime_response.route_evidence,
            steps: runtime_response.steps,
            plugin_results: plugin_dispatch.results,
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
    ) -> JarvisResult<PluginDispatch> {
        let Some(mut plugin_request) = first_party_plugin_request(input) else {
            return Ok(PluginDispatch::default());
        };
        let host = PluginHost::with_first_party_plugins()?;
        let manifest = host.manifest(&plugin_request.plugin_id)?;
        let action = manifest.action(&plugin_request.action).ok_or_else(|| {
            JarvisError::Plugin(format!(
                "plugin {} does not declare action {}",
                plugin_request.plugin_id, plugin_request.action
            ))
        })?;
        let requested_scopes = plugin_permission_scopes(&action.permissions);
        let mut granted_scopes = requested_scopes.clone();
        granted_scopes.push(CapabilityScope::Conversation);
        plugin_request.granted_scopes = granted_scopes.clone();
        plugin_request.sensitivity = sensitivity;
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
            let approval = self.persist_pending_approval(NewPendingApproval {
                task_id,
                action: format!("{}.{}", plugin_request.plugin_id, plugin_request.action),
                requested_scopes: policy_request.requested_scopes,
                risk_tier: action.risk_tier,
                sensitivity,
                reason: policy.reason.clone(),
            })?;
            let approval_audit = AuditEntry::new(
                Some(task_id),
                "approval_pending",
                "first-party plugin action is pending explicit approval and did not execute",
                json!({
                    "approval_id": approval.id,
                    "plugin_id": plugin_request.plugin_id,
                    "action": plugin_request.action,
                    "risk_tier": approval.risk_tier,
                    "sensitivity": approval.sensitivity,
                    "requested_scopes": approval.requested_scopes,
                    "approval_status": approval.status,
                    "side_effect_executed": false,
                }),
            );
            command_store.append_audit_entry(&approval_audit)?;
            audit_entries.push(approval_audit);
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
            return Ok(PluginDispatch::default());
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
        Ok(PluginDispatch {
            waiting_for_approval: result.status == PluginCallStatus::ApprovalRequired,
            results: vec![result],
        })
    }

    fn persist_pending_approval(
        &self,
        approval: NewPendingApproval,
    ) -> JarvisResult<PendingApproval> {
        self.using_repository(|repository| repository.create_pending_approval(approval))
    }

    pub fn pause(&self, reason: impl Into<String>) -> JarvisResult<EmergencyPauseResponse> {
        let reason = reason.into();
        let stored_pause = self.persist_pause(true, Some(&reason))?;
        let cancelled_jobs = self
            .scheduler
            .cancel_active_jobs(format!("emergency pause: {reason}"));
        for job in &cancelled_jobs {
            self.persist_scheduler_job(job)?;
        }
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
            cancelled_scheduler_jobs: cancelled_jobs.len(),
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

    pub fn schedule_scheduler_job(&self, spec: SchedulerJobSpec) -> JarvisResult<SchedulerJob> {
        let job = self.scheduler.schedule(spec)?;
        self.persist_scheduler_job(&job)?;
        Ok(job)
    }

    pub fn mark_scheduler_job_running(&self, id: Uuid) -> JarvisResult<SchedulerJob> {
        let job = self.scheduler.mark_running(id)?;
        self.persist_scheduler_transition(&job, |repository| {
            repository.mark_scheduler_job_running(id)
        })
    }

    pub fn complete_scheduler_job(&self, id: Uuid) -> JarvisResult<SchedulerJob> {
        let job = self.scheduler.complete(id)?;
        self.persist_scheduler_transition(&job, |repository| repository.complete_scheduler_job(id))
    }

    pub fn fail_scheduler_job(&self, id: Uuid) -> JarvisResult<SchedulerJob> {
        let job = self.scheduler.fail(id)?;
        self.persist_scheduler_transition(&job, |repository| repository.fail_scheduler_job(id))
    }

    pub fn cancel_scheduler_job(&self, id: Uuid, reason: &str) -> JarvisResult<SchedulerJob> {
        let job = self.scheduler.cancel(id, reason)?;
        let reason = reason.to_string();
        self.persist_scheduler_transition(&job, |repository| {
            repository.cancel_scheduler_job(id, reason)
        })
    }

    pub fn get_scheduler_job(&self, id: Uuid) -> JarvisResult<SchedulerJob> {
        self.scheduler
            .list()
            .into_iter()
            .find(|job| job.id == id)
            .ok_or_else(|| JarvisError::Storage(format!("scheduler job not found: {id}")))
    }

    fn persist_scheduler_job(&self, job: &SchedulerJob) -> JarvisResult<()> {
        self.repository
            .as_ref()
            .map(|repository| {
                repository
                    .lock()
                    .expect("IPC repository lock poisoned")
                    .upsert_scheduler_job(job)
            })
            .transpose()
            .map(|_| ())
    }

    fn persist_scheduler_transition(
        &self,
        job: &SchedulerJob,
        transition: impl FnOnce(&SqliteRepository) -> JarvisResult<SchedulerJob>,
    ) -> JarvisResult<SchedulerJob> {
        match &self.repository {
            Some(repository) => {
                let repository = repository.lock().expect("IPC repository lock poisoned");
                transition(&repository)
            }
            None => Ok(job.clone()),
        }
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

#[derive(Default)]
struct PluginDispatch {
    waiting_for_approval: bool,
    results: Vec<PluginCallResult>,
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

fn parse_approval_status(value: &str) -> JarvisResult<ApprovalStatus> {
    match value {
        "pending" => Ok(ApprovalStatus::Pending),
        "approved" => Ok(ApprovalStatus::Approved),
        "denied" => Ok(ApprovalStatus::Denied),
        _ => Err(JarvisError::Validation(
            "approval status must be pending, approved, or denied".to_string(),
        )),
    }
}

fn default_decided_by() -> String {
    "cli".to_string()
}

fn first_party_plugin_request(input: &str) -> Option<PluginCallRequest> {
    let trimmed = input.trim();
    if let Some(message) = trimmed.strip_prefix("plugin approval echo ") {
        return Some(PluginCallRequest::reactive(
            "fake_echo",
            "approval_echo",
            json!({ "message": message.trim() }),
        ));
    }

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

pub fn router(state: IpcState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/contract", get(contract))
        .route("/diagnostics/export", get(diagnostics_export))
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
        .route("/approvals", get(list_approvals))
        .route("/approvals/:id", get(get_approval))
        .route("/approvals/:id/approve", post(approve_approval))
        .route("/approvals/:id/deny", post(deny_approval))
        .route("/plugins/manifests", get(list_plugin_manifests))
        .route("/plugins/manifests/:id", get(get_plugin_manifest))
        .route(
            "/emergency-pause",
            get(pause_status).post(pause).delete(resume),
        )
        .route(
            "/scheduler/jobs",
            get(list_scheduler_jobs).post(create_scheduler_job),
        )
        .route(
            "/scheduler/jobs/:id",
            get(get_scheduler_job).delete(cancel_scheduler_job),
        )
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

async fn contract(State(state): State<IpcState>) -> Json<ContractResponse> {
    Json(state.contract())
}

async fn diagnostics_export(
    State(state): State<IpcState>,
) -> Result<Json<DiagnosticsExport>, (StatusCode, Json<ErrorResponse>)> {
    state.diagnostics_export().map(Json).map_err(error_response)
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

async fn list_approvals(
    State(state): State<IpcState>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Vec<PendingApproval>>, (StatusCode, Json<ErrorResponse>)> {
    let status = query
        .get("status")
        .map(|value| parse_approval_status(value))
        .transpose()
        .map_err(error_response)?;
    state
        .list_approvals(status)
        .map(Json)
        .map_err(error_response)
}

async fn get_approval(
    State(state): State<IpcState>,
    Path(id): Path<Uuid>,
) -> Result<Json<PendingApproval>, (StatusCode, Json<ErrorResponse>)> {
    state.get_approval(id).map(Json).map_err(error_response)
}

async fn approve_approval(
    State(state): State<IpcState>,
    Path(id): Path<Uuid>,
    Json(request): Json<ApprovalDecisionRequest>,
) -> Result<Json<PendingApproval>, (StatusCode, Json<ErrorResponse>)> {
    state
        .approve_approval(id, request.decided_by, request.reason)
        .map(Json)
        .map_err(error_response)
}

async fn deny_approval(
    State(state): State<IpcState>,
    Path(id): Path<Uuid>,
    Json(request): Json<ApprovalDecisionRequest>,
) -> Result<Json<PendingApproval>, (StatusCode, Json<ErrorResponse>)> {
    state
        .deny_approval(id, request.decided_by, request.reason)
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

async fn get_plugin_manifest(
    State(_state): State<IpcState>,
    Path(id): Path<String>,
) -> Result<Json<PluginManifest>, (StatusCode, Json<ErrorResponse>)> {
    PluginHost::with_first_party_plugins()
        .and_then(|host| host.manifest(&id))
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

async fn get_scheduler_job(
    State(state): State<IpcState>,
    Path(id): Path<Uuid>,
) -> Result<Json<SchedulerJob>, (StatusCode, Json<ErrorResponse>)> {
    state
        .get_scheduler_job(id)
        .map(Json)
        .map_err(error_response)
}

async fn create_scheduler_job(
    State(state): State<IpcState>,
    Json(request): Json<CreateSchedulerJobRequest>,
) -> Result<Json<SchedulerJob>, (StatusCode, Json<ErrorResponse>)> {
    state
        .schedule_scheduler_job(SchedulerJobSpec {
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
        .cancel_scheduler_job(id, "cancelled through IPC")
        .map(Json)
        .map_err(error_response)
}

fn contract_endpoints() -> Vec<ContractEndpoint> {
    vec![
        endpoint("GET", "/health", false, true),
        endpoint("GET", "/contract", false, true),
        endpoint("GET", "/diagnostics/export", false, true),
        endpoint("POST", "/commands", false, false),
        endpoint("GET", "/tasks", true, false),
        endpoint("GET", "/tasks/:id", true, false),
        endpoint("GET", "/tasks/:id/audit", true, false),
        endpoint("GET", "/audit", true, false),
        endpoint("GET", "/memory", true, false),
        endpoint("POST", "/memory", true, false),
        endpoint("GET", "/memory/:id", true, false),
        endpoint("PATCH", "/memory/:id", true, false),
        endpoint("DELETE", "/memory/:id", true, false),
        endpoint("POST", "/memory/:id/review", true, false),
        endpoint("GET", "/approvals", true, false),
        endpoint("GET", "/approvals/:id", true, false),
        endpoint("POST", "/approvals/:id/approve", true, false),
        endpoint("POST", "/approvals/:id/deny", true, false),
        endpoint("GET", "/plugins/manifests", false, true),
        endpoint("GET", "/plugins/manifests/:id", false, true),
        endpoint("GET", "/emergency-pause", false, true),
        endpoint("POST", "/emergency-pause", false, false),
        endpoint("DELETE", "/emergency-pause", false, true),
        endpoint("GET", "/scheduler/jobs", false, false),
        endpoint("POST", "/scheduler/jobs", false, false),
        endpoint("GET", "/scheduler/jobs/:id", false, false),
        endpoint("DELETE", "/scheduler/jobs/:id", false, false),
    ]
}

fn endpoint(
    method: impl Into<String>,
    path: impl Into<String>,
    repository_required: bool,
    redacted: bool,
) -> ContractEndpoint {
    ContractEndpoint {
        method: method.into(),
        path: path.into(),
        repository_required,
        redacted,
    }
}

fn error_response(error: JarvisError) -> (StatusCode, Json<ErrorResponse>) {
    let status = match error {
        JarvisError::Validation(_) => StatusCode::BAD_REQUEST,
        JarvisError::PolicyBlocked(_) => StatusCode::FORBIDDEN,
        JarvisError::ApprovalRequired(_) => StatusCode::ACCEPTED,
        JarvisError::Model(_) => StatusCode::BAD_GATEWAY,
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
        assert_eq!(health.contract.name, IPC_CONTRACT_NAME);
        assert_eq!(health.contract.version, IPC_CONTRACT_VERSION);
        assert_eq!(health.scheduler_jobs, 1);
        assert!(!health.emergency_paused);
        assert_eq!(health.emergency_pause_reason, None);
        assert_eq!(health.emergency_pause_updated_at, None);
        assert_eq!(
            health.command_runtime,
            "routed-fake-local-model+first-party-plugins"
        );
    }

    #[test]
    fn contract_endpoint_documents_safe_inspection_paths() {
        let state = IpcState::new();
        let contract = state.contract();

        assert_eq!(contract.contract.name, IPC_CONTRACT_NAME);
        assert_eq!(contract.contract.version, IPC_CONTRACT_VERSION);
        assert!(contract
            .safe_inspection_paths
            .contains(&"/diagnostics/export".to_string()));
        assert!(contract
            .safe_inspection_paths
            .contains(&"/plugins/manifests/:id".to_string()));
        assert!(contract
            .endpoints
            .iter()
            .any(|endpoint| endpoint.method == "GET"
                && endpoint.path == "/scheduler/jobs/:id"
                && !endpoint.repository_required));
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
        assert_eq!(response.audit_entry.event_type, "task_completed");
        assert_eq!(response.steps.len(), 1);
        assert!(response.message.contains("what is next"));
        assert_eq!(
            response.route.expect("fake local route").model,
            "fake-local-model"
        );
        let route_evidence = response.route_evidence.expect("route evidence");
        assert_eq!(route_evidence.outcome, crate::RouteOutcome::Selected);
        assert!(!route_evidence.evidence.chatgpt_enabled);
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
    async fn repository_backed_scheduler_jobs_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("jarvis.sqlite");
        let repository = SqliteRepository::open(&db_path).unwrap();
        let state = IpcState::with_repository(repository).expect("state");

        let Json(created) = create_scheduler_job(
            State(state.clone()),
            Json(CreateSchedulerJobRequest {
                name: "durable check".to_string(),
                command: "status check".to_string(),
                trigger: TriggerKind::Manual,
            }),
        )
        .await
        .expect("create scheduler job");
        assert_eq!(state.health().scheduler_jobs, 1);
        drop(state);

        let repository = SqliteRepository::open(&db_path).unwrap();
        let restarted = IpcState::with_repository(repository).expect("restarted state");
        let Json(jobs) = list_scheduler_jobs(State(restarted.clone())).await;
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, created.id);
        assert_eq!(jobs[0].status, crate::SchedulerJobStatus::Scheduled);

        let Json(fetched) = get_scheduler_job(State(restarted.clone()), Path(created.id))
            .await
            .expect("fetch scheduler job");
        assert_eq!(fetched.id, created.id);

        let Json(cancelled) = cancel_scheduler_job(State(restarted), Path(created.id))
            .await
            .expect("cancel scheduler job");
        assert_eq!(cancelled.status, crate::SchedulerJobStatus::Cancelled);

        let repository = SqliteRepository::open(db_path).unwrap();
        let stored = repository.list_scheduler_jobs().expect("stored jobs");
        assert_eq!(stored[0].status, crate::SchedulerJobStatus::Cancelled);
    }

    #[tokio::test]
    async fn repository_backed_scheduler_lifecycle_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("jarvis.sqlite");
        let repository = SqliteRepository::open(&db_path).unwrap();
        let state = IpcState::with_repository(repository).expect("state");

        let completed = state
            .schedule_scheduler_job(SchedulerJobSpec {
                name: "completed".to_string(),
                command: "record completed job".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("schedule completed");
        let failed = state
            .schedule_scheduler_job(SchedulerJobSpec {
                name: "failed".to_string(),
                command: "record failed job".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("schedule failed");

        assert_eq!(
            state
                .mark_scheduler_job_running(completed.id)
                .expect("mark running")
                .status,
            SchedulerJobStatus::Running
        );
        assert_eq!(
            state
                .complete_scheduler_job(completed.id)
                .expect("complete")
                .status,
            SchedulerJobStatus::Completed
        );
        assert_eq!(
            state.fail_scheduler_job(failed.id).expect("fail").status,
            SchedulerJobStatus::Failed
        );
        drop(state);

        let repository = SqliteRepository::open(&db_path).unwrap();
        let restarted = IpcState::with_repository(repository).expect("restarted state");
        let jobs = restarted.scheduler().list();
        assert_eq!(jobs.len(), 2);
        assert!(jobs
            .iter()
            .any(|job| job.id == completed.id && job.status == SchedulerJobStatus::Completed));
        assert!(jobs
            .iter()
            .any(|job| job.id == failed.id && job.status == SchedulerJobStatus::Failed));
    }

    #[tokio::test]
    async fn diagnostics_export_is_redacted_and_counts_repository_state() {
        let repository = SqliteRepository::in_memory().unwrap();
        let state = IpcState::with_repository(repository).expect("state");
        state
            .schedule_scheduler_job(SchedulerJobSpec {
                name: "diagnostic schedule".to_string(),
                command: "do not redact scheduler command".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("schedule");
        state
            .submit_command(CommandRequest {
                input: "private command body should stay out of diagnostics".to_string(),
                session_id: None,
                context: json!({"surface": "test"}),
                dry_run: true,
                sensitivity: Some(Sensitivity::Private),
            })
            .await
            .expect("command");

        let Json(export) = diagnostics_export(State(state))
            .await
            .expect("diagnostics export");
        assert_eq!(export.health.status, "ok");
        assert!(export.repository_backed);
        assert_eq!(export.schema_version, Some(3));
        assert_eq!(export.task_count, Some(1));
        assert!(export.audit_entry_count.unwrap_or_default() >= 2);
        assert_eq!(export.scheduler_jobs.len(), 1);
        assert!(export.redaction.contains("omits command bodies"));

        let encoded = serde_json::to_string(&export).unwrap();
        assert!(!encoded.contains("private command body"));
        assert!(!encoded.contains("do not redact scheduler command"));
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

        let Json(manifest) =
            get_plugin_manifest(State(IpcState::new()), Path("fake_echo".to_string()))
                .await
                .expect("fake_echo manifest");
        assert_eq!(manifest.id, "fake_echo");
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
        let scheduled = state
            .schedule_scheduler_job(SchedulerJobSpec {
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
        let jobs = repository.list_scheduler_jobs().unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, scheduled.id);
        assert_eq!(jobs[0].status, SchedulerJobStatus::Cancelled);
        assert!(jobs[0].cancelled_at.is_some());
        assert_eq!(
            jobs[0].cancellation_reason.as_deref(),
            Some("emergency pause: maintenance window")
        );
    }
}
