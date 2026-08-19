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
