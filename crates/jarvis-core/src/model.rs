use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::Duration;

use crate::plugin::{EchoPlugin, InProcessPlugin, PluginManifest, PluginSource, StatusPlugin};
use crate::router::{ModelProvider as RoutedModelProvider, ModelRouteRecord};
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
    pub base_url: String,
    #[serde(skip_serializing, skip_deserializing)]
    pub api_key: Option<String>,
    pub requires_approval: bool,
    pub timeout_ms: u64,
}

impl Default for ChatGptProviderConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            model: "chatgpt-disabled".to_string(),
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: None,
            requires_approval: true,
            timeout_ms: 30_000,
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
            config.local.timeout_ms = parse_positive_u64("JARVIS_LOCAL_MODEL_TIMEOUT_MS", &value)?;
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

        if let Some(value) = get("JARVIS_CHATGPT_ENABLED") {
            config.chatgpt.enabled = parse_bool("JARVIS_CHATGPT_ENABLED", &value)?;
        }

        if let Some(value) = get("JARVIS_CHATGPT_MODEL") {
            let value = value.trim();
            if value.is_empty() {
                return Err(JarvisError::Validation(
                    "JARVIS_CHATGPT_MODEL cannot be empty".to_string(),
                ));
            }
            config.chatgpt.model = value.to_string();
        }

        if let Some(value) =
            get("JARVIS_OPENAI_BASE_URL").or_else(|| get("JARVIS_CHATGPT_BASE_URL"))
        {
            let value = value.trim();
            if !value.is_empty() {
                config.chatgpt.base_url = value.trim_end_matches('/').to_string();
            }
        }

        if let Some(value) = get("JARVIS_OPENAI_API_KEY").or_else(|| get("JARVIS_CHATGPT_API_KEY"))
        {
            let value = value.trim();
            if !value.is_empty() {
                config.chatgpt.api_key = Some(value.to_string());
            }
        }

        if let Some(value) = get("JARVIS_CHATGPT_TIMEOUT_MS") {
            config.chatgpt.timeout_ms = parse_positive_u64("JARVIS_CHATGPT_TIMEOUT_MS", &value)?;
        }

        if let Some(value) = get("JARVIS_CHATGPT_REQUIRES_APPROVAL") {
            config.chatgpt.requires_approval =
                parse_bool("JARVIS_CHATGPT_REQUIRES_APPROVAL", &value)?;
        }

        if config.chatgpt.enabled {
            if config.chatgpt.model == "chatgpt-disabled" {
                config.chatgpt.model = "gpt-4.1-mini".to_string();
            }
            if config.chatgpt.api_key.is_none() {
                return Err(JarvisError::Validation(
                    "JARVIS_CHATGPT_ENABLED requires JARVIS_OPENAI_API_KEY".to_string(),
                ));
            }
            if !config.chatgpt.requires_approval {
                return Err(JarvisError::Validation(
                    "JARVIS_CHATGPT_REQUIRES_APPROVAL must remain enabled for ChatGPT routing"
                        .to_string(),
                ));
            }
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
        self.chatgpt.api_key = Some("test-openai-api-key".to_string());
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

    pub fn chatgpt(model: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            provider: ModelProvider::ChatGpt,
            model: model.into(),
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
    pub tool_results: Vec<ModelToolResult>,
    #[serde(default = "default_first_party_model_tools")]
    pub first_party_tools: Vec<ModelToolDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelToolDefinition {
    pub plugin_id: String,
    pub action: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelResponse {
    pub route: ModelRoute,
    pub message: String,
    pub complete: bool,
    #[serde(default)]
    pub output_chunks: Vec<ModelOutputChunk>,
    #[serde(default)]
    pub tool_requests: Vec<ModelToolRequest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelOutputChunk {
    pub sequence: u64,
    pub byte_count: usize,
    pub char_count: usize,
    #[serde(default)]
    pub final_chunk: bool,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProviderResponseEnvelope {
    message: String,
    complete: bool,
    tool_requests: Vec<ModelToolRequest>,
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

    async fn execute_route(
        &self,
        request: ModelRequest,
        _route: &ModelRouteRecord,
    ) -> JarvisResult<ModelResponse> {
        self.execute(request).await
    }
}

#[derive(Debug, Clone)]
pub struct RoutedModelExecutor {
    local: Option<LocalModelExecutor>,
    chatgpt: Option<ChatGptHttpModel>,
}

impl RoutedModelExecutor {
    pub fn from_config(config: &ProviderConfig) -> JarvisResult<Self> {
        Ok(Self {
            local: if config.local.enabled {
                Some(LocalModelExecutor::from_config(&config.local)?)
            } else {
                None
            },
            chatgpt: if config.chatgpt.enabled {
                Some(ChatGptHttpModel::from_config(&config.chatgpt)?)
            } else {
                None
            },
        })
    }
}

#[async_trait]
impl ModelExecutor for RoutedModelExecutor {
    async fn execute(&self, request: ModelRequest) -> JarvisResult<ModelResponse> {
        self.local
            .as_ref()
            .ok_or_else(|| JarvisError::Validation("local model provider is disabled".to_string()))?
            .execute(request)
            .await
    }

    async fn execute_route(
        &self,
        request: ModelRequest,
        route: &ModelRouteRecord,
    ) -> JarvisResult<ModelResponse> {
        match route.selected_provider {
            Some(RoutedModelProvider::Local) => self.execute(request).await,
            Some(RoutedModelProvider::ChatGpt) => {
                let model = self.chatgpt.as_ref().ok_or_else(|| {
                    JarvisError::Validation("ChatGPT provider is not configured".to_string())
                })?;
                model.execute_guarded(request, route).await
            }
            None => Err(JarvisError::Validation(
                "model route did not select a provider".to_string(),
            )),
        }
    }
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
        let message = format!("{}: {}", self.response_prefix, request.user_input);
        Ok(ModelResponse {
            route: ModelRoute::fake_local("local model is the default route for v1 commands"),
            output_chunks: bounded_output_chunks(&message),
            message,
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

        let raw_message = body.response.unwrap_or_default();
        if raw_message.trim().is_empty() {
            return Err(ollama_error(
                "provider returned an empty response",
                &self.safe_endpoint(),
                self.timeout,
                None,
            ));
        }
        let envelope = parse_provider_response_envelope(&raw_message).map_err(|error| {
            ollama_error(
                "provider returned an invalid tool-call envelope",
                &self.safe_endpoint(),
                self.timeout,
                Some(error.to_string()),
            )
        })?;

        Ok(ModelResponse {
            route: ModelRoute::local(
                self.model.clone(),
                format!(
                    "Ollama-compatible local HTTP provider at {}",
                    self.safe_endpoint()
                ),
            ),
            output_chunks: bounded_output_chunks(&envelope.message),
            message: envelope.message,
            complete: body.done.unwrap_or(envelope.complete),
            tool_requests: envelope.tool_requests,
        })
    }
}

#[derive(Debug, Clone)]
pub struct ChatGptHttpModel {
    model: String,
    base_url: String,
    api_key: String,
    timeout: Duration,
    client: reqwest::Client,
}

impl ChatGptHttpModel {
    pub fn from_config(config: &ChatGptProviderConfig) -> JarvisResult<Self> {
        if !config.enabled {
            return Err(JarvisError::Validation(
                "ChatGPT provider is disabled; set JARVIS_CHATGPT_ENABLED=true to opt in"
                    .to_string(),
            ));
        }
        if !config.requires_approval {
            return Err(JarvisError::Validation(
                "ChatGPT provider requires explicit route approval".to_string(),
            ));
        }
        let api_key = config.api_key.clone().ok_or_else(|| {
            JarvisError::Validation("ChatGPT provider requires JARVIS_OPENAI_API_KEY".to_string())
        })?;
        if config.model.trim().is_empty() || config.model == "chatgpt-disabled" {
            return Err(JarvisError::Validation(
                "ChatGPT provider requires a concrete model".to_string(),
            ));
        }

        let timeout = Duration::from_millis(config.timeout_ms);
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .build()
            .map_err(|error| JarvisError::Other(error.into()))?;

        Ok(Self {
            model: config.model.clone(),
            base_url: config.base_url.trim_end_matches('/').to_string(),
            api_key,
            timeout,
            client,
        })
    }

    pub fn safe_endpoint(&self) -> String {
        redact_url_credentials(&self.base_url)
    }

    async fn execute_guarded(
        &self,
        request: ModelRequest,
        route: &ModelRouteRecord,
    ) -> JarvisResult<ModelResponse> {
        if route.selected_provider != Some(RoutedModelProvider::ChatGpt) {
            return Err(JarvisError::Validation(
                "ChatGPT execution requires a selected ChatGPT route".to_string(),
            ));
        }
        if route.context_for_model.is_none() {
            return Err(JarvisError::Validation(
                "ChatGPT execution requires redacted route context".to_string(),
            ));
        }
        if route.evidence.restricted_cloud_block {
            return Err(JarvisError::Validation(
                "ChatGPT execution cannot run with restricted route evidence".to_string(),
            ));
        }

        let endpoint = format!("{}/chat/completions", self.base_url);
        let prompt = chatgpt_prompt(&request, route)?;
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        let auth_value =
            HeaderValue::from_str(&format!("Bearer {}", self.api_key)).map_err(|_| {
                JarvisError::Validation(
                    "JARVIS_OPENAI_API_KEY contains invalid header bytes".to_string(),
                )
            })?;
        headers.insert(AUTHORIZATION, auth_value);

        let response = self
            .client
            .post(&endpoint)
            .headers(headers)
            .json(&json!({
                "model": self.model,
                "messages": [
                    {
                        "role": "system",
                        "content": "You are Jarvis. Use only the redacted context supplied by the route guardrail. Do not request secrets or claim access to hidden local state. Prefer the provided first-party tools when a tool is needed. If native tool calls are unavailable, reply only with JSON using an exact provided plugin_id and action: {\"message\":\"short reason\",\"complete\":false,\"tool_requests\":[{\"plugin_id\":\"<provided plugin_id>\",\"action\":\"<provided action>\",\"input\":{}}]}. If no provided tool fits, answer directly without tool_requests."
                    },
                    {
                        "role": "user",
                        "content": prompt
                    }
                ],
                "temperature": 0.2,
                "tools": openai_first_party_tools(&request.first_party_tools),
                "tool_choice": "auto",
            }))
            .send()
            .await
            .map_err(|error| {
                chatgpt_error(
                    "request failed",
                    &self.safe_endpoint(),
                    self.timeout,
                    Some(error.to_string()),
                )
            })?;

        let status = response.status();
        if !status.is_success() {
            let detail = response.text().await.ok();
            return Err(chatgpt_error(
                &format!("provider returned HTTP {status}"),
                &self.safe_endpoint(),
                self.timeout,
                detail,
            ));
        }

        let body = response
            .json::<OpenAiChatCompletionResponse>()
            .await
            .map_err(|error| {
                chatgpt_error(
                    "provider returned invalid JSON",
                    &self.safe_endpoint(),
                    self.timeout,
                    Some(error.to_string()),
                )
            })?;

        let message = body
            .choices
            .first()
            .map(|choice| choice.message.clone())
            .ok_or_else(|| {
                chatgpt_error(
                    "provider returned no choices",
                    &self.safe_endpoint(),
                    self.timeout,
                    None,
                )
            })?;
        let tool_requests =
            parse_openai_native_tool_calls(&message.tool_calls).map_err(|error| {
                chatgpt_error(
                    "provider returned invalid native tool calls",
                    &self.safe_endpoint(),
                    self.timeout,
                    Some(error.to_string()),
                )
            })?;
        let raw_message = message
            .content
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_string();
        if raw_message.is_empty() && tool_requests.is_empty() {
            return Err(chatgpt_error(
                "provider returned an empty response",
                &self.safe_endpoint(),
                self.timeout,
                None,
            ));
        }
        let envelope = if tool_requests.is_empty() {
            parse_provider_response_envelope(&raw_message).map_err(|error| {
                chatgpt_error(
                    "provider returned an invalid tool-call envelope",
                    &self.safe_endpoint(),
                    self.timeout,
                    Some(error.to_string()),
                )
            })?
        } else {
            ProviderResponseEnvelope {
                message: raw_message,
                complete: false,
                tool_requests,
            }
        };

        Ok(ModelResponse {
            route: ModelRoute::chatgpt(
                self.model.clone(),
                format!(
                    "ChatGPT selected by audited route {}; endpoint={}; redaction_applied={}",
                    route.id,
                    self.safe_endpoint(),
                    route.redaction_applied
                ),
            ),
            output_chunks: bounded_output_chunks(&envelope.message),
            message: envelope.message,
            complete: envelope.complete,
            tool_requests: envelope.tool_requests,
        })
    }
}

pub fn bounded_output_chunks(message: &str) -> Vec<ModelOutputChunk> {
    const MAX_CHUNKS: usize = 32;
    const TARGET_CHUNK_CHARS: usize = 80;

    if message.is_empty() {
        return Vec::new();
    }

    let chars: Vec<char> = message.chars().collect();
    let mut chunks = Vec::new();
    for (sequence, slice) in chars
        .chunks(TARGET_CHUNK_CHARS)
        .take(MAX_CHUNKS)
        .enumerate()
    {
        let text: String = slice.iter().collect();
        chunks.push(ModelOutputChunk {
            sequence: sequence as u64,
            byte_count: text.len(),
            char_count: slice.len(),
            final_chunk: false,
        });
    }
    if let Some(last) = chunks.last_mut() {
        last.final_chunk = chars.len() <= TARGET_CHUNK_CHARS * MAX_CHUNKS;
    }
    chunks
}

#[derive(Debug, Deserialize)]
struct OpenAiChatCompletionResponse {
    choices: Vec<OpenAiChatChoice>,
}

#[derive(Debug, Deserialize)]
struct OpenAiChatChoice {
    message: OpenAiChatMessage,
}

#[derive(Debug, Clone, Deserialize)]
struct OpenAiChatMessage {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Vec<OpenAiToolCall>,
}

#[derive(Debug, Clone, Deserialize)]
struct OpenAiToolCall {
    #[serde(rename = "type")]
    call_type: String,
    function: OpenAiToolCallFunction,
}

#[derive(Debug, Clone, Deserialize)]
struct OpenAiToolCallFunction {
    name: String,
    arguments: String,
}

fn chatgpt_prompt(request: &ModelRequest, route: &ModelRouteRecord) -> JarvisResult<String> {
    let context = route
        .context_for_model
        .as_ref()
        .ok_or_else(|| JarvisError::Validation("ChatGPT route context is missing".to_string()))?;
    let tool_results = if request.tool_results.is_empty() {
        "[]".to_string()
    } else {
        redact_obvious_secrets(
            &serde_json::to_string(&request.tool_results).map_err(|error| {
                JarvisError::Validation(format!(
                    "failed to encode tool results for ChatGPT prompt: {error}"
                ))
            })?,
        )
    };

    Ok(format!(
        "Route id: {}\nSensitivity: {:?}\nRedacted task context: {}\nStep: {}\nTool results: {}",
        route.id, route.sensitivity, context, request.step_index, tool_results
    ))
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
        "You are Jarvis, a local-first assistant. Answer the user directly. Do not claim cloud access. Registered first-party tools are exactly this JSON allowlist: {}. Never invent plugin_id or action values. The plugin_id must equal one listed plugin_id exactly; action names, command aliases, endpoints, and capability names are invalid plugin ids. Choose exactly one response mode: plain natural language with no JSON-looking tool fields, or one strict JSON object with no surrounding prose. If a first-party tool is needed, copy one exact registered plugin_id and action into this JSON shape: {{\"message\":\"short reason\",\"complete\":false,\"tool_requests\":[{{\"plugin_id\":\"<registered plugin_id>\",\"action\":\"<registered action>\",\"input\":{{}}}}]}}. If no registered tool fits, answer directly without tool_requests. Task: {}\nStep: {}\nTool results: {}",
        first_party_tool_inventory_text(&request.first_party_tools),
        request.user_input,
        request.step_index,
        tool_results
    ))
}

fn parse_provider_response_envelope(raw: &str) -> JarvisResult<ProviderResponseEnvelope> {
    let trimmed = raw.trim();
    let Some(value) = parse_envelope_candidate(trimmed)? else {
        if trimmed.contains("\"tool_requests\"") || trimmed.contains("'tool_requests'") {
            return Err(JarvisError::Model(
                "provider mixed prose and tool_requests outside a strict JSON envelope".to_string(),
            ));
        }
        return Ok(ProviderResponseEnvelope {
            message: trimmed.to_string(),
            complete: true,
            tool_requests: Vec::new(),
        });
    };

    let object = value.as_object().ok_or_else(|| {
        JarvisError::Model("provider tool-call envelope must be a JSON object".to_string())
    })?;
    let is_envelope = object.contains_key("message")
        || object.contains_key("complete")
        || object.contains_key("tool_requests");
    if !is_envelope {
        return Ok(ProviderResponseEnvelope {
            message: trimmed.to_string(),
            complete: true,
            tool_requests: Vec::new(),
        });
    }

    let message = object
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let complete = object
        .get("complete")
        .and_then(Value::as_bool)
        .unwrap_or_else(|| !object.contains_key("tool_requests"));
    let tool_requests = parse_provider_tool_requests(object.get("tool_requests"))?;

    if message.is_empty() && tool_requests.is_empty() {
        return Err(JarvisError::Model(
            "provider tool-call envelope must include a message or tool_requests".to_string(),
        ));
    }

    Ok(ProviderResponseEnvelope {
        message,
        complete,
        tool_requests,
    })
}

fn parse_envelope_candidate(trimmed: &str) -> JarvisResult<Option<Value>> {
    if !(trimmed.starts_with('{') && trimmed.ends_with('}')) {
        return Ok(None);
    }

    serde_json::from_str::<Value>(trimmed)
        .map(Some)
        .map_err(|_| JarvisError::Model("provider JSON envelope is malformed".to_string()))
}

fn parse_provider_tool_requests(value: Option<&Value>) -> JarvisResult<Vec<ModelToolRequest>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let requests = value
        .as_array()
        .ok_or_else(|| JarvisError::Model("provider tool_requests must be an array".to_string()))?;

    let mut parsed = Vec::with_capacity(requests.len());
    for request in requests {
        let object = request.as_object().ok_or_else(|| {
            JarvisError::Model("provider tool_requests entries must be objects".to_string())
        })?;
        for key in object.keys() {
            if key != "plugin_id" && key != "action" && key != "input" {
                return Err(JarvisError::Model(format!(
                    "provider tool_requests entries contain unsupported field {key}"
                )));
            }
        }
        let plugin_id = object
            .get("plugin_id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                JarvisError::Model("provider tool_requests entries require plugin_id".to_string())
            })?;
        let action = object
            .get("action")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                JarvisError::Model("provider tool_requests entries require action".to_string())
            })?;
        let input = object.get("input").cloned().unwrap_or_else(|| json!({}));

        parsed.push(ModelToolRequest::new(plugin_id, action, input));
    }

    Ok(parsed)
}

fn parse_openai_native_tool_calls(calls: &[OpenAiToolCall]) -> JarvisResult<Vec<ModelToolRequest>> {
    let mut parsed = Vec::with_capacity(calls.len());
    for call in calls {
        if call.call_type != "function" {
            return Err(JarvisError::Model(
                "OpenAI tool_calls entries must be function calls".to_string(),
            ));
        }
        let (plugin_id, action) = parse_openai_function_name(&call.function.name)?;
        let arguments = call.function.arguments.trim();
        let input = if arguments.is_empty() {
            json!({})
        } else {
            serde_json::from_str::<Value>(arguments).map_err(|_| {
                JarvisError::Model("OpenAI function arguments must be valid JSON".to_string())
            })?
        };
        parsed.push(ModelToolRequest::new(plugin_id, action, input));
    }

    Ok(parsed)
}

fn parse_openai_function_name(name: &str) -> JarvisResult<(&str, &str)> {
    let name = name.trim();
    let (plugin_id, action) = name.split_once("__").ok_or_else(|| {
        JarvisError::Model(
            "OpenAI function names must use first-party plugin__action form".to_string(),
        )
    })?;
    if plugin_id.is_empty() || action.is_empty() {
        return Err(JarvisError::Model(
            "OpenAI function names require plugin and action".to_string(),
        ));
    }
    Ok((plugin_id, action))
}

fn default_first_party_model_tools() -> Vec<ModelToolDefinition> {
    model_tool_definitions_from_manifests([EchoPlugin.manifest(), StatusPlugin.manifest()])
}

fn openai_first_party_tools(tools: &[ModelToolDefinition]) -> Vec<Value> {
    sorted_model_tool_definitions(tools)
        .into_iter()
        .map(openai_tool_for_definition)
        .collect()
}

fn first_party_tool_inventory_text(tools: &[ModelToolDefinition]) -> String {
    let tools = sorted_model_tool_definitions(tools)
        .into_iter()
        .map(|tool| {
            json!({
                "plugin_id": tool.plugin_id,
                "action": tool.action,
                "description": tool.description,
                "input_schema": tool.input_schema,
            })
        })
        .collect::<Vec<_>>();
    if tools.is_empty() {
        "[]".to_string()
    } else {
        serde_json::to_string(&tools).unwrap_or_else(|_| "[]".to_string())
    }
}

pub(crate) fn model_tool_definitions_from_manifests(
    manifests: impl IntoIterator<Item = PluginManifest>,
) -> Vec<ModelToolDefinition> {
    let tools = manifests
        .into_iter()
        .filter(|manifest| manifest.source == PluginSource::FirstParty)
        .flat_map(|manifest| {
            let plugin_id = manifest.id;
            manifest
                .actions
                .into_iter()
                .map(move |action| ModelToolDefinition {
                    plugin_id: plugin_id.clone(),
                    action: action.name,
                    description: action.description,
                    input_schema: action.input_schema.schema,
                })
        })
        .collect::<Vec<_>>();
    sorted_model_tool_definitions(&tools)
}

fn sorted_model_tool_definitions(tools: &[ModelToolDefinition]) -> Vec<ModelToolDefinition> {
    let mut tools = tools.to_vec();
    tools.sort_by(|left, right| {
        left.plugin_id
            .cmp(&right.plugin_id)
            .then_with(|| left.action.cmp(&right.action))
    });
    tools
}

fn openai_tool_for_definition(tool: ModelToolDefinition) -> Value {
    json!({
        "type": "function",
        "function": {
            "name": format!("{}__{}", tool.plugin_id, tool.action),
            "description": tool.description,
            "parameters": tool.input_schema,
        }
    })
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

fn chatgpt_error(
    summary: &str,
    safe_endpoint: &str,
    timeout: Duration,
    details: Option<String>,
) -> JarvisError {
    let detail = details
        .map(|value| redact_obvious_secrets(&value))
        .unwrap_or_else(|| "no provider detail".to_string());
    JarvisError::Model(format!(
        "ChatGPT provider failed: {summary}; endpoint={safe_endpoint}; timeout_ms={}; detail={detail}",
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

fn parse_positive_u64(name: &str, value: &str) -> JarvisResult<u64> {
    let parsed = value
        .parse::<u64>()
        .map_err(|_| JarvisError::Validation(format!("{name} must be a positive integer")))?;
    if parsed == 0 {
        return Err(JarvisError::Validation(format!(
            "{name} must be greater than zero"
        )));
    }
    Ok(parsed)
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

    fn assert_array_contains(value: &Value, field: &str, expected: &str) {
        let items = value.as_array().expect("array");
        assert!(
            items
                .iter()
                .any(|item| item.get(field).and_then(Value::as_str) == Some(expected)),
            "{value}"
        );
    }

    fn assert_array_contains_nested(value: &Value, path: &[&str], expected: &str) {
        let items = value.as_array().expect("array");
        assert!(
            items.iter().any(|item| {
                let mut current = item;
                for field in path {
                    current = &current[*field];
                }
                current.as_str() == Some(expected)
            }),
            "{value}"
        );
    }

    fn model_tool(plugin_id: &str, action: &str) -> ModelToolDefinition {
        ModelToolDefinition {
            plugin_id: plugin_id.to_string(),
            action: action.to_string(),
            description: format!("{plugin_id}.{action} test tool"),
            input_schema: json!({
                "type": "object",
                "properties": {},
                "additionalProperties": false
            }),
        }
    }

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
    fn provider_config_requires_explicit_chatgpt_key_and_keeps_it_out_of_json() {
        let missing_key = ProviderConfig::from_env_values(|key| {
            (key == "JARVIS_CHATGPT_ENABLED").then(|| "true".to_string())
        })
        .expect_err("ChatGPT opt-in without key should fail");
        assert!(missing_key.to_string().contains("JARVIS_OPENAI_API_KEY"));

        let config = ProviderConfig::from_env_values(|key| match key {
            "JARVIS_CHATGPT_ENABLED" => Some("true".to_string()),
            "JARVIS_OPENAI_API_KEY" => Some("test-token-value".to_string()),
            "JARVIS_CHATGPT_MODEL" => Some("gpt-test".to_string()),
            "JARVIS_OPENAI_BASE_URL" => Some("http://127.0.0.1:1234/v1/".to_string()),
            "JARVIS_CHATGPT_TIMEOUT_MS" => Some("2500".to_string()),
            _ => None,
        })
        .expect("ChatGPT env config");

        assert!(config.chatgpt.enabled);
        assert_eq!(config.chatgpt.model, "gpt-test");
        assert_eq!(config.chatgpt.base_url, "http://127.0.0.1:1234/v1");
        assert_eq!(config.chatgpt.timeout_ms, 2500);
        assert_eq!(config.chatgpt.api_key.as_deref(), Some("test-token-value"));

        let encoded = serde_json::to_string(&config).expect("provider config json");
        assert!(!encoded.contains("test-token-value"));
        assert!(!encoded.contains("api_key"));
    }

    #[test]
    fn provider_config_rejects_chatgpt_without_approval_guardrail() {
        let error = ProviderConfig::from_env_values(|key| match key {
            "JARVIS_CHATGPT_ENABLED" => Some("true".to_string()),
            "JARVIS_OPENAI_API_KEY" => Some("test-token-value".to_string()),
            "JARVIS_CHATGPT_REQUIRES_APPROVAL" => Some("false".to_string()),
            _ => None,
        })
        .expect_err("approval guardrail cannot be disabled");

        assert!(error.to_string().contains("must remain enabled"));
        assert!(!error.to_string().contains("test-token-value"));
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

    #[test]
    fn provider_tool_request_envelope_rejects_malformed_tool_requests_without_leaking_prompt() {
        let raw = json!({
            "message": "use a tool for api_key=abc123",
            "tool_requests": "not an array"
        })
        .to_string();

        let error = parse_provider_response_envelope(&raw).expect_err("malformed envelope");

        let message = error.to_string();
        assert!(message.contains("tool_requests must be an array"));
        assert!(!message.contains("api_key=abc123"));
    }

    #[test]
    fn provider_tool_request_envelope_rejects_mixed_prose_and_tool_json() {
        let raw = "I can check that.\n{\"tool_requests\":[{\"plugin_id\":\"fake_status\",\"action\":\"status\",\"input\":{}}]}";

        let error = parse_provider_response_envelope(raw).expect_err("mixed envelope");

        let message = error.to_string();
        assert!(message.contains("mixed prose and tool_requests"));
        assert!(!message.contains("fake_status"));
    }

    #[test]
    fn provider_tool_request_envelope_rejects_unsupported_tool_fields() {
        let raw = json!({
            "message": "use a tool",
            "tool_requests": [
                {
                    "plugin_id": "fake_status",
                    "action": "status",
                    "tool_id": "invented",
                    "input": {}
                }
            ]
        })
        .to_string();

        let error = parse_provider_response_envelope(&raw).expect_err("unsupported field");

        let message = error.to_string();
        assert!(message.contains("unsupported field tool_id"));
        assert!(!message.contains("fake_status"));
    }

    #[test]
    fn ollama_prompt_uses_request_supplied_first_party_tool_inventory() {
        let prompt = ollama_prompt(&ModelRequest {
            task_id: uuid::Uuid::new_v4(),
            session_id: uuid::Uuid::new_v4(),
            user_input: "check runtime inventory".to_string(),
            step_index: 0,
            tool_results: Vec::new(),
            first_party_tools: vec![model_tool("runtime_status", "inspect")],
        })
        .expect("prompt");

        assert!(prompt.contains("\"plugin_id\":\"runtime_status\""));
        assert!(prompt.contains("\"action\":\"inspect\""));
        assert!(prompt.contains("\"input_schema\""));
        assert!(prompt.contains("JSON allowlist"));
        assert!(prompt.contains(
            "action names, command aliases, endpoints, and capability names are invalid plugin ids"
        ));
        assert!(!prompt.contains("fake_status.status"));
        assert!(!prompt.contains("fake_echo.echo"));
    }

    #[test]
    fn chatgpt_tools_use_request_supplied_first_party_tool_inventory() {
        let tools = Value::Array(openai_first_party_tools(&[model_tool(
            "runtime_status",
            "inspect",
        )]));

        assert_array_contains_nested(&tools, &["function", "name"], "runtime_status__inspect");
        assert!(!tools.to_string().contains("fake_status__status"));
        assert!(!tools.to_string().contains("fake_echo__echo"));
    }

    #[tokio::test]
    async fn ollama_http_provider_posts_non_streaming_generate_request() {
        async fn generate(Json(body): Json<Value>) -> Json<Value> {
            assert_eq!(body["model"], "test-local-model");
            assert_eq!(body["stream"], false);
            let prompt = body["prompt"].as_str().expect("prompt");
            assert!(prompt.contains("hello local"));
            assert!(prompt.contains("Registered first-party tools are exactly this JSON allowlist"));
            assert!(prompt.contains("\"plugin_id\":\"fake_status\""));
            assert!(prompt.contains("\"action\":\"status\""));
            assert!(prompt.contains("\"plugin_id\":\"fake_echo\""));
            assert!(prompt.contains("Never invent plugin_id or action values"));
            assert!(prompt.contains("action names, command aliases, endpoints, and capability names are invalid plugin ids"));
            assert!(prompt.contains("one strict JSON object with no surrounding prose"));
            assert!(prompt.contains("<registered plugin_id>"));
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
                first_party_tools: default_first_party_model_tools(),
            })
            .await
            .expect("model response");

        assert_eq!(response.message, "local answer");
        assert_eq!(response.route.model, "test-local-model");
        assert_eq!(response.route.provider, ModelProvider::Local);
        assert!(!response.route.reason.contains("hello local"));
    }

    #[tokio::test]
    async fn ollama_http_provider_parses_tool_request_envelope() {
        async fn generate() -> Json<Value> {
            Json(json!({
                "response": json!({
                    "message": "checking local status",
                    "complete": false,
                    "tool_requests": [
                        {
                            "plugin_id": "fake_status",
                            "action": "status",
                            "input": {}
                        }
                    ]
                }).to_string(),
                "done": false
            }))
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
                user_input: "status".to_string(),
                step_index: 0,
                tool_results: Vec::new(),
                first_party_tools: default_first_party_model_tools(),
            })
            .await
            .expect("model response");

        assert_eq!(response.message, "checking local status");
        assert!(!response.complete);
        assert_eq!(response.tool_requests.len(), 1);
        assert_eq!(response.tool_requests[0].plugin_id, "fake_status");
        assert_eq!(response.tool_requests[0].action, "status");
        assert_eq!(response.tool_requests[0].input, json!({}));
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
                first_party_tools: default_first_party_model_tools(),
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
                first_party_tools: default_first_party_model_tools(),
            })
            .await
            .expect_err("provider timeout");

        let message = error.to_string();
        assert!(message.contains("timeout_ms=10"));
        assert!(!message.contains("timeout body should not leak"));
    }

    #[tokio::test]
    async fn chatgpt_http_provider_posts_redacted_openai_compatible_request() {
        async fn chat(Json(body): Json<Value>) -> Json<Value> {
            assert_eq!(body["model"], "gpt-test");
            assert_array_contains(&body["tools"], "type", "function");
            assert_array_contains_nested(
                &body["tools"],
                &["function", "name"],
                "fake_status__status",
            );
            assert_eq!(body["tool_choice"], "auto");
            let messages = body["messages"].as_array().expect("messages");
            assert_eq!(messages[0]["role"], "system");
            let user_content = messages[1]["content"].as_str().expect("user content");
            assert!(user_content.contains("Redacted task context: workspace [REDACTED]"));
            assert!(!user_content.contains("api_key=abc123"));
            Json(json!({
                "choices": [
                    { "message": { "content": "cloud answer" } }
                ]
            }))
        }

        let app = Router::new().route("/v1/chat/completions", post(chat));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("test server");
        });

        let config = ChatGptProviderConfig {
            enabled: true,
            model: "gpt-test".to_string(),
            base_url: format!("http://{address}/v1"),
            api_key: Some("test-token-value".to_string()),
            requires_approval: true,
            timeout_ms: 2_000,
        };
        let model = ChatGptHttpModel::from_config(&config).expect("chatgpt model");
        let route = crate::ModelRouter::route(&crate::ModelRouteRequest {
            task_id: Some(uuid::Uuid::new_v4()),
            user_intent: "workspace planning".to_string(),
            sensitivity: crate::Sensitivity::Workspace,
            required_scopes: vec![crate::CapabilityScope::Conversation],
            granted_scopes: vec![
                crate::CapabilityScope::Conversation,
                crate::CapabilityScope::CloudModel,
            ],
            local_available: false,
            local_sufficient: false,
            provider_status: ProviderStatus::from_config(&ProviderConfig {
                local: LocalModelConfig {
                    enabled: false,
                    ..LocalModelConfig::default()
                },
                chatgpt: config.clone(),
            }),
            emergency_paused: false,
            approval: None,
            context_preview: "workspace api_key=abc123".to_string(),
        });

        let response = model
            .execute_guarded(
                ModelRequest {
                    task_id: route.task_id.expect("task id"),
                    session_id: uuid::Uuid::new_v4(),
                    user_input: "raw api_key=abc123 should not be sent".to_string(),
                    step_index: 0,
                    tool_results: Vec::new(),
                    first_party_tools: default_first_party_model_tools(),
                },
                &route,
            )
            .await
            .expect("chatgpt response");

        assert_eq!(response.message, "cloud answer");
        assert_eq!(response.route.provider, ModelProvider::ChatGpt);
        assert!(response.route.reason.contains("redaction_applied=true"));
        assert!(!response.route.reason.contains("test-token-value"));
    }

    #[tokio::test]
    async fn chatgpt_http_provider_parses_tool_request_envelope() {
        async fn chat() -> Json<Value> {
            Json(json!({
                "choices": [
                    {
                        "message": {
                            "content": json!({
                                "message": "checking cloud-routed status",
                                "complete": false,
                                "tool_requests": [
                                    {
                                        "plugin_id": "fake_status",
                                        "action": "status",
                                        "input": {}
                                    }
                                ]
                            }).to_string()
                        }
                    }
                ]
            }))
        }

        let app = Router::new().route("/v1/chat/completions", post(chat));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("test server");
        });

        let config = ChatGptProviderConfig {
            enabled: true,
            model: "gpt-test".to_string(),
            base_url: format!("http://{address}/v1"),
            api_key: Some("test-token-value".to_string()),
            requires_approval: true,
            timeout_ms: 2_000,
        };
        let model = ChatGptHttpModel::from_config(&config).expect("chatgpt model");
        let route = test_chatgpt_route(&config, "workspace context");

        let response = model
            .execute_guarded(
                ModelRequest {
                    task_id: route.task_id.expect("task id"),
                    session_id: uuid::Uuid::new_v4(),
                    user_input: "status".to_string(),
                    step_index: 0,
                    tool_results: Vec::new(),
                    first_party_tools: default_first_party_model_tools(),
                },
                &route,
            )
            .await
            .expect("chatgpt response");

        assert_eq!(response.message, "checking cloud-routed status");
        assert!(!response.complete);
        assert_eq!(response.tool_requests.len(), 1);
        assert_eq!(response.tool_requests[0].plugin_id, "fake_status");
        assert_eq!(response.tool_requests[0].action, "status");
    }

    #[tokio::test]
    async fn chatgpt_http_provider_parses_native_tool_calls() {
        async fn chat(Json(body): Json<Value>) -> Json<Value> {
            assert_array_contains_nested(&body["tools"], &["function", "name"], "fake_echo__echo");
            Json(json!({
                "choices": [
                    {
                        "message": {
                            "content": null,
                            "tool_calls": [
                                {
                                    "id": "call_1",
                                    "type": "function",
                                    "function": {
                                        "name": "fake_echo__echo",
                                        "arguments": "{\"message\":\"native call\"}"
                                    }
                                }
                            ]
                        }
                    }
                ]
            }))
        }

        let app = Router::new().route("/v1/chat/completions", post(chat));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("test server");
        });

        let config = ChatGptProviderConfig {
            enabled: true,
            model: "gpt-test".to_string(),
            base_url: format!("http://{address}/v1"),
            api_key: Some("test-token-value".to_string()),
            requires_approval: true,
            timeout_ms: 2_000,
        };
        let model = ChatGptHttpModel::from_config(&config).expect("chatgpt model");
        let route = test_chatgpt_route(&config, "workspace context");

        let response = model
            .execute_guarded(
                ModelRequest {
                    task_id: route.task_id.expect("task id"),
                    session_id: uuid::Uuid::new_v4(),
                    user_input: "use native tool".to_string(),
                    step_index: 0,
                    tool_results: Vec::new(),
                    first_party_tools: default_first_party_model_tools(),
                },
                &route,
            )
            .await
            .expect("chatgpt response");

        assert_eq!(response.message, "");
        assert!(!response.complete);
        assert_eq!(response.tool_requests.len(), 1);
        assert_eq!(response.tool_requests[0].plugin_id, "fake_echo");
        assert_eq!(response.tool_requests[0].action, "echo");
        assert_eq!(
            response.tool_requests[0].input,
            json!({ "message": "native call" })
        );
    }

    #[tokio::test]
    async fn chatgpt_http_provider_returns_redacted_structured_errors() {
        async fn chat() -> (axum::http::StatusCode, Json<Value>) {
            (
                axum::http::StatusCode::UNAUTHORIZED,
                Json(json!({ "error": { "message": "bad token test-token-value" } })),
            )
        }

        let app = Router::new().route("/v1/chat/completions", post(chat));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("test server");
        });

        let config = ChatGptProviderConfig {
            enabled: true,
            model: "gpt-test".to_string(),
            base_url: format!("http://user:password@{address}/v1"),
            api_key: Some("test-token-value".to_string()),
            requires_approval: true,
            timeout_ms: 2_000,
        };
        let model = ChatGptHttpModel::from_config(&config).expect("chatgpt model");
        let route = ModelRouteRecord {
            id: uuid::Uuid::new_v4(),
            task_id: Some(uuid::Uuid::new_v4()),
            outcome: crate::RouteOutcome::Selected,
            selected_provider: Some(RoutedModelProvider::ChatGpt),
            reason: "test route".to_string(),
            sensitivity: crate::Sensitivity::Workspace,
            approval_status: crate::ApprovalStatus::NotRequired,
            redaction_applied: true,
            context_for_model: Some("redacted context".to_string()),
            local_available: false,
            local_sufficient: false,
            evidence: crate::RouteEvidence {
                local_available: false,
                local_sufficient: false,
                local_provider: LocalModelProviderKind::Fake,
                local_model: "fake-local-model".to_string(),
                local_endpoint_configured: false,
                chatgpt_enabled: true,
                chatgpt_requires_approval: true,
                required_scopes: vec![crate::CapabilityScope::Conversation],
                granted_scopes: vec![crate::CapabilityScope::Conversation],
                restricted_cloud_block: false,
            },
            created_at: chrono::Utc::now(),
        };

        let error = model
            .execute_guarded(
                ModelRequest {
                    task_id: route.task_id.expect("task id"),
                    session_id: uuid::Uuid::new_v4(),
                    user_input: "raw command".to_string(),
                    step_index: 0,
                    tool_results: Vec::new(),
                    first_party_tools: default_first_party_model_tools(),
                },
                &route,
            )
            .await
            .expect_err("provider error");

        let message = error.to_string();
        assert!(message.contains("HTTP 401"));
        assert!(!message.contains("test-token-value"));
        assert!(!message.contains("password"));
        assert!(!message.contains("raw command"));
    }

    fn test_chatgpt_route(
        config: &ChatGptProviderConfig,
        context_preview: &str,
    ) -> ModelRouteRecord {
        crate::ModelRouter::route(&crate::ModelRouteRequest {
            task_id: Some(uuid::Uuid::new_v4()),
            user_intent: "workspace planning".to_string(),
            sensitivity: crate::Sensitivity::Workspace,
            required_scopes: vec![crate::CapabilityScope::Conversation],
            granted_scopes: vec![
                crate::CapabilityScope::Conversation,
                crate::CapabilityScope::CloudModel,
            ],
            local_available: false,
            local_sufficient: false,
            provider_status: ProviderStatus::from_config(&ProviderConfig {
                local: LocalModelConfig {
                    enabled: false,
                    ..LocalModelConfig::default()
                },
                chatgpt: config.clone(),
            }),
            emergency_paused: false,
            approval: None,
            context_preview: context_preview.to_string(),
        })
    }
}
