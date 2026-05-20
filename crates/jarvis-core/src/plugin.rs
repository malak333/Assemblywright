use crate::{
    ApprovalDecision, ApprovalGrant, ApprovalStatus, CapabilityScope, JarvisError, JarvisResult,
    PermissionEngine, PolicyRequest, RiskTier, Sensitivity,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::Duration,
};

const DEFAULT_TIMEOUT_MS: u64 = 5_000;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginPermission {
    ReadWorkspace,
    WriteWorkspace,
    ReadMemory,
    WriteMemory,
    CallModel,
    ProactiveRun,
    Network,
    SystemStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginSource {
    FirstParty,
    ThirdParty,
    LocalDevelopment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginAccess {
    None,
    Read,
    Write,
    ReadWrite,
}

impl PluginAccess {
    fn can_read(self) -> bool {
        matches!(self, Self::Read | Self::ReadWrite)
    }

    fn can_write(self) -> bool {
        matches!(self, Self::Write | Self::ReadWrite)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationBehavior {
    NotSupported,
    Cooperative,
    Immediate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginTimeout {
    pub timeout_ms: u64,
    pub on_timeout: PluginTimeoutAction,
}

impl PluginTimeout {
    pub fn default_for_action() -> Self {
        Self {
            timeout_ms: DEFAULT_TIMEOUT_MS,
            on_timeout: PluginTimeoutAction::Cancel,
        }
    }

    fn duration(&self) -> Duration {
        Duration::from_millis(self.timeout_ms)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginTimeoutAction {
    Cancel,
    MarkFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonSchema {
    pub schema: Value,
}

impl JsonSchema {
    pub fn new(schema: Value) -> Self {
        Self { schema }
    }

    pub fn object(properties: Map<String, Value>, required: Vec<String>) -> Self {
        Self {
            schema: json!({
                "type": "object",
                "properties": properties,
                "required": required,
                "additionalProperties": false
            }),
        }
    }

    pub fn empty_object() -> Self {
        Self::object(Map::new(), Vec::new())
    }

    pub fn validate_schema(&self, label: &str) -> JarvisResult<()> {
        let schema_type = self
            .schema
            .get("type")
            .and_then(Value::as_str)
            .ok_or_else(|| JarvisError::Validation(format!("{label} schema must declare type")))?;

        if schema_type != "object" {
            return Err(JarvisError::Validation(format!(
                "{label} schema type must be object"
            )));
        }

        if let Some(properties) = self.schema.get("properties") {
            let properties = properties.as_object().ok_or_else(|| {
                JarvisError::Validation(format!("{label} schema properties must be an object"))
            })?;

            for (name, property) in properties {
                if property.get("type").and_then(Value::as_str).is_none() {
                    return Err(JarvisError::Validation(format!(
                        "{label} schema property {name} must declare type"
                    )));
                }
            }
        }

        if let Some(required) = self.schema.get("required") {
            required.as_array().ok_or_else(|| {
                JarvisError::Validation(format!("{label} schema required must be an array"))
            })?;
        }

        Ok(())
    }

    pub fn validate_value(&self, label: &str, value: &Value) -> JarvisResult<()> {
        self.validate_schema(label)?;

        let object = value
            .as_object()
            .ok_or_else(|| JarvisError::Validation(format!("{label} must be an object")))?;

        if let Some(required) = self.schema.get("required").and_then(Value::as_array) {
            for field in required {
                let field = field.as_str().ok_or_else(|| {
                    JarvisError::Validation(format!("{label} required fields must be strings"))
                })?;
                if !object.contains_key(field) {
                    return Err(JarvisError::Validation(format!(
                        "{label} missing required field {field}"
                    )));
                }
            }
        }

        if let Some(properties) = self.schema.get("properties").and_then(Value::as_object) {
            let allow_additional = self
                .schema
                .get("additionalProperties")
                .and_then(Value::as_bool)
                .unwrap_or(true);

            if !allow_additional {
                for field in object.keys() {
                    if !properties.contains_key(field) {
                        return Err(JarvisError::Validation(format!(
                            "{label} contains undeclared field {field}"
                        )));
                    }
                }
            }

            for (field, schema) in properties {
                if let Some(actual) = object.get(field) {
                    let expected_type =
                        schema.get("type").and_then(Value::as_str).ok_or_else(|| {
                            JarvisError::Validation(format!(
                                "{label} schema property {field} must declare type"
                            ))
                        })?;
                    if !json_type_matches(expected_type, actual) {
                        return Err(JarvisError::Validation(format!(
                            "{label} field {field} must be {expected_type}"
                        )));
                    }
                }
            }
        }

        Ok(())
    }
}

fn json_type_matches(expected_type: &str, actual: &Value) -> bool {
    match expected_type {
        "array" => actual.is_array(),
        "boolean" => actual.is_boolean(),
        "integer" => actual.as_i64().is_some() || actual.as_u64().is_some(),
        "number" => actual.is_number(),
        "null" => actual.is_null(),
        "object" => actual.is_object(),
        "string" => actual.is_string(),
        _ => false,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginActionManifest {
    pub name: String,
    pub description: String,
    pub permissions: Vec<PluginPermission>,
    pub risk_tier: RiskTier,
    pub input_schema: JsonSchema,
    pub output_schema: JsonSchema,
    pub proactive: bool,
    pub memory_access: PluginAccess,
    pub model_access: PluginAccess,
    pub audit_fields: Vec<String>,
    pub timeout: PluginTimeout,
    pub cancellation: CancellationBehavior,
}

impl PluginActionManifest {
    pub fn validate(&self, plugin_id: &str) -> JarvisResult<()> {
        validate_identifier(&self.name, "action name")?;
        validate_non_empty(&self.description, "action description")?;
        self.input_schema
            .validate_schema(&format!("{plugin_id}.{} input", self.name))?;
        self.output_schema
            .validate_schema(&format!("{plugin_id}.{} output", self.name))?;

        if self.timeout.timeout_ms == 0 {
            return Err(JarvisError::Validation(format!(
                "{plugin_id}.{} timeout must be greater than zero",
                self.name
            )));
        }

        if self.proactive && !self.permissions.contains(&PluginPermission::ProactiveRun) {
            return Err(JarvisError::Validation(format!(
                "{plugin_id}.{} proactive actions must request proactive_run permission",
                self.name
            )));
        }

        if self.memory_access.can_read()
            && !self.permissions.contains(&PluginPermission::ReadMemory)
        {
            return Err(JarvisError::Validation(format!(
                "{plugin_id}.{} memory read access requires read_memory permission",
                self.name
            )));
        }

        if self.memory_access.can_write()
            && !self.permissions.contains(&PluginPermission::WriteMemory)
        {
            return Err(JarvisError::Validation(format!(
                "{plugin_id}.{} memory write access requires write_memory permission",
                self.name
            )));
        }

        if self.model_access != PluginAccess::None
            && !self.permissions.contains(&PluginPermission::CallModel)
        {
            return Err(JarvisError::Validation(format!(
                "{plugin_id}.{} model access requires call_model permission",
                self.name
            )));
        }

        if self.risk_tier == RiskTier::Block {
            return Err(JarvisError::Validation(format!(
                "{plugin_id}.{} cannot register as blocked",
                self.name
            )));
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub source: PluginSource,
    pub author: String,
    pub actions: Vec<PluginActionManifest>,
}

impl PluginManifest {
    pub fn validate(&self) -> JarvisResult<()> {
        validate_identifier(&self.id, "plugin id")?;
        validate_non_empty(&self.name, "plugin name")?;
        validate_non_empty(&self.version, "plugin version")?;
        validate_non_empty(&self.author, "plugin author")?;

        if self.actions.is_empty() {
            return Err(JarvisError::Validation(format!(
                "{} must declare at least one action",
                self.id
            )));
        }

        let mut action_names = HashSet::new();
        for action in &self.actions {
            if !action_names.insert(action.name.as_str()) {
                return Err(JarvisError::Validation(format!(
                    "{} contains duplicate action {}",
                    self.id, action.name
                )));
            }
            action.validate(&self.id)?;
        }

        Ok(())
    }

    pub fn action(&self, name: &str) -> Option<&PluginActionManifest> {
        self.actions.iter().find(|action| action.name == name)
    }
}

fn validate_identifier(value: &str, label: &str) -> JarvisResult<()> {
    validate_non_empty(value, label)?;
    if !value.chars().all(|character| {
        character.is_ascii_lowercase()
            || character.is_ascii_digit()
            || character == '_'
            || character == '-'
    }) {
        return Err(JarvisError::Validation(format!(
            "{label} must use lowercase ascii letters, digits, hyphen, or underscore"
        )));
    }
    Ok(())
}

fn validate_non_empty(value: &str, label: &str) -> JarvisResult<()> {
    if value.trim().is_empty() {
        return Err(JarvisError::Validation(format!("{label} is required")));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCallRequest {
    pub plugin_id: String,
    pub action: String,
    pub input: Value,
    pub approval_status: ApprovalStatus,
    #[serde(default)]
    pub granted_scopes: Vec<CapabilityScope>,
    #[serde(default)]
    pub approval: Option<ApprovalGrant>,
    #[serde(default = "default_plugin_sensitivity")]
    pub sensitivity: Sensitivity,
    pub proactive: bool,
}

impl PluginCallRequest {
    pub fn reactive(plugin_id: impl Into<String>, action: impl Into<String>, input: Value) -> Self {
        Self {
            plugin_id: plugin_id.into(),
            action: action.into(),
            input,
            approval_status: ApprovalStatus::NotRequired,
            granted_scopes: Vec::new(),
            approval: None,
            sensitivity: Sensitivity::Public,
            proactive: false,
        }
    }

    pub fn with_granted_scopes(mut self, granted_scopes: Vec<CapabilityScope>) -> Self {
        self.granted_scopes = granted_scopes;
        self
    }

    pub fn with_approval(mut self, approval: ApprovalGrant) -> Self {
        self.approval_status = approval.status;
        self.approval = Some(approval);
        self
    }

    pub fn with_sensitivity(mut self, sensitivity: Sensitivity) -> Self {
        self.sensitivity = sensitivity;
        self
    }
}

fn default_plugin_sensitivity() -> Sensitivity {
    Sensitivity::Public
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCallMetadata {
    pub plugin_id: String,
    pub action: String,
    pub permissions: Vec<PluginPermission>,
    pub risk_tier: RiskTier,
    pub approval_required: bool,
    pub approval_status: ApprovalStatus,
    pub proactive: bool,
    pub memory_access: PluginAccess,
    pub model_access: PluginAccess,
    pub timeout_ms: u64,
    pub cancellation: CancellationBehavior,
    pub audit_fields: Vec<String>,
}

impl PluginCallMetadata {
    fn from_manifest(
        manifest: &PluginManifest,
        action: &PluginActionManifest,
        request: &PluginCallRequest,
    ) -> Self {
        Self {
            plugin_id: manifest.id.clone(),
            action: action.name.clone(),
            permissions: action.permissions.clone(),
            risk_tier: action.risk_tier,
            approval_required: action.risk_tier >= RiskTier::Confirm,
            approval_status: request.approval_status,
            proactive: request.proactive,
            memory_access: action.memory_access,
            model_access: action.model_access,
            timeout_ms: action.timeout.timeout_ms,
            cancellation: action.cancellation,
            audit_fields: action.audit_fields.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginCallStatus {
    Completed,
    ApprovalRequired,
    TimedOut,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginCallResult {
    pub status: PluginCallStatus,
    pub output: Value,
    pub metadata: PluginCallMetadata,
}

impl PluginCallResult {
    fn approval_required(metadata: PluginCallMetadata) -> Self {
        Self {
            status: PluginCallStatus::ApprovalRequired,
            output: json!({ "approval_required": true }),
            metadata,
        }
    }

    fn timed_out(metadata: PluginCallMetadata) -> Self {
        Self {
            status: PluginCallStatus::TimedOut,
            output: json!({ "timed_out": true }),
            metadata,
        }
    }
}

#[derive(Debug, Clone)]
pub struct CancellationSignal {
    cancelled: Arc<AtomicBool>,
}

impl CancellationSignal {
    pub fn new() -> Self {
        Self {
            cancelled: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }
}

impl Default for CancellationSignal {
    fn default() -> Self {
        Self::new()
    }
}

pub trait InProcessPlugin: Send + Sync + 'static {
    fn manifest(&self) -> PluginManifest;

    fn execute(
        &self,
        action: &PluginActionManifest,
        input: Value,
        cancellation: CancellationSignal,
    ) -> JarvisResult<Value>;
}

#[derive(Default)]
pub struct PluginHost {
    plugins: HashMap<String, Arc<dyn InProcessPlugin>>,
}

impl PluginHost {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_first_party_plugins() -> JarvisResult<Self> {
        let mut host = Self::new();
        host.register(EchoPlugin)?;
        host.register(StatusPlugin)?;
        Ok(host)
    }

    pub fn register(&mut self, plugin: impl InProcessPlugin) -> JarvisResult<()> {
        let manifest = plugin.manifest();
        manifest.validate()?;

        if self.plugins.contains_key(&manifest.id) {
            return Err(JarvisError::Validation(format!(
                "plugin {} is already registered",
                manifest.id
            )));
        }

        self.plugins.insert(manifest.id, Arc::new(plugin));
        Ok(())
    }

    pub fn manifests(&self) -> JarvisResult<Vec<PluginManifest>> {
        self.plugins
            .values()
            .map(|plugin| {
                let manifest = plugin.manifest();
                manifest.validate()?;
                Ok(manifest)
            })
            .collect()
    }

    pub fn manifest(&self, plugin_id: &str) -> JarvisResult<PluginManifest> {
        let plugin = self
            .plugins
            .get(plugin_id)
            .ok_or_else(|| JarvisError::Plugin(format!("plugin {plugin_id} is not registered")))?;
        let manifest = plugin.manifest();
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn execute(&self, request: PluginCallRequest) -> JarvisResult<PluginCallResult> {
        let plugin = Arc::clone(self.plugins.get(&request.plugin_id).ok_or_else(|| {
            JarvisError::Plugin(format!("plugin {} is not registered", request.plugin_id))
        })?);
        let manifest = plugin.manifest();
        manifest.validate()?;
        let action = manifest.action(&request.action).cloned().ok_or_else(|| {
            JarvisError::Plugin(format!(
                "plugin {} does not declare action {}",
                request.plugin_id, request.action
            ))
        })?;
        let mut metadata = PluginCallMetadata::from_manifest(&manifest, &action, &request);

        if request.proactive && !action.proactive {
            return Err(JarvisError::PolicyBlocked(format!(
                "{}.{} cannot run proactively",
                manifest.id, action.name
            )));
        }

        let policy_request = PolicyRequest {
            task_id: None,
            action: format!("{}.{}", manifest.id, action.name),
            requested_scopes: plugin_permission_scopes(&action.permissions),
            granted_scopes: request.granted_scopes.clone(),
            risk_tier: action.risk_tier,
            sensitivity: request.sensitivity,
            emergency_paused: false,
            approval: request.approval.clone(),
        };
        let policy = PermissionEngine::evaluate(&policy_request);
        metadata.approval_required = policy.decision == ApprovalDecision::RequireConfirmation;
        metadata.approval_status = policy.approval_status;

        if policy.decision == ApprovalDecision::Blocked {
            return Err(JarvisError::PolicyBlocked(policy.reason));
        }

        if policy.decision == ApprovalDecision::RequireConfirmation {
            return Ok(PluginCallResult::approval_required(metadata));
        }

        action.input_schema.validate_value(
            &format!("{}.{} input", manifest.id, action.name),
            &request.input,
        )?;

        let cancellation = CancellationSignal::new();
        let worker_cancellation = cancellation.clone();
        let input = request.input;
        let timeout = action.timeout.clone();
        let output_schema = action.output_schema.clone();
        let action_name = action.name.clone();
        let plugin_id = manifest.id.clone();

        let (sender, receiver) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let result = plugin.execute(&action, input, worker_cancellation);
            let _ = sender.send(result);
        });

        match receiver.recv_timeout(timeout.duration()) {
            Ok(Ok(output)) => {
                output_schema
                    .validate_value(&format!("{plugin_id}.{action_name} output"), &output)?;
                Ok(PluginCallResult {
                    status: PluginCallStatus::Completed,
                    output,
                    metadata,
                })
            }
            Ok(Err(error)) => Ok(PluginCallResult {
                status: PluginCallStatus::Failed,
                output: json!({ "error": error.to_string() }),
                metadata,
            }),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if timeout.on_timeout == PluginTimeoutAction::Cancel {
                    cancellation.cancel();
                }
                Ok(PluginCallResult::timed_out(metadata))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Ok(PluginCallResult {
                status: PluginCallStatus::Failed,
                output: json!({ "error": "plugin worker disconnected" }),
                metadata,
            }),
        }
    }
}

pub fn plugin_permission_scopes(permissions: &[PluginPermission]) -> Vec<CapabilityScope> {
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

#[derive(Debug, Clone)]
pub struct EchoPlugin;

impl InProcessPlugin for EchoPlugin {
    fn manifest(&self) -> PluginManifest {
        let mut input_properties = Map::new();
        input_properties.insert("message".to_string(), json!({ "type": "string" }));

        let mut output_properties = Map::new();
        output_properties.insert("message".to_string(), json!({ "type": "string" }));

        PluginManifest {
            id: "fake_echo".to_string(),
            name: "Fake Echo".to_string(),
            version: "0.1.0".to_string(),
            source: PluginSource::FirstParty,
            author: "Jarvis".to_string(),
            actions: vec![
                PluginActionManifest {
                    name: "echo".to_string(),
                    description: "Return the provided message for host contract testing."
                        .to_string(),
                    permissions: Vec::new(),
                    risk_tier: RiskTier::Low,
                    input_schema: JsonSchema::object(
                        input_properties.clone(),
                        vec!["message".to_string()],
                    ),
                    output_schema: JsonSchema::object(
                        output_properties.clone(),
                        vec!["message".to_string()],
                    ),
                    proactive: false,
                    memory_access: PluginAccess::None,
                    model_access: PluginAccess::None,
                    audit_fields: vec!["message".to_string()],
                    timeout: PluginTimeout::default_for_action(),
                    cancellation: CancellationBehavior::Cooperative,
                },
                PluginActionManifest {
                    name: "approval_echo".to_string(),
                    description:
                        "Approval-gated echo scaffold for high-risk host contract testing."
                            .to_string(),
                    permissions: vec![PluginPermission::WriteWorkspace],
                    risk_tier: RiskTier::Confirm,
                    input_schema: JsonSchema::object(input_properties, vec!["message".to_string()]),
                    output_schema: JsonSchema::object(
                        output_properties,
                        vec!["message".to_string()],
                    ),
                    proactive: false,
                    memory_access: PluginAccess::None,
                    model_access: PluginAccess::None,
                    audit_fields: vec!["message".to_string()],
                    timeout: PluginTimeout::default_for_action(),
                    cancellation: CancellationBehavior::Cooperative,
                },
            ],
        }
    }

    fn execute(
        &self,
        _action: &PluginActionManifest,
        input: Value,
        cancellation: CancellationSignal,
    ) -> JarvisResult<Value> {
        if cancellation.is_cancelled() {
            return Err(JarvisError::Plugin("echo cancelled".to_string()));
        }

        Ok(json!({
            "message": input.get("message").and_then(Value::as_str).unwrap_or_default()
        }))
    }
}

#[derive(Debug, Clone)]
pub struct StatusPlugin;

impl InProcessPlugin for StatusPlugin {
    fn manifest(&self) -> PluginManifest {
        let mut output_properties = Map::new();
        output_properties.insert("status".to_string(), json!({ "type": "string" }));
        output_properties.insert("plugin_count".to_string(), json!({ "type": "integer" }));

        PluginManifest {
            id: "fake_status".to_string(),
            name: "Fake Status".to_string(),
            version: "0.1.0".to_string(),
            source: PluginSource::FirstParty,
            author: "Jarvis".to_string(),
            actions: vec![PluginActionManifest {
                name: "status".to_string(),
                description: "Report deterministic first-party host status for contract testing."
                    .to_string(),
                permissions: vec![
                    PluginPermission::SystemStatus,
                    PluginPermission::ProactiveRun,
                ],
                risk_tier: RiskTier::Notify,
                input_schema: JsonSchema::empty_object(),
                output_schema: JsonSchema::object(output_properties, vec!["status".to_string()]),
                proactive: true,
                memory_access: PluginAccess::None,
                model_access: PluginAccess::None,
                audit_fields: vec!["status".to_string()],
                timeout: PluginTimeout::default_for_action(),
                cancellation: CancellationBehavior::Cooperative,
            }],
        }
    }

    fn execute(
        &self,
        _action: &PluginActionManifest,
        _input: Value,
        cancellation: CancellationSignal,
    ) -> JarvisResult<Value> {
        if cancellation.is_cancelled() {
            return Err(JarvisError::Plugin("status cancelled".to_string()));
        }

        Ok(json!({
            "status": "ok",
            "plugin_count": 2
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{thread, time::Duration};

    #[test]
    fn validates_first_party_manifests() {
        let host = PluginHost::with_first_party_plugins().expect("host should build");
        let manifests = host.manifests().expect("manifests should validate");

        assert_eq!(manifests.len(), 2);
        assert!(manifests.iter().any(|manifest| manifest.id == "fake_echo"));
        assert!(manifests
            .iter()
            .any(|manifest| manifest.id == "fake_status"));
    }

    #[test]
    fn rejects_manifest_when_memory_access_lacks_permission() {
        let mut manifest = EchoPlugin.manifest();
        manifest.actions[0].memory_access = PluginAccess::Read;

        let error = manifest.validate().expect_err("manifest should fail");
        assert!(error
            .to_string()
            .contains("memory read access requires read_memory permission"));
    }

    #[test]
    fn rejects_manifest_with_duplicate_action_names() {
        let mut manifest = EchoPlugin.manifest();
        manifest.actions.push(manifest.actions[0].clone());

        let error = manifest.validate().expect_err("manifest should fail");
        assert!(error.to_string().contains("duplicate action echo"));
    }

    #[test]
    fn echo_plugin_round_trips_input_and_metadata() {
        let host = PluginHost::with_first_party_plugins().expect("host should build");
        let result = host
            .execute(
                PluginCallRequest::reactive("fake_echo", "echo", json!({ "message": "hello" }))
                    .with_granted_scopes(vec![CapabilityScope::PluginRun]),
            )
            .expect("echo should execute");

        assert_eq!(result.status, PluginCallStatus::Completed);
        assert_eq!(result.output, json!({ "message": "hello" }));
        assert_eq!(result.metadata.risk_tier, RiskTier::Low);
        assert!(!result.metadata.approval_required);
        assert_eq!(result.metadata.timeout_ms, DEFAULT_TIMEOUT_MS);
        assert_eq!(
            result.metadata.cancellation,
            CancellationBehavior::Cooperative
        );
    }

    #[test]
    fn status_plugin_allows_proactive_notify_runs() {
        let host = PluginHost::with_first_party_plugins().expect("host should build");
        let mut request = PluginCallRequest::reactive("fake_status", "status", json!({}));
        request.proactive = true;
        request.granted_scopes = plugin_permission_scopes(&[
            PluginPermission::SystemStatus,
            PluginPermission::ProactiveRun,
        ]);

        let result = host.execute(request).expect("status should execute");

        assert_eq!(result.status, PluginCallStatus::Completed);
        assert_eq!(result.output["status"], "ok");
        assert_eq!(result.metadata.risk_tier, RiskTier::Notify);
        assert_eq!(
            result.metadata.permissions,
            vec![
                PluginPermission::SystemStatus,
                PluginPermission::ProactiveRun
            ]
        );
        assert!(result.metadata.proactive);
    }

    #[test]
    fn schema_validation_blocks_invalid_input() {
        let host = PluginHost::with_first_party_plugins().expect("host should build");
        let error = host
            .execute(
                PluginCallRequest::reactive("fake_echo", "echo", json!({ "message": 42 }))
                    .with_granted_scopes(vec![CapabilityScope::PluginRun]),
            )
            .expect_err("invalid input should fail");

        assert!(error
            .to_string()
            .contains("fake_echo.echo input field message must be string"));
    }

    #[test]
    fn missing_granted_scope_blocks_before_execution() {
        let host = PluginHost::with_first_party_plugins().expect("host should build");
        let error = host
            .execute(PluginCallRequest::reactive(
                "fake_echo",
                "echo",
                json!({ "message": "hello" }),
            ))
            .expect_err("missing plugin_run grant should block execution");

        assert!(error
            .to_string()
            .contains("requested capability scope is not granted"));
    }

    #[test]
    fn private_sensitivity_requires_approval_even_for_low_risk_action() {
        let host = PluginHost::with_first_party_plugins().expect("host should build");
        let result = host
            .execute(
                PluginCallRequest::reactive("fake_echo", "echo", json!({ "message": "private" }))
                    .with_granted_scopes(vec![CapabilityScope::PluginRun])
                    .with_sensitivity(Sensitivity::Private),
            )
            .expect("private low-risk action should require approval");

        assert_eq!(result.status, PluginCallStatus::ApprovalRequired);
        assert_eq!(result.metadata.approval_status, ApprovalStatus::Pending);
    }

    #[test]
    fn confirm_risk_returns_approval_metadata_without_executing() {
        let mut host = PluginHost::new();
        host.register(ApprovalPlugin)
            .expect("register approval plugin");

        let result = host
            .execute(
                PluginCallRequest::reactive(
                    "approval_plugin",
                    "needs_approval",
                    json!({ "message": "hello" }),
                )
                .with_granted_scopes(plugin_permission_scopes(&[
                    PluginPermission::WriteWorkspace,
                ])),
            )
            .expect("approval result should be returned");

        assert_eq!(result.status, PluginCallStatus::ApprovalRequired);
        assert!(result.metadata.approval_required);
        assert_eq!(result.metadata.approval_status, ApprovalStatus::Pending);
    }

    #[test]
    fn approved_status_without_grant_still_requires_approval() {
        let mut host = PluginHost::new();
        host.register(ApprovalPlugin)
            .expect("register approval plugin");
        let mut request = PluginCallRequest::reactive(
            "approval_plugin",
            "needs_approval",
            json!({ "message": "status only" }),
        );
        request.approval_status = ApprovalStatus::Approved;
        request.granted_scopes = plugin_permission_scopes(&[PluginPermission::WriteWorkspace]);

        let result = host
            .execute(request)
            .expect("approval requirement should be returned");

        assert_eq!(result.status, PluginCallStatus::ApprovalRequired);
        assert_eq!(result.metadata.approval_status, ApprovalStatus::Pending);
    }

    #[test]
    fn valid_approval_grant_executes_confirm_risk() {
        let mut host = PluginHost::new();
        host.register(ApprovalPlugin)
            .expect("register approval plugin");
        let request = PluginCallRequest::reactive(
            "approval_plugin",
            "needs_approval",
            json!({ "message": "approved" }),
        )
        .with_granted_scopes(plugin_permission_scopes(&[
            PluginPermission::WriteWorkspace,
        ]))
        .with_approval(ApprovalGrant::approved(plugin_permission_scopes(&[
            PluginPermission::WriteWorkspace,
        ])));

        let result = host.execute(request).expect("approved call should execute");

        assert_eq!(result.status, PluginCallStatus::Completed);
        assert_eq!(result.output, json!({ "message": "approved" }));
        assert_eq!(result.metadata.approval_status, ApprovalStatus::Approved);
    }

    #[test]
    fn timeout_cancels_and_returns_timeout_status() {
        let mut host = PluginHost::new();
        host.register(SlowPlugin).expect("register slow plugin");

        let result = host
            .execute(
                PluginCallRequest::reactive("slow_plugin", "sleep", json!({}))
                    .with_granted_scopes(vec![CapabilityScope::PluginRun]),
            )
            .expect("timeout should return result");

        assert_eq!(result.status, PluginCallStatus::TimedOut);
        assert_eq!(result.output, json!({ "timed_out": true }));
        assert_eq!(result.metadata.approval_status, ApprovalStatus::NotRequired);
        assert_eq!(result.metadata.timeout_ms, 10);
        assert_eq!(
            result.metadata.cancellation,
            CancellationBehavior::Cooperative
        );
    }

    struct ApprovalPlugin;

    impl InProcessPlugin for ApprovalPlugin {
        fn manifest(&self) -> PluginManifest {
            let mut properties = Map::new();
            properties.insert("message".to_string(), json!({ "type": "string" }));
            PluginManifest {
                id: "approval_plugin".to_string(),
                name: "Approval Plugin".to_string(),
                version: "0.1.0".to_string(),
                source: PluginSource::FirstParty,
                author: "Jarvis".to_string(),
                actions: vec![PluginActionManifest {
                    name: "needs_approval".to_string(),
                    description: "Requires approval before execution.".to_string(),
                    permissions: vec![PluginPermission::WriteWorkspace],
                    risk_tier: RiskTier::Confirm,
                    input_schema: JsonSchema::object(
                        properties.clone(),
                        vec!["message".to_string()],
                    ),
                    output_schema: JsonSchema::object(properties, vec!["message".to_string()]),
                    proactive: false,
                    memory_access: PluginAccess::None,
                    model_access: PluginAccess::None,
                    audit_fields: vec!["message".to_string()],
                    timeout: PluginTimeout::default_for_action(),
                    cancellation: CancellationBehavior::Cooperative,
                }],
            }
        }

        fn execute(
            &self,
            _action: &PluginActionManifest,
            input: Value,
            _cancellation: CancellationSignal,
        ) -> JarvisResult<Value> {
            Ok(input)
        }
    }

    struct SlowPlugin;

    impl InProcessPlugin for SlowPlugin {
        fn manifest(&self) -> PluginManifest {
            PluginManifest {
                id: "slow_plugin".to_string(),
                name: "Slow Plugin".to_string(),
                version: "0.1.0".to_string(),
                source: PluginSource::FirstParty,
                author: "Jarvis".to_string(),
                actions: vec![PluginActionManifest {
                    name: "sleep".to_string(),
                    description: "Sleeps longer than its timeout.".to_string(),
                    permissions: Vec::new(),
                    risk_tier: RiskTier::Low,
                    input_schema: JsonSchema::empty_object(),
                    output_schema: JsonSchema::empty_object(),
                    proactive: false,
                    memory_access: PluginAccess::None,
                    model_access: PluginAccess::None,
                    audit_fields: Vec::new(),
                    timeout: PluginTimeout {
                        timeout_ms: 10,
                        on_timeout: PluginTimeoutAction::Cancel,
                    },
                    cancellation: CancellationBehavior::Cooperative,
                }],
            }
        }

        fn execute(
            &self,
            _action: &PluginActionManifest,
            _input: Value,
            cancellation: CancellationSignal,
        ) -> JarvisResult<Value> {
            for _ in 0..20 {
                if cancellation.is_cancelled() {
                    return Err(JarvisError::Plugin("slow cancelled".to_string()));
                }
                thread::sleep(Duration::from_millis(5));
            }
            Ok(json!({}))
        }
    }
}
