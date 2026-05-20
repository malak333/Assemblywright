use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::model::{ModelExecutor, ModelRequest, ModelResponse, ModelRoute};
use crate::types::{AuditEntry, JarvisResult, Sensitivity, TaskRecord, TaskStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeConfig {
    pub max_steps: u32,
}

impl RuntimeConfig {
    pub const fn new(max_steps: u32) -> Self {
        Self { max_steps }
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self { max_steps: 8 }
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

pub struct ConversationRuntime<M, H = NoopRuntimeHooks> {
    config: RuntimeConfig,
    control: RuntimeControl,
    model: M,
    hooks: H,
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

impl<M, H> ConversationRuntime<M, H> {
    pub fn with_parts(config: RuntimeConfig, control: RuntimeControl, model: M, hooks: H) -> Self {
        Self {
            config,
            control,
            model,
            hooks,
        }
    }

    pub fn control(&self) -> RuntimeControl {
        self.control.clone()
    }
}

impl<M, H> ConversationRuntime<M, H>
where
    M: ModelExecutor,
    H: RuntimeHooks,
{
    pub async fn execute_command(&self, request: CommandRequest) -> JarvisResult<CommandResponse> {
        let mut task = TaskRecord {
            id: Uuid::new_v4(),
            session_id: request.session_id,
            user_input: request.input,
            status: TaskStatus::Created,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let mut audit_entries = vec![AuditEntry::new(
            Some(task.id),
            "task_created",
            "created command task",
            json!({
                "session_id": task.session_id,
                "sensitivity": request.sensitivity,
            }),
        )];
        self.hooks.task_created(&task);

        if task.user_input.trim().is_empty() {
            task.status = TaskStatus::Failed;
            touch(&mut task);
            audit_entries.push(AuditEntry::new(
                Some(task.id),
                "validation_failed",
                "command input is empty",
                json!({ "field": "input" }),
            ));
            return Ok(self.finish(
                task,
                "Command input is required.",
                None,
                vec![],
                audit_entries,
            ));
        }

        if self.control.is_emergency_paused() {
            task.status = TaskStatus::Blocked;
            touch(&mut task);
            audit_entries.push(AuditEntry::new(
                Some(task.id),
                "emergency_pause_blocked",
                "emergency pause blocked command execution",
                json!({ "emergency_paused": true }),
            ));
            return Ok(self.finish(
                task,
                "Emergency pause is active; command execution is blocked.",
                None,
                vec![],
                audit_entries,
            ));
        }

        task.status = TaskStatus::Running;
        touch(&mut task);
        audit_entries.push(AuditEntry::new(
            Some(task.id),
            "task_running",
            "command entered model execution",
            json!({ "max_steps": self.config.max_steps }),
        ));

        let mut route = None;
        let mut steps = Vec::new();

        for step_index in 0..self.config.max_steps {
            if self.control.is_emergency_paused() {
                task.status = TaskStatus::Cancelled;
                touch(&mut task);
                audit_entries.push(AuditEntry::new(
                    Some(task.id),
                    "emergency_pause_cancelled",
                    "emergency pause cancelled active command",
                    json!({ "step_index": step_index }),
                ));
                return Ok(self.finish(
                    task,
                    "Command cancelled because emergency pause was activated.",
                    route,
                    steps,
                    audit_entries,
                ));
            }

            if self.control.is_task_cancelled(task.id) {
                task.status = TaskStatus::Cancelled;
                touch(&mut task);
                self.control.clear_task_cancellation(task.id);
                audit_entries.push(AuditEntry::new(
                    Some(task.id),
                    "task_cancelled",
                    "command cancelled before model step",
                    json!({ "step_index": step_index }),
                ));
                return Ok(self.finish(task, "Command cancelled.", route, steps, audit_entries));
            }

            self.hooks.before_model_step(&task, step_index);
            if self.control.is_emergency_paused() {
                task.status = TaskStatus::Cancelled;
                touch(&mut task);
                audit_entries.push(AuditEntry::new(
                    Some(task.id),
                    "emergency_pause_cancelled",
                    "emergency pause cancelled active command from runtime hook",
                    json!({ "step_index": step_index }),
                ));
                return Ok(self.finish(
                    task,
                    "Command cancelled because emergency pause was activated.",
                    route,
                    steps,
                    audit_entries,
                ));
            }

            if self.control.is_task_cancelled(task.id) {
                task.status = TaskStatus::Cancelled;
                touch(&mut task);
                self.control.clear_task_cancellation(task.id);
                audit_entries.push(AuditEntry::new(
                    Some(task.id),
                    "task_cancelled",
                    "command cancelled by runtime hook",
                    json!({ "step_index": step_index }),
                ));
                return Ok(self.finish(task, "Command cancelled.", route, steps, audit_entries));
            }

            let model_response = self
                .model
                .execute(ModelRequest {
                    task_id: task.id,
                    session_id: task.session_id,
                    user_input: task.user_input.clone(),
                    step_index,
                })
                .await?;
            route = Some(model_response.route.clone());
            audit_entries.push(model_audit_entry(task.id, step_index, &model_response));

            let step = RuntimeStep {
                index: step_index,
                message: model_response.message.clone(),
                complete: model_response.complete,
            };
            self.hooks.model_step_completed(&task, &step);
            steps.push(step);

            if model_response.complete {
                task.status = TaskStatus::Completed;
                touch(&mut task);
                audit_entries.push(AuditEntry::new(
                    Some(task.id),
                    "task_completed",
                    "command completed",
                    json!({ "steps": steps.len() }),
                ));
                return Ok(self.finish(task, model_response.message, route, steps, audit_entries));
            }
        }

        task.status = TaskStatus::Failed;
        touch(&mut task);
        audit_entries.push(AuditEntry::new(
            Some(task.id),
            "step_limit_exceeded",
            "command exceeded configured step limit",
            json!({ "max_steps": self.config.max_steps }),
        ));
        Ok(self.finish(
            task,
            "Command failed because the runtime step limit was reached.",
            route,
            steps,
            audit_entries,
        ))
    }

    fn finish(
        &self,
        task: TaskRecord,
        message: impl Into<String>,
        route: Option<ModelRoute>,
        steps: Vec<RuntimeStep>,
        audit_entries: Vec<AuditEntry>,
    ) -> CommandResponse {
        let response = CommandResponse {
            task,
            message: message.into(),
            route,
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

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::model::{FakeLocalModel, ModelProvider};
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
        assert!(response
            .audit_entries
            .iter()
            .any(|entry| entry.event_type == "task_completed"));
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
}
