use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::net::SocketAddr;
use uuid::Uuid;

pub const PROTOCOL_VERSION: u16 = 2;
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
pub const MAX_FEATURE_CONVEYOR_CODING_DISPATCH_REQUEST_BYTES: usize = 8 * 1024;
pub const MAX_FEATURE_CONVEYOR_REPOSITORY_PATH_BYTES: usize = 4 * 1024;
pub const MAX_FEATURE_CONVEYOR_BASE_BRANCH_BYTES: usize = 255;
pub const LOCAL_CODING_CAPABILITY_ID: &str = "local.coding.v1";
pub const LOCAL_CODING_PROVIDER: &str = "assemblywright-agent";
pub const LOCAL_CODING_MODEL: &str = "assemblywright-local-coding-v1";
pub const MAX_LOCAL_CODING_CONTEXT_BYTES: usize = 8 * 1024;
pub const MAX_LOCAL_CODING_RESULT_BYTES: usize = 32 * 1024;
pub const MAX_LOCAL_CODING_SNAPSHOT_CHUNK_BYTES: usize = 128 * 1024;
pub const MAX_LOCAL_CODING_SNAPSHOT_BUNDLE_BYTES: u64 = 320 * 1024 * 1024;
pub const MAX_LOCAL_CODING_SNAPSHOT_CHUNK_FRAME_BYTES: usize = 384 * 1024;

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

/// Bounded, path-free metadata for one owner-approved coding work packet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureConveyorCodingWorkPacketMetadata {
    pub packet_id: Uuid,
    pub ordinal: u16,
    pub acceptance_criteria_count: u16,
}

impl FeatureConveyorCodingWorkPacketMetadata {
    pub fn validate(&self) -> Result<(), ProtocolError> {
        validate_uuid("packet_id", self.packet_id)?;
        if self.ordinal == 0 || self.acceptance_criteria_count == 0 {
            return Err(ProtocolError::InvalidFeatureConveyorOwnerControl);
        }
        Ok(())
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
        if self.snapshot_sha256 == [0; 32] || self.work_packet_sha256 == [0; 32] {
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
        let request = FeatureConveyorCodingDispatchRequest {
            schema_version: self.schema_version,
            feature_id: self.feature_id,
            specification_revision: self.specification_revision,
            expected_lifecycle_revision: self.lifecycle_revision,
            feature_lease_id: self.feature_lease_id,
            snapshot_id: self.snapshot_id,
            snapshot_sha256: self.snapshot_sha256,
            work_packet_sha256: self.work_packet_sha256,
            work_packet: FeatureConveyorCodingWorkPacketMetadata {
                packet_id: self.packet_id,
                ordinal: 1,
                acceptance_criteria_count: 1,
            },
            device_id: self.device_id,
            device_registry_revision: self.device_registry_revision,
            expected_queue_revision: self.queue_revision,
            expected_emergency_pause_revision: self.emergency_pause_revision,
        };
        request.validate()?;
        validate_uuid("task_id", self.task_id.0)?;
        validate_uuid("step_id", self.step_id.0)
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
            || self.context_handling != ContextHandlingPolicy::EphemeralNoRetention
            || serde_json::to_vec(&self.context)
                .map_err(|error| ProtocolError::Serialization {
                    field: "local_coding_context",
                    message: error.to_string(),
                })?
                .len()
                > MAX_LOCAL_CODING_CONTEXT_BYTES
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

/// Metadata-only receipt for an exact snapshot materialization proof. The
/// transferred repository remains ephemeral and no mutation is permitted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalCodingJobResult {
    pub status: String,
    pub work_packet_sha256: [u8; 32],
    pub admission_sha256: [u8; 32],
    pub snapshot_sha256: [u8; 32],
    pub mutation_performed: bool,
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
            || result.status != "snapshot_materialized"
            || result.work_packet_sha256 != request.work_packet_sha256
            || result.admission_sha256 == [0; 32]
            || result.snapshot_sha256 != request.snapshot_sha256
            || result.mutation_performed
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
