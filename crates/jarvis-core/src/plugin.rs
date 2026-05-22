use crate::{
    ApprovalDecision, ApprovalGrant, ApprovalStatus, CapabilityScope, JarvisError, JarvisResult,
    PermissionEngine, PolicyRequest, RiskTier, Sensitivity,
};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use chrono::{DateTime, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

const DEFAULT_TIMEOUT_MS: u64 = 5_000;
const MAX_TIMEOUT_MS: u64 = 60_000;
const LOCAL_MANIFEST_SCHEMA_VERSION: u16 = 1;

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

        if self.risk_tier == RiskTier::Block {
            return Err(JarvisError::Validation(format!(
                "{plugin_id}.{} cannot register as blocked",
                self.name
            )));
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
    #[serde(default)]
    pub source_path: Option<String>,
    #[serde(default)]
    pub subprocess: Option<PluginSubprocessManifest>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher_signature: Option<PluginPublisherSignature>,
    pub actions: Vec<PluginActionManifest>,
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
        signature.verify(&self.id, trusted_public_key, &self.signature_payload()?)
    }

    fn signature_payload(&self) -> JarvisResult<Vec<u8>> {
        let mut unsigned = self.clone();
        unsigned.publisher_signature = None;
        serde_json::to_vec(&unsigned).map_err(|err| {
            JarvisError::Validation(format!("{} publisher signature payload: {err}", self.id))
        })
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subprocess_command_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subprocess_command_sha256: Option<String>,
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
            subprocess_command_path: None,
            subprocess_command_sha256: None,
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
        let mut provenance = Self {
            provenance_schema_version: 1,
            capture_method: "local_manifest_snapshot".to_string(),
            manifest_path: manifest_path.display().to_string(),
            manifest_sha256: sha256_file(&manifest_path)?,
            source_path: source_path.display().to_string(),
            source_path_canonicalized: true,
            subprocess_command_path: None,
            subprocess_command_sha256: None,
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
            provenance.subprocess_command_sha256 = Some(sha256_file(&command_path)?);
            provenance.subprocess_command_path = Some(command_path.display().to_string());
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

        Ok(())
    }
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
}

impl InstalledPluginExecutionGrant {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MetadataOnly => "metadata_only",
            Self::SubprocessStdio => "subprocess_stdio",
        }
    }

    pub fn parse(value: &str) -> JarvisResult<Self> {
        match value {
            "metadata_only" => Ok(Self::MetadataOnly),
            "subprocess_stdio" => Ok(Self::SubprocessStdio),
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
}

pub fn execute_installed_subprocess_plugin(
    manifest: &PluginManifest,
    action: &PluginActionManifest,
    source_path: &Path,
    input: &Value,
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

    let mut child = Command::new(executable)
        .args(&subprocess.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| JarvisError::Plugin(format!("spawn subprocess plugin: {err}")))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(&request_bytes)
            .map_err(|err| JarvisError::Plugin(format!("write subprocess stdin: {err}")))?;
    }

    let deadline = Instant::now() + action.timeout.duration();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|err| JarvisError::Plugin(format!("wait subprocess plugin: {err}")))?
        {
            let output = child
                .wait_with_output()
                .map_err(|err| JarvisError::Plugin(format!("read subprocess output: {err}")))?;
            if !status.success() {
                return Err(JarvisError::Plugin(format!(
                    "subprocess plugin exited with status {status}"
                )));
            }
            let stdout_bytes = output.stdout.len();
            let stderr_bytes = output.stderr.len();
            let value: Value = serde_json::from_slice(&output.stdout).map_err(|err| {
                JarvisError::Plugin(format!("parse subprocess stdout JSON: {err}"))
            })?;
            action
                .output_schema
                .validate_value(&format!("{}.{} output", manifest.id, action.name), &value)?;
            return Ok(SubprocessPluginExecution {
                output: value,
                stdout_bytes,
                stderr_bytes,
                exit_code: status.code(),
            });
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(JarvisError::Plugin(format!(
                "subprocess plugin timed out after {}ms",
                action.timeout.timeout_ms
            )));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
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
    let bytes = fs::read(path)
        .map_err(|err| JarvisError::Validation(format!("hash {}: {err}", path.display())))?;
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
            manifest_schema_version: LOCAL_MANIFEST_SCHEMA_VERSION,
            id: "fake_echo".to_string(),
            name: "Fake Echo".to_string(),
            version: "0.1.0".to_string(),
            source: PluginSource::FirstParty,
            author: "Jarvis".to_string(),
            source_path: None,
            subprocess: None,
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
            manifest_schema_version: LOCAL_MANIFEST_SCHEMA_VERSION,
            id: "fake_status".to_string(),
            name: "Fake Status".to_string(),
            version: "0.1.0".to_string(),
            source: PluginSource::FirstParty,
            author: "Jarvis".to_string(),
            source_path: None,
            subprocess: None,
            publisher_signature: None,
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
    use std::{fs, thread, time::Duration};

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
                manifest_schema_version: LOCAL_MANIFEST_SCHEMA_VERSION,
                id: "approval_plugin".to_string(),
                name: "Approval Plugin".to_string(),
                version: "0.1.0".to_string(),
                source: PluginSource::FirstParty,
                author: "Jarvis".to_string(),
                source_path: None,
                subprocess: None,
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
                manifest_schema_version: LOCAL_MANIFEST_SCHEMA_VERSION,
                id: "slow_plugin".to_string(),
                name: "Slow Plugin".to_string(),
                version: "0.1.0".to_string(),
                source: PluginSource::FirstParty,
                author: "Jarvis".to_string(),
                source_path: None,
                subprocess: None,
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
