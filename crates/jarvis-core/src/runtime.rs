use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::model::{ModelExecutor, ModelRequest, ModelResponse, ModelRoute, ProviderConfig};
use crate::router::{
    ModelProvider as RoutedModelProvider, ModelRouteRecord, ModelRouteRequest, ModelRouter,
    RouteOutcome,
};
use crate::storage::SqliteRepository;
use crate::types::{AuditEntry, JarvisResult, Sensitivity, TaskRecord, TaskStatus};
use crate::CapabilityScope;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub max_steps: u32,
    pub provider_config: ProviderConfig,
}

impl RuntimeConfig {
    pub fn new(max_steps: u32) -> Self {
        Self {
            max_steps,
            provider_config: ProviderConfig::local_only(),
        }
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResponse {
    pub task: TaskRecord,
    pub message: String,
    pub route: Option<ModelRoute>,
    pub route_evidence: Option<ModelRouteRecord>,
    pub steps: Vec<RuntimeStep>,
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
}

pub struct ConversationRuntime<M, H = NoopRuntimeHooks, S = NoopRuntimeCommandStore> {
    config: RuntimeConfig,
    control: RuntimeControl,
    model: M,
    hooks: H,
    command_store: S,
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
        }
    }

    pub fn control(&self) -> RuntimeControl {
        self.control.clone()
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

        let route_record = ModelRouter::route(&ModelRouteRequest {
            task_id: Some(task.id),
            user_intent: task.user_input.clone(),
            sensitivity: request.sensitivity,
            required_scopes: vec![CapabilityScope::Conversation, CapabilityScope::LocalModel],
            granted_scopes: vec![CapabilityScope::Conversation, CapabilityScope::LocalModel],
            local_available: self.config.provider_config.local.enabled,
            local_sufficient: self.config.provider_config.local.enabled,
            provider_status: crate::ProviderStatus::from_config(&self.config.provider_config),
            emergency_paused: self.control.is_emergency_paused(),
            approval: None,
            context_preview: task.user_input.clone(),
        });
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

        if route_record.outcome == RouteOutcome::Blocked
            || route_record.selected_provider != Some(RoutedModelProvider::Local)
        {
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
                    "provider": "local",
                }),
            ),
        )?;

        let mut route = None;
        let route_evidence = Some(route_record);
        let mut steps = Vec::new();

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
                .execute(ModelRequest {
                    task_id: task.id,
                    session_id: task.session_id,
                    user_input: task.user_input.clone(),
                    step_index,
                })
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
                                "error": error.to_string(),
                            }),
                        ),
                    )?;
                    return Err(error);
                }
            };
            route = Some(model_response.route.clone());
            self.record_audit(
                &mut audit_entries,
                model_audit_entry(task.id, step_index, &model_response),
            )?;

            let step = RuntimeStep {
                index: step_index,
                message: model_response.message.clone(),
                complete: model_response.complete,
            };
            self.hooks.model_step_completed(&task, &step);
            steps.push(step);

            if model_response.complete {
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

    fn finish(
        &self,
        task: TaskRecord,
        message: impl Into<String>,
        route: Option<ModelRoute>,
        route_evidence: Option<ModelRouteRecord>,
        steps: Vec<RuntimeStep>,
        audit_entries: Vec<AuditEntry>,
    ) -> CommandResponse {
        let response = CommandResponse {
            task,
            message: message.into(),
            route,
            route_evidence,
            steps,
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
        }),
    )
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

#[cfg(test)]
mod tests {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    };

    use super::*;
    use crate::model::{FakeLocalModel, ModelProvider, ProviderConfig};
    use crate::router::RouteOutcome;
    use crate::types::TaskStatus;

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
            })
        }
    }
}
