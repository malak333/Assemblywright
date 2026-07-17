use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::model::{
    model_tool_definitions_from_manifests, ModelExecutor, ModelRequest, ModelResponse, ModelRoute,
    ModelToolRequest, ModelToolResult, ProviderConfig,
};
use crate::plugin::{
    plugin_permission_scopes, PluginCallRequest, PluginCallResult, PluginCallStatus, PluginHost,
    PluginManifest, PluginSource,
};
use crate::router::{
    ModelProvider as RouteModelProvider, ModelRouteRecord, ModelRouteRequest, ModelRouter,
    RouteOutcome,
};
use crate::storage::SqliteRepository;
use crate::types::{AuditEntry, JarvisResult, Sensitivity, TaskRecord, TaskStatus};
use crate::{ApprovalGrant, CapabilityScope, JarvisError, MemoryRetrieval, MemoryRetrievalControl};

const MAX_TASK_TOOL_OUTPUT_BYTES: usize = 128 * 1024;
const MAX_ACTIVE_RUNTIME_CANCELLATIONS: usize = 128;
const MAX_CONSUMED_RUNTIME_CANCELLATIONS: usize = 1_024;
const MAX_MODEL_PLANNED_WASM_TOOLS: usize = 16;
const MAX_MODEL_PLANNED_WASM_DESCRIPTION_BYTES: usize = 1_024;
const MAX_MODEL_PLANNED_WASM_SCHEMA_BYTES: usize = 16 * 1_024;
const MAX_MODEL_PLANNED_WASM_CATALOG_BYTES: usize = 64 * 1_024;

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
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub proactive: bool,
    #[serde(default)]
    pub memory_context: bool,
    #[serde(default)]
    pub installed_wasm_tools: bool,
    #[serde(default)]
    pub cancellation_id: Option<Uuid>,
    #[serde(default)]
    pub cloud_route_approved: bool,
    #[serde(default)]
    pub expected_workspace_request: Option<serde_json::Value>,
}

impl CommandRequest {
    pub fn new(input: impl Into<String>) -> Self {
        Self {
            session_id: Uuid::new_v4(),
            input: input.into(),
            sensitivity: Sensitivity::Personal,
            dry_run: false,
            proactive: false,
            memory_context: false,
            installed_wasm_tools: false,
            cancellation_id: None,
            cloud_route_approved: false,
            expected_workspace_request: None,
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

    pub fn with_dry_run(mut self, dry_run: bool) -> Self {
        self.dry_run = dry_run;
        self
    }

    pub fn with_proactive(mut self, proactive: bool) -> Self {
        self.proactive = proactive;
        self
    }

    pub fn with_memory_context(mut self, memory_context: bool) -> Self {
        self.memory_context = memory_context;
        self
    }

    pub fn with_installed_wasm_tools(mut self, installed_wasm_tools: bool) -> Self {
        self.installed_wasm_tools = installed_wasm_tools;
        self
    }

    pub fn with_cancellation_id(mut self, cancellation_id: Option<Uuid>) -> Self {
        self.cancellation_id = cancellation_id;
        self
    }

    pub fn with_cloud_route_approval(mut self, approved: bool) -> Self {
        self.cloud_route_approved = approved;
        self
    }

    pub fn with_expected_workspace_request(
        mut self,
        expected_workspace_request: Option<serde_json::Value>,
    ) -> Self {
        self.expected_workspace_request = expected_workspace_request;
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
    active_runtime_cancellations: HashSet<Uuid>,
    started_runtime_cancellations: HashSet<Uuid>,
    cancelled_runtime_cancellations: HashSet<Uuid>,
    runtime_cancellation_tasks: HashMap<Uuid, Uuid>,
    consumed_runtime_cancellations: HashSet<Uuid>,
    consumed_runtime_cancellation_order: VecDeque<Uuid>,
}

pub struct RuntimeCancellationGuard {
    control: RuntimeControl,
    cancellation_id: Uuid,
    active: bool,
}

impl RuntimeCancellationGuard {
    pub fn activate(&mut self) {
        self.control
            .activate_runtime_cancellation(self.cancellation_id);
    }

    pub fn finalize(mut self) -> bool {
        let cancelled = self
            .control
            .finish_runtime_cancellation(self.cancellation_id);
        self.active = false;
        cancelled
    }
}

impl Drop for RuntimeCancellationGuard {
    fn drop(&mut self) {
        if self.active {
            self.control
                .finish_runtime_cancellation(self.cancellation_id);
        }
    }
}

impl RuntimeControl {
    pub fn emergency_pause(&self) {
        let mut state = self.inner.lock().expect("runtime control lock poisoned");
        state.emergency_paused = true;
        let started = state
            .started_runtime_cancellations
            .iter()
            .copied()
            .collect::<Vec<_>>();
        state.cancelled_runtime_cancellations.extend(started);
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
        if state.started_runtime_cancellations.contains(&task_id) {
            state.cancelled_runtime_cancellations.insert(task_id);
        }
    }

    pub fn is_task_cancelled(&self, task_id: Uuid) -> bool {
        self.inner
            .lock()
            .expect("runtime control lock poisoned")
            .cancelled_tasks
            .contains(&task_id)
    }

    pub fn register_runtime_cancellation(
        &self,
        cancellation_id: Uuid,
    ) -> Option<RuntimeCancellationGuard> {
        let mut state = self.inner.lock().expect("runtime control lock poisoned");
        if state.active_runtime_cancellations.len() >= MAX_ACTIVE_RUNTIME_CANCELLATIONS
            || state
                .consumed_runtime_cancellations
                .contains(&cancellation_id)
            || !state.active_runtime_cancellations.insert(cancellation_id)
        {
            return None;
        }
        state
            .cancelled_runtime_cancellations
            .remove(&cancellation_id);
        Some(RuntimeCancellationGuard {
            control: self.clone(),
            cancellation_id,
            active: true,
        })
    }

    pub fn cancel_runtime_execution(&self, cancellation_id: Uuid) -> bool {
        let mut state = self.inner.lock().expect("runtime control lock poisoned");
        if !state
            .started_runtime_cancellations
            .contains(&cancellation_id)
        {
            return false;
        }
        state
            .cancelled_runtime_cancellations
            .insert(cancellation_id);
        if let Some(task_id) = state
            .runtime_cancellation_tasks
            .get(&cancellation_id)
            .copied()
        {
            state.cancelled_tasks.insert(task_id);
            if state.started_runtime_cancellations.contains(&task_id) {
                state.cancelled_runtime_cancellations.insert(task_id);
            }
        }
        true
    }

    pub fn bind_runtime_cancellation_to_task(&self, cancellation_id: Uuid, task_id: Uuid) -> bool {
        let mut state = self.inner.lock().expect("runtime control lock poisoned");
        if !state
            .active_runtime_cancellations
            .contains(&cancellation_id)
        {
            return false;
        }
        state
            .runtime_cancellation_tasks
            .insert(cancellation_id, task_id);
        if state
            .cancelled_runtime_cancellations
            .contains(&cancellation_id)
        {
            state.cancelled_tasks.insert(task_id);
        }
        true
    }

    pub fn is_runtime_cancelled(&self, cancellation_id: Uuid) -> bool {
        self.inner
            .lock()
            .expect("runtime control lock poisoned")
            .cancelled_runtime_cancellations
            .contains(&cancellation_id)
    }

    fn activate_runtime_cancellation(&self, cancellation_id: Uuid) {
        let mut state = self.inner.lock().expect("runtime control lock poisoned");
        if state
            .active_runtime_cancellations
            .contains(&cancellation_id)
        {
            state.started_runtime_cancellations.insert(cancellation_id);
        }
    }

    fn finish_runtime_cancellation(&self, cancellation_id: Uuid) -> bool {
        let mut state = self.inner.lock().expect("runtime control lock poisoned");
        state.active_runtime_cancellations.remove(&cancellation_id);
        state.started_runtime_cancellations.remove(&cancellation_id);
        let task_id = state.runtime_cancellation_tasks.remove(&cancellation_id);
        let cancelled = state
            .cancelled_runtime_cancellations
            .remove(&cancellation_id);
        if let Some(task_id) = task_id {
            state.cancelled_tasks.remove(&task_id);
        }
        remember_consumed_runtime_cancellation(&mut state, cancellation_id);
        cancelled
    }

    fn clear_task_cancellation(&self, task_id: Uuid) {
        let mut state = self.inner.lock().expect("runtime control lock poisoned");
        state.cancelled_tasks.remove(&task_id);
    }
}

fn remember_consumed_runtime_cancellation(state: &mut RuntimeControlState, cancellation_id: Uuid) {
    if !state.consumed_runtime_cancellations.insert(cancellation_id) {
        return;
    }
    state
        .consumed_runtime_cancellation_order
        .push_back(cancellation_id);
    while state.consumed_runtime_cancellation_order.len() > MAX_CONSUMED_RUNTIME_CANCELLATIONS {
        if let Some(expired) = state.consumed_runtime_cancellation_order.pop_front() {
            state.consumed_runtime_cancellations.remove(&expired);
        }
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
    fn retrieve_memory_context(
        &self,
        _query: &str,
        _sensitivity: Sensitivity,
        _control: &mut dyn FnMut() -> MemoryRetrievalControl,
    ) -> JarvisResult<MemoryRetrieval> {
        Err(JarvisError::Storage(
            "memory retrieval requires repository-backed storage".to_string(),
        ))
    }

    fn model_planned_wasm_manifests(&self) -> JarvisResult<Vec<crate::PluginManifest>> {
        Ok(Vec::new())
    }

    fn execute_model_planned_wasm(
        &self,
        _task_id: Uuid,
        _session_id: Uuid,
        _request: &ModelToolRequest,
    ) -> JarvisResult<PluginCallResult> {
        Err(JarvisError::PolicyBlocked(
            "model-planned installed WASM execution requires repository-backed IPC".to_string(),
        ))
    }
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

    fn retrieve_memory_context(
        &self,
        query: &str,
        sensitivity: Sensitivity,
        control: &mut dyn FnMut() -> MemoryRetrievalControl,
    ) -> JarvisResult<MemoryRetrieval> {
        (*self).retrieve_memory_context(query, sensitivity, control)
    }

    fn model_planned_wasm_manifests(&self) -> JarvisResult<Vec<crate::PluginManifest>> {
        (*self).model_planned_wasm_manifests()
    }

    fn execute_model_planned_wasm(
        &self,
        task_id: Uuid,
        session_id: Uuid,
        request: &ModelToolRequest,
    ) -> JarvisResult<PluginCallResult> {
        (*self).execute_model_planned_wasm(task_id, session_id, request)
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

    fn retrieve_memory_context(
        &self,
        query: &str,
        sensitivity: Sensitivity,
        control: &mut dyn FnMut() -> MemoryRetrievalControl,
    ) -> JarvisResult<MemoryRetrieval> {
        self.retrieve_memory_context_with_control(query, sensitivity, control)
    }

    fn model_planned_wasm_manifests(&self) -> JarvisResult<Vec<crate::PluginManifest>> {
        SqliteRepository::model_planned_wasm_manifests(self)
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
        if let Some(cancellation_id) = request.cancellation_id {
            if !self
                .control
                .bind_runtime_cancellation_to_task(cancellation_id, task.id)
            {
                return Err(JarvisError::Conflict(
                    "command cancellation handle is no longer active".to_string(),
                ));
            }
        }
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
                    "installed_wasm_tools_requested": request.installed_wasm_tools,
                    "cloud_route_approved": request.cloud_route_approved,
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
            approval: (request.cloud_route_approved && !request.proactive).then(|| {
                ApprovalGrant::approved(vec![
                    CapabilityScope::Conversation,
                    CapabilityScope::LocalModel,
                    CapabilityScope::CloudModel,
                ])
            }),
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

        let memory_context = if request.memory_context {
            if request.proactive
                || route_record.selected_provider != Some(RouteModelProvider::Local)
            {
                self.update_task_status(&mut task, TaskStatus::Blocked)?;
                self.record_audit(
                    &mut audit_entries,
                    AuditEntry::new(
                        Some(task.id),
                        "memory_context_blocked",
                        "memory context request failed closed before retrieval",
                        json!({
                            "requested": true,
                            "proactive": request.proactive,
                            "local_route": route_record.selected_provider == Some(RouteModelProvider::Local),
                            "retrieval_query_omitted": true,
                            "values_redacted": true,
                        }),
                    ),
                )?;
                return Ok(self.finish(
                    task,
                    "Memory context is available only for explicit, non-proactive local-model commands.",
                    None,
                    Some(route_record),
                    vec![],
                    audit_entries,
                ));
            }
            let mut control = || {
                if self.control.is_emergency_paused() {
                    MemoryRetrievalControl::EmergencyPaused
                } else if self.control.is_task_cancelled(task.id) {
                    MemoryRetrievalControl::Cancelled
                } else {
                    MemoryRetrievalControl::Continue
                }
            };
            match self.command_store.retrieve_memory_context(
                &task.user_input,
                request.sensitivity,
                &mut control,
            ) {
                Ok(retrieval) => {
                    self.record_audit(
                        &mut audit_entries,
                        AuditEntry::new(
                            Some(task.id),
                            "memory_context_checked",
                            "bounded local memory context was evaluated",
                            json!({
                                "requested": true,
                                "attached": !retrieval.context.is_empty(),
                                "matched_count": retrieval.matched_count,
                                "omitted_count": retrieval.omitted_count,
                                "context_bytes": retrieval.context.len(),
                                "highest_sensitivity": retrieval.highest_sensitivity,
                                "retrieval_query_omitted": true,
                                "values_redacted": true,
                                "identifiers_redacted": true,
                            }),
                        ),
                    )?;
                    (!retrieval.context.is_empty()).then_some(retrieval.context)
                }
                Err(_)
                    if self.control.is_emergency_paused()
                        || self.control.is_task_cancelled(task.id) =>
                {
                    let emergency_paused = self.control.is_emergency_paused();
                    if !emergency_paused {
                        self.control.clear_task_cancellation(task.id);
                    }
                    self.update_task_status(&mut task, TaskStatus::Cancelled)?;
                    self.record_audit(
                        &mut audit_entries,
                        AuditEntry::new(
                            Some(task.id),
                            if emergency_paused {
                                "emergency_pause_cancelled"
                            } else {
                                "task_cancelled"
                            },
                            "memory retrieval was cancelled before model execution",
                            json!({
                                "memory_context_requested": true,
                                "retrieval_query_omitted": true,
                                "values_redacted": true,
                            }),
                        ),
                    )?;
                    return Ok(self.finish(
                        task,
                        if emergency_paused {
                            "Command cancelled because emergency pause was activated."
                        } else {
                            "Command cancelled."
                        },
                        None,
                        Some(route_record),
                        vec![],
                        audit_entries,
                    ));
                }
                Err(error) => {
                    let failure_kind = match error {
                        JarvisError::Validation(_) => "invalid_or_over_budget",
                        JarvisError::Storage(_) => "index_unavailable_or_invalid",
                        JarvisError::PolicyBlocked(_) => "policy_blocked",
                        _ => "internal_failure",
                    };
                    self.update_task_status(&mut task, TaskStatus::Blocked)?;
                    self.record_audit(
                        &mut audit_entries,
                        AuditEntry::new(
                            Some(task.id),
                            "memory_context_blocked",
                            "memory context retrieval failed closed",
                            json!({
                                "requested": true,
                                "failure_kind": failure_kind,
                                "retrieval_query_omitted": true,
                                "values_redacted": true,
                                "identifiers_redacted": true,
                            }),
                        ),
                    )?;
                    return Ok(self.finish(
                        task,
                        "Memory context is unavailable; rebuild the local memory index or retry without memory context.",
                        None,
                        Some(route_record),
                        vec![],
                        audit_entries,
                    ));
                }
            }
        } else {
            None
        };

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

            let selected_provider = route_evidence
                .as_ref()
                .and_then(|record| record.selected_provider);
            // Preserve the exact catalog presented to this model step. The installed
            // runner independently revalidates current provenance and grants just
            // before execution, but a mid-step mutation must fail as an execution
            // denial rather than changing the historical model allowlist.
            let advertised_installed_wasm = self.registered_installed_wasm_manifests_for_provider(
                selected_provider,
                request.installed_wasm_tools,
                request.proactive,
            );
            let model_request = ModelRequest {
                task_id: task.id,
                session_id: task.session_id,
                user_input: task.user_input.clone(),
                step_index,
                tool_results: tool_results.clone(),
                memory_context: memory_context.clone(),
                first_party_tools: self.registered_model_tools_from_manifests(
                    selected_provider,
                    &advertised_installed_wasm,
                ),
            };
            let execution_route = route_evidence
                .as_ref()
                .expect("route evidence is set before execution")
                .clone();
            let model_result = {
                let model_future = self.model.execute_route(model_request, &execution_route);
                tokio::pin!(model_future);
                loop {
                    tokio::select! {
                        result = &mut model_future => break Some(result),
                        _ = tokio::time::sleep(std::time::Duration::from_millis(25)) => {
                            if self.control.is_emergency_paused()
                                || self.control.is_task_cancelled(task.id)
                            {
                                break None;
                            }
                        }
                    }
                }
            };
            let model_result =
                if self.control.is_emergency_paused() || self.control.is_task_cancelled(task.id) {
                    None
                } else {
                    model_result
                };
            let Some(model_result) = model_result else {
                let emergency_paused = self.control.is_emergency_paused();
                if !emergency_paused {
                    self.control.clear_task_cancellation(task.id);
                }
                self.update_task_status(&mut task, TaskStatus::Cancelled)?;
                self.record_audit(
                    &mut audit_entries,
                    AuditEntry::new(
                        Some(task.id),
                        if emergency_paused {
                            "emergency_pause_cancelled"
                        } else {
                            "task_cancelled"
                        },
                        "active model transport was cancelled before completion",
                        json!({
                            "step_index": step_index,
                            "partial_output_discarded": true,
                            "tool_envelope_exposed": false,
                        }),
                    ),
                )?;
                return Ok(self.finish(
                    task,
                    if emergency_paused {
                        "Command cancelled because emergency pause was activated."
                    } else {
                        "Command cancelled."
                    },
                    route,
                    route_evidence,
                    steps,
                    audit_entries,
                ));
            };
            let model_response = match model_result {
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
            for chunk_entry in
                model_output_chunk_audit_entries(task.id, step_index, &model_response)
            {
                self.record_audit(&mut audit_entries, chunk_entry)?;
            }

            if !model_response.tool_requests.is_empty() {
                self.record_audit(
                    &mut audit_entries,
                    tool_plan_audit_entry(task.id, step_index, &model_response.tool_requests),
                )?;
            }

            let step_tool_results = match self.execute_tool_plan(
                &mut task,
                ToolPlanContext {
                    sensitivity: request.sensitivity,
                    step_index,
                    tool_requests: &model_response.tool_requests,
                    selected_provider,
                    dry_run: request.dry_run,
                    proactive: request.proactive,
                    advertised_installed_wasm: &advertised_installed_wasm,
                    expected_workspace_request: request.expected_workspace_request.as_ref(),
                    prior_tool_output_bytes: serde_json::to_vec(&tool_results)
                        .map(|encoded| encoded.len())
                        .unwrap_or(MAX_TASK_TOOL_OUTPUT_BYTES + 1),
                },
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
                ToolPlanOutcome::Cancelled(results, message) => {
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
        context: ToolPlanContext<'_>,
        audit_entries: &mut Vec<AuditEntry>,
    ) -> JarvisResult<ToolPlanOutcome> {
        let ToolPlanContext {
            sensitivity,
            step_index,
            tool_requests,
            selected_provider,
            dry_run,
            proactive,
            advertised_installed_wasm,
            expected_workspace_request,
            prior_tool_output_bytes,
        } = context;
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
        let mut tool_output_bytes = prior_tool_output_bytes;
        for (tool_index, tool_request) in tool_requests.iter().enumerate() {
            let manifest = match self.validate_tool_request(
                task.id,
                step_index,
                tool_request,
                selected_provider,
                proactive,
                advertised_installed_wasm,
            ) {
                Ok(manifest) => manifest,
                Err(error) => {
                    let registered_tools = self.registered_model_tool_names_for_provider(
                        selected_provider,
                        advertised_installed_wasm,
                    );
                    let guidance = tool_rejection_message(&error, &registered_tools);
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
                                "error": if tool_request.plugin_id == "workspace_inspect" {
                                    "workspace request rejected".to_string()
                                } else {
                                    error.to_string()
                                },
                                "audit_summary": if tool_request.plugin_id == "workspace_inspect" {
                                    crate::workspace::audit_request_summary(&tool_request.input)
                                } else {
                                    serde_json::Value::Null
                                },
                                "registered_tools": registered_tools,
                            }),
                        ),
                    )?;
                    results.push(model_tool_rejection_result(
                        tool_request,
                        &error,
                        &registered_tools,
                        &guidance,
                    ));
                    continue;
                }
            };
            if tool_request.plugin_id == "workspace_inspect" {
                if let Some(expected) = expected_workspace_request {
                    let actual = crate::workspace::audit_request_summary(&tool_request.input);
                    if actual["root_id"] != expected["root_id"]
                        || actual["relative_path"] != expected["relative_path"]
                    {
                        let error = crate::JarvisError::PolicyBlocked(
                            "workspace tool request does not match the explicit command authority"
                                .to_string(),
                        );
                        let registered_tools = self.registered_model_tool_names_for_provider(
                            selected_provider,
                            advertised_installed_wasm,
                        );
                        let guidance = tool_rejection_message(&error, &registered_tools);
                        self.record_audit(
                            audit_entries,
                            AuditEntry::new(
                                Some(task.id),
                                "tool_request_rejected",
                                "model-planned workspace request differed from explicit authority",
                                json!({
                                    "step_index": step_index,
                                    "tool_index": tool_index,
                                    "plugin_id": tool_request.plugin_id,
                                    "action": tool_request.action,
                                    "error": "workspace request authority mismatch",
                                    "audit_summary": actual,
                                    "expected_request_redacted": true,
                                }),
                            ),
                        )?;
                        results.push(model_tool_rejection_result(
                            tool_request,
                            &error,
                            &registered_tools,
                            &guidance,
                        ));
                        continue;
                    }
                }
            }
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

            if dry_run {
                let result = ModelToolResult {
                    plugin_id: tool_request.plugin_id.clone(),
                    action: tool_request.action.clone(),
                    status: "dry_run".to_string(),
                    output: json!({"dry_run": true, "side_effect_executed": false}),
                };
                self.record_audit(
                    audit_entries,
                    AuditEntry::new(
                        Some(task.id),
                        "tool_dry_run",
                        "dry run skipped model-planned capability execution",
                        json!({
                            "step_index": step_index,
                            "tool_index": tool_index,
                            "plugin_id": tool_request.plugin_id,
                            "action": tool_request.action,
                            "side_effect_executed": false,
                            "audit_summary": if tool_request.plugin_id == "workspace_inspect" {
                                crate::workspace::audit_request_summary(&tool_request.input)
                            } else {
                                serde_json::Value::Null
                            },
                        }),
                    ),
                )?;
                results.push(result);
                continue;
            }

            let execute_result = if manifest.source == PluginSource::LocalWasm {
                let policy = crate::PermissionEngine::evaluate(&crate::PolicyRequest {
                    task_id: Some(task.id),
                    action: format!("{}.{}", manifest.id, action.name),
                    requested_scopes: granted_scopes.clone(),
                    granted_scopes: granted_scopes.clone(),
                    risk_tier: action.risk_tier,
                    sensitivity,
                    emergency_paused: self.control.is_emergency_paused(),
                    approval: None,
                });
                match policy.decision {
                    crate::ApprovalDecision::Blocked => {
                        Err(crate::JarvisError::PolicyBlocked(policy.reason))
                    }
                    crate::ApprovalDecision::RequireConfirmation => Ok(PluginCallResult {
                        status: PluginCallStatus::ApprovalRequired,
                        output: json!({ "approval_required": true }),
                        metadata: crate::PluginCallMetadata {
                            plugin_id: manifest.id.clone(),
                            action: action.name.clone(),
                            permissions: action.permissions.clone(),
                            risk_tier: action.risk_tier,
                            approval_required: true,
                            approval_status: policy.approval_status,
                            proactive,
                            memory_access: action.memory_access,
                            model_access: action.model_access,
                            timeout_ms: action.timeout.timeout_ms,
                            cancellation: action.cancellation,
                            audit_fields: action.audit_fields.clone(),
                            audit_summary: json!({
                                "runtime_kind": "wasm",
                                "execution_started": false,
                                "input_output_content_redacted": true,
                            }),
                        },
                    }),
                    crate::ApprovalDecision::AllowSilently
                    | crate::ApprovalDecision::AllowWithNotification => self
                        .command_store
                        .execute_model_planned_wasm(task.id, task.session_id, tool_request),
                }
            } else {
                self.plugin_host.execute_cancellable(
                    PluginCallRequest::reactive(
                        tool_request.plugin_id.clone(),
                        tool_request.action.clone(),
                        tool_request.input.clone(),
                    )
                    .with_granted_scopes(granted_scopes)
                    .with_sensitivity(sensitivity)
                    .with_proactive(proactive),
                    || {
                        self.control.is_emergency_paused()
                            || self.control.is_task_cancelled(task.id)
                    },
                )
            };
            let call_result = match execute_result {
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
                                "error": if tool_request.plugin_id == "workspace_inspect" {
                                    "workspace execution blocked".to_string()
                                } else {
                                    error.to_string()
                                },
                                "audit_summary": if tool_request.plugin_id == "workspace_inspect" {
                                    crate::workspace::audit_request_summary(&tool_request.input)
                                } else {
                                    serde_json::Value::Null
                                },
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
            let result_bytes = serde_json::to_vec(&result)
                .map(|encoded| encoded.len())
                .unwrap_or(MAX_TASK_TOOL_OUTPUT_BYTES + 1);
            tool_output_bytes = tool_output_bytes.saturating_add(result_bytes);
            if tool_output_bytes > MAX_TASK_TOOL_OUTPUT_BYTES {
                self.update_task_status(task, TaskStatus::Failed)?;
                self.record_audit(
                    audit_entries,
                    AuditEntry::new(
                        Some(task.id),
                        "tool_output_budget_exceeded",
                        "model-planned tool output exceeded the per-task continuation budget",
                        json!({
                            "step_index": step_index,
                            "tool_index": tool_index,
                            "plugin_id": call_result.metadata.plugin_id,
                            "action": call_result.metadata.action,
                            "max_task_tool_output_bytes": MAX_TASK_TOOL_OUTPUT_BYTES,
                            "output_exposed_to_model": false,
                            "audit_summary": call_result.metadata.audit_summary,
                        }),
                    ),
                )?;
                return Ok(ToolPlanOutcome::Blocked(
                    results,
                    "Command failed because tool output exceeded the bounded continuation budget."
                        .to_string(),
                ));
            }
            self.record_audit(
                audit_entries,
                tool_result_audit_entry(task.id, step_index, tool_index, &call_result),
            )?;
            results.push(result);

            if call_result.status == PluginCallStatus::Cancelled {
                let emergency_paused = self.control.is_emergency_paused();
                if !emergency_paused {
                    self.control.clear_task_cancellation(task.id);
                }
                self.update_task_status(task, TaskStatus::Cancelled)?;
                self.record_audit(
                    audit_entries,
                    AuditEntry::new(
                        Some(task.id),
                        if emergency_paused {
                            "emergency_pause_cancelled"
                        } else {
                            "task_cancelled"
                        },
                        "in-process capability was cancelled before output exposure",
                        json!({
                            "step_index": step_index,
                            "tool_index": tool_index,
                            "plugin_id": tool_request.plugin_id,
                            "action": tool_request.action,
                            "partial_output_discarded": true,
                        }),
                    ),
                )?;
                return Ok(ToolPlanOutcome::Cancelled(
                    results,
                    if emergency_paused {
                        "Command cancelled because emergency pause was activated.".to_string()
                    } else {
                        "Command cancelled.".to_string()
                    },
                ));
            }

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
        selected_provider: Option<crate::router::ModelProvider>,
        proactive: bool,
        advertised_installed_wasm: &[PluginManifest],
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

        let manifest = match self.plugin_host.manifest(&request.plugin_id) {
            Ok(manifest) => manifest,
            Err(first_party_error) => {
                if proactive || selected_provider != Some(crate::router::ModelProvider::Local) {
                    return Err(first_party_error);
                }
                advertised_installed_wasm
                    .iter()
                    .find(|manifest| manifest.id == request.plugin_id)
                    .cloned()
                    .ok_or(first_party_error)?
            }
        };
        if request.plugin_id == "workspace_inspect"
            && selected_provider == Some(crate::router::ModelProvider::ChatGpt)
        {
            return Err(crate::JarvisError::PolicyBlocked(
                "workspace inspection is restricted to local-model routes".to_string(),
            ));
        }
        if manifest.source != PluginSource::FirstParty && manifest.source != PluginSource::LocalWasm
        {
            return Err(crate::JarvisError::PolicyBlocked(
                "runtime only executes first-party or explicitly opted-in confined WASM model tools"
                    .to_string(),
            ));
        }
        if manifest.source == PluginSource::LocalWasm
            && (proactive || selected_provider != Some(crate::router::ModelProvider::Local))
        {
            return Err(crate::JarvisError::PolicyBlocked(
                "installed WASM model tools require explicit opt-in on a reactive local-model route"
                    .to_string(),
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

    fn registered_model_tool_names_for_provider(
        &self,
        selected_provider: Option<crate::router::ModelProvider>,
        advertised_installed_wasm: &[PluginManifest],
    ) -> Vec<String> {
        self.registered_model_tools_from_manifests(selected_provider, advertised_installed_wasm)
            .into_iter()
            .map(|tool| format!("{}.{}", tool.plugin_id, tool.action))
            .collect()
    }

    #[cfg(test)]
    fn registered_first_party_model_tools_for_provider(
        &self,
        selected_provider: Option<crate::router::ModelProvider>,
        installed_wasm_tools: bool,
        proactive: bool,
    ) -> Vec<crate::model::ModelToolDefinition> {
        let installed_wasm = self.registered_installed_wasm_manifests_for_provider(
            selected_provider,
            installed_wasm_tools,
            proactive,
        );
        self.registered_model_tools_from_manifests(selected_provider, &installed_wasm)
    }

    fn registered_installed_wasm_manifests_for_provider(
        &self,
        selected_provider: Option<crate::router::ModelProvider>,
        installed_wasm_tools: bool,
        proactive: bool,
    ) -> Vec<PluginManifest> {
        if !installed_wasm_tools
            || proactive
            || selected_provider != Some(crate::router::ModelProvider::Local)
        {
            return Vec::new();
        }
        let first_party_ids = self
            .plugin_host
            .manifests()
            .unwrap_or_default()
            .into_iter()
            .map(|manifest| manifest.id)
            .collect::<HashSet<_>>();
        let manifests = self
            .command_store
            .model_planned_wasm_manifests()
            .unwrap_or_default()
            .into_iter()
            .filter(|manifest| !first_party_ids.contains(&manifest.id))
            .collect();
        bounded_model_planned_wasm_manifests(manifests)
    }

    fn registered_model_tools_from_manifests(
        &self,
        selected_provider: Option<crate::router::ModelProvider>,
        advertised_installed_wasm: &[PluginManifest],
    ) -> Vec<crate::model::ModelToolDefinition> {
        let mut tools = model_tool_definitions_from_manifests(
            self.plugin_host
                .manifests()
                .unwrap_or_default()
                .into_iter()
                .filter(|manifest| manifest.source == PluginSource::FirstParty)
                .filter(|manifest| {
                    manifest.id != "workspace_inspect"
                        || selected_provider != Some(crate::router::ModelProvider::ChatGpt)
                }),
        );
        if !advertised_installed_wasm.is_empty() {
            let first_party_ids = tools
                .iter()
                .map(|tool| tool.plugin_id.clone())
                .collect::<HashSet<_>>();
            for manifest in advertised_installed_wasm {
                if first_party_ids.contains(&manifest.id) {
                    continue;
                }
                for action in &manifest.actions {
                    tools.push(crate::model::ModelToolDefinition {
                        plugin_id: manifest.id.clone(),
                        action: action.name.clone(),
                        description: action.description.clone(),
                        input_schema: action.input_schema.schema.clone(),
                    });
                }
            }
        }
        tools.sort_by(|left, right| {
            left.plugin_id
                .cmp(&right.plugin_id)
                .then_with(|| left.action.cmp(&right.action))
        });
        tools.dedup_by(|left, right| {
            left.plugin_id == right.plugin_id && left.action == right.action
        });
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

fn bounded_model_planned_wasm_manifests(mut manifests: Vec<PluginManifest>) -> Vec<PluginManifest> {
    manifests.sort_by(|left, right| left.id.cmp(&right.id));
    let mut tool_count = 0_usize;
    let mut catalog = Vec::new();
    let mut bounded = Vec::new();

    for mut manifest in manifests {
        manifest
            .actions
            .sort_by(|left, right| left.name.cmp(&right.name));
        manifest.actions.retain(|action| {
            if tool_count >= MAX_MODEL_PLANNED_WASM_TOOLS
                || action.description.len() > MAX_MODEL_PLANNED_WASM_DESCRIPTION_BYTES
            {
                return false;
            }
            let Ok(schema) = serde_json::to_vec(&action.input_schema.schema) else {
                return false;
            };
            if schema.len() > MAX_MODEL_PLANNED_WASM_SCHEMA_BYTES {
                return false;
            }
            catalog.push(crate::model::ModelToolDefinition {
                plugin_id: manifest.id.clone(),
                action: action.name.clone(),
                description: action.description.clone(),
                input_schema: action.input_schema.schema.clone(),
            });
            let within_catalog_budget = serde_json::to_vec(&catalog)
                .is_ok_and(|encoded| encoded.len() <= MAX_MODEL_PLANNED_WASM_CATALOG_BYTES);
            if !within_catalog_budget {
                catalog.pop();
                return false;
            }
            tool_count += 1;
            true
        });
        if !manifest.actions.is_empty() {
            bounded.push(manifest);
        }
        if tool_count >= MAX_MODEL_PLANNED_WASM_TOOLS {
            break;
        }
    }
    bounded
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

fn model_output_chunk_audit_entries(
    task_id: Uuid,
    step_index: u32,
    response: &ModelResponse,
) -> Vec<AuditEntry> {
    response
        .output_chunks
        .iter()
        .map(|chunk| {
            AuditEntry::new(
                Some(task_id),
                "model_output_chunk",
                "model output chunk metadata recorded",
                json!({
                    "step_index": step_index,
                    "provider": response.route.provider,
                    "model": response.route.model,
                    "sequence": chunk.sequence,
                    "byte_count": chunk.byte_count,
                    "char_count": chunk.char_count,
                    "final_chunk": chunk.final_chunk,
                    "provider_native": chunk.provider_native,
                    "content_redacted": true,
                }),
            )
        })
        .collect()
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
            "audit_summary": result.metadata.audit_summary,
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

fn model_tool_rejection_result(
    request: &ModelToolRequest,
    error: &crate::JarvisError,
    registered_tools: &[String],
    guidance: &str,
) -> ModelToolResult {
    ModelToolResult {
        plugin_id: request.plugin_id.clone(),
        action: request.action.clone(),
        status: "rejected".to_string(),
        output: json!({
            "error": error.to_string(),
            "registered_tools": registered_tools,
            "guidance": guidance,
        }),
    }
}

fn route_from_evidence(record: &ModelRouteRecord) -> Option<ModelRoute> {
    match record.selected_provider {
        Some(crate::router::ModelProvider::Local) => Some(ModelRoute::local(
            record.evidence.local_model.clone(),
            record.reason.clone(),
        )),
        Some(crate::router::ModelProvider::ChatGpt) => {
            Some(ModelRoute::chatgpt("codex", record.reason.clone()))
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
        crate::JarvisError::Conflict(_) => "conflict",
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

struct ToolPlanContext<'a> {
    sensitivity: Sensitivity,
    step_index: u32,
    tool_requests: &'a [ModelToolRequest],
    selected_provider: Option<crate::router::ModelProvider>,
    dry_run: bool,
    proactive: bool,
    advertised_installed_wasm: &'a [PluginManifest],
    expected_workspace_request: Option<&'a serde_json::Value>,
    prior_tool_output_bytes: usize,
}

enum ToolPlanOutcome {
    Completed(Vec<ModelToolResult>),
    WaitingForApproval(Vec<ModelToolResult>, String),
    Blocked(Vec<ModelToolResult>, String),
    Cancelled(Vec<ModelToolResult>, String),
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
    use crate::plugin::{
        CancellationBehavior, CancellationSignal, InProcessPlugin, JsonSchema, PluginAccess,
        PluginActionManifest, PluginManifest, PluginPermission, PluginTimeout,
    };
    use crate::router::{ModelProvider as RoutedModelProvider, RouteOutcome};
    use crate::storage::NewMemoryItem;
    use crate::types::TaskStatus;
    use axum::routing::post;
    use axum::{Json, Router};
    use serde_json::json;
    use tokio::net::TcpListener;

    #[test]
    fn runtime_cancellation_finalize_is_the_acceptance_linearization_point() {
        let control = RuntimeControl::default();
        let cancellation_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let mut guard = control
            .register_runtime_cancellation(cancellation_id)
            .expect("register active runtime");
        assert!(!control.cancel_runtime_execution(cancellation_id));
        guard.activate();
        assert!(control
            .register_runtime_cancellation(cancellation_id)
            .is_none());
        assert!(control.bind_runtime_cancellation_to_task(cancellation_id, task_id));
        assert!(control.cancel_runtime_execution(cancellation_id));
        assert!(control.is_task_cancelled(task_id));
        assert!(guard.finalize());
        assert!(!control.cancel_runtime_execution(cancellation_id));
        assert!(!control.is_runtime_cancelled(cancellation_id));
        assert!(!control.is_task_cancelled(task_id));
        assert!(control
            .register_runtime_cancellation(cancellation_id)
            .is_none());
    }

    #[test]
    fn consumed_runtime_cancellation_tombstones_are_bounded_fifo() {
        let control = RuntimeControl::default();
        let mut ids = Vec::new();
        for _ in 0..=MAX_CONSUMED_RUNTIME_CANCELLATIONS {
            let id = Uuid::new_v4();
            ids.push(id);
            drop(
                control
                    .register_runtime_cancellation(id)
                    .expect("register unique cancellation handle"),
            );
        }

        assert!(control
            .register_runtime_cancellation(*ids.last().expect("latest handle"))
            .is_none());
        assert!(control.register_runtime_cancellation(ids[0]).is_some());
    }

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
                "model_output_chunk",
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
            6
        );
    }

    #[tokio::test]
    async fn explicit_local_memory_context_injects_only_reviewed_eligible_records() {
        let dir = tempfile::tempdir().expect("temp dir");
        let db_path = dir.path().join("jarvis.sqlite");
        let repo = SqliteRepository::open(&db_path).expect("sqlite repository");
        let eligible = repo
            .create_memory_item(NewMemoryItem {
                category: "workflow".to_string(),
                key: "release-alpha".to_string(),
                value: "run the alpha release gate before delivery".to_string(),
                provenance: "operator reviewed note".to_string(),
                sensitivity: Sensitivity::Workspace,
            })
            .expect("eligible memory");
        repo.mark_memory_reviewed(eligible.id)
            .expect("review eligible memory");
        repo.create_memory_item(NewMemoryItem {
            category: "workflow".to_string(),
            key: "release-alpha-draft".to_string(),
            value: "unreviewed alpha draft must not be injected".to_string(),
            provenance: "unreviewed note".to_string(),
            sensitivity: Sensitivity::Workspace,
        })
        .expect("unreviewed memory");
        let private = repo
            .create_memory_item(NewMemoryItem {
                category: "personal".to_string(),
                key: "release-alpha-secret".to_string(),
                value: "private alpha value must not be injected".to_string(),
                provenance: "private note".to_string(),
                sensitivity: Sensitivity::Private,
            })
            .expect("private memory");
        repo.mark_memory_reviewed(private.id)
            .expect("review private memory");
        repo.rebuild_memory_index().expect("current memory index");

        let captured_contexts = Arc::new(Mutex::new(Vec::new()));
        let runtime = ConversationRuntime::with_storage_parts(
            RuntimeConfig::default(),
            RuntimeControl::default(),
            MemoryCaptureModel {
                captured_contexts: captured_contexts.clone(),
            },
            NoopRuntimeHooks,
            &repo,
        );
        let response = runtime
            .execute_command(
                CommandRequest::new("prepare alpha release delivery")
                    .with_sensitivity(Sensitivity::Workspace)
                    .with_memory_context(true),
            )
            .await
            .expect("memory-backed command");

        assert_eq!(response.task.status, TaskStatus::Completed);
        let contexts = captured_contexts.lock().expect("captured contexts");
        let context = contexts[0].as_deref().expect("attached context");
        assert!(context.contains("run the alpha release gate before delivery"));
        assert!(!context.contains("unreviewed alpha draft"));
        assert!(!context.contains("private alpha value"));
        let audit = response
            .audit_entries
            .iter()
            .find(|entry| entry.event_type == "memory_context_checked")
            .expect("redacted memory audit");
        assert_eq!(audit.payload["matched_count"], 1);
        assert_eq!(audit.payload["values_redacted"], true);
        assert!(!audit.payload.to_string().contains("release gate"));
    }

    #[tokio::test]
    async fn stale_memory_index_blocks_before_model_execution_without_values() {
        let dir = tempfile::tempdir().expect("temp dir");
        let repo =
            SqliteRepository::open(dir.path().join("jarvis.sqlite")).expect("sqlite repository");
        let item = repo
            .create_memory_item(NewMemoryItem {
                category: "workflow".to_string(),
                key: "stale-memory".to_string(),
                value: "stale sentinel must remain private from audit".to_string(),
                provenance: "operator note".to_string(),
                sensitivity: Sensitivity::Workspace,
            })
            .expect("memory");
        repo.mark_memory_reviewed(item.id).expect("review memory");
        repo.rebuild_memory_index().expect("initial index");
        repo.update_memory_item(
            item.id,
            "changed stale sentinel must remain private from audit",
            "updated operator note",
            Sensitivity::Workspace,
        )
        .expect("stale canonical update");
        let executions = Arc::new(AtomicUsize::new(0));
        let runtime = ConversationRuntime::with_storage_parts(
            RuntimeConfig::default(),
            RuntimeControl::default(),
            CountingModel {
                executions: executions.clone(),
            },
            NoopRuntimeHooks,
            &repo,
        );
        let response = runtime
            .execute_command(
                CommandRequest::new("find stale memory")
                    .with_sensitivity(Sensitivity::Workspace)
                    .with_memory_context(true),
            )
            .await
            .expect("structured fail-closed response");

        assert_eq!(response.task.status, TaskStatus::Blocked);
        assert_eq!(executions.load(Ordering::SeqCst), 0);
        let audit_json = serde_json::to_string(&response.audit_entries).expect("audit json");
        assert!(audit_json.contains("memory_context_blocked"));
        assert!(!audit_json.contains("stale sentinel"));
    }

    #[tokio::test]
    async fn oversized_active_memory_corpus_blocks_before_model_execution() {
        let dir = tempfile::tempdir().expect("temp dir");
        let repo =
            SqliteRepository::open(dir.path().join("jarvis.sqlite")).expect("sqlite repository");
        let oversized = repo
            .create_memory_item(NewMemoryItem {
                category: "personal".to_string(),
                key: "oversized-private".to_string(),
                value: "x".repeat(crate::MAX_MEMORY_RETRIEVAL_CORPUS_BYTES + 1),
                provenance: "oversized fixture".to_string(),
                sensitivity: Sensitivity::Private,
            })
            .expect("oversized memory fixture");
        repo.mark_memory_reviewed(oversized.id)
            .expect("review oversized memory");
        repo.rebuild_memory_index().expect("index fixture");
        let executions = Arc::new(AtomicUsize::new(0));
        let runtime = ConversationRuntime::with_storage_parts(
            RuntimeConfig::default(),
            RuntimeControl::default(),
            CountingModel {
                executions: executions.clone(),
            },
            NoopRuntimeHooks,
            &repo,
        );
        let response = runtime
            .execute_command(CommandRequest::new("oversized memory").with_memory_context(true))
            .await
            .expect("bounded failure response");

        assert_eq!(response.task.status, TaskStatus::Blocked);
        assert_eq!(executions.load(Ordering::SeqCst), 0);
        assert!(response
            .audit_entries
            .iter()
            .any(|entry| entry.event_type == "memory_context_blocked"));
    }

    #[tokio::test]
    async fn model_request_advertises_registered_first_party_tools_only() {
        let captured_tools = Arc::new(Mutex::new(Vec::new()));
        let model = InventoryCaptureModel {
            captured_tools: captured_tools.clone(),
        };
        let mut plugin_host = PluginHost::new();
        plugin_host
            .register(InventoryPlugin {
                plugin_id: "runtime_status",
                source: PluginSource::FirstParty,
            })
            .expect("first-party plugin registers");
        plugin_host
            .register(InventoryPlugin {
                plugin_id: "local_dev_status",
                source: PluginSource::LocalDevelopment,
            })
            .expect("local development plugin registers");
        let runtime = ConversationRuntime::new(model).with_plugin_host(plugin_host);

        let response = runtime
            .execute_command(CommandRequest::new("check registered tools"))
            .await
            .expect("command should execute");

        assert_eq!(response.task.status, TaskStatus::Completed);
        let tools = captured_tools.lock().expect("captured tools").clone();
        assert_eq!(tools, vec![vec!["runtime_status.inspect".to_string()]]);
    }

    #[tokio::test]
    async fn installed_wasm_model_tools_require_opt_in_and_execute_only_on_local_route() {
        let executions = Arc::new(AtomicUsize::new(0));
        let store = ModelPlannedWasmStore {
            executions: executions.clone(),
        };
        let captured_tools = Arc::new(Mutex::new(Vec::new()));
        let runtime = ConversationRuntime::with_storage_parts(
            RuntimeConfig::new(3),
            RuntimeControl::default(),
            InstalledWasmToolModel {
                captured_tools: captured_tools.clone(),
            },
            NoopRuntimeHooks,
            store,
        );

        let default_response = runtime
            .execute_command(CommandRequest::new("do not expose installed tools"))
            .await
            .expect("default command");
        assert_eq!(default_response.task.status, TaskStatus::Completed);
        assert_eq!(executions.load(Ordering::SeqCst), 0);
        assert_eq!(
            default_response.audit_entries[0].payload["installed_wasm_tools_requested"],
            false
        );
        assert!(!captured_tools
            .lock()
            .expect("captured tools")
            .first()
            .expect("default inventory")
            .iter()
            .any(|tool| tool == "installed_compute.compute"));

        captured_tools.lock().expect("captured tools").clear();
        let opted_in = runtime
            .execute_command(
                CommandRequest::new("use confined compute").with_installed_wasm_tools(true),
            )
            .await
            .expect("opted-in command");
        assert_eq!(opted_in.task.status, TaskStatus::Completed);
        assert_eq!(executions.load(Ordering::SeqCst), 1);
        assert_eq!(
            opted_in.audit_entries[0].payload["installed_wasm_tools_requested"],
            true
        );
        assert!(captured_tools
            .lock()
            .expect("captured tools")
            .iter()
            .flatten()
            .any(|tool| tool == "installed_compute.compute"));
        assert_eq!(opted_in.tool_results[0].output["computed"], true);

        let private_response = runtime
            .execute_command(
                CommandRequest::new("use confined compute on private context")
                    .with_sensitivity(Sensitivity::Private)
                    .with_installed_wasm_tools(true),
            )
            .await
            .expect("private installed tool request");
        assert_eq!(private_response.task.status, TaskStatus::WaitingForApproval);
        assert_eq!(executions.load(Ordering::SeqCst), 1);
        assert_eq!(private_response.tool_results.len(), 1);
        assert_eq!(private_response.tool_results[0].status, "approval_required");
        assert_eq!(
            private_response.tool_results[0].output,
            json!({ "approval_required": true })
        );
        assert!(private_response.audit_entries.iter().any(|entry| {
            entry.event_type == "tool_execution_result"
                && entry.payload["approval_required"] == true
                && entry.payload["approval_status"] == "pending"
        }));

        assert!(!runtime
            .registered_first_party_model_tools_for_provider(
                Some(crate::router::ModelProvider::ChatGpt),
                true,
                false,
            )
            .iter()
            .any(|tool| tool.plugin_id == "installed_compute"));
        assert!(!runtime
            .registered_first_party_model_tools_for_provider(
                Some(crate::router::ModelProvider::Local),
                true,
                true,
            )
            .iter()
            .any(|tool| tool.plugin_id == "installed_compute"));
    }

    #[test]
    fn installed_wasm_model_catalog_is_deterministically_bounded() {
        let mut manifest = model_planned_wasm_manifest();
        let template = manifest.actions.remove(0);

        let mut oversized_description = template.clone();
        oversized_description.name = "00_oversized_description".to_string();
        oversized_description.description =
            "x".repeat(MAX_MODEL_PLANNED_WASM_DESCRIPTION_BYTES + 1);
        manifest.actions.push(oversized_description);

        let mut oversized_schema = template.clone();
        oversized_schema.name = "01_oversized_schema".to_string();
        oversized_schema.input_schema = JsonSchema::new(json!({
            "type": "object",
            "description": "x".repeat(MAX_MODEL_PLANNED_WASM_SCHEMA_BYTES + 1)
        }));
        manifest.actions.push(oversized_schema);

        for index in (0..MAX_MODEL_PLANNED_WASM_TOOLS + 2).rev() {
            let mut action = template.clone();
            action.name = format!("tool_{index:02}");
            manifest.actions.push(action);
        }

        let bounded = bounded_model_planned_wasm_manifests(vec![manifest]);
        let actions = bounded
            .iter()
            .flat_map(|manifest| {
                manifest
                    .actions
                    .iter()
                    .map(move |action| (manifest, action))
            })
            .collect::<Vec<_>>();

        assert_eq!(actions.len(), MAX_MODEL_PLANNED_WASM_TOOLS);
        assert!(actions
            .iter()
            .all(|(_, action)| action.name.starts_with("tool_")));
        assert!(actions.windows(2).all(|pair| {
            (pair[0].0.id.as_str(), pair[0].1.name.as_str())
                < (pair[1].0.id.as_str(), pair[1].1.name.as_str())
        }));
        let catalog = actions
            .iter()
            .map(|(manifest, action)| crate::model::ModelToolDefinition {
                plugin_id: manifest.id.clone(),
                action: action.name.clone(),
                description: action.description.clone(),
                input_schema: action.input_schema.schema.clone(),
            })
            .collect::<Vec<_>>();
        assert!(
            serde_json::to_vec(&catalog)
                .expect("catalog serializes")
                .len()
                <= MAX_MODEL_PLANNED_WASM_CATALOG_BYTES
        );

        let mut escaped_manifest = model_planned_wasm_manifest();
        let escaped_template = escaped_manifest.actions.remove(0);
        for index in 0..MAX_MODEL_PLANNED_WASM_TOOLS {
            let mut action = escaped_template.clone();
            action.name = format!("escaped_{index:02}");
            action.description = "\\".repeat(MAX_MODEL_PLANNED_WASM_DESCRIPTION_BYTES);
            action.input_schema = JsonSchema::new(json!({
                "type": "object",
                "description": "\\".repeat(2_500)
            }));
            escaped_manifest.actions.push(action);
        }
        let escaped_bounded = bounded_model_planned_wasm_manifests(vec![escaped_manifest]);
        let escaped_catalog = model_tool_definitions_from_manifests(escaped_bounded);
        assert!(escaped_catalog.len() < MAX_MODEL_PLANNED_WASM_TOOLS);
        assert!(
            serde_json::to_vec(&escaped_catalog)
                .expect("escaped catalog serializes")
                .len()
                <= MAX_MODEL_PLANNED_WASM_CATALOG_BYTES
        );
    }

    #[test]
    fn installed_wasm_identifier_collision_preserves_first_party_catalog() {
        let mut plugin_host = PluginHost::new();
        plugin_host
            .register(InventoryPlugin {
                plugin_id: "installed_compute",
                source: PluginSource::FirstParty,
            })
            .expect("colliding first-party plugin registers");
        let runtime = ConversationRuntime::with_storage_parts(
            RuntimeConfig::default(),
            RuntimeControl::default(),
            InventoryCaptureModel {
                captured_tools: Arc::new(Mutex::new(Vec::new())),
            },
            NoopRuntimeHooks,
            ModelPlannedWasmStore {
                executions: Arc::new(AtomicUsize::new(0)),
            },
        )
        .with_plugin_host(plugin_host);

        let tools = runtime.registered_first_party_model_tools_for_provider(
            Some(crate::router::ModelProvider::Local),
            true,
            false,
        );

        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].plugin_id, "installed_compute");
        assert_eq!(tools[0].action, "inspect");
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
                    .with_sensitivity(Sensitivity::Restricted)
                    .with_cloud_route_approval(true),
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
    async fn personal_cloud_route_executes_only_after_one_shot_command_approval() {
        let executions = Arc::new(AtomicUsize::new(0));
        let runtime = ConversationRuntime::with_parts(
            RuntimeConfig::default().with_provider_config(
                ProviderConfig::local_only()
                    .without_local()
                    .with_chatgpt_enabled("chatgpt-approved-test"),
            ),
            RuntimeControl::default(),
            CountingModel {
                executions: Arc::clone(&executions),
            },
            NoopRuntimeHooks,
        );

        let waiting = runtime
            .execute_command(CommandRequest::new("personal cloud request"))
            .await
            .expect("unapproved cloud route should return structured response");
        assert_eq!(waiting.task.status, TaskStatus::WaitingForApproval);
        assert_eq!(executions.load(Ordering::SeqCst), 0);

        let approved = runtime
            .execute_command(
                CommandRequest::new("personal cloud request").with_cloud_route_approval(true),
            )
            .await
            .expect("approved cloud route should execute");
        assert_eq!(approved.task.status, TaskStatus::Completed);
        assert_eq!(executions.load(Ordering::SeqCst), 1);
        let route = approved.route_evidence.expect("approved route evidence");
        assert_eq!(route.outcome, RouteOutcome::Selected);
        assert_eq!(route.approval_status, crate::ApprovalStatus::Approved);
    }

    #[tokio::test]
    async fn proactive_cloud_route_ignores_command_approval() {
        let executions = Arc::new(AtomicUsize::new(0));
        let runtime = ConversationRuntime::with_parts(
            RuntimeConfig::default().with_provider_config(
                ProviderConfig::local_only()
                    .without_local()
                    .with_chatgpt_enabled("chatgpt-proactive-test"),
            ),
            RuntimeControl::default(),
            CountingModel {
                executions: Arc::clone(&executions),
            },
            NoopRuntimeHooks,
        );

        let response = runtime
            .execute_command(
                CommandRequest::new("proactive cloud request")
                    .with_proactive(true)
                    .with_cloud_route_approval(true),
            )
            .await
            .expect("proactive approval should fail closed");

        assert_eq!(response.task.status, TaskStatus::WaitingForApproval);
        assert_eq!(executions.load(Ordering::SeqCst), 0);
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
                auth_mode: crate::ChatGptAuthMode::ApiKey,
                model: "gpt-test".to_string(),
                base_url: format!("http://{address}/v1"),
                api_key: Some("test-token-value".to_string()),
                codex_executable: "codex".to_string(),
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
        ))
        .with_plugin_host(PluginHost::with_test_fixtures().expect("test fixtures"));

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
        let runtime = ConversationRuntime::new(ToolAwareModel)
            .with_plugin_host(PluginHost::with_test_fixtures().expect("test fixtures"));

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
        let runtime = ConversationRuntime::new(ProviderEnvelopeToolModel)
            .with_plugin_host(PluginHost::with_test_fixtures().expect("test fixtures"));

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
        ))
        .with_plugin_host(PluginHost::with_test_fixtures().expect("test fixtures"));

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
        ))
        .with_plugin_host(PluginHost::with_test_fixtures().expect("test fixtures"));

        let response = runtime
            .execute_command(CommandRequest::new("use malformed tool"))
            .await
            .expect("validation failure should return structured response");

        assert_eq!(response.task.status, TaskStatus::Completed);
        assert_eq!(response.tool_results.len(), 1);
        assert_eq!(response.tool_results[0].status, "rejected");
        assert!(response.tool_results[0].output["guidance"]
            .as_str()
            .expect("guidance")
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
        )
        .with_plugin_host(PluginHost::with_test_fixtures().expect("test fixtures"));

        let response = runtime
            .execute_command(CommandRequest::new("use invented status tool"))
            .await
            .expect("validation failure should return structured response");

        assert_eq!(response.task.status, TaskStatus::Completed);
        assert_eq!(response.tool_results.len(), 1);
        assert_eq!(response.tool_results[0].plugin_id, "status");
        assert_eq!(response.tool_results[0].action, "status");
        assert_eq!(response.tool_results[0].status, "rejected");
        assert!(response.tool_results[0].output["error"]
            .as_str()
            .expect("error")
            .contains("plugin error: plugin status is not registered"));
        assert!(response.tool_results[0].output["guidance"]
            .as_str()
            .expect("guidance")
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
        )
        .with_plugin_host(PluginHost::with_test_fixtures().expect("test fixtures"));

        let response = runtime
            .execute_command(CommandRequest::new("use invented status action"))
            .await
            .expect("validation failure should return structured response");

        assert_eq!(response.task.status, TaskStatus::Completed);
        assert_eq!(response.tool_results.len(), 1);
        assert_eq!(response.tool_results[0].plugin_id, "fake_status");
        assert_eq!(response.tool_results[0].action, "list");
        assert_eq!(response.tool_results[0].status, "rejected");
        assert!(response.tool_results[0].output["error"]
            .as_str()
            .expect("error")
            .contains("plugin fake_status does not declare action list"));
        assert!(response.tool_results[0].output["guidance"]
            .as_str()
            .expect("guidance")
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

    struct SlowModel;

    #[async_trait::async_trait]
    impl ModelExecutor for SlowModel {
        async fn execute(&self, _request: ModelRequest) -> JarvisResult<ModelResponse> {
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            Ok(ModelResponse {
                route: ModelRoute::fake_local("slow model"),
                message: "partial secret output must never surface".to_string(),
                complete: true,
                output_chunks: crate::model::bounded_output_chunks("partial secret output"),
                tool_requests: vec![ModelToolRequest::new("fake_status", "status", json!({}))],
            })
        }
    }

    #[tokio::test]
    async fn emergency_pause_cancels_in_flight_model_transport_and_discards_partial_state() {
        let control = RuntimeControl::default();
        let runtime = ConversationRuntime::with_parts(
            RuntimeConfig::default(),
            control.clone(),
            SlowModel,
            NoopRuntimeHooks,
        );

        let (response, ()) = tokio::join!(
            runtime.execute_command(CommandRequest::new("cancel active transport")),
            async {
                tokio::time::sleep(std::time::Duration::from_millis(75)).await;
                control.emergency_pause();
            }
        );
        let response = response.expect("cancellation returns structured response");

        assert_eq!(response.task.status, TaskStatus::Cancelled);
        assert!(response.steps.is_empty());
        assert!(response.tool_results.is_empty());
        assert!(!response.message.contains("partial secret"));
        assert!(!response
            .audit_entries
            .iter()
            .any(|entry| entry.event_type == "model_output_chunk"));
        let cancellation = response.audit_entries.last().expect("cancellation audit");
        assert_eq!(cancellation.event_type, "emergency_pause_cancelled");
        assert_eq!(cancellation.payload["partial_output_discarded"], true);
        assert_eq!(cancellation.payload["tool_envelope_exposed"], false);
    }

    #[tokio::test]
    async fn explicit_command_handle_cancels_only_its_active_model_transport() {
        let control = RuntimeControl::default();
        let cancelled_id = Uuid::new_v4();
        let unrelated_id = Uuid::new_v4();
        let mut cancelled_guard = control
            .register_runtime_cancellation(cancelled_id)
            .expect("register command cancellation");
        cancelled_guard.activate();
        let mut unrelated_guard = control
            .register_runtime_cancellation(unrelated_id)
            .expect("register unrelated command cancellation");
        unrelated_guard.activate();
        let runtime = ConversationRuntime::with_parts(
            RuntimeConfig::default(),
            control.clone(),
            SlowModel,
            NoopRuntimeHooks,
        );

        let (response, cancellation_found) = tokio::join!(
            runtime.execute_command(
                CommandRequest::new("cancel this command").with_cancellation_id(Some(cancelled_id)),
            ),
            async {
                tokio::time::sleep(std::time::Duration::from_millis(75)).await;
                control.cancel_runtime_execution(cancelled_id)
            }
        );
        let response = response.expect("command cancellation returns structured response");

        assert!(cancellation_found);
        assert_eq!(response.task.status, TaskStatus::Cancelled);
        assert!(response.steps.is_empty());
        assert!(response.tool_results.is_empty());
        assert!(!response.message.contains("partial secret"));
        assert!(!control.is_runtime_cancelled(unrelated_id));
        assert!(cancelled_guard.finalize());
        assert!(!unrelated_guard.finalize());
    }

    struct CancelBeforeReturnModel {
        control: RuntimeControl,
    }

    #[async_trait::async_trait]
    impl ModelExecutor for CancelBeforeReturnModel {
        async fn execute(&self, request: ModelRequest) -> JarvisResult<ModelResponse> {
            self.control.cancel_task(request.task_id);
            Ok(ModelResponse {
                route: ModelRoute::fake_local("completion cancellation race"),
                message: "do not expose completion-race output".to_string(),
                complete: false,
                output_chunks: crate::model::bounded_output_chunks("hidden race output"),
                tool_requests: vec![ModelToolRequest::new("fake_status", "status", json!({}))],
            })
        }
    }

    #[tokio::test]
    async fn cancellation_dominates_a_model_completion_race_before_audit_or_tools() {
        let control = RuntimeControl::default();
        let runtime = ConversationRuntime::with_parts(
            RuntimeConfig::default(),
            control.clone(),
            CancelBeforeReturnModel { control },
            NoopRuntimeHooks,
        );

        let response = runtime
            .execute_command(CommandRequest::new("race cancellation"))
            .await
            .expect("cancellation race returns structured response");

        assert_eq!(response.task.status, TaskStatus::Cancelled);
        assert!(response.steps.is_empty());
        assert!(response.tool_results.is_empty());
        assert!(!response.message.contains("completion-race"));
        assert!(!response.audit_entries.iter().any(|entry| matches!(
            entry.event_type.as_str(),
            "model_step_completed" | "model_output_chunk" | "tool_plan_received"
        )));
        assert_eq!(
            response
                .audit_entries
                .last()
                .expect("cancel audit")
                .event_type,
            "task_cancelled"
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

    struct MemoryCaptureModel {
        captured_contexts: Arc<Mutex<Vec<Option<String>>>>,
    }

    #[async_trait::async_trait]
    impl ModelExecutor for MemoryCaptureModel {
        async fn execute(&self, request: ModelRequest) -> JarvisResult<ModelResponse> {
            self.captured_contexts
                .lock()
                .expect("captured memory contexts")
                .push(request.memory_context);
            Ok(ModelResponse {
                route: ModelRoute::fake_local("memory capture model"),
                message: "captured memory context".to_string(),
                complete: true,
                output_chunks: Vec::new(),
                tool_requests: Vec::new(),
            })
        }
    }

    #[async_trait::async_trait]
    impl ModelExecutor for CountingModel {
        async fn execute(&self, request: ModelRequest) -> JarvisResult<ModelResponse> {
            self.executions.fetch_add(1, Ordering::SeqCst);
            Ok(ModelResponse {
                route: ModelRoute::fake_local("counting local model"),
                message: format!("counted: {}", request.user_input),
                complete: true,
                output_chunks: Vec::new(),
                tool_requests: Vec::new(),
            })
        }
    }

    struct InventoryCaptureModel {
        captured_tools: Arc<Mutex<Vec<Vec<String>>>>,
    }

    struct InstalledWasmToolModel {
        captured_tools: Arc<Mutex<Vec<Vec<String>>>>,
    }

    #[async_trait::async_trait]
    impl ModelExecutor for InstalledWasmToolModel {
        async fn execute(&self, request: ModelRequest) -> JarvisResult<ModelResponse> {
            self.captured_tools.lock().expect("captured tools").push(
                request
                    .first_party_tools
                    .iter()
                    .map(|tool| format!("{}.{}", tool.plugin_id, tool.action))
                    .collect(),
            );
            let tool_requests = if request.step_index == 0
                && request
                    .first_party_tools
                    .iter()
                    .any(|tool| tool.plugin_id == "installed_compute")
            {
                vec![ModelToolRequest::new(
                    "installed_compute",
                    "compute",
                    json!({}),
                )]
            } else {
                Vec::new()
            };
            Ok(ModelResponse {
                route: ModelRoute::fake_local("installed wasm tool model"),
                message: "installed WASM tool step".to_string(),
                complete: tool_requests.is_empty(),
                output_chunks: Vec::new(),
                tool_requests,
            })
        }
    }

    struct ModelPlannedWasmStore {
        executions: Arc<AtomicUsize>,
    }

    impl RuntimeCommandStore for ModelPlannedWasmStore {
        fn create_task(&self, session_id: Uuid, user_input: String) -> JarvisResult<TaskRecord> {
            NoopRuntimeCommandStore.create_task(session_id, user_input)
        }

        fn update_task_status(
            &self,
            task: &mut TaskRecord,
            status: TaskStatus,
        ) -> JarvisResult<()> {
            NoopRuntimeCommandStore.update_task_status(task, status)
        }

        fn append_audit_entry(&self, entry: &AuditEntry) -> JarvisResult<()> {
            NoopRuntimeCommandStore.append_audit_entry(entry)
        }

        fn append_model_route_record(&self, record: &ModelRouteRecord) -> JarvisResult<()> {
            NoopRuntimeCommandStore.append_model_route_record(record)
        }

        fn model_planned_wasm_manifests(&self) -> JarvisResult<Vec<PluginManifest>> {
            Ok(vec![model_planned_wasm_manifest()])
        }

        fn execute_model_planned_wasm(
            &self,
            _task_id: Uuid,
            _session_id: Uuid,
            request: &ModelToolRequest,
        ) -> JarvisResult<PluginCallResult> {
            assert_eq!(request.plugin_id, "installed_compute");
            assert_eq!(request.action, "compute");
            self.executions.fetch_add(1, Ordering::SeqCst);
            let action = model_planned_wasm_manifest().actions.remove(0);
            Ok(PluginCallResult {
                status: PluginCallStatus::Completed,
                output: json!({"computed": true}),
                metadata: crate::PluginCallMetadata {
                    plugin_id: request.plugin_id.clone(),
                    action: request.action.clone(),
                    permissions: Vec::new(),
                    risk_tier: crate::RiskTier::Low,
                    approval_required: false,
                    approval_status: crate::ApprovalStatus::NotRequired,
                    proactive: false,
                    memory_access: PluginAccess::None,
                    model_access: PluginAccess::None,
                    timeout_ms: action.timeout.timeout_ms,
                    cancellation: action.cancellation,
                    audit_fields: Vec::new(),
                    audit_summary: json!({
                        "runtime_kind": "wasm",
                        "input_output_content_redacted": true,
                    }),
                },
            })
        }
    }

    fn model_planned_wasm_manifest() -> PluginManifest {
        PluginManifest {
            manifest_schema_version: 1,
            id: "installed_compute".to_string(),
            name: "Installed Compute".to_string(),
            version: "1.0.0".to_string(),
            source: PluginSource::LocalWasm,
            author: "Jarvis Tests".to_string(),
            source_path: Some("/redacted".to_string()),
            subprocess: None,
            wasm: Some(crate::PluginWasmManifest {
                module: "plugin.wasm".to_string(),
                abi: crate::PluginWasmAbi::JarvisJsonV1,
            }),
            publisher_signature: None,
            actions: vec![PluginActionManifest {
                name: "compute".to_string(),
                description: "Run confined local computation.".to_string(),
                permissions: Vec::new(),
                risk_tier: crate::RiskTier::Low,
                input_schema: JsonSchema::empty_object(),
                output_schema: JsonSchema::new(json!({
                    "type": "object",
                    "properties": {"computed": {"type": "boolean"}},
                    "required": ["computed"],
                    "additionalProperties": false
                })),
                proactive: false,
                memory_access: PluginAccess::None,
                model_access: PluginAccess::None,
                network_access: Default::default(),
                audit_fields: Vec::new(),
                timeout: PluginTimeout::default_for_action(),
                cancellation: CancellationBehavior::Cooperative,
            }],
        }
    }

    #[async_trait::async_trait]
    impl ModelExecutor for InventoryCaptureModel {
        async fn execute(&self, request: ModelRequest) -> JarvisResult<ModelResponse> {
            let tool_names = request
                .first_party_tools
                .iter()
                .map(|tool| format!("{}.{}", tool.plugin_id, tool.action))
                .collect::<Vec<_>>();
            self.captured_tools
                .lock()
                .expect("captured tools")
                .push(tool_names);
            Ok(ModelResponse {
                route: ModelRoute::fake_local("inventory capture model"),
                message: "captured inventory".to_string(),
                complete: true,
                output_chunks: Vec::new(),
                tool_requests: Vec::new(),
            })
        }
    }

    struct InventoryPlugin {
        plugin_id: &'static str,
        source: PluginSource,
    }

    impl InProcessPlugin for InventoryPlugin {
        fn manifest(&self) -> PluginManifest {
            PluginManifest {
                manifest_schema_version: 1,
                id: self.plugin_id.to_string(),
                name: self.plugin_id.to_string(),
                version: "0.1.0".to_string(),
                source: self.source,
                author: "Jarvis Tests".to_string(),
                source_path: None,
                subprocess: None,
                wasm: None,
                publisher_signature: None,
                actions: vec![PluginActionManifest {
                    name: "inspect".to_string(),
                    description: "Inspect runtime state for inventory tests.".to_string(),
                    permissions: vec![PluginPermission::SystemStatus],
                    risk_tier: crate::RiskTier::Low,
                    input_schema: JsonSchema::empty_object(),
                    output_schema: JsonSchema::empty_object(),
                    proactive: false,
                    memory_access: PluginAccess::None,
                    model_access: PluginAccess::None,
                    network_access: Default::default(),
                    audit_fields: Vec::new(),
                    timeout: PluginTimeout::default_for_action(),
                    cancellation: CancellationBehavior::Cooperative,
                }],
            }
        }

        fn execute(
            &self,
            _action: &PluginActionManifest,
            _input: serde_json::Value,
            _cancellation: CancellationSignal,
        ) -> JarvisResult<serde_json::Value> {
            Ok(json!({}))
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
                    output_chunks: Vec::new(),
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
                output_chunks: Vec::new(),
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
                    output_chunks: Vec::new(),
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
                output_chunks: Vec::new(),
                tool_requests: Vec::new(),
            })
        }
    }
}
