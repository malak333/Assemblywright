use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::net::SocketAddr;
use uuid::Uuid;

pub const PROTOCOL_VERSION: u16 = 1;
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

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
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
        if self.role != DeviceRole::MacBridge {
            return Err(ProtocolError::UnsupportedFixedValue {
                field: "role",
                expected: "mac_bridge".to_string(),
                received: "inference_worker".to_string(),
            });
        }
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
        )
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
/// The digest is SHA-256 over 32 bytes exported with the fixed Jarvis exporter
/// label. Keeping this value inside the bounded application handshake prevents
/// a valid device handshake from being replayed on another TLS connection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthenticatedHandshakeRequest {
    pub handshake: HandshakeRequest,
    pub tls_exporter_sha256: [u8; 32],
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
            | DistributedEventKind::StepFailed => {
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
