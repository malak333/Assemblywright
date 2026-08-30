use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, HashSet};
use std::net::SocketAddr;
use uuid::Uuid;

pub const PROTOCOL_VERSION: u16 = 5;
pub const MAX_DEVICE_NAME_BYTES: usize = 128;
pub const MAX_CAPABILITIES_PER_DEVICE: usize = 64;
pub const MAX_CAPABILITY_ID_BYTES: usize = 64;
pub const MAX_PROVIDER_NAME_BYTES: usize = 64;
pub const MAX_MODEL_NAME_BYTES: usize = 128;
pub const LOCAL_MODEL_SELECTION_SCHEMA_VERSION: u16 = 1;
pub const MAX_LOCAL_MODEL_SELECTION_FRAME_BYTES: usize = 8 * 1024;
pub const MAX_HANDSHAKE_FRAME_BYTES: usize = 64 * 1024;
pub const MAX_JOB_CONTEXT_BYTES: usize = 256 * 1024;
pub const MAX_JOB_RESULT_BYTES: usize = 768 * 1024;
pub const MAX_WIRE_FRAME_BYTES: usize = 1024 * 1024;
pub const MAX_DISTRIBUTED_EVENT_BATCH_BYTES: usize = 64 * 1024;
pub const MAX_DISTRIBUTED_EVENTS_PER_BATCH: usize = 64;
pub const MAX_LEASE_DURATION_MS: u64 = 10 * 60 * 1000;
pub const MAX_STEP_DEADLINE_MS: u64 = 2 * 60 * 60 * 1000;
pub const ENROLLMENT_PAIRING_SCHEMA_VERSION: u16 = 1;
pub const MAX_ENROLLMENT_CSR_PEM_BYTES: usize = 64 * 1024;
pub const MAX_ENROLLMENT_PAIRING_FRAME_BYTES: usize = 64 * 1024;
pub const ENROLLMENT_INVITATION_READY_STATUS: &str = "enrollment_invitation_ready";
pub const ENROLLMENT_CSR_READY_STATUS: &str = "enrollment_csr_ready";
pub const FIXTURE_REASONING_CAPABILITY_ID: &str = "fixture.reasoning";
pub const FIXTURE_REASONING_PROVIDER: &str = "assemblywright-fixture";
pub const FIXTURE_REASONING_MODEL: &str = "assemblywright-fixture-v1";
pub const FIXTURE_SYNTHETIC_ECHO_OPERATION: &str = "synthetic_echo";
pub const MAX_FIXTURE_CONTEXT_BYTES: usize = 8 * 1024;
pub const MAX_FIXTURE_RESULT_BYTES: usize = 8 * 1024;
pub const MAX_FIXTURE_INPUT_BYTES: usize = 4 * 1024;
pub const MAX_FIXTURE_DELAY_MS: u64 = 5_000;
pub const CANCELLATION_ACK_DEADLINE_MS: u64 = 2_000;
pub const MLX_REASONING_CAPABILITY_ID: &str = "mlx.reasoning";
pub const MLX_REASONING_PROVIDER: &str = "mlx";
pub const MLX_GENERATE_TEXT_OPERATION: &str = "generate_text";
pub const MAX_MLX_PROMPT_BYTES: usize = 32 * 1024;
pub const MAX_MLX_TOKENS: u32 = 512;
pub const MAX_MLX_TEMPERATURE_MILLI: u32 = 2_000;
pub const FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION: u16 = 1;
pub const MAX_FEATURE_CONVEYOR_APPROVED_MANIFEST_BYTES: usize = 256 * 1024;
pub const MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES: usize = 320 * 1024;
pub const MAX_FEATURE_CONVEYOR_DEPENDENCIES: usize = 100;
pub const MAX_FEATURE_CONVEYOR_IDENTIFIER_BYTES: usize = 128;
pub const MAX_FEATURE_CONVEYOR_REPOSITORY_PREFLIGHT_REQUEST_BYTES: usize = 8 * 1024;
pub const MAX_FEATURE_CONVEYOR_SNAPSHOT_CLAIM_REQUEST_BYTES: usize = 12 * 1024;
pub const MAX_FEATURE_CONVEYOR_CODING_DISPATCH_REQUEST_BYTES: usize = 256 * 1024;
pub const MAX_FEATURE_CONVEYOR_OWNER_RESOLUTION_REQUEST_BYTES: usize = 4 * 1024;
pub const MAX_FEATURE_CONVEYOR_REPOSITORY_PATH_BYTES: usize = 4 * 1024;
pub const MAX_FEATURE_CONVEYOR_BASE_BRANCH_BYTES: usize = 255;
pub const LOCAL_CODING_CAPABILITY_ID: &str = "local.coding.v1";
pub const LOCAL_CODING_PROVIDER: &str = "assemblywright-agent";
pub const LOCAL_CODING_MODEL: &str = "assemblywright-local-coding-v1";
/// Compatibility fixture used by tests and live proof. General coding packets
/// may select other protocol-validated relative paths.
pub const LOCAL_CODING_FIXTURE_ALLOWED_PATH: &str = "README.md";
pub const LOCAL_CODING_COMPLETED_STATUS: &str = "contained_coding_completed";
pub const LOCAL_CODING_FIXTURE_TEST_STATUS: &str = "not_run";
pub const MAX_LOCAL_CODING_JOB_FRAME_BYTES: usize = 16 * 1024;
pub const MAX_LOCAL_CODING_CONTEXT_BYTES: usize = 12 * 1024;
pub const MAX_LOCAL_CODING_RESULT_BYTES: usize = 32 * 1024;
pub const MAX_LOCAL_CODING_SNAPSHOT_CHUNK_BYTES: usize = 128 * 1024;
pub const MAX_LOCAL_CODING_SNAPSHOT_BUNDLE_BYTES: u64 = 320 * 1024 * 1024;
pub const MAX_LOCAL_CODING_SNAPSHOT_CHUNK_FRAME_BYTES: usize = 384 * 1024;
pub const MAX_LOCAL_CODING_RESULT_ARTIFACT_BYTES: usize = 256 * 1024;
pub const MAX_LOCAL_CODING_RESULT_ARTIFACT_FRAME_BYTES: usize = 640 * 1024;
pub const MAX_LOCAL_CODING_EDIT_PATHS: usize = 64;
pub const MAX_LOCAL_CODING_EDIT_OPERATIONS: usize = 64;
pub const MAX_LOCAL_CODING_EDIT_PATH_BYTES: usize = 1024;
pub const MAX_LOCAL_CODING_EDIT_CONTENT_BYTES: usize = 4 * 1024;
pub const LOCAL_CODING_WRITE_FILE_TOOL_ID: &str = "file.write.v1";
pub const LOCAL_CODING_DELETE_FILE_TOOL_ID: &str = "file.delete.v1";
pub const LOCAL_CODING_RESULT_ARTIFACT_FORMAT: &str = "assemblywright.multi-file-patch.v1";
const LOCAL_CODING_V4_RESULT_ARTIFACT_FORMAT: &str = "assemblywright.readme-replacement.v1";
pub const LOCAL_CODING_RESULT_ARTIFACT_STATUS: &str = "result_artifact_admitted";
pub const LOCAL_CODING_FIXTURE_CONTENT: &[u8] = b"assemblywright contained coding fixture\n";
pub const FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION: u16 = 1;
pub const MAX_GITHUB_REPOSITORY_URL_BYTES: usize = 256;
pub const MAX_BRAINSTORMING_INPUT_BYTES: usize = 16 * 1024;
pub const MAX_BRAINSTORMING_SPECIFICATION_BYTES: usize = 64 * 1024;
pub const MAX_BRAINSTORMING_ITEMS: usize = 100;
pub const MAX_ORCHESTRATOR_PROFILES: usize = 64;
pub const MAX_ASSEMBLY_LINE_QUEUE_COUNT: u16 = 100;
pub const MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES: usize = 96 * 1024;
pub const MAX_ASSEMBLY_LINE_REPOSITORIES: usize = 100;
pub const MAX_ASSEMBLY_LINE_OWNER_PROJECTION_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ProtocolError {
    #[error("unsupported protocol version: expected {expected}, received {received}")]
    UnsupportedVersion { expected: u16, received: u16 },
    #[error("{field} must not be empty")]
    EmptyField { field: &'static str },
    #[error("{field} must not be a nil UUID")]
    NilIdentifier { field: &'static str },
    #[error("{field} exceeds the {maximum}-byte limit")]
    FieldTooLarge { field: &'static str, maximum: usize },
    #[error("{field} must contain only printable ASCII without surrounding whitespace")]
    InvalidIdentifier { field: &'static str },
    #[error("capability count exceeds the {maximum}-entry limit")]
    TooManyCapabilities { maximum: usize },
    #[error("duplicate capability id: {0}")]
    DuplicateCapability(String),
    #[error("{field} must be between 1 and {maximum}")]
    InvalidLimit { field: &'static str, maximum: u64 },
    #[error("{field} must be a JSON object")]
    ExpectedObject { field: &'static str },
    #[error("{field} exceeds the {maximum}-byte serialized limit")]
    SerializedValueTooLarge { field: &'static str, maximum: usize },
    #[error("{field} frame exceeds the {maximum}-byte limit")]
    FrameTooLarge { field: &'static str, maximum: usize },
    #[error("failed to decode {field}: {message}")]
    Deserialization {
        field: &'static str,
        message: String,
    },
    #[error("failed to serialize {field} for bounds validation: {message}")]
    Serialization {
        field: &'static str,
        message: String,
    },
    #[error("result identity does not match its leased job")]
    ResultIdentityMismatch,
    #[error("{field} does not match the SHA-256 digest of its payload")]
    PayloadDigestMismatch { field: &'static str },
    #[error("TLS channel binding must not be all zeroes")]
    InvalidChannelBinding,
    #[error("unsupported {field}: expected {expected}, received {received}")]
    UnsupportedFixedValue {
        field: &'static str,
        expected: String,
        received: String,
    },
    #[error("{field} must be a concrete IP endpoint with a nonzero port")]
    InvalidSocketEndpoint { field: &'static str },
    #[error("{field} must be a lowercase 64-character SHA-256 hex digest")]
    InvalidSha256Hex { field: &'static str },
    #[error("enrollment invitation is expired")]
    EnrollmentInvitationExpired,
    #[error("event cursor stream id must not be nil")]
    NilEventStreamIdentifier,
    #[error("event cursor sequence must advance contiguously")]
    EventCursorGap,
    #[error("event batch exceeds the {maximum}-entry limit")]
    TooManyDistributedEvents { maximum: usize },
    #[error("distributed event identity fields do not match its kind")]
    DistributedEventIdentityMismatch,
    #[error("fixture reasoning capability must use the exact fixture contract")]
    InvalidFixtureCapability,
    #[error("fixture job must use the exact bounded public synthetic contract")]
    InvalidFixtureJob,
    #[error("MLX reasoning capability must use the exact local inference contract")]
    InvalidMlxCapability,
    #[error("MLX job must use the exact bounded public ephemeral contract")]
    InvalidMlxJob,
    #[error("MLX result must use the exact bounded generate-text contract")]
    InvalidMlxResult,
    #[error("local coding capability must use the exact local.coding.v1 contract")]
    InvalidLocalCodingCapability,
    #[error("local coding job must use the exact snapshot-bound metadata-only contract")]
    InvalidLocalCodingJob,
    #[error("local coding result must use the exact bounded metadata-only contract")]
    InvalidLocalCodingResult,
    #[error("local coding snapshot transfer must use the exact leased-attempt contract")]
    InvalidLocalCodingSnapshotTransfer,
    #[error("local coding result artifact must use the exact bounded leased-attempt contract")]
    InvalidLocalCodingResultArtifact,
    #[error("feature conveyor owner-control request is invalid")]
    InvalidFeatureConveyorOwnerControl,
    #[error("canonical GitHub repository URL is invalid")]
    InvalidGitHubRepositoryUrl,
    #[error("full-machine assembly-line contract is invalid")]
    InvalidFullMachineAssemblyLine,
}

macro_rules! uuid_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord,
        )]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new(value: Uuid) -> Self {
                Self(value)
            }
        }
    };
}

uuid_id!(DeviceId);
uuid_id!(TaskId);
uuid_id!(StepId);
uuid_id!(AttemptId);
uuid_id!(LeaseId);
uuid_id!(CancellationId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceRole {
    MacBridge,
    InferenceWorker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKind {
    LocalInference,
    LocalCoding,
    AppleIntegration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    Public,
    Workspace,
    Personal,
    Private,
    CredentialAdjacent,
    Restricted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextHandlingPolicy {
    EphemeralNoRetention,
    SealedUntilResolvedOrExpired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityDescriptor {
    pub id: String,
    pub kind: CapabilityKind,
    pub provider: String,
    pub model: String,
    pub max_context_bytes: u32,
    pub max_result_bytes: u32,
}

/// Secret-free Windows-to-Mac invitation for one interactive enrollment.
///
/// The corresponding grant secret never crosses the pairing boundary. The
/// Windows master retains it only until a matching CSR reply is validated and
/// issued or this process is interrupted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentInvitation {
    pub schema_version: u16,
    pub status: String,
    pub grant_id: Uuid,
    pub device_id: DeviceId,
    pub device_name: String,
    pub role: DeviceRole,
    pub registry_revision: u64,
    pub expires_at_ms: u64,
    pub capabilities: Vec<CapabilityDescriptor>,
    pub master_endpoint: SocketAddr,
    pub ca_fingerprint_sha256: String,
}

impl EnrollmentInvitation {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_and_validate_frame(
            "enrollment_invitation",
            frame,
            MAX_ENROLLMENT_PAIRING_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_fixed_value(
            "schema_version",
            self.schema_version.to_string(),
            ENROLLMENT_PAIRING_SCHEMA_VERSION.to_string(),
        )?;
        validate_fixed_value(
            "status",
            self.status.clone(),
            ENROLLMENT_INVITATION_READY_STATUS.to_string(),
        )?;
        validate_uuid("grant_id", self.grant_id)?;
        validate_uuid("device_id", self.device_id.0)?;
        validate_text("device_name", &self.device_name, MAX_DEVICE_NAME_BYTES)?;
        if self.registry_revision == 0 {
            return Err(ProtocolError::InvalidLimit {
                field: "registry_revision",
                maximum: u64::MAX,
            });
        }
        if self.capabilities.is_empty() {
            return Err(ProtocolError::EmptyField {
                field: "capabilities",
            });
        }
        validate_capabilities(&self.capabilities)?;
        if self.role == DeviceRole::InferenceWorker
            && self.capabilities.as_slice() != [CapabilityDescriptor::local_coding()]
        {
            return Err(ProtocolError::InvalidLocalCodingCapability);
        }
        if self.role == DeviceRole::MacBridge
            && self.capabilities.iter().any(|capability| {
                capability.id == LOCAL_CODING_CAPABILITY_ID
                    || capability.kind == CapabilityKind::LocalCoding
            })
        {
            return Err(ProtocolError::InvalidLocalCodingCapability);
        }
        if self.expires_at_ms == 0 {
            return Err(ProtocolError::InvalidLimit {
                field: "expires_at_ms",
                maximum: u64::MAX,
            });
        }
        validate_socket_endpoint("master_endpoint", self.master_endpoint)?;
        validate_sha256_hex("ca_fingerprint_sha256", &self.ca_fingerprint_sha256)?;
        validate_serialized_limit(
            "enrollment_invitation",
            self,
            MAX_ENROLLMENT_PAIRING_FRAME_BYTES,
        )
    }

    pub fn validate_at(&self, now_ms: u64) -> Result<(), ProtocolError> {
        self.validate()?;
        if now_ms >= self.expires_at_ms {
            return Err(ProtocolError::EnrollmentInvitationExpired);
        }
        Ok(())
    }
}

/// Mac-to-Windows reply containing only the public-key certificate request and
/// exact invitation identifiers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentCsrReply {
    pub schema_version: u16,
    pub status: String,
    pub grant_id: Uuid,
    pub device_id: DeviceId,
    pub csr_pem: String,
}

impl EnrollmentCsrReply {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_and_validate_frame(
            "enrollment_csr_reply",
            frame,
            MAX_ENROLLMENT_PAIRING_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_fixed_value(
            "schema_version",
            self.schema_version.to_string(),
            ENROLLMENT_PAIRING_SCHEMA_VERSION.to_string(),
        )?;
        validate_fixed_value(
            "status",
            self.status.clone(),
            ENROLLMENT_CSR_READY_STATUS.to_string(),
        )?;
        validate_uuid("grant_id", self.grant_id)?;
        validate_uuid("device_id", self.device_id.0)?;
        validate_text("csr_pem", &self.csr_pem, MAX_ENROLLMENT_CSR_PEM_BYTES)?;
        validate_serialized_limit(
            "enrollment_csr_reply",
            self,
            MAX_ENROLLMENT_PAIRING_FRAME_BYTES,
        )
    }
}

impl CapabilityDescriptor {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_identifier("capability.id", &self.id, MAX_CAPABILITY_ID_BYTES)?;
        validate_identifier(
            "capability.provider",
            &self.provider,
            MAX_PROVIDER_NAME_BYTES,
        )?;
        validate_text("capability.model", &self.model, MAX_MODEL_NAME_BYTES)?;
        validate_positive_limit(
            "capability.max_context_bytes",
            u64::from(self.max_context_bytes),
            MAX_JOB_CONTEXT_BYTES as u64,
        )?;
        validate_positive_limit(
            "capability.max_result_bytes",
            u64::from(self.max_result_bytes),
            MAX_JOB_RESULT_BYTES as u64,
        )?;
        if self.id == FIXTURE_REASONING_CAPABILITY_ID
            && (self.kind != CapabilityKind::LocalInference
                || self.provider != FIXTURE_REASONING_PROVIDER
                || self.model != FIXTURE_REASONING_MODEL
                || self.max_context_bytes != MAX_FIXTURE_CONTEXT_BYTES as u32
                || self.max_result_bytes != MAX_FIXTURE_RESULT_BYTES as u32)
        {
            return Err(ProtocolError::InvalidFixtureCapability);
        }
        if self.id == MLX_REASONING_CAPABILITY_ID
            && (self.kind != CapabilityKind::LocalInference
                || self.provider != MLX_REASONING_PROVIDER)
        {
            return Err(ProtocolError::InvalidMlxCapability);
        }
        if self.id == LOCAL_CODING_CAPABILITY_ID && *self != CapabilityDescriptor::local_coding() {
            return Err(ProtocolError::InvalidLocalCodingCapability);
        }
        Ok(())
    }

    pub fn fixture_reasoning() -> Self {
        Self {
            id: FIXTURE_REASONING_CAPABILITY_ID.to_string(),
            kind: CapabilityKind::LocalInference,
            provider: FIXTURE_REASONING_PROVIDER.to_string(),
            model: FIXTURE_REASONING_MODEL.to_string(),
            max_context_bytes: MAX_FIXTURE_CONTEXT_BYTES as u32,
            max_result_bytes: MAX_FIXTURE_RESULT_BYTES as u32,
        }
    }

    pub fn mlx_reasoning(
        model: impl Into<String>,
        max_context_bytes: u32,
        max_result_bytes: u32,
    ) -> Self {
        Self {
            id: MLX_REASONING_CAPABILITY_ID.to_string(),
            kind: CapabilityKind::LocalInference,
            provider: MLX_REASONING_PROVIDER.to_string(),
            model: model.into(),
            max_context_bytes,
            max_result_bytes,
        }
    }

    pub fn local_coding() -> Self {
        Self {
            id: LOCAL_CODING_CAPABILITY_ID.to_string(),
            kind: CapabilityKind::LocalCoding,
            provider: LOCAL_CODING_PROVIDER.to_string(),
            model: LOCAL_CODING_MODEL.to_string(),
            max_context_bytes: MAX_LOCAL_CODING_CONTEXT_BYTES as u32,
            max_result_bytes: MAX_LOCAL_CODING_RESULT_BYTES as u32,
        }
    }
}

/// One explicit owner-confirmed, model-only mutation for the exact designated
/// MLX MacBridge. Local executable and model-directory paths are intentionally
/// absent from this wire contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalModelSelectionRequest {
    pub schema_version: u16,
    pub device_id: DeviceId,
    pub expected_registry_revision: u64,
    pub expected_designation_revision: u64,
    pub expected_emergency_pause_revision: u64,
    pub model_id: String,
}

impl LocalModelSelectionRequest {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "local_model_selection_request",
            frame,
            MAX_LOCAL_MODEL_SELECTION_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_fixed_value(
            "schema_version",
            self.schema_version.to_string(),
            LOCAL_MODEL_SELECTION_SCHEMA_VERSION.to_string(),
        )?;
        validate_uuid("device_id", self.device_id.0)?;
        validate_positive_limit(
            "expected_registry_revision",
            self.expected_registry_revision,
            u64::MAX,
        )?;
        validate_positive_limit(
            "expected_designation_revision",
            self.expected_designation_revision,
            u64::MAX,
        )?;
        validate_local_model_selection_id(&self.model_id)?;
        validate_serialized_limit(
            "local_model_selection_request",
            self,
            MAX_LOCAL_MODEL_SELECTION_FRAME_BYTES,
        )
    }
}

/// Exact Windows-authoritative selection projection used for both ordinary
/// display and ambiguous-result reconciliation. It contains no local paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalModelSelectionProjection {
    pub schema_version: u16,
    pub device_id: DeviceId,
    pub device_name: String,
    pub registry_revision: u64,
    pub designation_revision: u64,
    pub emergency_pause_revision: u64,
    pub emergency_paused: bool,
    pub model_id: String,
}

impl LocalModelSelectionProjection {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_fixed_value(
            "schema_version",
            self.schema_version.to_string(),
            LOCAL_MODEL_SELECTION_SCHEMA_VERSION.to_string(),
        )?;
        validate_uuid("device_id", self.device_id.0)?;
        validate_identifier("device_name", &self.device_name, MAX_DEVICE_NAME_BYTES)?;
        validate_positive_limit("registry_revision", self.registry_revision, u64::MAX)?;
        validate_positive_limit("designation_revision", self.designation_revision, u64::MAX)?;
        validate_local_model_selection_id(&self.model_id)?;
        validate_serialized_limit(
            "local_model_selection_projection",
            self,
            MAX_LOCAL_MODEL_SELECTION_FRAME_BYTES,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalModelSelectionStatus {
    Selected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalModelSelectionReceipt {
    pub schema_version: u16,
    pub device_id: DeviceId,
    pub registry_revision: u64,
    pub designation_revision: u64,
    pub emergency_pause_revision: u64,
    pub model_id: String,
    pub selected_at_ms: u64,
    pub status: LocalModelSelectionStatus,
}

impl LocalModelSelectionReceipt {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        let projection = LocalModelSelectionProjection {
            schema_version: self.schema_version,
            device_id: self.device_id,
            device_name: "receipt-binding".to_string(),
            registry_revision: self.registry_revision,
            designation_revision: self.designation_revision,
            emergency_pause_revision: self.emergency_pause_revision,
            emergency_paused: false,
            model_id: self.model_id.clone(),
        };
        projection.validate()?;
        validate_positive_limit("selected_at_ms", self.selected_at_ms, u64::MAX)?;
        if self.status != LocalModelSelectionStatus::Selected {
            return Err(ProtocolError::InvalidMlxCapability);
        }
        Ok(())
    }
}

/// Canonical owner-facing GitHub identity. The durable authority continues to
/// use the separate UUID in `AssemblyLineRepositoryIdentity`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalGitHubRepositoryUrl {
    pub url: String,
}

impl CanonicalGitHubRepositoryUrl {
    pub fn parse(value: &str) -> Result<Self, ProtocolError> {
        if value.len() > MAX_GITHUB_REPOSITORY_URL_BYTES
            || value.chars().any(char::is_control)
            || value.contains(['?', '#', '%', '\\'])
        {
            return Err(ProtocolError::InvalidGitHubRepositoryUrl);
        }
        let (scheme, remainder) = value
            .split_once("://")
            .ok_or(ProtocolError::InvalidGitHubRepositoryUrl)?;
        if !scheme.eq_ignore_ascii_case("https") {
            return Err(ProtocolError::InvalidGitHubRepositoryUrl);
        }
        let (authority, path) = remainder
            .split_once('/')
            .ok_or(ProtocolError::InvalidGitHubRepositoryUrl)?;
        if !authority.eq_ignore_ascii_case("github.com")
            || authority.contains('@')
            || authority.contains(':')
        {
            return Err(ProtocolError::InvalidGitHubRepositoryUrl);
        }
        let path = path.strip_suffix('/').unwrap_or(path);
        let mut components = path.split('/');
        let owner = components
            .next()
            .ok_or(ProtocolError::InvalidGitHubRepositoryUrl)?;
        let repository = components
            .next()
            .ok_or(ProtocolError::InvalidGitHubRepositoryUrl)?;
        if components.next().is_some() {
            return Err(ProtocolError::InvalidGitHubRepositoryUrl);
        }
        let repository = repository.strip_suffix(".git").unwrap_or(repository);
        validate_github_name(owner, 39, false)?;
        validate_github_name(repository, 100, true)?;
        Ok(Self {
            url: format!(
                "https://github.com/{}/{}",
                owner.to_ascii_lowercase(),
                repository.to_ascii_lowercase()
            ),
        })
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if Self::parse(&self.url)?.url != self.url {
            return Err(ProtocolError::InvalidGitHubRepositoryUrl);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyLineRepositoryIdentity {
    pub repository_id: Uuid,
    pub git_url: CanonicalGitHubRepositoryUrl,
}

impl AssemblyLineRepositoryIdentity {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_uuid("repository_id", self.repository_id)?;
        self.git_url.validate()
    }

    pub fn canonical_sha256(&self) -> Result<[u8; 32], ProtocolError> {
        canonical_sha256("assembly_line_repository_identity", self, Self::validate)
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectVisibility {
    #[default]
    Public,
    Private,
}

/// One explicitly provisioned planning-only provider/model binding. The
/// absence of a fallback field is intentional and enforced by strict decode.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrchestratorProfile {
    pub configuration_revision: u64,
    pub provider_id: String,
    pub model_id: String,
}

impl Default for OrchestratorProfile {
    fn default() -> Self {
        Self {
            configuration_revision: 1,
            provider_id: "openai.codex".to_string(),
            model_id: "gpt-5.6-sol".to_string(),
        }
    }
}

impl OrchestratorProfile {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_positive_limit(
            "orchestrator_configuration_revision",
            self.configuration_revision,
            u64::MAX,
        )?;
        validate_identifier(
            "orchestrator_provider_id",
            &self.provider_id,
            MAX_PROVIDER_NAME_BYTES,
        )?;
        validate_identifier(
            "orchestrator_model_id",
            &self.model_id,
            MAX_MODEL_NAME_BYTES,
        )?;
        if is_path_or_secret_shaped(&self.provider_id) || is_path_or_secret_shaped(&self.model_id) {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        Ok(())
    }

    pub fn canonical_sha256(&self) -> Result<[u8; 32], ProtocolError> {
        canonical_sha256("orchestrator_profile", self, Self::validate)
    }
}

/// Strict configured planning catalog. Membership, its selected default, and
/// the complete catalog revision are digest-bound; no fallback is expressible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrchestratorCatalog {
    pub schema_version: u16,
    pub catalog_revision: u64,
    pub profiles: Vec<OrchestratorProfile>,
    pub default_profile_sha256: [u8; 32],
    pub catalog_sha256: [u8; 32],
}

impl Default for OrchestratorCatalog {
    fn default() -> Self {
        let profile = OrchestratorProfile::default();
        let mut catalog = Self {
            schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
            catalog_revision: profile.configuration_revision,
            default_profile_sha256: profile
                .canonical_sha256()
                .expect("fixed default orchestrator profile must remain valid"),
            profiles: vec![profile],
            catalog_sha256: [0; 32],
        };
        catalog.catalog_sha256 = catalog
            .canonical_catalog_sha256()
            .expect("fixed default orchestrator catalog must remain valid");
        catalog
    }
}

impl OrchestratorCatalog {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "orchestrator_catalog",
            frame,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            Self::validate,
        )
    }

    fn validate_content(&self) -> Result<(), ProtocolError> {
        validate_assembly_line_schema(self.schema_version)?;
        validate_positive_limit(
            "orchestrator_catalog_revision",
            self.catalog_revision,
            u64::MAX,
        )?;
        if self.profiles.is_empty() || self.profiles.len() > MAX_ORCHESTRATOR_PROFILES {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        let mut previous: Option<(&str, &str)> = None;
        let mut default_matches = 0_usize;
        for profile in &self.profiles {
            profile.validate()?;
            if profile.configuration_revision != self.catalog_revision {
                return Err(ProtocolError::InvalidFullMachineAssemblyLine);
            }
            let identity = (profile.provider_id.as_str(), profile.model_id.as_str());
            if previous.is_some_and(|previous| previous >= identity) {
                return Err(ProtocolError::InvalidFullMachineAssemblyLine);
            }
            previous = Some(identity);
            if profile.canonical_sha256()? == self.default_profile_sha256 {
                default_matches += 1;
            }
        }
        if self.default_profile_sha256 == [0; 32] || default_matches != 1 {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        Ok(())
    }

    pub fn canonical_catalog_sha256(&self) -> Result<[u8; 32], ProtocolError> {
        self.validate_content()?;
        let value = serde_json::json!({
            "schema_version": self.schema_version,
            "catalog_revision": self.catalog_revision,
            "profiles": self.profiles,
            "default_profile_sha256": self.default_profile_sha256,
        });
        Ok(Sha256::digest(canonical_json_bytes(&value)?).into())
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.validate_content()?;
        if self.catalog_sha256 == [0; 32] || self.canonical_catalog_sha256()? != self.catalog_sha256
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        validate_serialized_limit(
            "orchestrator_catalog",
            self,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
        )
    }

    pub fn validate_selection(&self, selection: &OrchestratorProfile) -> Result<(), ProtocolError> {
        self.validate()?;
        selection.validate()?;
        if selection.configuration_revision != self.catalog_revision
            || self
                .profiles
                .iter()
                .filter(|profile| *profile == selection)
                .count()
                != 1
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        Ok(())
    }

    /// Bind a caller-carried catalog and selection to the independently loaded
    /// Windows runtime catalog. Structural self-validation alone is not
    /// authority.
    pub fn validate_against_authoritative_catalog(
        &self,
        authoritative_catalog: &Self,
        selection: &OrchestratorProfile,
    ) -> Result<(), ProtocolError> {
        self.validate_selection(selection)?;
        authoritative_catalog.validate_selection(selection)?;
        if self.catalog_revision != authoritative_catalog.catalog_revision
            || self.catalog_sha256 != authoritative_catalog.catalog_sha256
            || self != authoritative_catalog
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectBrainstormingDraft {
    pub schema_version: u16,
    pub draft_id: Uuid,
    pub draft_revision: u64,
    pub repository: AssemblyLineRepositoryIdentity,
    #[serde(default)]
    pub visibility: ProjectVisibility,
    pub orchestrator_catalog: OrchestratorCatalog,
    pub orchestrator: OrchestratorProfile,
    pub idea: String,
}

impl ProjectBrainstormingDraft {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "project_brainstorming_draft",
            frame,
            MAX_BRAINSTORMING_INPUT_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_assembly_line_schema(self.schema_version)?;
        validate_uuid("draft_id", self.draft_id)?;
        validate_positive_limit("draft_revision", self.draft_revision, u64::MAX)?;
        self.repository.validate()?;
        self.orchestrator_catalog
            .validate_selection(&self.orchestrator)?;
        validate_planning_text("project_idea", &self.idea, MAX_BRAINSTORMING_INPUT_BYTES)?;
        validate_serialized_limit(
            "project_brainstorming_draft",
            self,
            MAX_BRAINSTORMING_INPUT_BYTES,
        )
    }

    pub fn canonical_sha256(&self) -> Result<[u8; 32], ProtocolError> {
        canonical_sha256("project_brainstorming_draft", self, Self::validate)
    }

    pub fn validate_against_authoritative_catalog(
        &self,
        authoritative_catalog: &OrchestratorCatalog,
    ) -> Result<(), ProtocolError> {
        self.validate()?;
        self.orchestrator_catalog
            .validate_against_authoritative_catalog(authoritative_catalog, &self.orchestrator)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureBrainstormingDraft {
    pub schema_version: u16,
    pub draft_id: Uuid,
    pub draft_revision: u64,
    pub repository: AssemblyLineRepositoryIdentity,
    pub expected_repository_revision: u64,
    pub orchestrator_catalog: OrchestratorCatalog,
    pub orchestrator: OrchestratorProfile,
    pub idea: String,
}

impl FeatureBrainstormingDraft {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_brainstorming_draft",
            frame,
            MAX_BRAINSTORMING_INPUT_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_assembly_line_schema(self.schema_version)?;
        validate_uuid("draft_id", self.draft_id)?;
        validate_positive_limit("draft_revision", self.draft_revision, u64::MAX)?;
        validate_positive_limit(
            "expected_repository_revision",
            self.expected_repository_revision,
            u64::MAX,
        )?;
        self.repository.validate()?;
        self.orchestrator_catalog
            .validate_selection(&self.orchestrator)?;
        validate_planning_text("feature_idea", &self.idea, MAX_BRAINSTORMING_INPUT_BYTES)?;
        validate_serialized_limit(
            "feature_brainstorming_draft",
            self,
            MAX_BRAINSTORMING_INPUT_BYTES,
        )
    }

    pub fn canonical_sha256(&self) -> Result<[u8; 32], ProtocolError> {
        canonical_sha256("feature_brainstorming_draft", self, Self::validate)
    }

    pub fn validate_against_authoritative_catalog(
        &self,
        authoritative_catalog: &OrchestratorCatalog,
    ) -> Result<(), ProtocolError> {
        self.validate()?;
        self.orchestrator_catalog
            .validate_against_authoritative_catalog(authoritative_catalog, &self.orchestrator)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrainstormingTargetKind {
    Project,
    Feature,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrainstormingAcceptanceCriterion {
    pub id: String,
    pub requirement: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrainstormingSpecificationDocument {
    pub title: String,
    pub outcome: String,
    pub acceptance_criteria: Vec<BrainstormingAcceptanceCriterion>,
    pub obligations: Vec<String>,
}

impl BrainstormingSpecificationDocument {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_planning_text("specification_title", &self.title, 256)?;
        validate_planning_text("specification_outcome", &self.outcome, 8 * 1024)?;
        if self.acceptance_criteria.is_empty()
            || self.acceptance_criteria.len() > MAX_BRAINSTORMING_ITEMS
            || self.obligations.is_empty()
            || self.obligations.len() > MAX_BRAINSTORMING_ITEMS
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        let mut acceptance_ids = BTreeSet::new();
        for criterion in &self.acceptance_criteria {
            validate_identifier("acceptance_criterion_id", &criterion.id, 128)?;
            validate_planning_text(
                "acceptance_criterion_requirement",
                &criterion.requirement,
                4 * 1024,
            )?;
            if !acceptance_ids.insert(criterion.id.as_str()) {
                return Err(ProtocolError::InvalidFullMachineAssemblyLine);
            }
        }
        let mut obligations = BTreeSet::new();
        for obligation in &self.obligations {
            validate_planning_text("specification_obligation", obligation, 2 * 1024)?;
            if !obligations.insert(obligation.as_str()) {
                return Err(ProtocolError::InvalidFullMachineAssemblyLine);
            }
        }
        validate_serialized_limit(
            "brainstorming_specification_document",
            self,
            MAX_BRAINSTORMING_SPECIFICATION_BYTES,
        )
    }

    pub fn canonical_sha256(&self) -> Result<[u8; 32], ProtocolError> {
        canonical_sha256("brainstorming_specification_document", self, Self::validate)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrozenBrainstormingSpecification {
    pub schema_version: u16,
    pub specification_id: Uuid,
    pub specification_revision: u64,
    pub target_kind: BrainstormingTargetKind,
    pub draft_id: Uuid,
    pub draft_revision: u64,
    pub draft_sha256: [u8; 32],
    pub repository: AssemblyLineRepositoryIdentity,
    pub visibility: Option<ProjectVisibility>,
    pub orchestrator_catalog_revision: u64,
    pub orchestrator_catalog_sha256: [u8; 32],
    pub orchestrator_profile_sha256: [u8; 32],
    pub specification: BrainstormingSpecificationDocument,
    pub specification_sha256: [u8; 32],
}

impl FrozenBrainstormingSpecification {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "frozen_brainstorming_specification",
            frame,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_assembly_line_schema(self.schema_version)?;
        validate_uuid("specification_id", self.specification_id)?;
        validate_uuid("draft_id", self.draft_id)?;
        self.repository.validate()?;
        validate_positive_limit(
            "specification_revision",
            self.specification_revision,
            u64::MAX,
        )?;
        validate_positive_limit("draft_revision", self.draft_revision, u64::MAX)?;
        validate_positive_limit(
            "orchestrator_catalog_revision",
            self.orchestrator_catalog_revision,
            u64::MAX,
        )?;
        self.specification.validate()?;
        if (self.target_kind == BrainstormingTargetKind::Project) != self.visibility.is_some()
            || self.draft_sha256 == [0; 32]
            || self.orchestrator_catalog_sha256 == [0; 32]
            || self.orchestrator_profile_sha256 == [0; 32]
            || self.specification_sha256 == [0; 32]
            || self.specification.canonical_sha256()? != self.specification_sha256
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        validate_serialized_limit(
            "frozen_brainstorming_specification",
            self,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
        )
    }

    pub fn validate_for_project_draft(
        &self,
        draft: &ProjectBrainstormingDraft,
        authoritative_catalog: &OrchestratorCatalog,
    ) -> Result<(), ProtocolError> {
        draft.validate_against_authoritative_catalog(authoritative_catalog)?;
        self.validate()?;
        if self.target_kind != BrainstormingTargetKind::Project
            || self.draft_id != draft.draft_id
            || self.draft_revision != draft.draft_revision
            || self.draft_sha256 != draft.canonical_sha256()?
            || self.repository != draft.repository
            || self.visibility != Some(draft.visibility)
            || self.orchestrator_catalog_revision != draft.orchestrator_catalog.catalog_revision
            || self.orchestrator_catalog_sha256 != draft.orchestrator_catalog.catalog_sha256
            || self.orchestrator_profile_sha256 != draft.orchestrator.canonical_sha256()?
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        Ok(())
    }

    pub fn validate_for_feature_draft(
        &self,
        draft: &FeatureBrainstormingDraft,
        authoritative_catalog: &OrchestratorCatalog,
    ) -> Result<(), ProtocolError> {
        draft.validate_against_authoritative_catalog(authoritative_catalog)?;
        self.validate()?;
        if self.target_kind != BrainstormingTargetKind::Feature
            || self.draft_id != draft.draft_id
            || self.draft_revision != draft.draft_revision
            || self.draft_sha256 != draft.canonical_sha256()?
            || self.repository != draft.repository
            || self.visibility.is_some()
            || self.orchestrator_catalog_revision != draft.orchestrator_catalog.catalog_revision
            || self.orchestrator_catalog_sha256 != draft.orchestrator_catalog.catalog_sha256
            || self.orchestrator_profile_sha256 != draft.orchestrator.canonical_sha256()?
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BrainstormingOwnerApprovalBinding {
    pub schema_version: u16,
    pub approval_id: Uuid,
    pub approved_at_ms: u64,
    pub owner_control_revision: u64,
    pub target_kind: BrainstormingTargetKind,
    pub repository: AssemblyLineRepositoryIdentity,
    pub visibility: Option<ProjectVisibility>,
    pub expected_repository_revision: Option<u64>,
    pub expected_queue_revision: Option<u64>,
    pub draft_id: Uuid,
    pub draft_revision: u64,
    pub draft_sha256: [u8; 32],
    pub orchestrator_catalog_revision: u64,
    pub orchestrator_catalog_sha256: [u8; 32],
    pub specification_id: Uuid,
    pub specification_revision: u64,
    pub specification_sha256: [u8; 32],
    pub orchestrator_profile_sha256: [u8; 32],
    pub owner_approval_sha256: [u8; 32],
}

impl BrainstormingOwnerApprovalBinding {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "brainstorming_owner_approval_binding",
            frame,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_assembly_line_schema(self.schema_version)?;
        validate_uuid("approval_id", self.approval_id)?;
        validate_uuid("draft_id", self.draft_id)?;
        validate_uuid("specification_id", self.specification_id)?;
        validate_positive_limit("approved_at_ms", self.approved_at_ms, u64::MAX)?;
        validate_positive_limit(
            "owner_control_revision",
            self.owner_control_revision,
            u64::MAX,
        )?;
        validate_positive_limit(
            "specification_revision",
            self.specification_revision,
            u64::MAX,
        )?;
        validate_positive_limit("draft_revision", self.draft_revision, u64::MAX)?;
        validate_positive_limit(
            "orchestrator_catalog_revision",
            self.orchestrator_catalog_revision,
            u64::MAX,
        )?;
        self.repository.validate()?;
        match self.target_kind {
            BrainstormingTargetKind::Project => {
                if self.visibility.is_none()
                    || self.expected_repository_revision != Some(0)
                    || self.expected_queue_revision.is_some()
                {
                    return Err(ProtocolError::InvalidFullMachineAssemblyLine);
                }
            }
            BrainstormingTargetKind::Feature => {
                if self.visibility.is_some()
                    || self
                        .expected_repository_revision
                        .is_none_or(|revision| revision == 0)
                    || self.expected_queue_revision.is_none()
                {
                    return Err(ProtocolError::InvalidFullMachineAssemblyLine);
                }
            }
        }
        if self.draft_sha256 == [0; 32]
            || self.orchestrator_catalog_sha256 == [0; 32]
            || self.specification_sha256 == [0; 32]
            || self.orchestrator_profile_sha256 == [0; 32]
            || self.owner_approval_sha256 == [0; 32]
            || self.canonical_approval_sha256()? != self.owner_approval_sha256
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        validate_serialized_limit(
            "brainstorming_owner_approval_binding",
            self,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
        )
    }

    pub fn canonical_approval_sha256(&self) -> Result<[u8; 32], ProtocolError> {
        let value = serde_json::json!({
            "schema_version": self.schema_version,
            "approval_id": self.approval_id,
            "approved_at_ms": self.approved_at_ms,
            "owner_control_revision": self.owner_control_revision,
            "target_kind": self.target_kind,
            "repository": self.repository,
            "visibility": self.visibility,
            "expected_repository_revision": self.expected_repository_revision,
            "expected_queue_revision": self.expected_queue_revision,
            "draft_id": self.draft_id,
            "draft_revision": self.draft_revision,
            "draft_sha256": self.draft_sha256,
            "orchestrator_catalog_revision": self.orchestrator_catalog_revision,
            "orchestrator_catalog_sha256": self.orchestrator_catalog_sha256,
            "specification_id": self.specification_id,
            "specification_revision": self.specification_revision,
            "specification_sha256": self.specification_sha256,
            "orchestrator_profile_sha256": self.orchestrator_profile_sha256,
        });
        Ok(Sha256::digest(canonical_json_bytes(&value)?).into())
    }

    fn validate_for_frozen(
        &self,
        frozen: &FrozenBrainstormingSpecification,
    ) -> Result<(), ProtocolError> {
        self.validate()?;
        frozen.validate()?;
        if self.target_kind != frozen.target_kind
            || self.repository != frozen.repository
            || self.visibility != frozen.visibility
            || self.draft_id != frozen.draft_id
            || self.draft_revision != frozen.draft_revision
            || self.draft_sha256 != frozen.draft_sha256
            || self.orchestrator_catalog_revision != frozen.orchestrator_catalog_revision
            || self.orchestrator_catalog_sha256 != frozen.orchestrator_catalog_sha256
            || self.specification_id != frozen.specification_id
            || self.specification_revision != frozen.specification_revision
            || self.specification_sha256 != frozen.specification_sha256
            || self.orchestrator_profile_sha256 != frozen.orchestrator_profile_sha256
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        Ok(())
    }

    pub fn validate_for_project(
        &self,
        draft: &ProjectBrainstormingDraft,
        frozen: &FrozenBrainstormingSpecification,
        authoritative_catalog: &OrchestratorCatalog,
    ) -> Result<(), ProtocolError> {
        frozen.validate_for_project_draft(draft, authoritative_catalog)?;
        self.validate_for_frozen(frozen)
    }

    pub fn validate_for_feature(
        &self,
        draft: &FeatureBrainstormingDraft,
        frozen: &FrozenBrainstormingSpecification,
        authoritative_catalog: &OrchestratorCatalog,
    ) -> Result<(), ProtocolError> {
        frozen.validate_for_feature_draft(draft, authoritative_catalog)?;
        self.validate_for_frozen(frozen)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssemblyLineLifecycleState {
    Stopped,
    Running,
    Stopping,
    PausedAtCheckpoint,
    EmergencyPaused,
    WaitingForHostReconnect,
    ReconciliationRequired,
    IncompleteTermination,
    WaitingForOwnerStart,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyLineState {
    pub schema_version: u16,
    pub state_revision: u64,
    pub queue_revision: u64,
    pub queue_count: u16,
    #[serde(default = "default_true")]
    pub auto_run: bool,
    pub lifecycle: AssemblyLineLifecycleState,
    pub session_id: Option<Uuid>,
    pub active_child_epoch_id: Option<Uuid>,
    pub active_feature_id: Option<Uuid>,
}

impl AssemblyLineState {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "assembly_line_state",
            frame,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_assembly_line_schema(self.schema_version)?;
        validate_positive_limit(
            "assembly_line_state_revision",
            self.state_revision,
            u64::MAX,
        )?;
        if self.queue_count > MAX_ASSEMBLY_LINE_QUEUE_COUNT
            || (self.queue_count > 0 && self.queue_revision == 0)
            || self.active_child_epoch_id.is_some() != self.active_feature_id.is_some()
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        for (field, value) in [
            ("session_id", self.session_id),
            ("active_child_epoch_id", self.active_child_epoch_id),
            ("active_feature_id", self.active_feature_id),
        ] {
            if let Some(value) = value {
                validate_uuid(field, value)?;
            }
        }
        match self.lifecycle {
            AssemblyLineLifecycleState::Stopped
            | AssemblyLineLifecycleState::WaitingForOwnerStart => {
                if self.session_id.is_some() || self.active_child_epoch_id.is_some() {
                    return Err(ProtocolError::InvalidFullMachineAssemblyLine);
                }
            }
            AssemblyLineLifecycleState::Running => {
                if self.queue_count == 0
                    || self.session_id.is_none()
                    || self.active_child_epoch_id.is_none()
                {
                    return Err(ProtocolError::InvalidFullMachineAssemblyLine);
                }
            }
            _ => {
                if self.session_id.is_none() || self.active_child_epoch_id.is_none() {
                    return Err(ProtocolError::InvalidFullMachineAssemblyLine);
                }
            }
        }
        validate_serialized_limit(
            "assembly_line_state",
            self,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyLineStartRequest {
    pub schema_version: u16,
    pub request_id: Uuid,
    pub expected_state_revision: u64,
    pub expected_queue_revision: u64,
    pub expected_emergency_pause_revision: u64,
    pub queue_count: u16,
    pub windows_executor_id: Uuid,
    pub windows_executor_revision: u64,
    pub mac_executor_id: Uuid,
    pub mac_executor_revision: u64,
    #[serde(default = "default_true")]
    pub auto_run: bool,
    pub owner_start_approval_sha256: [u8; 32],
}

impl AssemblyLineStartRequest {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "assembly_line_start_request",
            frame,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_assembly_line_schema(self.schema_version)?;
        validate_uuid("request_id", self.request_id)?;
        validate_uuid("windows_executor_id", self.windows_executor_id)?;
        validate_uuid("mac_executor_id", self.mac_executor_id)?;
        validate_positive_limit(
            "expected_state_revision",
            self.expected_state_revision,
            u64::MAX,
        )?;
        validate_positive_limit(
            "expected_queue_revision",
            self.expected_queue_revision,
            u64::MAX,
        )?;
        validate_positive_limit(
            "windows_executor_revision",
            self.windows_executor_revision,
            u64::MAX,
        )?;
        validate_positive_limit(
            "mac_executor_revision",
            self.mac_executor_revision,
            u64::MAX,
        )?;
        if self.queue_count == 0
            || self.queue_count > MAX_ASSEMBLY_LINE_QUEUE_COUNT
            || self.windows_executor_id == self.mac_executor_id
            || self.owner_start_approval_sha256 == [0; 32]
            || self.canonical_owner_start_approval_sha256()? != self.owner_start_approval_sha256
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        validate_serialized_limit(
            "assembly_line_start_request",
            self,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
        )
    }

    pub fn canonical_owner_start_approval_sha256(&self) -> Result<[u8; 32], ProtocolError> {
        let value = serde_json::json!({
            "schema_version": self.schema_version,
            "request_id": self.request_id,
            "expected_state_revision": self.expected_state_revision,
            "expected_queue_revision": self.expected_queue_revision,
            "expected_emergency_pause_revision": self.expected_emergency_pause_revision,
            "queue_count": self.queue_count,
            "windows_executor_id": self.windows_executor_id,
            "windows_executor_revision": self.windows_executor_revision,
            "mac_executor_id": self.mac_executor_id,
            "mac_executor_revision": self.mac_executor_revision,
            "auto_run": self.auto_run,
        });
        Ok(Sha256::digest(canonical_json_bytes(&value)?).into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyLineStopRequest {
    pub schema_version: u16,
    pub request_id: Uuid,
    pub session_id: Uuid,
    pub expected_state_revision: u64,
    pub expected_child_epoch_id: Uuid,
}

impl AssemblyLineStopRequest {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "assembly_line_stop_request",
            frame,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_assembly_line_schema(self.schema_version)?;
        validate_uuid("request_id", self.request_id)?;
        validate_uuid("session_id", self.session_id)?;
        validate_uuid("expected_child_epoch_id", self.expected_child_epoch_id)?;
        validate_positive_limit(
            "expected_state_revision",
            self.expected_state_revision,
            u64::MAX,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyLineEmergencyPauseRequest {
    pub schema_version: u16,
    pub request_id: Uuid,
    pub session_id: Uuid,
    pub expected_child_epoch_id: Uuid,
    pub expected_state_revision: u64,
    pub expected_emergency_pause_revision: u64,
}

impl AssemblyLineEmergencyPauseRequest {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "assembly_line_emergency_pause_request",
            frame,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_assembly_line_schema(self.schema_version)?;
        validate_uuid("request_id", self.request_id)?;
        validate_uuid("session_id", self.session_id)?;
        validate_uuid("expected_child_epoch_id", self.expected_child_epoch_id)?;
        validate_positive_limit(
            "expected_state_revision",
            self.expected_state_revision,
            u64::MAX,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyLineAutoRunRequest {
    pub schema_version: u16,
    pub request_id: Uuid,
    pub expected_state_revision: u64,
    pub auto_run: bool,
}

impl AssemblyLineAutoRunRequest {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "assembly_line_auto_run_request",
            frame,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_assembly_line_schema(self.schema_version)?;
        validate_uuid("request_id", self.request_id)?;
        validate_positive_limit(
            "expected_state_revision",
            self.expected_state_revision,
            u64::MAX,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyLineSessionEpoch {
    pub schema_version: u16,
    pub session_id: Uuid,
    pub session_revision: u64,
    pub start_request_id: Uuid,
    pub started_queue_count: u16,
    pub state_revision: u64,
    pub queue_revision: u64,
    pub emergency_pause_revision: u64,
    pub owner_start_approval_sha256: [u8; 32],
    pub windows_executor_id: Uuid,
    pub windows_executor_revision: u64,
    pub mac_executor_id: Uuid,
    pub mac_executor_revision: u64,
    pub auto_run: bool,
}

impl AssemblyLineSessionEpoch {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "assembly_line_session_epoch",
            frame,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_assembly_line_schema(self.schema_version)?;
        for (field, value) in [
            ("session_id", self.session_id),
            ("start_request_id", self.start_request_id),
            ("windows_executor_id", self.windows_executor_id),
            ("mac_executor_id", self.mac_executor_id),
        ] {
            validate_uuid(field, value)?;
        }
        for (field, value) in [
            ("session_revision", self.session_revision),
            ("state_revision", self.state_revision),
            ("queue_revision", self.queue_revision),
            ("windows_executor_revision", self.windows_executor_revision),
            ("mac_executor_revision", self.mac_executor_revision),
        ] {
            validate_positive_limit(field, value, u64::MAX)?;
        }
        if self.started_queue_count == 0
            || self.started_queue_count > MAX_ASSEMBLY_LINE_QUEUE_COUNT
            || self.windows_executor_id == self.mac_executor_id
            || self.owner_start_approval_sha256 == [0; 32]
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        Ok(())
    }

    pub fn validate_for_start(
        &self,
        start: &AssemblyLineStartRequest,
    ) -> Result<(), ProtocolError> {
        self.validate()?;
        start.validate()?;
        if self.start_request_id != start.request_id
            || self.started_queue_count != start.queue_count
            || start.expected_state_revision == u64::MAX
            || self.state_revision != start.expected_state_revision + 1
            || self.queue_revision != start.expected_queue_revision
            || self.emergency_pause_revision != start.expected_emergency_pause_revision
            || self.owner_start_approval_sha256 != start.owner_start_approval_sha256
            || self.windows_executor_id != start.windows_executor_id
            || self.windows_executor_revision != start.windows_executor_revision
            || self.mac_executor_id != start.mac_executor_id
            || self.mac_executor_revision != start.mac_executor_revision
            || self.auto_run != start.auto_run
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyLineChildEpoch {
    pub schema_version: u16,
    pub child_epoch_id: Uuid,
    pub child_epoch_revision: u64,
    pub session_id: Uuid,
    pub session_revision: u64,
    pub feature_id: Uuid,
    pub repository_id: Uuid,
    pub feature_lifecycle_revision: u64,
    pub queue_revision: u64,
    pub windows_executor_id: Uuid,
    pub windows_executor_revision: u64,
    pub mac_executor_id: Uuid,
    pub mac_executor_revision: u64,
}

impl AssemblyLineChildEpoch {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "assembly_line_child_epoch",
            frame,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_assembly_line_schema(self.schema_version)?;
        for (field, value) in [
            ("child_epoch_id", self.child_epoch_id),
            ("session_id", self.session_id),
            ("feature_id", self.feature_id),
            ("repository_id", self.repository_id),
            ("windows_executor_id", self.windows_executor_id),
            ("mac_executor_id", self.mac_executor_id),
        ] {
            validate_uuid(field, value)?;
        }
        for (field, value) in [
            ("child_epoch_revision", self.child_epoch_revision),
            ("session_revision", self.session_revision),
            (
                "feature_lifecycle_revision",
                self.feature_lifecycle_revision,
            ),
            ("queue_revision", self.queue_revision),
            ("windows_executor_revision", self.windows_executor_revision),
            ("mac_executor_revision", self.mac_executor_revision),
        ] {
            validate_positive_limit(field, value, u64::MAX)?;
        }
        if self.windows_executor_id == self.mac_executor_id {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        Ok(())
    }

    pub fn validate_for_session(
        &self,
        session: &AssemblyLineSessionEpoch,
    ) -> Result<(), ProtocolError> {
        self.validate()?;
        session.validate()?;
        if self.session_id != session.session_id
            || self.session_revision != session.session_revision
            || self.queue_revision < session.queue_revision
            || self.windows_executor_id != session.windows_executor_id
            || self.windows_executor_revision != session.windows_executor_revision
            || self.mac_executor_id != session.mac_executor_id
            || self.mac_executor_revision != session.mac_executor_revision
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepositoryCreationLifecycle {
    CreationPending,
    Reconciling,
    Created,
    Conflict,
    ReconciliationRequired,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryCreationProjection {
    pub schema_version: u16,
    pub repository: AssemblyLineRepositoryIdentity,
    pub repository_revision: u64,
    pub lifecycle_revision: u64,
    pub visibility: ProjectVisibility,
    pub approved_specification_id: Uuid,
    pub approved_specification_revision: u64,
    pub approved_specification_sha256: [u8; 32],
    pub owner_approval_sha256: [u8; 32],
    pub lifecycle: RepositoryCreationLifecycle,
    pub effect_possible: bool,
    pub creation_evidence_sha256: Option<[u8; 32]>,
}

impl RepositoryCreationProjection {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "repository_creation_projection",
            frame,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_assembly_line_schema(self.schema_version)?;
        self.repository.validate()?;
        validate_uuid("approved_specification_id", self.approved_specification_id)?;
        for (field, value) in [
            ("repository_revision", self.repository_revision),
            ("repository_lifecycle_revision", self.lifecycle_revision),
            (
                "approved_specification_revision",
                self.approved_specification_revision,
            ),
        ] {
            validate_positive_limit(field, value, u64::MAX)?;
        }
        if self.approved_specification_sha256 == [0; 32]
            || self.owner_approval_sha256 == [0; 32]
            || self.creation_evidence_sha256 == Some([0; 32])
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        let valid_lifecycle = match self.lifecycle {
            RepositoryCreationLifecycle::CreationPending
            | RepositoryCreationLifecycle::Conflict
            | RepositoryCreationLifecycle::Failed => {
                !self.effect_possible && self.creation_evidence_sha256.is_none()
            }
            RepositoryCreationLifecycle::Reconciling
            | RepositoryCreationLifecycle::ReconciliationRequired => {
                self.effect_possible && self.creation_evidence_sha256.is_none()
            }
            RepositoryCreationLifecycle::Created => {
                self.effect_possible && self.creation_evidence_sha256.is_some()
            }
        };
        if !valid_lifecycle {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        validate_serialized_limit(
            "repository_creation_projection",
            self,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureQueueLifecycle {
    Queued,
    Active,
    Stopping,
    PausedAtCheckpoint,
    EmergencyPaused,
    WaitingForHostReconnect,
    ReconciliationRequired,
    IncompleteTermination,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureQueueEntryProjection {
    pub schema_version: u16,
    pub feature_id: Uuid,
    pub repository_id: Uuid,
    pub specification_id: Uuid,
    pub specification_revision: u64,
    pub specification_sha256: [u8; 32],
    pub owner_approval_sha256: [u8; 32],
    pub position: u16,
    pub lifecycle_revision: u64,
    pub lifecycle: FeatureQueueLifecycle,
}

impl FeatureQueueEntryProjection {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_queue_entry_projection",
            frame,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_assembly_line_schema(self.schema_version)?;
        validate_uuid("feature_id", self.feature_id)?;
        validate_uuid("repository_id", self.repository_id)?;
        validate_uuid("specification_id", self.specification_id)?;
        validate_positive_limit(
            "feature_specification_revision",
            self.specification_revision,
            u64::MAX,
        )?;
        validate_positive_limit(
            "feature_lifecycle_revision",
            self.lifecycle_revision,
            u64::MAX,
        )?;
        if self.position == 0
            || self.position > MAX_ASSEMBLY_LINE_QUEUE_COUNT
            || self.specification_sha256 == [0; 32]
            || self.owner_approval_sha256 == [0; 32]
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAvailabilityStatus {
    Available,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeUnavailableReason {
    NotConfigured,
    NotAuthenticated,
    Disconnected,
    Unhealthy,
    EmergencyPaused,
    IdentityDrift,
    EvidenceRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeComponentAvailability {
    pub binding_revision: u64,
    pub binding_sha256: [u8; 32],
    pub status: RuntimeAvailabilityStatus,
    pub unavailable_reason: Option<RuntimeUnavailableReason>,
}

impl RuntimeComponentAvailability {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_positive_limit("runtime_binding_revision", self.binding_revision, u64::MAX)?;
        if self.binding_sha256 == [0; 32]
            || (self.status == RuntimeAvailabilityStatus::Available)
                != self.unavailable_reason.is_none()
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyLineRuntimeAvailabilityProjection {
    pub schema_version: u16,
    pub availability_revision: u64,
    pub observed_at_ms: u64,
    pub brainstorming_provider: RuntimeComponentAvailability,
    pub github_creation: RuntimeComponentAvailability,
    pub windows_executor: RuntimeComponentAvailability,
    pub mac_executor: RuntimeComponentAvailability,
    pub protected_brokers: RuntimeComponentAvailability,
}

impl AssemblyLineRuntimeAvailabilityProjection {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "assembly_line_runtime_availability_projection",
            frame,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_assembly_line_schema(self.schema_version)?;
        validate_positive_limit(
            "runtime_availability_revision",
            self.availability_revision,
            u64::MAX,
        )?;
        validate_positive_limit(
            "runtime_availability_observed_at_ms",
            self.observed_at_ms,
            u64::MAX,
        )?;
        for component in [
            self.brainstorming_provider,
            self.github_creation,
            self.windows_executor,
            self.mac_executor,
            self.protected_brokers,
        ] {
            component.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyLineOwnerProjection {
    pub schema_version: u16,
    pub owner_control_revision: u64,
    pub emergency_pause_revision: u64,
    pub emergency_paused: bool,
    pub orchestrator_catalog: OrchestratorCatalog,
    pub repositories: Vec<RepositoryCreationProjection>,
    pub queue: Vec<FeatureQueueEntryProjection>,
    pub assembly_line: AssemblyLineState,
    pub availability: AssemblyLineRuntimeAvailabilityProjection,
}

impl AssemblyLineOwnerProjection {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "assembly_line_owner_projection",
            frame,
            MAX_ASSEMBLY_LINE_OWNER_PROJECTION_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_assembly_line_schema(self.schema_version)?;
        validate_positive_limit(
            "owner_control_revision",
            self.owner_control_revision,
            u64::MAX,
        )?;
        self.orchestrator_catalog.validate()?;
        self.assembly_line.validate()?;
        self.availability.validate()?;
        if self.repositories.len() > MAX_ASSEMBLY_LINE_REPOSITORIES
            || self.queue.len() > MAX_ASSEMBLY_LINE_QUEUE_COUNT as usize
            || self.queue.len() != self.assembly_line.queue_count as usize
            || self.assembly_line.lifecycle == AssemblyLineLifecycleState::EmergencyPaused
                && !self.emergency_paused
            || self.emergency_paused
                && !matches!(
                    self.assembly_line.lifecycle,
                    AssemblyLineLifecycleState::Stopped
                        | AssemblyLineLifecycleState::EmergencyPaused
                        | AssemblyLineLifecycleState::IncompleteTermination
                )
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        let mut repository_ids = BTreeSet::new();
        let mut created_repository_ids = BTreeSet::new();
        let mut repository_urls = BTreeSet::new();
        let mut previous_url: Option<&str> = None;
        for repository in &self.repositories {
            repository.validate()?;
            let url = repository.repository.git_url.url.as_str();
            if previous_url.is_some_and(|previous| previous >= url)
                || !repository_ids.insert(repository.repository.repository_id)
                || !repository_urls.insert(url)
            {
                return Err(ProtocolError::InvalidFullMachineAssemblyLine);
            }
            if repository.lifecycle == RepositoryCreationLifecycle::Created
                && repository.creation_evidence_sha256.is_some()
            {
                created_repository_ids.insert(repository.repository.repository_id);
            }
            previous_url = Some(url);
        }
        if !execution_availability_matches_lifecycle(&self.availability, &self.assembly_line) {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        let expected_active_lifecycle = owner_queue_active_lifecycle(self.assembly_line.lifecycle);
        let mut feature_ids = BTreeSet::new();
        let mut active_entries = 0_usize;
        for (index, entry) in self.queue.iter().enumerate() {
            entry.validate()?;
            if entry.position as usize != index + 1
                || !feature_ids.insert(entry.feature_id)
                || !created_repository_ids.contains(&entry.repository_id)
            {
                return Err(ProtocolError::InvalidFullMachineAssemblyLine);
            }
            if entry.lifecycle != FeatureQueueLifecycle::Queued {
                active_entries += 1;
                if entry.position != 1
                    || Some(entry.lifecycle) != expected_active_lifecycle
                    || Some(entry.feature_id) != self.assembly_line.active_feature_id
                {
                    return Err(ProtocolError::InvalidFullMachineAssemblyLine);
                }
            }
        }
        if active_entries != usize::from(expected_active_lifecycle.is_some()) {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        validate_serialized_limit(
            "assembly_line_owner_projection",
            self,
            MAX_ASSEMBLY_LINE_OWNER_PROJECTION_BYTES,
        )
    }
}

fn execution_availability_matches_lifecycle(
    availability: &AssemblyLineRuntimeAvailabilityProjection,
    state: &AssemblyLineState,
) -> bool {
    let execution_unavailable = [
        availability.windows_executor,
        availability.mac_executor,
        availability.protected_brokers,
    ]
    .into_iter()
    .any(|component| component.status != RuntimeAvailabilityStatus::Available);
    if !execution_unavailable {
        return true;
    }
    matches!(
        state.lifecycle,
        AssemblyLineLifecycleState::Stopped
            | AssemblyLineLifecycleState::WaitingForOwnerStart
            | AssemblyLineLifecycleState::PausedAtCheckpoint
            | AssemblyLineLifecycleState::EmergencyPaused
            | AssemblyLineLifecycleState::WaitingForHostReconnect
            | AssemblyLineLifecycleState::ReconciliationRequired
            | AssemblyLineLifecycleState::IncompleteTermination
    )
}

fn owner_queue_active_lifecycle(
    lifecycle: AssemblyLineLifecycleState,
) -> Option<FeatureQueueLifecycle> {
    match lifecycle {
        AssemblyLineLifecycleState::Stopped | AssemblyLineLifecycleState::WaitingForOwnerStart => {
            None
        }
        AssemblyLineLifecycleState::Running => Some(FeatureQueueLifecycle::Active),
        AssemblyLineLifecycleState::Stopping => Some(FeatureQueueLifecycle::Stopping),
        AssemblyLineLifecycleState::PausedAtCheckpoint => {
            Some(FeatureQueueLifecycle::PausedAtCheckpoint)
        }
        AssemblyLineLifecycleState::EmergencyPaused => Some(FeatureQueueLifecycle::EmergencyPaused),
        AssemblyLineLifecycleState::WaitingForHostReconnect => {
            Some(FeatureQueueLifecycle::WaitingForHostReconnect)
        }
        AssemblyLineLifecycleState::ReconciliationRequired => {
            Some(FeatureQueueLifecycle::ReconciliationRequired)
        }
        AssemblyLineLifecycleState::IncompleteTermination => {
            Some(FeatureQueueLifecycle::IncompleteTermination)
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyLineStartReceipt {
    pub schema_version: u16,
    pub request_id: Uuid,
    pub owner_start_approval_sha256: [u8; 32],
    pub resulting_state: AssemblyLineState,
    pub session: AssemblyLineSessionEpoch,
    pub child: AssemblyLineChildEpoch,
}

impl AssemblyLineStartReceipt {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "assembly_line_start_receipt",
            frame,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_assembly_line_schema(self.schema_version)?;
        validate_uuid("request_id", self.request_id)?;
        self.resulting_state.validate()?;
        self.session.validate()?;
        self.child.validate_for_session(&self.session)?;
        if self.owner_start_approval_sha256 == [0; 32]
            || self.resulting_state.lifecycle != AssemblyLineLifecycleState::Running
            || self.request_id != self.session.start_request_id
            || self.owner_start_approval_sha256 != self.session.owner_start_approval_sha256
            || self.resulting_state.state_revision != self.session.state_revision
            || self.resulting_state.queue_revision != self.session.queue_revision
            || self.resulting_state.queue_count != self.session.started_queue_count
            || self.resulting_state.auto_run != self.session.auto_run
            || self.resulting_state.session_id != Some(self.session.session_id)
            || self.resulting_state.active_child_epoch_id != Some(self.child.child_epoch_id)
            || self.resulting_state.active_feature_id != Some(self.child.feature_id)
            || self.child.queue_revision != self.session.queue_revision
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        validate_serialized_limit(
            "assembly_line_start_receipt",
            self,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
        )
    }

    pub fn validate_for_request(
        &self,
        request: &AssemblyLineStartRequest,
    ) -> Result<(), ProtocolError> {
        self.validate()?;
        self.session.validate_for_start(request)?;
        if self.request_id != request.request_id
            || self.owner_start_approval_sha256 != request.owner_start_approval_sha256
            || self.resulting_state.auto_run != request.auto_run
            || self.session.auto_run != request.auto_run
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessTerminationOutcome {
    AllTerminated,
    SurvivorsDetected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessTerminationEvidenceReference {
    pub evidence_id: Uuid,
    pub evidence_sha256: [u8; 32],
    pub observed_at_ms: u64,
    pub outcome: ProcessTerminationOutcome,
}

impl ProcessTerminationEvidenceReference {
    pub fn canonical_evidence_sha256(&self) -> Result<[u8; 32], ProtocolError> {
        self.validate_metadata()?;
        let value = serde_json::json!({
            "evidence_type": "assembly_line_process_termination",
            "evidence_id": self.evidence_id,
            "observed_at_ms": self.observed_at_ms,
            "outcome": self.outcome,
        });
        Ok(Sha256::digest(canonical_json_bytes(&value)?).into())
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.validate_metadata()?;
        if self.evidence_sha256 != self.canonical_evidence_sha256()? {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        Ok(())
    }

    fn validate_metadata(&self) -> Result<(), ProtocolError> {
        validate_uuid("termination_evidence_id", self.evidence_id)?;
        validate_positive_limit("termination_observed_at_ms", self.observed_at_ms, u64::MAX)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyLineStopReceipt {
    pub schema_version: u16,
    pub request_id: Uuid,
    pub session_id: Uuid,
    pub child_epoch_id: Uuid,
    pub checkpoint_id: Uuid,
    pub checkpoint_sha256: [u8; 32],
    pub resulting_state: AssemblyLineState,
    pub termination_evidence: ProcessTerminationEvidenceReference,
}

impl AssemblyLineStopReceipt {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "assembly_line_stop_receipt",
            frame,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_assembly_line_schema(self.schema_version)?;
        for (field, value) in [
            ("request_id", self.request_id),
            ("session_id", self.session_id),
            ("child_epoch_id", self.child_epoch_id),
            ("checkpoint_id", self.checkpoint_id),
        ] {
            validate_uuid(field, value)?;
        }
        self.resulting_state.validate()?;
        self.termination_evidence.validate()?;
        let valid_result = match self.resulting_state.lifecycle {
            AssemblyLineLifecycleState::PausedAtCheckpoint => {
                self.termination_evidence.outcome == ProcessTerminationOutcome::AllTerminated
            }
            AssemblyLineLifecycleState::IncompleteTermination => {
                self.termination_evidence.outcome == ProcessTerminationOutcome::SurvivorsDetected
            }
            _ => false,
        };
        if self.checkpoint_sha256 == [0; 32]
            || self.resulting_state.session_id != Some(self.session_id)
            || self.resulting_state.active_child_epoch_id != Some(self.child_epoch_id)
            || !valid_result
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        validate_serialized_limit(
            "assembly_line_stop_receipt",
            self,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
        )
    }

    pub fn validate_for_request_and_prior_state(
        &self,
        request: &AssemblyLineStopRequest,
        prior_state: &AssemblyLineState,
    ) -> Result<(), ProtocolError> {
        self.validate()?;
        request.validate()?;
        prior_state.validate()?;
        if request.expected_state_revision == u64::MAX
            || self.request_id != request.request_id
            || self.session_id != request.session_id
            || self.child_epoch_id != request.expected_child_epoch_id
            || request.expected_state_revision != prior_state.state_revision
            || prior_state.session_id != Some(request.session_id)
            || prior_state.active_child_epoch_id != Some(request.expected_child_epoch_id)
            || self.resulting_state.state_revision != request.expected_state_revision + 1
            || !control_resulting_state_preserves_prior(prior_state, &self.resulting_state)
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyLineEmergencyPauseReceipt {
    pub schema_version: u16,
    pub request_id: Uuid,
    pub session_id: Uuid,
    pub child_epoch_id: Uuid,
    pub emergency_pause_revision: u64,
    pub checkpoint_id: Uuid,
    pub checkpoint_sha256: [u8; 32],
    pub resulting_state: AssemblyLineState,
    pub termination_evidence: ProcessTerminationEvidenceReference,
}

impl AssemblyLineEmergencyPauseReceipt {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "assembly_line_emergency_pause_receipt",
            frame,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_assembly_line_schema(self.schema_version)?;
        for (field, value) in [
            ("request_id", self.request_id),
            ("session_id", self.session_id),
            ("child_epoch_id", self.child_epoch_id),
            ("checkpoint_id", self.checkpoint_id),
        ] {
            validate_uuid(field, value)?;
        }
        validate_positive_limit(
            "emergency_pause_revision",
            self.emergency_pause_revision,
            u64::MAX,
        )?;
        self.resulting_state.validate()?;
        self.termination_evidence.validate()?;
        let valid_result = match self.resulting_state.lifecycle {
            AssemblyLineLifecycleState::EmergencyPaused => {
                self.termination_evidence.outcome == ProcessTerminationOutcome::AllTerminated
            }
            AssemblyLineLifecycleState::IncompleteTermination => {
                self.termination_evidence.outcome == ProcessTerminationOutcome::SurvivorsDetected
            }
            _ => false,
        };
        if self.checkpoint_sha256 == [0; 32]
            || self.resulting_state.session_id != Some(self.session_id)
            || self.resulting_state.active_child_epoch_id != Some(self.child_epoch_id)
            || !valid_result
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        validate_serialized_limit(
            "assembly_line_emergency_pause_receipt",
            self,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
        )
    }

    pub fn validate_for_request_and_prior_state(
        &self,
        request: &AssemblyLineEmergencyPauseRequest,
        prior_state: &AssemblyLineState,
    ) -> Result<(), ProtocolError> {
        self.validate()?;
        request.validate()?;
        prior_state.validate()?;
        if request.expected_state_revision == u64::MAX
            || request.expected_emergency_pause_revision == u64::MAX
            || self.request_id != request.request_id
            || self.session_id != request.session_id
            || self.child_epoch_id != request.expected_child_epoch_id
            || request.expected_state_revision != prior_state.state_revision
            || prior_state.session_id != Some(request.session_id)
            || prior_state.active_child_epoch_id != Some(request.expected_child_epoch_id)
            || self.resulting_state.state_revision != request.expected_state_revision + 1
            || self.emergency_pause_revision != request.expected_emergency_pause_revision + 1
            || !control_resulting_state_preserves_prior(prior_state, &self.resulting_state)
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssemblyLineAutoRunReceipt {
    pub schema_version: u16,
    pub request_id: Uuid,
    pub resulting_state: AssemblyLineState,
}

impl AssemblyLineAutoRunReceipt {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "assembly_line_auto_run_receipt",
            frame,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_assembly_line_schema(self.schema_version)?;
        validate_uuid("request_id", self.request_id)?;
        self.resulting_state.validate()?;
        validate_serialized_limit(
            "assembly_line_auto_run_receipt",
            self,
            MAX_FULL_MACHINE_ASSEMBLY_LINE_FRAME_BYTES,
        )
    }

    pub fn validate_for_request_and_prior_state(
        &self,
        request: &AssemblyLineAutoRunRequest,
        prior_state: &AssemblyLineState,
    ) -> Result<(), ProtocolError> {
        self.validate()?;
        request.validate()?;
        prior_state.validate()?;
        if request.expected_state_revision == u64::MAX
            || self.request_id != request.request_id
            || request.expected_state_revision != prior_state.state_revision
            || self.resulting_state.state_revision != request.expected_state_revision + 1
            || self.resulting_state.auto_run != request.auto_run
            || self.resulting_state.schema_version != prior_state.schema_version
            || self.resulting_state.queue_revision != prior_state.queue_revision
            || self.resulting_state.queue_count != prior_state.queue_count
            || self.resulting_state.lifecycle != prior_state.lifecycle
            || self.resulting_state.session_id != prior_state.session_id
            || self.resulting_state.active_child_epoch_id != prior_state.active_child_epoch_id
            || self.resulting_state.active_feature_id != prior_state.active_feature_id
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        Ok(())
    }
}

fn control_resulting_state_preserves_prior(
    prior_state: &AssemblyLineState,
    resulting_state: &AssemblyLineState,
) -> bool {
    resulting_state.schema_version == prior_state.schema_version
        && resulting_state.queue_revision == prior_state.queue_revision
        && resulting_state.queue_count == prior_state.queue_count
        && resulting_state.auto_run == prior_state.auto_run
        && resulting_state.session_id == prior_state.session_id
        && resulting_state.active_child_epoch_id == prior_state.active_child_epoch_id
        && resulting_state.active_feature_id == prior_state.active_feature_id
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandshakeRequest {
    pub protocol_version: u16,
    pub device_id: DeviceId,
    pub device_name: String,
    pub role: DeviceRole,
    pub registry_revision: u64,
    pub capabilities: Vec<CapabilityDescriptor>,
}

impl HandshakeRequest {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_and_validate_frame(
            "handshake",
            frame,
            MAX_HANDSHAKE_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_version(self.protocol_version)?;
        validate_uuid("device_id", self.device_id.0)?;
        validate_text("device_name", &self.device_name, MAX_DEVICE_NAME_BYTES)?;
        validate_capabilities(&self.capabilities)?;
        validate_serialized_limit("handshake", self, MAX_HANDSHAKE_FRAME_BYTES)
    }
}

/// Cross-device handshake envelope bound to the authenticated TLS 1.3 session.
///
/// The digest is SHA-256 over 32 bytes exported with the fixed Assemblywright exporter
/// label. Keeping this value inside the bounded application handshake prevents
/// a valid device handshake from being replayed on another TLS connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthenticatedHandshakeRequest {
    pub handshake: HandshakeRequest,
    pub tls_exporter_sha256: [u8; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorOwnerBridgeDesignationRequest {
    pub schema_version: u16,
    pub device_id: DeviceId,
    pub expected_designation_revision: u64,
}

impl FeatureConveyorOwnerBridgeDesignationRequest {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_owner_bridge_designation_request",
            frame,
            MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_fixed_value(
            "schema_version",
            self.schema_version.to_string(),
            FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION.to_string(),
        )?;
        validate_uuid("device_id", self.device_id.0)?;
        validate_serialized_limit(
            "feature_conveyor_owner_bridge_designation_request",
            self,
            MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorOwnerBridgeDesignationStatus {
    Designated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorOwnerBridgeDesignationReceipt {
    pub schema_version: u16,
    pub device_id: DeviceId,
    pub registry_revision: u64,
    pub designation_revision: u64,
    pub status: FeatureConveyorOwnerBridgeDesignationStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorRepositoryGrantKind {
    Registration,
    CloudDisclosure,
    AutonomousPublication,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorRepositoryGrantRevision {
    pub repository_id: Uuid,
    pub kind: FeatureConveyorRepositoryGrantKind,
    pub revision: u64,
    pub scope_sha256: [u8; 32],
    pub owner_approval_sha256: [u8; 32],
    pub expires_at_ms: Option<u64>,
    pub revoked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorRepositoryGrantRequest {
    pub schema_version: u16,
    pub expected_current_revision: u64,
    pub expected_emergency_pause_revision: u64,
    pub grant: FeatureConveyorRepositoryGrantRevision,
}

impl FeatureConveyorRepositoryGrantRequest {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_repository_grant_request",
            frame,
            MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_fixed_value(
            "schema_version",
            self.schema_version.to_string(),
            FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION.to_string(),
        )?;
        validate_uuid("repository_id", self.grant.repository_id)?;
        validate_positive_limit("repository_grant_revision", self.grant.revision, u64::MAX)?;
        if self.grant.revision != self.expected_current_revision.saturating_add(1)
            || self.expected_current_revision == u64::MAX
            || self.grant.scope_sha256 == [0; 32]
            || self.grant.owner_approval_sha256 == [0; 32]
            || self.grant.expires_at_ms == Some(0)
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_serialized_limit(
            "feature_conveyor_repository_grant_request",
            self,
            MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorRepositoryGrantStatus {
    Recorded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorRepositoryGrantReceipt {
    pub schema_version: u16,
    pub repository_id: Uuid,
    pub kind: FeatureConveyorRepositoryGrantKind,
    pub revision: u64,
    pub scope_sha256: [u8; 32],
    pub owner_approval_sha256: [u8; 32],
    pub expires_at_ms: Option<u64>,
    pub revoked: bool,
    pub emergency_pause_revision: u64,
    pub status: FeatureConveyorRepositoryGrantStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorRepositoryGrantView {
    pub revision: u64,
    pub scope_sha256: [u8; 32],
    pub owner_approval_sha256: [u8; 32],
    pub expires_at_ms: Option<u64>,
    pub revoked: bool,
    pub active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorRepositoryGrantSet {
    pub schema_version: u16,
    pub repository_id: Uuid,
    pub emergency_paused: bool,
    pub emergency_pause_revision: u64,
    pub registration: Option<FeatureConveyorRepositoryGrantView>,
    pub cloud_disclosure: Option<FeatureConveyorRepositoryGrantView>,
    pub autonomous_publication: Option<FeatureConveyorRepositoryGrantView>,
}

/// Exact owner-approved repository observation scope.
///
/// The path is accepted only by the owner-token loopback preflight route and is
/// never durable or returned in a receipt. Its canonical digest must match the
/// current active registration grant before any local repository observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorRepositoryScopeDocument {
    pub repository_id: Uuid,
    pub repository_path: String,
    pub expected_base_branch: String,
    pub expected_head_commit: String,
}

impl FeatureConveyorRepositoryScopeDocument {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_uuid("repository_id", self.repository_id)?;
        validate_text(
            "repository_path",
            &self.repository_path,
            MAX_FEATURE_CONVEYOR_REPOSITORY_PATH_BYTES,
        )?;
        if self.repository_path.chars().any(char::is_control)
            || self.repository_path.starts_with("//")
            || self.repository_path.starts_with("\\\\")
            || !(self.repository_path.starts_with('/')
                || is_windows_absolute_path(&self.repository_path))
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_git_branch(&self.expected_base_branch)?;
        validate_git_commit(&self.expected_head_commit)?;
        Ok(())
    }

    pub fn canonical_scope_sha256(&self) -> Result<[u8; 32], ProtocolError> {
        self.validate()?;
        let value = serde_json::to_value(self).map_err(|error| ProtocolError::Serialization {
            field: "feature_conveyor_repository_scope",
            message: error.to_string(),
        })?;
        Ok(Sha256::digest(canonical_json_bytes(&value)?).into())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorRepositoryPreflightRequest {
    pub schema_version: u16,
    pub scope: FeatureConveyorRepositoryScopeDocument,
    pub scope_sha256: [u8; 32],
    pub registration_grant_revision: u64,
    pub expected_emergency_pause_revision: u64,
}

impl FeatureConveyorRepositoryPreflightRequest {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_repository_preflight_request",
            frame,
            MAX_FEATURE_CONVEYOR_REPOSITORY_PREFLIGHT_REQUEST_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_fixed_value(
            "schema_version",
            self.schema_version.to_string(),
            FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION.to_string(),
        )?;
        self.scope.validate()?;
        validate_positive_limit(
            "registration_grant_revision",
            self.registration_grant_revision,
            u64::MAX,
        )?;
        if self.scope_sha256 == [0; 32] || self.scope.canonical_scope_sha256()? != self.scope_sha256
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_serialized_limit(
            "feature_conveyor_repository_preflight_request",
            self,
            MAX_FEATURE_CONVEYOR_REPOSITORY_PREFLIGHT_REQUEST_BYTES,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorRepositoryPreflightStatus {
    IdentityEligible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorRepositoryPreflightReceipt {
    pub schema_version: u16,
    pub repository_id: Uuid,
    pub registration_grant_revision: u64,
    pub scope_sha256: [u8; 32],
    pub emergency_pause_revision: u64,
    pub base_branch: String,
    pub head_commit: String,
    pub preflight_fingerprint_sha256: [u8; 32],
    pub observed_at_ms: u64,
    pub status: FeatureConveyorRepositoryPreflightStatus,
}

impl FeatureConveyorRepositoryPreflightReceipt {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_repository_preflight_receipt",
            frame,
            MAX_FEATURE_CONVEYOR_REPOSITORY_PREFLIGHT_REQUEST_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_fixed_value(
            "schema_version",
            self.schema_version.to_string(),
            FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION.to_string(),
        )?;
        validate_uuid("repository_id", self.repository_id)?;
        validate_positive_limit(
            "registration_grant_revision",
            self.registration_grant_revision,
            u64::MAX,
        )?;
        validate_git_branch(&self.base_branch)?;
        validate_git_commit(&self.head_commit)?;
        validate_positive_limit("observed_at_ms", self.observed_at_ms, u64::MAX)?;
        if self.scope_sha256 == [0; 32]
            || self.preflight_fingerprint_sha256 == [0; 32]
            || self.preflight_fingerprint_sha256
                != repository_preflight_fingerprint_sha256(
                    self.repository_id,
                    self.registration_grant_revision,
                    &self.scope_sha256,
                    self.emergency_pause_revision,
                    &self.base_branch,
                    &self.head_commit,
                    self.observed_at_ms,
                )
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_serialized_limit(
            "feature_conveyor_repository_preflight_receipt",
            self,
            MAX_FEATURE_CONVEYOR_REPOSITORY_PREFLIGHT_REQUEST_BYTES,
        )
    }
}

pub fn repository_preflight_fingerprint_sha256(
    repository_id: Uuid,
    registration_grant_revision: u64,
    scope_sha256: &[u8; 32],
    emergency_pause_revision: u64,
    base_branch: &str,
    head_commit: &str,
    observed_at_ms: u64,
) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"assemblywright.repository-preflight.v1\0");
    digest.update(repository_id.as_bytes());
    digest.update(registration_grant_revision.to_be_bytes());
    digest.update(scope_sha256);
    digest.update(emergency_pause_revision.to_be_bytes());
    digest.update((base_branch.len() as u64).to_be_bytes());
    digest.update(base_branch.as_bytes());
    digest.update((head_commit.len() as u64).to_be_bytes());
    digest.update(head_commit.as_bytes());
    digest.update(observed_at_ms.to_be_bytes());
    digest.finalize().into()
}

/// Exact owner-bound request to snapshot and atomically claim the strict queue head.
///
/// The repository path is consumed only by the owner-token loopback route. It is
/// never stored and is absent from the receipt and audit evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorRepositorySnapshotClaimRequest {
    pub schema_version: u16,
    pub scope: FeatureConveyorRepositoryScopeDocument,
    pub scope_sha256: [u8; 32],
    pub expected_feature_id: Uuid,
    pub expected_specification_revision: u64,
    pub expected_queue_revision: u64,
    pub expected_emergency_pause_revision: u64,
    pub grants: FeatureConveyorGrantRevisions,
    pub provider_id: String,
    pub model_id: String,
}

impl FeatureConveyorRepositorySnapshotClaimRequest {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_repository_snapshot_claim_request",
            frame,
            MAX_FEATURE_CONVEYOR_SNAPSHOT_CLAIM_REQUEST_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_fixed_value(
            "schema_version",
            self.schema_version.to_string(),
            FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION.to_string(),
        )?;
        self.scope.validate()?;
        validate_uuid("expected_feature_id", self.expected_feature_id)?;
        for (field, revision) in [
            (
                "expected_specification_revision",
                self.expected_specification_revision,
            ),
            ("registration_grant_revision", self.grants.registration),
            (
                "cloud_disclosure_grant_revision",
                self.grants.cloud_disclosure,
            ),
            (
                "autonomous_publication_grant_revision",
                self.grants.autonomous_publication,
            ),
        ] {
            validate_positive_limit(field, revision, u64::MAX)?;
        }
        validate_identifier(
            "provider_id",
            &self.provider_id,
            MAX_FEATURE_CONVEYOR_IDENTIFIER_BYTES,
        )?;
        validate_identifier(
            "model_id",
            &self.model_id,
            MAX_FEATURE_CONVEYOR_IDENTIFIER_BYTES,
        )?;
        if self.scope_sha256 == [0; 32]
            || self.scope.canonical_scope_sha256()? != self.scope_sha256
            || self.scope.repository_id.is_nil()
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_serialized_limit(
            "feature_conveyor_repository_snapshot_claim_request",
            self,
            MAX_FEATURE_CONVEYOR_SNAPSHOT_CLAIM_REQUEST_BYTES,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorRepositorySnapshotClaimStatus {
    SnapshotBound,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorRepositorySnapshotClaimReceipt {
    pub schema_version: u16,
    pub feature_id: Uuid,
    pub specification_revision: u64,
    pub lifecycle_revision: u64,
    pub queue_revision: u64,
    pub emergency_pause_revision: u64,
    pub lease_id: Uuid,
    pub snapshot_id: Uuid,
    pub snapshot_sha256: [u8; 32],
    pub base_commit: String,
    pub grants: FeatureConveyorGrantRevisions,
    pub provider_binding_sha256: [u8; 32],
    pub status: FeatureConveyorRepositorySnapshotClaimStatus,
}

impl FeatureConveyorRepositorySnapshotClaimReceipt {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_repository_snapshot_claim_receipt",
            frame,
            MAX_FEATURE_CONVEYOR_SNAPSHOT_CLAIM_REQUEST_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_fixed_value(
            "schema_version",
            self.schema_version.to_string(),
            FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION.to_string(),
        )?;
        validate_uuid("feature_id", self.feature_id)?;
        validate_uuid("lease_id", self.lease_id)?;
        validate_uuid("snapshot_id", self.snapshot_id)?;
        validate_positive_limit(
            "specification_revision",
            self.specification_revision,
            u64::MAX,
        )?;
        validate_positive_limit("lifecycle_revision", self.lifecycle_revision, u64::MAX)?;
        validate_git_commit(&self.base_commit)?;
        if self.snapshot_sha256 == [0; 32]
            || self.provider_binding_sha256 == [0; 32]
            || self.grants.registration == 0
            || self.grants.cloud_disclosure == 0
            || self.grants.autonomous_publication == 0
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_serialized_limit(
            "feature_conveyor_repository_snapshot_claim_receipt",
            self,
            MAX_FEATURE_CONVEYOR_SNAPSHOT_CLAIM_REQUEST_BYTES,
        )
    }
}

/// Exact protocol-owned file operation. This is deliberately not a command or
/// general tool invocation: the only accepted argument schemas are encoded by
/// this enum and contain no environment, credential, network, or executable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "tool_id", content = "arguments")]
pub enum LocalCodingEditOperation {
    #[serde(rename = "file.write.v1")]
    Write(LocalCodingWriteFileArguments),
    #[serde(rename = "file.delete.v1")]
    Delete(LocalCodingDeleteFileArguments),
}

impl LocalCodingEditOperation {
    pub fn path(&self) -> &str {
        match self {
            Self::Write(arguments) => &arguments.path,
            Self::Delete(arguments) => &arguments.path,
        }
    }

    fn validate(&self) -> Result<(), ProtocolError> {
        match self {
            Self::Write(arguments) => arguments.validate(),
            Self::Delete(arguments) => arguments.validate(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalCodingWriteFileArguments {
    pub path: String,
    /// `None` authorizes creation only; `Some` authorizes replacement only
    /// after the exact current bytes match.
    pub expected_before_sha256: Option<[u8; 32]>,
    pub replacement_sha256: [u8; 32],
    pub replacement_hex: String,
    pub executable: bool,
}

impl LocalCodingWriteFileArguments {
    fn validate(&self) -> Result<(), ProtocolError> {
        validate_local_coding_relative_path(&self.path)?;
        if self.expected_before_sha256 == Some([0; 32]) {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        let replacement = decode_lower_hex(&self.replacement_hex)
            .ok_or(ProtocolError::InvalidFeatureConveyorOwnerControl)?;
        if replacement.len() > MAX_LOCAL_CODING_EDIT_CONTENT_BYTES
            || self.replacement_sha256 != <[u8; 32]>::from(Sha256::digest(&replacement))
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        Ok(())
    }

    pub fn replacement_bytes(&self) -> Result<Vec<u8>, ProtocolError> {
        self.validate()?;
        decode_lower_hex(&self.replacement_hex)
            .ok_or(ProtocolError::InvalidFeatureConveyorOwnerControl)
    }
}

pub const FEATURE_CONVEYOR_ARTIFACT_INTEGRATION_SCHEMA_VERSION: u16 = 1;
pub const MAX_FEATURE_CONVEYOR_INTEGRATION_ARTIFACTS: usize = 3;

pub const FEATURE_CONVEYOR_VALIDATION_GATE_SCHEMA_VERSION: u16 = 1;
pub const FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION: u16 = 1;
pub const FEATURE_CONVEYOR_PUBLICATION_COORDINATOR_SCHEMA_VERSION: u16 = 1;
pub const FEATURE_CONVEYOR_ORCHESTRATION_SCHEMA_VERSION: u16 = 1;
pub const FEATURE_CONVEYOR_OWNER_ACTIVATION_SCHEMA_VERSION: u16 = 1;
pub const MAX_FEATURE_CONVEYOR_OWNER_ACTIVATION_FRAME_BYTES: usize = 8 * 1024;
pub const MAX_FEATURE_CONVEYOR_REPLACEMENT_CANDIDATES: u8 = 3;
pub const MAX_FEATURE_CONVEYOR_ACTIVE_PROCESSING_MS: u64 = 24 * 60 * 60 * 1_000;
pub const MAX_FEATURE_CONVEYOR_PUBLICATION_CHECKS: usize = 64;
pub const MAX_FEATURE_CONVEYOR_REVIEW_PACKET_BYTES: usize = 256 * 1024;
pub const MAX_FEATURE_CONVEYOR_REVIEW_OUTPUT_BYTES: usize = 64 * 1024;
pub const MAX_FEATURE_CONVEYOR_REVIEW_FINDINGS: usize = 128;
pub const MAX_FEATURE_CONVEYOR_REVIEW_REQUIREMENT_COVERAGE: usize = 256;
pub const MAX_FEATURE_CONVEYOR_REVIEW_EVIDENCE_DIGESTS: usize = 32;
pub const MAX_FEATURE_CONVEYOR_REVIEW_TRANSPORT_ATTEMPTS_PER_CANDIDATE: u8 = 3;
pub const MAX_FEATURE_CONVEYOR_REVIEW_CALLS_PER_FEATURE: u8 = 12;
pub const FEATURE_CONVEYOR_REVIEW_BACKOFF_MS: [u64; 3] = [60_000, 300_000, 900_000];
pub const MAX_FEATURE_CONVEYOR_VALIDATION_COMMANDS: usize = 13;
pub const FEATURE_CONVEYOR_MINIMUM_LINE_COVERAGE_PERCENT: u8 = 70;

/// Durable orchestration checkpoints are path-free master projections. They
/// never carry commands, paths, provider output, adapter evidence,
/// credentials, or caller-selected failure classifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorOrchestrationStage {
    Implementing,
    Validating,
    Reviewing,
    Publishing,
    VerifyingMain,
    Repairing,
    Paused,
    AttentionRequired,
    Failed,
    Succeeded,
    Quarantined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorOrchestrationAction {
    Inactive,
    AwaitImplementationEvidence,
    AwaitValidationEvidence,
    AwaitReviewDecision,
    RetryReviewTransport,
    AwaitPublicationEvidence,
    AwaitMainVerification,
    ReplacementCandidateRequired,
    OwnerAttentionRequired,
    ReconcileQuarantine,
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorOrchestrationPauseKind {
    Provider,
    Worker,
    Maintenance,
    Owner,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorOrchestrationReason {
    CapabilityInactive,
    CheckpointEffectFree,
    ExistingEffectAmbiguous,
    ValidationFailed,
    ReviewRejected,
    ReviewTransportBackoff,
    ReviewBudgetExhausted,
    PublicationFailed,
    ReplacementCandidateContractUnavailable,
    RepairBudgetExhausted,
    ActiveProcessingBudgetExhausted,
    Cancelled,
    Failed,
    Succeeded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorOrchestrationProjection {
    pub schema_version: u16,
    pub feature_id: Uuid,
    pub lifecycle_revision: u64,
    pub orchestration_revision: u64,
    pub stage: FeatureConveyorOrchestrationStage,
    pub action: FeatureConveyorOrchestrationAction,
    pub reason: FeatureConveyorOrchestrationReason,
    pub checkpoint_id: Uuid,
    pub checkpoint_sha256: [u8; 32],
    pub replacement_candidates_used: u8,
    pub active_processing_ms: u64,
    pub active_processing_budget_ms: u64,
    pub pause_kind: Option<FeatureConveyorOrchestrationPauseKind>,
    pub next_retry_at_ms: Option<u64>,
    pub effect_possible: bool,
    pub activated: bool,
}

impl FeatureConveyorOrchestrationProjection {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.schema_version != FEATURE_CONVEYOR_ORCHESTRATION_SCHEMA_VERSION
            || self.feature_id.is_nil()
            || self.lifecycle_revision == 0
            || self.orchestration_revision == 0
            || self.checkpoint_id.is_nil()
            || self.checkpoint_sha256 == [0; 32]
            || self.replacement_candidates_used > MAX_FEATURE_CONVEYOR_REPLACEMENT_CANDIDATES
            || self.active_processing_ms > MAX_FEATURE_CONVEYOR_ACTIVE_PROCESSING_MS
            || self.active_processing_budget_ms != MAX_FEATURE_CONVEYOR_ACTIVE_PROCESSING_MS
            || (self.pause_kind.is_some()
                != matches!(self.stage, FeatureConveyorOrchestrationStage::Paused))
            || (self.next_retry_at_ms.is_some()
                && !matches!(
                    self.reason,
                    FeatureConveyorOrchestrationReason::ReviewTransportBackoff
                ))
            || (!self.activated
                && (!matches!(self.action, FeatureConveyorOrchestrationAction::Inactive)
                    || !matches!(
                        self.reason,
                        FeatureConveyorOrchestrationReason::CapabilityInactive
                    )))
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        Ok(())
    }
}

/// A fixed, path-free summary of the exact active feature that owner control
/// may act on. `orchestration_revision` is zero only before the first durable
/// orchestration checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorOwnerActiveFeature {
    pub feature_id: Uuid,
    pub specification_revision: u64,
    pub lifecycle_revision: u64,
    pub orchestration_revision: u64,
    pub lifecycle_status: FeatureConveyorOwnerLifecycleStatus,
    pub stage: FeatureConveyorOrchestrationStage,
    pub owner_paused: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorOwnerLifecycleStatus {
    Implementing,
    Validating,
    Reviewing,
    Publishing,
    VerifyingMain,
    Repairing,
    Paused,
    AttentionRequired,
    Failed,
    Cancelled,
    Quarantined,
}

impl FeatureConveyorOwnerActiveFeature {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_uuid("feature_id", self.feature_id)?;
        validate_positive_limit(
            "specification_revision",
            self.specification_revision,
            u64::MAX,
        )?;
        validate_positive_limit("lifecycle_revision", self.lifecycle_revision, u64::MAX)?;
        if self.owner_paused
            && (self.stage != FeatureConveyorOrchestrationStage::Paused
                || self.lifecycle_status != FeatureConveyorOwnerLifecycleStatus::Paused
                || self.orchestration_revision == 0)
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorActivationStatus {
    Inactive,
    Active,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorActivationBlocker {
    None,
    EmergencyPaused,
    EvidenceRequired,
    AlreadyActivated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorActivationEvidenceCategory {
    RepositoryGateProof,
    RestrictedWorkerLive,
    ReviewProviderLive,
    GithubPublicationLive,
    RestartRecoveryLive,
    MacWindowsControlEventStreamingLive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorActivationEvidenceOrigin {
    RepositoryGateProofController,
    RestrictedWorkerProofController,
    ReviewProviderProofController,
    GithubPublicationProofController,
    RestartRecoveryProofController,
    MacWindowsControlEventStreamingProofController,
}

impl FeatureConveyorActivationEvidenceCategory {
    pub fn accepts_origin(self, origin: FeatureConveyorActivationEvidenceOrigin) -> bool {
        matches!(
            (self, origin),
            (
                Self::RepositoryGateProof,
                FeatureConveyorActivationEvidenceOrigin::RepositoryGateProofController
            ) | (
                Self::RestrictedWorkerLive,
                FeatureConveyorActivationEvidenceOrigin::RestrictedWorkerProofController
            ) | (
                Self::ReviewProviderLive,
                FeatureConveyorActivationEvidenceOrigin::ReviewProviderProofController
            ) | (
                Self::GithubPublicationLive,
                FeatureConveyorActivationEvidenceOrigin::GithubPublicationProofController
            ) | (
                Self::RestartRecoveryLive,
                FeatureConveyorActivationEvidenceOrigin::RestartRecoveryProofController
            ) | (
                Self::MacWindowsControlEventStreamingLive,
                FeatureConveyorActivationEvidenceOrigin::MacWindowsControlEventStreamingProofController
            )
        )
    }
}

/// A Windows-admitted digest-only proof-controller receipt. Remote activation
/// can reference this record but cannot create it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorActivationEvidenceReference {
    pub evidence_id: Uuid,
    pub revision: u64,
    pub receipt_sha256: [u8; 32],
}

impl FeatureConveyorActivationEvidenceReference {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_uuid("evidence_id", self.evidence_id)?;
        validate_positive_limit("evidence_revision", self.revision, u64::MAX)?;
        if self.receipt_sha256 == [0; 32] {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorActivationEvidenceSet {
    pub repository_gate_proof: FeatureConveyorActivationEvidenceReference,
    pub restricted_worker_live: FeatureConveyorActivationEvidenceReference,
    pub review_provider_live: FeatureConveyorActivationEvidenceReference,
    pub github_publication_live: FeatureConveyorActivationEvidenceReference,
    pub restart_recovery_live: FeatureConveyorActivationEvidenceReference,
    pub mac_windows_control_event_streaming_live: FeatureConveyorActivationEvidenceReference,
}

impl FeatureConveyorActivationEvidenceSet {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.repository_gate_proof.validate()?;
        self.restricted_worker_live.validate()?;
        self.review_provider_live.validate()?;
        self.github_publication_live.validate()?;
        self.restart_recovery_live.validate()?;
        self.mac_windows_control_event_streaming_live.validate()?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorActivationEvidenceProjection {
    pub repository_gate_proof: Option<FeatureConveyorActivationEvidenceReference>,
    pub restricted_worker_live: Option<FeatureConveyorActivationEvidenceReference>,
    pub review_provider_live: Option<FeatureConveyorActivationEvidenceReference>,
    pub github_publication_live: Option<FeatureConveyorActivationEvidenceReference>,
    pub restart_recovery_live: Option<FeatureConveyorActivationEvidenceReference>,
    pub mac_windows_control_event_streaming_live:
        Option<FeatureConveyorActivationEvidenceReference>,
}

impl FeatureConveyorActivationEvidenceProjection {
    pub fn complete(self) -> Option<FeatureConveyorActivationEvidenceSet> {
        Some(FeatureConveyorActivationEvidenceSet {
            repository_gate_proof: self.repository_gate_proof?,
            restricted_worker_live: self.restricted_worker_live?,
            review_provider_live: self.review_provider_live?,
            github_publication_live: self.github_publication_live?,
            restart_recovery_live: self.restart_recovery_live?,
            mac_windows_control_event_streaming_live: self
                .mac_windows_control_event_streaming_live?,
        })
    }

    fn validate(&self) -> Result<(), ProtocolError> {
        for value in [
            self.repository_gate_proof,
            self.restricted_worker_live,
            self.review_provider_live,
            self.github_publication_live,
            self.restart_recovery_live,
            self.mac_windows_control_event_streaming_live,
        ]
        .into_iter()
        .flatten()
        {
            value.validate()?;
        }
        Ok(())
    }
}

/// Owner-token loopback-only admission of one external proof-controller
/// receipt. The master stores no report body, command, path, provider output,
/// credential, or secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorActivationEvidenceAdmissionRequest {
    pub schema_version: u16,
    pub category: FeatureConveyorActivationEvidenceCategory,
    pub origin: FeatureConveyorActivationEvidenceOrigin,
    pub evidence_id: Uuid,
    pub revision: u64,
    pub expected_current_revision: u64,
    pub receipt_sha256: [u8; 32],
    pub observed_at_ms: u64,
    pub expected_emergency_pause_revision: u64,
}

impl FeatureConveyorActivationEvidenceAdmissionRequest {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_activation_evidence_admission_request",
            frame,
            MAX_FEATURE_CONVEYOR_OWNER_ACTIVATION_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_fixed_value(
            "schema_version",
            self.schema_version.to_string(),
            FEATURE_CONVEYOR_OWNER_ACTIVATION_SCHEMA_VERSION.to_string(),
        )?;
        validate_uuid("evidence_id", self.evidence_id)?;
        validate_positive_limit("revision", self.revision, u64::MAX)?;
        validate_positive_limit("observed_at_ms", self.observed_at_ms, u64::MAX)?;
        if self.revision != self.expected_current_revision.saturating_add(1)
            || self.expected_current_revision == u64::MAX
            || self.receipt_sha256 == [0; 32]
            || !self.category.accepts_origin(self.origin)
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_serialized_limit(
            "feature_conveyor_activation_evidence_admission_request",
            self,
            MAX_FEATURE_CONVEYOR_OWNER_ACTIVATION_FRAME_BYTES,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorActivationEvidenceAdmissionReceipt {
    pub schema_version: u16,
    pub category: FeatureConveyorActivationEvidenceCategory,
    pub origin: FeatureConveyorActivationEvidenceOrigin,
    pub evidence: FeatureConveyorActivationEvidenceReference,
    pub observed_at_ms: u64,
    pub emergency_pause_revision: u64,
}

impl FeatureConveyorActivationEvidenceAdmissionReceipt {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.evidence.validate()?;
        validate_positive_limit("observed_at_ms", self.observed_at_ms, u64::MAX)?;
        if self.schema_version != FEATURE_CONVEYOR_OWNER_ACTIVATION_SCHEMA_VERSION
            || !self.category.accepts_origin(self.origin)
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        Ok(())
    }
}

/// Owner-token loopback-only preflight for deliberate evidence admission. It
/// exposes only the pause/activation CAS state and current digest references;
/// it carries no report body, path, command, credential, or provider output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorActivationEvidenceAdmissionProjection {
    pub schema_version: u16,
    pub emergency_paused: bool,
    pub emergency_pause_revision: u64,
    pub activation_status: FeatureConveyorActivationStatus,
    pub activation_id: Option<Uuid>,
    pub evidence: FeatureConveyorActivationEvidenceProjection,
}

impl FeatureConveyorActivationEvidenceAdmissionProjection {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_fixed_value(
            "schema_version",
            self.schema_version.to_string(),
            FEATURE_CONVEYOR_OWNER_ACTIVATION_SCHEMA_VERSION.to_string(),
        )?;
        self.evidence.validate()?;
        let active = self.activation_status == FeatureConveyorActivationStatus::Active;
        if active != self.activation_id.is_some()
            || self.activation_id.is_some_and(|id| id.is_nil())
            || (active && self.evidence.complete().is_none())
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_serialized_limit(
            "feature_conveyor_activation_evidence_admission_projection",
            self,
            MAX_FEATURE_CONVEYOR_OWNER_ACTIVATION_FRAME_BYTES,
        )
    }
}

/// Windows-authoritative owner-control projection. It is deliberately limited
/// to revision bindings, fixed enums, IDs, and digests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorOwnerControlProjection {
    pub schema_version: u16,
    pub queue_revision: u64,
    pub emergency_paused: bool,
    pub emergency_pause_revision: u64,
    pub owner_control_designation_revision: u64,
    pub activation_status: FeatureConveyorActivationStatus,
    pub activation_id: Option<Uuid>,
    pub activation_ready: bool,
    pub activation_blocker: FeatureConveyorActivationBlocker,
    pub active_feature: Option<FeatureConveyorOwnerActiveFeature>,
    pub evidence: FeatureConveyorActivationEvidenceProjection,
}

impl FeatureConveyorOwnerControlProjection {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_fixed_value(
            "schema_version",
            self.schema_version.to_string(),
            FEATURE_CONVEYOR_OWNER_ACTIVATION_SCHEMA_VERSION.to_string(),
        )?;
        validate_positive_limit(
            "owner_control_designation_revision",
            self.owner_control_designation_revision,
            u64::MAX,
        )?;
        if let Some(active) = self.active_feature {
            active.validate()?;
        }
        self.evidence.validate()?;
        let active = self.activation_status == FeatureConveyorActivationStatus::Active;
        if active != self.activation_id.is_some()
            || (active && self.evidence.complete().is_none())
            || self.activation_id.is_some_and(|id| id.is_nil())
            || self.activation_ready
                != (!active && !self.emergency_paused && self.evidence.complete().is_some())
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        let expected_blocker = if active {
            FeatureConveyorActivationBlocker::AlreadyActivated
        } else if self.emergency_paused {
            FeatureConveyorActivationBlocker::EmergencyPaused
        } else if self.evidence.complete().is_none() {
            FeatureConveyorActivationBlocker::EvidenceRequired
        } else {
            FeatureConveyorActivationBlocker::None
        };
        if self.activation_blocker != expected_blocker {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_serialized_limit(
            "feature_conveyor_owner_control_projection",
            self,
            MAX_FEATURE_CONVEYOR_OWNER_ACTIVATION_FRAME_BYTES,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorActivationRequest {
    pub schema_version: u16,
    pub expected_queue_revision: u64,
    pub expected_owner_control_designation_revision: u64,
    pub expected_emergency_pause_revision: u64,
    pub evidence: FeatureConveyorActivationEvidenceSet,
}

impl FeatureConveyorActivationRequest {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_activation_request",
            frame,
            MAX_FEATURE_CONVEYOR_OWNER_ACTIVATION_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_fixed_value(
            "schema_version",
            self.schema_version.to_string(),
            FEATURE_CONVEYOR_OWNER_ACTIVATION_SCHEMA_VERSION.to_string(),
        )?;
        validate_positive_limit(
            "expected_owner_control_designation_revision",
            self.expected_owner_control_designation_revision,
            u64::MAX,
        )?;
        self.evidence.validate()?;
        validate_serialized_limit(
            "feature_conveyor_activation_request",
            self,
            MAX_FEATURE_CONVEYOR_OWNER_ACTIVATION_FRAME_BYTES,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorActivationReceipt {
    pub schema_version: u16,
    pub activation_id: Uuid,
    pub queue_revision: u64,
    pub owner_control_designation_revision: u64,
    pub emergency_pause_revision: u64,
    pub evidence: FeatureConveyorActivationEvidenceSet,
    pub activated_at_ms: u64,
    pub status: FeatureConveyorActivationStatus,
}

impl FeatureConveyorActivationReceipt {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        let request = FeatureConveyorActivationRequest {
            schema_version: self.schema_version,
            expected_queue_revision: self.queue_revision,
            expected_owner_control_designation_revision: self.owner_control_designation_revision,
            expected_emergency_pause_revision: self.emergency_pause_revision,
            evidence: self.evidence,
        };
        request.validate()?;
        validate_uuid("activation_id", self.activation_id)?;
        validate_positive_limit("activated_at_ms", self.activated_at_ms, u64::MAX)?;
        if self.status != FeatureConveyorActivationStatus::Active {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorOwnerOrchestrationControlRequest {
    pub schema_version: u16,
    pub feature_id: Uuid,
    pub expected_lifecycle_revision: u64,
    pub expected_orchestration_revision: u64,
    pub expected_queue_revision: u64,
    pub expected_owner_control_designation_revision: u64,
    pub expected_emergency_pause_revision: u64,
}

impl FeatureConveyorOwnerOrchestrationControlRequest {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_owner_orchestration_control_request",
            frame,
            MAX_FEATURE_CONVEYOR_OWNER_ACTIVATION_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_fixed_value(
            "schema_version",
            self.schema_version.to_string(),
            FEATURE_CONVEYOR_OWNER_ACTIVATION_SCHEMA_VERSION.to_string(),
        )?;
        validate_uuid("feature_id", self.feature_id)?;
        validate_positive_limit(
            "expected_lifecycle_revision",
            self.expected_lifecycle_revision,
            u64::MAX,
        )?;
        validate_positive_limit(
            "expected_orchestration_revision",
            self.expected_orchestration_revision,
            u64::MAX,
        )?;
        validate_positive_limit(
            "expected_owner_control_designation_revision",
            self.expected_owner_control_designation_revision,
            u64::MAX,
        )?;
        validate_serialized_limit(
            "feature_conveyor_owner_orchestration_control_request",
            self,
            MAX_FEATURE_CONVEYOR_OWNER_ACTIVATION_FRAME_BYTES,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorOwnerOrchestrationControlStatus {
    Paused,
    Resumed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorOwnerOrchestrationControlReceipt {
    pub schema_version: u16,
    pub feature_id: Uuid,
    pub lifecycle_revision: u64,
    pub orchestration_revision: u64,
    pub queue_revision: u64,
    pub owner_control_designation_revision: u64,
    pub emergency_pause_revision: u64,
    pub checkpoint_id: Uuid,
    pub checkpoint_sha256: [u8; 32],
    pub status: FeatureConveyorOwnerOrchestrationControlStatus,
}

impl FeatureConveyorOwnerOrchestrationControlReceipt {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_fixed_value(
            "schema_version",
            self.schema_version.to_string(),
            FEATURE_CONVEYOR_OWNER_ACTIVATION_SCHEMA_VERSION.to_string(),
        )?;
        validate_uuid("feature_id", self.feature_id)?;
        validate_uuid("checkpoint_id", self.checkpoint_id)?;
        validate_positive_limit("lifecycle_revision", self.lifecycle_revision, u64::MAX)?;
        validate_positive_limit(
            "orchestration_revision",
            self.orchestration_revision,
            u64::MAX,
        )?;
        validate_positive_limit(
            "owner_control_designation_revision",
            self.owner_control_designation_revision,
            u64::MAX,
        )?;
        if self.checkpoint_sha256 == [0; 32] {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        Ok(())
    }
}

/// Closed, protocol-owned validation commands. These identifiers select
/// master-owned argv; they are never interpreted as executable names or shell
/// input.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorValidationCommandId {
    RequirementsBinding,
    Coverage,
    FocusedUnitTests,
    NativeE2e,
    Documentation,
    KnowledgeBase,
    Formatting,
    Lint,
    Build,
    Safety,
    ChangedPaths,
    SecretScan,
    RepositoryValidation,
}

impl FeatureConveyorValidationCommandId {
    pub const REQUIRED: [Self; MAX_FEATURE_CONVEYOR_VALIDATION_COMMANDS] = [
        Self::RequirementsBinding,
        Self::Coverage,
        Self::FocusedUnitTests,
        Self::NativeE2e,
        Self::Documentation,
        Self::KnowledgeBase,
        Self::Formatting,
        Self::Lint,
        Self::Build,
        Self::Safety,
        Self::ChangedPaths,
        Self::SecretScan,
        Self::RepositoryValidation,
    ];
}

pub fn feature_conveyor_validation_plan_sha256(
    commands: &[FeatureConveyorValidationCommandId],
) -> Result<[u8; 32], ProtocolError> {
    validate_validation_command_ids(commands)?;
    let value = serde_json::to_value(commands).map_err(|error| ProtocolError::Serialization {
        field: "feature_conveyor_validation_plan",
        message: error.to_string(),
    })?;
    let canonical = canonical_json_bytes(&value)?;
    let mut digest = Sha256::new();
    digest.update(b"assemblywright.validation-plan.v1\0");
    digest.update((canonical.len() as u64).to_be_bytes());
    digest.update(canonical);
    Ok(digest.finalize().into())
}

fn validate_validation_command_ids(
    commands: &[FeatureConveyorValidationCommandId],
) -> Result<(), ProtocolError> {
    if commands != FeatureConveyorValidationCommandId::REQUIRED {
        return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorValidationGateRequest {
    pub schema_version: u16,
    pub validation_id: Uuid,
    pub feature_id: Uuid,
    pub specification_revision: u64,
    pub expected_lifecycle_revision: u64,
    pub feature_lease_id: Uuid,
    pub snapshot_id: Uuid,
    pub snapshot_sha256: [u8; 32],
    pub integration_id: Uuid,
    pub artifact_set_sha256: [u8; 32],
    pub candidate_commit: String,
    pub candidate_tree: String,
    pub base_commit: String,
    pub command_ids: Vec<FeatureConveyorValidationCommandId>,
    pub plan_sha256: [u8; 32],
    pub expected_queue_revision: u64,
    pub expected_emergency_pause_revision: u64,
    pub grants: FeatureConveyorGrantRevisions,
}

impl FeatureConveyorValidationGateRequest {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_validation_gate_request",
            frame,
            MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.schema_version != FEATURE_CONVEYOR_VALIDATION_GATE_SCHEMA_VERSION
            || self.validation_id.is_nil()
            || self.feature_id.is_nil()
            || self.feature_lease_id.is_nil()
            || self.snapshot_id.is_nil()
            || self.integration_id.is_nil()
            || self.specification_revision == 0
            || self.expected_lifecycle_revision == 0
            || self.snapshot_sha256 == [0; 32]
            || self.artifact_set_sha256 == [0; 32]
            || self.plan_sha256 == [0; 32]
            || self.grants.registration == 0
            || self.grants.cloud_disclosure == 0
            || self.grants.autonomous_publication == 0
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_validation_command_ids(&self.command_ids)?;
        if feature_conveyor_validation_plan_sha256(&self.command_ids)? != self.plan_sha256 {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_git_commit(&self.base_commit)?;
        validate_git_commit(&self.candidate_commit)?;
        validate_git_commit(&self.candidate_tree)?;
        validate_serialized_limit(
            "feature_conveyor_validation_gate_request",
            self,
            MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES,
        )
    }
}

pub fn feature_conveyor_validation_request_binding_sha256(
    request: &FeatureConveyorValidationGateRequest,
) -> Result<[u8; 32], ProtocolError> {
    request.validate()?;
    let value = serde_json::to_value(request).map_err(|error| ProtocolError::Serialization {
        field: "feature_conveyor_validation_gate_request",
        message: error.to_string(),
    })?;
    let canonical = canonical_json_bytes(&value)?;
    let mut digest = Sha256::new();
    digest.update(b"assemblywright.validation-request-binding.v1\0");
    digest.update((canonical.len() as u64).to_be_bytes());
    digest.update(canonical);
    Ok(digest.finalize().into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorValidationGateStatus {
    EvidenceAccepted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorValidationGateReceipt {
    pub schema_version: u16,
    pub validation_id: Uuid,
    pub feature_id: Uuid,
    pub specification_revision: u64,
    pub lifecycle_revision: u64,
    pub feature_lease_id: Uuid,
    pub integration_id: Uuid,
    pub candidate_commit: String,
    pub candidate_tree: String,
    pub evidence_manifest_sha256: [u8; 32],
    pub plan_sha256: [u8; 32],
    pub queue_revision: u64,
    pub emergency_pause_revision: u64,
    pub grants: FeatureConveyorGrantRevisions,
    pub status: FeatureConveyorValidationGateStatus,
}

impl FeatureConveyorValidationGateReceipt {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.schema_version != FEATURE_CONVEYOR_VALIDATION_GATE_SCHEMA_VERSION
            || self.validation_id.is_nil()
            || self.feature_id.is_nil()
            || self.feature_lease_id.is_nil()
            || self.integration_id.is_nil()
            || self.specification_revision == 0
            || self.lifecycle_revision == 0
            || self.evidence_manifest_sha256 == [0; 32]
            || self.plan_sha256 == [0; 32]
            || self.grants.registration == 0
            || self.grants.cloud_disclosure == 0
            || self.grants.autonomous_publication == 0
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_git_commit(&self.candidate_commit)?;
        validate_git_commit(&self.candidate_tree)?;
        validate_serialized_limit(
            "feature_conveyor_validation_gate_receipt",
            self,
            MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorReviewGatewayRequest {
    pub schema_version: u16,
    pub review_call_id: Uuid,
    pub feature_id: Uuid,
    pub specification_revision: u64,
    pub expected_lifecycle_revision: u64,
    pub feature_lease_id: Uuid,
    pub integration_id: Uuid,
    pub validation_id: Uuid,
    pub candidate_commit: String,
    pub candidate_tree: String,
    pub base_commit: String,
    pub candidate_diff_sha256: [u8; 32],
    pub evidence_manifest_sha256: [u8; 32],
    pub review_packet_sha256: [u8; 32],
    pub provider_id: String,
    pub model_id: String,
    pub expected_queue_revision: u64,
    pub expected_emergency_pause_revision: u64,
    pub grants: FeatureConveyorGrantRevisions,
}

impl FeatureConveyorReviewGatewayRequest {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_review_gateway_request",
            frame,
            MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.schema_version != FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION
            || self.review_call_id.is_nil()
            || self.feature_id.is_nil()
            || self.feature_lease_id.is_nil()
            || self.integration_id.is_nil()
            || self.validation_id.is_nil()
            || self.specification_revision == 0
            || self.expected_lifecycle_revision == 0
            || self.candidate_diff_sha256 == [0; 32]
            || self.evidence_manifest_sha256 == [0; 32]
            || self.review_packet_sha256 == [0; 32]
            || self.grants.registration == 0
            || self.grants.cloud_disclosure == 0
            || self.grants.autonomous_publication == 0
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_git_commit(&self.candidate_commit)?;
        validate_git_commit(&self.candidate_tree)?;
        validate_git_commit(&self.base_commit)?;
        validate_identifier(
            "provider_id",
            &self.provider_id,
            MAX_FEATURE_CONVEYOR_IDENTIFIER_BYTES,
        )?;
        validate_identifier(
            "model_id",
            &self.model_id,
            MAX_FEATURE_CONVEYOR_IDENTIFIER_BYTES,
        )?;
        validate_serialized_limit(
            "feature_conveyor_review_gateway_request",
            self,
            MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES,
        )
    }
}

pub fn feature_conveyor_review_request_binding_sha256(
    request: &FeatureConveyorReviewGatewayRequest,
) -> Result<[u8; 32], ProtocolError> {
    request.validate()?;
    let value = serde_json::to_value(request).map_err(|error| ProtocolError::Serialization {
        field: "feature_conveyor_review_gateway_request",
        message: error.to_string(),
    })?;
    let canonical = canonical_json_bytes(&value)?;
    let mut digest = Sha256::new();
    digest.update(b"assemblywright.review-request-binding.v1\0");
    digest.update((canonical.len() as u64).to_be_bytes());
    digest.update(canonical);
    Ok(digest.finalize().into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorReviewDecision {
    Approved,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorReviewCoverageStatus {
    Covered,
    Uncovered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorKnowledgeBaseDetermination {
    Updated,
    NoNewKnowledge,
    UpdateRequired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorReviewFinding {
    pub finding_id: String,
    pub requirement_id: String,
    pub evidence_sha256: [u8; 32],
}

impl FeatureConveyorReviewFinding {
    fn validate(&self) -> Result<(), ProtocolError> {
        validate_identifier("finding_id", &self.finding_id, 128)?;
        validate_identifier("requirement_id", &self.requirement_id, 128)?;
        if self.evidence_sha256 == [0; 32] {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorReviewRequirementCoverage {
    pub requirement_id: String,
    pub status: FeatureConveyorReviewCoverageStatus,
    pub evidence_sha256: [u8; 32],
}

impl FeatureConveyorReviewRequirementCoverage {
    fn validate(&self) -> Result<(), ProtocolError> {
        validate_identifier("requirement_id", &self.requirement_id, 128)?;
        if self.evidence_sha256 == [0; 32] {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorReviewProviderOutput {
    pub schema_version: u16,
    pub review_packet_sha256: [u8; 32],
    pub provider_id: String,
    pub model_id: String,
    pub decision: FeatureConveyorReviewDecision,
    pub blocking_findings: Vec<FeatureConveyorReviewFinding>,
    pub non_blocking_findings: Vec<FeatureConveyorReviewFinding>,
    pub requirement_coverage: Vec<FeatureConveyorReviewRequirementCoverage>,
    pub evidence_digests: Vec<[u8; 32]>,
    pub knowledge_base_determination: FeatureConveyorKnowledgeBaseDetermination,
    pub knowledge_base_evidence_sha256: [u8; 32],
}

impl FeatureConveyorReviewProviderOutput {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_review_provider_output",
            frame,
            MAX_FEATURE_CONVEYOR_REVIEW_OUTPUT_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.schema_version != FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION
            || self.review_packet_sha256 == [0; 32]
            || self.knowledge_base_evidence_sha256 == [0; 32]
            || self.blocking_findings.len() > MAX_FEATURE_CONVEYOR_REVIEW_FINDINGS
            || self.non_blocking_findings.len() > MAX_FEATURE_CONVEYOR_REVIEW_FINDINGS
            || self.requirement_coverage.is_empty()
            || self.requirement_coverage.len() > MAX_FEATURE_CONVEYOR_REVIEW_REQUIREMENT_COVERAGE
            || self.evidence_digests.is_empty()
            || self.evidence_digests.len() > MAX_FEATURE_CONVEYOR_REVIEW_EVIDENCE_DIGESTS
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_identifier(
            "provider_id",
            &self.provider_id,
            MAX_FEATURE_CONVEYOR_IDENTIFIER_BYTES,
        )?;
        validate_identifier(
            "model_id",
            &self.model_id,
            MAX_FEATURE_CONVEYOR_IDENTIFIER_BYTES,
        )?;
        let mut finding_ids = HashSet::new();
        for finding in self
            .blocking_findings
            .iter()
            .chain(self.non_blocking_findings.iter())
        {
            finding.validate()?;
            if !finding_ids.insert(&finding.finding_id) {
                return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
            }
        }
        let mut requirements = HashSet::new();
        for coverage in &self.requirement_coverage {
            coverage.validate()?;
            if !requirements.insert(&coverage.requirement_id) {
                return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
            }
        }
        let mut evidence = HashSet::new();
        if self
            .evidence_digests
            .iter()
            .any(|digest| *digest == [0; 32] || !evidence.insert(*digest))
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        let uncovered = self
            .requirement_coverage
            .iter()
            .any(|coverage| coverage.status == FeatureConveyorReviewCoverageStatus::Uncovered);
        match self.decision {
            FeatureConveyorReviewDecision::Approved
                if !self.blocking_findings.is_empty()
                    || uncovered
                    || self.knowledge_base_determination
                        == FeatureConveyorKnowledgeBaseDetermination::UpdateRequired =>
            {
                return Err(ProtocolError::InvalidFeatureConveyorOwnerControl)
            }
            FeatureConveyorReviewDecision::Rejected
                if self.blocking_findings.is_empty() && !uncovered =>
            {
                return Err(ProtocolError::InvalidFeatureConveyorOwnerControl)
            }
            _ => {}
        }
        validate_serialized_limit(
            "feature_conveyor_review_provider_output",
            self,
            MAX_FEATURE_CONVEYOR_REVIEW_OUTPUT_BYTES,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorReviewPacket {
    pub schema_version: u16,
    pub feature_id: Uuid,
    pub specification_revision: u64,
    pub approved_specification: Value,
    pub approved_specification_sha256: [u8; 32],
    pub candidate_commit: String,
    pub candidate_tree: String,
    pub base_commit: String,
    pub candidate_diff: String,
    pub candidate_diff_sha256: [u8; 32],
    pub evidence_manifest_sha256: [u8; 32],
    pub evidence_digests: Vec<[u8; 32]>,
    pub requirements_sha256: [u8; 32],
    pub requirement_ids: Vec<String>,
    pub provider_id: String,
    pub model_id: String,
    pub grants: FeatureConveyorGrantRevisions,
}

impl FeatureConveyorReviewPacket {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_review_packet",
            frame,
            MAX_FEATURE_CONVEYOR_REVIEW_PACKET_BYTES,
            Self::validate,
        )
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, ProtocolError> {
        self.validate()?;
        let value = serde_json::to_value(self).map_err(|error| ProtocolError::Serialization {
            field: "feature_conveyor_review_packet",
            message: error.to_string(),
        })?;
        canonical_json_bytes(&value)
    }

    pub fn sha256(&self) -> Result<[u8; 32], ProtocolError> {
        Ok(Sha256::digest(self.canonical_bytes()?).into())
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.schema_version != FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION
            || self.feature_id.is_nil()
            || self.specification_revision == 0
            || self.approved_specification_sha256 == [0; 32]
            || self.candidate_diff_sha256 == [0; 32]
            || self.evidence_manifest_sha256 == [0; 32]
            || self.requirements_sha256 == [0; 32]
            || self.requirement_ids.is_empty()
            || self.requirement_ids.len() > MAX_FEATURE_CONVEYOR_REVIEW_REQUIREMENT_COVERAGE
            || self.evidence_digests.is_empty()
            || self.evidence_digests.len() > MAX_FEATURE_CONVEYOR_REVIEW_EVIDENCE_DIGESTS
            || self.grants.registration == 0
            || self.grants.cloud_disclosure == 0
            || self.grants.autonomous_publication == 0
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_object("approved_specification", &self.approved_specification)?;
        let canonical_specification = canonical_json_bytes(&self.approved_specification)?;
        if <[u8; 32]>::from(Sha256::digest(&canonical_specification))
            != self.approved_specification_sha256
            || <[u8; 32]>::from(Sha256::digest(self.candidate_diff.as_bytes()))
                != self.candidate_diff_sha256
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_git_commit(&self.candidate_commit)?;
        validate_git_commit(&self.candidate_tree)?;
        validate_git_commit(&self.base_commit)?;
        validate_identifier(
            "provider_id",
            &self.provider_id,
            MAX_FEATURE_CONVEYOR_IDENTIFIER_BYTES,
        )?;
        validate_identifier(
            "model_id",
            &self.model_id,
            MAX_FEATURE_CONVEYOR_IDENTIFIER_BYTES,
        )?;
        let mut evidence = HashSet::new();
        if self
            .evidence_digests
            .iter()
            .any(|digest| *digest == [0; 32] || !evidence.insert(*digest))
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        let mut requirements = HashSet::new();
        for requirement_id in &self.requirement_ids {
            validate_identifier("requirement_id", requirement_id, 128)?;
            if !requirements.insert(requirement_id) {
                return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
            }
        }
        validate_serialized_limit(
            "feature_conveyor_review_packet",
            self,
            MAX_FEATURE_CONVEYOR_REVIEW_PACKET_BYTES,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorReviewGatewayStatus {
    Approved,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorReviewGatewayReceipt {
    pub schema_version: u16,
    pub review_call_id: Uuid,
    pub feature_id: Uuid,
    pub specification_revision: u64,
    pub lifecycle_revision: u64,
    pub feature_lease_id: Uuid,
    pub integration_id: Uuid,
    pub validation_id: Uuid,
    pub candidate_commit: String,
    pub candidate_diff_sha256: [u8; 32],
    pub evidence_manifest_sha256: [u8; 32],
    pub review_packet_sha256: [u8; 32],
    pub provider_id: String,
    pub model_id: String,
    pub candidate_attempt: u8,
    pub feature_call: u8,
    pub decision_sha256: [u8; 32],
    pub queue_revision: u64,
    pub emergency_pause_revision: u64,
    pub grants: FeatureConveyorGrantRevisions,
    pub status: FeatureConveyorReviewGatewayStatus,
}

/// Owner-loopback publication admission. It contains only exact, path-free
/// authority bindings. Repository locations, credentials, commands, provider
/// output, and adapter output are deliberately not part of the wire contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorPublicationRequest {
    pub schema_version: u16,
    pub publication_id: Uuid,
    pub feature_id: Uuid,
    pub specification_revision: u64,
    pub expected_lifecycle_revision: u64,
    pub feature_lease_id: Uuid,
    pub integration_id: Uuid,
    pub validation_id: Uuid,
    pub review_call_id: Uuid,
    pub candidate_commit: String,
    pub candidate_tree: String,
    pub candidate_diff_sha256: [u8; 32],
    pub evidence_manifest_sha256: [u8; 32],
    pub review_decision_sha256: [u8; 32],
    pub provider_id: String,
    pub model_id: String,
    pub remote_base_commit: String,
    pub branch_policy_sha256: [u8; 32],
    pub expected_queue_revision: u64,
    pub expected_emergency_pause_revision: u64,
    pub grants: FeatureConveyorGrantRevisions,
}

impl FeatureConveyorPublicationRequest {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_publication_request",
            frame,
            MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.schema_version != FEATURE_CONVEYOR_PUBLICATION_COORDINATOR_SCHEMA_VERSION
            || self.publication_id.is_nil()
            || self.feature_id.is_nil()
            || self.feature_lease_id.is_nil()
            || self.integration_id.is_nil()
            || self.validation_id.is_nil()
            || self.review_call_id.is_nil()
            || self.specification_revision == 0
            || self.expected_lifecycle_revision == 0
            || self.candidate_diff_sha256 == [0; 32]
            || self.evidence_manifest_sha256 == [0; 32]
            || self.review_decision_sha256 == [0; 32]
            || self.branch_policy_sha256 == [0; 32]
            || self.grants.registration == 0
            || self.grants.cloud_disclosure == 0
            || self.grants.autonomous_publication == 0
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_git_commit(&self.candidate_commit)?;
        validate_git_commit(&self.candidate_tree)?;
        validate_git_commit(&self.remote_base_commit)?;
        validate_identifier(
            "provider_id",
            &self.provider_id,
            MAX_FEATURE_CONVEYOR_IDENTIFIER_BYTES,
        )?;
        validate_identifier(
            "model_id",
            &self.model_id,
            MAX_FEATURE_CONVEYOR_IDENTIFIER_BYTES,
        )?;
        validate_serialized_limit(
            "feature_conveyor_publication_request",
            self,
            MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES,
        )
    }
}

pub fn feature_conveyor_publication_request_binding_sha256(
    request: &FeatureConveyorPublicationRequest,
) -> Result<[u8; 32], ProtocolError> {
    request.validate()?;
    let value = serde_json::to_value(request).map_err(|error| ProtocolError::Serialization {
        field: "feature_conveyor_publication_request",
        message: error.to_string(),
    })?;
    let canonical = canonical_json_bytes(&value)?;
    let mut digest = Sha256::new();
    digest.update(b"assemblywright.publication-request-binding.v1\0");
    digest.update((canonical.len() as u64).to_be_bytes());
    digest.update(canonical);
    Ok(digest.finalize().into())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorPublicationStatus {
    Succeeded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorPublicationReceipt {
    pub schema_version: u16,
    pub publication_id: Uuid,
    pub feature_id: Uuid,
    pub specification_revision: u64,
    pub lifecycle_revision: u64,
    pub candidate_commit: String,
    pub merge_commit: String,
    pub remote_main_commit: String,
    pub post_merge_evidence_sha256: [u8; 32],
    pub branch_policy_sha256: [u8; 32],
    pub queue_revision: u64,
    pub emergency_pause_revision: u64,
    pub grants: FeatureConveyorGrantRevisions,
    pub status: FeatureConveyorPublicationStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorPublicationAction {
    PushBranch,
    UpsertPullRequest,
    ObserveRequiredChecks,
    VerifyPullRequestHead,
    MergePullRequest,
    ReconcileRemoteMain,
    RunPostMergeGate,
}

impl FeatureConveyorPublicationAction {
    pub const ORDERED: [Self; 7] = [
        Self::PushBranch,
        Self::UpsertPullRequest,
        Self::ObserveRequiredChecks,
        Self::VerifyPullRequestHead,
        Self::MergePullRequest,
        Self::ReconcileRemoteMain,
        Self::RunPostMergeGate,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::PushBranch => "push_branch",
            Self::UpsertPullRequest => "upsert_pull_request",
            Self::ObserveRequiredChecks => "observe_required_checks",
            Self::VerifyPullRequestHead => "verify_pull_request_head",
            Self::MergePullRequest => "merge_pull_request",
            Self::ReconcileRemoteMain => "reconcile_remote_main",
            Self::RunPostMergeGate => "run_post_merge_gate",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorPublicationActionEvidence {
    pub schema_version: u16,
    pub publication_id: Uuid,
    pub action: FeatureConveyorPublicationAction,
    pub remote_base_commit: String,
    pub candidate_commit: String,
    pub feature_branch: String,
    pub base_branch: String,
    pub pull_request_number: Option<u64>,
    pub observed_head_commit: String,
    pub required_checks_sha256: Option<[u8; 32]>,
    pub required_check_count: u16,
    pub required_checks_passed: bool,
    pub branch_protection_enforced: bool,
    pub bypass_used: bool,
    pub merge_strategy: Option<String>,
    pub resulting_main_commit: Option<String>,
    pub post_merge_gate_id: Option<String>,
    pub post_merge_gate_passed: bool,
    pub evidence_sha256: [u8; 32],
}

impl FeatureConveyorPublicationActionEvidence {
    pub fn expected_evidence_sha256(&self) -> Result<[u8; 32], ProtocolError> {
        let mut value =
            serde_json::to_value(self).map_err(|error| ProtocolError::Serialization {
                field: "feature_conveyor_publication_action_evidence",
                message: error.to_string(),
            })?;
        value
            .as_object_mut()
            .ok_or(ProtocolError::InvalidFeatureConveyorOwnerControl)?
            .remove("evidence_sha256");
        let canonical = canonical_json_bytes(&value)?;
        let mut digest = Sha256::new();
        digest.update(b"assemblywright.publication-action-evidence.v1\0");
        digest.update((canonical.len() as u64).to_be_bytes());
        digest.update(canonical);
        Ok(digest.finalize().into())
    }

    pub fn seal(mut self) -> Result<Self, ProtocolError> {
        self.evidence_sha256 = self.expected_evidence_sha256()?;
        self.validate()?;
        Ok(self)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.schema_version != FEATURE_CONVEYOR_PUBLICATION_COORDINATOR_SCHEMA_VERSION
            || self.publication_id.is_nil()
            || self.evidence_sha256 == [0; 32]
            || self.evidence_sha256 != self.expected_evidence_sha256()?
            || !self.branch_protection_enforced
            || self.bypass_used
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_git_commit(&self.remote_base_commit)?;
        validate_git_commit(&self.candidate_commit)?;
        validate_git_commit(&self.observed_head_commit)?;
        validate_publication_branch("feature_branch", &self.feature_branch)?;
        validate_publication_branch("base_branch", &self.base_branch)?;
        if self
            .resulting_main_commit
            .as_deref()
            .is_some_and(|commit| validate_git_commit(commit).is_err())
            || self
                .required_checks_sha256
                .is_some_and(|digest| digest == [0; 32])
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        let pr = self.pull_request_number.is_some_and(|number| number > 0);
        let checks = self.required_checks_sha256.is_some()
            && self.required_check_count > 0
            && self.required_checks_passed;
        let merge = self.merge_strategy.is_some() && self.resulting_main_commit.is_some();
        let gate = self.post_merge_gate_id.is_some() && self.post_merge_gate_passed;
        let exact_shape = match self.action {
            FeatureConveyorPublicationAction::PushBranch => {
                !pr && !checks && !merge && !gate && self.required_check_count == 0
            }
            FeatureConveyorPublicationAction::UpsertPullRequest => {
                pr && !checks && !merge && !gate && self.required_check_count == 0
            }
            FeatureConveyorPublicationAction::ObserveRequiredChecks
            | FeatureConveyorPublicationAction::VerifyPullRequestHead => {
                pr && checks && !merge && !gate
            }
            FeatureConveyorPublicationAction::MergePullRequest => pr && checks && merge && !gate,
            FeatureConveyorPublicationAction::ReconcileRemoteMain => {
                !pr && checks && merge && !gate
            }
            FeatureConveyorPublicationAction::RunPostMergeGate => !pr && checks && merge && gate,
        };
        if !exact_shape {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        if let Some(strategy) = &self.merge_strategy {
            if !matches!(strategy.as_str(), "merge" | "squash" | "rebase") {
                return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
            }
        }
        if let Some(gate) = &self.post_merge_gate_id {
            if gate != "release-local" {
                return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
            }
        }
        validate_serialized_limit(
            "feature_conveyor_publication_action_evidence",
            self,
            MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES,
        )
    }
}

pub fn feature_conveyor_publication_required_checks_sha256(
    checks: &[String],
) -> Result<[u8; 32], ProtocolError> {
    if checks.is_empty()
        || checks.len() > MAX_FEATURE_CONVEYOR_PUBLICATION_CHECKS
        || checks.iter().any(|check| {
            check.is_empty()
                || check.len() > MAX_FEATURE_CONVEYOR_IDENTIFIER_BYTES
                || check.trim() != check
                || !check.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-' | b'/')
                })
        })
    {
        return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
    }
    let mut sorted = checks.to_vec();
    sorted.sort();
    let before = sorted.len();
    sorted.dedup();
    if sorted.len() != before {
        return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
    }
    let canonical = canonical_json_bytes(&serde_json::to_value(sorted).map_err(|error| {
        ProtocolError::Serialization {
            field: "feature_conveyor_publication_required_checks",
            message: error.to_string(),
        }
    })?)?;
    let mut digest = Sha256::new();
    digest.update(b"assemblywright.publication-required-checks.v1\0");
    digest.update((canonical.len() as u64).to_be_bytes());
    digest.update(canonical);
    Ok(digest.finalize().into())
}

impl FeatureConveyorPublicationReceipt {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.schema_version != FEATURE_CONVEYOR_PUBLICATION_COORDINATOR_SCHEMA_VERSION
            || self.publication_id.is_nil()
            || self.feature_id.is_nil()
            || self.specification_revision == 0
            || self.lifecycle_revision == 0
            || self.post_merge_evidence_sha256 == [0; 32]
            || self.branch_policy_sha256 == [0; 32]
            || self.grants.registration == 0
            || self.grants.cloud_disclosure == 0
            || self.grants.autonomous_publication == 0
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_git_commit(&self.candidate_commit)?;
        validate_git_commit(&self.merge_commit)?;
        validate_git_commit(&self.remote_main_commit)?;
        if self.merge_commit != self.remote_main_commit {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_serialized_limit(
            "feature_conveyor_publication_receipt",
            self,
            MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES,
        )
    }
}

impl FeatureConveyorReviewGatewayReceipt {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.schema_version != FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION
            || self.review_call_id.is_nil()
            || self.feature_id.is_nil()
            || self.feature_lease_id.is_nil()
            || self.integration_id.is_nil()
            || self.validation_id.is_nil()
            || self.specification_revision == 0
            || self.lifecycle_revision == 0
            || self.candidate_diff_sha256 == [0; 32]
            || self.evidence_manifest_sha256 == [0; 32]
            || self.review_packet_sha256 == [0; 32]
            || self.decision_sha256 == [0; 32]
            || self.grants.registration == 0
            || self.grants.cloud_disclosure == 0
            || self.grants.autonomous_publication == 0
            || !(1..=MAX_FEATURE_CONVEYOR_REVIEW_TRANSPORT_ATTEMPTS_PER_CANDIDATE)
                .contains(&self.candidate_attempt)
            || !(1..=MAX_FEATURE_CONVEYOR_REVIEW_CALLS_PER_FEATURE).contains(&self.feature_call)
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_git_commit(&self.candidate_commit)?;
        validate_identifier(
            "provider_id",
            &self.provider_id,
            MAX_FEATURE_CONVEYOR_IDENTIFIER_BYTES,
        )?;
        validate_identifier(
            "model_id",
            &self.model_id,
            MAX_FEATURE_CONVEYOR_IDENTIFIER_BYTES,
        )?;
        validate_serialized_limit(
            "feature_conveyor_review_gateway_receipt",
            self,
            MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorArtifactIntegrationPlan {
    pub schema_version: u16,
    pub feature_id: Uuid,
    pub specification_revision: u64,
    pub lifecycle_revision: u64,
    pub feature_lease_id: Uuid,
    pub snapshot_id: Uuid,
    pub snapshot_sha256: [u8; 32],
    pub artifact_ids: Vec<Uuid>,
    pub queue_revision: u64,
    pub emergency_pause_revision: u64,
    pub grants: FeatureConveyorGrantRevisions,
    pub base_commit: String,
}

impl FeatureConveyorArtifactIntegrationPlan {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_artifact_integration_plan",
            frame,
            MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES,
            Self::validate,
        )
    }
    pub fn validate(&self) -> Result<(), ProtocolError> {
        FeatureConveyorArtifactIntegrationRequest {
            schema_version: self.schema_version,
            integration_id: Uuid::from_u128(1),
            feature_id: self.feature_id,
            specification_revision: self.specification_revision,
            expected_lifecycle_revision: self.lifecycle_revision,
            feature_lease_id: self.feature_lease_id,
            snapshot_id: self.snapshot_id,
            snapshot_sha256: self.snapshot_sha256,
            artifact_ids: self.artifact_ids.clone(),
            expected_queue_revision: self.queue_revision,
            expected_emergency_pause_revision: self.emergency_pause_revision,
            grants: self.grants,
            base_commit: self.base_commit.clone(),
        }
        .validate()?;
        validate_serialized_limit(
            "feature_conveyor_artifact_integration_plan",
            self,
            MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorArtifactIntegrationRequest {
    pub schema_version: u16,
    pub integration_id: Uuid,
    pub feature_id: Uuid,
    pub specification_revision: u64,
    pub expected_lifecycle_revision: u64,
    pub feature_lease_id: Uuid,
    pub snapshot_id: Uuid,
    pub snapshot_sha256: [u8; 32],
    pub artifact_ids: Vec<Uuid>,
    pub expected_queue_revision: u64,
    pub expected_emergency_pause_revision: u64,
    pub grants: FeatureConveyorGrantRevisions,
    pub base_commit: String,
}

impl FeatureConveyorArtifactIntegrationRequest {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_artifact_integration_request",
            frame,
            MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.schema_version != FEATURE_CONVEYOR_ARTIFACT_INTEGRATION_SCHEMA_VERSION
            || self.integration_id.is_nil()
            || self.feature_id.is_nil()
            || self.feature_lease_id.is_nil()
            || self.snapshot_id.is_nil()
            || self.specification_revision == 0
            || self.expected_lifecycle_revision == 0
            || self.snapshot_sha256 == [0; 32]
            || self.artifact_ids.is_empty()
            || self.artifact_ids.len() > MAX_FEATURE_CONVEYOR_INTEGRATION_ARTIFACTS
            || self.grants.registration == 0
            || self.grants.cloud_disclosure == 0
            || self.grants.autonomous_publication == 0
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        let mut prior = None;
        for artifact_id in &self.artifact_ids {
            if artifact_id.is_nil() || prior.is_some_and(|value| value >= *artifact_id) {
                return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
            }
            prior = Some(*artifact_id);
        }
        validate_git_commit(&self.base_commit)?;
        validate_serialized_limit(
            "feature_conveyor_artifact_integration_request",
            self,
            MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorArtifactIntegrationStatus {
    CandidateFrozen,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorArtifactIntegrationReceipt {
    pub schema_version: u16,
    pub integration_id: Uuid,
    pub feature_id: Uuid,
    pub specification_revision: u64,
    pub lifecycle_revision: u64,
    pub feature_lease_id: Uuid,
    pub snapshot_id: Uuid,
    pub snapshot_sha256: [u8; 32],
    pub artifact_set_sha256: [u8; 32],
    pub candidate_commit: String,
    pub candidate_tree: String,
    pub base_commit: String,
    pub queue_revision: u64,
    pub emergency_pause_revision: u64,
    pub grants: FeatureConveyorGrantRevisions,
    pub status: FeatureConveyorArtifactIntegrationStatus,
}

impl FeatureConveyorArtifactIntegrationReceipt {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_artifact_integration_receipt",
            frame,
            MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.schema_version != FEATURE_CONVEYOR_ARTIFACT_INTEGRATION_SCHEMA_VERSION
            || self.integration_id.is_nil()
            || self.feature_id.is_nil()
            || self.feature_lease_id.is_nil()
            || self.snapshot_id.is_nil()
            || self.specification_revision == 0
            || self.lifecycle_revision == 0
            || self.snapshot_sha256 == [0; 32]
            || self.artifact_set_sha256 == [0; 32]
            || self.grants.registration == 0
            || self.grants.cloud_disclosure == 0
            || self.grants.autonomous_publication == 0
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_git_commit(&self.base_commit)?;
        validate_git_commit(&self.candidate_commit)?;
        validate_git_commit(&self.candidate_tree)?;
        validate_serialized_limit(
            "feature_conveyor_artifact_integration_receipt",
            self,
            MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalCodingDeleteFileArguments {
    pub path: String,
    pub expected_before_sha256: [u8; 32],
}

impl LocalCodingDeleteFileArguments {
    fn validate(&self) -> Result<(), ProtocolError> {
        validate_local_coding_relative_path(&self.path)?;
        if self.expected_before_sha256 == [0; 32] {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        Ok(())
    }
}

/// Bounded immutable implementation packet for one owner-approved dispatch.
/// Canonical JSON of this complete value is the work-packet digest binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorCodingWorkPacketMetadata {
    pub packet_id: Uuid,
    pub ordinal: u16,
    pub acceptance_criteria_count: u16,
    pub allowed_paths: Vec<String>,
    pub operations: Vec<LocalCodingEditOperation>,
}

impl FeatureConveyorCodingWorkPacketMetadata {
    pub fn fixture(packet_id: Uuid, expected_before_sha256: [u8; 32]) -> Self {
        let replacement_sha256 = Sha256::digest(LOCAL_CODING_FIXTURE_CONTENT).into();
        Self {
            packet_id,
            ordinal: 1,
            acceptance_criteria_count: 1,
            allowed_paths: vec![LOCAL_CODING_FIXTURE_ALLOWED_PATH.to_string()],
            operations: vec![LocalCodingEditOperation::Write(
                LocalCodingWriteFileArguments {
                    path: LOCAL_CODING_FIXTURE_ALLOWED_PATH.to_string(),
                    expected_before_sha256: Some(expected_before_sha256),
                    replacement_sha256,
                    replacement_hex: hex_lower(LOCAL_CODING_FIXTURE_CONTENT),
                    executable: false,
                },
            )],
        }
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_uuid("packet_id", self.packet_id)?;
        if self.ordinal == 0
            || self.acceptance_criteria_count == 0
            || self.allowed_paths.is_empty()
            || self.allowed_paths.len() > MAX_LOCAL_CODING_EDIT_PATHS
            || self.operations.is_empty()
            || self.operations.len() > MAX_LOCAL_CODING_EDIT_OPERATIONS
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        let mut prior_path: Option<&str> = None;
        let mut allowed = BTreeSet::new();
        for path in &self.allowed_paths {
            validate_local_coding_relative_path(path)?;
            if prior_path.is_some_and(|prior| prior >= path.as_str())
                || !allowed.insert(path.as_str())
            {
                return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
            }
            prior_path = Some(path);
        }
        let mut operated = BTreeSet::new();
        let mut replacement_bytes = 0_usize;
        for operation in &self.operations {
            operation.validate()?;
            if !allowed.contains(operation.path()) || !operated.insert(operation.path()) {
                return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
            }
            if let LocalCodingEditOperation::Write(arguments) = operation {
                replacement_bytes = replacement_bytes
                    .checked_add(arguments.replacement_hex.len() / 2)
                    .ok_or(ProtocolError::InvalidFeatureConveyorOwnerControl)?;
            }
        }
        if operated.len() != allowed.len()
            || replacement_bytes > MAX_LOCAL_CODING_EDIT_CONTENT_BYTES
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_serialized_limit(
            "feature_conveyor_coding_work_packet",
            self,
            MAX_LOCAL_CODING_CONTEXT_BYTES,
        )?;
        Ok(())
    }

    pub fn canonical_sha256(&self) -> Result<[u8; 32], ProtocolError> {
        self.validate()?;
        let value = serde_json::to_value(self).map_err(|error| ProtocolError::Serialization {
            field: "feature_conveyor_coding_work_packet",
            message: error.to_string(),
        })?;
        let bytes = canonical_json_bytes(&value)?;
        Ok(Sha256::digest(bytes).into())
    }

    pub fn allowed_paths_sha256(&self) -> Result<[u8; 32], ProtocolError> {
        self.validate()?;
        Ok(local_coding_paths_sha256(&self.allowed_paths))
    }
}

/// Explicit owner-token loopback action for one already-claimed snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorCodingDispatchRequest {
    pub schema_version: u16,
    pub feature_id: Uuid,
    pub specification_revision: u64,
    pub expected_lifecycle_revision: u64,
    pub feature_lease_id: Uuid,
    pub snapshot_id: Uuid,
    pub snapshot_sha256: [u8; 32],
    pub work_packet_sha256: [u8; 32],
    pub work_packet: FeatureConveyorCodingWorkPacketMetadata,
    pub device_id: DeviceId,
    pub device_registry_revision: u64,
    pub expected_queue_revision: u64,
    pub expected_emergency_pause_revision: u64,
}

impl FeatureConveyorCodingDispatchRequest {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_coding_dispatch_request",
            frame,
            MAX_FEATURE_CONVEYOR_CODING_DISPATCH_REQUEST_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_fixed_value(
            "schema_version",
            self.schema_version.to_string(),
            FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION.to_string(),
        )?;
        validate_uuid("feature_id", self.feature_id)?;
        validate_uuid("feature_lease_id", self.feature_lease_id)?;
        validate_uuid("snapshot_id", self.snapshot_id)?;
        validate_uuid("device_id", self.device_id.0)?;
        validate_positive_limit(
            "specification_revision",
            self.specification_revision,
            u64::MAX,
        )?;
        validate_positive_limit(
            "expected_lifecycle_revision",
            self.expected_lifecycle_revision,
            u64::MAX,
        )?;
        validate_positive_limit(
            "device_registry_revision",
            self.device_registry_revision,
            u64::MAX,
        )?;
        self.work_packet.validate()?;
        if self.snapshot_sha256 == [0; 32]
            || self.work_packet_sha256 == [0; 32]
            || self.work_packet.canonical_sha256()? != self.work_packet_sha256
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_serialized_limit(
            "feature_conveyor_coding_dispatch_request",
            self,
            MAX_FEATURE_CONVEYOR_CODING_DISPATCH_REQUEST_BYTES,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorCodingDispatchStatus {
    Queued,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorCodingDispatchReceipt {
    pub schema_version: u16,
    pub feature_id: Uuid,
    pub specification_revision: u64,
    pub lifecycle_revision: u64,
    pub feature_lease_id: Uuid,
    pub snapshot_id: Uuid,
    pub snapshot_sha256: [u8; 32],
    pub work_packet_sha256: [u8; 32],
    pub packet_id: Uuid,
    pub device_id: DeviceId,
    pub device_registry_revision: u64,
    pub queue_revision: u64,
    pub emergency_pause_revision: u64,
    pub task_id: TaskId,
    pub step_id: StepId,
    pub status: FeatureConveyorCodingDispatchStatus,
}

impl FeatureConveyorCodingDispatchReceipt {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_coding_dispatch_receipt",
            frame,
            MAX_FEATURE_CONVEYOR_CODING_DISPATCH_REQUEST_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_fixed_value(
            "schema_version",
            self.schema_version.to_string(),
            FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION.to_string(),
        )?;
        validate_uuid("feature_id", self.feature_id)?;
        validate_uuid("feature_lease_id", self.feature_lease_id)?;
        validate_uuid("snapshot_id", self.snapshot_id)?;
        validate_uuid("packet_id", self.packet_id)?;
        validate_uuid("device_id", self.device_id.0)?;
        validate_positive_limit(
            "specification_revision",
            self.specification_revision,
            u64::MAX,
        )?;
        validate_positive_limit("lifecycle_revision", self.lifecycle_revision, u64::MAX)?;
        validate_positive_limit(
            "device_registry_revision",
            self.device_registry_revision,
            u64::MAX,
        )?;
        if self.snapshot_sha256 == [0; 32] || self.work_packet_sha256 == [0; 32] {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_uuid("task_id", self.task_id.0)?;
        validate_uuid("step_id", self.step_id.0)
    }
}

/// Explicit owner-token loopback action that stops one exact active feature.
///
/// Cancellation retains the active feature lease and does not authorize queue
/// advancement. Queue and Emergency Pause revisions are compare-and-set
/// bindings, including when cancellation is deliberately performed while
/// Emergency Pause is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorCancelActiveFeatureRequest {
    pub schema_version: u16,
    pub feature_id: Uuid,
    pub expected_lifecycle_revision: u64,
    pub expected_queue_revision: u64,
    pub expected_emergency_pause_revision: u64,
}

/// Remote designated-bridge form of owner cancellation. The designation CAS
/// is additional to the existing lifecycle/queue/Emergency Pause bindings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorRemoteCancelActiveFeatureRequest {
    pub schema_version: u16,
    pub feature_id: Uuid,
    pub expected_lifecycle_revision: u64,
    pub expected_queue_revision: u64,
    pub expected_owner_control_designation_revision: u64,
    pub expected_emergency_pause_revision: u64,
}

impl FeatureConveyorRemoteCancelActiveFeatureRequest {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_remote_cancel_active_feature_request",
            frame,
            MAX_FEATURE_CONVEYOR_OWNER_RESOLUTION_REQUEST_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        FeatureConveyorCancelActiveFeatureRequest {
            schema_version: self.schema_version,
            feature_id: self.feature_id,
            expected_lifecycle_revision: self.expected_lifecycle_revision,
            expected_queue_revision: self.expected_queue_revision,
            expected_emergency_pause_revision: self.expected_emergency_pause_revision,
        }
        .validate()?;
        validate_positive_limit(
            "expected_owner_control_designation_revision",
            self.expected_owner_control_designation_revision,
            u64::MAX,
        )
    }
}

impl FeatureConveyorCancelActiveFeatureRequest {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_cancel_active_feature_request",
            frame,
            MAX_FEATURE_CONVEYOR_OWNER_RESOLUTION_REQUEST_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_fixed_value(
            "schema_version",
            self.schema_version.to_string(),
            FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION.to_string(),
        )?;
        validate_uuid("feature_id", self.feature_id)?;
        validate_positive_limit(
            "expected_lifecycle_revision",
            self.expected_lifecycle_revision,
            u64::MAX,
        )?;
        validate_serialized_limit(
            "feature_conveyor_cancel_active_feature_request",
            self,
            MAX_FEATURE_CONVEYOR_OWNER_RESOLUTION_REQUEST_BYTES,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorCancelActiveFeatureStatus {
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorCancelActiveFeatureReceipt {
    pub schema_version: u16,
    pub feature_id: Uuid,
    pub lifecycle_revision: u64,
    pub queue_revision: u64,
    pub emergency_pause_revision: u64,
    pub lease_retained: bool,
    pub advancement_authorized: bool,
    pub status: FeatureConveyorCancelActiveFeatureStatus,
}

impl FeatureConveyorCancelActiveFeatureReceipt {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_cancel_active_feature_receipt",
            frame,
            MAX_FEATURE_CONVEYOR_OWNER_RESOLUTION_REQUEST_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        let request = FeatureConveyorCancelActiveFeatureRequest {
            schema_version: self.schema_version,
            feature_id: self.feature_id,
            expected_lifecycle_revision: self.lifecycle_revision,
            expected_queue_revision: self.queue_revision,
            expected_emergency_pause_revision: self.emergency_pause_revision,
        };
        request.validate()?;
        if !self.lease_retained || self.advancement_authorized {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        Ok(())
    }
}

/// Digest-only evidence required for an explicit owner abandonment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorAbandonmentEvidence {
    pub safe_reconciliation_sha256: [u8; 32],
    pub merged: bool,
    pub verified_healthy_main_sha256: Option<[u8; 32]>,
}

impl FeatureConveyorAbandonmentEvidence {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.safe_reconciliation_sha256 == [0; 32]
            || self
                .verified_healthy_main_sha256
                .is_some_and(|digest| digest == [0; 32])
            || (self.merged && self.verified_healthy_main_sha256.is_none())
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        Ok(())
    }
}

/// Explicit owner-token loopback action that records non-approval and advances
/// only after the kernel proves reconciliation and, when merged, healthy main.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorAbandonAndAdvanceRequest {
    pub schema_version: u16,
    pub feature_id: Uuid,
    pub expected_lifecycle_revision: u64,
    pub expected_queue_revision: u64,
    pub expected_emergency_pause_revision: u64,
    pub evidence: FeatureConveyorAbandonmentEvidence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorRemoteAbandonAndAdvanceRequest {
    pub schema_version: u16,
    pub feature_id: Uuid,
    pub expected_lifecycle_revision: u64,
    pub expected_queue_revision: u64,
    pub expected_owner_control_designation_revision: u64,
    pub expected_emergency_pause_revision: u64,
    pub evidence: FeatureConveyorAbandonmentEvidence,
}

impl FeatureConveyorRemoteAbandonAndAdvanceRequest {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_remote_abandon_and_advance_request",
            frame,
            MAX_FEATURE_CONVEYOR_OWNER_RESOLUTION_REQUEST_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        FeatureConveyorAbandonAndAdvanceRequest {
            schema_version: self.schema_version,
            feature_id: self.feature_id,
            expected_lifecycle_revision: self.expected_lifecycle_revision,
            expected_queue_revision: self.expected_queue_revision,
            expected_emergency_pause_revision: self.expected_emergency_pause_revision,
            evidence: self.evidence,
        }
        .validate()?;
        validate_positive_limit(
            "expected_owner_control_designation_revision",
            self.expected_owner_control_designation_revision,
            u64::MAX,
        )
    }
}

impl FeatureConveyorAbandonAndAdvanceRequest {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_abandon_and_advance_request",
            frame,
            MAX_FEATURE_CONVEYOR_OWNER_RESOLUTION_REQUEST_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_fixed_value(
            "schema_version",
            self.schema_version.to_string(),
            FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION.to_string(),
        )?;
        validate_uuid("feature_id", self.feature_id)?;
        validate_positive_limit(
            "expected_lifecycle_revision",
            self.expected_lifecycle_revision,
            u64::MAX,
        )?;
        self.evidence.validate()?;
        if self.expected_queue_revision == u64::MAX {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_serialized_limit(
            "feature_conveyor_abandon_and_advance_request",
            self,
            MAX_FEATURE_CONVEYOR_OWNER_RESOLUTION_REQUEST_BYTES,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorAbandonAndAdvanceStatus {
    Abandoned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorAbandonAndAdvanceReceipt {
    pub schema_version: u16,
    pub feature_id: Uuid,
    pub lifecycle_revision: u64,
    pub queue_revision: u64,
    pub emergency_pause_revision: u64,
    pub lease_released: bool,
    pub status: FeatureConveyorAbandonAndAdvanceStatus,
}

impl FeatureConveyorAbandonAndAdvanceReceipt {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_abandon_and_advance_receipt",
            frame,
            MAX_FEATURE_CONVEYOR_OWNER_RESOLUTION_REQUEST_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_fixed_value(
            "schema_version",
            self.schema_version.to_string(),
            FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION.to_string(),
        )?;
        validate_uuid("feature_id", self.feature_id)?;
        validate_positive_limit("lifecycle_revision", self.lifecycle_revision, u64::MAX)?;
        if !self.lease_released {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_serialized_limit(
            "feature_conveyor_abandon_and_advance_receipt",
            self,
            MAX_FEATURE_CONVEYOR_OWNER_RESOLUTION_REQUEST_BYTES,
        )
    }
}

/// The only context accepted by `local.coding.v1`. It contains no repository
/// bytes, repository path, allowed path, command, provider prompt, or credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalCodingJobRequest {
    pub feature_id: Uuid,
    pub specification_revision: u64,
    pub lifecycle_revision: u64,
    pub feature_lease_id: Uuid,
    pub snapshot_id: Uuid,
    pub snapshot_sha256: [u8; 32],
    pub work_packet_sha256: [u8; 32],
    pub work_packet: FeatureConveyorCodingWorkPacketMetadata,
    pub device_id: DeviceId,
    pub device_registry_revision: u64,
    pub queue_revision: u64,
    pub emergency_pause_revision: u64,
}

impl LocalCodingJobRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        let owner_request = FeatureConveyorCodingDispatchRequest {
            schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
            feature_id: self.feature_id,
            specification_revision: self.specification_revision,
            expected_lifecycle_revision: self.lifecycle_revision,
            feature_lease_id: self.feature_lease_id,
            snapshot_id: self.snapshot_id,
            snapshot_sha256: self.snapshot_sha256,
            work_packet_sha256: self.work_packet_sha256,
            work_packet: self.work_packet.clone(),
            device_id: self.device_id,
            device_registry_revision: self.device_registry_revision,
            expected_queue_revision: self.queue_revision,
            expected_emergency_pause_revision: self.emergency_pause_revision,
        };
        owner_request.validate()
    }
}

/// Strict cursor for one bounded read from the exact snapshot assigned to an
/// already-leased `local.coding.v1` attempt. Repository paths and bytes remain
/// outside the job envelope and are available only through this separate pull.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalCodingSnapshotChunkRequest {
    pub protocol_version: u16,
    pub connection_epoch: u64,
    pub task_id: TaskId,
    pub step_id: StepId,
    pub attempt_id: AttemptId,
    pub lease_id: LeaseId,
    pub cancellation_id: CancellationId,
    pub snapshot_id: Uuid,
    pub snapshot_sha256: [u8; 32],
    pub offset: u64,
}

impl LocalCodingSnapshotChunkRequest {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "local_coding_snapshot_chunk_request",
            frame,
            MAX_LOCAL_CODING_CONTEXT_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_version(self.protocol_version)?;
        validate_positive_limit("connection_epoch", self.connection_epoch, u64::MAX)?;
        validate_uuid("task_id", self.task_id.0)?;
        validate_uuid("step_id", self.step_id.0)?;
        validate_uuid("attempt_id", self.attempt_id.0)?;
        validate_uuid("lease_id", self.lease_id.0)?;
        validate_uuid("cancellation_id", self.cancellation_id.0)?;
        validate_uuid("snapshot_id", self.snapshot_id)?;
        if self.snapshot_sha256 == [0; 32] || self.offset > MAX_LOCAL_CODING_SNAPSHOT_BUNDLE_BYTES {
            return Err(ProtocolError::InvalidLocalCodingSnapshotTransfer);
        }
        validate_serialized_limit(
            "local_coding_snapshot_chunk_request",
            self,
            MAX_LOCAL_CODING_CONTEXT_BYTES,
        )
    }

    pub fn validate_for_job(&self, job: &JobEnvelope) -> Result<(), ProtocolError> {
        self.validate()?;
        let context = job.validate_local_coding()?;
        if self.protocol_version != job.protocol_version
            || self.connection_epoch != job.connection_epoch
            || self.task_id != job.task_id
            || self.step_id != job.step_id
            || self.attempt_id != job.attempt_id
            || self.lease_id != job.lease_id
            || self.cancellation_id != job.cancellation_id
            || self.snapshot_id != context.snapshot_id
            || self.snapshot_sha256 != context.snapshot_sha256
        {
            return Err(ProtocolError::InvalidLocalCodingSnapshotTransfer);
        }
        Ok(())
    }
}

/// One bounded, sequentially addressed portion of a deterministic snapshot
/// bundle. Hex encoding keeps the JSON contract strict and portable without
/// allowing arbitrary nested payload values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalCodingSnapshotChunk {
    pub protocol_version: u16,
    pub connection_epoch: u64,
    pub task_id: TaskId,
    pub step_id: StepId,
    pub attempt_id: AttemptId,
    pub lease_id: LeaseId,
    pub cancellation_id: CancellationId,
    pub snapshot_id: Uuid,
    pub snapshot_sha256: [u8; 32],
    pub offset: u64,
    pub total_bytes: u64,
    pub content_sha256: [u8; 32],
    pub content_hex: String,
    pub complete: bool,
}

impl LocalCodingSnapshotChunk {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "local_coding_snapshot_chunk",
            frame,
            MAX_LOCAL_CODING_SNAPSHOT_CHUNK_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_version(self.protocol_version)?;
        validate_positive_limit("connection_epoch", self.connection_epoch, u64::MAX)?;
        validate_uuid("task_id", self.task_id.0)?;
        validate_uuid("step_id", self.step_id.0)?;
        validate_uuid("attempt_id", self.attempt_id.0)?;
        validate_uuid("lease_id", self.lease_id.0)?;
        validate_uuid("cancellation_id", self.cancellation_id.0)?;
        validate_uuid("snapshot_id", self.snapshot_id)?;
        if self.snapshot_sha256 == [0; 32]
            || self.content_sha256 == [0; 32]
            || self.total_bytes == 0
            || self.total_bytes > MAX_LOCAL_CODING_SNAPSHOT_BUNDLE_BYTES
            || self.offset >= self.total_bytes
            || self.content_hex.is_empty()
            || self.content_hex.len() % 2 != 0
            || self.content_hex.len() / 2 > MAX_LOCAL_CODING_SNAPSHOT_CHUNK_BYTES
            || !self
                .content_hex
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(ProtocolError::InvalidLocalCodingSnapshotTransfer);
        }
        let content = self.decode_content()?;
        let end = self
            .offset
            .checked_add(content.len() as u64)
            .ok_or(ProtocolError::InvalidLocalCodingSnapshotTransfer)?;
        if end > self.total_bytes
            || self.complete != (end == self.total_bytes)
            || Sha256::digest(&content).as_slice() != self.content_sha256
        {
            return Err(ProtocolError::InvalidLocalCodingSnapshotTransfer);
        }
        validate_serialized_limit(
            "local_coding_snapshot_chunk",
            self,
            MAX_LOCAL_CODING_SNAPSHOT_CHUNK_FRAME_BYTES,
        )
    }

    pub fn validate_for_request(
        &self,
        request: &LocalCodingSnapshotChunkRequest,
    ) -> Result<(), ProtocolError> {
        self.validate()?;
        request.validate()?;
        if self.protocol_version != request.protocol_version
            || self.connection_epoch != request.connection_epoch
            || self.task_id != request.task_id
            || self.step_id != request.step_id
            || self.attempt_id != request.attempt_id
            || self.lease_id != request.lease_id
            || self.cancellation_id != request.cancellation_id
            || self.snapshot_id != request.snapshot_id
            || self.snapshot_sha256 != request.snapshot_sha256
            || self.offset != request.offset
        {
            return Err(ProtocolError::InvalidLocalCodingSnapshotTransfer);
        }
        Ok(())
    }

    pub fn decode_content(&self) -> Result<Vec<u8>, ProtocolError> {
        if self.content_hex.len() % 2 != 0
            || self.content_hex.len() / 2 > MAX_LOCAL_CODING_SNAPSHOT_CHUNK_BYTES
        {
            return Err(ProtocolError::InvalidLocalCodingSnapshotTransfer);
        }
        self.content_hex
            .as_bytes()
            .chunks_exact(2)
            .map(|pair| {
                let high = decode_lower_hex_nibble(pair[0])?;
                let low = decode_lower_hex_nibble(pair[1])?;
                Ok((high << 4) | low)
            })
            .collect()
    }
}

fn decode_lower_hex_nibble(byte: u8) -> Result<u8, ProtocolError> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(ProtocolError::InvalidLocalCodingSnapshotTransfer),
    }
}

pub fn feature_conveyor_provider_binding_sha256(provider_id: &str, model_id: &str) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"assemblywright.feature-provider-binding.v1\0");
    digest.update((provider_id.len() as u64).to_be_bytes());
    digest.update(provider_id.as_bytes());
    digest.update((model_id.len() as u64).to_be_bytes());
    digest.update(model_id.as_bytes());
    digest.finalize().into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorGrantRevisions {
    pub registration: u64,
    pub cloud_disclosure: u64,
    pub autonomous_publication: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorApprovedSpecification {
    pub feature_id: Uuid,
    pub revision: u64,
    pub repository_id: Uuid,
    pub manifest: Value,
    pub manifest_sha256: [u8; 32],
    pub design_sha256: [u8; 32],
    pub brainstorming_sha256: [u8; 32],
    pub owner_approval_sha256: [u8; 32],
    pub grants: FeatureConveyorGrantRevisions,
    pub provider_id: String,
    pub model_id: String,
    pub dependencies: Vec<Uuid>,
}

impl FeatureConveyorApprovedSpecification {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_uuid("feature_id", self.feature_id)?;
        validate_uuid("repository_id", self.repository_id)?;
        validate_positive_limit("specification_revision", self.revision, u64::MAX)?;
        validate_positive_limit(
            "registration_grant_revision",
            self.grants.registration,
            u64::MAX,
        )?;
        validate_positive_limit(
            "cloud_disclosure_grant_revision",
            self.grants.cloud_disclosure,
            u64::MAX,
        )?;
        validate_positive_limit(
            "autonomous_publication_grant_revision",
            self.grants.autonomous_publication,
            u64::MAX,
        )?;
        if self.manifest_sha256 == [0; 32]
            || self.design_sha256 == [0; 32]
            || self.brainstorming_sha256 == [0; 32]
            || self.owner_approval_sha256 == [0; 32]
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        validate_object("manifest", &self.manifest)?;
        validate_identifier(
            "provider_id",
            &self.provider_id,
            MAX_FEATURE_CONVEYOR_IDENTIFIER_BYTES,
        )?;
        validate_identifier(
            "model_id",
            &self.model_id,
            MAX_FEATURE_CONVEYOR_IDENTIFIER_BYTES,
        )?;
        if self.dependencies.len() > MAX_FEATURE_CONVEYOR_DEPENDENCIES {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        let mut dependencies = HashSet::with_capacity(self.dependencies.len());
        if self.dependencies.iter().any(|dependency| {
            dependency.is_nil()
                || *dependency == self.feature_id
                || !dependencies.insert(*dependency)
        }) {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        let canonical_manifest = canonical_json_bytes(&self.manifest)?;
        if canonical_manifest.len() > MAX_FEATURE_CONVEYOR_APPROVED_MANIFEST_BYTES
            || <[u8; 32]>::from(Sha256::digest(&canonical_manifest)) != self.manifest_sha256
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorApprovedFeatureRequest {
    pub schema_version: u16,
    pub expected_queue_revision: u64,
    pub owner_control_designation_revision: u64,
    pub emergency_pause_revision: u64,
    pub specification: FeatureConveyorApprovedSpecification,
}

impl FeatureConveyorApprovedFeatureRequest {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "feature_conveyor_approved_feature_request",
            frame,
            MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_fixed_value(
            "schema_version",
            self.schema_version.to_string(),
            FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION.to_string(),
        )?;
        validate_positive_limit(
            "owner_control_designation_revision",
            self.owner_control_designation_revision,
            u64::MAX,
        )?;
        self.specification.validate()?;
        validate_serialized_limit(
            "feature_conveyor_approved_feature_request",
            self,
            MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES,
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureConveyorApprovedFeatureStatus {
    Queued,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorApprovedFeatureReceipt {
    pub schema_version: u16,
    pub feature_id: Uuid,
    pub specification_revision: u64,
    pub lifecycle_revision: u64,
    pub queue_revision: u64,
    pub owner_control_designation_revision: u64,
    pub emergency_pause_revision: u64,
    pub status: FeatureConveyorApprovedFeatureStatus,
}

impl AuthenticatedHandshakeRequest {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_and_validate_frame(
            "authenticated_handshake",
            frame,
            MAX_HANDSHAKE_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        self.handshake.validate()?;
        if self.tls_exporter_sha256.iter().all(|byte| *byte == 0) {
            return Err(ProtocolError::InvalidChannelBinding);
        }
        validate_serialized_limit("authenticated_handshake", self, MAX_HANDSHAKE_FRAME_BYTES)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HandshakeStatus {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandshakeResponse {
    pub protocol_version: u16,
    pub status: HandshakeStatus,
    pub connection_epoch: u64,
    pub accepted_registry_revision: u64,
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DistributedEventCursor {
    pub stream_id: Uuid,
    pub sequence: u64,
}

impl DistributedEventCursor {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.stream_id.is_nil() {
            return Err(ProtocolError::NilEventStreamIdentifier);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DistributedEventKind {
    DeviceConnected,
    DeviceDisconnected,
    StepQueued,
    StepLeased,
    StepSucceeded,
    StepFailed,
    StepCancelled,
    StepCancellationRequested,
    StepCancellationAcknowledged,
    StepCancellationExpired,
    StepLeaseExpired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DistributedEvent {
    pub protocol_version: u16,
    pub cursor: DistributedEventCursor,
    pub occurred_at_ms: u64,
    pub kind: DistributedEventKind,
    pub task_id: Option<TaskId>,
    pub step_id: Option<StepId>,
    pub device_id: Option<DeviceId>,
    pub connection_epoch: Option<u64>,
}

impl DistributedEvent {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_version(self.protocol_version)?;
        self.cursor.validate()?;
        validate_positive_limit("occurred_at_ms", self.occurred_at_ms, u64::MAX)?;
        if self.task_id.is_some_and(|value| value.0.is_nil())
            || self.step_id.is_some_and(|value| value.0.is_nil())
            || self.device_id.is_some_and(|value| value.0.is_nil())
            || self.connection_epoch == Some(0)
        {
            return Err(ProtocolError::DistributedEventIdentityMismatch);
        }
        let identity_matches = match self.kind {
            DistributedEventKind::DeviceConnected | DistributedEventKind::DeviceDisconnected => {
                self.task_id.is_none()
                    && self.step_id.is_none()
                    && self.device_id.is_some()
                    && self.connection_epoch.is_some()
            }
            DistributedEventKind::StepQueued => {
                self.task_id.is_some()
                    && self.step_id.is_some()
                    && self.device_id.is_none()
                    && self.connection_epoch.is_none()
            }
            DistributedEventKind::StepLeased
            | DistributedEventKind::StepSucceeded
            | DistributedEventKind::StepFailed
            | DistributedEventKind::StepCancellationRequested
            | DistributedEventKind::StepCancellationAcknowledged
            | DistributedEventKind::StepCancellationExpired
            | DistributedEventKind::StepLeaseExpired => {
                self.task_id.is_some()
                    && self.step_id.is_some()
                    && self.device_id.is_some()
                    && self.connection_epoch.is_some()
            }
            DistributedEventKind::StepCancelled => {
                self.task_id.is_some()
                    && self.step_id.is_some()
                    && self.device_id.is_none()
                    && self.connection_epoch.is_none()
            }
        };
        if !identity_matches {
            return Err(ProtocolError::DistributedEventIdentityMismatch);
        }
        validate_serialized_limit("distributed_event", self, MAX_DISTRIBUTED_EVENT_BATCH_BYTES)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DistributedEventBatchRequest {
    pub protocol_version: u16,
    pub connection_epoch: u64,
    pub after: Option<DistributedEventCursor>,
    pub limit: u16,
}

impl DistributedEventBatchRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_version(self.protocol_version)?;
        validate_positive_limit("connection_epoch", self.connection_epoch, u64::MAX)?;
        validate_positive_limit(
            "limit",
            u64::from(self.limit),
            MAX_DISTRIBUTED_EVENTS_PER_BATCH as u64,
        )?;
        if let Some(after) = self.after {
            after.validate()?;
        }
        validate_serialized_limit(
            "distributed_event_batch_request",
            self,
            MAX_DISTRIBUTED_EVENT_BATCH_BYTES,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DistributedEventBatch {
    pub protocol_version: u16,
    pub stream_id: Uuid,
    pub after_sequence: u64,
    pub next_sequence: u64,
    pub events: Vec<DistributedEvent>,
    pub has_more: bool,
}

impl DistributedEventBatch {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_and_validate_frame(
            "distributed_event_batch",
            frame,
            MAX_DISTRIBUTED_EVENT_BATCH_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_version(self.protocol_version)?;
        if self.stream_id.is_nil() {
            return Err(ProtocolError::NilEventStreamIdentifier);
        }
        if self.events.len() > MAX_DISTRIBUTED_EVENTS_PER_BATCH {
            return Err(ProtocolError::TooManyDistributedEvents {
                maximum: MAX_DISTRIBUTED_EVENTS_PER_BATCH,
            });
        }
        let mut expected = self.after_sequence;
        for event in &self.events {
            event.validate()?;
            expected = expected
                .checked_add(1)
                .ok_or(ProtocolError::EventCursorGap)?;
            if event.cursor.stream_id != self.stream_id || event.cursor.sequence != expected {
                return Err(ProtocolError::EventCursorGap);
            }
        }
        if self.next_sequence != expected {
            return Err(ProtocolError::EventCursorGap);
        }
        validate_serialized_limit(
            "distributed_event_batch",
            self,
            MAX_DISTRIBUTED_EVENT_BATCH_BYTES,
        )
    }
}

impl HandshakeResponse {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_and_validate_frame(
            "handshake_response",
            frame,
            MAX_HANDSHAKE_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_version(self.protocol_version)?;
        if let Some(reason_code) = self.reason_code.as_deref() {
            validate_identifier("reason_code", reason_code, MAX_CAPABILITY_ID_BYTES)?;
        }
        validate_serialized_limit("handshake_response", self, MAX_HANDSHAKE_FRAME_BYTES)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobEnvelope {
    pub protocol_version: u16,
    pub connection_epoch: u64,
    pub sequence: u64,
    pub task_id: TaskId,
    pub step_id: StepId,
    pub attempt_id: AttemptId,
    pub lease_id: LeaseId,
    pub cancellation_id: CancellationId,
    pub capability_id: String,
    pub selected_model: String,
    pub sensitivity: Sensitivity,
    pub context_handling: ContextHandlingPolicy,
    pub lease_duration_ms: u64,
    pub deadline_after_ms: u64,
    pub context_sha256: [u8; 32],
    pub context: Value,
}

impl JobEnvelope {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_and_validate_frame("job", frame, MAX_WIRE_FRAME_BYTES, Self::validate)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_version(self.protocol_version)?;
        validate_uuid("task_id", self.task_id.0)?;
        validate_uuid("step_id", self.step_id.0)?;
        validate_uuid("attempt_id", self.attempt_id.0)?;
        validate_uuid("lease_id", self.lease_id.0)?;
        validate_uuid("cancellation_id", self.cancellation_id.0)?;
        validate_identifier(
            "capability_id",
            &self.capability_id,
            MAX_CAPABILITY_ID_BYTES,
        )?;
        validate_text("selected_model", &self.selected_model, MAX_MODEL_NAME_BYTES)?;
        validate_positive_limit(
            "lease_duration_ms",
            self.lease_duration_ms,
            MAX_LEASE_DURATION_MS,
        )?;
        validate_positive_limit(
            "deadline_after_ms",
            self.deadline_after_ms,
            MAX_STEP_DEADLINE_MS,
        )?;
        validate_object("context", &self.context)?;
        validate_serialized_limit("context", &self.context, MAX_JOB_CONTEXT_BYTES)?;
        validate_payload_digest("context_sha256", &self.context, &self.context_sha256)?;
        validate_serialized_limit("job", self, MAX_WIRE_FRAME_BYTES)
    }

    pub fn validate_fixture_reasoning(&self) -> Result<FixtureJobRequest, ProtocolError> {
        self.validate()?;
        if self.capability_id != FIXTURE_REASONING_CAPABILITY_ID
            || self.selected_model != FIXTURE_REASONING_MODEL
            || self.sensitivity != Sensitivity::Public
            || self.context_handling != ContextHandlingPolicy::EphemeralNoRetention
            || serde_json::to_vec(&self.context)
                .map_err(|error| ProtocolError::Serialization {
                    field: "fixture_context",
                    message: error.to_string(),
                })?
                .len()
                > MAX_FIXTURE_CONTEXT_BYTES
        {
            return Err(ProtocolError::InvalidFixtureJob);
        }
        let request: FixtureJobRequest = serde_json::from_value(self.context.clone())
            .map_err(|_| ProtocolError::InvalidFixtureJob)?;
        request.validate()?;
        Ok(request)
    }

    pub fn validate_mlx_reasoning(&self) -> Result<MlxJobRequest, ProtocolError> {
        self.validate()?;
        if self.capability_id != MLX_REASONING_CAPABILITY_ID
            || self.sensitivity != Sensitivity::Public
            || self.context_handling != ContextHandlingPolicy::EphemeralNoRetention
        {
            return Err(ProtocolError::InvalidMlxJob);
        }
        let request: MlxJobRequest = serde_json::from_value(self.context.clone())
            .map_err(|_| ProtocolError::InvalidMlxJob)?;
        request.validate()?;
        Ok(request)
    }

    pub fn validate_local_coding(&self) -> Result<LocalCodingJobRequest, ProtocolError> {
        self.validate()?;
        if self.capability_id != LOCAL_CODING_CAPABILITY_ID
            || self.selected_model != LOCAL_CODING_MODEL
            || self.sensitivity != Sensitivity::Workspace
            || self.context_handling != ContextHandlingPolicy::SealedUntilResolvedOrExpired
            || serde_json::to_vec(&self.context)
                .map_err(|error| ProtocolError::Serialization {
                    field: "local_coding_context",
                    message: error.to_string(),
                })?
                .len()
                > MAX_LOCAL_CODING_CONTEXT_BYTES
            || serde_json::to_vec(self)
                .map_err(|error| ProtocolError::Serialization {
                    field: "local_coding_job",
                    message: error.to_string(),
                })?
                .len()
                > MAX_LOCAL_CODING_JOB_FRAME_BYTES
        {
            return Err(ProtocolError::InvalidLocalCodingJob);
        }
        let request: LocalCodingJobRequest = serde_json::from_value(self.context.clone())
            .map_err(|_| ProtocolError::InvalidLocalCodingJob)?;
        request
            .validate()
            .map_err(|_| ProtocolError::InvalidLocalCodingJob)?;
        Ok(request)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureJobRequest {
    pub operation: String,
    pub input: String,
    pub delay_ms: u64,
}

impl FixtureJobRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.operation != FIXTURE_SYNTHETIC_ECHO_OPERATION
            || self.input.len() > MAX_FIXTURE_INPUT_BYTES
            || self.delay_ms > MAX_FIXTURE_DELAY_MS
        {
            return Err(ProtocolError::InvalidFixtureJob);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FixtureJobResult {
    pub operation: String,
    pub output: String,
    pub synthetic: bool,
}

impl FixtureJobResult {
    pub fn synthetic_echo(input: String) -> Self {
        Self {
            operation: FIXTURE_SYNTHETIC_ECHO_OPERATION.to_string(),
            output: input,
            synthetic: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MlxJobRequest {
    pub operation: String,
    pub prompt: String,
    pub max_tokens: u32,
    pub temperature_milli: u32,
}

impl MlxJobRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        if self.operation != MLX_GENERATE_TEXT_OPERATION
            || self.prompt.is_empty()
            || self.prompt.len() > MAX_MLX_PROMPT_BYTES
            || self.max_tokens == 0
            || self.max_tokens > MAX_MLX_TOKENS
            || self.temperature_milli > MAX_MLX_TEMPERATURE_MILLI
        {
            return Err(ProtocolError::InvalidMlxJob);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MlxJobResult {
    pub operation: String,
    pub output: String,
    pub model: String,
}

/// Metadata-only receipt for the exact contained-coding fixture. Its mutation
/// exists only inside an ephemeral attempt workspace removed before return.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalCodingJobResult {
    pub status: String,
    pub work_packet_sha256: [u8; 32],
    pub admission_sha256: [u8; 32],
    pub snapshot_sha256: [u8; 32],
    pub allowed_paths_sha256: [u8; 32],
    pub changed_paths_sha256: [u8; 32],
    pub patch_sha256: [u8; 32],
    pub artifact_id: Uuid,
    pub artifact_sha256: [u8; 32],
    pub artifact_size_bytes: u64,
    pub changed_file_count: u16,
    pub test_status: String,
    pub mutation_performed: bool,
    pub workspace_retained: bool,
    pub workspace_expires_at_ms: u64,
    pub ambiguous: bool,
}

/// Protocol-owned canonical multi-file patch. It repeats only the exact
/// deterministic operations admitted by the digest-bound implementation packet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalCodingCanonicalPatchArtifact {
    format: String,
    work_packet_sha256: [u8; 32],
    changes: Vec<LocalCodingEditOperation>,
}

/// Historical protocol-v4 artifact retained only so a schema-v12 master can be
/// migrated and reopened without discarding already-admitted immutable
/// evidence. New admissions never call this validator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HistoricalLocalCodingV4PatchArtifact {
    format: String,
    path: String,
    expected_before_sha256: [u8; 32],
    replacement_sha256: [u8; 32],
    replacement_hex: String,
}

pub fn build_local_coding_patch_artifact(
    packet: &FeatureConveyorCodingWorkPacketMetadata,
) -> Result<Vec<u8>, ProtocolError> {
    let work_packet_sha256 = packet.canonical_sha256()?;
    let document = LocalCodingCanonicalPatchArtifact {
        format: LOCAL_CODING_RESULT_ARTIFACT_FORMAT.to_string(),
        work_packet_sha256,
        changes: packet.operations.clone(),
    };
    let value = serde_json::to_value(&document).map_err(|error| ProtocolError::Serialization {
        field: "local_coding_result_artifact",
        message: error.to_string(),
    })?;
    let bytes = canonical_json_bytes(&value)?;
    validate_local_coding_patch_artifact(&bytes)?;
    Ok(bytes)
}

pub fn validate_local_coding_patch_artifact(bytes: &[u8]) -> Result<[u8; 32], ProtocolError> {
    if bytes.is_empty() || bytes.len() > MAX_LOCAL_CODING_RESULT_ARTIFACT_BYTES {
        return Err(ProtocolError::InvalidLocalCodingResultArtifact);
    }
    let document: LocalCodingCanonicalPatchArtifact =
        decode_strict_json("local_coding_result_artifact", bytes)?;
    if document.format != LOCAL_CODING_RESULT_ARTIFACT_FORMAT
        || document.work_packet_sha256 == [0; 32]
        || document.changes.is_empty()
        || document.changes.len() > MAX_LOCAL_CODING_EDIT_OPERATIONS
        || canonical_json_bytes(&serde_json::to_value(&document).map_err(|error| {
            ProtocolError::Serialization {
                field: "local_coding_result_artifact",
                message: error.to_string(),
            }
        })?)?
            != bytes
    {
        return Err(ProtocolError::InvalidLocalCodingResultArtifact);
    }
    let mut paths = BTreeSet::new();
    let mut replacement_bytes = 0_usize;
    for change in &document.changes {
        change
            .validate()
            .map_err(|_| ProtocolError::InvalidLocalCodingResultArtifact)?;
        if !paths.insert(change.path()) {
            return Err(ProtocolError::InvalidLocalCodingResultArtifact);
        }
        if let LocalCodingEditOperation::Write(arguments) = change {
            replacement_bytes = replacement_bytes
                .checked_add(arguments.replacement_hex.len() / 2)
                .ok_or(ProtocolError::InvalidLocalCodingResultArtifact)?;
        }
    }
    if replacement_bytes > MAX_LOCAL_CODING_EDIT_CONTENT_BYTES {
        return Err(ProtocolError::InvalidLocalCodingResultArtifact);
    }
    Ok(Sha256::digest(bytes).into())
}

pub fn validate_local_coding_patch_artifact_for_packet(
    bytes: &[u8],
    packet: &FeatureConveyorCodingWorkPacketMetadata,
) -> Result<[u8; 32], ProtocolError> {
    let digest = validate_local_coding_patch_artifact(bytes)?;
    let document: LocalCodingCanonicalPatchArtifact =
        decode_strict_json("local_coding_result_artifact", bytes)?;
    if document.work_packet_sha256 != packet.canonical_sha256()?
        || document.changes != packet.operations
    {
        return Err(ProtocolError::InvalidLocalCodingResultArtifact);
    }
    Ok(digest)
}

/// Compatibility helper for the deterministic README live fixture. The
/// resulting bytes use the general v5 multi-file artifact contract.
pub fn build_local_coding_fixture_patch_artifact(
    expected_before_sha256: [u8; 32],
) -> Result<Vec<u8>, ProtocolError> {
    build_local_coding_patch_artifact(&FeatureConveyorCodingWorkPacketMetadata::fixture(
        Uuid::from_u128(1),
        expected_before_sha256,
    ))
}

pub fn validate_local_coding_fixture_patch_artifact(
    bytes: &[u8],
) -> Result<[u8; 32], ProtocolError> {
    validate_local_coding_patch_artifact(bytes)
}

pub fn validate_historical_local_coding_v4_fixture_patch_artifact(
    bytes: &[u8],
) -> Result<[u8; 32], ProtocolError> {
    if bytes.is_empty() || bytes.len() > MAX_LOCAL_CODING_RESULT_ARTIFACT_BYTES {
        return Err(ProtocolError::InvalidLocalCodingResultArtifact);
    }
    let document: HistoricalLocalCodingV4PatchArtifact =
        decode_strict_json("historical_local_coding_v4_result_artifact", bytes)?;
    let replacement = decode_lower_hex(&document.replacement_hex)
        .ok_or(ProtocolError::InvalidLocalCodingResultArtifact)?;
    if document.format != LOCAL_CODING_V4_RESULT_ARTIFACT_FORMAT
        || document.path != LOCAL_CODING_FIXTURE_ALLOWED_PATH
        || document.expected_before_sha256 == [0; 32]
        || replacement != LOCAL_CODING_FIXTURE_CONTENT
        || document.replacement_sha256 != <[u8; 32]>::from(Sha256::digest(&replacement))
        || serde_json::to_vec(&document).map_err(|error| ProtocolError::Serialization {
            field: "historical_local_coding_v4_result_artifact",
            message: error.to_string(),
        })? != bytes
    {
        return Err(ProtocolError::InvalidLocalCodingResultArtifact);
    }
    Ok(Sha256::digest(bytes).into())
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalCodingAgentCompletion {
    pub result: JobResultEnvelope,
    pub artifact: LocalCodingResultArtifact,
}

impl LocalCodingAgentCompletion {
    pub fn validate_for_job(&self, job: &JobEnvelope) -> Result<(), ProtocolError> {
        self.result.validate_local_coding_result(job)?;
        let artifact_bytes = self.artifact.validate()?;
        let result: LocalCodingJobResult = serde_json::from_value(self.result.payload.clone())
            .map_err(|_| ProtocolError::InvalidLocalCodingResult)?;
        let request = job.validate_local_coding()?;
        if result.artifact_id != self.artifact.artifact_id
            || result.patch_sha256 != self.artifact.artifact_sha256
            || result.artifact_sha256 != self.artifact.artifact_sha256
            || result.artifact_size_bytes != self.artifact.artifact_size_bytes
            || validate_local_coding_patch_artifact_for_packet(
                &artifact_bytes,
                &request.work_packet,
            )? != self.artifact.artifact_sha256
        {
            return Err(ProtocolError::InvalidLocalCodingResultArtifact);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalCodingResultArtifact {
    pub artifact_id: Uuid,
    pub artifact_sha256: [u8; 32],
    pub artifact_size_bytes: u64,
    pub artifact_hex: String,
}

impl LocalCodingResultArtifact {
    pub fn from_bytes(artifact_id: Uuid, bytes: &[u8]) -> Result<Self, ProtocolError> {
        let artifact_sha256 = validate_local_coding_patch_artifact(bytes)?;
        Ok(Self {
            artifact_id,
            artifact_sha256,
            artifact_size_bytes: bytes.len() as u64,
            artifact_hex: hex_lower(bytes),
        })
    }

    pub fn validate(&self) -> Result<Vec<u8>, ProtocolError> {
        validate_uuid("artifact_id", self.artifact_id)?;
        let bytes = decode_lower_hex(&self.artifact_hex)
            .ok_or(ProtocolError::InvalidLocalCodingResultArtifact)?;
        if self.artifact_size_bytes == 0
            || self.artifact_size_bytes > MAX_LOCAL_CODING_RESULT_ARTIFACT_BYTES as u64
            || bytes.len() as u64 != self.artifact_size_bytes
            || validate_local_coding_patch_artifact(&bytes)? != self.artifact_sha256
        {
            return Err(ProtocolError::InvalidLocalCodingResultArtifact);
        }
        Ok(bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalCodingResultArtifactAdmission {
    pub protocol_version: u16,
    pub connection_epoch: u64,
    pub sequence: u64,
    pub task_id: TaskId,
    pub step_id: StepId,
    pub attempt_id: AttemptId,
    pub lease_id: LeaseId,
    pub cancellation_id: CancellationId,
    pub context_sha256: [u8; 32],
    pub feature_id: Uuid,
    pub feature_lease_id: Uuid,
    pub snapshot_id: Uuid,
    pub snapshot_sha256: [u8; 32],
    pub work_packet_sha256: [u8; 32],
    pub workspace_retained: bool,
    pub workspace_expires_at_ms: u64,
    pub artifact: LocalCodingResultArtifact,
}

impl LocalCodingResultArtifactAdmission {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_strict_and_validate_frame(
            "local_coding_result_artifact_admission",
            frame,
            MAX_LOCAL_CODING_RESULT_ARTIFACT_FRAME_BYTES,
            Self::validate,
        )
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_version(self.protocol_version)?;
        validate_positive_limit("connection_epoch", self.connection_epoch, u64::MAX)?;
        validate_positive_limit("sequence", self.sequence, u64::MAX)?;
        validate_uuid("task_id", self.task_id.0)?;
        validate_uuid("step_id", self.step_id.0)?;
        validate_uuid("attempt_id", self.attempt_id.0)?;
        validate_uuid("lease_id", self.lease_id.0)?;
        validate_uuid("cancellation_id", self.cancellation_id.0)?;
        validate_uuid("feature_id", self.feature_id)?;
        validate_uuid("feature_lease_id", self.feature_lease_id)?;
        validate_uuid("snapshot_id", self.snapshot_id)?;
        if self.context_sha256 == [0; 32]
            || self.snapshot_sha256 == [0; 32]
            || self.work_packet_sha256 == [0; 32]
            || !self.workspace_retained
            || self.workspace_expires_at_ms == 0
        {
            return Err(ProtocolError::InvalidLocalCodingResultArtifact);
        }
        self.artifact.validate()?;
        validate_serialized_limit(
            "local_coding_result_artifact_admission",
            self,
            MAX_LOCAL_CODING_RESULT_ARTIFACT_FRAME_BYTES,
        )
    }

    pub fn validate_for_job(&self, job: &JobEnvelope) -> Result<Vec<u8>, ProtocolError> {
        self.validate()?;
        let context = job.validate_local_coding()?;
        if self.protocol_version != job.protocol_version
            || self.connection_epoch != job.connection_epoch
            || self.sequence <= job.sequence
            || self.task_id != job.task_id
            || self.step_id != job.step_id
            || self.attempt_id != job.attempt_id
            || self.lease_id != job.lease_id
            || self.cancellation_id != job.cancellation_id
            || self.context_sha256 != job.context_sha256
            || self.feature_id != context.feature_id
            || self.feature_lease_id != context.feature_lease_id
            || self.snapshot_id != context.snapshot_id
            || self.snapshot_sha256 != context.snapshot_sha256
            || self.work_packet_sha256 != context.work_packet_sha256
        {
            return Err(ProtocolError::InvalidLocalCodingResultArtifact);
        }
        let bytes = self.artifact.validate()?;
        if validate_local_coding_patch_artifact_for_packet(&bytes, &context.work_packet)?
            != self.artifact.artifact_sha256
        {
            return Err(ProtocolError::InvalidLocalCodingResultArtifact);
        }
        Ok(bytes)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalCodingResultArtifactReceipt {
    pub protocol_version: u16,
    pub connection_epoch: u64,
    pub sequence: u64,
    pub task_id: TaskId,
    pub step_id: StepId,
    pub attempt_id: AttemptId,
    pub lease_id: LeaseId,
    pub cancellation_id: CancellationId,
    pub artifact_id: Uuid,
    pub artifact_sha256: [u8; 32],
    pub artifact_size_bytes: u64,
    pub workspace_retained: bool,
    pub workspace_expires_at_ms: u64,
    pub status: String,
}

impl LocalCodingResultArtifactReceipt {
    pub fn validate_for_admission(
        &self,
        admission: &LocalCodingResultArtifactAdmission,
    ) -> Result<(), ProtocolError> {
        admission.validate()?;
        if self.protocol_version != admission.protocol_version
            || self.connection_epoch != admission.connection_epoch
            || self.sequence != admission.sequence
            || self.task_id != admission.task_id
            || self.step_id != admission.step_id
            || self.attempt_id != admission.attempt_id
            || self.lease_id != admission.lease_id
            || self.cancellation_id != admission.cancellation_id
            || self.artifact_id != admission.artifact.artifact_id
            || self.artifact_sha256 != admission.artifact.artifact_sha256
            || self.artifact_size_bytes != admission.artifact.artifact_size_bytes
            || self.workspace_retained != admission.workspace_retained
            || self.workspace_expires_at_ms != admission.workspace_expires_at_ms
            || self.status != LOCAL_CODING_RESULT_ARTIFACT_STATUS
        {
            return Err(ProtocolError::InvalidLocalCodingResultArtifact);
        }
        Ok(())
    }
}

pub fn local_coding_fixture_allowed_paths_sha256() -> [u8; 32] {
    local_coding_paths_sha256(&[LOCAL_CODING_FIXTURE_ALLOWED_PATH.to_string()])
}

pub fn local_coding_paths_sha256(paths: &[String]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"assemblywright.local-coding-allowed-paths.v2\0");
    digest.update((paths.len() as u16).to_be_bytes());
    for path in paths {
        digest.update((path.len() as u64).to_be_bytes());
        digest.update(path.as_bytes());
    }
    digest.finalize().into()
}

/// Hashes the exact cross-language admission transcript for one job:
/// domain separator, protocol version as u16 BE, context SHA-256 bytes,
/// task/step/attempt/lease/cancellation UUID bytes in network order, then
/// connection epoch, sequence, lease duration, and deadline as u64 BE.
pub fn local_coding_admission_sha256(job: &JobEnvelope) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"assemblywright.local-coding-admission.v1\0");
    digest.update(job.protocol_version.to_be_bytes());
    digest.update(job.context_sha256);
    digest.update(job.task_id.0.as_bytes());
    digest.update(job.step_id.0.as_bytes());
    digest.update(job.attempt_id.0.as_bytes());
    digest.update(job.lease_id.0.as_bytes());
    digest.update(job.cancellation_id.0.as_bytes());
    digest.update(job.connection_epoch.to_be_bytes());
    digest.update(job.sequence.to_be_bytes());
    digest.update(job.lease_duration_ms.to_be_bytes());
    digest.update(job.deadline_after_ms.to_be_bytes());
    digest.finalize().into()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancellationInstruction {
    pub protocol_version: u16,
    pub connection_epoch: u64,
    pub sequence: u64,
    pub task_id: TaskId,
    pub step_id: StepId,
    pub attempt_id: AttemptId,
    pub lease_id: LeaseId,
    pub cancellation_id: CancellationId,
    pub deadline_after_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancellationPollRequest {
    pub protocol_version: u16,
    pub connection_epoch: u64,
}

impl CancellationPollRequest {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_version(self.protocol_version)?;
        validate_positive_limit("connection_epoch", self.connection_epoch, u64::MAX)?;
        validate_serialized_limit("cancellation_poll_request", self, MAX_WIRE_FRAME_BYTES)
    }
}

impl CancellationInstruction {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_version(self.protocol_version)?;
        validate_positive_limit("connection_epoch", self.connection_epoch, u64::MAX)?;
        validate_positive_limit("sequence", self.sequence, u64::MAX)?;
        validate_uuid("task_id", self.task_id.0)?;
        validate_uuid("step_id", self.step_id.0)?;
        validate_uuid("attempt_id", self.attempt_id.0)?;
        validate_uuid("lease_id", self.lease_id.0)?;
        validate_uuid("cancellation_id", self.cancellation_id.0)?;
        validate_positive_limit(
            "deadline_after_ms",
            self.deadline_after_ms,
            CANCELLATION_ACK_DEADLINE_MS,
        )?;
        validate_serialized_limit("cancellation_instruction", self, MAX_WIRE_FRAME_BYTES)
    }

    pub fn validate_for_job(&self, job: &JobEnvelope) -> Result<(), ProtocolError> {
        self.validate()?;
        job.validate()?;
        if self.protocol_version != job.protocol_version
            || self.connection_epoch != job.connection_epoch
            || self.sequence <= job.sequence
            || self.task_id != job.task_id
            || self.step_id != job.step_id
            || self.attempt_id != job.attempt_id
            || self.lease_id != job.lease_id
            || self.cancellation_id != job.cancellation_id
        {
            return Err(ProtocolError::ResultIdentityMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancellationAcknowledgementStatus {
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancellationAcknowledgement {
    pub protocol_version: u16,
    pub connection_epoch: u64,
    pub sequence: u64,
    pub task_id: TaskId,
    pub step_id: StepId,
    pub attempt_id: AttemptId,
    pub lease_id: LeaseId,
    pub cancellation_id: CancellationId,
    pub status: CancellationAcknowledgementStatus,
}

impl CancellationAcknowledgement {
    pub fn validate_for_instruction(
        &self,
        instruction: &CancellationInstruction,
    ) -> Result<(), ProtocolError> {
        instruction.validate()?;
        validate_version(self.protocol_version)?;
        validate_positive_limit("sequence", self.sequence, u64::MAX)?;
        if self.protocol_version != instruction.protocol_version
            || self.connection_epoch != instruction.connection_epoch
            || self.sequence <= instruction.sequence
            || self.task_id != instruction.task_id
            || self.step_id != instruction.step_id
            || self.attempt_id != instruction.attempt_id
            || self.lease_id != instruction.lease_id
            || self.cancellation_id != instruction.cancellation_id
        {
            return Err(ProtocolError::ResultIdentityMismatch);
        }
        validate_serialized_limit("cancellation_acknowledgement", self, MAX_WIRE_FRAME_BYTES)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobResultStatus {
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobResultEnvelope {
    pub protocol_version: u16,
    pub connection_epoch: u64,
    pub sequence: u64,
    pub task_id: TaskId,
    pub step_id: StepId,
    pub attempt_id: AttemptId,
    pub lease_id: LeaseId,
    pub cancellation_id: CancellationId,
    pub status: JobResultStatus,
    pub context_sha256: [u8; 32],
    pub payload_sha256: [u8; 32],
    pub payload: Value,
}

impl JobResultEnvelope {
    pub fn decode_frame(frame: &[u8]) -> Result<Self, ProtocolError> {
        decode_and_validate_frame("job_result", frame, MAX_WIRE_FRAME_BYTES, Self::validate)
    }

    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_version(self.protocol_version)?;
        validate_uuid("task_id", self.task_id.0)?;
        validate_uuid("step_id", self.step_id.0)?;
        validate_uuid("attempt_id", self.attempt_id.0)?;
        validate_uuid("lease_id", self.lease_id.0)?;
        validate_uuid("cancellation_id", self.cancellation_id.0)?;
        validate_object("payload", &self.payload)?;
        validate_serialized_limit("payload", &self.payload, MAX_JOB_RESULT_BYTES)?;
        validate_payload_digest("payload_sha256", &self.payload, &self.payload_sha256)?;
        validate_serialized_limit("job_result", self, MAX_WIRE_FRAME_BYTES)
    }

    pub fn validate_for_job(&self, job: &JobEnvelope) -> Result<(), ProtocolError> {
        job.validate()?;
        self.validate()?;
        if self.protocol_version != job.protocol_version
            || self.connection_epoch != job.connection_epoch
            || self.sequence <= job.sequence
            || self.task_id != job.task_id
            || self.step_id != job.step_id
            || self.attempt_id != job.attempt_id
            || self.lease_id != job.lease_id
            || self.cancellation_id != job.cancellation_id
            || self.context_sha256 != job.context_sha256
        {
            return Err(ProtocolError::ResultIdentityMismatch);
        }
        Ok(())
    }

    pub fn validate_fixture_reasoning_result(
        &self,
        job: &JobEnvelope,
    ) -> Result<(), ProtocolError> {
        let request = job.validate_fixture_reasoning()?;
        self.validate_for_job(job)?;
        if self.status != JobResultStatus::Completed
            || serde_json::to_vec(&self.payload)
                .map_err(|error| ProtocolError::Serialization {
                    field: "fixture_result",
                    message: error.to_string(),
                })?
                .len()
                > MAX_FIXTURE_RESULT_BYTES
        {
            return Err(ProtocolError::InvalidFixtureJob);
        }
        let result: FixtureJobResult = serde_json::from_value(self.payload.clone())
            .map_err(|_| ProtocolError::InvalidFixtureJob)?;
        if result.operation != FIXTURE_SYNTHETIC_ECHO_OPERATION
            || result.output != request.input
            || !result.synthetic
        {
            return Err(ProtocolError::InvalidFixtureJob);
        }
        Ok(())
    }

    pub fn validate_mlx_reasoning_result(&self, job: &JobEnvelope) -> Result<(), ProtocolError> {
        job.validate_mlx_reasoning()?;
        self.validate_for_job(job)?;
        if self.status != JobResultStatus::Completed {
            return Err(ProtocolError::InvalidMlxResult);
        }
        let result: MlxJobResult = serde_json::from_value(self.payload.clone())
            .map_err(|_| ProtocolError::InvalidMlxResult)?;
        if result.operation != MLX_GENERATE_TEXT_OPERATION
            || result.output.is_empty()
            || result.model != job.selected_model
        {
            return Err(ProtocolError::InvalidMlxResult);
        }
        Ok(())
    }

    pub fn validate_local_coding_result(&self, job: &JobEnvelope) -> Result<(), ProtocolError> {
        let request = job.validate_local_coding()?;
        self.validate_for_job(job)?;
        let result: LocalCodingJobResult = serde_json::from_value(self.payload.clone())
            .map_err(|_| ProtocolError::InvalidLocalCodingResult)?;
        if self.status != JobResultStatus::Completed
            || result.status != LOCAL_CODING_COMPLETED_STATUS
            || result.work_packet_sha256 != request.work_packet_sha256
            || result.admission_sha256 != local_coding_admission_sha256(job)
            || result.snapshot_sha256 != request.snapshot_sha256
            || result.allowed_paths_sha256 != request.work_packet.allowed_paths_sha256()?
            || result.changed_paths_sha256 != result.allowed_paths_sha256
            || result.patch_sha256 == [0; 32]
            || result.artifact_id.is_nil()
            || result.artifact_sha256 != result.patch_sha256
            || result.artifact_size_bytes == 0
            || result.artifact_size_bytes > MAX_LOCAL_CODING_RESULT_ARTIFACT_BYTES as u64
            || result.changed_file_count as usize != request.work_packet.allowed_paths.len()
            || result.test_status != LOCAL_CODING_FIXTURE_TEST_STATUS
            || !result.mutation_performed
            || !result.workspace_retained
            || result.workspace_expires_at_ms == 0
            || result.ambiguous
        {
            return Err(ProtocolError::InvalidLocalCodingResult);
        }
        Ok(())
    }
}

fn validate_version(received: u16) -> Result<(), ProtocolError> {
    if received != PROTOCOL_VERSION {
        return Err(ProtocolError::UnsupportedVersion {
            expected: PROTOCOL_VERSION,
            received,
        });
    }
    Ok(())
}

fn default_true() -> bool {
    true
}

fn validate_assembly_line_schema(received: u16) -> Result<(), ProtocolError> {
    validate_fixed_value(
        "schema_version",
        received.to_string(),
        FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION.to_string(),
    )
}

fn validate_github_name(
    value: &str,
    maximum: usize,
    allow_underscore_and_dot: bool,
) -> Result<(), ProtocolError> {
    if value.is_empty()
        || value.len() > maximum
        || value.starts_with(['-', '.', '_'])
        || value.ends_with(['-', '.', '_'])
        || value.contains("--")
        || !value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || byte == b'-'
                || (allow_underscore_and_dot && matches!(byte, b'_' | b'.'))
        })
    {
        return Err(ProtocolError::InvalidGitHubRepositoryUrl);
    }
    Ok(())
}

fn is_path_or_secret_shaped(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    let compact = lower
        .chars()
        .filter(|character| !character.is_ascii_whitespace())
        .collect::<String>();
    if lower.contains("-----begin ")
        || compact.contains("authorization:bearer")
        || compact.contains("authorization:basic")
        || contains_sensitive_assignment(&lower)
        || contains_prefixed_token_case_insensitive(&lower, "github_pat_", 8)
        || ["ghp_", "gho_", "ghu_", "ghs_", "ghr_"]
            .iter()
            .any(|prefix| contains_prefixed_token_case_insensitive(&lower, prefix, 16))
        || contains_prefixed_token_case_insensitive(&lower, "sk-", 17)
        || contains_aws_access_key_case_insensitive(&lower)
        || contains_embedded_jwt(value)
        || contains_credentialed_url(value)
    {
        return true;
    }
    let path_markers = [
        "/users/",
        "/home/",
        "/.ssh/",
        "~/.ssh",
        "/etc/",
        "/var/",
        "/private/",
        "/tmp/",
        "/opt/",
        "/usr/",
        "/root/",
        "file://",
        "ssh://",
        "git://",
        "git@",
    ];
    path_markers.iter().any(|marker| compact.contains(marker))
        || compact.ends_with("/users")
        || compact.ends_with("/home")
        || compact.contains("\\\\")
        || compact.as_bytes().windows(3).any(|window| {
            window[0].is_ascii_alphabetic()
                && window[1] == b':'
                && matches!(window[2], b'/' | b'\\')
        })
}

fn contains_prefixed_token_case_insensitive(
    lower: &str,
    prefix: &str,
    minimum_suffix: usize,
) -> bool {
    lower.match_indices(prefix).any(|(offset, _)| {
        let has_token_boundary = offset == 0
            || !lower.as_bytes()[offset - 1].is_ascii_alphanumeric()
                && !matches!(lower.as_bytes()[offset - 1], b'_' | b'-');
        if !has_token_boundary {
            return false;
        }
        lower[offset + prefix.len()..]
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
            .count()
            >= minimum_suffix
    })
}

fn contains_sensitive_assignment(lower: &str) -> bool {
    const SENSITIVE_KEYS: [&str; 7] = [
        "password",
        "token",
        "secret",
        "apikey",
        "clientsecret",
        "accesstoken",
        "apitoken",
    ];
    lower.match_indices([':', '=']).any(|(separator, _)| {
        let key_start = lower[..separator]
            .char_indices()
            .rev()
            .take_while(|(_, character)| {
                character.is_ascii_alphanumeric()
                    || character.is_ascii_whitespace()
                    || matches!(character, '_' | '-' | '.')
            })
            .last()
            .map(|(offset, _)| offset)
            .unwrap_or(separator);
        let normalized = lower[key_start..separator]
            .chars()
            .filter(|character| character.is_ascii_alphanumeric())
            .collect::<String>();
        SENSITIVE_KEYS
            .iter()
            .any(|sensitive| normalized.ends_with(sensitive))
    })
}

fn contains_aws_access_key_case_insensitive(lower: &str) -> bool {
    lower.match_indices("akia").any(|(offset, _)| {
        lower.as_bytes()[offset + 4..]
            .iter()
            .take(16)
            .copied()
            .filter(|byte| byte.is_ascii_alphanumeric())
            .count()
            == 16
    })
}

fn contains_embedded_jwt(value: &str) -> bool {
    value.match_indices("eyJ").any(|(offset, _)| {
        let candidate = &value[offset..];
        let mut segments = candidate.splitn(3, '.');
        let (Some(header), Some(payload), Some(signature_and_suffix)) =
            (segments.next(), segments.next(), segments.next())
        else {
            return false;
        };
        if !header
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
            || payload.is_empty()
            || !payload
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            return false;
        }
        let signature_len = signature_and_suffix
            .bytes()
            .take_while(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
            .count();
        signature_len > 0 && header.len() + payload.len() + signature_len + 2 >= 32
    })
}

fn contains_credentialed_url(value: &str) -> bool {
    value.match_indices("://").any(|(offset, _)| {
        value[offset + 3..]
            .split(|character: char| {
                character.is_ascii_whitespace() || matches!(character, '/' | '?' | '#')
            })
            .next()
            .unwrap_or_default()
            .contains('@')
    })
}

fn validate_planning_text(
    field: &'static str,
    value: &str,
    maximum: usize,
) -> Result<(), ProtocolError> {
    validate_text(field, value, maximum)?;
    if value.trim() != value
        || value
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        || is_path_or_secret_shaped(value)
    {
        return Err(ProtocolError::InvalidFullMachineAssemblyLine);
    }
    Ok(())
}

fn canonical_sha256<T: Serialize>(
    field: &'static str,
    value: &T,
    validate: impl FnOnce(&T) -> Result<(), ProtocolError>,
) -> Result<[u8; 32], ProtocolError> {
    validate(value)?;
    let value = serde_json::to_value(value).map_err(|error| ProtocolError::Serialization {
        field,
        message: error.to_string(),
    })?;
    Ok(Sha256::digest(canonical_json_bytes(&value)?).into())
}

fn validate_fixed_value(
    field: &'static str,
    received: String,
    expected: String,
) -> Result<(), ProtocolError> {
    if received != expected {
        return Err(ProtocolError::UnsupportedFixedValue {
            field,
            expected,
            received,
        });
    }
    Ok(())
}

pub fn validate_local_coding_relative_path(path: &str) -> Result<(), ProtocolError> {
    if path.is_empty()
        || path.len() > MAX_LOCAL_CODING_EDIT_PATH_BYTES
        || path.starts_with('/')
        || path.ends_with('/')
        || path.contains('\\')
        || path.contains(':')
        || path.contains('\0')
    {
        return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
    }
    let mut component_count = 0_usize;
    for component in path.split('/') {
        component_count += 1;
        if component.is_empty()
            || component == "."
            || component == ".."
            || component.eq_ignore_ascii_case(".git")
            || component.ends_with('.')
            || component.ends_with(' ')
            || component.chars().any(char::is_control)
            || is_windows_reserved_component(component)
        {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
    }
    if component_count == 0 {
        return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
    }
    Ok(())
}

fn is_windows_reserved_component(component: &str) -> bool {
    let stem = component
        .split('.')
        .next()
        .unwrap_or(component)
        .to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

fn validate_capabilities(capabilities: &[CapabilityDescriptor]) -> Result<(), ProtocolError> {
    if capabilities.len() > MAX_CAPABILITIES_PER_DEVICE {
        return Err(ProtocolError::TooManyCapabilities {
            maximum: MAX_CAPABILITIES_PER_DEVICE,
        });
    }
    let mut ids = HashSet::with_capacity(capabilities.len());
    for capability in capabilities {
        capability.validate()?;
        if !ids.insert(capability.id.as_str()) {
            return Err(ProtocolError::DuplicateCapability(capability.id.clone()));
        }
    }
    Ok(())
}

fn validate_socket_endpoint(
    field: &'static str,
    endpoint: SocketAddr,
) -> Result<(), ProtocolError> {
    if endpoint.port() == 0 || endpoint.ip().is_unspecified() || endpoint.ip().is_multicast() {
        return Err(ProtocolError::InvalidSocketEndpoint { field });
    }
    Ok(())
}

fn validate_sha256_hex(field: &'static str, value: &str) -> Result<(), ProtocolError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(ProtocolError::InvalidSha256Hex { field });
    }
    Ok(())
}

fn validate_uuid(field: &'static str, value: Uuid) -> Result<(), ProtocolError> {
    if value.is_nil() {
        return Err(ProtocolError::NilIdentifier { field });
    }
    Ok(())
}

fn validate_text(field: &'static str, value: &str, maximum: usize) -> Result<(), ProtocolError> {
    if value.is_empty() {
        return Err(ProtocolError::EmptyField { field });
    }
    if value.len() > maximum {
        return Err(ProtocolError::FieldTooLarge { field, maximum });
    }
    Ok(())
}

fn validate_identifier(
    field: &'static str,
    value: &str,
    maximum: usize,
) -> Result<(), ProtocolError> {
    validate_text(field, value, maximum)?;
    if value.trim() != value
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && !byte.is_ascii_control())
    {
        return Err(ProtocolError::InvalidIdentifier { field });
    }
    Ok(())
}

fn validate_local_model_selection_id(value: &str) -> Result<(), ProtocolError> {
    validate_identifier("model_id", value, MAX_MODEL_NAME_BYTES)?;
    if value.starts_with('/')
        || value.contains('\\')
        || value.starts_with("file:")
        || value
            .split('/')
            .any(|component| component == "." || component == "..")
    {
        return Err(ProtocolError::InvalidMlxCapability);
    }
    Ok(())
}

fn is_windows_absolute_path(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'/' | b'\\')
}

fn validate_git_branch(value: &str) -> Result<(), ProtocolError> {
    validate_identifier(
        "expected_base_branch",
        value,
        MAX_FEATURE_CONVEYOR_BASE_BRANCH_BYTES,
    )?;
    if value.starts_with('.')
        || value.starts_with('/')
        || value.ends_with('.')
        || value.ends_with('/')
        || value.contains('/')
        || value.ends_with(".lock")
        || value.contains("..")
        || value.contains("//")
        || value.contains("@{")
        || value
            .bytes()
            .any(|byte| matches!(byte, b' ' | b'~' | b'^' | b':' | b'?' | b'*' | b'[' | b'\\'))
    {
        return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
    }
    Ok(())
}

fn validate_publication_branch(field: &'static str, value: &str) -> Result<(), ProtocolError> {
    validate_identifier(field, value, MAX_FEATURE_CONVEYOR_BASE_BRANCH_BYTES)?;
    if value.starts_with('.')
        || value.starts_with('/')
        || value.ends_with('.')
        || value.ends_with('/')
        || value.ends_with(".lock")
        || value.contains("..")
        || value.contains("//")
        || value.contains("@{")
        || value
            .bytes()
            .any(|byte| matches!(byte, b' ' | b'~' | b'^' | b':' | b'?' | b'*' | b'[' | b'\\'))
    {
        return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
    }
    Ok(())
}

fn validate_git_commit(value: &str) -> Result<(), ProtocolError> {
    if !matches!(value.len(), 40 | 64)
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || value.bytes().all(|byte| byte == b'0')
    {
        return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
    }
    Ok(())
}

fn validate_positive_limit(
    field: &'static str,
    value: u64,
    maximum: u64,
) -> Result<(), ProtocolError> {
    if value == 0 || value > maximum {
        return Err(ProtocolError::InvalidLimit { field, maximum });
    }
    Ok(())
}

fn validate_object(field: &'static str, value: &Value) -> Result<(), ProtocolError> {
    if !value.is_object() {
        return Err(ProtocolError::ExpectedObject { field });
    }
    Ok(())
}

fn validate_serialized_limit<T: Serialize>(
    field: &'static str,
    value: &T,
    maximum: usize,
) -> Result<(), ProtocolError> {
    let bytes = serde_json::to_vec(value).map_err(|error| ProtocolError::Serialization {
        field,
        message: error.to_string(),
    })?;
    if bytes.len() > maximum {
        return Err(ProtocolError::SerializedValueTooLarge { field, maximum });
    }
    Ok(())
}

fn validate_payload_digest<T: Serialize>(
    field: &'static str,
    value: &T,
    expected: &[u8; 32],
) -> Result<(), ProtocolError> {
    let bytes = serde_json::to_vec(value).map_err(|error| ProtocolError::Serialization {
        field,
        message: error.to_string(),
    })?;
    let actual: [u8; 32] = Sha256::digest(bytes).into();
    if &actual != expected {
        return Err(ProtocolError::PayloadDigestMismatch { field });
    }
    Ok(())
}

fn canonical_json_bytes(value: &Value) -> Result<Vec<u8>, ProtocolError> {
    fn write_value(value: &Value, output: &mut Vec<u8>) -> Result<(), ProtocolError> {
        match value {
            Value::Null => output.extend_from_slice(b"null"),
            Value::Bool(value) => output.extend_from_slice(if *value { b"true" } else { b"false" }),
            Value::Number(value) => output.extend_from_slice(value.to_string().as_bytes()),
            Value::String(value) => output.extend_from_slice(
                serde_json::to_string(value)
                    .map_err(|error| ProtocolError::Serialization {
                        field: "manifest",
                        message: error.to_string(),
                    })?
                    .as_bytes(),
            ),
            Value::Array(values) => {
                output.push(b'[');
                for (index, value) in values.iter().enumerate() {
                    if index != 0 {
                        output.push(b',');
                    }
                    write_value(value, output)?;
                }
                output.push(b']');
            }
            Value::Object(values) => {
                output.push(b'{');
                let mut keys = values.keys().collect::<Vec<_>>();
                keys.sort();
                for (index, key) in keys.into_iter().enumerate() {
                    if index != 0 {
                        output.push(b',');
                    }
                    output.extend_from_slice(
                        serde_json::to_string(key)
                            .map_err(|error| ProtocolError::Serialization {
                                field: "manifest",
                                message: error.to_string(),
                            })?
                            .as_bytes(),
                    );
                    output.push(b':');
                    write_value(&values[key], output)?;
                }
                output.push(b'}');
            }
        }
        Ok(())
    }

    let mut output = Vec::new();
    write_value(value, &mut output)?;
    Ok(output)
}

fn decode_and_validate_frame<T>(
    field: &'static str,
    frame: &[u8],
    maximum: usize,
    validate: impl FnOnce(&T) -> Result<(), ProtocolError>,
) -> Result<T, ProtocolError>
where
    T: for<'de> Deserialize<'de>,
{
    if frame.len() > maximum {
        return Err(ProtocolError::FrameTooLarge { field, maximum });
    }
    let value = serde_json::from_slice(frame).map_err(|error| ProtocolError::Deserialization {
        field,
        message: error.to_string(),
    })?;
    validate(&value)?;
    Ok(value)
}

struct StrictJsonValue(Value);

impl<'de> Deserialize<'de> for StrictJsonValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct StrictJsonVisitor;

        impl<'de> serde::de::Visitor<'de> for StrictJsonVisitor {
            type Value = StrictJsonValue;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a JSON value without duplicate object keys")
            }

            fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
                Ok(StrictJsonValue(Value::Bool(value)))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
                Ok(StrictJsonValue(Value::Number(value.into())))
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
                Ok(StrictJsonValue(Value::Number(value.into())))
            }

            fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                serde_json::Number::from_f64(value)
                    .map(Value::Number)
                    .map(StrictJsonValue)
                    .ok_or_else(|| E::custom("non-finite JSON number"))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E> {
                Ok(StrictJsonValue(Value::String(value.to_string())))
            }

            fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
                Ok(StrictJsonValue(Value::String(value)))
            }

            fn visit_none<E>(self) -> Result<Self::Value, E> {
                Ok(StrictJsonValue(Value::Null))
            }

            fn visit_unit<E>(self) -> Result<Self::Value, E> {
                Ok(StrictJsonValue(Value::Null))
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut values = Vec::new();
                while let Some(value) = sequence.next_element::<StrictJsonValue>()? {
                    values.push(value.0);
                }
                Ok(StrictJsonValue(Value::Array(values)))
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut values = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom("duplicate JSON object key"));
                    }
                    values.insert(key, map.next_value::<StrictJsonValue>()?.0);
                }
                Ok(StrictJsonValue(Value::Object(values)))
            }
        }

        deserializer.deserialize_any(StrictJsonVisitor)
    }
}

fn decode_strict_and_validate_frame<T>(
    field: &'static str,
    frame: &[u8],
    maximum: usize,
    validate: impl FnOnce(&T) -> Result<(), ProtocolError>,
) -> Result<T, ProtocolError>
where
    T: for<'de> Deserialize<'de>,
{
    if frame.len() > maximum {
        return Err(ProtocolError::FrameTooLarge { field, maximum });
    }
    let strict = serde_json::from_slice::<StrictJsonValue>(frame).map_err(|error| {
        ProtocolError::Deserialization {
            field,
            message: error.to_string(),
        }
    })?;
    let value =
        serde_json::from_value(strict.0).map_err(|error| ProtocolError::Deserialization {
            field,
            message: error.to_string(),
        })?;
    validate(&value)?;
    Ok(value)
}

fn decode_strict_json<T>(field: &'static str, frame: &[u8]) -> Result<T, ProtocolError>
where
    T: for<'de> Deserialize<'de>,
{
    let strict = serde_json::from_slice::<StrictJsonValue>(frame).map_err(|error| {
        ProtocolError::Deserialization {
            field,
            message: error.to_string(),
        }
    })?;
    serde_json::from_value(strict.0).map_err(|error| ProtocolError::Deserialization {
        field,
        message: error.to_string(),
    })
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn decode_lower_hex(value: &str) -> Option<Vec<u8>> {
    if value.len() % 2 != 0
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return None;
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let high = match pair[0] {
                b'0'..=b'9' => pair[0] - b'0',
                b'a'..=b'f' => pair[0] - b'a' + 10,
                _ => return None,
            };
            let low = match pair[1] {
                b'0'..=b'9' => pair[1] - b'0',
                b'a'..=b'f' => pair[1] - b'a' + 10,
                _ => return None,
            };
            Some((high << 4) | low)
        })
        .collect()
}
