use crate::{
    ApprovalDecision, ApprovalGrant, ApprovalStatus, CapabilityScope, JarvisError, JarvisResult,
    PermissionEngine, PolicyRequest, RiskTier, Sensitivity,
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
#[cfg(unix)]
use rustix::{
    io::Errno,
    process::{
        kill_process_group, test_kill_process_group, waitid, Pid, Signal, WaitId, WaitIdOptions,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    thread,
    time::{Duration, Instant},
};

const DEFAULT_TIMEOUT_MS: u64 = 5_000;
const MAX_TIMEOUT_MS: u64 = 60_000;
const LOCAL_MANIFEST_SCHEMA_VERSION: u16 = 1;
const MAX_PLUGIN_PROGRESS_EVENTS: usize = 32;
const MAX_PLUGIN_PROGRESS_LINE_BYTES: usize = 4_096;
const MAX_PLUGIN_PROGRESS_STAGE_CHARS: usize = 64;
const MAX_PLUGIN_PROGRESS_MESSAGE_CHARS: usize = 240;
const MAX_PLUGIN_STDOUT_BYTES: usize = 1024 * 1024;
const MAX_PLUGIN_STDERR_BYTES: usize = 256 * 1024;
const MAX_PLUGIN_SOURCE_TREE_FILES: usize = 4_096;
const MAX_PLUGIN_SOURCE_TREE_ENTRIES: usize = 8_192;
const MAX_PLUGIN_SOURCE_TREE_DEPTH: usize = 64;
const MAX_PLUGIN_SOURCE_TREE_BYTES: u64 = 64 * 1024 * 1024;
#[cfg(unix)]
const SUBPROCESS_TERMINATION_GRACE: Duration = Duration::from_millis(100);
const SUBPROCESS_IO_JOIN_TIMEOUT: Duration = Duration::from_secs(1);
#[cfg(unix)]
const SUBPROCESS_GROUP_KILL_CONFIRM_TIMEOUT: Duration = Duration::from_millis(500);

#[cfg(unix)]
type SubprocessGroupId = Pid;
#[cfg(not(unix))]
type SubprocessGroupId = u32;

struct SubprocessExitObservation {
    // Unix uses waitid(WNOWAIT) so cleanup can keep the PID/PGID pinned until
    // the final group signal. Other platforms retain the status reaped by
    // Child::try_wait so it is not lost during cleanup.
    reaped_status: Option<ExitStatus>,
}

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
    LocalSubprocess,
    LocalWasm,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginNetworkAccess {
    #[serde(default)]
    pub mode: PluginNetworkAccessMode,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_hosts: Vec<String>,
}

impl Default for PluginNetworkAccess {
    fn default() -> Self {
        Self {
            mode: PluginNetworkAccessMode::None,
            allowed_hosts: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginNetworkAccessMode {
    #[default]
    None,
    DeclaredHosts,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    #[serde(default)]
    pub network_access: PluginNetworkAccess,
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
        if self.timeout.timeout_ms > MAX_TIMEOUT_MS {
            return Err(JarvisError::Validation(format!(
                "{plugin_id}.{} timeout cannot exceed {MAX_TIMEOUT_MS}ms",
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

        self.validate_network_access(plugin_id)?;

        if self.risk_tier == RiskTier::Block {
            return Err(JarvisError::Validation(format!(
                "{plugin_id}.{} cannot register as blocked",
                self.name
            )));
        }

        Ok(())
    }

    fn validate_network_access(&self, plugin_id: &str) -> JarvisResult<()> {
        let has_network_permission = self.permissions.contains(&PluginPermission::Network);
        match self.network_access.mode {
            PluginNetworkAccessMode::None => {
                if !self.network_access.allowed_hosts.is_empty() {
                    return Err(JarvisError::Validation(format!(
                        "{plugin_id}.{} network_access none cannot declare allowed_hosts",
                        self.name
                    )));
                }
                if has_network_permission {
                    return Err(JarvisError::Validation(format!(
                        "{plugin_id}.{} network permission requires network_access declared_hosts",
                        self.name
                    )));
                }
            }
            PluginNetworkAccessMode::DeclaredHosts => {
                if !has_network_permission {
                    return Err(JarvisError::Validation(format!(
                        "{plugin_id}.{} network_access declared_hosts requires network permission",
                        self.name
                    )));
                }
                if self.network_access.allowed_hosts.is_empty() {
                    return Err(JarvisError::Validation(format!(
                        "{plugin_id}.{} network_access declared_hosts requires allowed_hosts",
                        self.name
                    )));
                }
                let mut normalized_hosts = HashSet::new();
                for host in &self.network_access.allowed_hosts {
                    validate_network_host(host).map_err(|err| {
                        JarvisError::Validation(format!(
                            "{plugin_id}.{} network host {host:?} is invalid: {err}",
                            self.name
                        ))
                    })?;
                    let normalized_host = host.to_ascii_lowercase();
                    if !normalized_hosts.insert(normalized_host) {
                        return Err(JarvisError::Validation(format!(
                            "{plugin_id}.{} network host {host:?} duplicates another allowed_host after case folding",
                            self.name
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    #[serde(default = "default_manifest_schema_version")]
    pub manifest_schema_version: u16,
    pub id: String,
    pub name: String,
    pub version: String,
    pub source: PluginSource,
    pub author: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subprocess: Option<PluginSubprocessManifest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wasm: Option<PluginWasmManifest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher_signature: Option<PluginPublisherSignature>,
    pub actions: Vec<PluginActionManifest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginWasmManifest {
    pub module: String,
    pub abi: PluginWasmAbi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginWasmAbi {
    JarvisJsonV1,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginPublisherSignature {
    pub scheme: String,
    pub public_key: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginSubprocessManifest {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub stdin: PluginSubprocessStream,
    pub stdout: PluginSubprocessStream,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginSubprocessStream {
    Json,
}

impl PluginManifest {
    pub fn validate(&self) -> JarvisResult<()> {
        validate_identifier(&self.id, "plugin id")?;
        validate_non_empty(&self.name, "plugin name")?;
        validate_non_empty(&self.version, "plugin version")?;
        validate_non_empty(&self.author, "plugin author")?;

        if self.manifest_schema_version != LOCAL_MANIFEST_SCHEMA_VERSION {
            return Err(JarvisError::Validation(format!(
                "{} manifest_schema_version must be {}",
                self.id, LOCAL_MANIFEST_SCHEMA_VERSION
            )));
        }

        if self.actions.is_empty() {
            return Err(JarvisError::Validation(format!(
                "{} must declare at least one action",
                self.id
            )));
        }

        if let Some(signature) = &self.publisher_signature {
            signature.validate(&self.id)?;
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

    pub fn validate_local_install(&self, manifest_path: &Path) -> JarvisResult<PathBuf> {
        self.validate()?;

        if self.source == PluginSource::FirstParty {
            return Err(JarvisError::Validation(format!(
                "{} local installs cannot claim first_party source",
                self.id
            )));
        }
        if self.source == PluginSource::LocalSubprocess && self.subprocess.is_none() {
            return Err(JarvisError::Validation(format!(
                "{} local_subprocess manifests must declare subprocess",
                self.id
            )));
        }
        if self.source != PluginSource::LocalSubprocess && self.subprocess.is_some() {
            return Err(JarvisError::Validation(format!(
                "{} subprocess config requires local_subprocess source",
                self.id
            )));
        }
        if self.source == PluginSource::LocalWasm && self.wasm.is_none() {
            return Err(JarvisError::Validation(format!(
                "{} local_wasm manifests must declare wasm",
                self.id
            )));
        }
        if self.source != PluginSource::LocalWasm && self.wasm.is_some() {
            return Err(JarvisError::Validation(format!(
                "{} wasm config requires local_wasm source",
                self.id
            )));
        }

        let source_path = self.source_path.as_deref().ok_or_else(|| {
            JarvisError::Validation(format!("{} local install requires source_path", self.id))
        })?;
        let declared_source = Path::new(source_path);
        if !declared_source.is_absolute() {
            return Err(JarvisError::Validation(format!(
                "{} source_path must be absolute",
                self.id
            )));
        }
        if declared_source
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(JarvisError::Validation(format!(
                "{} source_path cannot contain parent directory components",
                self.id
            )));
        }

        let canonical_source = fs::canonicalize(declared_source).map_err(|err| {
            JarvisError::Validation(format!("{} source_path is not readable: {err}", self.id))
        })?;
        if !canonical_source.is_dir() {
            return Err(JarvisError::Validation(format!(
                "{} source_path must be a directory",
                self.id
            )));
        }

        let canonical_manifest = fs::canonicalize(manifest_path).map_err(|err| {
            JarvisError::Validation(format!("{} manifest path is not readable: {err}", self.id))
        })?;
        if !canonical_manifest.is_file() {
            return Err(JarvisError::Validation(format!(
                "{} manifest path must be a file",
                self.id
            )));
        }
        if !canonical_manifest.starts_with(&canonical_source) {
            return Err(JarvisError::Validation(format!(
                "{} manifest must live under source_path",
                self.id
            )));
        }
        if let Some(subprocess) = &self.subprocess {
            subprocess.validate(&self.id, &canonical_source)?;
        }
        if let Some(wasm) = &self.wasm {
            wasm.validate(&self.id, &canonical_source)?;
            for action in &self.actions {
                if action.risk_tier != RiskTier::Low
                    || action.proactive
                    || !action.permissions.is_empty()
                    || action.memory_access != PluginAccess::None
                    || action.model_access != PluginAccess::None
                    || action.network_access != PluginNetworkAccess::default()
                {
                    return Err(JarvisError::Validation(format!(
                        "{}.{} local_wasm actions must be low-risk, non-proactive, permissionless, and have no memory, model, or network access",
                        self.id, action.name
                    )));
                }
            }
        }

        Ok(canonical_source)
    }

    pub fn action(&self, name: &str) -> Option<&PluginActionManifest> {
        self.actions.iter().find(|action| action.name == name)
    }

    pub fn verify_publisher_signature(&self, trusted_public_key: &str) -> JarvisResult<()> {
        let signature = self.publisher_signature.as_ref().ok_or_else(|| {
            JarvisError::Validation(format!(
                "{} publisher signature is required for signature verification",
                self.id
            ))
        })?;
        signature.verify(
            &self.id,
            trusted_public_key,
            &self.publisher_signature_payload()?,
        )
    }

    fn publisher_signature_payload(&self) -> JarvisResult<Vec<u8>> {
        let mut unsigned = self.clone();
        unsigned.publisher_signature = None;
        unsigned.source_path = None;
        serde_json::to_vec(&unsigned).map_err(|err| {
            JarvisError::Validation(format!("{} publisher signature payload: {err}", self.id))
        })
    }
}

impl PluginWasmManifest {
    pub fn validate(&self, plugin_id: &str, source_path: &Path) -> JarvisResult<PathBuf> {
        validate_non_empty(&self.module, "wasm module")?;
        if self.module.contains('\0') {
            return Err(JarvisError::Validation(format!(
                "{plugin_id} wasm module cannot contain NUL"
            )));
        }
        let module = Path::new(&self.module);
        if module.is_absolute()
            || module
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(JarvisError::Validation(format!(
                "{plugin_id} wasm module must be a relative path without parent components"
            )));
        }
        let canonical = fs::canonicalize(source_path.join(module)).map_err(|err| {
            JarvisError::Validation(format!("{plugin_id} wasm module is not readable: {err}"))
        })?;
        if !canonical.starts_with(source_path) || !canonical.is_file() {
            return Err(JarvisError::Validation(format!(
                "{plugin_id} wasm module must be a file under source_path"
            )));
        }
        Ok(canonical)
    }
}

impl PluginPublisherSignature {
    pub const ED25519_V1: &'static str = "ed25519-v1";

    pub fn validate(&self, plugin_id: &str) -> JarvisResult<()> {
        validate_non_empty(&self.scheme, "publisher signature scheme")?;
        validate_non_empty(&self.public_key, "publisher signature public_key")?;
        validate_non_empty(&self.signature, "publisher signature")?;
        if self.scheme != Self::ED25519_V1 {
            return Err(JarvisError::Validation(format!(
                "{plugin_id} publisher signature scheme must be {}",
                Self::ED25519_V1
            )));
        }
        Ok(())
    }

    pub fn verify(
        &self,
        plugin_id: &str,
        trusted_public_key: &str,
        payload: &[u8],
    ) -> JarvisResult<()> {
        self.validate(plugin_id)?;
        let embedded_key = decode_fixed_base64::<32>(
            &self.public_key,
            "publisher signature public_key",
            plugin_id,
        )?;
        let trusted_key =
            decode_fixed_base64::<32>(trusted_public_key, "trusted_public_key", plugin_id)?;
        if embedded_key != trusted_key {
            return Err(JarvisError::Validation(format!(
                "{plugin_id} publisher signature public_key does not match trusted_public_key"
            )));
        }
        let signature_bytes =
            decode_fixed_base64::<64>(&self.signature, "publisher signature", plugin_id)?;
        let verifying_key = VerifyingKey::from_bytes(&embedded_key).map_err(|err| {
            JarvisError::Validation(format!(
                "{plugin_id} publisher public_key is invalid: {err}"
            ))
        })?;
        let signature = Signature::from_bytes(&signature_bytes);
        verifying_key.verify(payload, &signature).map_err(|err| {
            JarvisError::Validation(format!(
                "{plugin_id} publisher signature verification failed: {err}"
            ))
        })
    }
}

impl PluginSubprocessManifest {
    pub fn validate(&self, plugin_id: &str, source_path: &Path) -> JarvisResult<PathBuf> {
        validate_non_empty(&self.command, "subprocess command")?;
        if self.command.contains('\0') {
            return Err(JarvisError::Validation(format!(
                "{plugin_id} subprocess command cannot contain NUL"
            )));
        }
        for arg in &self.args {
            if arg.contains('\0') {
                return Err(JarvisError::Validation(format!(
                    "{plugin_id} subprocess args cannot contain NUL"
                )));
            }
        }

        let command_path = Path::new(&self.command);
        if command_path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(JarvisError::Validation(format!(
                "{plugin_id} subprocess command cannot contain parent directory components"
            )));
        }
        let declared_command = if command_path.is_absolute() {
            command_path.to_path_buf()
        } else {
            source_path.join(command_path)
        };
        let canonical_command = fs::canonicalize(&declared_command).map_err(|err| {
            JarvisError::Validation(format!(
                "{plugin_id} subprocess command is not readable: {err}"
            ))
        })?;
        if !canonical_command.starts_with(source_path) {
            return Err(JarvisError::Validation(format!(
                "{plugin_id} subprocess command must live under source_path"
            )));
        }
        if !canonical_command.is_file() {
            return Err(JarvisError::Validation(format!(
                "{plugin_id} subprocess command must be a file"
            )));
        }

        Ok(canonical_command)
    }
}

fn default_manifest_schema_version() -> u16 {
    LOCAL_MANIFEST_SCHEMA_VERSION
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SemanticVersionIdentifier {
    Numeric(String),
    Text(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SemanticVersion {
    core: [String; 3],
    prerelease: Vec<SemanticVersionIdentifier>,
}

impl SemanticVersion {
    fn parse(value: &str) -> JarvisResult<Self> {
        let (without_build, build) = value
            .split_once('+')
            .map_or((value, None), |(version, build)| (version, Some(build)));
        if without_build.contains('+') || build.is_some_and(|build| build.contains('+')) {
            return Err(JarvisError::Validation(format!(
                "plugin version must be valid SemVer 2.0.0: {value}"
            )));
        }
        if let Some(build) = build {
            validate_semantic_identifiers(build, false, value)?;
        }
        let (core, prerelease) = without_build
            .split_once('-')
            .map_or((without_build, None), |(core, prerelease)| {
                (core, Some(prerelease))
            });
        let mut components = core.split('.');
        let major = parse_semantic_core_number(components.next(), value)?;
        let minor = parse_semantic_core_number(components.next(), value)?;
        let patch = parse_semantic_core_number(components.next(), value)?;
        if components.next().is_some() {
            return Err(JarvisError::Validation(format!(
                "plugin version must be valid SemVer 2.0.0: {value}"
            )));
        }
        let prerelease = prerelease
            .map(|prerelease| parse_semantic_prerelease(prerelease, value))
            .transpose()?
            .unwrap_or_default();
        Ok(Self {
            core: [major, minor, patch],
            prerelease,
        })
    }
}

fn parse_semantic_core_number(value: Option<&str>, full: &str) -> JarvisResult<String> {
    let value = value.ok_or_else(|| {
        JarvisError::Validation(format!("plugin version must be valid SemVer 2.0.0: {full}"))
    })?;
    if value.is_empty()
        || !value.bytes().all(|byte| byte.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return Err(JarvisError::Validation(format!(
            "plugin version must be valid SemVer 2.0.0: {full}"
        )));
    }
    Ok(value.to_string())
}

fn validate_semantic_identifiers(
    value: &str,
    reject_numeric_leading_zero: bool,
    full: &str,
) -> JarvisResult<()> {
    if value.is_empty() {
        return Err(JarvisError::Validation(format!(
            "plugin version must be valid SemVer 2.0.0: {full}"
        )));
    }
    for identifier in value.split('.') {
        if identifier.is_empty()
            || !identifier
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            || (reject_numeric_leading_zero
                && identifier.bytes().all(|byte| byte.is_ascii_digit())
                && identifier.len() > 1
                && identifier.starts_with('0'))
        {
            return Err(JarvisError::Validation(format!(
                "plugin version must be valid SemVer 2.0.0: {full}"
            )));
        }
    }
    Ok(())
}

fn parse_semantic_prerelease(
    value: &str,
    full: &str,
) -> JarvisResult<Vec<SemanticVersionIdentifier>> {
    validate_semantic_identifiers(value, true, full)?;
    value
        .split('.')
        .map(|identifier| {
            if identifier.bytes().all(|byte| byte.is_ascii_digit()) {
                Ok(SemanticVersionIdentifier::Numeric(identifier.to_string()))
            } else {
                Ok(SemanticVersionIdentifier::Text(identifier.to_string()))
            }
        })
        .collect()
}

fn compare_semantic_versions(
    left: &SemanticVersion,
    right: &SemanticVersion,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    for (left, right) in left.core.iter().zip(&right.core) {
        match compare_semantic_numeric_identifier(left, right) {
            Ordering::Equal => {}
            ordering => return ordering,
        }
    }
    match (left.prerelease.is_empty(), right.prerelease.is_empty()) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        (false, false) => {}
    }
    for (left, right) in left.prerelease.iter().zip(&right.prerelease) {
        let ordering = match (left, right) {
            (
                SemanticVersionIdentifier::Numeric(left),
                SemanticVersionIdentifier::Numeric(right),
            ) => compare_semantic_numeric_identifier(left, right),
            (SemanticVersionIdentifier::Numeric(_), SemanticVersionIdentifier::Text(_)) => {
                Ordering::Less
            }
            (SemanticVersionIdentifier::Text(_), SemanticVersionIdentifier::Numeric(_)) => {
                Ordering::Greater
            }
            (SemanticVersionIdentifier::Text(left), SemanticVersionIdentifier::Text(right)) => {
                left.cmp(right)
            }
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    left.prerelease.len().cmp(&right.prerelease.len())
}

fn compare_semantic_numeric_identifier(left: &str, right: &str) -> std::cmp::Ordering {
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

pub(crate) fn require_valid_semantic_version(version: &str) -> JarvisResult<()> {
    SemanticVersion::parse(version).map(|_| ())
}

pub(crate) fn require_strictly_newer_semantic_version(
    current: &str,
    candidate: &str,
) -> JarvisResult<()> {
    let current_version = SemanticVersion::parse(current)?;
    let candidate_version = SemanticVersion::parse(candidate)?;
    if compare_semantic_versions(&candidate_version, &current_version)
        != std::cmp::Ordering::Greater
    {
        return Err(JarvisError::Validation(format!(
            "installed plugin update requires a strictly newer semantic version than {current}"
        )));
    }
    Ok(())
}

pub(crate) fn require_installed_plugin_update_version(
    current: &str,
    candidate: &str,
) -> JarvisResult<()> {
    SemanticVersion::parse(candidate)?;
    if SemanticVersion::parse(current).is_err() {
        // A persisted pre-SemVer record may cross this boundary once. Once
        // stored, the valid candidate makes every later update strictly
        // ordered by SemVer precedence.
        return Ok(());
    }
    require_strictly_newer_semantic_version(current, candidate)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPlugin {
    pub manifest: PluginManifest,
    pub source_path: String,
    pub provenance: InstalledPluginProvenance,
    pub execution_enabled: bool,
    #[serde(default)]
    pub execution_grant: InstalledPluginExecutionGrant,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledPluginProvenance {
    pub provenance_schema_version: u16,
    pub capture_method: String,
    pub manifest_path: String,
    pub manifest_sha256: String,
    pub source_path: String,
    pub source_path_canonicalized: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_tree_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_tree_file_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subprocess_command_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subprocess_command_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wasm_module_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wasm_module_sha256: Option<String>,
    pub captured_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_verified_at: Option<DateTime<Utc>>,
    pub integrity_status: InstalledPluginIntegrityStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin_claim: Option<String>,
    pub origin_claim_verified: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstalledPluginIntegrityStatus {
    NotVerified,
    MatchesInstallSnapshot,
    ChangedSinceInstall,
    MissingFile,
    InvalidManifest,
}

impl InstalledPluginIntegrityStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotVerified => "not_verified",
            Self::MatchesInstallSnapshot => "matches_install_snapshot",
            Self::ChangedSinceInstall => "changed_since_install",
            Self::MissingFile => "missing_file",
            Self::InvalidManifest => "invalid_manifest",
        }
    }
}

impl InstalledPluginProvenance {
    pub fn legacy_unverified(source_path: impl Into<String>, captured_at: DateTime<Utc>) -> Self {
        Self {
            provenance_schema_version: 1,
            capture_method: "legacy_migration".to_string(),
            manifest_path: String::new(),
            manifest_sha256: String::new(),
            source_path: source_path.into(),
            source_path_canonicalized: false,
            source_tree_sha256: None,
            source_tree_file_count: None,
            subprocess_command_path: None,
            subprocess_command_sha256: None,
            wasm_module_path: None,
            wasm_module_sha256: None,
            captured_at,
            last_verified_at: None,
            integrity_status: InstalledPluginIntegrityStatus::NotVerified,
            origin_claim: None,
            origin_claim_verified: false,
        }
    }

    pub fn capture(
        manifest_path: &Path,
        manifest: &PluginManifest,
        source_path: &Path,
        captured_at: DateTime<Utc>,
    ) -> JarvisResult<Self> {
        let manifest_path = fs::canonicalize(manifest_path).map_err(|err| {
            JarvisError::Validation(format!("manifest path is not readable: {err}"))
        })?;
        let source_path = fs::canonicalize(source_path).map_err(|err| {
            JarvisError::Validation(format!("source_path is not readable: {err}"))
        })?;
        validate_source_tree_required_path("manifest", &source_path, &manifest_path)?;
        let source_tree_snapshot = source_tree_snapshot(&source_path)?;
        let mut provenance = Self {
            provenance_schema_version: 1,
            capture_method: "local_manifest_snapshot".to_string(),
            manifest_path: manifest_path.display().to_string(),
            manifest_sha256: sha256_file(&manifest_path)?,
            source_path: source_path.display().to_string(),
            source_path_canonicalized: true,
            source_tree_sha256: Some(source_tree_snapshot.sha256),
            source_tree_file_count: Some(source_tree_snapshot.file_count),
            subprocess_command_path: None,
            subprocess_command_sha256: None,
            wasm_module_path: None,
            wasm_module_sha256: None,
            captured_at,
            last_verified_at: None,
            integrity_status: InstalledPluginIntegrityStatus::NotVerified,
            origin_claim: Some(manifest.author.clone()),
            origin_claim_verified: false,
        };

        if manifest.source == PluginSource::LocalSubprocess {
            let subprocess = manifest.subprocess.as_ref().ok_or_else(|| {
                JarvisError::Validation(format!(
                    "{} local_subprocess manifests must declare subprocess",
                    manifest.id
                ))
            })?;
            let command_path = subprocess.validate(&manifest.id, &source_path)?;
            validate_source_tree_required_path("subprocess command", &source_path, &command_path)?;
            provenance.subprocess_command_sha256 = Some(sha256_file(&command_path)?);
            provenance.subprocess_command_path = Some(command_path.display().to_string());
        }
        if manifest.source == PluginSource::LocalWasm {
            let wasm = manifest.wasm.as_ref().ok_or_else(|| {
                JarvisError::Validation(format!(
                    "{} local_wasm manifests must declare wasm",
                    manifest.id
                ))
            })?;
            let module_path = wasm.validate(&manifest.id, &source_path)?;
            validate_source_tree_required_path("wasm module", &source_path, &module_path)?;
            provenance.wasm_module_sha256 = Some(sha256_file(&module_path)?);
            provenance.wasm_module_path = Some(module_path.display().to_string());
        }

        Ok(provenance)
    }

    pub fn verify_snapshot(&self, manifest: &PluginManifest, verified_at: DateTime<Utc>) -> Self {
        let status = match self.verify_status(manifest) {
            Ok(()) => InstalledPluginIntegrityStatus::MatchesInstallSnapshot,
            Err(PluginProvenanceVerificationError::Changed) => {
                InstalledPluginIntegrityStatus::ChangedSinceInstall
            }
            Err(PluginProvenanceVerificationError::MissingFile) => {
                InstalledPluginIntegrityStatus::MissingFile
            }
            Err(PluginProvenanceVerificationError::InvalidManifest) => {
                InstalledPluginIntegrityStatus::InvalidManifest
            }
        };
        let mut verified = self.clone();
        verified.integrity_status = status;
        verified.last_verified_at = Some(verified_at);
        verified
    }

    fn verify_status(
        &self,
        manifest: &PluginManifest,
    ) -> Result<(), PluginProvenanceVerificationError> {
        let manifest_path = Path::new(&self.manifest_path);
        if !manifest_path.exists() {
            return Err(PluginProvenanceVerificationError::MissingFile);
        }
        let manifest_sha = sha256_file(manifest_path)
            .map_err(|_| PluginProvenanceVerificationError::MissingFile)?;
        if manifest_sha != self.manifest_sha256 {
            return Err(PluginProvenanceVerificationError::Changed);
        }

        let source_path = Path::new(&self.source_path);
        if !source_path.exists() {
            return Err(PluginProvenanceVerificationError::MissingFile);
        }
        if let Some(expected_tree_sha) = self.source_tree_sha256.as_deref() {
            let source_tree_snapshot = source_tree_snapshot(source_path).map_err(|err| {
                if err.to_string().contains("symlink") {
                    PluginProvenanceVerificationError::InvalidManifest
                } else {
                    PluginProvenanceVerificationError::MissingFile
                }
            })?;
            if source_tree_snapshot.sha256 != expected_tree_sha {
                return Err(PluginProvenanceVerificationError::Changed);
            }
        }
        if let Some(expected_sha) = self.subprocess_command_sha256.as_deref() {
            let subprocess = manifest
                .subprocess
                .as_ref()
                .ok_or(PluginProvenanceVerificationError::InvalidManifest)?;
            let command_path = subprocess
                .validate(&manifest.id, source_path)
                .map_err(|_| PluginProvenanceVerificationError::InvalidManifest)?;
            let command_sha = sha256_file(&command_path)
                .map_err(|_| PluginProvenanceVerificationError::MissingFile)?;
            if command_sha != expected_sha {
                return Err(PluginProvenanceVerificationError::Changed);
            }
        }
        if let Some(expected_sha) = self.wasm_module_sha256.as_deref() {
            let wasm = manifest
                .wasm
                .as_ref()
                .ok_or(PluginProvenanceVerificationError::InvalidManifest)?;
            let module_path = wasm
                .validate(&manifest.id, source_path)
                .map_err(|_| PluginProvenanceVerificationError::InvalidManifest)?;
            let artifact = crate::read_wasm_artifact(&module_path)
                .map_err(|_| PluginProvenanceVerificationError::MissingFile)?;
            if artifact.sha256 != expected_sha {
                return Err(PluginProvenanceVerificationError::Changed);
            }
        }

        Ok(())
    }
}

#[derive(Debug)]
struct SourceTreeSnapshot {
    sha256: String,
    file_count: usize,
}

fn source_tree_snapshot(root: &Path) -> JarvisResult<SourceTreeSnapshot> {
    let canonical_root = fs::canonicalize(root).map_err(|err| {
        JarvisError::Validation(format!("plugin source_path is not readable: {err}"))
    })?;
    let mut files = Vec::new();
    let mut normalized_paths = HashSet::new();
    let mut discovered_bytes = 0_u64;
    let mut discovered_entries = 0_usize;
    collect_source_tree_files(
        &canonical_root,
        &canonical_root,
        &mut files,
        &mut discovered_bytes,
        &mut discovered_entries,
        0,
    )?;
    files.sort_by(|left, right| left.0.cmp(&right.0));

    let mut digest = Sha256::new();
    let mut hashed_bytes = 0_u64;
    digest.update(b"jarvis-plugin-source-tree-sha256-v1\0");
    digest.update(b"ignore-policy-v1\0");
    for (relative_path, file_path) in &files {
        let collision_key = relative_path.to_lowercase();
        if !normalized_paths.insert(collision_key) {
            return Err(JarvisError::Validation(format!(
                "plugin source tree has case-insensitive duplicate path: {relative_path}"
            )));
        }
        let remaining = MAX_PLUGIN_SOURCE_TREE_BYTES.saturating_sub(hashed_bytes);
        let mut file = fs::File::open(file_path).map_err(|err| {
            JarvisError::Validation(format!("read plugin source tree file: {err}"))
        })?;
        let mut file_bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(remaining.saturating_add(1))
            .read_to_end(&mut file_bytes)
            .map_err(|err| {
                JarvisError::Validation(format!("read plugin source tree file: {err}"))
            })?;
        if file_bytes.len() as u64 > remaining {
            return Err(JarvisError::Validation(format!(
                "plugin source tree exceeds {MAX_PLUGIN_SOURCE_TREE_BYTES} bytes"
            )));
        }
        hashed_bytes = hashed_bytes.saturating_add(file_bytes.len() as u64);
        digest.update(b"file\0");
        update_digest_with_len_prefixed_bytes(&mut digest, relative_path.as_bytes());
        update_digest_with_len_prefixed_bytes(&mut digest, &file_bytes);
    }

    Ok(SourceTreeSnapshot {
        sha256: format!("{:x}", digest.finalize()),
        file_count: files.len(),
    })
}

fn collect_source_tree_files(
    root: &Path,
    current: &Path,
    files: &mut Vec<(String, PathBuf)>,
    total_bytes: &mut u64,
    total_entries: &mut usize,
    depth: usize,
) -> JarvisResult<()> {
    if depth > MAX_PLUGIN_SOURCE_TREE_DEPTH {
        return Err(JarvisError::Validation(format!(
            "plugin source tree exceeds depth {MAX_PLUGIN_SOURCE_TREE_DEPTH}"
        )));
    }
    let read_dir = fs::read_dir(current)
        .map_err(|err| JarvisError::Validation(format!("read plugin source tree: {err}")))?;
    let mut entries = Vec::new();
    for entry in read_dir {
        if *total_entries >= MAX_PLUGIN_SOURCE_TREE_ENTRIES {
            return Err(JarvisError::Validation(format!(
                "plugin source tree exceeds {MAX_PLUGIN_SOURCE_TREE_ENTRIES} entries"
            )));
        }
        entries.push(
            entry.map_err(|err| {
                JarvisError::Validation(format!("read plugin source tree: {err}"))
            })?,
        );
        *total_entries += 1;
    }
    entries.sort_by_key(|entry| entry.path());

    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|err| {
            JarvisError::Validation(format!("read plugin source tree metadata: {err}"))
        })?;
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            return Err(JarvisError::Validation(format!(
                "plugin source tree cannot include symlink: {}",
                path.display()
            )));
        }
        let relative_path = normalized_source_tree_relative_path(root, &path)?;
        if source_tree_ignore_match(&relative_path, file_type.is_dir()) {
            continue;
        }
        if file_type.is_dir() {
            collect_source_tree_files(root, &path, files, total_bytes, total_entries, depth + 1)?;
            continue;
        }
        if file_type.is_file() {
            if files.len() >= MAX_PLUGIN_SOURCE_TREE_FILES {
                return Err(JarvisError::Validation(format!(
                    "plugin source tree exceeds {MAX_PLUGIN_SOURCE_TREE_FILES} files"
                )));
            }
            *total_bytes = total_bytes.checked_add(metadata.len()).ok_or_else(|| {
                JarvisError::Validation("plugin source tree byte count overflowed".to_string())
            })?;
            if *total_bytes > MAX_PLUGIN_SOURCE_TREE_BYTES {
                return Err(JarvisError::Validation(format!(
                    "plugin source tree exceeds {MAX_PLUGIN_SOURCE_TREE_BYTES} bytes"
                )));
            }
            let canonical_path = fs::canonicalize(&path).map_err(|err| {
                JarvisError::Validation(format!("plugin source tree file is not readable: {err}"))
            })?;
            if !canonical_path.starts_with(root) {
                return Err(JarvisError::Validation(format!(
                    "plugin source tree file escapes source_path: {}",
                    path.display()
                )));
            }
            files.push((relative_path, canonical_path));
            continue;
        }
        return Err(JarvisError::Validation(format!(
            "plugin source tree contains unsupported file type: {}",
            path.display()
        )));
    }
    Ok(())
}

fn update_digest_with_len_prefixed_bytes(digest: &mut Sha256, bytes: &[u8]) {
    digest.update((bytes.len() as u64).to_be_bytes());
    digest.update(bytes);
}

fn validate_source_tree_required_path(label: &str, root: &Path, path: &Path) -> JarvisResult<()> {
    let relative_path = normalized_source_tree_relative_path(root, path)?;
    if source_tree_ignore_match(&relative_path, path.is_dir()) {
        return Err(JarvisError::Validation(format!(
            "plugin {label} cannot be excluded from source tree provenance: {relative_path}"
        )));
    }
    Ok(())
}

fn normalized_source_tree_relative_path(root: &Path, path: &Path) -> JarvisResult<String> {
    let canonical_path = fs::canonicalize(path).map_err(|err| {
        JarvisError::Validation(format!("plugin source tree path is not readable: {err}"))
    })?;
    if !canonical_path.starts_with(root) {
        return Err(JarvisError::Validation(format!(
            "plugin source tree path escapes source_path: {}",
            path.display()
        )));
    }
    let relative_path = canonical_path.strip_prefix(root).map_err(|err| {
        JarvisError::Validation(format!("plugin source tree relative path: {err}"))
    })?;
    let mut components = Vec::new();
    for component in relative_path.components() {
        let Some(component) = component.as_os_str().to_str() else {
            return Err(JarvisError::Validation(format!(
                "plugin source tree path must be valid UTF-8: {}",
                path.display()
            )));
        };
        components.push(component);
    }
    Ok(components.join("/"))
}

fn source_tree_ignore_match(relative_path: &str, is_dir: bool) -> bool {
    let components: Vec<&str> = relative_path.split('/').collect();
    components.iter().any(|component| {
        matches!(
            *component,
            ".git"
                | ".hg"
                | ".svn"
                | ".AppleDouble"
                | "__MACOSX"
                | ".Spotlight-V100"
                | ".Trashes"
                | ".fseventsd"
                | "target"
                | "node_modules"
                | "dist"
                | "build"
                | ".cache"
                | ".pytest_cache"
                | ".mypy_cache"
                | ".ruff_cache"
                | "__pycache__"
                | ".venv"
                | "venv"
        )
    }) || components.last().is_some_and(|name| {
        *name == ".DS_Store"
            || name.starts_with("._")
            || *name == ".env"
            || name.starts_with(".env.")
            || name.ends_with(".pyc")
            || name.ends_with(".pyo")
            || name.ends_with(".log")
            || (is_dir
                && matches!(
                    *name,
                    ".git"
                        | ".hg"
                        | ".svn"
                        | ".AppleDouble"
                        | "__MACOSX"
                        | ".Spotlight-V100"
                        | ".Trashes"
                        | ".fseventsd"
                        | "target"
                        | "node_modules"
                        | "dist"
                        | "build"
                        | ".cache"
                        | ".pytest_cache"
                        | ".mypy_cache"
                        | ".ruff_cache"
                        | "__pycache__"
                        | ".venv"
                        | "venv"
                ))
    })
}

enum PluginProvenanceVerificationError {
    Changed,
    MissingFile,
    InvalidManifest,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstalledPluginExecutionGrant {
    #[default]
    MetadataOnly,
    SubprocessStdio,
    SubprocessStdioNetwork,
    WasmCompute,
}

impl InstalledPluginExecutionGrant {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MetadataOnly => "metadata_only",
            Self::SubprocessStdio => "subprocess_stdio",
            Self::SubprocessStdioNetwork => "subprocess_stdio_network",
            Self::WasmCompute => "wasm_compute",
        }
    }

    pub fn parse(value: &str) -> JarvisResult<Self> {
        match value {
            "metadata_only" => Ok(Self::MetadataOnly),
            "subprocess_stdio" => Ok(Self::SubprocessStdio),
            "subprocess_stdio_network" => Ok(Self::SubprocessStdioNetwork),
            "wasm_compute" => Ok(Self::WasmCompute),
            _ => Err(JarvisError::Validation(format!(
                "unknown installed plugin execution grant: {value}"
            ))),
        }
    }
}

impl std::str::FromStr for InstalledPluginExecutionGrant {
    type Err = JarvisError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubprocessPluginExecution {
    pub output: Value,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub progress_events: Vec<PluginProgressEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubprocessControlState {
    Continue,
    EmergencyPaused,
    Cancelled,
}

impl SubprocessControlState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Continue => "continue",
            Self::EmergencyPaused => "emergency_paused",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginProgressEvent {
    pub sequence: usize,
    pub stage: String,
    pub message: String,
}

pub fn execute_installed_subprocess_plugin(
    manifest: &PluginManifest,
    action: &PluginActionManifest,
    source_path: &Path,
    input: &Value,
) -> JarvisResult<SubprocessPluginExecution> {
    execute_installed_subprocess_plugin_cancellable(manifest, action, source_path, input, || {
        SubprocessControlState::Continue
    })
}

pub fn execute_installed_subprocess_plugin_cancellable(
    manifest: &PluginManifest,
    action: &PluginActionManifest,
    source_path: &Path,
    input: &Value,
    mut control: impl FnMut() -> SubprocessControlState,
) -> JarvisResult<SubprocessPluginExecution> {
    if manifest.source != PluginSource::LocalSubprocess {
        return Err(JarvisError::Validation(format!(
            "{} installed execution requires local_subprocess source",
            manifest.id
        )));
    }
    let subprocess = manifest.subprocess.as_ref().ok_or_else(|| {
        JarvisError::Validation(format!("{} missing subprocess config", manifest.id))
    })?;
    if subprocess.stdin != PluginSubprocessStream::Json
        || subprocess.stdout != PluginSubprocessStream::Json
    {
        return Err(JarvisError::Validation(format!(
            "{} subprocess must use JSON stdin/stdout",
            manifest.id
        )));
    }
    let executable = subprocess.validate(&manifest.id, source_path)?;
    let request = json!({
        "plugin_id": manifest.id,
        "action": action.name,
        "input": input,
    });
    let request_bytes = serde_json::to_vec(&request)
        .map_err(|err| JarvisError::Plugin(format!("serialize subprocess input: {err}")))?;

    let mut command = Command::new(executable);
    command
        .args(&subprocess.args)
        .env_clear()
        .env("JARVIS_PLUGIN_ID", &manifest.id)
        .env("JARVIS_PLUGIN_ACTION", &action.name)
        .env("JARVIS_PLUGIN_SOURCE_PATH", source_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    command.process_group(0);
    if let Some(path) = plugin_subprocess_path_env() {
        command.env("PATH", path);
    }

    let mut child = command
        .spawn()
        .map_err(|err| JarvisError::Plugin(format!("spawn subprocess plugin: {err}")))?;

    let process_group = subprocess_process_group(&child)?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| JarvisError::Plugin("capture subprocess stdout".to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| JarvisError::Plugin("capture subprocess stderr".to_string()))?;
    let (output_tx, output_rx) = mpsc::channel();
    let stdout_reader = spawn_bounded_output_reader(
        PluginOutputStream::Stdout,
        stdout,
        MAX_PLUGIN_STDOUT_BYTES,
        output_tx.clone(),
    );
    let stderr_reader = spawn_bounded_output_reader(
        PluginOutputStream::Stderr,
        stderr,
        MAX_PLUGIN_STDERR_BYTES,
        output_tx,
    );
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| JarvisError::Plugin("capture subprocess stdin".to_string()))?;
    let (stdin_tx, stdin_rx) = mpsc::channel();
    let stdin_writer = thread::spawn(move || {
        let mut stdin = stdin;
        let result = stdin
            .write_all(&request_bytes)
            .map_err(|err| format!("write subprocess stdin: {err}"));
        drop(stdin);
        let _ = stdin_tx.send(result);
    });
    let mut stdout_output: Option<Vec<u8>> = None;
    let mut stderr_output: Option<Vec<u8>> = None;
    let mut stdin_complete = false;

    let deadline = Instant::now() + action.timeout.duration();
    loop {
        if let Err(error) = ensure_subprocess_control(control()) {
            return fail_subprocess_after_spawn(
                error,
                &mut child,
                process_group,
                stdin_writer,
                stdout_reader,
                stderr_reader,
            );
        }
        if !stdin_complete {
            match stdin_rx.try_recv() {
                Ok(Ok(())) => stdin_complete = true,
                Ok(Err(error)) => {
                    return fail_subprocess_after_spawn(
                        JarvisError::Plugin(error),
                        &mut child,
                        process_group,
                        stdin_writer,
                        stdout_reader,
                        stderr_reader,
                    );
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    return fail_subprocess_after_spawn(
                        JarvisError::Plugin(
                            "subprocess stdin writer stopped unexpectedly".to_string(),
                        ),
                        &mut child,
                        process_group,
                        stdin_writer,
                        stdout_reader,
                        stderr_reader,
                    );
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Err(error) =
            handle_subprocess_output_events(&output_rx, &mut stdout_output, &mut stderr_output)
        {
            return fail_subprocess_after_spawn(
                error,
                &mut child,
                process_group,
                stdin_writer,
                stdout_reader,
                stderr_reader,
            );
        }
        let exit_observation = match observe_subprocess_exit(&mut child) {
            Ok(observation) => observation,
            Err(error) => {
                return fail_subprocess_after_spawn(
                    JarvisError::Plugin(format!("wait subprocess plugin: {error}")),
                    &mut child,
                    process_group,
                    stdin_writer,
                    stdout_reader,
                    stderr_reader,
                );
            }
        };
        if let Some(exit_observation) = exit_observation {
            if let Err(error) = ensure_subprocess_control(control()) {
                return fail_subprocess_after_spawn(
                    error,
                    &mut child,
                    process_group,
                    stdin_writer,
                    stdout_reader,
                    stderr_reader,
                );
            }
            // A plugin leader may exit while a descendant keeps inherited pipes
            // or continues work. End the dedicated group before collecting the
            // final bounded output so the invocation cannot detach descendants.
            let status = close_subprocess_after_spawn(
                &mut child,
                process_group,
                exit_observation.reaped_status,
                stdin_writer,
                stdout_reader,
                stderr_reader,
            )?;
            collect_subprocess_outputs(
                &output_rx,
                &mut stdout_output,
                &mut stderr_output,
                Instant::now() + Duration::from_secs(1),
            )?;
            require_subprocess_stdin_delivery(&stdin_rx, stdin_complete)?;
            if !status.success() {
                return Err(JarvisError::Plugin(format!(
                    "subprocess plugin exited with status {status}"
                )));
            }
            let stdout = stdout_output.take().unwrap_or_default();
            let stderr = stderr_output.take().unwrap_or_default();
            let stdout_bytes = stdout.len();
            let stderr_bytes = stderr.len();
            let progress_events = parse_plugin_progress_events(&stderr);
            let value: Value = serde_json::from_slice(&stdout).map_err(|err| {
                JarvisError::Plugin(format!("parse subprocess stdout JSON: {err}"))
            })?;
            action
                .output_schema
                .validate_value(&format!("{}.{} output", manifest.id, action.name), &value)?;
            ensure_subprocess_control(control())?;
            return Ok(SubprocessPluginExecution {
                output: value,
                stdout_bytes,
                stderr_bytes,
                exit_code: status.code(),
                progress_events,
            });
        }
        if Instant::now() >= deadline {
            return fail_subprocess_after_spawn(
                JarvisError::Plugin(format!(
                    "subprocess plugin timed out after {}ms",
                    action.timeout.timeout_ms
                )),
                &mut child,
                process_group,
                stdin_writer,
                stdout_reader,
                stderr_reader,
            );
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn ensure_subprocess_control(state: SubprocessControlState) -> JarvisResult<()> {
    match state {
        SubprocessControlState::Continue => Ok(()),
        SubprocessControlState::EmergencyPaused => Err(JarvisError::PolicyBlocked(
            "subprocess execution cancelled by emergency pause".to_string(),
        )),
        SubprocessControlState::Cancelled => Err(JarvisError::Plugin(
            "subprocess execution cancelled".to_string(),
        )),
    }
}

#[cfg(unix)]
fn observe_subprocess_exit(child: &mut Child) -> io::Result<Option<SubprocessExitObservation>> {
    let status = waitid(
        WaitId::Pid(Pid::from_child(child)),
        WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
    )?;
    Ok(status.map(|_| SubprocessExitObservation {
        reaped_status: None,
    }))
}

#[cfg(not(unix))]
fn observe_subprocess_exit(child: &mut Child) -> io::Result<Option<SubprocessExitObservation>> {
    child.try_wait().map(|status| {
        status.map(|status| SubprocessExitObservation {
            reaped_status: Some(status),
        })
    })
}

#[cfg(unix)]
fn subprocess_process_group(child: &Child) -> JarvisResult<SubprocessGroupId> {
    Ok(Pid::from_child(child))
}

#[cfg(not(unix))]
fn subprocess_process_group(child: &Child) -> JarvisResult<SubprocessGroupId> {
    Ok(child.id())
}

#[cfg(unix)]
fn signal_subprocess_group(process_group: SubprocessGroupId, signal: Signal) -> Result<(), Errno> {
    match kill_process_group(process_group, signal) {
        Ok(()) | Err(Errno::SRCH) => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SubprocessGroupInspection {
    Absent,
    Signalable,
    PresentButNotSignalable,
}

#[cfg(unix)]
fn classify_subprocess_group_probe(
    result: Result<(), Errno>,
) -> JarvisResult<SubprocessGroupInspection> {
    match result {
        Ok(()) => Ok(SubprocessGroupInspection::Signalable),
        Err(Errno::SRCH) => Ok(SubprocessGroupInspection::Absent),
        // POSIX kill(-pgid, 0) uses EPERM to report that the group exists but
        // the caller cannot signal it. This is never evidence of cleanup.
        Err(Errno::PERM) => Ok(SubprocessGroupInspection::PresentButNotSignalable),
        Err(error) => Err(JarvisError::Plugin(format!(
            "inspect subprocess process group: {error}"
        ))),
    }
}

#[cfg(unix)]
fn inspect_subprocess_group(
    process_group: SubprocessGroupId,
) -> JarvisResult<SubprocessGroupInspection> {
    classify_subprocess_group_probe(test_kill_process_group(process_group))
}

#[cfg(unix)]
fn terminate_subprocess_group(
    child: &mut Child,
    process_group: SubprocessGroupId,
    reaped_status: Option<ExitStatus>,
) -> JarvisResult<ExitStatus> {
    let mut cleanup_errors = Vec::new();
    if reaped_status.is_some() {
        cleanup_errors
            .push("subprocess leader was reaped before Unix process-group cleanup".to_string());
    }
    if let Err(error) = signal_subprocess_group(process_group, Signal::TERM) {
        // macOS can report EPERM when the pinned group contains only a zombie
        // leader. Defer EPERM to the post-reap, probe-only confirmation;
        // persistent EPERM there still fails closed.
        if error != Errno::PERM {
            cleanup_errors.push(format!("signal subprocess process group: {error}"));
        }
    }
    // Keep the leader unreaped until the final process-group signal. While the
    // leader remains a child (or a zombie), its PID/PGID cannot be recycled for
    // an unrelated process group.
    let mut group_absent = false;
    let deadline = Instant::now() + SUBPROCESS_TERMINATION_GRACE;
    while Instant::now() < deadline {
        match inspect_subprocess_group(process_group) {
            Ok(SubprocessGroupInspection::Absent) => {
                group_absent = true;
                break;
            }
            Ok(_) => {}
            Err(error) => {
                cleanup_errors.push(error.to_string());
                break;
            }
        }
        thread::sleep(Duration::from_millis(5));
    }

    if !group_absent {
        if let Err(error) = signal_subprocess_group(process_group, Signal::KILL) {
            if error != Errno::PERM {
                cleanup_errors.push(format!("signal subprocess process group: {error}"));
            }
        }
    }

    // No process-group signal is allowed after this point. Reaping may release
    // the numeric PID/PGID for reuse, so leader fallback cleanup is deliberately
    // bounded and scoped to the still-owned Child handle.
    let leader_deadline = Instant::now() + SUBPROCESS_GROUP_KILL_CONFIRM_TIMEOUT;
    let mut fallback_kill_attempted = false;
    let mut leader_status = None;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                leader_status = Some(status);
                break;
            }
            Ok(None) if !fallback_kill_attempted => {
                fallback_kill_attempted = true;
                if let Err(error) = child.kill() {
                    cleanup_errors.push(format!("fallback kill subprocess leader: {error}"));
                }
            }
            Ok(None) if Instant::now() < leader_deadline => {
                thread::sleep(Duration::from_millis(5));
            }
            Ok(None) => {
                cleanup_errors.push(
                    "subprocess leader was not reaped after bounded fallback kill confirmation"
                        .to_string(),
                );
                break;
            }
            Err(error) => {
                cleanup_errors.push(format!("reap subprocess plugin: {error}"));
                break;
            }
        }
    }

    if !group_absent {
        // Probe-only confirmation cannot affect a recycled PGID. A recycled or
        // otherwise persistent group can cause conservative false uncertainty,
        // but it must never trigger a signal after the leader has been reaped.
        let confirmation_deadline = Instant::now() + SUBPROCESS_GROUP_KILL_CONFIRM_TIMEOUT;
        loop {
            match inspect_subprocess_group(process_group) {
                Ok(SubprocessGroupInspection::Absent) => break,
                Ok(_) if Instant::now() < confirmation_deadline => {
                    thread::sleep(Duration::from_millis(5));
                }
                Ok(SubprocessGroupInspection::Signalable) => {
                    cleanup_errors.push(
                        "subprocess process group remained after bounded KILL confirmation"
                            .to_string(),
                    );
                    break;
                }
                Ok(SubprocessGroupInspection::PresentButNotSignalable) => {
                    cleanup_errors.push(
                        "subprocess process group existence could not be cleared after bounded KILL confirmation because the group was not signalable"
                            .to_string(),
                    );
                    break;
                }
                Err(error) => {
                    cleanup_errors.push(error.to_string());
                    break;
                }
            }
        }
    }
    if !cleanup_errors.is_empty() {
        return Err(JarvisError::Plugin(cleanup_errors.join("; ")));
    }
    leader_status.ok_or_else(|| {
        JarvisError::Plugin("subprocess leader exit status was not collected".to_string())
    })
}

#[cfg(not(unix))]
fn terminate_subprocess_group(
    child: &mut Child,
    _process_group: SubprocessGroupId,
    reaped_status: Option<ExitStatus>,
) -> JarvisResult<ExitStatus> {
    if let Some(status) = reaped_status {
        return Ok(status);
    }
    let mut cleanup_errors = Vec::new();
    if let Err(error) = child.kill() {
        cleanup_errors.push(format!("terminate subprocess plugin: {error}"));
    }
    let status = match child.wait() {
        Ok(status) => Some(status),
        Err(error) => {
            cleanup_errors.push(format!("reap subprocess plugin: {error}"));
            None
        }
    };
    if !cleanup_errors.is_empty() {
        return Err(JarvisError::Plugin(cleanup_errors.join("; ")));
    }
    status.ok_or_else(|| {
        JarvisError::Plugin("subprocess leader exit status was not collected".to_string())
    })
}

fn join_subprocess_io_threads(
    stdin_writer: thread::JoinHandle<()>,
    stdout_reader: thread::JoinHandle<()>,
    stderr_reader: thread::JoinHandle<()>,
) -> JarvisResult<()> {
    let handles = [
        ("stdin writer", stdin_writer),
        ("stdout reader", stdout_reader),
        ("stderr reader", stderr_reader),
    ];
    let deadline = Instant::now() + SUBPROCESS_IO_JOIN_TIMEOUT;
    while handles.iter().any(|(_, handle)| !handle.is_finished()) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    let unfinished = handles
        .iter()
        .filter_map(|(label, handle)| (!handle.is_finished()).then_some(*label))
        .collect::<Vec<_>>();
    if !unfinished.is_empty() {
        return Err(JarvisError::Plugin(format!(
            "subprocess I/O workers did not stop after pipe closure: {}",
            unfinished.join(", ")
        )));
    }
    for (label, handle) in handles {
        handle
            .join()
            .map_err(|_| JarvisError::Plugin(format!("subprocess {label} panicked")))?;
    }
    Ok(())
}

fn close_subprocess_after_spawn(
    child: &mut Child,
    process_group: SubprocessGroupId,
    reaped_status: Option<ExitStatus>,
    stdin_writer: thread::JoinHandle<()>,
    stdout_reader: thread::JoinHandle<()>,
    stderr_reader: thread::JoinHandle<()>,
) -> JarvisResult<ExitStatus> {
    let termination = terminate_subprocess_group(child, process_group, reaped_status);
    let io = join_subprocess_io_threads(stdin_writer, stdout_reader, stderr_reader);
    match (termination, io) {
        (Ok(status), Ok(())) => Ok(status),
        (Err(error), Ok(())) | (Ok(_), Err(error)) => Err(error),
        (Err(termination), Err(io)) => Err(JarvisError::Plugin(format!(
            "{termination}; additional subprocess I/O cleanup failure: {io}"
        ))),
    }
}

fn fail_subprocess_after_spawn<T>(
    primary: JarvisError,
    child: &mut Child,
    process_group: SubprocessGroupId,
    stdin_writer: thread::JoinHandle<()>,
    stdout_reader: thread::JoinHandle<()>,
    stderr_reader: thread::JoinHandle<()>,
) -> JarvisResult<T> {
    match close_subprocess_after_spawn(
        child,
        process_group,
        None,
        stdin_writer,
        stdout_reader,
        stderr_reader,
    ) {
        Ok(_) => Err(primary),
        Err(cleanup) => Err(attach_subprocess_cleanup_failure(primary, cleanup)),
    }
}

fn attach_subprocess_cleanup_failure(primary: JarvisError, cleanup: JarvisError) -> JarvisError {
    // Cleanup uncertainty is elevated to a plugin failure even when the
    // primary cause was a policy block. Callers must not infer that emergency
    // pause completed containment successfully from the primary enum alone.
    JarvisError::Plugin(format!("{primary}; subprocess cleanup failure: {cleanup}"))
}

fn require_subprocess_stdin_delivery(
    receiver: &mpsc::Receiver<Result<(), String>>,
    already_complete: bool,
) -> JarvisResult<()> {
    if already_complete {
        return Ok(());
    }
    match receiver.try_recv() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(JarvisError::Plugin(error)),
        Err(mpsc::TryRecvError::Disconnected) => Err(JarvisError::Plugin(
            "subprocess stdin writer stopped unexpectedly".to_string(),
        )),
        Err(mpsc::TryRecvError::Empty) => Err(JarvisError::Plugin(
            "subprocess stdin delivery did not complete".to_string(),
        )),
    }
}

#[derive(Debug, Clone, Copy)]
enum PluginOutputStream {
    Stdout,
    Stderr,
}

impl PluginOutputStream {
    fn label(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

enum PluginOutputEvent {
    Complete(PluginOutputStream, Vec<u8>),
    Exceeded(PluginOutputStream, usize),
    Error(PluginOutputStream, String),
}

fn spawn_bounded_output_reader<R>(
    stream: PluginOutputStream,
    mut reader: R,
    limit: usize,
    sender: mpsc::Sender<PluginOutputEvent>,
) -> thread::JoinHandle<()>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut output = Vec::new();
        let mut buffer = [0_u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => {
                    let _ = sender.send(PluginOutputEvent::Complete(stream, output));
                    return;
                }
                Ok(read) => {
                    if output.len().saturating_add(read) > limit {
                        let observed = output.len().saturating_add(read);
                        let _ = sender.send(PluginOutputEvent::Exceeded(stream, observed));
                        return;
                    }
                    output.extend_from_slice(&buffer[..read]);
                }
                Err(err) => {
                    let _ = sender.send(PluginOutputEvent::Error(stream, err.to_string()));
                    return;
                }
            }
        }
    })
}

fn handle_subprocess_output_events(
    receiver: &mpsc::Receiver<PluginOutputEvent>,
    stdout_output: &mut Option<Vec<u8>>,
    stderr_output: &mut Option<Vec<u8>>,
) -> JarvisResult<()> {
    while let Ok(event) = receiver.try_recv() {
        handle_subprocess_output_event(event, stdout_output, stderr_output)?;
    }
    Ok(())
}

fn collect_subprocess_outputs(
    receiver: &mpsc::Receiver<PluginOutputEvent>,
    stdout_output: &mut Option<Vec<u8>>,
    stderr_output: &mut Option<Vec<u8>>,
    deadline: Instant,
) -> JarvisResult<()> {
    while stdout_output.is_none() || stderr_output.is_none() {
        let now = Instant::now();
        if now >= deadline {
            return Err(JarvisError::Plugin(
                "read subprocess output after exit timed out".to_string(),
            ));
        }
        let timeout = deadline.saturating_duration_since(now);
        let event = receiver.recv_timeout(timeout).map_err(|err| {
            JarvisError::Plugin(format!("read subprocess output after exit: {err}"))
        })?;
        handle_subprocess_output_event_without_child(event, stdout_output, stderr_output)?;
    }
    Ok(())
}

fn handle_subprocess_output_event(
    event: PluginOutputEvent,
    stdout_output: &mut Option<Vec<u8>>,
    stderr_output: &mut Option<Vec<u8>>,
) -> JarvisResult<()> {
    match event {
        PluginOutputEvent::Complete(stream, output) => {
            store_subprocess_output(stream, output, stdout_output, stderr_output);
            Ok(())
        }
        PluginOutputEvent::Exceeded(stream, observed) => Err(JarvisError::Plugin(format!(
            "subprocess plugin {} exceeded {} byte limit after at least {observed} bytes",
            stream.label(),
            output_limit_for_stream(stream)
        ))),
        PluginOutputEvent::Error(stream, error) => Err(JarvisError::Plugin(format!(
            "read subprocess plugin {}: {error}",
            stream.label()
        ))),
    }
}

fn handle_subprocess_output_event_without_child(
    event: PluginOutputEvent,
    stdout_output: &mut Option<Vec<u8>>,
    stderr_output: &mut Option<Vec<u8>>,
) -> JarvisResult<()> {
    match event {
        PluginOutputEvent::Complete(stream, output) => {
            store_subprocess_output(stream, output, stdout_output, stderr_output);
            Ok(())
        }
        PluginOutputEvent::Exceeded(stream, observed) => Err(JarvisError::Plugin(format!(
            "subprocess plugin {} exceeded {} byte limit after at least {observed} bytes",
            stream.label(),
            output_limit_for_stream(stream)
        ))),
        PluginOutputEvent::Error(stream, error) => Err(JarvisError::Plugin(format!(
            "read subprocess plugin {}: {error}",
            stream.label()
        ))),
    }
}

fn store_subprocess_output(
    stream: PluginOutputStream,
    output: Vec<u8>,
    stdout_output: &mut Option<Vec<u8>>,
    stderr_output: &mut Option<Vec<u8>>,
) {
    match stream {
        PluginOutputStream::Stdout => *stdout_output = Some(output),
        PluginOutputStream::Stderr => *stderr_output = Some(output),
    }
}

fn output_limit_for_stream(stream: PluginOutputStream) -> usize {
    match stream {
        PluginOutputStream::Stdout => MAX_PLUGIN_STDOUT_BYTES,
        PluginOutputStream::Stderr => MAX_PLUGIN_STDERR_BYTES,
    }
}

fn plugin_subprocess_path_env() -> Option<String> {
    if cfg!(windows) {
        std::env::var("PATH").ok()
    } else {
        Some("/usr/bin:/bin:/usr/sbin:/sbin".to_string())
    }
}

fn parse_plugin_progress_events(stderr: &[u8]) -> Vec<PluginProgressEvent> {
    let text = String::from_utf8_lossy(stderr);
    let mut events = Vec::new();

    for line in text.lines() {
        if events.len() >= MAX_PLUGIN_PROGRESS_EVENTS {
            break;
        }
        let line = line.trim();
        if line.is_empty() || line.len() > MAX_PLUGIN_PROGRESS_LINE_BYTES {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if value.get("jarvis_progress").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let Some(stage) = value.get("stage").and_then(Value::as_str) else {
            continue;
        };
        let Some(message) = value.get("message").and_then(Value::as_str) else {
            continue;
        };
        let stage = sanitize_progress_text(stage, MAX_PLUGIN_PROGRESS_STAGE_CHARS);
        let message = sanitize_progress_text(message, MAX_PLUGIN_PROGRESS_MESSAGE_CHARS);
        if stage.is_empty() || message.is_empty() {
            continue;
        }
        events.push(PluginProgressEvent {
            sequence: events.len() + 1,
            stage,
            message,
        });
    }

    events
}

fn sanitize_progress_text(value: &str, max_chars: usize) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_control())
        .take(max_chars)
        .collect::<String>()
        .trim()
        .to_string()
}

impl InstalledPlugin {
    pub fn from_local_manifest_path(manifest_path: impl AsRef<Path>) -> JarvisResult<Self> {
        let manifest_path = manifest_path.as_ref();
        if !manifest_path.is_absolute() {
            return Err(JarvisError::Validation(
                "manifest path must be absolute".to_string(),
            ));
        }
        let content = fs::read_to_string(manifest_path).map_err(|err| {
            JarvisError::Validation(format!(
                "read plugin manifest {}: {err}",
                manifest_path.display()
            ))
        })?;
        let manifest: PluginManifest = serde_json::from_str(&content)
            .map_err(|err| JarvisError::Validation(format!("parse plugin manifest: {err}")))?;
        require_valid_semantic_version(&manifest.version)?;
        let source_path = manifest.validate_local_install(manifest_path)?;

        let provenance =
            InstalledPluginProvenance::capture(manifest_path, &manifest, &source_path, Utc::now())?;

        Ok(Self {
            manifest,
            source_path: source_path.display().to_string(),
            provenance,
            execution_enabled: false,
            execution_grant: InstalledPluginExecutionGrant::MetadataOnly,
        })
    }
}

fn sha256_file(path: &Path) -> JarvisResult<String> {
    let mut file = fs::File::open(path)
        .map_err(|err| JarvisError::Validation(format!("hash {}: {err}", path.display())))?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_PLUGIN_SOURCE_TREE_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|err| JarvisError::Validation(format!("hash {}: {err}", path.display())))?;
    if bytes.len() as u64 > MAX_PLUGIN_SOURCE_TREE_BYTES {
        return Err(JarvisError::Validation(format!(
            "plugin file exceeds {MAX_PLUGIN_SOURCE_TREE_BYTES} bytes: {}",
            path.display()
        )));
    }
    let digest = Sha256::digest(&bytes);
    Ok(format!("{digest:x}"))
}

fn decode_fixed_base64<const N: usize>(
    value: &str,
    label: &str,
    plugin_id: &str,
) -> JarvisResult<[u8; N]> {
    let decoded = BASE64_STANDARD.decode(value).map_err(|err| {
        JarvisError::Validation(format!("{plugin_id} {label} is not valid base64: {err}"))
    })?;
    decoded.try_into().map_err(|bytes: Vec<u8>| {
        JarvisError::Validation(format!(
            "{plugin_id} {label} must decode to {N} bytes, got {}",
            bytes.len()
        ))
    })
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

fn validate_network_host(host: &str) -> Result<(), &'static str> {
    if host.is_empty() {
        return Err("host is required");
    }
    if host.len() > 253 {
        return Err("host is too long");
    }
    if host.contains('*')
        || host.contains('/')
        || host.contains(':')
        || host.chars().any(char::is_whitespace)
    {
        return Err(
            "host must be a plain hostname without wildcard, scheme, path, port, or whitespace",
        );
    }
    if !host
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '.' || character == '-')
    {
        return Err("host must use ascii letters, digits, dots, or hyphens");
    }
    if host != host.to_ascii_lowercase() {
        return Err("host must be lowercase");
    }
    if host.starts_with('.') || host.ends_with('.') {
        return Err("host cannot start or end with a dot");
    }
    if host.parse::<std::net::IpAddr>().is_ok() {
        return Err("host must be a hostname, not an IP literal");
    }
    if host
        .split('.')
        .any(|label| label.is_empty() || label.starts_with('-') || label.ends_with('-'))
    {
        return Err("host labels cannot be empty or start/end with hyphen");
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

    pub fn with_proactive(mut self, proactive: bool) -> Self {
        self.proactive = proactive;
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
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub audit_summary: Value,
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
            audit_summary: if manifest.id == "workspace_inspect" {
                crate::workspace::audit_request_summary(&request.input)
            } else {
                Value::Null
            },
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
    fn approval_required(mut metadata: PluginCallMetadata) -> Self {
        finish_plugin_audit(&mut metadata, "approval_required", None);
        Self {
            status: PluginCallStatus::ApprovalRequired,
            output: json!({ "approval_required": true }),
            metadata,
        }
    }

    fn timed_out(mut metadata: PluginCallMetadata) -> Self {
        finish_plugin_audit(&mut metadata, "timed_out", None);
        Self {
            status: PluginCallStatus::TimedOut,
            output: json!({ "timed_out": true }),
            metadata,
        }
    }
}

fn finish_plugin_audit(metadata: &mut PluginCallMetadata, outcome: &str, output: Option<&Value>) {
    if metadata.plugin_id == "workspace_inspect" {
        crate::workspace::finish_audit_summary(&mut metadata.audit_summary, outcome, output);
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

#[derive(Clone, Default)]
pub struct PluginHost {
    plugins: HashMap<String, Arc<dyn InProcessPlugin>>,
}

impl PluginHost {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_first_party_plugins() -> JarvisResult<Self> {
        Self::with_workspace_roots(Vec::new())
    }

    pub fn with_workspace_roots(
        workspace_roots: Vec<crate::WorkspaceRootConfig>,
    ) -> JarvisResult<Self> {
        let mut host = Self::new();
        let workspace = crate::WorkspaceInspectPlugin::open(workspace_roots)?;
        host.register(StatusPlugin)?;
        if let Some(workspace) = workspace {
            host.register(workspace)?;
        }
        Ok(host)
    }

    #[cfg(test)]
    pub(crate) fn with_test_fixtures() -> JarvisResult<Self> {
        let mut host = Self::new();
        host.register(EchoPlugin)?;
        host.register(FakeStatusPlugin)?;
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
        self.execute_cancellable(request, || false)
    }

    pub fn execute_cancellable(
        &self,
        request: PluginCallRequest,
        should_cancel: impl Fn() -> bool,
    ) -> JarvisResult<PluginCallResult> {
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

        let deadline = Instant::now() + timeout.duration();
        loop {
            if should_cancel() {
                cancellation.cancel();
                finish_plugin_audit(&mut metadata, "cancelled", None);
                return Ok(PluginCallResult {
                    status: PluginCallStatus::Cancelled,
                    output: json!({ "cancelled": true }),
                    metadata,
                });
            }
            let now = Instant::now();
            if now >= deadline {
                if timeout.on_timeout == PluginTimeoutAction::Cancel {
                    cancellation.cancel();
                }
                return Ok(PluginCallResult::timed_out(metadata));
            }
            let wait = std::cmp::min(deadline - now, Duration::from_millis(10));
            match receiver.recv_timeout(wait) {
                Ok(Ok(output)) => {
                    if should_cancel() {
                        cancellation.cancel();
                        finish_plugin_audit(&mut metadata, "cancelled", None);
                        return Ok(PluginCallResult {
                            status: PluginCallStatus::Cancelled,
                            output: json!({ "cancelled": true }),
                            metadata,
                        });
                    }
                    output_schema
                        .validate_value(&format!("{plugin_id}.{action_name} output"), &output)?;
                    if should_cancel() {
                        cancellation.cancel();
                        finish_plugin_audit(&mut metadata, "cancelled", None);
                        return Ok(PluginCallResult {
                            status: PluginCallStatus::Cancelled,
                            output: json!({ "cancelled": true }),
                            metadata,
                        });
                    }
                    if Instant::now() >= deadline {
                        cancellation.cancel();
                        return Ok(PluginCallResult::timed_out(metadata));
                    }
                    finish_plugin_audit(&mut metadata, "completed", Some(&output));
                    return Ok(PluginCallResult {
                        status: PluginCallStatus::Completed,
                        output,
                        metadata,
                    });
                }
                Ok(Err(error)) => {
                    if should_cancel() {
                        cancellation.cancel();
                        finish_plugin_audit(&mut metadata, "cancelled", None);
                        return Ok(PluginCallResult {
                            status: PluginCallStatus::Cancelled,
                            output: json!({ "cancelled": true }),
                            metadata,
                        });
                    }
                    if Instant::now() >= deadline {
                        cancellation.cancel();
                        return Ok(PluginCallResult::timed_out(metadata));
                    }
                    finish_plugin_audit(&mut metadata, "failed", None);
                    return Ok(PluginCallResult {
                        status: PluginCallStatus::Failed,
                        output: json!({ "error": error.to_string() }),
                        metadata,
                    });
                }
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if should_cancel() {
                        cancellation.cancel();
                        finish_plugin_audit(&mut metadata, "cancelled", None);
                        return Ok(PluginCallResult {
                            status: PluginCallStatus::Cancelled,
                            output: json!({ "cancelled": true }),
                            metadata,
                        });
                    }
                    if Instant::now() >= deadline {
                        cancellation.cancel();
                        return Ok(PluginCallResult::timed_out(metadata));
                    }
                    finish_plugin_audit(&mut metadata, "failed", None);
                    return Ok(PluginCallResult {
                        status: PluginCallStatus::Failed,
                        output: json!({ "error": "plugin worker disconnected" }),
                        metadata,
                    });
                }
            }
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

#[cfg(test)]
#[derive(Debug, Clone)]
pub(crate) struct EchoPlugin;

#[cfg(test)]
impl InProcessPlugin for EchoPlugin {
    fn manifest(&self) -> PluginManifest {
        let mut input_properties = Map::new();
        input_properties.insert("message".to_string(), json!({ "type": "string" }));

        let mut output_properties = Map::new();
        output_properties.insert("message".to_string(), json!({ "type": "string" }));

        PluginManifest {
            manifest_schema_version: LOCAL_MANIFEST_SCHEMA_VERSION,
            id: "fake_echo".to_string(),
            name: "Fake Echo".to_string(),
            version: "0.1.0".to_string(),
            source: PluginSource::FirstParty,
            author: "Jarvis".to_string(),
            source_path: None,
            subprocess: None,
            wasm: None,
            publisher_signature: None,
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
                    network_access: PluginNetworkAccess::default(),
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
                    network_access: PluginNetworkAccess::default(),
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

        PluginManifest {
            manifest_schema_version: LOCAL_MANIFEST_SCHEMA_VERSION,
            id: "system_status".to_string(),
            name: "System Status".to_string(),
            version: "1.0.0".to_string(),
            source: PluginSource::FirstParty,
            author: "Jarvis".to_string(),
            source_path: None,
            subprocess: None,
            wasm: None,
            publisher_signature: None,
            actions: vec![PluginActionManifest {
                name: "status".to_string(),
                description: "Report bounded local first-party capability-host status.".to_string(),
                permissions: vec![PluginPermission::SystemStatus],
                risk_tier: RiskTier::Notify,
                input_schema: JsonSchema::empty_object(),
                output_schema: JsonSchema::object(output_properties, vec!["status".to_string()]),
                proactive: false,
                memory_access: PluginAccess::None,
                model_access: PluginAccess::None,
                network_access: PluginNetworkAccess::default(),
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
            "status": "operational"
        }))
    }
}

#[cfg(test)]
#[derive(Debug, Clone)]
pub(crate) struct FakeStatusPlugin;

#[cfg(test)]
impl InProcessPlugin for FakeStatusPlugin {
    fn manifest(&self) -> PluginManifest {
        let mut manifest = StatusPlugin.manifest();
        manifest.id = "fake_status".to_string();
        manifest.name = "Fake Status".to_string();
        manifest.version = "0.1.0".to_string();
        manifest.actions[0].proactive = true;
        manifest.actions[0]
            .permissions
            .push(PluginPermission::ProactiveRun);
        let properties = manifest.actions[0]
            .output_schema
            .schema
            .get_mut("properties")
            .and_then(Value::as_object_mut)
            .expect("status output properties");
        properties.insert("plugin_count".to_string(), json!({"type":"integer"}));
        manifest
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
        Ok(json!({"status":"ok", "plugin_count":2}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::AtomicBool as TestAtomicBool;
    use std::{fs, thread, time::Duration};

    #[test]
    fn validates_first_party_manifests() {
        let host = PluginHost::with_test_fixtures().expect("host should build");
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
    fn rejects_manifest_when_declared_flags_lack_permissions_or_safe_timeout() {
        let mut proactive = EchoPlugin.manifest();
        proactive.actions[0].proactive = true;
        let error = proactive
            .validate()
            .expect_err("proactive flag must require permission");
        assert!(error
            .to_string()
            .contains("proactive actions must request proactive_run permission"));

        let mut model_access = EchoPlugin.manifest();
        model_access.actions[0].model_access = PluginAccess::Read;
        let error = model_access
            .validate()
            .expect_err("model access must require permission");
        assert!(error
            .to_string()
            .contains("model access requires call_model permission"));

        let mut network_access = EchoPlugin.manifest();
        network_access.actions[0].network_access = PluginNetworkAccess {
            mode: PluginNetworkAccessMode::DeclaredHosts,
            allowed_hosts: vec!["api.example.com".to_string()],
        };
        let error = network_access
            .validate()
            .expect_err("network access must require permission");
        assert!(error
            .to_string()
            .contains("network_access declared_hosts requires network permission"));

        let mut network_without_hosts = EchoPlugin.manifest();
        network_without_hosts.actions[0]
            .permissions
            .push(PluginPermission::Network);
        let error = network_without_hosts
            .validate()
            .expect_err("network permission must declare hosts");
        assert!(error
            .to_string()
            .contains("network permission requires network_access declared_hosts"));

        let mut invalid_network_host = EchoPlugin.manifest();
        invalid_network_host.actions[0]
            .permissions
            .push(PluginPermission::Network);
        invalid_network_host.actions[0].network_access = PluginNetworkAccess {
            mode: PluginNetworkAccessMode::DeclaredHosts,
            allowed_hosts: vec!["https://api.example.com".to_string()],
        };
        let error = invalid_network_host
            .validate()
            .expect_err("network host must be plain hostname");
        assert!(error.to_string().contains("network host"));

        let mut ip_literal_network_host = EchoPlugin.manifest();
        ip_literal_network_host.actions[0]
            .permissions
            .push(PluginPermission::Network);
        ip_literal_network_host.actions[0].network_access = PluginNetworkAccess {
            mode: PluginNetworkAccessMode::DeclaredHosts,
            allowed_hosts: vec!["127.0.0.1".to_string()],
        };
        let error = ip_literal_network_host
            .validate()
            .expect_err("network host must reject IP literals");
        assert!(error.to_string().contains("network host"));

        let mut uppercase_network_host = EchoPlugin.manifest();
        uppercase_network_host.actions[0]
            .permissions
            .push(PluginPermission::Network);
        uppercase_network_host.actions[0].network_access = PluginNetworkAccess {
            mode: PluginNetworkAccessMode::DeclaredHosts,
            allowed_hosts: vec!["API.example.com".to_string()],
        };
        let error = uppercase_network_host
            .validate()
            .expect_err("network host must be lowercase");
        assert!(error.to_string().contains("host must be lowercase"));

        let mut duplicate_network_host = EchoPlugin.manifest();
        duplicate_network_host.actions[0]
            .permissions
            .push(PluginPermission::Network);
        duplicate_network_host.actions[0].network_access = PluginNetworkAccess {
            mode: PluginNetworkAccessMode::DeclaredHosts,
            allowed_hosts: vec!["api.example.com".to_string(), "api.example.com".to_string()],
        };
        let error = duplicate_network_host
            .validate()
            .expect_err("network hosts must be unique");
        assert!(error
            .to_string()
            .contains("duplicates another allowed_host"));

        let mut blocked = EchoPlugin.manifest();
        blocked.actions[0].risk_tier = RiskTier::Block;
        let error = blocked.validate().expect_err("blocked action must fail");
        assert!(error.to_string().contains("cannot register as blocked"));

        let mut long_timeout = EchoPlugin.manifest();
        long_timeout.actions[0].timeout.timeout_ms = MAX_TIMEOUT_MS + 1;
        let error = long_timeout
            .validate()
            .expect_err("excessive timeout must fail");
        assert!(error.to_string().contains("timeout cannot exceed"));
    }

    #[test]
    fn publisher_signature_payload_omits_local_source_path() {
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let trusted_public_key = BASE64_STANDARD.encode(signing_key.verifying_key().as_bytes());
        let mut manifest = EchoPlugin.manifest();
        manifest.source = PluginSource::LocalDevelopment;
        manifest.source_path = Some("/tmp/jarvis-plugin-install-a".to_string());
        manifest.publisher_signature =
            Some(publisher_signature_for_manifest(&manifest, &signing_key));

        manifest
            .verify_publisher_signature(&trusted_public_key)
            .expect("original manifest signature should verify");

        let mut moved_manifest = manifest.clone();
        moved_manifest.source_path = Some("/tmp/jarvis-plugin-install-b".to_string());
        moved_manifest
            .verify_publisher_signature(&trusted_public_key)
            .expect("local source path should not be part of publisher signature identity");

        let mut tampered_manifest = moved_manifest;
        tampered_manifest.version = "0.2.0".to_string();
        let error = tampered_manifest
            .verify_publisher_signature(&trusted_public_key)
            .expect_err("signed manifest identity changes must fail verification");
        assert!(error
            .to_string()
            .contains("publisher signature verification failed"));
    }

    #[test]
    fn rejects_manifest_with_duplicate_action_names() {
        let mut manifest = EchoPlugin.manifest();
        manifest.actions.push(manifest.actions[0].clone());

        let error = manifest.validate().expect_err("manifest should fail");
        assert!(error.to_string().contains("duplicate action echo"));
    }

    #[test]
    fn local_install_accepts_valid_metadata_without_enabling_execution() {
        let dir = tempfile::tempdir().expect("temp plugin dir");
        let manifest_path = dir.path().join("jarvis-plugin.json");
        let source_path = dir.path().canonicalize().expect("canonical source");
        fs::write(
            &manifest_path,
            json!({
                "manifest_schema_version": 1,
                "id": "local_notes",
                "name": "Local Notes",
                "version": "0.1.0",
                "source": "local_development",
                "author": "Local Tester",
                "source_path": source_path.display().to_string(),
                "actions": [{
                    "name": "summarize",
                    "description": "Summarize local notes metadata for validation.",
                    "permissions": ["read_workspace"],
                    "risk_tier": "low",
                    "input_schema": {
                        "schema": {
                            "type": "object",
                            "properties": { "path": { "type": "string" } },
                            "required": ["path"],
                            "additionalProperties": false
                        }
                    },
                    "output_schema": {
                        "schema": {
                            "type": "object",
                            "properties": { "summary": { "type": "string" } },
                            "required": ["summary"],
                            "additionalProperties": false
                        }
                    },
                    "proactive": false,
                    "memory_access": "none",
                    "model_access": "none",
                    "audit_fields": ["path"],
                    "timeout": { "timeout_ms": 5000, "on_timeout": "cancel" },
                    "cancellation": "cooperative"
                }]
            })
            .to_string(),
        )
        .expect("write manifest");

        let installed = InstalledPlugin::from_local_manifest_path(&manifest_path)
            .expect("valid local metadata should install");

        assert_eq!(installed.manifest.id, "local_notes");
        assert_eq!(installed.manifest.source, PluginSource::LocalDevelopment);
        assert!(!installed.execution_enabled);
        assert_eq!(installed.source_path, source_path.display().to_string());
        assert!(installed.provenance.source_tree_sha256.is_some());
        assert_eq!(installed.provenance.source_tree_file_count, Some(1));
    }

    #[test]
    fn local_install_source_tree_provenance_detects_helper_changes() {
        let dir = tempfile::tempdir().expect("temp plugin dir");
        let manifest_path = dir.path().join("jarvis-plugin.json");
        let helper_path = dir.path().join("helper.txt");
        fs::write(&helper_path, "original helper").expect("write helper");
        let source_path = dir.path().canonicalize().expect("canonical source");
        fs::write(
            &manifest_path,
            json!({
                "manifest_schema_version": 1,
                "id": "local_tree",
                "name": "Local Tree",
                "version": "0.1.0",
                "source": "local_development",
                "author": "Local Tester",
                "source_path": source_path.display().to_string(),
                "actions": [{
                    "name": "summarize",
                    "description": "Summarize local tree metadata for validation.",
                    "permissions": ["read_workspace"],
                    "risk_tier": "low",
                    "input_schema": {
                        "schema": {
                            "type": "object",
                            "properties": { "path": { "type": "string" } },
                            "required": ["path"],
                            "additionalProperties": false
                        }
                    },
                    "output_schema": {
                        "schema": {
                            "type": "object",
                            "properties": { "summary": { "type": "string" } },
                            "required": ["summary"],
                            "additionalProperties": false
                        }
                    },
                    "proactive": false,
                    "memory_access": "none",
                    "model_access": "none",
                    "audit_fields": ["path"],
                    "timeout": { "timeout_ms": 5000, "on_timeout": "cancel" },
                    "cancellation": "cooperative"
                }]
            })
            .to_string(),
        )
        .expect("write manifest");

        let installed = InstalledPlugin::from_local_manifest_path(&manifest_path)
            .expect("valid local metadata should install");
        assert_eq!(installed.provenance.source_tree_file_count, Some(2));
        assert_eq!(
            installed
                .provenance
                .verify_snapshot(&installed.manifest, Utc::now())
                .integrity_status,
            InstalledPluginIntegrityStatus::MatchesInstallSnapshot
        );

        fs::write(&helper_path, "changed helper").expect("mutate helper");
        assert_eq!(
            installed
                .provenance
                .verify_snapshot(&installed.manifest, Utc::now())
                .integrity_status,
            InstalledPluginIntegrityStatus::ChangedSinceInstall
        );
    }

    #[test]
    fn source_tree_snapshot_is_stable_and_ignores_generated_artifacts() {
        let left = tempfile::tempdir().expect("left temp plugin dir");
        let right = tempfile::tempdir().expect("right temp plugin dir");
        fs::create_dir_all(left.path().join("nested")).expect("left nested");
        fs::create_dir_all(right.path().join("nested")).expect("right nested");
        fs::write(left.path().join("nested/tool.txt"), "runtime").expect("left tool");
        fs::write(left.path().join("jarvis-plugin.json"), "{}").expect("left manifest");
        fs::write(right.path().join("jarvis-plugin.json"), "{}").expect("right manifest");
        fs::write(right.path().join("nested/tool.txt"), "runtime").expect("right tool");

        let left_snapshot = source_tree_snapshot(left.path()).expect("left snapshot");
        fs::create_dir_all(left.path().join(".git")).expect("left git dir");
        fs::write(left.path().join(".git/config"), "ignored").expect("left git config");
        fs::write(left.path().join(".DS_Store"), "ignored").expect("left ds store");
        fs::create_dir_all(left.path().join("__pycache__")).expect("left pycache");
        fs::write(left.path().join("__pycache__/tool.pyc"), "ignored").expect("left pyc");
        fs::create_dir_all(left.path().join("target/debug")).expect("left target");
        fs::write(left.path().join("target/debug/build.log"), "ignored").expect("left build log");
        let left_with_ignored = source_tree_snapshot(left.path()).expect("left ignored snapshot");
        let right_snapshot = source_tree_snapshot(right.path()).expect("right snapshot");

        assert_eq!(left_snapshot.sha256, left_with_ignored.sha256);
        assert_eq!(left_snapshot.sha256, right_snapshot.sha256);
        assert_eq!(left_snapshot.file_count, 2);

        fs::write(right.path().join("nested/tool.txt"), "changed").expect("right tool changed");
        let changed = source_tree_snapshot(right.path()).expect("changed snapshot");
        assert_ne!(right_snapshot.sha256, changed.sha256);
    }

    #[test]
    fn source_tree_snapshot_rejects_oversized_tree_before_reading_contents() {
        let dir = tempfile::tempdir().expect("oversized temp plugin dir");
        let oversized = fs::File::create(dir.path().join("oversized.bin"))
            .expect("create oversized sparse file");
        oversized
            .set_len(MAX_PLUGIN_SOURCE_TREE_BYTES + 1)
            .expect("size oversized sparse file");

        let error = source_tree_snapshot(dir.path()).expect_err("reject oversized source tree");

        assert!(error
            .to_string()
            .contains("plugin source tree exceeds 67108864 bytes"));
    }

    #[test]
    fn source_tree_snapshot_rejects_excessive_entry_fanout_and_depth() {
        let fanout = tempfile::tempdir().expect("fanout temp plugin dir");
        for index in 0..=MAX_PLUGIN_SOURCE_TREE_ENTRIES {
            fs::create_dir(fanout.path().join(format!("dir-{index:05}")))
                .expect("create fanout directory");
        }
        let fanout_error =
            source_tree_snapshot(fanout.path()).expect_err("reject excessive fanout");
        assert!(fanout_error
            .to_string()
            .contains("plugin source tree exceeds 8192 entries"));

        let depth = tempfile::tempdir().expect("depth temp plugin dir");
        let mut current = depth.path().to_path_buf();
        for index in 0..=MAX_PLUGIN_SOURCE_TREE_DEPTH {
            current = current.join(format!("d{index}"));
            fs::create_dir(&current).expect("create nested directory");
        }
        let depth_error = source_tree_snapshot(depth.path()).expect_err("reject excessive depth");
        assert!(depth_error
            .to_string()
            .contains("plugin source tree exceeds depth 64"));
    }

    #[cfg(unix)]
    #[test]
    fn source_tree_snapshot_rejects_symlinks_and_case_collisions() {
        use std::os::unix::fs::symlink;

        let symlink_dir = tempfile::tempdir().expect("symlink temp plugin dir");
        fs::write(symlink_dir.path().join("jarvis-plugin.json"), "{}").expect("manifest");
        symlink("/bin/echo", symlink_dir.path().join("echo-link")).expect("symlink");
        let symlink_error = source_tree_snapshot(symlink_dir.path()).expect_err("reject symlink");
        assert!(symlink_error.to_string().contains("cannot include symlink"));

        let collision_dir = tempfile::tempdir().expect("collision temp plugin dir");
        let upper = collision_dir.path().join("README.md");
        let lower = collision_dir.path().join("readme.md");
        fs::write(&upper, "upper").expect("upper");
        fs::write(&lower, "lower").expect("lower");
        let upper_path = fs::canonicalize(&upper).expect("canonical upper");
        let lower_path = fs::canonicalize(&lower).expect("canonical lower");
        if upper_path != lower_path {
            let collision_error =
                source_tree_snapshot(collision_dir.path()).expect_err("reject path collision");
            assert!(collision_error
                .to_string()
                .contains("case-insensitive duplicate path"));
        }
    }

    #[test]
    fn local_install_rejects_first_party_claims_and_unsafe_paths() {
        let dir = tempfile::tempdir().expect("temp plugin dir");
        let manifest_path = dir.path().join("jarvis-plugin.json");
        let source_path = dir.path().canonicalize().expect("canonical source");
        fs::write(
            &manifest_path,
            json!({
                "manifest_schema_version": 1,
                "id": "fake_claim",
                "name": "Fake Claim",
                "version": "0.1.0",
                "source": "first_party",
                "author": "Local Tester",
                "source_path": source_path.display().to_string(),
                "actions": [{
                    "name": "echo",
                    "description": "Invalid first-party claim.",
                    "permissions": [],
                    "risk_tier": "low",
                    "input_schema": { "schema": { "type": "object" } },
                    "output_schema": { "schema": { "type": "object" } },
                    "proactive": false,
                    "memory_access": "none",
                    "model_access": "none",
                    "audit_fields": [],
                    "timeout": { "timeout_ms": 5000, "on_timeout": "cancel" },
                    "cancellation": "cooperative"
                }]
            })
            .to_string(),
        )
        .expect("write manifest");

        let error = InstalledPlugin::from_local_manifest_path(&manifest_path)
            .expect_err("local install cannot claim first-party source");
        assert!(error.to_string().contains("cannot claim first_party"));

        fs::write(
            &manifest_path,
            json!({
                "manifest_schema_version": 1,
                "id": "relative_source",
                "name": "Relative Source",
                "version": "0.1.0",
                "source": "local_development",
                "author": "Local Tester",
                "source_path": "../relative",
                "actions": [{
                    "name": "echo",
                    "description": "Invalid relative source path.",
                    "permissions": [],
                    "risk_tier": "low",
                    "input_schema": { "schema": { "type": "object" } },
                    "output_schema": { "schema": { "type": "object" } },
                    "proactive": false,
                    "memory_access": "none",
                    "model_access": "none",
                    "audit_fields": [],
                    "timeout": { "timeout_ms": 5000, "on_timeout": "cancel" },
                    "cancellation": "cooperative"
                }]
            })
            .to_string(),
        )
        .expect("write manifest");

        let error = InstalledPlugin::from_local_manifest_path(&manifest_path)
            .expect_err("local install source path must be absolute");
        assert!(error.to_string().contains("source_path must be absolute"));
    }

    #[test]
    fn echo_plugin_round_trips_input_and_metadata() {
        let host = PluginHost::with_test_fixtures().expect("host should build");
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
        let host = PluginHost::with_test_fixtures().expect("host should build");
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
        let host = PluginHost::with_test_fixtures().expect("host should build");
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
        let host = PluginHost::with_test_fixtures().expect("host should build");
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
        let host = PluginHost::with_test_fixtures().expect("host should build");
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

    #[test]
    fn external_cancellation_dominates_worker_error_race() {
        let cancelled = Arc::new(TestAtomicBool::new(false));
        let mut host = PluginHost::new();
        host.register(ErrorAfterCancellationPlugin {
            cancelled: Arc::clone(&cancelled),
        })
        .expect("register cancellation race plugin");

        let result = host
            .execute_cancellable(
                PluginCallRequest::reactive("cancel_race", "run", json!({}))
                    .with_granted_scopes(vec![CapabilityScope::PluginRun]),
                || cancelled.load(Ordering::SeqCst),
            )
            .expect("cancellation result");

        assert_eq!(result.status, PluginCallStatus::Cancelled);
        assert_eq!(result.output, json!({"cancelled": true}));
    }

    #[test]
    fn local_subprocess_output_within_limits_executes_and_parses_progress() {
        let fixture = local_subprocess_fixture(
            r#"#!/bin/sh
printf '%s\n' '{"jarvis_progress":true,"stage":"prepare","message":"bounded output"}' >&2
printf '%s\n' '{"ok":true}'
"#,
        );
        let manifest = local_subprocess_manifest("bounded_success", "runner.sh");
        let source_path = fixture.path().canonicalize().expect("canonical fixture");

        let execution = execute_installed_subprocess_plugin(
            &manifest,
            &manifest.actions[0],
            &source_path,
            &json!({}),
        )
        .expect("bounded subprocess should execute");

        assert_eq!(execution.output, json!({ "ok": true }));
        assert!(execution.stdout_bytes < MAX_PLUGIN_STDOUT_BYTES);
        assert!(execution.stderr_bytes < MAX_PLUGIN_STDERR_BYTES);
        assert_eq!(execution.progress_events.len(), 1);
        assert_eq!(execution.progress_events[0].stage, "prepare");
    }

    #[cfg(unix)]
    #[test]
    fn local_subprocess_cancellation_terminates_descendant_process_group() {
        let fixture = local_subprocess_fixture(
            r#"#!/bin/sh
trap '' TERM
(
  trap '' TERM
  while :; do
    printf x >> "$JARVIS_PLUGIN_SOURCE_PATH/heartbeat"
    sleep 0.01
  done
) &
while :; do sleep 1; done
"#,
        );
        let manifest = local_subprocess_manifest("cancel_process_group", "runner.sh");
        let source_path = fixture.path().canonicalize().expect("canonical fixture");
        let heartbeat = source_path.join("heartbeat");

        let error = execute_installed_subprocess_plugin_cancellable(
            &manifest,
            &manifest.actions[0],
            &source_path,
            &json!({}),
            || {
                if fs::metadata(&heartbeat).is_ok_and(|metadata| metadata.len() >= 3) {
                    SubprocessControlState::Cancelled
                } else {
                    SubprocessControlState::Continue
                }
            },
        )
        .expect_err("cancellation must terminate the subprocess group");

        assert!(error.to_string().contains("cancelled"), "{error}");
        let stopped_len = fs::metadata(&heartbeat).expect("heartbeat evidence").len();
        thread::sleep(Duration::from_millis(200));
        assert_eq!(
            fs::metadata(&heartbeat).expect("stable heartbeat").len(),
            stopped_len,
            "a descendant continued running after cancellation returned"
        );
    }

    #[cfg(unix)]
    #[test]
    fn local_subprocess_emergency_pause_terminates_and_reaps_promptly() {
        let fixture = local_subprocess_fixture(
            r#"#!/bin/sh
trap '' TERM
while :; do
  printf x >> "$JARVIS_PLUGIN_SOURCE_PATH/heartbeat"
  sleep 0.01
done
"#,
        );
        let manifest = local_subprocess_manifest("pause_process_group", "runner.sh");
        let source_path = fixture.path().canonicalize().expect("canonical fixture");
        let heartbeat = source_path.join("heartbeat");
        let started = Instant::now();

        let error = execute_installed_subprocess_plugin_cancellable(
            &manifest,
            &manifest.actions[0],
            &source_path,
            &json!({}),
            || {
                if fs::metadata(&heartbeat).is_ok_and(|metadata| metadata.len() >= 1) {
                    SubprocessControlState::EmergencyPaused
                } else {
                    SubprocessControlState::Continue
                }
            },
        )
        .expect_err("emergency pause must terminate the subprocess group");

        assert!(matches!(error, JarvisError::PolicyBlocked(_)), "{error}");
        assert!(error.to_string().contains("emergency pause"), "{error}");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "pause waited for the plugin timeout instead of terminating it"
        );
        let stopped_len = fs::metadata(&heartbeat).expect("heartbeat evidence").len();
        thread::sleep(Duration::from_millis(200));
        assert_eq!(
            fs::metadata(&heartbeat).expect("stable heartbeat").len(),
            stopped_len,
            "the emergency-paused subprocess continued after cleanup returned"
        );
    }

    #[cfg(unix)]
    #[test]
    fn process_group_probe_permission_denied_is_not_treated_as_absent() {
        assert_eq!(
            classify_subprocess_group_probe(Err(Errno::PERM)).unwrap(),
            SubprocessGroupInspection::PresentButNotSignalable
        );
        assert_eq!(
            classify_subprocess_group_probe(Err(Errno::SRCH)).unwrap(),
            SubprocessGroupInspection::Absent
        );
    }

    #[test]
    fn emergency_pause_cleanup_uncertainty_is_elevated_with_primary_evidence() {
        let error = attach_subprocess_cleanup_failure(
            JarvisError::PolicyBlocked("emergency pause".to_string()),
            JarvisError::Plugin("process group was not signalable".to_string()),
        );
        assert!(matches!(error, JarvisError::Plugin(_)), "{error}");
        assert!(error.to_string().contains("emergency pause"), "{error}");
        assert!(error.to_string().contains("cleanup failure"), "{error}");
        assert!(error.to_string().contains("not signalable"), "{error}");
    }

    #[test]
    fn local_subprocess_stdout_over_limit_fails_closed() {
        let fixture = local_subprocess_fixture(&format!(
            r#"#!/bin/sh
python3 - <<'PY'
import sys
sys.stdout.write("x" * {})
sys.stdout.flush()
PY
"#,
            MAX_PLUGIN_STDOUT_BYTES + 1
        ));
        let manifest = local_subprocess_manifest("noisy_stdout", "runner.sh");
        let source_path = fixture.path().canonicalize().expect("canonical fixture");

        let error = execute_installed_subprocess_plugin(
            &manifest,
            &manifest.actions[0],
            &source_path,
            &json!({}),
        )
        .expect_err("oversize stdout must fail closed");

        assert!(error.to_string().contains("stdout exceeded"), "{error}");
        assert!(error.to_string().contains("byte limit"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn local_subprocess_output_limit_unblocks_pending_stdin_and_reaps() {
        let fixture = local_subprocess_fixture(&format!(
            r#"#!/bin/sh
python3 - <<'PY'
import sys
sys.stdout.write("x" * {})
sys.stdout.flush()
PY
sleep 30
"#,
            MAX_PLUGIN_STDOUT_BYTES + 1
        ));
        let manifest = local_subprocess_manifest("blocked_stdin_noisy_stdout", "runner.sh");
        let source_path = fixture.path().canonicalize().expect("canonical fixture");
        let oversized_request = json!({ "payload": "x".repeat(2 * 1024 * 1024) });
        let started = Instant::now();

        let error = execute_installed_subprocess_plugin(
            &manifest,
            &manifest.actions[0],
            &source_path,
            &oversized_request,
        )
        .expect_err("output limit must terminate a child blocked on unread stdin");

        assert!(error.to_string().contains("stdout exceeded"), "{error}");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "blocked stdin prevented bounded process-group cleanup"
        );
    }

    #[test]
    fn local_subprocess_stderr_over_limit_fails_closed() {
        let fixture = local_subprocess_fixture(&format!(
            r#"#!/bin/sh
python3 - <<'PY'
import sys
sys.stderr.write("x" * {})
sys.stderr.flush()
sys.stdout.write('{{"ok":true}}')
PY
"#,
            MAX_PLUGIN_STDERR_BYTES + 1
        ));
        let manifest = local_subprocess_manifest("noisy_stderr", "runner.sh");
        let source_path = fixture.path().canonicalize().expect("canonical fixture");

        let error = execute_installed_subprocess_plugin(
            &manifest,
            &manifest.actions[0],
            &source_path,
            &json!({}),
        )
        .expect_err("oversize stderr must fail closed");

        assert!(error.to_string().contains("stderr exceeded"), "{error}");
        assert!(error.to_string().contains("byte limit"), "{error}");
    }

    fn local_subprocess_fixture(script: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("temp plugin dir");
        let runner = dir.path().join("runner.sh");
        fs::write(&runner, script).expect("write runner");
        #[cfg(unix)]
        {
            let mut permissions = fs::metadata(&runner)
                .expect("runner metadata")
                .permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&runner, permissions).expect("chmod runner");
        }
        dir
    }

    fn local_subprocess_manifest(id: &str, command: &str) -> PluginManifest {
        let mut output_properties = Map::new();
        output_properties.insert("ok".to_string(), json!({ "type": "boolean" }));
        PluginManifest {
            manifest_schema_version: LOCAL_MANIFEST_SCHEMA_VERSION,
            id: id.to_string(),
            name: "Local Subprocess Fixture".to_string(),
            version: "0.1.0".to_string(),
            source: PluginSource::LocalSubprocess,
            author: "Jarvis Test".to_string(),
            source_path: None,
            subprocess: Some(PluginSubprocessManifest {
                command: command.to_string(),
                args: Vec::new(),
                stdin: PluginSubprocessStream::Json,
                stdout: PluginSubprocessStream::Json,
            }),
            wasm: None,
            publisher_signature: None,
            actions: vec![PluginActionManifest {
                name: "run".to_string(),
                description: "Run local subprocess fixture.".to_string(),
                permissions: Vec::new(),
                risk_tier: RiskTier::Low,
                input_schema: JsonSchema::empty_object(),
                output_schema: JsonSchema::object(output_properties, vec!["ok".to_string()]),
                proactive: false,
                memory_access: PluginAccess::None,
                model_access: PluginAccess::None,
                network_access: PluginNetworkAccess::default(),
                audit_fields: Vec::new(),
                timeout: PluginTimeout {
                    // Python startup and pipe draining can exceed two seconds when the
                    // full test suite runs subprocess-heavy cases in parallel. Keep
                    // this fixture comfortably below the production maximum while
                    // ensuring the output-bound assertion remains the failure source.
                    timeout_ms: 10_000,
                    on_timeout: PluginTimeoutAction::Cancel,
                },
                cancellation: CancellationBehavior::Cooperative,
            }],
        }
    }

    fn publisher_signature_for_manifest(
        manifest: &PluginManifest,
        signing_key: &SigningKey,
    ) -> PluginPublisherSignature {
        let payload = manifest
            .publisher_signature_payload()
            .expect("publisher signature payload");
        let signature = signing_key.sign(&payload);
        PluginPublisherSignature {
            scheme: PluginPublisherSignature::ED25519_V1.to_string(),
            public_key: BASE64_STANDARD.encode(signing_key.verifying_key().as_bytes()),
            signature: BASE64_STANDARD.encode(signature.to_bytes()),
        }
    }

    struct ApprovalPlugin;

    impl InProcessPlugin for ApprovalPlugin {
        fn manifest(&self) -> PluginManifest {
            let mut properties = Map::new();
            properties.insert("message".to_string(), json!({ "type": "string" }));
            PluginManifest {
                manifest_schema_version: LOCAL_MANIFEST_SCHEMA_VERSION,
                id: "approval_plugin".to_string(),
                name: "Approval Plugin".to_string(),
                version: "0.1.0".to_string(),
                source: PluginSource::FirstParty,
                author: "Jarvis".to_string(),
                source_path: None,
                subprocess: None,
                wasm: None,
                publisher_signature: None,
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
                    network_access: PluginNetworkAccess::default(),
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

    struct ErrorAfterCancellationPlugin {
        cancelled: Arc<TestAtomicBool>,
    }

    impl InProcessPlugin for ErrorAfterCancellationPlugin {
        fn manifest(&self) -> PluginManifest {
            let mut manifest = SlowPlugin.manifest();
            manifest.id = "cancel_race".to_string();
            manifest.name = "Cancellation Race".to_string();
            manifest.actions[0].name = "run".to_string();
            manifest.actions[0].timeout.timeout_ms = 1_000;
            manifest
        }

        fn execute(
            &self,
            _action: &PluginActionManifest,
            _input: Value,
            _cancellation: CancellationSignal,
        ) -> JarvisResult<Value> {
            self.cancelled.store(true, Ordering::SeqCst);
            Err(JarvisError::Plugin(
                "worker failed after cancellation".to_string(),
            ))
        }
    }

    impl InProcessPlugin for SlowPlugin {
        fn manifest(&self) -> PluginManifest {
            PluginManifest {
                manifest_schema_version: LOCAL_MANIFEST_SCHEMA_VERSION,
                id: "slow_plugin".to_string(),
                name: "Slow Plugin".to_string(),
                version: "0.1.0".to_string(),
                source: PluginSource::FirstParty,
                author: "Jarvis".to_string(),
                source_path: None,
                subprocess: None,
                wasm: None,
                publisher_signature: None,
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
                    network_access: PluginNetworkAccess::default(),
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

    #[test]
    fn semantic_version_update_order_follows_semver_precedence() {
        for (current, candidate) in [
            ("1.0.0", "1.0.1"),
            ("1.9.9", "2.0.0"),
            ("1.0.0-alpha.1", "1.0.0-alpha.beta"),
            ("1.0.0-rc.1", "1.0.0"),
            ("1.0.0+build.1", "1.0.1+build.1"),
        ] {
            require_strictly_newer_semantic_version(current, candidate).unwrap();
        }
        for (current, candidate) in [
            ("1.0.0", "1.0.0"),
            ("1.0.0+build.1", "1.0.0+build.2"),
            ("2.0.0", "1.9.9"),
            ("1.0.0", "1.0.0-rc.1"),
        ] {
            assert!(require_strictly_newer_semantic_version(current, candidate).is_err());
        }
    }

    #[test]
    fn semantic_version_update_rejects_invalid_versions() {
        for invalid in ["1", "1.0", "01.0.0", "1.00.0", "1.0.0-01", "1.0.0+"] {
            assert!(require_strictly_newer_semantic_version("1.0.0", invalid).is_err());
        }
    }

    #[test]
    fn semantic_version_supports_unbounded_numeric_identifiers() {
        let huge = "9".repeat(256);
        let huger = format!("1{huge}");
        require_strictly_newer_semantic_version(&format!("{huge}.0.0"), &format!("{huger}.0.0"))
            .unwrap();
        require_strictly_newer_semantic_version(
            &format!("1.0.0-{huge}"),
            &format!("1.0.0-{huger}"),
        )
        .unwrap();
        assert!(require_valid_semantic_version(&format!("1.0.0-0{huge}")).is_err());
    }

    #[test]
    fn installed_plugin_update_allows_one_legacy_version_migration() {
        require_installed_plugin_update_version("legacy-v1", "0.1.0").unwrap();
        assert!(require_installed_plugin_update_version("legacy-v1", "still-legacy").is_err());
        assert!(require_installed_plugin_update_version("0.1.0", "0.1.0").is_err());
    }

    #[test]
    fn local_manifest_install_rejects_non_semver_version() {
        let dir = local_subprocess_fixture("#!/bin/sh\nprintf '{\"ok\":true}'\n");
        let source_path = dir.path().canonicalize().expect("canonical plugin dir");
        let manifest_path = source_path.join("jarvis-plugin.json");
        let mut manifest = local_subprocess_manifest("invalid_version", "runner.sh");
        manifest.version = "release-one".to_string();
        manifest.source_path = Some(source_path.display().to_string());
        fs::write(
            &manifest_path,
            serde_json::to_vec_pretty(&manifest).expect("serialize manifest"),
        )
        .expect("write manifest");

        let error = InstalledPlugin::from_local_manifest_path(&manifest_path)
            .expect_err("new local install requires SemVer");
        assert!(error.to_string().contains("valid SemVer 2.0.0"));
    }
}
