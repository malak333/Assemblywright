use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::types::JarvisResult;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelProvider {
    Local,
    ChatGpt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRoute {
    pub provider: ModelProvider,
    pub model: String,
    pub reason: String,
}

impl ModelRoute {
    pub fn fake_local(reason: impl Into<String>) -> Self {
        Self {
            provider: ModelProvider::Local,
            model: "fake-local-model".to_string(),
            reason: reason.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRequest {
    pub task_id: uuid::Uuid,
    pub session_id: uuid::Uuid,
    pub user_input: String,
    pub step_index: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelResponse {
    pub route: ModelRoute,
    pub message: String,
    pub complete: bool,
}

#[async_trait]
pub trait ModelExecutor: Send + Sync {
    async fn execute(&self, request: ModelRequest) -> JarvisResult<ModelResponse>;
}

#[derive(Debug, Clone)]
pub struct FakeLocalModel {
    response_prefix: String,
    complete_after_steps: u32,
}

impl FakeLocalModel {
    pub fn new(response_prefix: impl Into<String>) -> Self {
        Self {
            response_prefix: response_prefix.into(),
            complete_after_steps: 1,
        }
    }

    pub fn complete_after_steps(mut self, steps: u32) -> Self {
        self.complete_after_steps = steps.max(1);
        self
    }
}

impl Default for FakeLocalModel {
    fn default() -> Self {
        Self::new("local response")
    }
}

#[async_trait]
impl ModelExecutor for FakeLocalModel {
    async fn execute(&self, request: ModelRequest) -> JarvisResult<ModelResponse> {
        let complete = request.step_index + 1 >= self.complete_after_steps;
        Ok(ModelResponse {
            route: ModelRoute::fake_local("local model is the default route for v1 commands"),
            message: format!("{}: {}", self.response_prefix, request.user_input),
            complete,
        })
    }
}
