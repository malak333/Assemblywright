pub mod ipc;
pub mod memory_index;
pub mod model;
pub mod plugin;
pub mod policy;
pub mod router;
pub mod runtime;
pub mod scheduler;
pub mod startup;
pub mod storage;
pub mod trusted_wake;
pub mod types;
pub mod wasm_plugin;
pub mod workspace;

pub use ipc::{
    router, router_with_auth, serve, serve_listener, serve_listener_with_auth, serve_with_auth,
    ApprovalDecisionRequest, ApprovalExecutionResponse, ApprovalStatusCount, CommandRequest,
    CommandResponse, CreateMemoryItemRequest, CreateSchedulerJobRequest, EmergencyPauseRequest,
    EmergencyPauseResponse, ErrorResponse, HealthResponse, InstallPluginRequest,
    InstalledPluginExecutionRequest, InstalledPluginGrantSurface,
    InstalledPluginPublisherSignatureVerificationRequest,
    InstalledPluginPublisherVerificationRequest, InstalledPluginRunRequest,
    InstalledPluginRunResponse, IpcAuth, IpcState, ModelToolCatalogEntry, ModelToolCatalogResponse,
    ModelToolConstraints, PermissionGrantSummary, PermissionPolicyReview,
    PermissionPolicyReviewItem, ReleaseReadinessFeature, ReleaseReadinessResponse,
    RuntimeCancellationResponse, SchedulerAttentionItem, SchedulerAttentionSummary,
    SchedulerBackgroundConfig, SchedulerJobExecution, SchedulerRunResponse,
    SchedulerStaleRecoveryItem, SchedulerStaleRecoveryResponse, UpdateMemoryItemRequest,
    DEFAULT_ACTIVITY_EVENT_INTERVAL_MS, DEFAULT_ACTIVITY_EVENT_LIMIT,
    DEFAULT_SCHEDULER_BACKGROUND_INTERVAL_MS, DEFAULT_SCHEDULER_BACKGROUND_LIMIT,
    DEFAULT_SCHEDULER_STALE_RECOVERY_LIMIT, DEFAULT_SCHEDULER_STALE_RECOVERY_OLDER_THAN_SECONDS,
    IPC_BEARER_TOKEN_BYTES, IPC_BEARER_TOKEN_LENGTH, MAX_ACTIVITY_EVENT_LIMIT,
    MAX_SCHEDULER_BACKGROUND_LIMIT,
};
pub use memory_index::{
    MemoryIndexState, MemoryIndexStatus, MemoryIndexStore, MEMORY_INDEX_VERSION,
};
pub use model::{
    redact_url_credentials, ChatGptAuthMode, ChatGptHttpModel, ChatGptProviderConfig,
    FakeLocalModel, LocalModelConfig, LocalModelExecutor, LocalModelProviderKind, ModelExecutor,
    ModelProvider, ModelRequest, ModelResponse, ModelRoute, ModelToolDefinition, ModelToolRequest,
    ModelToolResult, OllamaHttpModel, ProviderConfig, ProviderStatus, RoutedModelExecutor,
};
pub use plugin::{
    execute_installed_subprocess_plugin, plugin_permission_scopes, CancellationBehavior,
    CancellationSignal, InProcessPlugin, InstalledPlugin, InstalledPluginExecutionGrant,
    InstalledPluginIntegrityStatus, InstalledPluginProvenance, JsonSchema, PluginAccess,
    PluginActionManifest, PluginCallMetadata, PluginCallRequest, PluginCallResult,
    PluginCallStatus, PluginHost, PluginManifest, PluginNetworkAccess, PluginNetworkAccessMode,
    PluginPermission, PluginProgressEvent, PluginPublisherSignature, PluginSource,
    PluginSubprocessManifest, PluginSubprocessStream, PluginTimeout, PluginTimeoutAction,
    PluginWasmAbi, PluginWasmManifest, StatusPlugin, SubprocessPluginExecution,
};
pub use policy::{
    ApprovalDecision, ApprovalGrant, CapabilityScope, PermissionEngine, PolicyDecision,
    PolicyRequest,
};
pub use router::{
    redact_for_chatgpt, ModelProvider as RoutedModelProvider, ModelRouteRecord, ModelRouteRequest,
    ModelRouter, RouteEvidence, RouteOutcome,
};
pub use runtime::{
    CommandRequest as RuntimeCommandRequest, CommandResponse as RuntimeCommandResponse,
    ConversationRuntime, NoopRuntimeCommandStore, NoopRuntimeHooks, RuntimeCommandStore,
    RuntimeConfig, RuntimeControl, RuntimeHooks, RuntimeStep,
};
pub use scheduler::{Scheduler, SchedulerJob, SchedulerJobSpec, SchedulerJobStatus, TriggerKind};
pub use startup::{
    ServeStartupConfig, TrustedWakeStartupDocument, MAX_SERVE_STARTUP_CONFIG_BYTES,
    SERVE_STARTUP_CONFIG_VERSION,
};
pub use storage::{
    EmergencyPauseState, InstalledPluginRecord, MemoryItem, NewMemoryItem, NewPendingApproval,
    PendingApproval, SqliteRepository,
};
pub use storage::{MemoryClassificationCount, MemoryClassificationSummary};
pub use trusted_wake::{
    TrustedWakeAcceptedEvent, TrustedWakeAttentionItem, TrustedWakeDispatchState,
    TrustedWakeEnvelope, TrustedWakeKeyControlCancelRequest, TrustedWakeKeyControlInstallDocument,
    TrustedWakeKeyControlMode, TrustedWakeKeyControlPrepareRequest,
    TrustedWakeKeyControlPrepareResponse, TrustedWakeKeyControlProof,
    TrustedWakeKeyControlProofPayload, TrustedWakePayload, TrustedWakePendingKeyControl,
    TrustedWakeResolutionRequest, TrustedWakeRule, TrustedWakeRuleEnablement,
    TrustedWakeRuleEnrollment, TrustedWakeSessionStatus, TRUSTED_WAKE_CANCEL_CONFIRMATION,
    TRUSTED_WAKE_KEY_CONTROL_DOMAIN, TRUSTED_WAKE_RECOVER_CONFIRMATION,
    TRUSTED_WAKE_ROTATE_CONFIRMATION, TRUSTED_WAKE_RULE_ID, TRUSTED_WAKE_SCHEMA_VERSION,
};
pub use types::{
    ApprovalStatus, AuditEntry, JarvisError, JarvisResult, RiskTier, Sensitivity, TaskRecord,
    TaskStatus,
};
pub use wasm_plugin::{
    execute_installed_wasm_plugin, read_wasm_artifact, WasmArtifact, WasmControlState,
    WasmPluginExecution, MAX_WASM_FUEL, MAX_WASM_MEMORY_BYTES, MAX_WASM_MODULE_BYTES,
    MAX_WASM_OUTPUT_BYTES, MAX_WASM_REQUEST_BYTES, MAX_WASM_TABLE_ELEMENTS,
};
pub use workspace::{
    WorkspaceInspectPlugin, WorkspaceRootConfig, MAX_WORKSPACE_LIST_ENTRIES,
    MAX_WORKSPACE_RELATIVE_PATH_BYTES, MAX_WORKSPACE_ROOTS, MAX_WORKSPACE_ROOT_ID_BYTES,
    MAX_WORKSPACE_ROOT_PATH_BYTES, MAX_WORKSPACE_TEXT_BYTES,
};
