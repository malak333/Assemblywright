use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

use crate::types::{JarvisError, JarvisResult};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelProvider {
    Local,
    ChatGpt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalModelProviderKind {
    Fake,
    Ollama,
}

impl LocalModelProviderKind {
    fn parse(value: &str) -> JarvisResult<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "fake" | "fake_local" | "fake-local" => Ok(Self::Fake),
            "ollama" | "ollama_http" | "ollama-http" => Ok(Self::Ollama),
            other => Err(JarvisError::Validation(format!(
                "unsupported local model provider: {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalModelConfig {
    pub enabled: bool,
    pub provider: LocalModelProviderKind,
    pub model: String,
    pub base_url: Option<String>,
    pub timeout_ms: u64,
}

impl Default for LocalModelConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            provider: LocalModelProviderKind::Fake,
            model: "fake-local-model".to_string(),
            base_url: None,
            timeout_ms: 15_000,
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

    pub fn from_env() -> JarvisResult<Self> {
        Self::from_env_values(|key| std::env::var(key).ok())
    }

    pub fn from_env_values(get: impl Fn(&str) -> Option<String>) -> JarvisResult<Self> {
        let mut config = Self::default();

        if let Some(value) = get("JARVIS_LOCAL_MODEL_ENABLED") {
            config.local.enabled = parse_bool("JARVIS_LOCAL_MODEL_ENABLED", &value)?;
        }

        if let Some(value) = get("JARVIS_LOCAL_MODEL_PROVIDER") {
            config.local.provider = LocalModelProviderKind::parse(&value)?;
        }

        if let Some(value) = get("JARVIS_LOCAL_MODEL") {
            let value = value.trim();
            if value.is_empty() {
                return Err(JarvisError::Validation(
                    "JARVIS_LOCAL_MODEL cannot be empty".to_string(),
                ));
            }
            config.local.model = value.to_string();
        }

        if let Some(value) = get("JARVIS_OLLAMA_BASE_URL") {
            let value = value.trim();
            if !value.is_empty() {
                config.local.base_url = Some(value.trim_end_matches('/').to_string());
            }
        }

        if let Some(value) = get("JARVIS_LOCAL_MODEL_TIMEOUT_MS") {
            let parsed = value.parse::<u64>().map_err(|_| {
                JarvisError::Validation(
                    "JARVIS_LOCAL_MODEL_TIMEOUT_MS must be a positive integer".to_string(),
                )
            })?;
            if parsed == 0 {
                return Err(JarvisError::Validation(
                    "JARVIS_LOCAL_MODEL_TIMEOUT_MS must be greater than zero".to_string(),
                ));
            }
            config.local.timeout_ms = parsed;
        }

        if config.local.provider == LocalModelProviderKind::Ollama {
            if config.local.model == "fake-local-model" {
                config.local.model = "llama3.2".to_string();
            }
            config
                .local
                .base_url
                .get_or_insert_with(|| "http://127.0.0.1:11434".to_string());
        }

        Ok(config)
    }

    pub fn with_ollama_local(
        mut self,
        model: impl Into<String>,
        base_url: impl Into<String>,
    ) -> Self {
        self.local.enabled = true;
        self.local.provider = LocalModelProviderKind::Ollama;
        self.local.model = model.into();
        self.local.base_url = Some(base_url.into().trim_end_matches('/').to_string());
        self
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
    pub local_provider: LocalModelProviderKind,
    pub local_model: String,
    pub local_endpoint_configured: bool,
    pub chatgpt_enabled: bool,
    pub chatgpt_model: String,
    pub chatgpt_requires_approval: bool,
}

impl ProviderStatus {
    pub fn from_config(config: &ProviderConfig) -> Self {
        Self {
            local_available: config.local.enabled,
            local_provider: config.local.provider,
            local_model: config.local.model.clone(),
            local_endpoint_configured: config.local.base_url.is_some(),
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
    pub fn local(model: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            provider: ModelProvider::Local,
            model: model.into(),
            reason: reason.into(),
        }
    }

    pub fn fake_local(reason: impl Into<String>) -> Self {
        Self::local("fake-local-model", reason)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRequest {
    pub task_id: uuid::Uuid,
    pub session_id: uuid::Uuid,
    pub user_input: String,
    pub step_index: u32,
    pub tool_results: Vec<ModelToolResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelResponse {
    pub route: ModelRoute,
    pub message: String,
    pub complete: bool,
    #[serde(default)]
    pub tool_requests: Vec<ModelToolRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelToolRequest {
    pub plugin_id: String,
    pub action: String,
    pub input: Value,
}

impl ModelToolRequest {
    pub fn new(plugin_id: impl Into<String>, action: impl Into<String>, input: Value) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            action: action.into(),
            input,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelToolResult {
    pub plugin_id: String,
    pub action: String,
    pub status: String,
    pub output: Value,
}

#[async_trait]
pub trait ModelExecutor: Send + Sync {
    async fn execute(&self, request: ModelRequest) -> JarvisResult<ModelResponse>;
}

#[derive(Debug, Clone)]
pub enum LocalModelExecutor {
    Fake(FakeLocalModel),
    Ollama(OllamaHttpModel),
}

impl LocalModelExecutor {
    pub fn from_config(config: &LocalModelConfig) -> JarvisResult<Self> {
        if !config.enabled {
            return Err(JarvisError::Validation(
                "local model provider is disabled".to_string(),
            ));
        }

        match config.provider {
            LocalModelProviderKind::Fake => Ok(Self::Fake(FakeLocalModel::default())),
            LocalModelProviderKind::Ollama => {
                Ok(Self::Ollama(OllamaHttpModel::from_config(config)?))
            }
        }
    }
}

#[async_trait]
impl ModelExecutor for LocalModelExecutor {
    async fn execute(&self, request: ModelRequest) -> JarvisResult<ModelResponse> {
        match self {
            Self::Fake(model) => model.execute(request).await,
            Self::Ollama(model) => model.execute(request).await,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FakeLocalModel {
    response_prefix: String,
    complete_after_steps: u32,
    tool_requests: Vec<ModelToolRequest>,
}

impl FakeLocalModel {
    pub fn new(response_prefix: impl Into<String>) -> Self {
        Self {
            response_prefix: response_prefix.into(),
            complete_after_steps: 1,
            tool_requests: Vec::new(),
        }
    }

    pub fn complete_after_steps(mut self, steps: u32) -> Self {
        self.complete_after_steps = steps.max(1);
        self
    }

    pub fn with_tool_request(mut self, request: ModelToolRequest) -> Self {
        self.tool_requests.push(request);
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
            tool_requests: if request.step_index == 0 {
                self.tool_requests.clone()
            } else {
                Vec::new()
            },
        })
    }
}

#[derive(Debug, Clone)]
pub struct OllamaHttpModel {
    model: String,
    base_url: String,
    timeout: Duration,
    client: reqwest::Client,
}

impl OllamaHttpModel {
    pub fn from_config(config: &LocalModelConfig) -> JarvisResult<Self> {
        let base_url = config.base_url.clone().ok_or_else(|| {
            JarvisError::Validation(
                "Ollama local provider requires JARVIS_OLLAMA_BASE_URL".to_string(),
            )
        })?;
        let timeout = Duration::from_millis(config.timeout_ms);
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|error| JarvisError::Other(error.into()))?;

        Ok(Self {
            model: config.model.clone(),
            base_url: base_url.trim_end_matches('/').to_string(),
            timeout,
            client,
        })
    }

    pub fn safe_endpoint(&self) -> String {
        redact_url_credentials(&self.base_url)
    }
}

#[derive(Debug, Deserialize)]
struct OllamaGenerateResponse {
    response: Option<String>,
    done: Option<bool>,
    error: Option<String>,
}

#[async_trait]
impl ModelExecutor for OllamaHttpModel {
    async fn execute(&self, request: ModelRequest) -> JarvisResult<ModelResponse> {
        let endpoint = format!("{}/api/generate", self.base_url);
        let prompt = ollama_prompt(&request)?;
        let response = self
            .client
            .post(&endpoint)
            .json(&json!({
                "model": self.model,
                "prompt": prompt,
                "stream": false,
            }))
            .send()
            .await
            .map_err(|error| {
                ollama_error(
                    "request failed",
                    &self.safe_endpoint(),
                    self.timeout,
                    Some(error.to_string()),
                )
            })?;

        let status = response.status();
        if !status.is_success() {
            return Err(ollama_error(
                &format!("provider returned HTTP {status}"),
                &self.safe_endpoint(),
                self.timeout,
                None,
            ));
        }

        let body = response
            .json::<OllamaGenerateResponse>()
            .await
            .map_err(|error| {
                ollama_error(
                    "provider returned invalid JSON",
                    &self.safe_endpoint(),
                    self.timeout,
                    Some(error.to_string()),
                )
            })?;

        if let Some(error) = body.error {
            return Err(ollama_error(
                "provider returned an error",
                &self.safe_endpoint(),
                self.timeout,
                Some(error),
            ));
        }

        let message = body.response.unwrap_or_default();
        if message.trim().is_empty() {
            return Err(ollama_error(
                "provider returned an empty response",
                &self.safe_endpoint(),
                self.timeout,
                None,
            ));
        }

        Ok(ModelResponse {
            route: ModelRoute::local(
                self.model.clone(),
                format!(
                    "Ollama-compatible local HTTP provider at {}",
                    self.safe_endpoint()
                ),
            ),
            message,
            complete: body.done.unwrap_or(true),
            tool_requests: Vec::new(),
        })
    }
}

fn ollama_prompt(request: &ModelRequest) -> JarvisResult<String> {
    let tool_results = if request.tool_results.is_empty() {
        "[]".to_string()
    } else {
        serde_json::to_string(&request.tool_results).map_err(|error| {
            JarvisError::Validation(format!(
                "failed to encode tool results for model prompt: {error}"
            ))
        })?
    };

    Ok(format!(
        "You are Jarvis, a local-first assistant. Answer the user directly. Do not claim cloud access. Task: {}\nStep: {}\nTool results: {}",
        request.user_input, request.step_index, tool_results
    ))
}

fn ollama_error(
    summary: &str,
    safe_endpoint: &str,
    timeout: Duration,
    details: Option<String>,
) -> JarvisError {
    let detail = details
        .map(|value| redact_obvious_secrets(&value))
        .unwrap_or_else(|| "no provider detail".to_string());
    JarvisError::Model(format!(
        "local model provider failed: {summary}; endpoint={safe_endpoint}; timeout_ms={}; detail={detail}",
        timeout.as_millis()
    ))
}

fn parse_bool(name: &str, value: &str) -> JarvisResult<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(JarvisError::Validation(format!(
            "{name} must be one of true,false,1,0,yes,no,on,off"
        ))),
    }
}

pub fn redact_url_credentials(input: &str) -> String {
    match reqwest::Url::parse(input) {
        Ok(mut url) => {
            if !url.username().is_empty() {
                let _ = url.set_username("[REDACTED]");
            }
            if url.password().is_some() {
                let _ = url.set_password(Some("[REDACTED]"));
            }
            url.to_string().trim_end_matches('/').to_string()
        }
        Err(_) => redact_obvious_secrets(input),
    }
}

fn redact_obvious_secrets(input: &str) -> String {
    input
        .split_whitespace()
        .map(|token| {
            let normalized = token
                .trim_matches(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-')
                .to_ascii_lowercase();
            if normalized.contains("api_key")
                || normalized.contains("token")
                || normalized.contains("secret")
                || normalized.contains("password")
                || token.starts_with("sk-")
            {
                "[REDACTED]".to_string()
            } else {
                token.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::routing::post;
    use axum::{Json, Router};
    use serde_json::Value;
    use tokio::net::TcpListener;

    #[test]
    fn provider_config_parses_ollama_env_without_chatgpt() {
        let config = ProviderConfig::from_env_values(|key| match key {
            "JARVIS_LOCAL_MODEL_PROVIDER" => Some("ollama".to_string()),
            "JARVIS_LOCAL_MODEL" => Some("llama3.1:8b".to_string()),
            "JARVIS_OLLAMA_BASE_URL" => Some("http://user:secret@127.0.0.1:11434/".to_string()),
            "JARVIS_LOCAL_MODEL_TIMEOUT_MS" => Some("2500".to_string()),
            _ => None,
        })
        .expect("env config");

        assert_eq!(config.local.provider, LocalModelProviderKind::Ollama);
        assert_eq!(config.local.model, "llama3.1:8b");
        assert_eq!(
            config.local.base_url.as_deref(),
            Some("http://user:secret@127.0.0.1:11434")
        );
        assert_eq!(config.local.timeout_ms, 2500);
        assert!(!config.chatgpt.enabled);
    }

    #[test]
    fn rejects_invalid_timeout_env() {
        let error = ProviderConfig::from_env_values(|key| {
            (key == "JARVIS_LOCAL_MODEL_TIMEOUT_MS").then(|| "0".to_string())
        })
        .expect_err("zero timeout should be invalid");

        assert!(error.to_string().contains("greater than zero"));
    }

    #[test]
    fn redacts_url_credentials_for_audit_safe_route_reasons() {
        assert_eq!(
            redact_url_credentials("http://ollama:password@127.0.0.1:11434/"),
            "http://%5BREDACTED%5D:%5BREDACTED%5D@127.0.0.1:11434"
        );
    }

    #[tokio::test]
    async fn ollama_http_provider_posts_non_streaming_generate_request() {
        async fn generate(Json(body): Json<Value>) -> Json<Value> {
            assert_eq!(body["model"], "test-local-model");
            assert_eq!(body["stream"], false);
            assert!(body["prompt"]
                .as_str()
                .expect("prompt")
                .contains("hello local"));
            Json(json!({ "response": "local answer", "done": true }))
        }

        let app = Router::new().route("/api/generate", post(generate));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("test server");
        });

        let config = LocalModelConfig {
            enabled: true,
            provider: LocalModelProviderKind::Ollama,
            model: "test-local-model".to_string(),
            base_url: Some(format!("http://{address}")),
            timeout_ms: 2_000,
        };
        let model = OllamaHttpModel::from_config(&config).expect("ollama model");

        let response = model
            .execute(ModelRequest {
                task_id: uuid::Uuid::new_v4(),
                session_id: uuid::Uuid::new_v4(),
                user_input: "hello local".to_string(),
                step_index: 0,
                tool_results: Vec::new(),
            })
            .await
            .expect("model response");

        assert_eq!(response.message, "local answer");
        assert_eq!(response.route.model, "test-local-model");
        assert_eq!(response.route.provider, ModelProvider::Local);
        assert!(!response.route.reason.contains("hello local"));
    }

    #[tokio::test]
    async fn ollama_http_provider_returns_redacted_structured_errors() {
        async fn generate() -> (axum::http::StatusCode, Json<Value>) {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "token sk-secret should not leak" })),
            )
        }

        let app = Router::new().route("/api/generate", post(generate));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("test server");
        });

        let config = LocalModelConfig {
            enabled: true,
            provider: LocalModelProviderKind::Ollama,
            model: "test-local-model".to_string(),
            base_url: Some(format!("http://user:secret@{address}")),
            timeout_ms: 2_000,
        };
        let model = OllamaHttpModel::from_config(&config).expect("ollama model");
        let error = model
            .execute(ModelRequest {
                task_id: uuid::Uuid::new_v4(),
                session_id: uuid::Uuid::new_v4(),
                user_input: "do not leak this body".to_string(),
                step_index: 0,
                tool_results: Vec::new(),
            })
            .await
            .expect_err("provider error");

        let message = error.to_string();
        assert!(message.contains("HTTP 500"));
        assert!(!message.contains("secret"));
        assert!(!message.contains("do not leak this body"));
    }

    #[tokio::test]
    async fn ollama_http_provider_enforces_configured_timeout() {
        async fn generate() -> Json<Value> {
            tokio::time::sleep(Duration::from_millis(200)).await;
            Json(json!({ "response": "late answer", "done": true }))
        }

        let app = Router::new().route("/api/generate", post(generate));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("test server");
        });

        let config = LocalModelConfig {
            enabled: true,
            provider: LocalModelProviderKind::Ollama,
            model: "test-local-model".to_string(),
            base_url: Some(format!("http://{address}")),
            timeout_ms: 10,
        };
        let model = OllamaHttpModel::from_config(&config).expect("ollama model");
        let error = model
            .execute(ModelRequest {
                task_id: uuid::Uuid::new_v4(),
                session_id: uuid::Uuid::new_v4(),
                user_input: "timeout body should not leak".to_string(),
                step_index: 0,
                tool_results: Vec::new(),
            })
            .await
            .expect_err("provider timeout");

        let message = error.to_string();
        assert!(message.contains("timeout_ms=10"));
        assert!(!message.contains("timeout body should not leak"));
    }
}
