use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::model::{
    ModelExecutor, ModelRequest, ModelResponse, ModelRoute, ModelToolRequest, ModelToolResult,
    ProviderConfig,
};
use crate::plugin::{
    plugin_permission_scopes, PluginCallRequest, PluginCallResult, PluginCallStatus, PluginHost,
    PluginSource,
};
use crate::router::{ModelRouteRecord, ModelRouteRequest, ModelRouter, RouteOutcome};
use crate::storage::SqliteRepository;
use crate::types::{AuditEntry, JarvisResult, Sensitivity, TaskRecord, TaskStatus};
use crate::CapabilityScope;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub max_steps: u32,
    #[serde(default = "default_max_tool_calls_per_step")]
    pub max_tool_calls_per_step: u32,
    pub provider_config: ProviderConfig,
}

impl RuntimeConfig {
    pub fn new(max_steps: u32) -> Self {
        Self {
            max_steps,
            max_tool_calls_per_step: 4,
            provider_config: ProviderConfig::local_only(),
        }
    }

    pub fn with_max_tool_calls_per_step(mut self, max_tool_calls_per_step: u32) -> Self {
        self.max_tool_calls_per_step = max_tool_calls_per_step;
        self
    }

    pub fn with_provider_config(mut self, provider_config: ProviderConfig) -> Self {
        self.provider_config = provider_config;
        self
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self::new(8)
    }
}

fn default_max_tool_calls_per_step() -> u32 {
    4
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandRequest {
    pub session_id: Uuid,
    pub input: String,
    pub sensitivity: Sensitivity,
}

impl CommandRequest {
    pub fn new(input: impl Into<String>) -> Self {
        Self {
            session_id: Uuid::new_v4(),
            input: input.into(),
            sensitivity: Sensitivity::Personal,
        }
    }

    pub fn with_session_id(mut self, session_id: Uuid) -> Self {
        self.session_id = session_id;
        self
    }

    pub fn with_sensitivity(mut self, sensitivity: Sensitivity) -> Self {
        self.sensitivity = sensitivity;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeStep {
    pub index: u32,
    pub message: String,
    pub complete: bool,
    #[serde(default)]
    pub tool_results: Vec<ModelToolResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResponse {
    pub task: TaskRecord,
    pub message: String,
    pub route: Option<ModelRoute>,
    pub route_evidence: Option<ModelRouteRecord>,
    pub steps: Vec<RuntimeStep>,
    #[serde(default)]
    pub tool_results: Vec<ModelToolResult>,
    pub audit_entries: Vec<AuditEntry>,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeControl {
    inner: Arc<Mutex<RuntimeControlState>>,
}

#[derive(Debug, Default)]
struct RuntimeControlState {
    emergency_paused: bool,
    cancelled_tasks: HashSet<Uuid>,
}

impl RuntimeControl {
    pub fn emergency_pause(&self) {
        let mut state = self.inner.lock().expect("runtime control lock poisoned");
        state.emergency_paused = true;
    }

    pub fn resume(&self) {
        let mut state = self.inner.lock().expect("runtime control lock poisoned");
        state.emergency_paused = false;
    }

    pub fn is_emergency_paused(&self) -> bool {
        self.inner
            .lock()
            .expect("runtime control lock poisoned")
            .emergency_paused
    }

    pub fn cancel_task(&self, task_id: Uuid) {
        let mut state = self.inner.lock().expect("runtime control lock poisoned");
        state.cancelled_tasks.insert(task_id);
    }

    pub fn is_task_cancelled(&self, task_id: Uuid) -> bool {
        self.inner
            .lock()
            .expect("runtime control lock poisoned")
            .cancelled_tasks
            .contains(&task_id)
    }

    fn clear_task_cancellation(&self, task_id: Uuid) {
        let mut state = self.inner.lock().expect("runtime control lock poisoned");
        state.cancelled_tasks.remove(&task_id);
    }
}

pub trait RuntimeHooks: Send + Sync {
    fn task_created(&self, _task: &TaskRecord) {}
    fn before_model_step(&self, _task: &TaskRecord, _step_index: u32) {}
    fn model_step_completed(&self, _task: &TaskRecord, _step: &RuntimeStep) {}
    fn task_finished(&self, _task: &TaskRecord, _response: &CommandResponse) {}
}

#[derive(Debug, Default)]
pub struct NoopRuntimeHooks;

impl RuntimeHooks for NoopRuntimeHooks {}

pub trait RuntimeCommandStore {
    fn create_task(&self, session_id: Uuid, user_input: String) -> JarvisResult<TaskRecord>;
    fn update_task_status(&self, task: &mut TaskRecord, status: TaskStatus) -> JarvisResult<()>;
    fn append_audit_entry(&self, entry: &AuditEntry) -> JarvisResult<()>;
    fn append_model_route_record(&self, record: &ModelRouteRecord) -> JarvisResult<()>;
}

#[derive(Debug, Default)]
pub struct NoopRuntimeCommandStore;

impl RuntimeCommandStore for NoopRuntimeCommandStore {
    fn create_task(&self, session_id: Uuid, user_input: String) -> JarvisResult<TaskRecord> {
        let now = Utc::now();
        Ok(TaskRecord {
            id: Uuid::new_v4(),
            session_id,
            user_input,
            status: TaskStatus::Created,
            created_at: now,
            updated_at: now,
        })
    }

    fn update_task_status(&self, task: &mut TaskRecord, status: TaskStatus) -> JarvisResult<()> {
        task.status = status;
        touch(task);
        Ok(())
    }

    fn append_audit_entry(&self, _entry: &AuditEntry) -> JarvisResult<()> {
        Ok(())
    }

    fn append_model_route_record(&self, _record: &ModelRouteRecord) -> JarvisResult<()> {
        Ok(())
    }
}

impl<T> RuntimeCommandStore for &T
where
    T: RuntimeCommandStore + ?Sized,
{
    fn create_task(&self, session_id: Uuid, user_input: String) -> JarvisResult<TaskRecord> {
        (*self).create_task(session_id, user_input)
    }

    fn update_task_status(&self, task: &mut TaskRecord, status: TaskStatus) -> JarvisResult<()> {
        (*self).update_task_status(task, status)
    }

    fn append_audit_entry(&self, entry: &AuditEntry) -> JarvisResult<()> {
        (*self).append_audit_entry(entry)
    }

    fn append_model_route_record(&self, record: &ModelRouteRecord) -> JarvisResult<()> {
        (*self).append_model_route_record(record)
    }
}

impl RuntimeCommandStore for SqliteRepository {
    fn create_task(&self, session_id: Uuid, user_input: String) -> JarvisResult<TaskRecord> {
        SqliteRepository::create_task(self, session_id, user_input)
    }

    fn update_task_status(&self, task: &mut TaskRecord, status: TaskStatus) -> JarvisResult<()> {
        *task = SqliteRepository::update_task_status(self, task.id, status)?;
        Ok(())
    }

    fn append_audit_entry(&self, entry: &AuditEntry) -> JarvisResult<()> {
        SqliteRepository::append_audit_entry(self, entry)
    }

    fn append_model_route_record(&self, record: &ModelRouteRecord) -> JarvisResult<()> {
        SqliteRepository::append_model_route_record(self, record)
    }
}

pub struct ConversationRuntime<M, H = NoopRuntimeHooks, S = NoopRuntimeCommandStore> {
    config: RuntimeConfig,
    control: RuntimeControl,
    model: M,
    hooks: H,
    command_store: S,
    plugin_host: PluginHost,
}

impl<M> ConversationRuntime<M, NoopRuntimeHooks> {
    pub fn new(model: M) -> Self {
        Self::with_parts(
            RuntimeConfig::default(),
            RuntimeControl::default(),
            model,
            NoopRuntimeHooks,
        )
    }
}

impl<M, H> ConversationRuntime<M, H, NoopRuntimeCommandStore> {
    pub fn with_parts(config: RuntimeConfig, control: RuntimeControl, model: M, hooks: H) -> Self {
        Self::with_storage_parts(config, control, model, hooks, NoopRuntimeCommandStore)
    }
}

impl<M, H, S> ConversationRuntime<M, H, S> {
    pub fn with_storage_parts(
        config: RuntimeConfig,
        control: RuntimeControl,
        model: M,
        hooks: H,
        command_store: S,
    ) -> Self {
        Self {
            config,
            control,
            model,
            hooks,
            command_store,
            plugin_host: PluginHost::with_first_party_plugins()
                .expect("first-party plugin manifests must validate"),
        }
    }

    pub fn control(&self) -> RuntimeControl {
        self.control.clone()
    }

    pub fn with_plugin_host(mut self, plugin_host: PluginHost) -> Self {
        self.plugin_host = plugin_host;
        self
    }
}

impl<M, H, S> ConversationRuntime<M, H, S>
where
    M: ModelExecutor,
    H: RuntimeHooks,
    S: RuntimeCommandStore,
{
    pub async fn execute_command(&self, request: CommandRequest) -> JarvisResult<CommandResponse> {
        let mut task = self
            .command_store
            .create_task(request.session_id, request.input)?;
        let mut audit_entries = Vec::new();
        self.record_audit(
            &mut audit_entries,
            AuditEntry::new(
                Some(task.id),
                "task_created",
                "created command task",
                json!({
                    "session_id": task.session_id,
                    "sensitivity": request.sensitivity,
                }),
            ),
        )?;
        self.hooks.task_created(&task);

        if task.user_input.trim().is_empty() {
            self.update_task_status(&mut task, TaskStatus::Failed)?;
            self.record_audit(
                &mut audit_entries,
                AuditEntry::new(
                    Some(task.id),
                    "validation_failed",
                    "command input is empty",
                    json!({ "field": "input" }),
                ),
            )?;
            return Ok(self.finish(
                task,
                "Command input is required.",
                None,
                None,
                vec![],
                audit_entries,
            ));
        }

        if self.control.is_emergency_paused() {
            self.update_task_status(&mut task, TaskStatus::Blocked)?;
            self.record_audit(
                &mut audit_entries,
                AuditEntry::new(
                    Some(task.id),
                    "emergency_pause_blocked",
                    "emergency pause blocked command execution",
                    json!({ "emergency_paused": true }),
                ),
            )?;
            return Ok(self.finish(
                task,
                "Emergency pause is active; command execution is blocked.",
                None,
                None,
                vec![],
                audit_entries,
            ));
        }

        let mut granted_scopes = vec![CapabilityScope::Conversation, CapabilityScope::LocalModel];
        if self.config.provider_config.chatgpt.enabled {
            granted_scopes.push(CapabilityScope::CloudModel);
        }
        let route_record = ModelRouter::route(&ModelRouteRequest {
            task_id: Some(task.id),
            user_intent: task.user_input.clone(),
            sensitivity: request.sensitivity,
            required_scopes: vec![CapabilityScope::Conversation, CapabilityScope::LocalModel],
            granted_scopes,
            local_available: self.config.provider_config.local.enabled,
            local_sufficient: self.config.provider_config.local.enabled,
            provider_status: crate::ProviderStatus::from_config(&self.config.provider_config),
            emergency_paused: self.control.is_emergency_paused(),
            approval: None,
            context_preview: task.user_input.clone(),
        });
        self.command_store
            .append_model_route_record(&route_record)?;
        self.record_audit(
            &mut audit_entries,
            route_audit_entry(task.id, &route_record),
        )?;

        if route_record.outcome == RouteOutcome::NeedsApproval {
            self.update_task_status(&mut task, TaskStatus::WaitingForApproval)?;
            return Ok(self.finish(
                task,
                "Model route requires approval before execution.",
                None,
                Some(route_record),
                vec![],
                audit_entries,
            ));
        }

        if route_record.outcome == RouteOutcome::Blocked {
            self.update_task_status(&mut task, TaskStatus::Blocked)?;
            return Ok(self.finish(
                task,
                format!("Model route blocked: {}", route_record.reason),
                None,
                Some(route_record),
                vec![],
                audit_entries,
            ));
        }

        self.update_task_status(&mut task, TaskStatus::Running)?;
        self.record_audit(
            &mut audit_entries,
            AuditEntry::new(
                Some(task.id),
                "task_running",
                "command entered model execution",
                json!({
                    "max_steps": self.config.max_steps,
                    "provider": route_record.selected_provider,
                }),
            ),
        )?;

        let mut route = None;
        let route_evidence = Some(route_record);
        let mut steps = Vec::new();
        let mut tool_results = Vec::new();

        for step_index in 0..self.config.max_steps {
            if self.control.is_emergency_paused() {
                self.update_task_status(&mut task, TaskStatus::Cancelled)?;
                self.record_audit(
                    &mut audit_entries,
                    AuditEntry::new(
                        Some(task.id),
                        "emergency_pause_cancelled",
                        "emergency pause cancelled active command",
                        json!({ "step_index": step_index }),
                    ),
                )?;
                return Ok(self.finish(
                    task,
                    "Command cancelled because emergency pause was activated.",
                    route,
                    route_evidence,
                    steps,
                    audit_entries,
                ));
            }

            if self.control.is_task_cancelled(task.id) {
                self.update_task_status(&mut task, TaskStatus::Cancelled)?;
                self.control.clear_task_cancellation(task.id);
                self.record_audit(
                    &mut audit_entries,
                    AuditEntry::new(
                        Some(task.id),
                        "task_cancelled",
                        "command cancelled before model step",
                        json!({ "step_index": step_index }),
                    ),
                )?;
                return Ok(self.finish(
                    task,
                    "Command cancelled.",
                    route,
                    route_evidence,
                    steps,
                    audit_entries,
                ));
            }

            self.hooks.before_model_step(&task, step_index);
            if self.control.is_emergency_paused() {
                self.update_task_status(&mut task, TaskStatus::Cancelled)?;
                self.record_audit(
                    &mut audit_entries,
                    AuditEntry::new(
                        Some(task.id),
                        "emergency_pause_cancelled",
                        "emergency pause cancelled active command from runtime hook",
                        json!({ "step_index": step_index }),
                    ),
                )?;
                return Ok(self.finish(
                    task,
                    "Command cancelled because emergency pause was activated.",
                    route,
                    route_evidence,
                    steps,
                    audit_entries,
                ));
            }

            if self.control.is_task_cancelled(task.id) {
                self.update_task_status(&mut task, TaskStatus::Cancelled)?;
                self.control.clear_task_cancellation(task.id);
                self.record_audit(
                    &mut audit_entries,
                    AuditEntry::new(
                        Some(task.id),
                        "task_cancelled",
                        "command cancelled by runtime hook",
                        json!({ "step_index": step_index }),
                    ),
                )?;
                return Ok(self.finish(
                    task,
                    "Command cancelled.",
                    route,
                    route_evidence,
                    steps,
                    audit_entries,
                ));
            }

            let model_response = match self
                .model
                .execute_route(
                    ModelRequest {
                        task_id: task.id,
                        session_id: task.session_id,
                        user_input: task.user_input.clone(),
                        step_index,
                        tool_results: tool_results.clone(),
                    },
                    route_evidence
                        .as_ref()
                        .expect("route evidence is set before execution"),
                )
                .await
            {
                Ok(response) => response,
                Err(error) => {
                    self.update_task_status(&mut task, TaskStatus::Failed)?;
                    self.record_audit(
                        &mut audit_entries,
                        AuditEntry::new(
                            Some(task.id),
                            "model_step_failed",
                            "model step failed",
                            json!({
                                "step_index": step_index,
                                "error": redacted_model_error(&error),
                                "error_kind": model_error_kind(&error),
                                "selected_provider": route_evidence
                                    .as_ref()
                                    .and_then(|record| record.selected_provider),
                                "route_id": route_evidence.as_ref().map(|record| record.id),
                            }),
                        ),
                    )?;
                    let failed_route =
                        route.or_else(|| route_evidence.as_ref().and_then(route_from_evidence));
                    return Ok(self.finish(
                        task,
                        model_failure_message(step_index),
                        failed_route,
                        route_evidence,
                        steps,
                        audit_entries,
                    ));
                }
            };
            route = Some(model_response.route.clone());
            self.record_audit(
                &mut audit_entries,
                model_audit_entry(task.id, step_index, &model_response),
            )?;

            if !model_response.tool_requests.is_empty() {
                self.record_audit(
                    &mut audit_entries,
                    tool_plan_audit_entry(task.id, step_index, &model_response.tool_requests),
                )?;
            }

            let step_tool_results = match self.execute_tool_plan(
                &mut task,
                request.sensitivity,
                step_index,
                &model_response.tool_requests,
                &mut audit_entries,
            )? {
                ToolPlanOutcome::Completed(results) => results,
                ToolPlanOutcome::WaitingForApproval(results, message) => {
                    tool_results.extend(results.clone());
                    let step = RuntimeStep {
                        index: step_index,
                        message: model_response.message,
                        complete: false,
                        tool_results: results,
                    };
                    self.hooks.model_step_completed(&task, &step);
                    steps.push(step);
                    return Ok(self.finish(
                        task,
                        message,
                        route,
                        route_evidence,
                        steps,
                        audit_entries,
                    ));
                }
                ToolPlanOutcome::Blocked(results, message) => {
                    tool_results.extend(results.clone());
                    let step = RuntimeStep {
                        index: step_index,
                        message: model_response.message,
                        complete: false,
                        tool_results: results,
                    };
                    self.hooks.model_step_completed(&task, &step);
                    steps.push(step);
                    return Ok(self.finish(
                        task,
                        message,
                        route,
                        route_evidence,
                        steps,
                        audit_entries,
                    ));
                }
            };
            tool_results.extend(step_tool_results.clone());
            let step_complete = model_response.complete && model_response.tool_requests.is_empty();

            let step = RuntimeStep {
                index: step_index,
                message: model_response.message.clone(),
                complete: step_complete,
                tool_results: step_tool_results,
            };
            self.hooks.model_step_completed(&task, &step);
            steps.push(step);

            if step_complete {
                self.update_task_status(&mut task, TaskStatus::Completed)?;
                self.record_audit(
                    &mut audit_entries,
                    AuditEntry::new(
                        Some(task.id),
                        "task_completed",
                        "command completed",
                        json!({ "steps": steps.len() }),
                    ),
                )?;
                return Ok(self.finish(
                    task,
                    model_response.message,
                    route,
                    route_evidence,
                    steps,
                    audit_entries,
                ));
            }
        }

        self.update_task_status(&mut task, TaskStatus::Failed)?;
        self.record_audit(
            &mut audit_entries,
            AuditEntry::new(
                Some(task.id),
                "step_limit_exceeded",
                "command exceeded configured step limit",
                json!({ "max_steps": self.config.max_steps }),
            ),
        )?;
        Ok(self.finish(
            task,
            "Command failed because the runtime step limit was reached.",
            route,
            route_evidence,
            steps,
            audit_entries,
        ))
    }

    fn update_task_status(&self, task: &mut TaskRecord, status: TaskStatus) -> JarvisResult<()> {
        self.command_store.update_task_status(task, status)
    }

    fn record_audit(
        &self,
        audit_entries: &mut Vec<AuditEntry>,
        entry: AuditEntry,
    ) -> JarvisResult<()> {
        self.command_store.append_audit_entry(&entry)?;
        audit_entries.push(entry);
        Ok(())
    }

    fn execute_tool_plan(
        &self,
        task: &mut TaskRecord,
        sensitivity: Sensitivity,
        step_index: u32,
        tool_requests: &[ModelToolRequest],
        audit_entries: &mut Vec<AuditEntry>,
    ) -> JarvisResult<ToolPlanOutcome> {
        if tool_requests.is_empty() {
            return Ok(ToolPlanOutcome::Completed(Vec::new()));
        }

        if tool_requests.len() as u32 > self.config.max_tool_calls_per_step {
            self.update_task_status(task, TaskStatus::Failed)?;
            self.record_audit(
                audit_entries,
                AuditEntry::new(
                    Some(task.id),
                    "tool_plan_rejected",
                    "model planned more tool calls than the runtime allows",
                    json!({
                        "step_index": step_index,
                        "planned_tool_calls": tool_requests.len(),
                        "max_tool_calls_per_step": self.config.max_tool_calls_per_step,
                    }),
                ),
            )?;
            return Ok(ToolPlanOutcome::Blocked(
                Vec::new(),
                "Command failed because the model planned too many tool calls.".to_string(),
            ));
        }

        let mut results = Vec::new();
        for (tool_index, tool_request) in tool_requests.iter().enumerate() {
            let manifest = match self.validate_tool_request(task.id, step_index, tool_request) {
                Ok(manifest) => manifest,
                Err(error) => {
                    let registered_tools = self.registered_first_party_tool_names();
                    let guidance = tool_rejection_message(&error, &registered_tools);
                    self.update_task_status(task, TaskStatus::Failed)?;
                    self.record_audit(
                        audit_entries,
                        AuditEntry::new(
                            Some(task.id),
                            "tool_request_rejected",
                            "model-planned tool request failed validation",
                            json!({
                                "step_index": step_index,
                                "tool_index": tool_index,
                                "plugin_id": tool_request.plugin_id,
                                "action": tool_request.action,
                                "error": error.to_string(),
                                "registered_tools": registered_tools,
                            }),
                        ),
                    )?;
                    return Ok(ToolPlanOutcome::Blocked(results, guidance));
                }
            };
            let action = manifest
                .action(&tool_request.action)
                .expect("validate_tool_request confirmed action exists");
            let granted_scopes = plugin_permission_scopes(&action.permissions);
            self.record_audit(
                audit_entries,
                AuditEntry::new(
                    Some(task.id),
                    "tool_policy_check",
                    "runtime submitted model-planned tool call to plugin policy",
                    json!({
                        "step_index": step_index,
                        "tool_index": tool_index,
                        "plugin_id": tool_request.plugin_id,
                        "action": tool_request.action,
                        "risk_tier": action.risk_tier,
                        "granted_scopes": granted_scopes,
                    }),
                ),
            )?;

            let call_result = match self.plugin_host.execute(
                PluginCallRequest::reactive(
                    tool_request.plugin_id.clone(),
                    tool_request.action.clone(),
                    tool_request.input.clone(),
                )
                .with_granted_scopes(granted_scopes)
                .with_sensitivity(sensitivity),
            ) {
                Ok(result) => result,
                Err(error) => {
                    let status = if matches!(error, crate::JarvisError::PolicyBlocked(_)) {
                        TaskStatus::Blocked
                    } else {
                        TaskStatus::Failed
                    };
                    self.update_task_status(task, status)?;
                    self.record_audit(
                        audit_entries,
                        AuditEntry::new(
                            Some(task.id),
                            "tool_execution_blocked",
                            "model-planned tool call was blocked before execution",
                            json!({
                                "step_index": step_index,
                                "tool_index": tool_index,
                                "plugin_id": tool_request.plugin_id,
                                "action": tool_request.action,
                                "error": error.to_string(),
                            }),
                        ),
                    )?;
                    return Ok(ToolPlanOutcome::Blocked(
                        results,
                        format!("Tool execution blocked: {error}"),
                    ));
                }
            };

            let result = model_tool_result(&call_result);
            self.record_audit(
                audit_entries,
                tool_result_audit_entry(task.id, step_index, tool_index, &call_result),
            )?;
            results.push(result);

            if call_result.status == PluginCallStatus::ApprovalRequired {
                self.update_task_status(task, TaskStatus::WaitingForApproval)?;
                return Ok(ToolPlanOutcome::WaitingForApproval(
                    results,
                    "Tool execution requires approval before continuing.".to_string(),
                ));
            }
        }

        Ok(ToolPlanOutcome::Completed(results))
    }

    fn validate_tool_request(
        &self,
        task_id: Uuid,
        step_index: u32,
        request: &ModelToolRequest,
    ) -> JarvisResult<crate::PluginManifest> {
        if request.plugin_id.trim().is_empty() {
            return Err(crate::JarvisError::Validation(
                "tool request plugin_id is required".to_string(),
            ));
        }
        if request.action.trim().is_empty() {
            return Err(crate::JarvisError::Validation(
                "tool request action is required".to_string(),
            ));
        }
        if !request.input.is_object() {
            return Err(crate::JarvisError::Validation(
                "tool request input must be an object".to_string(),
            ));
        }

        let manifest = self.plugin_host.manifest(&request.plugin_id)?;
        if manifest.source != PluginSource::FirstParty {
            return Err(crate::JarvisError::PolicyBlocked(
                "runtime only executes first-party model-planned tools".to_string(),
            ));
        }
        let action = manifest.action(&request.action).ok_or_else(|| {
            crate::JarvisError::Plugin(format!(
                "plugin {} does not declare action {}",
                request.plugin_id, request.action
            ))
        })?;
        action.input_schema.validate_value(
            &format!(
                "{}.{} model-planned input for task {task_id} step {step_index}",
                request.plugin_id, request.action
            ),
            &request.input,
        )?;

        Ok(manifest)
    }

    fn registered_first_party_tool_names(&self) -> Vec<String> {
        let mut tools = self
            .plugin_host
            .manifests()
            .unwrap_or_default()
            .into_iter()
            .filter(|manifest| manifest.source == PluginSource::FirstParty)
            .flat_map(|manifest| {
                let plugin_id = manifest.id;
                manifest
                    .actions
                    .into_iter()
                    .map(move |action| format!("{}.{}", plugin_id, action.name))
            })
            .collect::<Vec<_>>();
        tools.sort();
        tools
    }

    fn finish(
        &self,
        task: TaskRecord,
        message: impl Into<String>,
        route: Option<ModelRoute>,
        route_evidence: Option<ModelRouteRecord>,
        steps: Vec<RuntimeStep>,
        audit_entries: Vec<AuditEntry>,
    ) -> CommandResponse {
        let tool_results = steps
            .iter()
            .flat_map(|step| step.tool_results.clone())
            .collect();
        let response = CommandResponse {
            task,
            message: message.into(),
            route,
            route_evidence,
            steps,
            tool_results,
            audit_entries,
        };
        self.hooks.task_finished(&response.task, &response);
        response
    }
}

fn touch(task: &mut TaskRecord) {
    task.updated_at = Utc::now();
}

fn model_audit_entry(task_id: Uuid, step_index: u32, response: &ModelResponse) -> AuditEntry {
    AuditEntry::new(
        Some(task_id),
        "model_step_completed",
        "model step completed",
        json!({
            "step_index": step_index,
            "provider": response.route.provider,
            "model": response.route.model,
            "complete": response.complete,
            "planned_tool_calls": response.tool_requests.len(),
        }),
    )
}

fn tool_plan_audit_entry(
    task_id: Uuid,
    step_index: u32,
    tool_requests: &[ModelToolRequest],
) -> AuditEntry {
    AuditEntry::new(
        Some(task_id),
        "tool_plan_received",
        "model planned tool calls",
        json!({
            "step_index": step_index,
            "tool_calls": tool_requests.iter().map(|request| {
                json!({
                    "plugin_id": request.plugin_id,
                    "action": request.action,
                    "input_is_object": request.input.is_object(),
                })
            }).collect::<Vec<_>>(),
        }),
    )
}

fn tool_result_audit_entry(
    task_id: Uuid,
    step_index: u32,
    tool_index: usize,
    result: &PluginCallResult,
) -> AuditEntry {
    AuditEntry::new(
        Some(task_id),
        "tool_execution_result",
        "model-planned tool call completed policy and host execution",
        json!({
            "step_index": step_index,
            "tool_index": tool_index,
            "plugin_id": result.metadata.plugin_id,
            "action": result.metadata.action,
            "status": result.status,
            "approval_required": result.metadata.approval_required,
            "approval_status": result.metadata.approval_status,
            "risk_tier": result.metadata.risk_tier,
        }),
    )
}

fn model_tool_result(result: &PluginCallResult) -> ModelToolResult {
    ModelToolResult {
        plugin_id: result.metadata.plugin_id.clone(),
        action: result.metadata.action.clone(),
        status: serde_json::to_value(&result.status)
            .ok()
            .and_then(|value| value.as_str().map(ToOwned::to_owned))
            .unwrap_or_else(|| "failed".to_string()),
        output: result.output.clone(),
    }
}

fn route_from_evidence(record: &ModelRouteRecord) -> Option<ModelRoute> {
    match record.selected_provider {
        Some(crate::router::ModelProvider::Local) => Some(ModelRoute::local(
            record.evidence.local_model.clone(),
            record.reason.clone(),
        )),
        Some(crate::router::ModelProvider::ChatGpt) => {
            Some(ModelRoute::chatgpt("chatgpt", record.reason.clone()))
        }
        None => None,
    }
}

fn model_failure_message(step_index: u32) -> String {
    format!(
        "Model execution failed during step {step_index}. The task was marked failed; inspect audit entries for redacted provider diagnostics."
    )
}

fn model_error_kind(error: &crate::JarvisError) -> &'static str {
    match error {
        crate::JarvisError::PolicyBlocked(_) => "policy_blocked",
        crate::JarvisError::ApprovalRequired(_) => "approval_required",
        crate::JarvisError::Validation(_) => "validation",
        crate::JarvisError::Storage(_) => "storage",
        crate::JarvisError::Plugin(_) => "plugin",
        crate::JarvisError::Model(_) => "model",
        crate::JarvisError::Other(_) => "other",
    }
}

fn tool_rejection_message(error: &crate::JarvisError, registered_tools: &[String]) -> String {
    if registered_tools.is_empty() {
        return format!(
            "Tool request rejected: {error}. No first-party model tools are registered."
        );
    }

    format!(
        "Tool request rejected: {error}. Registered first-party model tools are: {}. Retry with one of those exact plugin.action names or answer without a tool.",
        registered_tools.join(", ")
    )
}

fn redacted_model_error(error: &crate::JarvisError) -> String {
    error
        .to_string()
        .split_whitespace()
        .map(redact_error_token)
        .collect::<Vec<_>>()
        .join(" ")
}

fn redact_error_token(token: &str) -> String {
    let normalized = token
        .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-')
        .to_ascii_lowercase();
    if normalized.contains("api_key")
        || normalized.contains("token")
        || normalized.contains("secret")
        || normalized.contains("password")
        || normalized.starts_with("sk-")
    {
        return "[REDACTED]".to_string();
    }

    if token.contains("://") && token.contains('@') {
        return "[REDACTED_URL]".to_string();
    }

    token.to_string()
}

fn route_audit_entry(task_id: Uuid, record: &ModelRouteRecord) -> AuditEntry {
    AuditEntry::new(
        Some(task_id),
        "model_route_selected",
        "model router selected the command route",
        json!({
            "route_id": record.id,
            "outcome": record.outcome,
            "selected_provider": record.selected_provider,
            "reason": record.reason,
            "sensitivity": record.sensitivity,
            "approval_status": record.approval_status,
            "redaction_applied": record.redaction_applied,
            "evidence": record.evidence,
        }),
    )
}

enum ToolPlanOutcome {
    Completed(Vec<ModelToolResult>),
    WaitingForApproval(Vec<ModelToolResult>, String),
    Blocked(Vec<ModelToolResult>, String),
}

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    };

    use super::*;
    use crate::model::{
        ChatGptProviderConfig, FakeLocalModel, LocalModelConfig, ModelProvider, ModelToolRequest,
        ProviderConfig, RoutedModelExecutor,
    };
    use crate::router::{ModelProvider as RoutedModelProvider, RouteOutcome};
    use crate::types::TaskStatus;
    use axum::routing::post;
    use axum::{Json, Router};
    use serde_json::json;
    use tokio::net::TcpListener;

    #[tokio::test]
    async fn executes_command_with_fake_local_model() {
        let runtime = ConversationRuntime::new(FakeLocalModel::default());

        let response = runtime
            .execute_command(CommandRequest::new("summarize today"))
            .await
            .expect("command should execute");

        assert_eq!(response.task.status, TaskStatus::Completed);
        assert_eq!(response.steps.len(), 1);
        assert!(response.message.contains("summarize today"));
        assert_eq!(
            response.route.expect("route").provider,
            ModelProvider::Local
        );
        let route_evidence = response.route_evidence.expect("route evidence");
        assert_eq!(route_evidence.outcome, RouteOutcome::Selected);
        assert_eq!(
            route_evidence.selected_provider,
            Some(RoutedModelProvider::Local)
        );
        assert!(response
            .audit_entries
            .iter()
            .any(|entry| entry.event_type == "task_completed"));
    }

    #[tokio::test]
    async fn sqlite_command_store_persists_runtime_task_and_audit_entries() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("jarvis.sqlite");
        let repo = SqliteRepository::open(&db_path).expect("sqlite repository");
        let runtime = ConversationRuntime::with_storage_parts(
            RuntimeConfig::default(),
            RuntimeControl::default(),
            FakeLocalModel::default(),
            NoopRuntimeHooks,
            &repo,
        );

        let response = runtime
            .execute_command(CommandRequest::new("persist this command"))
            .await
            .expect("command should execute");

        assert_eq!(response.task.status, TaskStatus::Completed);
        let persisted = repo
            .get_task(response.task.id)
            .expect("task lookup")
            .expect("persisted task");
        assert_eq!(persisted.status, TaskStatus::Completed);
        assert_eq!(persisted.user_input, "persist this command");

        let persisted_entries = repo
            .list_audit_entries(Some(response.task.id))
            .expect("audit lookup");
        let event_types = persisted_entries
            .iter()
            .map(|entry| entry.event_type.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            event_types,
            vec![
                "task_created",
                "model_route_selected",
                "task_running",
                "model_step_completed",
                "task_completed"
            ]
        );
        assert_eq!(persisted_entries.len(), response.audit_entries.len());

        let task_id = response.task.id;
        drop(runtime);
        drop(repo);

        let reopened = SqliteRepository::open(db_path).expect("reopened sqlite repository");
        assert_eq!(
            reopened
                .get_task(task_id)
                .expect("reopened task")
                .unwrap()
                .status,
            TaskStatus::Completed
        );
        assert_eq!(
            reopened
                .list_audit_entries(Some(task_id))
                .expect("reopened audit")
                .len(),
            5
        );
    }

    #[tokio::test]
    async fn chatgpt_disabled_blocks_before_model_execution_when_local_unavailable() {
        let executions = Arc::new(AtomicUsize::new(0));
        let runtime = ConversationRuntime::with_parts(
            RuntimeConfig::default()
                .with_provider_config(ProviderConfig::local_only().without_local()),
            RuntimeControl::default(),
            CountingModel {
                executions: Arc::clone(&executions),
            },
            NoopRuntimeHooks,
        );

        let response = runtime
            .execute_command(CommandRequest::new("needs remote model"))
            .await
            .expect("route block should return structured response");

        assert_eq!(response.task.status, TaskStatus::Blocked);
        assert!(response.steps.is_empty());
        assert_eq!(executions.load(Ordering::SeqCst), 0);
        let route = response.route_evidence.expect("route evidence");
        assert_eq!(route.outcome, RouteOutcome::Blocked);
        assert!(route.reason.contains("ChatGPT routing is disabled"));
        assert!(!route.evidence.chatgpt_enabled);
    }

    #[tokio::test]
    async fn model_provider_failure_returns_failed_response_with_route_evidence() {
        let runtime = ConversationRuntime::with_parts(
            RuntimeConfig::default(),
            RuntimeControl::default(),
            FailingModel,
            NoopRuntimeHooks,
        );

        let response = runtime
            .execute_command(CommandRequest::new("provider failure"))
            .await
            .expect("provider failure should return structured response");

        assert_eq!(response.task.status, TaskStatus::Failed);
        assert!(response
            .message
            .contains("Model execution failed during step 0"));
        assert!(!response.message.contains("sk-test"));
        assert!(response.steps.is_empty());
        assert_eq!(
            response.route.as_ref().expect("failed route").provider,
            ModelProvider::Local
        );
        let route = response.route_evidence.as_ref().expect("route evidence");
        assert_eq!(route.outcome, RouteOutcome::Selected);
        assert_eq!(route.selected_provider, Some(RoutedModelProvider::Local));
        let failure = response
            .audit_entries
            .iter()
            .find(|entry| entry.event_type == "model_step_failed")
            .expect("failure audit");
        assert_eq!(failure.payload["error_kind"], "model");
        assert_eq!(failure.payload["selected_provider"], "local");
        let encoded = serde_json::to_string(&response).expect("response JSON");
        assert!(!encoded.contains("sk-test"));
    }

    #[tokio::test]
    async fn restricted_data_never_routes_to_cloud_before_model_execution() {
        let executions = Arc::new(AtomicUsize::new(0));
        let runtime = ConversationRuntime::with_parts(
            RuntimeConfig::default().with_provider_config(
                ProviderConfig::local_only()
                    .without_local()
                    .with_chatgpt_enabled("chatgpt-disabled-test"),
            ),
            RuntimeControl::default(),
            CountingModel {
                executions: Arc::clone(&executions),
            },
            NoopRuntimeHooks,
        );

        let response = runtime
            .execute_command(
                CommandRequest::new("summarize restricted credential material")
                    .with_sensitivity(Sensitivity::Restricted),
            )
            .await
            .expect("restricted route block should return structured response");

        assert_eq!(response.task.status, TaskStatus::Blocked);
        assert!(response.steps.is_empty());
        assert_eq!(executions.load(Ordering::SeqCst), 0);
        let route = response.route_evidence.expect("route evidence");
        assert_eq!(route.outcome, RouteOutcome::Blocked);
        assert!(route.reason.contains("restricted data"));
        assert!(route.evidence.chatgpt_enabled);
        assert!(route.evidence.restricted_cloud_block);
    }

    #[tokio::test]
    async fn chatgpt_route_executes_only_after_opt_in_and_records_audited_evidence() {
        async fn chat(Json(body): Json<serde_json::Value>) -> Json<serde_json::Value> {
            let user_content = body["messages"][1]["content"]
                .as_str()
                .expect("user content");
            assert!(user_content.contains("Redacted task context: summarize [REDACTED]"));
            assert!(!user_content.contains("api_key=abc123"));
            Json(json!({
                "choices": [
                    { "message": { "content": "cloud route answer" } }
                ]
            }))
        }

        let app = Router::new().route("/v1/chat/completions", post(chat));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("test server");
        });

        let provider_config = ProviderConfig {
            local: LocalModelConfig {
                enabled: false,
                ..LocalModelConfig::default()
            },
            chatgpt: ChatGptProviderConfig {
                enabled: true,
                model: "gpt-test".to_string(),
                base_url: format!("http://{address}/v1"),
                api_key: Some("test-token-value".to_string()),
                requires_approval: true,
                timeout_ms: 2_000,
            },
        };
        let runtime = ConversationRuntime::with_parts(
            RuntimeConfig::default().with_provider_config(provider_config.clone()),
            RuntimeControl::default(),
            RoutedModelExecutor::from_config(&provider_config).expect("routed model"),
            NoopRuntimeHooks,
        );

        let response = runtime
            .execute_command(
                CommandRequest::new("summarize api_key=abc123")
                    .with_sensitivity(Sensitivity::Workspace),
            )
            .await
            .expect("ChatGPT route should execute");

        assert_eq!(response.task.status, TaskStatus::Completed);
        assert_eq!(response.message, "cloud route answer");
        assert_eq!(
            response.route.as_ref().expect("route").provider,
            ModelProvider::ChatGpt
        );
        let route = response.route_evidence.as_ref().expect("route evidence");
        assert_eq!(route.outcome, RouteOutcome::Selected);
        assert_eq!(
            route.context_for_model.as_deref(),
            Some("summarize [REDACTED]")
        );
        assert!(route.redaction_applied);
        assert!(response.audit_entries.iter().any(|entry| {
            entry.event_type == "model_route_selected"
                && entry.payload["evidence"]["chatgpt_enabled"] == true
                && entry.payload["redaction_applied"] == true
        }));

        let encoded = serde_json::to_string(&response.audit_entries).expect("audit JSON");
        assert!(!encoded.contains("test-token-value"));
        assert!(!encoded.contains("api_key=abc123"));
    }

    #[tokio::test]
    async fn enforces_step_limit() {
        let runtime = ConversationRuntime::with_parts(
            RuntimeConfig::new(2),
            RuntimeControl::default(),
            FakeLocalModel::default().complete_after_steps(3),
            NoopRuntimeHooks,
        );

        let response = runtime
            .execute_command(CommandRequest::new("keep working"))
            .await
            .expect("command should return structured failure");

        assert_eq!(response.task.status, TaskStatus::Failed);
        assert_eq!(response.steps.len(), 2);
        assert_eq!(
            response
                .audit_entries
                .last()
                .expect("audit entry")
                .event_type,
            "step_limit_exceeded"
        );
    }

    #[tokio::test]
    async fn executes_model_planned_first_party_tool_call() {
        let runtime = ConversationRuntime::new(FakeLocalModel::default().with_tool_request(
            ModelToolRequest::new("fake_echo", "echo", json!({ "message": "from plan" })),
        ));

        let response = runtime
            .execute_command(CommandRequest::new("use echo tool"))
            .await
            .expect("command should execute");

        assert_eq!(response.task.status, TaskStatus::Completed);
        assert_eq!(response.steps.len(), 2);
        assert!(!response.steps[0].complete);
        assert_eq!(response.tool_results.len(), 1);
        assert_eq!(response.tool_results[0].plugin_id, "fake_echo");
        assert_eq!(response.tool_results[0].action, "echo");
        assert_eq!(response.tool_results[0].status, "completed");
        assert_eq!(
            response.tool_results[0].output,
            json!({ "message": "from plan" })
        );
        assert_eq!(response.steps[0].tool_results, response.tool_results);
        let event_types = response
            .audit_entries
            .iter()
            .map(|entry| entry.event_type.as_str())
            .collect::<Vec<_>>();
        assert!(event_types.contains(&"tool_plan_received"));
        assert!(event_types.contains(&"tool_policy_check"));
        assert!(event_types.contains(&"tool_execution_result"));
    }

    #[tokio::test]
    async fn feeds_tool_results_back_into_next_model_step() {
        let runtime = ConversationRuntime::new(ToolAwareModel);

        let response = runtime
            .execute_command(CommandRequest::new("inspect tool result"))
            .await
            .expect("command should execute");

        assert_eq!(response.task.status, TaskStatus::Completed);
        assert_eq!(response.steps.len(), 2);
        assert_eq!(response.tool_results.len(), 1);
        assert_eq!(response.message, "saw tool result: from tool-aware model");
    }

    #[tokio::test]
    async fn provider_originated_tool_request_executes_first_party_tool_and_feeds_result() {
        let runtime = ConversationRuntime::new(ProviderEnvelopeToolModel);

        let response = runtime
            .execute_command(CommandRequest::new("provider envelope tool"))
            .await
            .expect("command should execute");

        assert_eq!(response.task.status, TaskStatus::Completed);
        assert_eq!(response.steps.len(), 2);
        assert_eq!(response.tool_results.len(), 1);
        assert_eq!(response.tool_results[0].plugin_id, "fake_echo");
        assert_eq!(
            response.message,
            "provider envelope saw: from provider envelope"
        );
        assert!(response
            .audit_entries
            .iter()
            .any(|entry| entry.event_type == "tool_plan_received"));
    }

    #[tokio::test]
    async fn private_model_planned_tool_call_waits_for_approval_before_execution() {
        let runtime = ConversationRuntime::new(FakeLocalModel::default().with_tool_request(
            ModelToolRequest::new("fake_echo", "echo", json!({ "message": "private" })),
        ));

        let response = runtime
            .execute_command(
                CommandRequest::new("use echo tool on private context")
                    .with_sensitivity(Sensitivity::Private),
            )
            .await
            .expect("approval requirement should return structured response");

        assert_eq!(response.task.status, TaskStatus::WaitingForApproval);
        assert_eq!(response.tool_results.len(), 1);
        assert_eq!(response.tool_results[0].status, "approval_required");
        assert_eq!(
            response.tool_results[0].output,
            json!({ "approval_required": true })
        );
        assert!(response
            .audit_entries
            .iter()
            .any(|entry| entry.event_type == "tool_execution_result"
                && entry.payload["approval_required"] == true
                && entry.payload["approval_status"] == "pending"));
    }

    #[tokio::test]
    async fn rejects_invalid_model_planned_tool_request_before_execution() {
        let runtime = ConversationRuntime::new(FakeLocalModel::default().with_tool_request(
            ModelToolRequest::new("fake_echo", "echo", json!("not an object")),
        ));

        let response = runtime
            .execute_command(CommandRequest::new("use malformed tool"))
            .await
            .expect("validation failure should return structured response");

        assert_eq!(response.task.status, TaskStatus::Failed);
        assert!(response.tool_results.is_empty());
        assert!(response.message.contains("Tool request rejected"));
        assert!(response
            .message
            .contains("Registered first-party model tools are: fake_echo.approval_echo, fake_echo.echo, fake_status.status"));
        assert!(response.audit_entries.iter().any(|entry| entry.event_type
            == "tool_request_rejected"
            && entry.payload["error"]
                .as_str()
                .expect("error")
                .contains("input must be an object")
            && entry.payload["registered_tools"]
                .as_array()
                .expect("registered tools")
                .iter()
                .any(|tool| tool == "fake_status.status")));
        assert!(!response.audit_entries.iter().any(|entry| {
            entry.event_type == "tool_policy_check" || entry.event_type == "tool_execution_result"
        }));
    }

    #[tokio::test]
    async fn rejects_hallucinated_model_planned_plugin_with_registered_tool_guidance() {
        let runtime = ConversationRuntime::new(
            FakeLocalModel::default().with_tool_request(ModelToolRequest::new(
                "status",
                "status",
                json!({}),
            )),
        );

        let response = runtime
            .execute_command(CommandRequest::new("use invented status tool"))
            .await
            .expect("validation failure should return structured response");

        assert_eq!(response.task.status, TaskStatus::Failed);
        assert!(response.tool_results.is_empty());
        assert!(response
            .message
            .contains("plugin error: plugin status is not registered"));
        assert!(response
            .message
            .contains("Registered first-party model tools are: fake_echo.approval_echo, fake_echo.echo, fake_status.status"));
        let rejection = response
            .audit_entries
            .iter()
            .find(|entry| entry.event_type == "tool_request_rejected")
            .expect("rejection audit");
        assert_eq!(rejection.payload["plugin_id"], "status");
        assert_eq!(rejection.payload["action"], "status");
        assert!(rejection.payload["registered_tools"]
            .as_array()
            .expect("registered tools")
            .iter()
            .any(|tool| tool == "fake_status.status"));
        assert!(!response.audit_entries.iter().any(|entry| {
            entry.event_type == "tool_policy_check" || entry.event_type == "tool_execution_result"
        }));
    }

    #[tokio::test]
    async fn rejects_hallucinated_model_planned_action_with_registered_tool_guidance() {
        let runtime = ConversationRuntime::new(
            FakeLocalModel::default().with_tool_request(ModelToolRequest::new(
                "fake_status",
                "list",
                json!({}),
            )),
        );

        let response = runtime
            .execute_command(CommandRequest::new("use invented status action"))
            .await
            .expect("validation failure should return structured response");

        assert_eq!(response.task.status, TaskStatus::Failed);
        assert!(response.tool_results.is_empty());
        assert!(response
            .message
            .contains("plugin fake_status does not declare action list"));
        assert!(response
            .message
            .contains("Registered first-party model tools are: fake_echo.approval_echo, fake_echo.echo, fake_status.status"));
        let rejection = response
            .audit_entries
            .iter()
            .find(|entry| entry.event_type == "tool_request_rejected")
            .expect("rejection audit");
        assert_eq!(rejection.payload["plugin_id"], "fake_status");
        assert_eq!(rejection.payload["action"], "list");
        assert!(rejection.payload["registered_tools"]
            .as_array()
            .expect("registered tools")
            .iter()
            .any(|tool| tool == "fake_status.status"));
    }

    #[tokio::test]
    async fn rejects_tool_plan_that_exceeds_runtime_bound() {
        let runtime = ConversationRuntime::with_parts(
            RuntimeConfig::default().with_max_tool_calls_per_step(1),
            RuntimeControl::default(),
            FakeLocalModel::default()
                .with_tool_request(ModelToolRequest::new(
                    "fake_echo",
                    "echo",
                    json!({ "message": "one" }),
                ))
                .with_tool_request(ModelToolRequest::new(
                    "fake_echo",
                    "echo",
                    json!({ "message": "two" }),
                )),
            NoopRuntimeHooks,
        );

        let response = runtime
            .execute_command(CommandRequest::new("too many tools"))
            .await
            .expect("bounded plan failure should return structured response");

        assert_eq!(response.task.status, TaskStatus::Failed);
        assert!(response.tool_results.is_empty());
        assert!(response
            .audit_entries
            .iter()
            .any(|entry| entry.event_type == "tool_plan_rejected"));
    }

    #[tokio::test]
    async fn emergency_pause_blocks_new_commands() {
        let control = RuntimeControl::default();
        control.emergency_pause();
        let runtime = ConversationRuntime::with_parts(
            RuntimeConfig::default(),
            control,
            FakeLocalModel::default(),
            NoopRuntimeHooks,
        );

        let response = runtime
            .execute_command(CommandRequest::new("turn on lights"))
            .await
            .expect("pause should produce blocked response");

        assert_eq!(response.task.status, TaskStatus::Blocked);
        assert!(response.steps.is_empty());
        assert!(response
            .audit_entries
            .iter()
            .any(|entry| entry.event_type == "emergency_pause_blocked"));
    }

    struct PauseOnStepHook {
        control: RuntimeControl,
    }

    impl RuntimeHooks for PauseOnStepHook {
        fn before_model_step(&self, _task: &TaskRecord, step_index: u32) {
            if step_index == 1 {
                self.control.emergency_pause();
            }
        }
    }

    #[tokio::test]
    async fn emergency_pause_cancels_active_command() {
        let control = RuntimeControl::default();
        let hooks = PauseOnStepHook {
            control: control.clone(),
        };
        let runtime = ConversationRuntime::with_parts(
            RuntimeConfig::new(4),
            control,
            FakeLocalModel::default().complete_after_steps(4),
            hooks,
        );

        let response = runtime
            .execute_command(CommandRequest::new("multi step"))
            .await
            .expect("emergency pause should return structured response");

        assert_eq!(response.task.status, TaskStatus::Cancelled);
        assert_eq!(response.steps.len(), 1);
        assert_eq!(
            response
                .audit_entries
                .last()
                .expect("audit entry")
                .event_type,
            "emergency_pause_cancelled"
        );
    }

    struct CancelOnStepHook {
        control: RuntimeControl,
        task_ids: Mutex<Vec<Uuid>>,
    }

    impl RuntimeHooks for CancelOnStepHook {
        fn task_created(&self, task: &TaskRecord) {
            self.task_ids.lock().expect("task id lock").push(task.id);
        }

        fn before_model_step(&self, task: &TaskRecord, step_index: u32) {
            if step_index == 1 {
                self.control.cancel_task(task.id);
            }
        }
    }

    #[tokio::test]
    async fn cancellation_hook_stops_before_next_model_step() {
        let control = RuntimeControl::default();
        let hooks = CancelOnStepHook {
            control: control.clone(),
            task_ids: Mutex::new(Vec::new()),
        };
        let runtime = ConversationRuntime::with_parts(
            RuntimeConfig::new(4),
            control,
            FakeLocalModel::default().complete_after_steps(4),
            hooks,
        );

        let response = runtime
            .execute_command(CommandRequest::new("multi step"))
            .await
            .expect("cancellation should return structured response");

        assert_eq!(response.task.status, TaskStatus::Cancelled);
        assert_eq!(response.steps.len(), 1);
        assert_eq!(
            response
                .audit_entries
                .last()
                .expect("audit entry")
                .event_type,
            "task_cancelled"
        );
    }

    struct CountingModel {
        executions: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl ModelExecutor for CountingModel {
        async fn execute(&self, request: ModelRequest) -> JarvisResult<ModelResponse> {
            self.executions.fetch_add(1, Ordering::SeqCst);
            Ok(ModelResponse {
                route: ModelRoute::fake_local("counting local model"),
                message: format!("counted: {}", request.user_input),
                complete: true,
                tool_requests: Vec::new(),
            })
        }
    }

    struct FailingModel;

    #[async_trait::async_trait]
    impl ModelExecutor for FailingModel {
        async fn execute(&self, _request: ModelRequest) -> JarvisResult<ModelResponse> {
            Err(crate::JarvisError::Model(
                "provider failed with token=sk-test".to_string(),
            ))
        }
    }

    struct ToolAwareModel;

    #[async_trait::async_trait]
    impl ModelExecutor for ToolAwareModel {
        async fn execute(&self, request: ModelRequest) -> JarvisResult<ModelResponse> {
            if request.step_index == 0 {
                return Ok(ModelResponse {
                    route: ModelRoute::fake_local("tool-aware local model"),
                    message: "planning echo".to_string(),
                    complete: false,
                    tool_requests: vec![ModelToolRequest::new(
                        "fake_echo",
                        "echo",
                        json!({ "message": "from tool-aware model" }),
                    )],
                });
            }

            Ok(ModelResponse {
                route: ModelRoute::fake_local("tool-aware local model"),
                message: format!(
                    "saw tool result: {}",
                    request.tool_results[0].output["message"]
                        .as_str()
                        .expect("echo message")
                ),
                complete: true,
                tool_requests: Vec::new(),
            })
        }
    }

    struct ProviderEnvelopeToolModel;

    #[async_trait::async_trait]
    impl ModelExecutor for ProviderEnvelopeToolModel {
        async fn execute(&self, request: ModelRequest) -> JarvisResult<ModelResponse> {
            if request.step_index == 0 {
                return Ok(ModelResponse {
                    route: ModelRoute::local(
                        "test-provider-envelope",
                        "provider-originated strict envelope",
                    ),
                    message: "provider envelope requested echo".to_string(),
                    complete: false,
                    tool_requests: vec![ModelToolRequest::new(
                        "fake_echo",
                        "echo",
                        json!({ "message": "from provider envelope" }),
                    )],
                });
            }

            Ok(ModelResponse {
                route: ModelRoute::local(
                    "test-provider-envelope",
                    "provider-originated strict envelope",
                ),
                message: format!(
                    "provider envelope saw: {}",
                    request.tool_results[0].output["message"]
                        .as_str()
                        .expect("echo message")
                ),
                complete: true,
                tool_requests: Vec::new(),
            })
        }
    }
}
