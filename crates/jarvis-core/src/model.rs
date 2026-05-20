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
pub struct LocalModelConfig {
    pub enabled: bool,
    pub model: String,
}

impl Default for LocalModelConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            model: "fake-local-model".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatGptProviderConfig {
    pub enabled: bool,
    pub model: String,
    pub requires_approval: bool,
}

impl Default for ChatGptProviderConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model: "chatgpt-disabled".to_string(),
            requires_approval: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ProviderConfig {
    pub local: LocalModelConfig,
    pub chatgpt: ChatGptProviderConfig,
}

impl ProviderConfig {
    pub fn local_only() -> Self {
        Self::default()
    }

    pub fn with_chatgpt_enabled(mut self, model: impl Into<String>) -> Self {
        self.chatgpt.enabled = true;
        self.chatgpt.model = model.into();
        self.chatgpt.requires_approval = true;
        self
    }

    pub fn without_local(mut self) -> Self {
        self.local.enabled = false;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderStatus {
    pub local_available: bool,
    pub local_model: String,
    pub chatgpt_enabled: bool,
    pub chatgpt_model: String,
    pub chatgpt_requires_approval: bool,
}

impl ProviderStatus {
    pub fn from_config(config: &ProviderConfig) -> Self {
        Self {
            local_available: config.local.enabled,
            local_model: config.local.model.clone(),
            chatgpt_enabled: config.chatgpt.enabled,
            chatgpt_model: config.chatgpt.model.clone(),
            chatgpt_requires_approval: config.chatgpt.requires_approval,
        }
    }
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
