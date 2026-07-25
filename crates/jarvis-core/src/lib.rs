//! Shared local foundation for Assemblywright.
//!
//! This crate owns the hardened app-supervised Unix-domain-socket transport,
//! its peer code-identity and startup contracts, and read-only release
//! readiness/evidence inspection. It holds no conversation, model, tool,
//! memory, scheduler, plugin, or repository authority.

pub mod ipc_transport;
#[cfg(target_os = "macos")]
mod macos_code_identity;
pub mod release;
pub mod startup;
pub mod types;

#[cfg(feature = "distributed-development")]
pub use jarvis_protocol as distributed_protocol;

pub use ipc_transport::{
    serve_router_unix_socket_with_peer_identity, MAX_UNIX_IPC_CONNECTIONS,
    MAX_UNIX_IPC_PATH_AND_QUERY_BYTES, MAX_UNIX_IPC_REQUEST_BODY_BYTES,
    MAX_UNIX_IPC_REQUEST_FRAME_BYTES, MAX_UNIX_IPC_REQUEST_HEADER_VALUE_BYTES,
    MAX_UNIX_IPC_RESPONSE_BODY_BYTES, MAX_UNIX_IPC_RESPONSE_CONTENT_TYPE_BYTES,
    MAX_UNIX_IPC_RESPONSE_FRAME_BYTES, UNIX_IPC_DISPATCH_TIMEOUT_SECONDS, UNIX_IPC_FRAME_VERSION,
    UNIX_IPC_PEER_IDENTITY_TIMEOUT_SECONDS, UNIX_IPC_READ_TIMEOUT_SECONDS,
    UNIX_IPC_WRITE_TIMEOUT_SECONDS,
};
pub use release::{
    release_evidence_bundle_runbook, release_evidence_status, release_live_device_runbook,
    release_plugin_trust_runbook, release_readiness, release_signed_distribution_runbook,
    ReleaseEvidenceItemStatus, ReleaseEvidenceKind, ReleaseEvidenceStatusItem,
    ReleaseEvidenceStatusResponse, ReleaseReadinessFeature, ReleaseReadinessResponse,
    ReleaseRunbookResponse,
};
pub use startup::{
    validate_peer_code_requirement, validate_unix_socket_path, PeerIdentityProfile,
    MAX_PEER_CODE_REQUIREMENT_BYTES, MAX_UNIX_SOCKET_PATH_BYTES,
};
pub use types::{JarvisError, JarvisResult};

pub const IPC_BEARER_TOKEN_LENGTH: usize = 43;
pub const IPC_BEARER_TOKEN_BYTES: usize = 32;
