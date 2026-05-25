use std::collections::HashMap;
use std::convert::Infallible;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path as FsPath, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::net::TcpListener;
use tokio_stream::wrappers::IntervalStream;
use tower_http::trace::TraceLayer;
use uuid::Uuid;

use futures_util::StreamExt as FuturesStreamExt;

use crate::model::{model_tool_definitions_from_manifests, ModelToolDefinition};
use crate::storage::{
    EmergencyPauseState as StoredEmergencyPauseState, MemoryClassificationSummary, NewMemoryItem,
    NewPendingApproval, PendingApproval, SqliteRepository,
};
use crate::{
    execute_installed_subprocess_plugin, plugin_permission_scopes, ApprovalDecision, ApprovalGrant,
    ApprovalStatus, AuditEntry, CapabilityScope, ConversationRuntime, InstalledPlugin,
    InstalledPluginExecutionGrant, InstalledPluginIntegrityStatus, InstalledPluginProvenance,
    InstalledPluginRecord, JarvisError, JarvisResult, LocalModelProviderKind, ModelRoute,
    ModelRouteRecord, PermissionEngine, PluginCallRequest, PluginCallResult, PluginCallStatus,
    PluginHost, PluginManifest, PluginSource, PolicyRequest, ProviderConfig, RoutedModelExecutor,
    RuntimeCommandRequest, RuntimeCommandStore, RuntimeConfig, RuntimeControl, RuntimeStep,
    Scheduler, SchedulerJob, SchedulerJobSpec, SchedulerJobStatus, Sensitivity, TaskRecord,
    TaskStatus, TriggerKind,
};

pub const IPC_CONTRACT_VERSION: u16 = 1;
pub const IPC_CONTRACT_NAME: &str = "jarvis.local-ipc";
pub const DEFAULT_SCHEDULER_BACKGROUND_INTERVAL_MS: u64 = 30_000;
pub const DEFAULT_SCHEDULER_BACKGROUND_LIMIT: usize = 16;
pub const DEFAULT_SCHEDULER_STALE_RECOVERY_OLDER_THAN_SECONDS: u64 = 3_600;
pub const DEFAULT_SCHEDULER_STALE_RECOVERY_LIMIT: usize = 16;
pub const MAX_SCHEDULER_BACKGROUND_LIMIT: usize = 64;
pub const DEFAULT_ACTIVITY_EVENT_INTERVAL_MS: u64 = 1_000;
pub const DEFAULT_ACTIVITY_EVENT_LIMIT: usize = 3;
pub const MAX_ACTIVITY_EVENT_LIMIT: usize = 50;
const LIVE_DEVICE_QA_REQUIRED_FIELDS: &[&str] = &[
    "schema_version",
    "evidence_type",
    "generated_at",
    "installed_app_path",
    "validation_flags.clean_profile",
    "validation_flags.finder_launch",
    "validation_flags.microphone",
    "validation_flags.speech_permission",
    "validation_flags.transcript_handoff",
    "validation_flags.audio_output",
    "validation_flags.notification",
    "validation_flags.restart",
    "validation_flags.manual_release_qa",
    "voice_loop.microphone_permission_prompt",
    "voice_loop.speech_permission_prompt",
    "voice_loop.spoken_transcript_handoff",
    "voice_loop.same_command_path",
    "voice_loop.speech_output_playback",
    "app_bundle.bundle_identifier",
    "app_bundle.short_version",
    "app_bundle.build_version",
    "app_bundle.microphone_usage_description",
    "app_bundle.speech_recognition_usage_description",
    "owner_recorded_live_voice_evidence.owner_name",
    "owner_recorded_live_voice_evidence.device_label",
    "owner_recorded_live_voice_evidence.profile_label",
    "owner_recorded_live_voice_evidence.voice_check_started_at",
    "owner_recorded_live_voice_evidence.voice_check_completed_at",
    "owner_recorded_live_voice_evidence.microphone_evidence_note",
    "owner_recorded_live_voice_evidence.speech_permission_evidence_note",
    "owner_recorded_live_voice_evidence.transcript_handoff_evidence_note",
    "owner_recorded_live_voice_evidence.audio_output_evidence_note",
    "voice_command_observation.test_phrase",
    "voice_command_observation.observed_transcript",
    "voice_command_observation.expected_command_text",
    "voice_command_observation.observed_command_text",
    "voice_command_observation.command_result_evidence_id",
    "voice_command_observation.audio_output_device_label",
    "proof_boundary",
];
const PLUGIN_TRUST_QA_REQUIRED_FIELDS: &[&str] = &[
    "schema_version",
    "evidence_type",
    "generated_at",
    "review_source",
    "validation_flags.marketplace_review",
    "validation_flags.malware_scan",
    "validation_flags.os_sandbox",
    "validation_flags.egress_enforcement",
    "validation_flags.signed_publisher_policy",
    "validation_flags.manual_trust_review",
    "owner_recorded_plugin_trust_evidence.owner_name",
    "owner_recorded_plugin_trust_evidence.review_started_at",
    "owner_recorded_plugin_trust_evidence.review_completed_at",
    "owner_recorded_plugin_trust_evidence.marketplace_evidence_note",
    "owner_recorded_plugin_trust_evidence.malware_scan_evidence_note",
    "owner_recorded_plugin_trust_evidence.os_sandbox_evidence_note",
    "owner_recorded_plugin_trust_evidence.egress_evidence_note",
    "owner_recorded_plugin_trust_evidence.egress_policy_label",
    "owner_recorded_plugin_trust_evidence.egress_validation_completed_at",
    "owner_recorded_plugin_trust_evidence.egress_deny_fixture_evidence_note",
    "owner_recorded_plugin_trust_evidence.egress_allow_fixture_evidence_note",
    "owner_recorded_plugin_trust_evidence.signed_publisher_evidence_note",
    "owner_recorded_plugin_trust_evidence.manual_review_evidence_note",
    "proof_boundary",
];
const SIGNED_DISTRIBUTION_PROVENANCE_REQUIRED_FIELDS: &[&str] = &[
    "schema_version",
    "evidence_type",
    "generated_at",
    "version",
    "bundle_identifier",
    "artifacts.app_path",
    "artifacts.zip_path",
    "artifacts.pkg_path",
    "artifacts.zip_sha256",
    "artifacts.pkg_sha256",
    "artifacts.bundled_core_version",
    "signing.developer_id_application_identity",
    "signing.developer_id_installer_identity",
    "signing.app_bundle_codesign",
    "signing.app_executable_codesign",
    "signing.bundled_core_codesign",
    "signing.installer_pkg_signature",
    "notarization.app_zip_submission_id",
    "notarization.installer_pkg_submission_id",
    "notarization.app_zip_notary_log",
    "notarization.installer_pkg_notary_log",
    "stapling.app_bundle_validation",
    "stapling.installer_pkg_validation",
    "gatekeeper.app_bundle_assessment",
    "gatekeeper.installer_pkg_assessment",
    "validation_flags.developer_id_application_signed",
    "validation_flags.developer_id_installer_signed",
    "validation_flags.app_zip_notarized",
    "validation_flags.installer_pkg_notarized",
    "validation_flags.app_stapled",
    "validation_flags.installer_pkg_stapled",
    "validation_flags.gatekeeper_assessed",
    "validation_flags.artifact_digests_recorded",
    "proof_boundary",
];
const RELEASE_EVIDENCE_BUNDLE_REQUIRED_FIELDS: &[&str] = &[
    "schema_version",
    "evidence_type",
    "generated_at",
    "version",
    "artifacts.app_path",
    "artifacts.zip_path",
    "artifacts.pkg_path",
    "artifacts.zip_sha256",
    "artifacts.pkg_sha256",
    "reports.signed_distribution_provenance_report",
    "reports.live_device_qa_report",
    "reports.plugin_trust_qa_report",
    "reports.signed_distribution_provenance_sha256",
    "reports.live_device_qa_sha256",
    "reports.plugin_trust_qa_sha256",
    "validation_flags.signed_distribution",
    "validation_flags.notarization",
    "validation_flags.clean_profile",
    "validation_flags.live_device_qa",
    "validation_flags.plugin_trust_qa",
    "validation_flags.reports_archived",
    "validation_flags.local_signature_validation",
    "proof_boundary",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractMetadata {
    pub name: String,
    pub version: u16,
    pub core_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractEndpoint {
    pub method: String,
    pub path: String,
    pub repository_required: bool,
    pub redacted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractFeature {
    pub key: String,
    pub status: String,
    pub proof: String,
    pub boundary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractCompatibility {
    pub minimum_supported_version: u16,
    pub current_version: u16,
    pub additive_changes_allowed: bool,
    pub breaking_change_policy: String,
    pub deprecation_policy: String,
    pub client_requirements: Vec<String>,
    pub removed_endpoints: Vec<String>,
    pub deprecated_endpoints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractResponse {
    pub contract: ContractMetadata,
    pub compatibility: ContractCompatibility,
    pub endpoints: Vec<ContractEndpoint>,
    pub safe_inspection_paths: Vec<String>,
    pub features: Vec<ContractFeature>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelToolCatalogResponse {
    pub generated_at: DateTime<Utc>,
    pub source: String,
    pub tools: Vec<ModelToolDefinition>,
    pub proof_boundary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseReadinessResponse {
    pub generated_at: DateTime<Utc>,
    pub production_ready: bool,
    pub readiness_scope: String,
    pub verified_feature_count: usize,
    pub pending_feature_count: usize,
    pub implemented_features: Vec<ReleaseReadinessFeature>,
    pub pending_features: Vec<ReleaseReadinessFeature>,
    pub blocking_manual_gates: Vec<String>,
    pub recommended_verification_commands: Vec<String>,
    pub proof_boundary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseReadinessFeature {
    pub key: String,
    pub status: String,
    pub proof: String,
    pub boundary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseEvidenceStatusResponse {
    pub generated_at: DateTime<Utc>,
    pub complete: bool,
    pub satisfied_count: usize,
    pub missing_count: usize,
    pub invalid_count: usize,
    pub items: Vec<ReleaseEvidenceStatusItem>,
    pub proof_boundary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseEvidenceStatusItem {
    pub key: String,
    pub label: String,
    pub path: String,
    pub kind: ReleaseEvidenceKind,
    pub status: ReleaseEvidenceItemStatus,
    pub required_for_production: bool,
    pub manual_gate: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseEvidenceKind {
    Directory,
    File,
    Executable,
    JsonReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReleaseEvidenceItemStatus {
    Present,
    Missing,
    Invalid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub contract: ContractMetadata,
    pub started_at: DateTime<Utc>,
    pub emergency_paused: bool,
    pub emergency_pause_reason: Option<String>,
    pub emergency_pause_updated_at: Option<DateTime<Utc>>,
    pub scheduler_jobs: usize,
    pub command_runtime: String,
    pub local_model_provider: LocalModelProviderKind,
    pub local_model: String,
    pub local_endpoint_configured: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticsExport {
    pub generated_at: DateTime<Utc>,
    pub redaction: String,
    pub health: HealthResponse,
    pub scheduler_jobs: Vec<DiagnosticSchedulerJob>,
    pub repository_backed: bool,
    pub schema_version: Option<i64>,
    pub task_count: Option<usize>,
    pub audit_entry_count: Option<usize>,
    pub model_route_record_count: Option<usize>,
    pub active_memory_item_count: Option<usize>,
    pub unreviewed_memory_item_count: Option<usize>,
    pub sensitive_memory_item_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticSchedulerJob {
    pub id: Uuid,
    pub name: String,
    pub trigger: TriggerKind,
    pub status: SchedulerJobStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub cancelled_at: Option<DateTime<Utc>>,
    pub cancellation_reason_present: bool,
}

impl From<SchedulerJob> for DiagnosticSchedulerJob {
    fn from(job: SchedulerJob) -> Self {
        Self {
            id: job.id,
            name: job.name,
            trigger: job.trigger,
            status: job.status,
            created_at: job.created_at,
            updated_at: job.updated_at,
            cancelled_at: job.cancelled_at,
            cancellation_reason_present: job.cancellation_reason.is_some(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SchedulerBackgroundConfig {
    pub interval: StdDuration,
    pub limit: usize,
}

impl SchedulerBackgroundConfig {
    pub fn new(interval: StdDuration, limit: usize) -> JarvisResult<Self> {
        if interval.is_zero() {
            return Err(JarvisError::Validation(
                "scheduler background interval must be greater than zero".to_string(),
            ));
        }

        Ok(Self {
            interval,
            limit: limit.clamp(1, MAX_SCHEDULER_BACKGROUND_LIMIT),
        })
    }
}

impl Default for SchedulerBackgroundConfig {
    fn default() -> Self {
        Self {
            interval: StdDuration::from_millis(DEFAULT_SCHEDULER_BACKGROUND_INTERVAL_MS),
            limit: DEFAULT_SCHEDULER_BACKGROUND_LIMIT,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandRequest {
    pub input: String,
    #[serde(default)]
    pub session_id: Option<Uuid>,
    #[serde(default)]
    pub context: serde_json::Value,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub proactive: bool,
    #[serde(default)]
    pub sensitivity: Option<Sensitivity>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandResponse {
    pub accepted: bool,
    pub task: TaskRecord,
    pub audit_entry: AuditEntry,
    pub audit_entries: Vec<AuditEntry>,
    pub route: Option<ModelRoute>,
    pub route_evidence: Option<ModelRouteRecord>,
    pub steps: Vec<RuntimeStep>,
    pub plugin_results: Vec<PluginCallResult>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalExecutionResponse {
    pub accepted: bool,
    pub approval: PendingApproval,
    pub task: TaskRecord,
    pub audit_entry: AuditEntry,
    pub audit_entries: Vec<AuditEntry>,
    pub plugin_results: Vec<PluginCallResult>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityStatusCount {
    pub status: TaskStatus,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityTaskSummary {
    pub id: Uuid,
    pub session_id: Uuid,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<&TaskRecord> for ActivityTaskSummary {
    fn from(task: &TaskRecord) -> Self {
        Self {
            id: task.id,
            session_id: task.session_id,
            status: task.status.clone(),
            created_at: task.created_at,
            updated_at: task.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivitySummary {
    pub generated_at: DateTime<Utc>,
    pub repository_backed: bool,
    pub task_count: usize,
    pub audit_entry_count: usize,
    pub active_task_count: usize,
    pub status_counts: Vec<ActivityStatusCount>,
    pub recent_tasks: Vec<ActivityTaskSummary>,
    pub recent_audit_entries: Vec<AuditEntry>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActivityEventsQuery {
    #[serde(default)]
    pub interval_ms: Option<u64>,
    #[serde(default)]
    pub max_events: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActivityProgressEvent {
    pub audit_id: Uuid,
    #[serde(default)]
    pub task_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub plugin_id: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub session_id: Option<Uuid>,
    #[serde(default)]
    pub sequence: Option<u64>,
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
    pub stderr_redacted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalDecisionRequest {
    #[serde(default = "default_decided_by")]
    pub decided_by: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergencyPauseRequest {
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergencyPauseResponse {
    pub paused: bool,
    pub reason: Option<String>,
    pub paused_at: Option<DateTime<Utc>>,
    pub resumed_at: Option<DateTime<Utc>>,
    pub cancelled_scheduler_jobs: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSchedulerJobRequest {
    pub name: String,
    pub command: String,
    pub trigger: TriggerKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerRunResponse {
    pub checked_at: DateTime<Utc>,
    pub limit: usize,
    pub emergency_paused: bool,
    pub executions: Vec<SchedulerJobExecution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerStaleRecoveryResponse {
    pub checked_at: DateTime<Utc>,
    pub older_than_seconds: u64,
    pub limit: usize,
    pub recovered: Vec<SchedulerStaleRecoveryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerStaleRecoveryItem {
    pub job: DiagnosticSchedulerJob,
    pub stale_since: DateTime<Utc>,
    pub stale_for_seconds: i64,
    pub audit_entry: AuditEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerJobExecution {
    pub job: SchedulerJob,
    pub task: TaskRecord,
    pub accepted: bool,
    pub message: String,
    pub audit_entries: Vec<AuditEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerAttentionSummary {
    pub generated_at: DateTime<Utc>,
    pub emergency_paused: bool,
    pub attention_required: bool,
    pub due_count: usize,
    pub scheduled_count: usize,
    pub running_count: usize,
    pub failed_count: usize,
    pub next_due_at: Option<DateTime<Utc>>,
    pub items: Vec<SchedulerAttentionItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerAttentionItem {
    pub id: Uuid,
    pub name: String,
    pub trigger: TriggerKind,
    pub status: SchedulerJobStatus,
    pub due: bool,
    pub next_due_at: Option<DateTime<Utc>>,
    pub notification_kind: String,
    pub notification_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateMemoryItemRequest {
    pub category: String,
    pub key: String,
    pub value: String,
    pub provenance: String,
    pub sensitivity: Sensitivity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMemoryItemRequest {
    pub value: String,
    pub provenance: String,
    pub sensitivity: Sensitivity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallPluginRequest {
    pub manifest_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPluginExecutionRequest {
    #[serde(default)]
    pub execution_enabled: bool,
    #[serde(default)]
    pub execution_grant: InstalledPluginExecutionGrant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPluginPublisherVerificationRequest {
    pub trusted_origin: String,
    pub decided_by: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPluginPublisherSignatureVerificationRequest {
    pub trusted_public_key: String,
    pub decided_by: String,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPluginRunRequest {
    pub action: String,
    #[serde(default)]
    pub input: serde_json::Value,
    #[serde(default)]
    pub session_id: Option<Uuid>,
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPluginRunResponse {
    pub plugin_id: String,
    pub action: String,
    pub status: String,
    pub reason: String,
    pub execution_enabled: bool,
    pub execution_grant: crate::InstalledPluginExecutionGrant,
    pub provenance: InstalledPluginProvenance,
    pub manifest_valid: bool,
    pub action_declared: bool,
    pub input_valid: bool,
    pub contract_validated: bool,
    pub side_effect_executed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr_bytes: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(default)]
    pub progress_events: Vec<crate::PluginProgressEvent>,
    pub audit_entry: AuditEntry,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionGrantSummary {
    pub generated_at: DateTime<Utc>,
    pub approval_counts: Vec<ApprovalStatusCount>,
    pub latest_approvals: Vec<PendingApproval>,
    pub installed_plugin_grants: Vec<InstalledPluginGrantSurface>,
    pub high_risk_pending_count: usize,
    pub executable_installed_plugin_count: usize,
    pub unverified_installed_plugin_count: usize,
    pub side_effects_require_approval: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionPolicyReview {
    pub generated_at: DateTime<Utc>,
    pub status: String,
    pub review_item_count: usize,
    pub high_risk_pending_count: usize,
    pub executable_installed_plugin_count: usize,
    pub unverified_installed_plugin_count: usize,
    pub unreviewed_memory_item_count: usize,
    pub sensitive_memory_item_count: usize,
    pub side_effects_require_approval: bool,
    pub items: Vec<PermissionPolicyReviewItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionPolicyReviewItem {
    pub item_type: String,
    pub severity: String,
    pub title: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub memory_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalStatusCount {
    pub status: ApprovalStatus,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledPluginGrantSurface {
    pub plugin_id: String,
    pub name: String,
    pub execution_enabled: bool,
    pub execution_grant: crate::InstalledPluginExecutionGrant,
    pub integrity_status: crate::InstalledPluginIntegrityStatus,
    pub capture_method: String,
    pub last_verified_at: Option<DateTime<Utc>>,
    pub origin_claim: Option<String>,
    pub origin_claim_verified: bool,
    pub installed_at: DateTime<Utc>,
    pub action_count: usize,
    pub high_risk_action_count: usize,
}

#[derive(Debug, Clone, Default)]
struct EmergencyPauseState {
    paused: bool,
    reason: Option<String>,
    paused_at: Option<DateTime<Utc>>,
    resumed_at: Option<DateTime<Utc>>,
}

impl EmergencyPauseState {
    fn from_stored(stored: StoredEmergencyPauseState) -> Self {
        Self {
            paused: stored.paused,
            reason: stored.reason,
            paused_at: stored.paused.then_some(stored.updated_at),
            resumed_at: (!stored.paused).then_some(stored.updated_at),
        }
    }

    fn updated_at(&self) -> Option<DateTime<Utc>> {
        self.paused_at.or(self.resumed_at)
    }
}

#[derive(Clone)]
pub struct IpcState {
    version: String,
    started_at: DateTime<Utc>,
    scheduler: Scheduler,
    runtime_control: RuntimeControl,
    emergency_pause: Arc<Mutex<EmergencyPauseState>>,
    repository: Option<Arc<Mutex<SqliteRepository>>>,
    provider_config: ProviderConfig,
}

impl Default for IpcState {
    fn default() -> Self {
        Self::new()
    }
}

impl IpcState {
    pub fn new() -> Self {
        Self::with_provider_config(ProviderConfig::default())
    }

    pub fn with_provider_config(provider_config: ProviderConfig) -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            started_at: Utc::now(),
            scheduler: Scheduler::new(),
            runtime_control: RuntimeControl::default(),
            emergency_pause: Arc::new(Mutex::new(EmergencyPauseState::default())),
            repository: None,
            provider_config,
        }
    }

    pub fn with_repository(repository: SqliteRepository) -> JarvisResult<Self> {
        Self::with_repository_and_provider_config(repository, ProviderConfig::default())
    }

    pub fn with_repository_and_provider_config(
        repository: SqliteRepository,
        provider_config: ProviderConfig,
    ) -> JarvisResult<Self> {
        let stored_pause = repository.emergency_pause_state()?;
        let stored_scheduler_jobs = repository.list_scheduler_jobs()?;
        let scheduler = Scheduler::with_jobs(stored_scheduler_jobs);
        if stored_pause.paused {
            let control = RuntimeControl::default();
            control.emergency_pause();
            Ok(Self {
                version: env!("CARGO_PKG_VERSION").to_string(),
                started_at: Utc::now(),
                scheduler,
                runtime_control: control,
                emergency_pause: Arc::new(Mutex::new(EmergencyPauseState::from_stored(
                    stored_pause,
                ))),
                repository: Some(Arc::new(Mutex::new(repository))),
                provider_config,
            })
        } else {
            Ok(Self {
                version: env!("CARGO_PKG_VERSION").to_string(),
                started_at: Utc::now(),
                scheduler,
                runtime_control: RuntimeControl::default(),
                emergency_pause: Arc::new(Mutex::new(EmergencyPauseState::from_stored(
                    stored_pause,
                ))),
                repository: Some(Arc::new(Mutex::new(repository))),
                provider_config,
            })
        }
    }

    pub fn scheduler(&self) -> Scheduler {
        self.scheduler.clone()
    }

    pub fn contract(&self) -> ContractResponse {
        ContractResponse {
            contract: self.contract_metadata(),
            compatibility: contract_compatibility(),
            endpoints: contract_endpoints(),
            safe_inspection_paths: vec![
                "/health".to_string(),
                "/contract".to_string(),
                "/release/readiness".to_string(),
                "/release/evidence-status".to_string(),
                "/diagnostics/export".to_string(),
                "/tools/model".to_string(),
                "/plugins/manifests".to_string(),
                "/plugins/manifests/:id".to_string(),
                "/plugins/installed".to_string(),
                "/plugins/installed/:id".to_string(),
                "/scheduler/jobs".to_string(),
                "/scheduler/attention".to_string(),
                "/scheduler/jobs/:id".to_string(),
                "/activity/summary".to_string(),
                "/activity/events".to_string(),
                "/model-routes".to_string(),
                "/model-routes/:id".to_string(),
                "/memory/classification".to_string(),
                "/permissions/grants".to_string(),
                "/permissions/policy-review".to_string(),
                "/approvals".to_string(),
                "/approvals/:id".to_string(),
            ],
            features: contract_features(),
        }
    }

    pub fn model_tool_catalog(&self) -> JarvisResult<ModelToolCatalogResponse> {
        registered_first_party_model_tool_catalog()
    }

    pub fn health(&self) -> HealthResponse {
        let pause = self.pause_snapshot();
        HealthResponse {
            status: "ok".to_string(),
            version: self.version.clone(),
            contract: self.contract_metadata(),
            started_at: self.started_at,
            emergency_paused: pause.paused,
            emergency_pause_reason: pause.reason.clone(),
            emergency_pause_updated_at: pause.updated_at(),
            scheduler_jobs: self.scheduler.list().len(),
            command_runtime: self.command_runtime_label(),
            local_model_provider: self.provider_config.local.provider,
            local_model: self.provider_config.local.model.clone(),
            local_endpoint_configured: self.provider_config.local.base_url.is_some(),
        }
    }

    pub fn release_readiness(&self) -> ReleaseReadinessResponse {
        let evidence_status = release_evidence_status_from_env();
        let evidence_mode_enabled = release_readiness_evidence_mode_enabled();
        let features = release_readiness_features(&evidence_status, evidence_mode_enabled);
        let implemented_features = features
            .iter()
            .filter(|feature| feature.status == "implemented")
            .cloned()
            .map(ReleaseReadinessFeature::from)
            .collect::<Vec<_>>();
        let pending_features = features
            .into_iter()
            .filter(|feature| feature.status != "implemented")
            .map(ReleaseReadinessFeature::from)
            .collect::<Vec<_>>();
        let production_ready = release_production_ready(
            &evidence_status,
            evidence_mode_enabled,
            pending_features.is_empty(),
        );

        ReleaseReadinessResponse {
            generated_at: Utc::now(),
            production_ready,
            readiness_scope:
                "local Rust/CLI foundation and Swift shell evidence plus explicitly enabled external release evidence status"
                    .to_string(),
            verified_feature_count: implemented_features.len(),
            pending_feature_count: pending_features.len(),
            implemented_features,
            pending_features,
            blocking_manual_gates: release_blocking_manual_gates(&evidence_status, evidence_mode_enabled),
            recommended_verification_commands: release_verification_commands(),
            proof_boundary:
                "Read-only summary derived from /contract feature metadata, release checklist blockers, and explicitly enabled release evidence status; it does not perform signing, notarization, installation, Finder/LaunchServices validation, live microphone/Speech validation, spoken transcript handoff, live audio-output validation, App Store review, marketplace plugin review, malware analysis, or OS sandbox enforcement."
                    .to_string(),
        }
    }

    pub fn release_evidence_status(&self) -> ReleaseEvidenceStatusResponse {
        release_evidence_status_from_env()
    }

    fn command_runtime_label(&self) -> String {
        match self.provider_config.local.provider {
            LocalModelProviderKind::Fake => "routed-fake-local-model+first-party-plugins",
            LocalModelProviderKind::Ollama => "routed-ollama-local-model+first-party-plugins",
        }
        .to_string()
    }

    fn contract_metadata(&self) -> ContractMetadata {
        ContractMetadata {
            name: IPC_CONTRACT_NAME.to_string(),
            version: IPC_CONTRACT_VERSION,
            core_version: self.version.clone(),
        }
    }

    pub fn diagnostics_export(&self) -> JarvisResult<DiagnosticsExport> {
        let (
            schema_version,
            task_count,
            audit_entry_count,
            model_route_record_count,
            active_memory_item_count,
            unreviewed_memory_item_count,
            sensitive_memory_item_count,
        ) = match &self.repository {
            Some(_) => self.using_repository(|repository| {
                let memory_summary = repository.memory_classification_summary(false)?;
                Ok((
                    Some(repository.schema_version()?),
                    Some(repository.list_tasks()?.len()),
                    Some(repository.list_audit_entries(None)?.len()),
                    Some(repository.list_model_route_records(None)?.len()),
                    Some(memory_summary.active_count),
                    Some(memory_summary.unreviewed_active_count),
                    Some(memory_summary.sensitive_active_count),
                ))
            })?,
            None => (None, None, None, None, None, None, None),
        };

        Ok(DiagnosticsExport {
            generated_at: Utc::now(),
            redaction:
                "diagnostics export omits command bodies, scheduler commands, model route contexts, audit payloads, memory values, and cancellation reason text"
                    .to_string(),
            health: self.health(),
            scheduler_jobs: self
                .scheduler
                .list()
                .into_iter()
                .map(DiagnosticSchedulerJob::from)
                .collect(),
            repository_backed: self.repository.is_some(),
            schema_version,
            task_count,
            audit_entry_count,
            model_route_record_count,
            active_memory_item_count,
            unreviewed_memory_item_count,
            sensitive_memory_item_count,
        })
    }

    pub fn activity_summary(&self) -> JarvisResult<ActivitySummary> {
        self.using_repository(|repository| {
            let tasks = repository.list_tasks()?;
            let audit_entries = repository.list_audit_entries(None)?;
            let active_statuses = [
                TaskStatus::Created,
                TaskStatus::Running,
                TaskStatus::WaitingForApproval,
            ];
            let all_statuses = [
                TaskStatus::Created,
                TaskStatus::Running,
                TaskStatus::WaitingForApproval,
                TaskStatus::Blocked,
                TaskStatus::Completed,
                TaskStatus::Failed,
                TaskStatus::Cancelled,
            ];

            let status_counts = all_statuses
                .iter()
                .filter_map(|status| {
                    let count = tasks.iter().filter(|task| &task.status == status).count();
                    (count > 0).then_some(ActivityStatusCount {
                        status: status.clone(),
                        count,
                    })
                })
                .collect::<Vec<_>>();

            let active_task_count = tasks
                .iter()
                .filter(|task| active_statuses.contains(&task.status))
                .count();
            let recent_tasks = tasks
                .iter()
                .rev()
                .take(5)
                .map(ActivityTaskSummary::from)
                .collect::<Vec<_>>();
            let recent_audit_entries = audit_entries
                .iter()
                .rev()
                .take(10)
                .cloned()
                .collect::<Vec<_>>();

            Ok(ActivitySummary {
                generated_at: Utc::now(),
                repository_backed: true,
                task_count: tasks.len(),
                audit_entry_count: audit_entries.len(),
                active_task_count,
                status_counts,
                recent_tasks,
                recent_audit_entries,
            })
        })
    }

    pub fn list_approvals(
        &self,
        status: Option<ApprovalStatus>,
    ) -> JarvisResult<Vec<PendingApproval>> {
        self.using_repository(|repository| repository.list_pending_approvals(status))
    }

    pub fn permission_grant_summary(&self) -> JarvisResult<PermissionGrantSummary> {
        self.using_repository(|repository| {
            let mut approvals = repository.list_pending_approvals(None)?;
            let installed_plugins = repository.list_installed_plugins()?;
            let pending_count = approvals
                .iter()
                .filter(|approval| approval.status == ApprovalStatus::Pending)
                .count();
            let approved_count = approvals
                .iter()
                .filter(|approval| approval.status == ApprovalStatus::Approved)
                .count();
            let denied_count = approvals
                .iter()
                .filter(|approval| approval.status == ApprovalStatus::Denied)
                .count();

            approvals.sort_by(|left, right| {
                right
                    .requested_at
                    .cmp(&left.requested_at)
                    .then_with(|| left.id.cmp(&right.id))
            });

            let high_risk_pending_count = approvals
                .iter()
                .filter(|approval| {
                    approval.status == ApprovalStatus::Pending
                        && matches!(
                            approval.risk_tier,
                            crate::RiskTier::Confirm | crate::RiskTier::Block
                        )
                })
                .count();
            let executable_installed_plugin_count = installed_plugins
                .iter()
                .filter(|plugin| plugin.execution_enabled)
                .count();
            let unverified_installed_plugin_count = installed_plugins
                .iter()
                .filter(|plugin| {
                    plugin.provenance.integrity_status
                        != crate::InstalledPluginIntegrityStatus::MatchesInstallSnapshot
                })
                .count();
            let installed_plugin_grants = installed_plugins
                .into_iter()
                .map(|plugin| InstalledPluginGrantSurface {
                    plugin_id: plugin.id,
                    name: plugin.manifest.name,
                    execution_enabled: plugin.execution_enabled,
                    execution_grant: plugin.execution_grant,
                    integrity_status: plugin.provenance.integrity_status,
                    capture_method: plugin.provenance.capture_method,
                    last_verified_at: plugin.provenance.last_verified_at,
                    origin_claim: plugin.provenance.origin_claim,
                    origin_claim_verified: plugin.provenance.origin_claim_verified,
                    installed_at: plugin.installed_at,
                    action_count: plugin.manifest.actions.len(),
                    high_risk_action_count: plugin
                        .manifest
                        .actions
                        .iter()
                        .filter(|action| {
                            matches!(
                                action.risk_tier,
                                crate::RiskTier::Confirm | crate::RiskTier::Block
                            )
                        })
                        .count(),
                })
                .collect();

            Ok(PermissionGrantSummary {
                generated_at: Utc::now(),
                approval_counts: vec![
                    ApprovalStatusCount {
                        status: ApprovalStatus::Pending,
                        count: pending_count,
                    },
                    ApprovalStatusCount {
                        status: ApprovalStatus::Approved,
                        count: approved_count,
                    },
                    ApprovalStatusCount {
                        status: ApprovalStatus::Denied,
                        count: denied_count,
                    },
                ],
                latest_approvals: approvals.into_iter().take(10).collect(),
                installed_plugin_grants,
                high_risk_pending_count,
                executable_installed_plugin_count,
                unverified_installed_plugin_count,
                side_effects_require_approval: true,
            })
        })
    }

    pub fn permission_policy_review(&self) -> JarvisResult<PermissionPolicyReview> {
        self.using_repository(|repository| {
            let approvals = repository.list_pending_approvals(None)?;
            let installed_plugins = repository.list_installed_plugins()?;
            let memory_items = repository.list_memory_items(false)?;
            let all_memory_items = repository.list_memory_items(true)?;
            let memory_summary = repository.memory_classification_summary(false)?;
            let mut items = Vec::new();

            let high_risk_pending_count = approvals
                .iter()
                .filter(|approval| {
                    approval.status == ApprovalStatus::Pending
                        && matches!(
                            approval.risk_tier,
                            crate::RiskTier::Confirm | crate::RiskTier::Block
                        )
                })
                .count();

            for job in self.scheduler.list().into_iter().filter(|job| {
                matches!(
                    job.status,
                    SchedulerJobStatus::Scheduled | SchedulerJobStatus::Running
                )
            }) {
                items.push(scheduler_policy_review_item(job, Utc::now()));
            }

            for approval in approvals
                .iter()
                .filter(|approval| approval.status == ApprovalStatus::Pending)
            {
                let severity = match approval.risk_tier {
                    crate::RiskTier::Block => "critical",
                    crate::RiskTier::Confirm => "high",
                    crate::RiskTier::Notify => "medium",
                    crate::RiskTier::Low => "low",
                };
                items.push(PermissionPolicyReviewItem {
                    item_type: "pending_approval".to_string(),
                    severity: severity.to_string(),
                    title: "Pending approval requires review".to_string(),
                    detail: format!(
                        "{} requests {:?} access for {:?} data",
                        approval.action, approval.risk_tier, approval.sensitivity
                    ),
                    approval_id: Some(approval.id),
                    plugin_id: None,
                    memory_id: None,
                    action: Some(approval.action.clone()),
                });
            }

            let executable_installed_plugin_count = installed_plugins
                .iter()
                .filter(|plugin| plugin.execution_enabled)
                .count();
            let unverified_installed_plugin_count = installed_plugins
                .iter()
                .filter(|plugin| {
                    plugin.provenance.integrity_status
                        != crate::InstalledPluginIntegrityStatus::MatchesInstallSnapshot
                })
                .count();

            for plugin in installed_plugins {
                if plugin.provenance.integrity_status
                    != crate::InstalledPluginIntegrityStatus::MatchesInstallSnapshot
                {
                    items.push(PermissionPolicyReviewItem {
                        item_type: "installed_plugin_provenance".to_string(),
                        severity: if plugin.execution_enabled {
                            "critical"
                        } else {
                            "medium"
                        }
                        .to_string(),
                        title: "Installed plugin provenance is not verified".to_string(),
                        detail: format!(
                            "{} integrity status is {:?}; execution remains fail-closed until the install snapshot verifies",
                            plugin.manifest.name, plugin.provenance.integrity_status
                        ),
                        approval_id: None,
                        plugin_id: Some(plugin.id.clone()),
                        memory_id: None,
                        action: None,
                    });
                }

                if plugin
                    .provenance
                    .origin_claim
                    .as_ref()
                    .is_some_and(|_| !plugin.provenance.origin_claim_verified)
                {
                    items.push(PermissionPolicyReviewItem {
                        item_type: "publisher_identity".to_string(),
                        severity: "medium".to_string(),
                        title: "Plugin publisher origin is unverified".to_string(),
                        detail: format!(
                            "{} declares a publisher origin that has not been verified",
                            plugin.manifest.name
                        ),
                        approval_id: None,
                        plugin_id: Some(plugin.id.clone()),
                        memory_id: None,
                        action: None,
                    });
                }

                for action in plugin.manifest.actions.iter().filter(|action| {
                    matches!(
                        action.risk_tier,
                        crate::RiskTier::Confirm | crate::RiskTier::Block
                    )
                }) {
                    items.push(PermissionPolicyReviewItem {
                        item_type: "high_risk_plugin_action".to_string(),
                        severity: if plugin.execution_enabled {
                            "high"
                        } else {
                            "low"
                        }
                        .to_string(),
                        title: "Plugin declares high-risk action".to_string(),
                        detail: format!(
                            "{} declares {} as {:?}; side effects require explicit approval",
                            plugin.manifest.name, action.name, action.risk_tier
                        ),
                        approval_id: None,
                        plugin_id: Some(plugin.id.clone()),
                        memory_id: None,
                        action: Some(action.name.clone()),
                    });
                }

                for action in plugin.manifest.actions.iter().filter(|action| {
                    action.network_access.mode != crate::PluginNetworkAccessMode::None
                }) {
                    items.push(PermissionPolicyReviewItem {
                        item_type: "network_plugin_action".to_string(),
                        severity: if plugin.execution_enabled {
                            "high"
                        } else {
                            "medium"
                        }
                        .to_string(),
                        title: "Plugin declares network access".to_string(),
                        detail: format!(
                            "{} declares {} network access to {:?}; this is manifest governance, not an OS network sandbox",
                            plugin.manifest.name,
                            action.name,
                            action.network_access.allowed_hosts
                        ),
                        approval_id: None,
                        plugin_id: Some(plugin.id.clone()),
                        memory_id: None,
                        action: Some(action.name.clone()),
                    });
                }
            }

            for memory in memory_items
                .iter()
                .filter(|memory| memory.reviewed_at.is_none())
            {
                items.push(PermissionPolicyReviewItem {
                    item_type: "memory_review".to_string(),
                    severity: memory_review_severity(memory.sensitivity).to_string(),
                    title: "Memory item needs review".to_string(),
                    detail: format!(
                        "Memory item {}/{} is {:?} and unreviewed; value text is redacted from policy review",
                        memory.category, memory.key, memory.sensitivity
                    ),
                    approval_id: None,
                    plugin_id: None,
                    memory_id: Some(memory.id),
                    action: Some(format!("{}/{}", memory.category, memory.key)),
                });
            }

            for memory in all_memory_items.iter().filter(|memory| {
                memory.deleted_at.is_some()
                    && memory_sensitivity_requires_retention_review(memory.sensitivity)
            }) {
                items.push(PermissionPolicyReviewItem {
                    item_type: "memory_retention_review".to_string(),
                    severity: memory_review_severity(memory.sensitivity).to_string(),
                    title: "Deleted sensitive memory is retained locally".to_string(),
                    detail: format!(
                        "Deleted memory item {}/{} is {:?} and still retained in local storage; value text is redacted from policy review",
                        memory.category, memory.key, memory.sensitivity
                    ),
                    approval_id: None,
                    plugin_id: None,
                    memory_id: Some(memory.id),
                    action: Some(format!("{}/{}", memory.category, memory.key)),
                });
            }

            items.sort_by_key(|item| {
                (
                    permission_review_severity_rank(&item.severity),
                    item.item_type.clone(),
                    item.title.clone(),
                    item.plugin_id.clone(),
                    item.memory_id,
                    item.action.clone(),
                    item.approval_id,
                )
            });

            Ok(PermissionPolicyReview {
                generated_at: Utc::now(),
                status: if items.is_empty() {
                    "clear".to_string()
                } else {
                    "review_required".to_string()
                },
                review_item_count: items.len(),
                high_risk_pending_count,
                executable_installed_plugin_count,
                unverified_installed_plugin_count,
                unreviewed_memory_item_count: memory_summary.unreviewed_active_count,
                sensitive_memory_item_count: memory_summary.sensitive_active_count,
                side_effects_require_approval: true,
                items,
            })
        })
    }

    pub fn get_approval(&self, id: Uuid) -> JarvisResult<PendingApproval> {
        self.using_repository(|repository| {
            repository
                .get_pending_approval(id)?
                .ok_or_else(|| JarvisError::Storage(format!("pending approval not found: {id}")))
        })
    }

    pub fn approve_approval(
        &self,
        id: Uuid,
        decided_by: String,
        reason: Option<String>,
    ) -> JarvisResult<PendingApproval> {
        self.decide_approval(id, ApprovalStatus::Approved, decided_by, reason)
    }

    pub fn deny_approval(
        &self,
        id: Uuid,
        decided_by: String,
        reason: Option<String>,
    ) -> JarvisResult<PendingApproval> {
        self.decide_approval(id, ApprovalStatus::Denied, decided_by, reason)
    }

    fn decide_approval(
        &self,
        id: Uuid,
        status: ApprovalStatus,
        decided_by: String,
        reason: Option<String>,
    ) -> JarvisResult<PendingApproval> {
        self.using_repository(|repository| {
            let approval = repository.decide_pending_approval(id, status, decided_by, reason)?;
            let event_type = match status {
                ApprovalStatus::Approved => "approval_granted",
                ApprovalStatus::Denied => "approval_denied",
                _ => "approval_decision",
            };
            repository.append_audit_entry(&AuditEntry::new(
                Some(approval.task_id),
                event_type,
                "pending approval was decided; side effect remains unexecuted until retried with an approval grant",
                json!({
                    "approval_id": approval.id,
                    "action": approval.action,
                    "status": approval.status,
                    "risk_tier": approval.risk_tier,
                    "sensitivity": approval.sensitivity,
                    "requested_scopes": approval.requested_scopes,
                    "decided_by": approval.decided_by,
                    "decision_reason": approval.decision_reason,
                    "side_effect_executed": false,
                }),
            ))?;
            Ok(approval)
        })
    }

    pub fn execute_approved_approval(&self, id: Uuid) -> JarvisResult<ApprovalExecutionResponse> {
        let (approval, task) = self.using_repository(|repository| {
            let approval = repository
                .get_pending_approval(id)?
                .ok_or_else(|| JarvisError::Storage(format!("pending approval not found: {id}")))?;
            if approval.status != ApprovalStatus::Approved {
                return Err(JarvisError::Validation(format!(
                    "approval {id} must be approved before execution"
                )));
            }
            let task = repository.get_task(approval.task_id)?.ok_or_else(|| {
                JarvisError::Storage(format!("task not found: {}", approval.task_id))
            })?;
            let approval_id = approval.id.to_string();
            let already_executed =
                repository
                    .list_audit_entries(Some(task.id))?
                    .iter()
                    .any(|entry| {
                        entry.event_type == "approval_executed"
                            && entry
                                .payload
                                .get("approval_id")
                                .and_then(serde_json::Value::as_str)
                                == Some(approval_id.as_str())
                    });
            if already_executed {
                return Err(JarvisError::Validation(format!(
                    "approval {id} has already been executed"
                )));
            }
            Ok((approval, task))
        })?;

        let mut plugin_request = first_party_plugin_request(&task.user_input).ok_or_else(|| {
            JarvisError::Validation(format!(
                "approval {id} cannot be executed because its task is not a first-party plugin command"
            ))
        })?;
        let action_name = format!("{}.{}", plugin_request.plugin_id, plugin_request.action);
        if action_name != approval.action {
            return Err(JarvisError::Validation(format!(
                "approval {id} action mismatch: approval is {}, task would execute {action_name}",
                approval.action
            )));
        }

        let host = PluginHost::with_first_party_plugins()?;
        let manifest = host.manifest(&plugin_request.plugin_id)?;
        let action = manifest.action(&plugin_request.action).ok_or_else(|| {
            JarvisError::Plugin(format!(
                "plugin {} does not declare action {}",
                plugin_request.plugin_id, plugin_request.action
            ))
        })?;
        let requested_scopes = plugin_permission_scopes(&action.permissions);
        if requested_scopes != approval.requested_scopes {
            return Err(JarvisError::Validation(format!(
                "approval {id} scope mismatch; the current plugin contract differs from the approval record"
            )));
        }

        let mut granted_scopes = approval.requested_scopes.clone();
        granted_scopes.push(CapabilityScope::Conversation);
        plugin_request.granted_scopes = granted_scopes.clone();
        plugin_request.sensitivity = approval.sensitivity;
        plugin_request = plugin_request
            .with_approval(ApprovalGrant::approved(approval.requested_scopes.clone()));

        let policy_request = PolicyRequest {
            task_id: Some(task.id),
            action: approval.action.clone(),
            requested_scopes,
            granted_scopes,
            risk_tier: action.risk_tier,
            sensitivity: approval.sensitivity,
            emergency_paused: self.runtime_control.is_emergency_paused(),
            approval: plugin_request.approval.clone(),
        };
        let policy = PermissionEngine::evaluate(&policy_request);
        if policy.decision == ApprovalDecision::Blocked {
            return Err(JarvisError::PolicyBlocked(policy.reason));
        }
        if policy.decision == ApprovalDecision::RequireConfirmation {
            return Err(JarvisError::Validation(format!(
                "approval {id} did not satisfy the current policy requirement"
            )));
        }

        let result = host.execute(plugin_request)?;
        let completed = result.status == PluginCallStatus::Completed;
        let task_status = if completed {
            TaskStatus::Completed
        } else {
            TaskStatus::Failed
        };

        let mut audit_entries = Vec::new();
        let policy_audit = AuditEntry::new(
            Some(task.id),
            "approval_execution_policy_evaluated",
            "approved first-party plugin action policy was evaluated before execution",
            json!({
                "approval_id": approval.id,
                "action": approval.action,
                "decision": policy.decision,
                "reason": policy.reason,
                "risk_tier": policy.risk_tier,
                "approval_status": policy.approval_status,
                "side_effect_executed": false,
            }),
        );
        let plugin_event_type = match result.status {
            PluginCallStatus::Completed => "plugin_completed_after_approval",
            PluginCallStatus::ApprovalRequired => "plugin_approval_required_after_approval",
            PluginCallStatus::TimedOut => "plugin_timed_out_after_approval",
            PluginCallStatus::Cancelled => "plugin_cancelled_after_approval",
            PluginCallStatus::Failed => "plugin_failed_after_approval",
        };
        let plugin_audit = AuditEntry::new(
            Some(task.id),
            plugin_event_type,
            "approved first-party plugin action finished",
            json!({
                "approval_id": approval.id,
                "plugin_id": result.metadata.plugin_id,
                "action": result.metadata.action,
                "status": result.status,
                "risk_tier": result.metadata.risk_tier,
                "approval_status": result.metadata.approval_status,
                "proactive": result.metadata.proactive,
                "timeout_ms": result.metadata.timeout_ms,
                "side_effect_executed": completed,
            }),
        );
        let execution_audit = AuditEntry::new(
            Some(task.id),
            "approval_executed",
            "approved first-party plugin action execution completed",
            json!({
                "approval_id": approval.id,
                "action": approval.action,
                "status": result.status,
                "approval_status": approval.status,
                "side_effect_executed": completed,
            }),
        );
        audit_entries.push(policy_audit.clone());
        audit_entries.push(plugin_audit.clone());
        audit_entries.push(execution_audit.clone());

        let task = self.using_repository(|repository| {
            repository.append_audit_entry(&policy_audit)?;
            repository.append_audit_entry(&plugin_audit)?;
            repository.append_audit_entry(&execution_audit)?;
            repository.update_task_status(task.id, task_status)
        })?;

        Ok(ApprovalExecutionResponse {
            accepted: completed,
            approval,
            task,
            audit_entry: execution_audit,
            audit_entries,
            plugin_results: vec![result],
            message: if completed {
                "approved first-party plugin action executed".to_string()
            } else {
                "approved first-party plugin action did not complete".to_string()
            },
        })
    }

    pub async fn submit_command(&self, request: CommandRequest) -> JarvisResult<CommandResponse> {
        if request.input.trim().is_empty() {
            return Err(JarvisError::Validation(
                "command input cannot be empty".to_string(),
            ));
        }

        let command_store = self.command_store();
        let model = RoutedModelExecutor::from_config(&self.provider_config)?;
        let runtime = ConversationRuntime::with_storage_parts(
            RuntimeConfig::default().with_provider_config(self.provider_config.clone()),
            self.runtime_control.clone(),
            model,
            crate::NoopRuntimeHooks,
            command_store.clone(),
        );
        let sensitivity = request
            .sensitivity
            .or_else(|| sensitivity_from_context(&request.context))
            .unwrap_or(Sensitivity::Personal);
        let runtime_response = runtime
            .execute_command(
                RuntimeCommandRequest::new(request.input.clone())
                    .with_session_id(request.session_id.unwrap_or_else(Uuid::new_v4))
                    .with_sensitivity(sensitivity),
            )
            .await?;

        let mut audit_entries = runtime_response.audit_entries;
        let mut task = runtime_response.task;
        let plugin_dispatch = if task.status == TaskStatus::Completed {
            self.maybe_execute_first_party_plugin(
                &mut task,
                FirstPartyPluginDispatchContext {
                    input: &request.input,
                    sensitivity,
                    dry_run: request.dry_run,
                    proactive: request.proactive,
                    audit_entries: &mut audit_entries,
                    command_store: &command_store,
                },
            )?
        } else {
            PluginDispatch::default()
        };

        if plugin_dispatch.waiting_for_approval {
            command_store.update_task_status(&mut task, TaskStatus::WaitingForApproval)?;
        }

        let accepted = task.status == TaskStatus::Completed;
        let audit_entry = audit_entries.last().cloned().unwrap_or_else(|| {
            AuditEntry::new(
                Some(task.id),
                "command_runtime_empty",
                "command runtime returned no audit entries",
                json!({
                    "dry_run": request.dry_run,
                    "context": request.context,
                }),
            )
        });

        Ok(CommandResponse {
            accepted,
            task,
            audit_entry,
            audit_entries,
            route: runtime_response.route,
            route_evidence: runtime_response.route_evidence,
            steps: runtime_response.steps,
            plugin_results: plugin_dispatch.results,
            message: plugin_dispatch.message.unwrap_or(runtime_response.message),
        })
    }

    fn command_store(&self) -> SharedCommandStore {
        SharedCommandStore {
            repository: self.repository.clone(),
        }
    }

    fn maybe_execute_first_party_plugin(
        &self,
        task: &mut TaskRecord,
        context: FirstPartyPluginDispatchContext<'_>,
    ) -> JarvisResult<PluginDispatch> {
        let Some(mut plugin_request) = first_party_plugin_request(context.input) else {
            return Ok(PluginDispatch::default());
        };
        let host = PluginHost::with_first_party_plugins()?;
        let manifest = host.manifest(&plugin_request.plugin_id)?;
        let action = manifest.action(&plugin_request.action).ok_or_else(|| {
            JarvisError::Plugin(format!(
                "plugin {} does not declare action {}",
                plugin_request.plugin_id, plugin_request.action
            ))
        })?;
        let requested_scopes = plugin_permission_scopes(&action.permissions);
        let mut granted_scopes = requested_scopes.clone();
        granted_scopes.push(CapabilityScope::Conversation);
        plugin_request.granted_scopes = granted_scopes.clone();
        plugin_request.sensitivity = context.sensitivity;
        plugin_request.proactive = context.proactive;
        let policy_request = PolicyRequest {
            task_id: Some(task.id),
            action: format!("{}.{}", plugin_request.plugin_id, plugin_request.action),
            requested_scopes,
            granted_scopes,
            risk_tier: action.risk_tier,
            sensitivity: context.sensitivity,
            emergency_paused: self.runtime_control.is_emergency_paused(),
            approval: None,
        };
        let policy = PermissionEngine::evaluate(&policy_request);
        let policy_audit = AuditEntry::new(
            Some(task.id),
            "plugin_policy_evaluated",
            "policy evaluated first-party plugin action",
            json!({
                "plugin_id": plugin_request.plugin_id,
                "action": plugin_request.action,
                "decision": policy.decision,
                "reason": policy.reason,
                "risk_tier": policy.risk_tier,
                "approval_status": policy.approval_status,
                "missing_scopes": policy.missing_scopes,
                "dry_run": context.dry_run,
                "proactive": context.proactive,
            }),
        );
        context.command_store.append_audit_entry(&policy_audit)?;
        context.audit_entries.push(policy_audit);

        if policy.decision == ApprovalDecision::Blocked {
            return Err(JarvisError::PolicyBlocked(policy.reason));
        }

        if policy.decision == ApprovalDecision::RequireConfirmation {
            plugin_request.approval_status = ApprovalStatus::Pending;
            let approval = self.persist_pending_approval(NewPendingApproval {
                task_id: task.id,
                action: format!("{}.{}", plugin_request.plugin_id, plugin_request.action),
                requested_scopes: policy_request.requested_scopes,
                risk_tier: action.risk_tier,
                sensitivity: context.sensitivity,
                reason: policy.reason.clone(),
            })?;
            let approval_audit = AuditEntry::new(
                Some(task.id),
                "approval_pending",
                "first-party plugin action is pending explicit approval and did not execute",
                json!({
                    "approval_id": approval.id,
                    "plugin_id": plugin_request.plugin_id,
                    "action": plugin_request.action,
                    "risk_tier": approval.risk_tier,
                    "sensitivity": approval.sensitivity,
                    "requested_scopes": approval.requested_scopes,
                    "approval_status": approval.status,
                    "side_effect_executed": false,
                }),
            );
            context.command_store.append_audit_entry(&approval_audit)?;
            context.audit_entries.push(approval_audit);
        }

        if context.dry_run {
            let dry_run_audit = AuditEntry::new(
                Some(task.id),
                "plugin_dry_run",
                "dry run skipped first-party plugin execution",
                json!({
                    "plugin_id": plugin_request.plugin_id,
                    "action": plugin_request.action,
                    "approval_status": plugin_request.approval_status,
                    "proactive": context.proactive,
                }),
            );
            context.command_store.append_audit_entry(&dry_run_audit)?;
            context.audit_entries.push(dry_run_audit);
            return Ok(PluginDispatch::default());
        }

        let result = match host.execute(plugin_request) {
            Ok(result) => result,
            Err(error) => {
                let status = if matches!(error, JarvisError::PolicyBlocked(_)) {
                    TaskStatus::Blocked
                } else {
                    TaskStatus::Failed
                };
                context.command_store.update_task_status(task, status)?;
                let blocked_audit = AuditEntry::new(
                    Some(task.id),
                    "plugin_execution_blocked",
                    "first-party plugin action was blocked before execution",
                    json!({
                        "plugin_id": manifest.id,
                        "action": action.name,
                        "error": error.to_string(),
                        "proactive": context.proactive,
                        "side_effect_executed": false,
                    }),
                );
                context.command_store.append_audit_entry(&blocked_audit)?;
                context.audit_entries.push(blocked_audit);
                return Ok(PluginDispatch {
                    waiting_for_approval: false,
                    results: Vec::new(),
                    message: Some(format!("First-party plugin execution blocked: {error}")),
                });
            }
        };
        let event_type = match result.status {
            PluginCallStatus::Completed => "plugin_completed",
            PluginCallStatus::ApprovalRequired => "plugin_approval_required",
            PluginCallStatus::TimedOut => "plugin_timed_out",
            PluginCallStatus::Cancelled => "plugin_cancelled",
            PluginCallStatus::Failed => "plugin_failed",
        };
        let plugin_audit = AuditEntry::new(
            Some(task.id),
            event_type,
            "first-party plugin action finished",
            json!({
                "plugin_id": result.metadata.plugin_id,
                "action": result.metadata.action,
                "status": result.status,
                "risk_tier": result.metadata.risk_tier,
                "approval_status": result.metadata.approval_status,
                "proactive": result.metadata.proactive,
                "timeout_ms": result.metadata.timeout_ms,
            }),
        );
        context.command_store.append_audit_entry(&plugin_audit)?;
        context.audit_entries.push(plugin_audit);
        Ok(PluginDispatch {
            waiting_for_approval: result.status == PluginCallStatus::ApprovalRequired,
            results: vec![result],
            message: None,
        })
    }

    fn persist_pending_approval(
        &self,
        approval: NewPendingApproval,
    ) -> JarvisResult<PendingApproval> {
        self.using_repository(|repository| repository.create_pending_approval(approval))
    }

    pub fn run_installed_plugin(
        &self,
        id: &str,
        request: InstalledPluginRunRequest,
    ) -> JarvisResult<InstalledPluginRunResponse> {
        if request.action.trim().is_empty() {
            return Err(JarvisError::Validation(
                "installed plugin action cannot be empty".to_string(),
            ));
        }

        self.using_repository(|repository| {
            let record = repository.verify_installed_plugin_provenance(id)?;
            let manifest_validation = validate_installed_plugin_record(&record);
            let manifest_valid = manifest_validation.is_ok();
            let action_manifest = if manifest_valid {
                record.manifest.action(&request.action)
            } else {
                None
            };
            let action_declared = action_manifest.is_some();
            let input_validation = if let Some(action) = action_manifest {
                action
                    .input_schema
                    .validate_value("installed plugin input", &request.input)
            } else {
                Ok(())
            };
            let input_valid = action_declared && input_validation.is_ok();
            let contract_validated = manifest_valid && action_declared && input_valid;
            let action_requires_network_grant = action_manifest
                .is_some_and(|action| action.network_access.mode != crate::PluginNetworkAccessMode::None);
            let execution_ready = contract_validated
                && record.execution_enabled
                && installed_plugin_grant_allows_action(
                    record.execution_grant,
                    action_requires_network_grant,
                )
                && record.provenance.integrity_status
                    == InstalledPluginIntegrityStatus::MatchesInstallSnapshot
                && record.manifest.source == PluginSource::LocalSubprocess
                && record.manifest.subprocess.is_some();
            let mut output = None;
            let mut stdout_bytes = None;
            let mut stderr_bytes = None;
            let mut exit_code = None;
            let mut progress_events = Vec::new();
            let mut side_effect_executed = false;

            let (status, reason) = if let Err(error) = &manifest_validation {
                ("blocked".to_string(), error.to_string())
            } else if !action_declared {
                (
                    "blocked".to_string(),
                    format!(
                        "installed plugin {} does not declare action {}",
                        record.id, request.action
                    ),
                )
            } else if let Err(error) = &input_validation {
                ("blocked".to_string(), error.to_string())
            } else if request.dry_run {
                (
                    "dry_run".to_string(),
                    "installed plugin contract dry run validated manifest, action, and input schema without executing plugin code"
                        .to_string(),
                )
            } else if record.execution_grant == InstalledPluginExecutionGrant::MetadataOnly {
                (
                    "blocked".to_string(),
                    "installed plugin execution grant is metadata_only; only contract dry runs are allowed"
                        .to_string(),
                )
            } else if action_requires_network_grant
                && record.execution_grant != InstalledPluginExecutionGrant::SubprocessStdioNetwork
            {
                (
                    "blocked".to_string(),
                    "installed plugin action declares network access; execution requires subprocess_stdio_network grant"
                        .to_string(),
                )
            } else if !action_requires_network_grant
                && record.execution_grant == InstalledPluginExecutionGrant::SubprocessStdioNetwork
            {
                (
                    "blocked".to_string(),
                    "installed plugin action does not declare network access; subprocess_stdio_network grant is reserved for network-declaring actions"
                        .to_string(),
                )
            } else if !record.execution_enabled {
                (
                    "blocked".to_string(),
                    "installed plugin execution is disabled; enable execution with an explicit non-metadata grant"
                        .to_string(),
                )
            } else if record.provenance.integrity_status
                != InstalledPluginIntegrityStatus::MatchesInstallSnapshot
            {
                (
                    "blocked".to_string(),
                    "installed plugin provenance is not verified against the local install snapshot"
                        .to_string(),
                )
            } else if record.manifest.source != PluginSource::LocalSubprocess {
                (
                    "blocked".to_string(),
                    "installed plugin execution requires local_subprocess source".to_string(),
                )
            } else if record.manifest.subprocess.is_none() {
                (
                    "blocked".to_string(),
                    "installed plugin execution requires subprocess config".to_string(),
                )
            } else if execution_ready {
                let action = action_manifest.expect("contract validated action");
                let source_path = std::path::Path::new(&record.source_path);
                side_effect_executed = true;
                match execute_installed_subprocess_plugin(
                    &record.manifest,
                    action,
                    source_path,
                    &request.input,
                ) {
                    Ok(execution) => {
                        stdout_bytes = Some(execution.stdout_bytes);
                        stderr_bytes = Some(execution.stderr_bytes);
                        exit_code = execution.exit_code;
                        progress_events = execution.progress_events;
                        output = Some(execution.output);
                        (
                            "completed".to_string(),
                            "installed plugin subprocess completed with validated JSON output"
                                .to_string(),
                        )
                    }
                    Err(error) => ("failed".to_string(), error.to_string()),
                }
            } else {
                (
                    "blocked".to_string(),
                    "installed plugin execution did not satisfy subprocess execution preconditions"
                        .to_string(),
                )
            };
            let event_type = if status == "completed" {
                "installed_plugin_subprocess_completed"
            } else if status == "failed" {
                "installed_plugin_subprocess_failed"
            } else if request.dry_run && contract_validated {
                "installed_plugin_contract_dry_run"
            } else if manifest_valid && action_declared && !input_valid {
                "installed_plugin_input_invalid"
            } else if manifest_valid && action_declared {
                "installed_plugin_execution_blocked"
            } else if manifest_valid {
                "installed_plugin_action_blocked"
            } else {
                "installed_plugin_manifest_invalid"
            };
            let audit_entry = AuditEntry::new(
                None,
                event_type,
                "installed plugin run request failed closed before execution",
                json!({
                    "plugin_id": record.id,
                    "action": request.action,
                    "session_id": request.session_id,
                    "manifest_schema_version": record.manifest.manifest_schema_version,
                    "manifest_version": record.manifest.version,
                    "source": record.manifest.source,
                    "source_path": record.source_path,
                    "provenance": record.provenance,
                    "execution_enabled": record.execution_enabled,
                    "execution_grant": record.execution_grant,
                    "dry_run": request.dry_run,
                    "manifest_valid": manifest_valid,
                    "action_declared": action_declared,
                    "action_requires_network_grant": action_requires_network_grant,
                    "input_valid": input_valid,
                    "contract_validated": contract_validated,
                    "input_provided": !request.input.is_null(),
                    "side_effect_executed": side_effect_executed,
                    "subprocess_started": side_effect_executed,
                    "sandbox_process_started": side_effect_executed,
                    "stdout_bytes": stdout_bytes,
                    "stderr_bytes": stderr_bytes,
                    "exit_code": exit_code,
                    "progress_event_count": progress_events.len(),
                    "reason": reason,
                }),
            );
            for progress_event in &progress_events {
                repository.append_audit_entry(&AuditEntry::new(
                    None,
                    "installed_plugin_progress",
                    "installed subprocess plugin reported bounded progress",
                    json!({
                        "plugin_id": record.id,
                        "action": request.action,
                        "session_id": request.session_id,
                        "sequence": progress_event.sequence,
                        "stage": progress_event.stage,
                        "message": progress_event.message,
                        "stderr_redacted": true,
                    }),
                ))?;
            }
            repository.append_audit_entry(&audit_entry)?;

            Ok(InstalledPluginRunResponse {
                plugin_id: record.id,
                action: request.action,
                status,
                reason,
                execution_enabled: record.execution_enabled,
                execution_grant: record.execution_grant,
                provenance: record.provenance,
                manifest_valid,
                action_declared,
                input_valid,
                contract_validated,
                side_effect_executed,
                output,
                stdout_bytes,
                stderr_bytes,
                exit_code,
                progress_events,
                audit_entry,
            })
        })
    }

    pub fn set_installed_plugin_execution(
        &self,
        id: &str,
        request: InstalledPluginExecutionRequest,
    ) -> JarvisResult<InstalledPluginRecord> {
        self.using_repository(|repository| {
            let record = repository
                .get_installed_plugin(id)?
                .ok_or_else(|| JarvisError::Storage(format!("installed plugin not found: {id}")))?;
            validate_installed_plugin_record(&record)?;

            if request.execution_enabled {
                let has_network_action = record
                    .manifest
                    .actions
                    .iter()
                    .any(|action| action.network_access.mode != crate::PluginNetworkAccessMode::None);
                let has_non_network_action = record
                    .manifest
                    .actions
                    .iter()
                    .any(|action| action.network_access.mode == crate::PluginNetworkAccessMode::None);
                match request.execution_grant {
                    InstalledPluginExecutionGrant::MetadataOnly => {
                        return Err(JarvisError::Validation(
                            "installed plugin execution requires subprocess_stdio or subprocess_stdio_network grant".to_string(),
                        ));
                    }
                    InstalledPluginExecutionGrant::SubprocessStdio if !has_non_network_action => {
                        return Err(JarvisError::Validation(
                            "subprocess_stdio grant requires at least one non-network action".to_string(),
                        ));
                    }
                    InstalledPluginExecutionGrant::SubprocessStdioNetwork
                        if !has_network_action =>
                    {
                        return Err(JarvisError::Validation(
                            "subprocess_stdio_network grant requires at least one network-declaring action".to_string(),
                        ));
                    }
                    InstalledPluginExecutionGrant::SubprocessStdio
                    | InstalledPluginExecutionGrant::SubprocessStdioNetwork => {}
                }
                if record.manifest.source != PluginSource::LocalSubprocess {
                    return Err(JarvisError::Validation(
                        "installed plugin execution requires local_subprocess source".to_string(),
                    ));
                }
                let source_path = std::path::Path::new(&record.source_path);
                let subprocess = record.manifest.subprocess.as_ref().ok_or_else(|| {
                    JarvisError::Validation(
                        "installed plugin execution requires subprocess config".to_string(),
                    )
                })?;
                subprocess.validate(&record.id, source_path)?;
                if record.provenance.integrity_status
                    != InstalledPluginIntegrityStatus::MatchesInstallSnapshot
                {
                    return Err(JarvisError::Validation(
                        "installed plugin execution requires local provenance verification to match the install snapshot".to_string(),
                    ));
                }
            }

            repository.set_installed_plugin_execution(
                id,
                request.execution_enabled,
                request.execution_grant,
            )
        })
    }

    pub fn verify_installed_plugin_provenance(
        &self,
        id: &str,
    ) -> JarvisResult<InstalledPluginRecord> {
        self.using_repository(|repository| {
            let record = repository.verify_installed_plugin_provenance(id)?;
            let audit_entry = AuditEntry::new(
                None,
                "installed_plugin_provenance_verified",
                "installed plugin local provenance snapshot was checked",
                json!({
                    "plugin_id": record.id,
                    "manifest_version": record.manifest.version,
                    "source": record.manifest.source,
                    "source_path": record.source_path,
                    "integrity_status": record.provenance.integrity_status,
                    "origin_claim_verified": record.provenance.origin_claim_verified,
                }),
            );
            repository.append_audit_entry(&audit_entry)?;
            Ok(record)
        })
    }

    pub fn verify_installed_plugin_publisher(
        &self,
        id: &str,
        request: InstalledPluginPublisherVerificationRequest,
    ) -> JarvisResult<InstalledPluginRecord> {
        self.using_repository(|repository| {
            let trusted_origin = request.trusted_origin.trim();
            let decided_by = request.decided_by.trim();
            if trusted_origin.is_empty() {
                return Err(JarvisError::Validation(
                    "trusted_origin is required for publisher verification".to_string(),
                ));
            }
            if decided_by.is_empty() {
                return Err(JarvisError::Validation(
                    "decided_by is required for publisher verification".to_string(),
                ));
            }

            let record =
                repository.verify_installed_plugin_publisher(id, trusted_origin, Utc::now())?;
            let audit_entry = AuditEntry::new(
                None,
                "installed_plugin_publisher_verified",
                "installed plugin publisher origin claim was operator-verified",
                json!({
                    "plugin_id": record.id,
                    "manifest_version": record.manifest.version,
                    "source": record.manifest.source,
                    "origin_claim": record.provenance.origin_claim,
                    "trusted_origin": trusted_origin,
                    "decided_by": decided_by,
                    "reason": request.reason,
                    "integrity_status": record.provenance.integrity_status,
                    "origin_claim_verified": record.provenance.origin_claim_verified,
                }),
            );
            repository.append_audit_entry(&audit_entry)?;
            Ok(record)
        })
    }

    pub fn verify_installed_plugin_publisher_signature(
        &self,
        id: &str,
        request: InstalledPluginPublisherSignatureVerificationRequest,
    ) -> JarvisResult<InstalledPluginRecord> {
        self.using_repository(|repository| {
            let trusted_public_key = request.trusted_public_key.trim();
            let decided_by = request.decided_by.trim();
            if trusted_public_key.is_empty() {
                return Err(JarvisError::Validation(
                    "trusted_public_key is required for publisher signature verification"
                        .to_string(),
                ));
            }
            if decided_by.is_empty() {
                return Err(JarvisError::Validation(
                    "decided_by is required for publisher signature verification".to_string(),
                ));
            }

            let record = repository.verify_installed_plugin_publisher_signature(
                id,
                trusted_public_key,
                Utc::now(),
            )?;
            let audit_entry = AuditEntry::new(
                None,
                "installed_plugin_publisher_signature_verified",
                "installed plugin publisher signature was verified against a trusted key",
                json!({
                    "plugin_id": record.id,
                    "manifest_version": record.manifest.version,
                    "source": record.manifest.source,
                    "origin_claim": record.provenance.origin_claim,
                    "publisher_signature_scheme": record
                        .manifest
                        .publisher_signature
                        .as_ref()
                        .map(|signature| signature.scheme.as_str()),
                    "trusted_public_key_sha256": sha256_text(trusted_public_key),
                    "decided_by": decided_by,
                    "reason": request.reason,
                    "integrity_status": record.provenance.integrity_status,
                    "origin_claim_verified": record.provenance.origin_claim_verified,
                }),
            );
            repository.append_audit_entry(&audit_entry)?;
            Ok(record)
        })
    }

    pub fn pause(&self, reason: impl Into<String>) -> JarvisResult<EmergencyPauseResponse> {
        let reason = reason.into();
        let reason_present = !reason.trim().is_empty();
        let stored_pause = self.persist_pause(true, Some(&reason))?;
        let cancelled_jobs = self
            .scheduler
            .cancel_active_jobs(format!("emergency pause: {reason}"));
        for job in &cancelled_jobs {
            self.persist_scheduler_job(job)?;
        }
        self.runtime_control.emergency_pause();
        let paused_at = stored_pause
            .as_ref()
            .map(|pause| pause.updated_at)
            .unwrap_or_else(Utc::now);
        let mut pause = self
            .emergency_pause
            .lock()
            .expect("emergency pause lock poisoned");

        pause.paused = true;
        pause.reason = Some(reason);
        pause.paused_at = Some(paused_at);
        pause.resumed_at = None;

        let response = EmergencyPauseResponse {
            paused: pause.paused,
            reason: pause.reason.clone(),
            paused_at: pause.paused_at,
            resumed_at: pause.resumed_at,
            cancelled_scheduler_jobs: cancelled_jobs.len(),
        };

        self.append_scheduler_audit_entry(
            None,
            "emergency_pause_activated",
            "emergency pause activated and open scheduler jobs were cancelled",
            json!({
                "reason_present": reason_present,
                "cancelled_scheduler_jobs": response.cancelled_scheduler_jobs,
                "paused": response.paused,
            }),
        )?;

        Ok(response)
    }

    pub fn resume(&self) -> JarvisResult<EmergencyPauseResponse> {
        let stored_pause = self.persist_pause(false, None)?;
        let resumed_at = stored_pause
            .as_ref()
            .map(|pause| pause.updated_at)
            .unwrap_or_else(Utc::now);
        let mut pause = self
            .emergency_pause
            .lock()
            .expect("emergency pause lock poisoned");
        self.runtime_control.resume();
        pause.paused = false;
        pause.reason = None;
        pause.resumed_at = Some(resumed_at);

        Ok(EmergencyPauseResponse {
            paused: pause.paused,
            reason: pause.reason.clone(),
            paused_at: pause.paused_at,
            resumed_at: pause.resumed_at,
            cancelled_scheduler_jobs: 0,
        })
    }

    pub fn pause_status(&self) -> EmergencyPauseResponse {
        let pause = self.pause_snapshot();
        EmergencyPauseResponse {
            paused: pause.paused,
            reason: pause.reason,
            paused_at: pause.paused_at,
            resumed_at: pause.resumed_at,
            cancelled_scheduler_jobs: 0,
        }
    }

    fn persist_pause(
        &self,
        paused: bool,
        reason: Option<&str>,
    ) -> JarvisResult<Option<StoredEmergencyPauseState>> {
        self.repository
            .as_ref()
            .map(|repository| {
                repository
                    .lock()
                    .expect("IPC repository lock poisoned")
                    .set_emergency_pause(paused, reason, Some("ipc"))
            })
            .transpose()
    }

    pub fn schedule_scheduler_job(&self, spec: SchedulerJobSpec) -> JarvisResult<SchedulerJob> {
        let job = self.scheduler.schedule(spec)?;
        self.persist_scheduler_job(&job)?;
        Ok(job)
    }

    pub fn mark_scheduler_job_running(&self, id: Uuid) -> JarvisResult<SchedulerJob> {
        let job = self.scheduler.mark_running(id)?;
        self.persist_scheduler_transition(&job, |repository| {
            repository.mark_scheduler_job_running(id)
        })
    }

    pub fn complete_scheduler_job(&self, id: Uuid) -> JarvisResult<SchedulerJob> {
        let job = self.scheduler.complete(id)?;
        self.persist_scheduler_transition(&job, |repository| repository.complete_scheduler_job(id))
    }

    pub fn fail_scheduler_job(&self, id: Uuid) -> JarvisResult<SchedulerJob> {
        let job = self.scheduler.fail(id)?;
        self.persist_scheduler_transition(&job, |repository| repository.fail_scheduler_job(id))
    }

    pub fn cancel_scheduler_job(&self, id: Uuid, reason: &str) -> JarvisResult<SchedulerJob> {
        let job = self.scheduler.cancel(id, reason)?;
        let reason = reason.to_string();
        self.persist_scheduler_transition(&job, |repository| {
            repository.cancel_scheduler_job(id, reason)
        })
    }

    pub fn get_scheduler_job(&self, id: Uuid) -> JarvisResult<SchedulerJob> {
        self.scheduler
            .list()
            .into_iter()
            .find(|job| job.id == id)
            .ok_or_else(|| JarvisError::Storage(format!("scheduler job not found: {id}")))
    }

    pub async fn run_due_scheduler_jobs(&self, limit: usize) -> JarvisResult<SchedulerRunResponse> {
        let checked_at = Utc::now();
        let limit = limit.max(1);

        if self.runtime_control.is_emergency_paused() {
            return Ok(SchedulerRunResponse {
                checked_at,
                limit,
                emergency_paused: true,
                executions: Vec::new(),
            });
        }

        let due_jobs = self.scheduler.due_jobs(checked_at, limit);
        let mut executions = Vec::new();

        for due_job in due_jobs {
            if self.runtime_control.is_emergency_paused() {
                self.append_scheduler_audit_entry(
                    None,
                    "scheduler_run_stopped_by_emergency_pause",
                    "scheduler stopped before executing remaining due jobs because emergency pause is active",
                    json!({
                        "checked_at": checked_at,
                        "limit": limit,
                        "scheduler_job_id": due_job.id,
                    }),
                )?;
                break;
            }

            let running = self.mark_scheduler_job_running(due_job.id)?;
            self.append_scheduler_audit_entry(
                None,
                "scheduler_job_due",
                "scheduler selected due job for execution",
                json!({
                    "checked_at": checked_at,
                    "limit": limit,
                    "scheduler_job_id": running.id,
                    "scheduler_job_name": running.name,
                    "trigger": running.trigger,
                    "job_status": running.status,
                }),
            )?;
            let policy_audit =
                self.append_scheduler_proactive_policy_audit(&running, true, checked_at)?;
            let command_response = self
                .submit_command(CommandRequest {
                    input: running.command.clone(),
                    session_id: None,
                    context: json!({
                        "surface": "scheduler",
                        "scheduler_job_id": running.id,
                        "scheduler_job_name": running.name,
                        "trigger": running.trigger,
                        "sensitivity": "workspace",
                    }),
                    dry_run: false,
                    proactive: true,
                    sensitivity: Some(Sensitivity::Workspace),
                })
                .await?;

            let mut audit_entries = Vec::new();
            audit_entries.push(policy_audit);
            self.append_scheduler_execution_audit(
                "scheduler_job_started",
                "scheduler started due job command",
                &running,
                &command_response,
                &mut audit_entries,
            )?;

            let final_job = if command_response.accepted {
                if matches!(running.trigger, TriggerKind::Interval { .. }) {
                    self.reschedule_interval_scheduler_job(running.id)?
                } else {
                    self.complete_scheduler_job(running.id)?
                }
            } else {
                self.fail_scheduler_job(running.id)?
            };

            let event_type = match final_job.status {
                SchedulerJobStatus::Scheduled => "scheduler_job_rescheduled",
                SchedulerJobStatus::Completed => "scheduler_job_completed",
                SchedulerJobStatus::Failed => "scheduler_job_failed",
                SchedulerJobStatus::Cancelled => "scheduler_job_cancelled",
                SchedulerJobStatus::Running => "scheduler_job_running",
            };
            self.append_scheduler_execution_audit(
                event_type,
                "scheduler finished due job command",
                &final_job,
                &command_response,
                &mut audit_entries,
            )?;

            if !command_response.accepted {
                let pause_reason = format!(
                    "scheduler job {} did not complete accepted task",
                    final_job.id
                );
                let pause_response = self.pause(pause_reason.clone())?;
                let fail_closed_audit = self.append_scheduler_audit_entry(
                    Some(command_response.task.id),
                    "scheduler_fail_closed_emergency_pause",
                    "scheduler activated emergency pause after a due job failed closed",
                    json!({
                        "scheduler_job_id": final_job.id,
                        "scheduler_job_name": final_job.name,
                        "trigger": final_job.trigger,
                        "job_status": final_job.status,
                        "task_status": command_response.task.status,
                        "accepted": command_response.accepted,
                        "pause_reason_present": !pause_reason.is_empty(),
                        "cancelled_scheduler_jobs": pause_response.cancelled_scheduler_jobs,
                    }),
                )?;
                audit_entries.push(fail_closed_audit);
            }

            executions.push(SchedulerJobExecution {
                job: final_job,
                task: command_response.task,
                accepted: command_response.accepted,
                message: command_response.message,
                audit_entries,
            });

            if self.runtime_control.is_emergency_paused() {
                break;
            }
        }

        Ok(SchedulerRunResponse {
            checked_at,
            limit,
            emergency_paused: self.runtime_control.is_emergency_paused(),
            executions,
        })
    }

    pub fn recover_stale_scheduler_jobs(
        &self,
        older_than_seconds: u64,
        limit: usize,
    ) -> JarvisResult<SchedulerStaleRecoveryResponse> {
        self.recover_stale_scheduler_jobs_inner(older_than_seconds, limit, false)
    }

    pub fn recover_stale_scheduler_jobs_automatically(
        &self,
        older_than_seconds: u64,
        limit: usize,
    ) -> JarvisResult<SchedulerStaleRecoveryResponse> {
        self.recover_stale_scheduler_jobs_inner(older_than_seconds, limit, true)
    }

    fn recover_stale_scheduler_jobs_inner(
        &self,
        older_than_seconds: u64,
        limit: usize,
        automatic_recovery: bool,
    ) -> JarvisResult<SchedulerStaleRecoveryResponse> {
        let checked_at = Utc::now();
        let seconds = i64::try_from(older_than_seconds).map_err(|_| {
            JarvisError::Validation(
                "older_than_seconds must fit into a signed duration".to_string(),
            )
        })?;
        let limit = limit.max(1);
        let stale_jobs =
            self.scheduler
                .stale_running_jobs(checked_at, Duration::seconds(seconds), limit);
        let mut recovered = Vec::new();

        for stale_job in stale_jobs {
            let stale_since = stale_job.updated_at;
            let stale_for_seconds = checked_at
                .signed_duration_since(stale_since)
                .num_seconds()
                .max(0);
            let failed = self.fail_scheduler_job(stale_job.id)?;
            let audit_entry = self.append_scheduler_audit_entry(
                None,
                "scheduler_stale_running_recovered",
                if automatic_recovery {
                    "scheduler marked a stale running job failed during opt-in startup recovery"
                } else {
                    "scheduler marked a stale running job failed for explicit operator recovery"
                },
                json!({
                    "checked_at": checked_at,
                    "scheduler_job_id": failed.id,
                    "scheduler_job_name": failed.name,
                    "trigger": failed.trigger,
                    "job_status": failed.status,
                    "previous_status": SchedulerJobStatus::Running,
                    "stale_since": stale_since,
                    "stale_for_seconds": stale_for_seconds,
                    "older_than_seconds": older_than_seconds,
                    "command_redacted": true,
                    "automatic_recovery": automatic_recovery,
                }),
            )?;
            recovered.push(SchedulerStaleRecoveryItem {
                job: DiagnosticSchedulerJob::from(failed),
                stale_since,
                stale_for_seconds,
                audit_entry,
            });
        }

        Ok(SchedulerStaleRecoveryResponse {
            checked_at,
            older_than_seconds,
            limit,
            recovered,
        })
    }

    pub fn scheduler_attention(&self) -> SchedulerAttentionSummary {
        let generated_at = Utc::now();
        let emergency_paused = self.runtime_control.is_emergency_paused();
        let mut scheduled_count = 0;
        let mut running_count = 0;
        let mut failed_count = 0;
        let mut due_count = 0;
        let mut next_due_at: Option<DateTime<Utc>> = None;
        let mut items = Vec::new();

        for job in self.scheduler.list() {
            match job.status {
                SchedulerJobStatus::Scheduled => scheduled_count += 1,
                SchedulerJobStatus::Running => running_count += 1,
                SchedulerJobStatus::Failed => failed_count += 1,
                SchedulerJobStatus::Completed | SchedulerJobStatus::Cancelled => {}
            }

            let due_at = scheduler_job_due_at(&job);
            if let Some(candidate) = due_at.filter(|candidate| *candidate > generated_at) {
                next_due_at = Some(next_due_at.map_or(candidate, |current| current.min(candidate)));
            }

            let due = matches!(job.status, SchedulerJobStatus::Scheduled)
                && due_at.is_some_and(|candidate| candidate <= generated_at);
            if due {
                due_count += 1;
            }

            if let Some(item) = scheduler_attention_item(job, due, due_at, emergency_paused) {
                items.push(item);
            }
        }

        items.sort_by_key(|item| {
            (
                scheduler_notification_priority(&item.notification_kind),
                item.next_due_at,
                item.name.clone(),
                item.id,
            )
        });

        SchedulerAttentionSummary {
            generated_at,
            emergency_paused,
            attention_required: !items.is_empty(),
            due_count,
            scheduled_count,
            running_count,
            failed_count,
            next_due_at,
            items,
        }
    }

    pub fn spawn_scheduler_background_loop(
        &self,
        config: SchedulerBackgroundConfig,
    ) -> tokio::task::JoinHandle<()> {
        let state = self.clone();
        tokio::spawn(async move {
            state.run_scheduler_background_loop(config).await;
        })
    }

    async fn run_scheduler_background_loop(&self, config: SchedulerBackgroundConfig) {
        let mut ticker = tokio::time::interval(config.interval);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            ticker.tick().await;
            if let Err(error) = self.run_due_scheduler_jobs(config.limit).await {
                let _ = self.append_scheduler_audit_entry(
                    None,
                    "scheduler_background_tick_failed",
                    "background scheduler tick failed while running due jobs",
                    json!({
                        "limit": config.limit,
                        "error": error.to_string(),
                    }),
                );
            }
        }
    }

    pub fn reschedule_interval_scheduler_job(&self, id: Uuid) -> JarvisResult<SchedulerJob> {
        let job = self.scheduler.reschedule_interval(id)?;
        self.persist_scheduler_transition(&job, |repository| {
            repository.reschedule_interval_scheduler_job(id)
        })
    }

    fn append_scheduler_execution_audit(
        &self,
        event_type: &str,
        summary: &str,
        job: &SchedulerJob,
        command_response: &CommandResponse,
        audit_entries: &mut Vec<AuditEntry>,
    ) -> JarvisResult<()> {
        let entry = AuditEntry::new(
            Some(command_response.task.id),
            event_type,
            summary,
            json!({
                "scheduler_job_id": job.id,
                "scheduler_job_name": job.name,
                "trigger": job.trigger,
                "job_status": job.status,
                "task_status": command_response.task.status,
                "accepted": command_response.accepted,
            }),
        );
        self.command_store().append_audit_entry(&entry)?;
        audit_entries.push(entry);
        Ok(())
    }

    fn append_scheduler_proactive_policy_audit(
        &self,
        job: &SchedulerJob,
        due: bool,
        checked_at: DateTime<Utc>,
    ) -> JarvisResult<AuditEntry> {
        let classification = scheduler_policy_classification(job, due);
        self.append_scheduler_audit_entry(
            None,
            "scheduler_proactive_policy_checked",
            "scheduler checked proactive policy before submitting due job command",
            json!({
                "checked_at": checked_at,
                "scheduler_job_id": job.id,
                "scheduler_job_name": job.name,
                "trigger": job.trigger,
                "job_status": job.status,
                "due": due,
                "proactive_trigger": true,
                "command_redacted": true,
                "policy_review_item_type": classification.item_type,
                "severity": classification.severity,
                "trigger_label": classification.trigger_label,
                "side_effects_require_approval": true,
            }),
        )
    }

    fn append_scheduler_audit_entry(
        &self,
        task_id: Option<Uuid>,
        event_type: &str,
        summary: &str,
        payload: serde_json::Value,
    ) -> JarvisResult<AuditEntry> {
        let entry = AuditEntry::new(task_id, event_type, summary, payload);
        self.command_store().append_audit_entry(&entry)?;
        Ok(entry)
    }

    fn persist_scheduler_job(&self, job: &SchedulerJob) -> JarvisResult<()> {
        self.repository
            .as_ref()
            .map(|repository| {
                repository
                    .lock()
                    .expect("IPC repository lock poisoned")
                    .upsert_scheduler_job(job)
            })
            .transpose()
            .map(|_| ())
    }

    fn persist_scheduler_transition(
        &self,
        job: &SchedulerJob,
        transition: impl FnOnce(&SqliteRepository) -> JarvisResult<SchedulerJob>,
    ) -> JarvisResult<SchedulerJob> {
        match &self.repository {
            Some(repository) => {
                let repository = repository.lock().expect("IPC repository lock poisoned");
                transition(&repository)
            }
            None => Ok(job.clone()),
        }
    }

    fn pause_snapshot(&self) -> EmergencyPauseState {
        self.emergency_pause
            .lock()
            .expect("emergency pause lock poisoned")
            .clone()
    }

    fn using_repository<T>(
        &self,
        operation: impl FnOnce(&SqliteRepository) -> JarvisResult<T>,
    ) -> JarvisResult<T> {
        let repository = self.repository.as_ref().ok_or_else(|| {
            JarvisError::Storage(
                "this endpoint requires IpcState with SqliteRepository backing".to_string(),
            )
        })?;
        let repository = repository.lock().expect("IPC repository lock poisoned");
        operation(&repository)
    }
}

#[derive(Clone)]
struct SharedCommandStore {
    repository: Option<Arc<Mutex<SqliteRepository>>>,
}

#[derive(Default)]
struct PluginDispatch {
    waiting_for_approval: bool,
    results: Vec<PluginCallResult>,
    message: Option<String>,
}

struct FirstPartyPluginDispatchContext<'a> {
    input: &'a str,
    sensitivity: Sensitivity,
    dry_run: bool,
    proactive: bool,
    audit_entries: &'a mut Vec<AuditEntry>,
    command_store: &'a SharedCommandStore,
}

impl RuntimeCommandStore for SharedCommandStore {
    fn create_task(&self, session_id: Uuid, user_input: String) -> JarvisResult<TaskRecord> {
        match &self.repository {
            Some(repository) => repository
                .lock()
                .expect("IPC repository lock poisoned")
                .create_task(session_id, user_input),
            None => crate::NoopRuntimeCommandStore.create_task(session_id, user_input),
        }
    }

    fn update_task_status(&self, task: &mut TaskRecord, status: TaskStatus) -> JarvisResult<()> {
        match &self.repository {
            Some(repository) => {
                *task = repository
                    .lock()
                    .expect("IPC repository lock poisoned")
                    .update_task_status(task.id, status)?;
                Ok(())
            }
            None => crate::NoopRuntimeCommandStore.update_task_status(task, status),
        }
    }

    fn append_audit_entry(&self, entry: &AuditEntry) -> JarvisResult<()> {
        match &self.repository {
            Some(repository) => repository
                .lock()
                .expect("IPC repository lock poisoned")
                .append_audit_entry(entry),
            None => crate::NoopRuntimeCommandStore.append_audit_entry(entry),
        }
    }

    fn append_model_route_record(&self, record: &ModelRouteRecord) -> JarvisResult<()> {
        match &self.repository {
            Some(repository) => repository
                .lock()
                .expect("IPC repository lock poisoned")
                .append_model_route_record(record),
            None => crate::NoopRuntimeCommandStore.append_model_route_record(record),
        }
    }
}

fn scheduler_policy_review_item(
    job: SchedulerJob,
    now: DateTime<Utc>,
) -> PermissionPolicyReviewItem {
    let due_at = scheduler_job_due_at(&job);
    let due = matches!(job.status, SchedulerJobStatus::Scheduled)
        && due_at.is_some_and(|candidate| candidate <= now);
    let classification = scheduler_policy_classification(&job, due);
    let due_detail = if due {
        " The job is currently due."
    } else {
        ""
    };

    PermissionPolicyReviewItem {
        item_type: classification.item_type.to_string(),
        severity: classification.severity.to_string(),
        title: "Scheduler trigger is active".to_string(),
        detail: format!(
            "{} is {:?} with a {}; scheduler command text is redacted and due execution remains policy-gated.{due_detail}",
            job.name, job.status, classification.trigger_label
        ),
        approval_id: None,
        plugin_id: None,
        memory_id: None,
        action: Some(job.id.to_string()),
    }
}

struct SchedulerPolicyClassification {
    item_type: &'static str,
    severity: &'static str,
    trigger_label: String,
}

fn scheduler_policy_classification(job: &SchedulerJob, due: bool) -> SchedulerPolicyClassification {
    match job.trigger {
        TriggerKind::Manual => SchedulerPolicyClassification {
            item_type: "manual_scheduler_trigger",
            severity: "low",
            trigger_label: "manual trigger".to_string(),
        },
        TriggerKind::OnceAt { run_at } => SchedulerPolicyClassification {
            item_type: "scheduled_scheduler_trigger",
            severity: if due { "medium" } else { "low" },
            trigger_label: format!("one-time trigger at {run_at}"),
        },
        TriggerKind::Interval { every_seconds } => SchedulerPolicyClassification {
            item_type: "recurring_scheduler_trigger",
            severity: "medium",
            trigger_label: format!("recurring trigger every {every_seconds} seconds"),
        },
    }
}

fn scheduler_job_due_at(job: &SchedulerJob) -> Option<DateTime<Utc>> {
    match job.trigger {
        TriggerKind::Manual => Some(job.updated_at),
        TriggerKind::OnceAt { run_at } => Some(run_at),
        TriggerKind::Interval { every_seconds } => {
            let seconds = i64::try_from(every_seconds).ok()?;
            Some(job.updated_at + Duration::seconds(seconds))
        }
    }
}

fn scheduler_attention_item(
    job: SchedulerJob,
    due: bool,
    next_due_at: Option<DateTime<Utc>>,
    emergency_paused: bool,
) -> Option<SchedulerAttentionItem> {
    let (notification_kind, notification_reason) = match job.status {
        SchedulerJobStatus::Scheduled if due && emergency_paused => (
            "blocked_by_emergency_pause",
            "A due scheduler job is waiting, but emergency pause is active.",
        ),
        SchedulerJobStatus::Scheduled if due => (
            "due_now",
            "A scheduler job is due and ready for the app to surface.",
        ),
        SchedulerJobStatus::Running => (
            "running",
            "A scheduler job is still marked running and should remain visible.",
        ),
        SchedulerJobStatus::Failed => (
            "failed",
            "A scheduler job failed and needs review before stronger production claims.",
        ),
        SchedulerJobStatus::Scheduled
        | SchedulerJobStatus::Completed
        | SchedulerJobStatus::Cancelled => return None,
    };

    Some(SchedulerAttentionItem {
        id: job.id,
        name: job.name,
        trigger: job.trigger,
        status: job.status,
        due,
        next_due_at,
        notification_kind: notification_kind.to_string(),
        notification_reason: notification_reason.to_string(),
    })
}

fn scheduler_notification_priority(kind: &str) -> u8 {
    match kind {
        "blocked_by_emergency_pause" => 0,
        "failed" => 1,
        "due_now" => 2,
        "running" => 3,
        _ => 4,
    }
}

fn permission_review_severity_rank(severity: &str) -> u8 {
    match severity {
        "critical" => 0,
        "high" => 1,
        "medium" => 2,
        "low" => 3,
        _ => 4,
    }
}

fn memory_review_severity(sensitivity: Sensitivity) -> &'static str {
    match sensitivity {
        Sensitivity::Restricted | Sensitivity::CredentialAdjacent | Sensitivity::Private => "high",
        Sensitivity::Personal => "medium",
        Sensitivity::Workspace | Sensitivity::Public => "low",
    }
}

fn memory_sensitivity_requires_retention_review(sensitivity: Sensitivity) -> bool {
    matches!(
        sensitivity,
        Sensitivity::Private | Sensitivity::CredentialAdjacent | Sensitivity::Restricted
    )
}

fn sensitivity_from_context(context: &serde_json::Value) -> Option<Sensitivity> {
    let value = context.get("sensitivity")?.as_str()?;
    match value {
        "public" => Some(Sensitivity::Public),
        "workspace" => Some(Sensitivity::Workspace),
        "personal" => Some(Sensitivity::Personal),
        "private" => Some(Sensitivity::Private),
        "credential_adjacent" => Some(Sensitivity::CredentialAdjacent),
        "restricted" => Some(Sensitivity::Restricted),
        _ => None,
    }
}

fn parse_approval_status(value: &str) -> JarvisResult<ApprovalStatus> {
    match value {
        "pending" => Ok(ApprovalStatus::Pending),
        "approved" => Ok(ApprovalStatus::Approved),
        "denied" => Ok(ApprovalStatus::Denied),
        _ => Err(JarvisError::Validation(
            "approval status must be pending, approved, or denied".to_string(),
        )),
    }
}

fn default_decided_by() -> String {
    "cli".to_string()
}

fn first_party_plugin_request(input: &str) -> Option<PluginCallRequest> {
    let trimmed = input.trim();
    if let Some(message) = trimmed.strip_prefix("plugin approval echo ") {
        return Some(PluginCallRequest::reactive(
            "fake_echo",
            "approval_echo",
            json!({ "message": message.trim() }),
        ));
    }

    if let Some(message) = trimmed.strip_prefix("plugin echo ") {
        return Some(PluginCallRequest::reactive(
            "fake_echo",
            "echo",
            json!({ "message": message.trim() }),
        ));
    }

    if matches!(
        trimmed,
        "plugin status" | "core status" | "jarvis status" | "status"
    ) {
        return Some(PluginCallRequest::reactive(
            "fake_status",
            "status",
            json!({}),
        ));
    }

    None
}

fn validate_installed_plugin_record(record: &InstalledPluginRecord) -> JarvisResult<()> {
    if record.id != record.manifest.id {
        return Err(JarvisError::Validation(format!(
            "installed plugin id {} does not match manifest id {}",
            record.id, record.manifest.id
        )));
    }
    record.manifest.validate()?;
    if record.manifest.source == crate::PluginSource::FirstParty {
        return Err(JarvisError::Validation(format!(
            "{} installed plugins cannot claim first_party source",
            record.id
        )));
    }
    let Some(source_path) = record.manifest.source_path.as_deref() else {
        return Err(JarvisError::Validation(format!(
            "{} installed plugin requires manifest source_path",
            record.id
        )));
    };
    if source_path != record.source_path {
        return Err(JarvisError::Validation(format!(
            "{} installed plugin source_path does not match manifest source_path",
            record.id
        )));
    }
    if !std::path::Path::new(&record.source_path).is_absolute() {
        return Err(JarvisError::Validation(format!(
            "{} installed plugin source_path must be absolute",
            record.id
        )));
    }
    let canonical_source = std::fs::canonicalize(&record.source_path).map_err(|err| {
        JarvisError::Validation(format!(
            "{} installed plugin source_path is not readable: {err}",
            record.id
        ))
    })?;
    if canonical_source.display().to_string() != record.source_path {
        return Err(JarvisError::Validation(format!(
            "{} installed plugin source_path must be canonical",
            record.id
        )));
    }
    Ok(())
}

pub fn router(state: IpcState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/contract", get(contract))
        .route("/release/readiness", get(release_readiness))
        .route("/release/evidence-status", get(release_evidence_status))
        .route("/diagnostics/export", get(diagnostics_export))
        .route("/tools/model", get(model_tool_catalog))
        .route("/commands", post(command))
        .route("/tasks", get(list_tasks))
        .route("/tasks/:id", get(get_task))
        .route("/tasks/:id/audit", get(list_task_audit_entries))
        .route("/audit", get(list_audit_entries))
        .route("/activity/summary", get(activity_summary))
        .route("/activity/events", get(activity_events))
        .route("/model-routes", get(list_model_routes))
        .route("/model-routes/:id", get(get_model_route))
        .route("/memory", get(list_memory_items).post(create_memory_item))
        .route("/memory/classification", get(memory_classification_summary))
        .route(
            "/memory/:id",
            get(get_memory_item)
                .patch(update_memory_item)
                .delete(delete_memory_item),
        )
        .route("/memory/:id/review", post(review_memory_item))
        .route("/memory/:id/restore", post(restore_memory_item))
        .route("/permissions/grants", get(permission_grant_summary))
        .route("/permissions/policy-review", get(permission_policy_review))
        .route("/approvals", get(list_approvals))
        .route("/approvals/:id", get(get_approval))
        .route("/approvals/:id/approve", post(approve_approval))
        .route("/approvals/:id/deny", post(deny_approval))
        .route("/approvals/:id/execute", post(execute_approved_approval))
        .route("/plugins/manifests", get(list_plugin_manifests))
        .route("/plugins/manifests/:id", get(get_plugin_manifest))
        .route(
            "/plugins/installed",
            get(list_installed_plugins).post(install_plugin),
        )
        .route("/plugins/installed/:id", get(get_installed_plugin))
        .route(
            "/plugins/installed/:id/execution",
            post(set_installed_plugin_execution),
        )
        .route(
            "/plugins/installed/:id/provenance/verify",
            post(verify_installed_plugin_provenance),
        )
        .route(
            "/plugins/installed/:id/publisher/verify",
            post(verify_installed_plugin_publisher),
        )
        .route(
            "/plugins/installed/:id/publisher/signature/verify",
            post(verify_installed_plugin_publisher_signature),
        )
        .route("/plugins/installed/:id/run", post(run_installed_plugin))
        .route(
            "/emergency-pause",
            get(pause_status).post(pause).delete(resume),
        )
        .route(
            "/scheduler/jobs",
            get(list_scheduler_jobs).post(create_scheduler_job),
        )
        .route("/scheduler/attention", get(scheduler_attention))
        .route("/scheduler/run-due", post(run_due_scheduler_jobs))
        .route(
            "/scheduler/recover-stale",
            post(recover_stale_scheduler_jobs),
        )
        .route(
            "/scheduler/jobs/:id",
            get(get_scheduler_job).delete(cancel_scheduler_job),
        )
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

pub async fn serve(bind: SocketAddr, state: IpcState) -> anyhow::Result<()> {
    let listener = TcpListener::bind(bind).await?;
    serve_listener(listener, state).await
}

pub async fn serve_listener(listener: TcpListener, state: IpcState) -> anyhow::Result<()> {
    axum::serve(listener, router(state)).await?;
    Ok(())
}

async fn health(State(state): State<IpcState>) -> Json<HealthResponse> {
    Json(state.health())
}

async fn contract(State(state): State<IpcState>) -> Json<ContractResponse> {
    Json(state.contract())
}

async fn release_readiness(State(state): State<IpcState>) -> Json<ReleaseReadinessResponse> {
    Json(state.release_readiness())
}

async fn release_evidence_status(
    State(state): State<IpcState>,
) -> Json<ReleaseEvidenceStatusResponse> {
    Json(state.release_evidence_status())
}

async fn model_tool_catalog(
    State(state): State<IpcState>,
) -> Result<Json<ModelToolCatalogResponse>, (StatusCode, Json<ErrorResponse>)> {
    state.model_tool_catalog().map(Json).map_err(error_response)
}

async fn diagnostics_export(
    State(state): State<IpcState>,
) -> Result<Json<DiagnosticsExport>, (StatusCode, Json<ErrorResponse>)> {
    state.diagnostics_export().map(Json).map_err(error_response)
}

async fn command(
    State(state): State<IpcState>,
    Json(request): Json<CommandRequest>,
) -> Result<Json<CommandResponse>, (StatusCode, Json<ErrorResponse>)> {
    state
        .submit_command(request)
        .await
        .map(Json)
        .map_err(error_response)
}

async fn list_tasks(
    State(state): State<IpcState>,
) -> Result<Json<Vec<TaskRecord>>, (StatusCode, Json<ErrorResponse>)> {
    state
        .using_repository(SqliteRepository::list_tasks)
        .map(Json)
        .map_err(error_response)
}

async fn get_task(
    State(state): State<IpcState>,
    Path(id): Path<Uuid>,
) -> Result<Json<TaskRecord>, (StatusCode, Json<ErrorResponse>)> {
    state
        .using_repository(|repository| {
            repository
                .get_task(id)?
                .ok_or_else(|| JarvisError::Storage(format!("task not found: {id}")))
        })
        .map(Json)
        .map_err(error_response)
}

async fn list_task_audit_entries(
    State(state): State<IpcState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<AuditEntry>>, (StatusCode, Json<ErrorResponse>)> {
    state
        .using_repository(|repository| {
            if repository.get_task(id)?.is_none() {
                return Err(JarvisError::Storage(format!("task not found: {id}")));
            }
            repository.list_audit_entries(Some(id))
        })
        .map(Json)
        .map_err(error_response)
}

async fn list_audit_entries(
    State(state): State<IpcState>,
) -> Result<Json<Vec<AuditEntry>>, (StatusCode, Json<ErrorResponse>)> {
    state
        .using_repository(|repository| repository.list_audit_entries(None))
        .map(Json)
        .map_err(error_response)
}

async fn activity_summary(
    State(state): State<IpcState>,
) -> Result<Json<ActivitySummary>, (StatusCode, Json<ErrorResponse>)> {
    state.activity_summary().map(Json).map_err(error_response)
}

async fn activity_events(
    State(state): State<IpcState>,
    Query(query): Query<ActivityEventsQuery>,
) -> Result<
    Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>>,
    (StatusCode, Json<ErrorResponse>),
> {
    state.activity_summary().map_err(error_response)?;

    let interval = StdDuration::from_millis(
        query
            .interval_ms
            .unwrap_or(DEFAULT_ACTIVITY_EVENT_INTERVAL_MS)
            .max(100),
    );
    let max_events = query
        .max_events
        .unwrap_or(DEFAULT_ACTIVITY_EVENT_LIMIT)
        .clamp(1, MAX_ACTIVITY_EVENT_LIMIT);
    let stream_state = state.clone();
    let stream = FuturesStreamExt::flat_map(
        FuturesStreamExt::take(
            IntervalStream::new(tokio::time::interval(interval)),
            max_events,
        ),
        move |_| {
            let events = match stream_state.activity_summary() {
                Ok(summary) => {
                    let mut events = vec![Event::default().event("activity_summary").data(
                        serde_json::to_string(&summary).unwrap_or_else(|error| {
                            json!({
                                "error": format!("serialize activity summary: {error}")
                            })
                            .to_string()
                        }),
                    )];
                    for progress in activity_progress_events_from_summary(&summary) {
                        events.push(Event::default().event("activity_progress").data(
                            serde_json::to_string(&progress).unwrap_or_else(|error| {
                                json!({
                                    "error": format!("serialize activity progress: {error}")
                                })
                                .to_string()
                            }),
                        ));
                    }
                    events
                }
                Err(error) => vec![Event::default()
                    .event("activity_error")
                    .data(json!({ "error": error.to_string() }).to_string())],
            };
            tokio_stream::iter(events.into_iter().map(Ok))
        },
    );

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

fn activity_progress_events_from_summary(summary: &ActivitySummary) -> Vec<ActivityProgressEvent> {
    summary
        .recent_audit_entries
        .iter()
        .filter(|entry| entry.event_type == "installed_plugin_progress")
        .filter_map(activity_progress_event_from_audit)
        .collect()
}

fn activity_progress_event_from_audit(entry: &AuditEntry) -> Option<ActivityProgressEvent> {
    let payload = entry.payload.as_object()?;
    let session_id = payload
        .get("session_id")
        .and_then(serde_json::Value::as_str)
        .and_then(|value| Uuid::parse_str(value).ok());

    Some(ActivityProgressEvent {
        audit_id: entry.id,
        task_id: entry.task_id,
        created_at: entry.created_at,
        plugin_id: payload
            .get("plugin_id")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned),
        action: payload
            .get("action")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned),
        session_id,
        sequence: payload.get("sequence").and_then(serde_json::Value::as_u64),
        stage: payload
            .get("stage")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned),
        message: payload
            .get("message")
            .and_then(serde_json::Value::as_str)
            .map(ToOwned::to_owned),
        stderr_redacted: payload
            .get("stderr_redacted")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true),
    })
}

async fn list_model_routes(
    State(state): State<IpcState>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Vec<ModelRouteRecord>>, (StatusCode, Json<ErrorResponse>)> {
    let task_id = query
        .get("task_id")
        .map(|value| {
            Uuid::parse_str(value).map_err(|error| {
                JarvisError::Validation(format!("task_id must be a UUID: {error}"))
            })
        })
        .transpose()
        .map_err(error_response)?;
    state
        .using_repository(|repository| repository.list_model_route_records(task_id))
        .map(Json)
        .map_err(error_response)
}

async fn get_model_route(
    State(state): State<IpcState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ModelRouteRecord>, (StatusCode, Json<ErrorResponse>)> {
    state
        .using_repository(|repository| {
            repository
                .get_model_route_record(id)?
                .ok_or_else(|| JarvisError::Storage(format!("model route not found: {id}")))
        })
        .map(Json)
        .map_err(error_response)
}

async fn list_memory_items(
    State(state): State<IpcState>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Vec<crate::MemoryItem>>, (StatusCode, Json<ErrorResponse>)> {
    let include_deleted = query
        .get("include_deleted")
        .is_some_and(|value| value == "true" || value == "1");
    state
        .using_repository(|repository| repository.list_memory_items(include_deleted))
        .map(Json)
        .map_err(error_response)
}

async fn memory_classification_summary(
    State(state): State<IpcState>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<MemoryClassificationSummary>, (StatusCode, Json<ErrorResponse>)> {
    let include_deleted = query
        .get("include_deleted")
        .is_some_and(|value| value == "true" || value == "1");
    state
        .using_repository(|repository| repository.memory_classification_summary(include_deleted))
        .map(Json)
        .map_err(error_response)
}

async fn create_memory_item(
    State(state): State<IpcState>,
    Json(request): Json<CreateMemoryItemRequest>,
) -> Result<Json<crate::MemoryItem>, (StatusCode, Json<ErrorResponse>)> {
    state
        .using_repository(|repository| {
            repository.create_memory_item(NewMemoryItem {
                category: request.category,
                key: request.key,
                value: request.value,
                provenance: request.provenance,
                sensitivity: request.sensitivity,
            })
        })
        .map(Json)
        .map_err(error_response)
}

async fn get_memory_item(
    State(state): State<IpcState>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::MemoryItem>, (StatusCode, Json<ErrorResponse>)> {
    state
        .using_repository(|repository| {
            repository
                .get_memory_item(id)?
                .ok_or_else(|| JarvisError::Storage(format!("memory item not found: {id}")))
        })
        .map(Json)
        .map_err(error_response)
}

async fn update_memory_item(
    State(state): State<IpcState>,
    Path(id): Path<Uuid>,
    Json(request): Json<UpdateMemoryItemRequest>,
) -> Result<Json<crate::MemoryItem>, (StatusCode, Json<ErrorResponse>)> {
    state
        .using_repository(|repository| {
            repository.update_memory_item(
                id,
                request.value,
                request.provenance,
                request.sensitivity,
            )
        })
        .map(Json)
        .map_err(error_response)
}

async fn review_memory_item(
    State(state): State<IpcState>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::MemoryItem>, (StatusCode, Json<ErrorResponse>)> {
    state
        .using_repository(|repository| repository.mark_memory_reviewed(id))
        .map(Json)
        .map_err(error_response)
}

async fn delete_memory_item(
    State(state): State<IpcState>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::MemoryItem>, (StatusCode, Json<ErrorResponse>)> {
    state
        .using_repository(|repository| repository.delete_memory_item(id))
        .map(Json)
        .map_err(error_response)
}

async fn restore_memory_item(
    State(state): State<IpcState>,
    Path(id): Path<Uuid>,
) -> Result<Json<crate::MemoryItem>, (StatusCode, Json<ErrorResponse>)> {
    state
        .using_repository(|repository| repository.restore_memory_item(id))
        .map(Json)
        .map_err(error_response)
}

async fn list_approvals(
    State(state): State<IpcState>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<Vec<PendingApproval>>, (StatusCode, Json<ErrorResponse>)> {
    let status = query
        .get("status")
        .map(|value| parse_approval_status(value))
        .transpose()
        .map_err(error_response)?;
    state
        .list_approvals(status)
        .map(Json)
        .map_err(error_response)
}

async fn permission_grant_summary(
    State(state): State<IpcState>,
) -> Result<Json<PermissionGrantSummary>, (StatusCode, Json<ErrorResponse>)> {
    state
        .permission_grant_summary()
        .map(Json)
        .map_err(error_response)
}

async fn permission_policy_review(
    State(state): State<IpcState>,
) -> Result<Json<PermissionPolicyReview>, (StatusCode, Json<ErrorResponse>)> {
    state
        .permission_policy_review()
        .map(Json)
        .map_err(error_response)
}

async fn get_approval(
    State(state): State<IpcState>,
    Path(id): Path<Uuid>,
) -> Result<Json<PendingApproval>, (StatusCode, Json<ErrorResponse>)> {
    state.get_approval(id).map(Json).map_err(error_response)
}

async fn approve_approval(
    State(state): State<IpcState>,
    Path(id): Path<Uuid>,
    Json(request): Json<ApprovalDecisionRequest>,
) -> Result<Json<PendingApproval>, (StatusCode, Json<ErrorResponse>)> {
    state
        .approve_approval(id, request.decided_by, request.reason)
        .map(Json)
        .map_err(error_response)
}

async fn deny_approval(
    State(state): State<IpcState>,
    Path(id): Path<Uuid>,
    Json(request): Json<ApprovalDecisionRequest>,
) -> Result<Json<PendingApproval>, (StatusCode, Json<ErrorResponse>)> {
    state
        .deny_approval(id, request.decided_by, request.reason)
        .map(Json)
        .map_err(error_response)
}

async fn execute_approved_approval(
    State(state): State<IpcState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ApprovalExecutionResponse>, (StatusCode, Json<ErrorResponse>)> {
    state
        .execute_approved_approval(id)
        .map(Json)
        .map_err(error_response)
}

async fn list_plugin_manifests(
    State(_state): State<IpcState>,
) -> Result<Json<Vec<PluginManifest>>, (StatusCode, Json<ErrorResponse>)> {
    PluginHost::with_first_party_plugins()
        .and_then(|host| host.manifests())
        .map(Json)
        .map_err(error_response)
}

async fn get_plugin_manifest(
    State(_state): State<IpcState>,
    Path(id): Path<String>,
) -> Result<Json<PluginManifest>, (StatusCode, Json<ErrorResponse>)> {
    PluginHost::with_first_party_plugins()
        .and_then(|host| host.manifest(&id))
        .map(Json)
        .map_err(error_response)
}

fn registered_first_party_model_tool_catalog() -> JarvisResult<ModelToolCatalogResponse> {
    let host = PluginHost::with_first_party_plugins()?;
    let tools = model_tool_definitions_from_manifests(host.manifests()?);
    Ok(ModelToolCatalogResponse {
        generated_at: Utc::now(),
        source: "registered_first_party_plugins".to_string(),
        tools,
        proof_boundary:
            "Read-only model-tool catalog derived from validated first-party plugin manifests only; installed plugins, local paths, subprocess configuration, provenance hashes, audit payloads, memory values, and provider route context are excluded."
                .to_string(),
    })
}

async fn list_installed_plugins(
    State(state): State<IpcState>,
) -> Result<Json<Vec<InstalledPluginRecord>>, (StatusCode, Json<ErrorResponse>)> {
    state
        .using_repository(SqliteRepository::list_installed_plugins)
        .map(Json)
        .map_err(error_response)
}

async fn get_installed_plugin(
    State(state): State<IpcState>,
    Path(id): Path<String>,
) -> Result<Json<InstalledPluginRecord>, (StatusCode, Json<ErrorResponse>)> {
    state
        .using_repository(|repository| {
            repository
                .get_installed_plugin(&id)?
                .ok_or_else(|| JarvisError::Storage(format!("installed plugin not found: {id}")))
        })
        .map(Json)
        .map_err(error_response)
}

async fn install_plugin(
    State(state): State<IpcState>,
    Json(request): Json<InstallPluginRequest>,
) -> Result<Json<InstalledPluginRecord>, (StatusCode, Json<ErrorResponse>)> {
    let installed = InstalledPlugin::from_local_manifest_path(&request.manifest_path)
        .map_err(error_response)?;
    state
        .using_repository(|repository| repository.install_plugin_metadata(installed))
        .map(Json)
        .map_err(error_response)
}

async fn set_installed_plugin_execution(
    State(state): State<IpcState>,
    Path(id): Path<String>,
    Json(request): Json<InstalledPluginExecutionRequest>,
) -> Result<Json<InstalledPluginRecord>, (StatusCode, Json<ErrorResponse>)> {
    state
        .set_installed_plugin_execution(&id, request)
        .map(Json)
        .map_err(error_response)
}

async fn verify_installed_plugin_provenance(
    State(state): State<IpcState>,
    Path(id): Path<String>,
) -> Result<Json<InstalledPluginRecord>, (StatusCode, Json<ErrorResponse>)> {
    state
        .verify_installed_plugin_provenance(&id)
        .map(Json)
        .map_err(error_response)
}

async fn verify_installed_plugin_publisher(
    State(state): State<IpcState>,
    Path(id): Path<String>,
    Json(request): Json<InstalledPluginPublisherVerificationRequest>,
) -> Result<Json<InstalledPluginRecord>, (StatusCode, Json<ErrorResponse>)> {
    state
        .verify_installed_plugin_publisher(&id, request)
        .map(Json)
        .map_err(error_response)
}

async fn verify_installed_plugin_publisher_signature(
    State(state): State<IpcState>,
    Path(id): Path<String>,
    Json(request): Json<InstalledPluginPublisherSignatureVerificationRequest>,
) -> Result<Json<InstalledPluginRecord>, (StatusCode, Json<ErrorResponse>)> {
    state
        .verify_installed_plugin_publisher_signature(&id, request)
        .map(Json)
        .map_err(error_response)
}

async fn run_installed_plugin(
    State(state): State<IpcState>,
    Path(id): Path<String>,
    Json(request): Json<InstalledPluginRunRequest>,
) -> Result<Json<InstalledPluginRunResponse>, (StatusCode, Json<ErrorResponse>)> {
    state
        .run_installed_plugin(&id, request)
        .map(Json)
        .map_err(error_response)
}

async fn pause_status(State(state): State<IpcState>) -> Json<EmergencyPauseResponse> {
    Json(state.pause_status())
}

async fn pause(
    State(state): State<IpcState>,
    Json(request): Json<EmergencyPauseRequest>,
) -> Result<Json<EmergencyPauseResponse>, (StatusCode, Json<ErrorResponse>)> {
    state
        .pause(request.reason)
        .map(Json)
        .map_err(error_response)
}

async fn resume(
    State(state): State<IpcState>,
) -> Result<Json<EmergencyPauseResponse>, (StatusCode, Json<ErrorResponse>)> {
    state.resume().map(Json).map_err(error_response)
}

async fn list_scheduler_jobs(State(state): State<IpcState>) -> Json<Vec<SchedulerJob>> {
    Json(state.scheduler().list())
}

async fn scheduler_attention(State(state): State<IpcState>) -> Json<SchedulerAttentionSummary> {
    Json(state.scheduler_attention())
}

async fn get_scheduler_job(
    State(state): State<IpcState>,
    Path(id): Path<Uuid>,
) -> Result<Json<SchedulerJob>, (StatusCode, Json<ErrorResponse>)> {
    state
        .get_scheduler_job(id)
        .map(Json)
        .map_err(error_response)
}

async fn create_scheduler_job(
    State(state): State<IpcState>,
    Json(request): Json<CreateSchedulerJobRequest>,
) -> Result<Json<SchedulerJob>, (StatusCode, Json<ErrorResponse>)> {
    state
        .schedule_scheduler_job(SchedulerJobSpec {
            name: request.name,
            command: request.command,
            trigger: request.trigger,
        })
        .map(Json)
        .map_err(error_response)
}

async fn run_due_scheduler_jobs(
    State(state): State<IpcState>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<SchedulerRunResponse>, (StatusCode, Json<ErrorResponse>)> {
    let limit = query
        .get("limit")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(16);
    state
        .run_due_scheduler_jobs(limit)
        .await
        .map(Json)
        .map_err(error_response)
}

async fn recover_stale_scheduler_jobs(
    State(state): State<IpcState>,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Json<SchedulerStaleRecoveryResponse>, (StatusCode, Json<ErrorResponse>)> {
    let older_than_seconds = query
        .get("older_than_seconds")
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(3600);
    let limit = query
        .get("limit")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(16);
    state
        .recover_stale_scheduler_jobs(older_than_seconds, limit)
        .map(Json)
        .map_err(error_response)
}

async fn cancel_scheduler_job(
    State(state): State<IpcState>,
    Path(id): Path<Uuid>,
) -> Result<Json<SchedulerJob>, (StatusCode, Json<ErrorResponse>)> {
    state
        .cancel_scheduler_job(id, "cancelled through IPC")
        .map(Json)
        .map_err(error_response)
}

fn contract_endpoints() -> Vec<ContractEndpoint> {
    vec![
        endpoint("GET", "/health", false, true),
        endpoint("GET", "/contract", false, true),
        endpoint("GET", "/release/readiness", false, true),
        endpoint("GET", "/release/evidence-status", false, true),
        endpoint("GET", "/diagnostics/export", false, true),
        endpoint("GET", "/tools/model", false, true),
        endpoint("POST", "/commands", false, false),
        endpoint("GET", "/tasks", true, false),
        endpoint("GET", "/tasks/:id", true, false),
        endpoint("GET", "/tasks/:id/audit", true, false),
        endpoint("GET", "/audit", true, false),
        endpoint("GET", "/activity/summary", true, true),
        endpoint("GET", "/activity/events", true, true),
        endpoint("GET", "/model-routes", true, true),
        endpoint("GET", "/model-routes/:id", true, true),
        endpoint("GET", "/memory", true, false),
        endpoint("GET", "/memory/classification", true, true),
        endpoint("POST", "/memory", true, false),
        endpoint("GET", "/memory/:id", true, false),
        endpoint("PATCH", "/memory/:id", true, false),
        endpoint("DELETE", "/memory/:id", true, false),
        endpoint("POST", "/memory/:id/review", true, false),
        endpoint("POST", "/memory/:id/restore", true, false),
        endpoint("GET", "/permissions/grants", true, false),
        endpoint("GET", "/permissions/policy-review", true, false),
        endpoint("GET", "/approvals", true, false),
        endpoint("GET", "/approvals/:id", true, false),
        endpoint("POST", "/approvals/:id/approve", true, false),
        endpoint("POST", "/approvals/:id/deny", true, false),
        endpoint("POST", "/approvals/:id/execute", true, false),
        endpoint("GET", "/plugins/manifests", false, true),
        endpoint("GET", "/plugins/manifests/:id", false, true),
        endpoint("GET", "/plugins/installed", true, true),
        endpoint("POST", "/plugins/installed", true, false),
        endpoint("GET", "/plugins/installed/:id", true, true),
        endpoint("POST", "/plugins/installed/:id/execution", true, false),
        endpoint(
            "POST",
            "/plugins/installed/:id/provenance/verify",
            true,
            false,
        ),
        endpoint(
            "POST",
            "/plugins/installed/:id/publisher/verify",
            true,
            false,
        ),
        endpoint(
            "POST",
            "/plugins/installed/:id/publisher/signature/verify",
            true,
            false,
        ),
        endpoint("POST", "/plugins/installed/:id/run", true, false),
        endpoint("GET", "/emergency-pause", false, true),
        endpoint("POST", "/emergency-pause", false, false),
        endpoint("DELETE", "/emergency-pause", false, true),
        endpoint("GET", "/scheduler/jobs", false, false),
        endpoint("POST", "/scheduler/jobs", false, false),
        endpoint("GET", "/scheduler/attention", false, true),
        endpoint("POST", "/scheduler/run-due", false, false),
        endpoint("POST", "/scheduler/recover-stale", false, false),
        endpoint("GET", "/scheduler/jobs/:id", false, false),
        endpoint("DELETE", "/scheduler/jobs/:id", false, false),
    ]
}

impl From<ContractFeature> for ReleaseReadinessFeature {
    fn from(feature: ContractFeature) -> Self {
        Self {
            key: feature.key,
            status: feature.status,
            proof: feature.proof,
            boundary: feature.boundary,
        }
    }
}

fn release_readiness_features(
    evidence_status: &ReleaseEvidenceStatusResponse,
    evidence_mode_enabled: bool,
) -> Vec<ContractFeature> {
    let live_device_qa_valid =
        release_evidence_item_present(evidence_status, "live_device_qa_report")
            && evidence_mode_enabled;
    contract_features()
        .into_iter()
        .map(|mut feature| {
            if feature.key == "live_voice_loop" && live_device_qa_valid {
                feature.status = "implemented".to_string();
                feature.proof = "A valid owner-recorded live-device QA report is present through explicitly enabled release evidence status, including microphone/Speech permission prompts, spoken transcript handoff into the command path, and speech-output playback evidence.".to_string();
                feature.boundary = "Owner-recorded live-device QA evidence for the referenced release candidate only; readiness still does not perform signing, notarization, installation, Finder/LaunchServices validation, live audio capture, App Store review, marketplace review, malware analysis, or OS sandbox/egress enforcement.".to_string();
            }
            feature
        })
        .collect()
}

fn release_readiness_evidence_mode_enabled() -> bool {
    std::env::var("JARVIS_RELEASE_READINESS_EVIDENCE_MODE")
        .map(|value| value == "external")
        .unwrap_or(false)
}

fn release_evidence_item_present(status: &ReleaseEvidenceStatusResponse, key: &str) -> bool {
    status
        .items
        .iter()
        .any(|item| item.key == key && item.status == ReleaseEvidenceItemStatus::Present)
}

fn release_production_ready(
    evidence_status: &ReleaseEvidenceStatusResponse,
    evidence_mode_enabled: bool,
    no_pending_features: bool,
) -> bool {
    evidence_mode_enabled
        && no_pending_features
        && release_required_evidence_complete(evidence_status)
}

fn release_required_evidence_complete(evidence_status: &ReleaseEvidenceStatusResponse) -> bool {
    const REQUIRED_RELEASE_EVIDENCE_KEYS: &[&str] = &[
        "signed_app_bundle",
        "app_executable",
        "bundled_core_executable",
        "signed_app_zip",
        "signed_installer_package",
        "signed_distribution_provenance_report",
        "live_device_qa_report",
        "plugin_trust_qa_report",
        "release_evidence_bundle",
    ];

    evidence_status.complete
        && REQUIRED_RELEASE_EVIDENCE_KEYS
            .iter()
            .all(|key| release_evidence_item_present(evidence_status, key))
}

fn release_evidence_status_from_env() -> ReleaseEvidenceStatusResponse {
    let version = std::env::var("JARVIS_EVIDENCE_VERSION")
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string());
    let dist_dir = env_path("JARVIS_EVIDENCE_DIST_DIR", "target/distribution");
    let app_path = env_path_or("JARVIS_EVIDENCE_APP_PATH", dist_dir.join("Jarvis.app"));
    let zip_path = env_path_or(
        "JARVIS_EVIDENCE_ZIP_PATH",
        dist_dir.join(format!("Jarvis-{version}.zip")),
    );
    let pkg_path = env_path_or(
        "JARVIS_EVIDENCE_PKG_PATH",
        dist_dir.join(format!("Jarvis-{version}.pkg")),
    );
    let live_qa_report = env_path_alias(
        "JARVIS_QA_REPORT_PATH",
        "JARVIS_EVIDENCE_LIVE_QA_REPORT",
        "target/release-live-device-qa-report.json",
    );
    let plugin_qa_report = env_path_alias(
        "JARVIS_PLUGIN_QA_REPORT_PATH",
        "JARVIS_EVIDENCE_PLUGIN_QA_REPORT",
        "target/release-plugin-trust-qa-report.json",
    );
    let bundle_path = env_path(
        "JARVIS_EVIDENCE_OUTPUT_PATH",
        "target/release-evidence-bundle.json",
    );
    let signed_provenance_report = env_path_or(
        "JARVIS_EVIDENCE_SIGNED_PROVENANCE_REPORT",
        dist_dir.join(format!("Jarvis-{version}-signed-provenance.json")),
    );

    let bundle_digest_paths = ReleaseEvidenceBundleDigestPaths {
        app_path: &app_path,
        zip_path: &zip_path,
        pkg_path: &pkg_path,
        signed_provenance_report: &signed_provenance_report,
        live_qa_report: &live_qa_report,
        plugin_qa_report: &plugin_qa_report,
    };

    let mut items = vec![
        release_app_bundle_item("signed_app_bundle", "App bundle path", app_path.clone()),
        release_path_item(
            "app_executable",
            "App executable",
            app_path.join("Contents/MacOS/JarvisMacApp"),
            ReleaseEvidenceKind::Executable,
        ),
        release_bundled_core_item(
            "bundled_core_executable",
            "Bundled core executable",
            app_path.join("Contents/Resources/bin/jarvis-cli"),
        ),
        release_path_item(
            "signed_app_zip",
            "App zip path",
            zip_path.clone(),
            ReleaseEvidenceKind::File,
        ),
        release_path_item(
            "signed_installer_package",
            "Installer package path",
            pkg_path.clone(),
            ReleaseEvidenceKind::File,
        ),
        release_signed_distribution_provenance_report_item(
            "signed_distribution_provenance_report",
            "Signed-distribution provenance report",
            signed_provenance_report.clone(),
            SIGNED_DISTRIBUTION_PROVENANCE_REQUIRED_FIELDS,
            zip_path.clone(),
            pkg_path.clone(),
        ),
        release_json_report_item(
            "live_device_qa_report",
            "Live-device QA report",
            live_qa_report.clone(),
            LIVE_DEVICE_QA_REQUIRED_FIELDS,
        ),
        release_json_report_item(
            "plugin_trust_qa_report",
            "Plugin-trust QA report",
            plugin_qa_report.clone(),
            PLUGIN_TRUST_QA_REQUIRED_FIELDS,
        ),
        release_evidence_bundle_report_item(
            "release_evidence_bundle",
            "Release evidence bundle",
            bundle_path,
            RELEASE_EVIDENCE_BUNDLE_REQUIRED_FIELDS,
            bundle_digest_paths,
        ),
    ];

    if dist_dir != FsPath::new("target/distribution") {
        items.push(release_path_item(
            "distribution_directory",
            "Distribution directory",
            dist_dir,
            ReleaseEvidenceKind::Directory,
        ));
    }

    let satisfied_count = items
        .iter()
        .filter(|item| item.status == ReleaseEvidenceItemStatus::Present)
        .count();
    let missing_count = items
        .iter()
        .filter(|item| item.status == ReleaseEvidenceItemStatus::Missing)
        .count();
    let invalid_count = items
        .iter()
        .filter(|item| item.status == ReleaseEvidenceItemStatus::Invalid)
        .count();

    ReleaseEvidenceStatusResponse {
        generated_at: Utc::now(),
        complete: missing_count == 0 && invalid_count == 0,
        satisfied_count,
        missing_count,
        invalid_count,
        items,
        proof_boundary:
            "File/report inventory only; complete means expected paths are present, app bundle metadata matches the expected bundle identifier/version/build, bundled core version-marker metadata matches the expected release version, and JSON reports pass required field checks plus signed-provenance artifact digest matching, live-device QA release-metadata/non-future timestamp semantics, plugin-trust non-future timestamp semantics, and final evidence-bundle path/digest/signature-validation/non-future timestamp semantics. This endpoint does not sign, notarize, staple, install, Finder-launch, execute release artifacts, run live-device QA, run marketplace review, scan malware, or enforce an OS sandbox/egress policy."
                .to_string(),
    }
}

fn env_path(key: &str, default: &str) -> PathBuf {
    env_path_or(key, PathBuf::from(default))
}

fn env_path_alias(primary: &str, alias: &str, default: &str) -> PathBuf {
    std::env::var(primary)
        .or_else(|_| std::env::var(alias))
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(default))
}

fn env_path_or(key: &str, default: PathBuf) -> PathBuf {
    std::env::var(key).map(PathBuf::from).unwrap_or(default)
}

fn release_path_item(
    key: &str,
    label: &str,
    path: PathBuf,
    kind: ReleaseEvidenceKind,
) -> ReleaseEvidenceStatusItem {
    let (status, detail) = inspect_release_path(&path, kind);
    ReleaseEvidenceStatusItem {
        key: key.to_string(),
        label: label.to_string(),
        path: path.display().to_string(),
        kind,
        status,
        required_for_production: true,
        manual_gate: true,
        detail,
    }
}

fn release_app_bundle_item(key: &str, label: &str, path: PathBuf) -> ReleaseEvidenceStatusItem {
    let (status, detail) = inspect_release_app_bundle(&path);
    ReleaseEvidenceStatusItem {
        key: key.to_string(),
        label: label.to_string(),
        path: path.display().to_string(),
        kind: ReleaseEvidenceKind::Directory,
        status,
        required_for_production: true,
        manual_gate: true,
        detail,
    }
}

fn release_bundled_core_item(key: &str, label: &str, path: PathBuf) -> ReleaseEvidenceStatusItem {
    let (status, detail) = inspect_release_bundled_core(&path);
    ReleaseEvidenceStatusItem {
        key: key.to_string(),
        label: label.to_string(),
        path: path.display().to_string(),
        kind: ReleaseEvidenceKind::Executable,
        status,
        required_for_production: true,
        manual_gate: true,
        detail,
    }
}

fn release_json_report_item(
    key: &str,
    label: &str,
    path: PathBuf,
    required_fields: &[&str],
) -> ReleaseEvidenceStatusItem {
    let (status, detail) = inspect_release_json_report(key, &path, required_fields);
    ReleaseEvidenceStatusItem {
        key: key.to_string(),
        label: label.to_string(),
        path: path.display().to_string(),
        kind: ReleaseEvidenceKind::JsonReport,
        status,
        required_for_production: true,
        manual_gate: true,
        detail,
    }
}

fn release_signed_distribution_provenance_report_item(
    key: &str,
    label: &str,
    path: PathBuf,
    required_fields: &[&str],
    zip_path: PathBuf,
    pkg_path: PathBuf,
) -> ReleaseEvidenceStatusItem {
    let (status, detail) = inspect_release_json_report_with_artifacts(
        key,
        &path,
        required_fields,
        &zip_path,
        &pkg_path,
    );
    ReleaseEvidenceStatusItem {
        key: key.to_string(),
        label: label.to_string(),
        path: path.display().to_string(),
        kind: ReleaseEvidenceKind::JsonReport,
        status,
        required_for_production: true,
        manual_gate: true,
        detail,
    }
}

fn release_evidence_bundle_report_item(
    key: &str,
    label: &str,
    path: PathBuf,
    required_fields: &[&str],
    digest_paths: ReleaseEvidenceBundleDigestPaths<'_>,
) -> ReleaseEvidenceStatusItem {
    let (status, detail) =
        inspect_release_json_report_with_bundle_digests(key, &path, required_fields, digest_paths);
    ReleaseEvidenceStatusItem {
        key: key.to_string(),
        label: label.to_string(),
        path: path.display().to_string(),
        kind: ReleaseEvidenceKind::JsonReport,
        status,
        required_for_production: true,
        manual_gate: true,
        detail,
    }
}

#[derive(Clone, Copy)]
struct ReleaseEvidenceBundleDigestPaths<'a> {
    app_path: &'a FsPath,
    zip_path: &'a FsPath,
    pkg_path: &'a FsPath,
    signed_provenance_report: &'a FsPath,
    live_qa_report: &'a FsPath,
    plugin_qa_report: &'a FsPath,
}

fn inspect_release_path(
    path: &FsPath,
    kind: ReleaseEvidenceKind,
) -> (ReleaseEvidenceItemStatus, String) {
    const PRESENCE_ONLY_DETAIL: &str =
        "presence only; signing, notarization, and stapling are not validated by evidence-status";
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => {
            return (
                ReleaseEvidenceItemStatus::Missing,
                "expected evidence path is missing".to_string(),
            )
        }
    };

    match kind {
        ReleaseEvidenceKind::Directory if metadata.is_dir() => (
            ReleaseEvidenceItemStatus::Present,
            format!("directory exists; {PRESENCE_ONLY_DETAIL}"),
        ),
        ReleaseEvidenceKind::File if metadata.is_file() => (
            ReleaseEvidenceItemStatus::Present,
            format!("file exists; {PRESENCE_ONLY_DETAIL}"),
        ),
        ReleaseEvidenceKind::Executable if metadata.is_file() && is_executable(&metadata) => (
            ReleaseEvidenceItemStatus::Present,
            format!("executable file exists; {PRESENCE_ONLY_DETAIL}"),
        ),
        ReleaseEvidenceKind::Executable if metadata.is_file() => (
            ReleaseEvidenceItemStatus::Invalid,
            "file exists but is not executable".to_string(),
        ),
        _ => (
            ReleaseEvidenceItemStatus::Invalid,
            "path exists but has the wrong type".to_string(),
        ),
    }
}

fn inspect_release_app_bundle(path: &FsPath) -> (ReleaseEvidenceItemStatus, String) {
    let (status, detail) = inspect_release_path(path, ReleaseEvidenceKind::Directory);
    if status != ReleaseEvidenceItemStatus::Present {
        return (status, detail);
    }

    let info_plist = path.join("Contents/Info.plist");
    let contents = match fs::read_to_string(&info_plist) {
        Ok(contents) => contents,
        Err(_) => {
            return (
                ReleaseEvidenceItemStatus::Invalid,
                "app bundle Info.plist is missing or not readable as XML".to_string(),
            )
        }
    };

    let expected_bundle_id = expected_release_bundle_id();
    let expected_version = expected_release_evidence_version();
    for (key, expected) in [
        ("CFBundleIdentifier", expected_bundle_id.as_str()),
        ("CFBundleShortVersionString", expected_version.as_str()),
        ("CFBundleVersion", expected_version.as_str()),
    ] {
        match plist_xml_string_value(&contents, key) {
            Some(actual) if actual == expected => {}
            Some(actual) => {
                return (
                    ReleaseEvidenceItemStatus::Invalid,
                    format!(
                        "app bundle Info.plist {key} mismatch: expected {expected}, got {actual}"
                    ),
                )
            }
            None => {
                return (
                    ReleaseEvidenceItemStatus::Invalid,
                    format!("app bundle Info.plist missing {key}"),
                )
            }
        }
    }

    (
        ReleaseEvidenceItemStatus::Present,
        "directory exists; Info.plist bundle identifier, short version, and build version match expected release metadata; signing, notarization, and stapling are not validated by evidence-status".to_string(),
    )
}

fn inspect_release_bundled_core(path: &FsPath) -> (ReleaseEvidenceItemStatus, String) {
    let (status, detail) = inspect_release_path(path, ReleaseEvidenceKind::Executable);
    if status != ReleaseEvidenceItemStatus::Present {
        return (status, detail);
    }

    let remediation =
        "rerun ./scripts/package-distribution.sh --unsigned-launch-check for local evidence, \
         or the signed package-distribution.sh lane before final release evidence";
    let version_marker = path.with_file_name(format!(
        "{}.version",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("jarvis-cli")
    ));
    let version = match fs::read_to_string(&version_marker) {
        Ok(version) => version,
        Err(_) => {
            return (
                ReleaseEvidenceItemStatus::Invalid,
                format!("bundled core version marker is missing or not readable; {remediation}"),
            )
        }
    };
    let expected_version = format!("jarvis {}", expected_release_evidence_version());
    if version.trim() != expected_version {
        return (
            ReleaseEvidenceItemStatus::Invalid,
            format!(
                "bundled core version marker mismatch: expected {expected_version}, observed {}; {remediation}",
                version.trim()
            ),
        );
    }

    (
        ReleaseEvidenceItemStatus::Present,
        "executable file exists; bundled core version marker matches expected release version; signing, notarization, and stapling are not validated by evidence-status".to_string(),
    )
}

fn plist_xml_string_value(contents: &str, key: &str) -> Option<String> {
    let key_marker = format!("<key>{key}</key>");
    let after_key = contents.split_once(&key_marker)?.1;
    let after_string = after_key.split_once("<string>")?.1;
    let value = after_string.split_once("</string>")?.0;
    Some(value.trim().to_string())
}

fn inspect_release_json_report(
    key: &str,
    path: &FsPath,
    required_fields: &[&str],
) -> (ReleaseEvidenceItemStatus, String) {
    inspect_release_json_report_inner(key, path, required_fields, None, None)
}

fn inspect_release_json_report_with_artifacts(
    key: &str,
    path: &FsPath,
    required_fields: &[&str],
    zip_path: &FsPath,
    pkg_path: &FsPath,
) -> (ReleaseEvidenceItemStatus, String) {
    inspect_release_json_report_inner(key, path, required_fields, Some((zip_path, pkg_path)), None)
}

fn inspect_release_json_report_with_bundle_digests(
    key: &str,
    path: &FsPath,
    required_fields: &[&str],
    bundle_digest_paths: ReleaseEvidenceBundleDigestPaths<'_>,
) -> (ReleaseEvidenceItemStatus, String) {
    inspect_release_json_report_inner(key, path, required_fields, None, Some(bundle_digest_paths))
}

fn inspect_release_json_report_inner(
    key: &str,
    path: &FsPath,
    required_fields: &[&str],
    signed_artifact_paths: Option<(&FsPath, &FsPath)>,
    bundle_digest_paths: Option<ReleaseEvidenceBundleDigestPaths<'_>>,
) -> (ReleaseEvidenceItemStatus, String) {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(_) => {
            return (
                ReleaseEvidenceItemStatus::Missing,
                "expected JSON report is missing".to_string(),
            )
        }
    };
    let value: serde_json::Value = match serde_json::from_str(&contents) {
        Ok(value) => value,
        Err(error) => {
            return (
                ReleaseEvidenceItemStatus::Invalid,
                format!("JSON report is invalid: {error}"),
            )
        }
    };
    let missing = required_fields
        .iter()
        .copied()
        .filter(|field| !json_field_is_present(&value, field))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        if key == "live_device_qa_report" {
            if let Err(error) = validate_live_device_qa_report(&value) {
                return (ReleaseEvidenceItemStatus::Invalid, error);
            }
        } else if key == "plugin_trust_qa_report" {
            if let Err(error) = validate_plugin_trust_qa_report(&value) {
                return (ReleaseEvidenceItemStatus::Invalid, error);
            }
        } else if key == "signed_distribution_provenance_report" {
            if let Err(error) = validate_signed_distribution_provenance(&value) {
                return (ReleaseEvidenceItemStatus::Invalid, error);
            }
            if let Some((zip_path, pkg_path)) = signed_artifact_paths {
                if let Err(error) =
                    validate_signed_distribution_artifact_digests(&value, zip_path, pkg_path)
                {
                    return (ReleaseEvidenceItemStatus::Invalid, error);
                }
            }
        } else if key == "release_evidence_bundle" {
            if let Err(error) = validate_release_evidence_bundle(&value) {
                return (ReleaseEvidenceItemStatus::Invalid, error);
            }
            if let Some(paths) = bundle_digest_paths {
                if let Err(error) = validate_release_evidence_bundle_file_bindings(&value, paths) {
                    return (ReleaseEvidenceItemStatus::Invalid, error);
                }
            }
        }
        (
            ReleaseEvidenceItemStatus::Present,
            release_json_present_detail(key),
        )
    } else {
        (
            ReleaseEvidenceItemStatus::Invalid,
            format!(
                "JSON report is missing required fields: {}",
                missing.join(", ")
            ),
        )
    }
}

fn validate_live_device_qa_report(value: &serde_json::Value) -> Result<(), String> {
    let generated_at = require_utc_report_timestamp_not_future(value, "generated_at")?;
    if value
        .get("schema_version")
        .and_then(|schema| schema.as_i64())
        != Some(1)
    {
        return Err("JSON report schema_version must be 1".to_string());
    }
    if value.get("evidence_type").and_then(|kind| kind.as_str())
        != Some("owner_recorded_live_device_qa")
    {
        return Err("JSON report evidence_type must be owner_recorded_live_device_qa".to_string());
    }
    if value
        .get("self_test_fixture")
        .and_then(|fixture| fixture.as_bool())
        .unwrap_or(true)
    {
        return Err("JSON report must not be marked as a self-test fixture".to_string());
    }

    let expected_bundle_id = env_value_alias(
        "JARVIS_QA_EXPECTED_BUNDLE_ID",
        "JARVIS_EVIDENCE_EXPECTED_BUNDLE_ID",
        "com.nobiletechnology.jarvis",
    );
    let expected_version = expected_live_qa_version();
    require_json_string_value(value, "app_bundle.bundle_identifier", &expected_bundle_id)?;
    require_json_string_value(value, "app_bundle.short_version", &expected_version)?;
    require_json_string_value(value, "app_bundle.build_version", &expected_version)?;
    require_json_string_value(
        value,
        "installed_app_path",
        &std::env::var("JARVIS_QA_INSTALLED_APP_PATH")
            .unwrap_or_else(|_| "/Applications/Jarvis.app".to_string()),
    )?;
    for field in [
        "app_bundle.microphone_usage_description",
        "app_bundle.speech_recognition_usage_description",
        "owner_recorded_live_voice_evidence.owner_name",
        "owner_recorded_live_voice_evidence.device_label",
        "owner_recorded_live_voice_evidence.profile_label",
        "owner_recorded_live_voice_evidence.microphone_evidence_note",
        "owner_recorded_live_voice_evidence.speech_permission_evidence_note",
        "owner_recorded_live_voice_evidence.transcript_handoff_evidence_note",
        "owner_recorded_live_voice_evidence.audio_output_evidence_note",
        "voice_command_observation.test_phrase",
        "voice_command_observation.observed_transcript",
        "voice_command_observation.expected_command_text",
        "voice_command_observation.observed_command_text",
        "voice_command_observation.audio_output_device_label",
        "proof_boundary",
    ] {
        require_json_nonempty_string_value(value, field)?;
    }

    let started_at = require_utc_report_timestamp(
        value,
        "owner_recorded_live_voice_evidence.voice_check_started_at",
    )?;
    let completed_at = require_utc_report_timestamp(
        value,
        "owner_recorded_live_voice_evidence.voice_check_completed_at",
    )?;
    if completed_at < started_at {
        return Err("JSON report voice_check_completed_at must be greater than or equal to voice_check_started_at".to_string());
    }
    if generated_at < completed_at {
        return Err(
            "JSON report generated_at must be greater than or equal to voice_check_completed_at"
                .to_string(),
        );
    }

    let expected_command = json_string_at(value, "voice_command_observation.expected_command_text")
        .ok_or_else(|| {
            "JSON report is missing required field: voice_command_observation.expected_command_text"
                .to_string()
        })?;
    let observed_command = json_string_at(value, "voice_command_observation.observed_command_text")
        .ok_or_else(|| {
            "JSON report is missing required field: voice_command_observation.observed_command_text"
                .to_string()
        })?;
    let test_phrase =
        json_string_at(value, "voice_command_observation.test_phrase").ok_or_else(|| {
            "JSON report is missing required field: voice_command_observation.test_phrase"
                .to_string()
        })?;
    let observed_transcript = json_string_at(
        value,
        "voice_command_observation.observed_transcript",
    )
    .ok_or_else(|| {
        "JSON report is missing required field: voice_command_observation.observed_transcript"
            .to_string()
    })?;
    if test_phrase.trim() != observed_transcript.trim() {
        return Err(
            "JSON report observed_transcript must match test_phrase after trimming whitespace"
                .to_string(),
        );
    }
    if expected_command.trim() != observed_command.trim() {
        return Err(
            "JSON report observed_command_text must match expected_command_text".to_string(),
        );
    }
    let command_result_evidence_id = json_string_at(
        value,
        "voice_command_observation.command_result_evidence_id",
    )
    .ok_or_else(|| {
        "JSON report is missing required field: voice_command_observation.command_result_evidence_id"
            .to_string()
    })?;
    validate_command_result_evidence_id(&command_result_evidence_id)?;

    Ok(())
}

fn validate_command_result_evidence_id(value: &str) -> Result<(), String> {
    let (kind, id) = value.trim().split_once(':').ok_or_else(|| {
        "JSON report command_result_evidence_id must be task:<uuid> or audit:<uuid>".to_string()
    })?;
    if kind != "task" && kind != "audit" {
        return Err(
            "JSON report command_result_evidence_id must be task:<uuid> or audit:<uuid>"
                .to_string(),
        );
    }
    Uuid::parse_str(id).map_err(|_| {
        "JSON report command_result_evidence_id must be task:<uuid> or audit:<uuid>".to_string()
    })?;
    Ok(())
}

fn validate_plugin_trust_qa_report(value: &serde_json::Value) -> Result<(), String> {
    let generated_at = require_utc_report_timestamp_not_future(value, "generated_at")?;
    if value
        .get("schema_version")
        .and_then(|schema| schema.as_i64())
        != Some(1)
    {
        return Err("JSON report schema_version must be 1".to_string());
    }
    if value.get("evidence_type").and_then(|kind| kind.as_str())
        != Some("owner_recorded_plugin_trust_qa")
    {
        return Err("JSON report evidence_type must be owner_recorded_plugin_trust_qa".to_string());
    }
    require_json_bool_value(value, "validation_flags.egress_enforcement", true)?;
    let started_at = require_utc_report_timestamp(
        value,
        "owner_recorded_plugin_trust_evidence.review_started_at",
    )?;
    let completed_at = require_utc_report_timestamp(
        value,
        "owner_recorded_plugin_trust_evidence.review_completed_at",
    )?;
    let egress_completed_at = require_utc_report_timestamp(
        value,
        "owner_recorded_plugin_trust_evidence.egress_validation_completed_at",
    )?;
    require_json_nonempty_string_value(
        value,
        "owner_recorded_plugin_trust_evidence.egress_policy_label",
    )?;
    require_json_nonempty_string_value(
        value,
        "owner_recorded_plugin_trust_evidence.egress_deny_fixture_evidence_note",
    )?;
    require_json_nonempty_string_value(
        value,
        "owner_recorded_plugin_trust_evidence.egress_allow_fixture_evidence_note",
    )?;
    if completed_at < started_at {
        return Err(
            "JSON report review_completed_at must be greater than or equal to review_started_at"
                .to_string(),
        );
    }
    if egress_completed_at < started_at {
        return Err(
            "JSON report egress_validation_completed_at must be greater than or equal to review_started_at"
                .to_string(),
        );
    }
    if completed_at < egress_completed_at {
        return Err(
            "JSON report review_completed_at must be greater than or equal to egress_validation_completed_at"
                .to_string(),
        );
    }
    if generated_at < completed_at {
        return Err(
            "JSON report generated_at must be greater than or equal to review_completed_at"
                .to_string(),
        );
    }
    if json_string_at(value, "review_source")
        .map(|source| source == "self-test-fixture")
        .unwrap_or(false)
    {
        return Err("JSON report review_source must not be self-test-fixture".to_string());
    }

    Ok(())
}

fn validate_signed_distribution_provenance(value: &serde_json::Value) -> Result<(), String> {
    require_utc_report_timestamp_not_future(value, "generated_at")?;
    if value
        .get("schema_version")
        .and_then(|schema| schema.as_i64())
        != Some(1)
    {
        return Err("JSON report schema_version must be 1".to_string());
    }
    if value.get("evidence_type").and_then(|kind| kind.as_str())
        != Some("signed_distribution_provenance")
    {
        return Err("JSON report evidence_type must be signed_distribution_provenance".to_string());
    }
    require_json_string_value(value, "version", &expected_release_evidence_version())?;
    require_json_string_value(
        value,
        "artifacts.bundled_core_version",
        &format!("jarvis {}", expected_release_evidence_version()),
    )?;
    require_json_string_value(
        value,
        "bundle_identifier",
        &env_value_alias(
            "JARVIS_EVIDENCE_EXPECTED_BUNDLE_ID",
            "JARVIS_QA_EXPECTED_BUNDLE_ID",
            "com.nobiletechnology.jarvis",
        ),
    )?;
    for field in ["artifacts.zip_sha256", "artifacts.pkg_sha256"] {
        require_json_sha256_value(value, field)?;
    }
    for field in [
        "validation_flags.developer_id_application_signed",
        "validation_flags.developer_id_installer_signed",
        "validation_flags.app_zip_notarized",
        "validation_flags.installer_pkg_notarized",
        "validation_flags.app_stapled",
        "validation_flags.installer_pkg_stapled",
        "validation_flags.gatekeeper_assessed",
        "validation_flags.artifact_digests_recorded",
    ] {
        require_json_bool_value(value, field, true)?;
    }

    Ok(())
}

fn validate_signed_distribution_artifact_digests(
    value: &serde_json::Value,
    zip_path: &FsPath,
    pkg_path: &FsPath,
) -> Result<(), String> {
    require_json_sha256_matches_file(value, "artifacts.zip_sha256", "app zip artifact", zip_path)?;
    require_json_sha256_matches_file(
        value,
        "artifacts.pkg_sha256",
        "installer package artifact",
        pkg_path,
    )?;
    Ok(())
}

fn require_json_sha256_matches_file(
    value: &serde_json::Value,
    dotted_path: &str,
    artifact_label: &str,
    artifact_path: &FsPath,
) -> Result<(), String> {
    let expected = json_string_at(value, dotted_path)
        .ok_or_else(|| format!("JSON report is missing required field: {dotted_path}"))?;
    let actual = file_sha256(artifact_path).map_err(|error| {
        format!(
            "JSON report {dotted_path} cannot be checked because current {artifact_label} {} is unreadable: {error}",
            artifact_path.display()
        )
    })?;
    if expected == actual {
        Ok(())
    } else {
        Err(format!(
            "JSON report {dotted_path} does not match current {artifact_label} {}",
            artifact_path.display()
        ))
    }
}

fn file_sha256(path: &FsPath) -> std::io::Result<String> {
    let contents = fs::read(path)?;
    Ok(format!("{:x}", Sha256::digest(&contents)))
}

fn validate_release_evidence_bundle(value: &serde_json::Value) -> Result<(), String> {
    require_utc_report_timestamp_not_future(value, "generated_at")?;
    if value
        .get("schema_version")
        .and_then(|schema| schema.as_i64())
        != Some(1)
    {
        return Err("JSON report schema_version must be 1".to_string());
    }
    if value.get("evidence_type").and_then(|kind| kind.as_str()) != Some("release_evidence_bundle")
    {
        return Err("JSON report evidence_type must be release_evidence_bundle".to_string());
    }
    require_json_string_value(value, "version", &expected_release_evidence_version())?;
    require_json_bool_value(value, "validation_flags.local_signature_validation", true)?;
    for field in [
        "artifacts.zip_sha256",
        "artifacts.pkg_sha256",
        "reports.signed_distribution_provenance_sha256",
        "reports.live_device_qa_sha256",
        "reports.plugin_trust_qa_sha256",
    ] {
        require_json_sha256_value(value, field)?;
    }

    Ok(())
}

fn validate_release_evidence_bundle_file_bindings(
    value: &serde_json::Value,
    paths: ReleaseEvidenceBundleDigestPaths<'_>,
) -> Result<(), String> {
    require_json_string_value(
        value,
        "artifacts.app_path",
        &paths.app_path.display().to_string(),
    )?;
    require_json_string_value(
        value,
        "artifacts.zip_path",
        &paths.zip_path.display().to_string(),
    )?;
    require_json_string_value(
        value,
        "artifacts.pkg_path",
        &paths.pkg_path.display().to_string(),
    )?;
    require_json_string_value(
        value,
        "reports.signed_distribution_provenance_report",
        &paths.signed_provenance_report.display().to_string(),
    )?;
    require_json_string_value(
        value,
        "reports.live_device_qa_report",
        &paths.live_qa_report.display().to_string(),
    )?;
    require_json_string_value(
        value,
        "reports.plugin_trust_qa_report",
        &paths.plugin_qa_report.display().to_string(),
    )?;
    require_json_sha256_matches_file(
        value,
        "artifacts.zip_sha256",
        "app zip artifact",
        paths.zip_path,
    )?;
    require_json_sha256_matches_file(
        value,
        "artifacts.pkg_sha256",
        "installer package artifact",
        paths.pkg_path,
    )?;
    require_json_sha256_matches_file(
        value,
        "reports.signed_distribution_provenance_sha256",
        "signed-distribution provenance report",
        paths.signed_provenance_report,
    )?;
    require_json_sha256_matches_file(
        value,
        "reports.live_device_qa_sha256",
        "live-device QA report",
        paths.live_qa_report,
    )?;
    require_json_sha256_matches_file(
        value,
        "reports.plugin_trust_qa_sha256",
        "plugin-trust QA report",
        paths.plugin_qa_report,
    )?;
    Ok(())
}

fn env_value_alias(primary: &str, alias: &str, default: &str) -> String {
    std::env::var(primary)
        .or_else(|_| std::env::var(alias))
        .unwrap_or_else(|_| default.to_string())
}

fn expected_live_qa_version() -> String {
    std::env::var("JARVIS_QA_EXPECTED_VERSION")
        .or_else(|_| std::env::var("JARVIS_EVIDENCE_VERSION"))
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
}

fn expected_release_evidence_version() -> String {
    std::env::var("JARVIS_EVIDENCE_VERSION")
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_string())
}

fn expected_release_bundle_id() -> String {
    env_value_alias(
        "JARVIS_EVIDENCE_EXPECTED_BUNDLE_ID",
        "JARVIS_QA_EXPECTED_BUNDLE_ID",
        "com.nobiletechnology.jarvis",
    )
}

fn require_json_string_value(
    value: &serde_json::Value,
    dotted_path: &str,
    expected: &str,
) -> Result<(), String> {
    let found = json_string_at(value, dotted_path)
        .ok_or_else(|| format!("JSON report is missing required field: {dotted_path}"))?;
    if found == expected {
        Ok(())
    } else {
        Err(format!(
            "JSON report {dotted_path} mismatch: expected {expected}, got {found}"
        ))
    }
}

fn require_json_bool_value(
    value: &serde_json::Value,
    dotted_path: &str,
    expected: bool,
) -> Result<(), String> {
    let found = dotted_path
        .split('.')
        .try_fold(value, |current, key| current.get(key))
        .and_then(|found| found.as_bool())
        .ok_or_else(|| format!("JSON report is missing required boolean field: {dotted_path}"))?;
    if found == expected {
        Ok(())
    } else {
        Err(format!(
            "JSON report {dotted_path} must be {expected}, got {found}"
        ))
    }
}

fn require_json_nonempty_string_value(
    value: &serde_json::Value,
    dotted_path: &str,
) -> Result<(), String> {
    let found = json_string_at(value, dotted_path)
        .ok_or_else(|| format!("JSON report is missing required field: {dotted_path}"))?;
    if found.trim().is_empty() {
        return Err(format!("JSON report {dotted_path} must be non-empty"));
    }
    Ok(())
}

fn require_json_sha256_value(value: &serde_json::Value, dotted_path: &str) -> Result<(), String> {
    let found = json_string_at(value, dotted_path)
        .ok_or_else(|| format!("JSON report is missing required field: {dotted_path}"))?;
    let is_sha256 = found.len() == 64 && found.bytes().all(|byte| byte.is_ascii_hexdigit());
    if is_sha256 {
        Ok(())
    } else {
        Err(format!(
            "JSON report {dotted_path} must be a SHA-256 hex digest"
        ))
    }
}

fn require_utc_report_timestamp(
    value: &serde_json::Value,
    dotted_path: &str,
) -> Result<DateTime<Utc>, String> {
    let found = json_string_at(value, dotted_path)
        .ok_or_else(|| format!("JSON report is missing required field: {dotted_path}"))?;
    if !found.ends_with('Z') {
        return Err(format!(
            "JSON report {dotted_path} must be a UTC RFC3339 timestamp ending in Z"
        ));
    }
    DateTime::parse_from_rfc3339(&found)
        .map(|parsed| parsed.with_timezone(&Utc))
        .map_err(|_| format!("JSON report {dotted_path} must be a UTC RFC3339 timestamp"))
}

fn require_utc_report_timestamp_not_future(
    value: &serde_json::Value,
    dotted_path: &str,
) -> Result<DateTime<Utc>, String> {
    let timestamp = require_utc_report_timestamp(value, dotted_path)?;
    if timestamp > Utc::now() {
        return Err(format!(
            "JSON report {dotted_path} must not be later than the current time"
        ));
    }
    Ok(timestamp)
}

fn json_string_at(value: &serde_json::Value, dotted_path: &str) -> Option<String> {
    dotted_path
        .split('.')
        .try_fold(value, |current, key| current.get(key))
        .and_then(|found| found.as_str())
        .map(ToString::to_string)
}

fn release_json_present_detail(key: &str) -> String {
    match key {
        "release_evidence_bundle" => "JSON report exists, schema/evidence identity is valid, expected release version matches, artifact/report paths and SHA-256 digests match current artifacts and reports, and local signature validation is true; clean-profile, live-device, and plugin-trust claims remain owner-recorded external evidence".to_string(),
        "signed_distribution_provenance_report" => "JSON report exists, expected release version, bundle identifier, and bundled core version match, signing/notarization/stapling/Gatekeeper evidence fields are present, required flags are true, and artifact SHA-256 digests match the current zip/pkg files; clean-profile install and live-device QA remain separate manual gates".to_string(),
        "live_device_qa_report" => "JSON report exists, required owner-recorded fields and proof boundary are non-empty, installed app path, release metadata, timestamps, observed transcript, observed command text, and task/audit command evidence reference match expected values; live-device claims are still owner-recorded external evidence".to_string(),
        "plugin_trust_qa_report" => "JSON report exists, schema/evidence identity is valid, required owner-recorded fields are present, review and egress validation timestamps are valid and ordered, and deny/allow egress fixture notes are present; marketplace, malware, sandbox, and host-level egress claims remain owner-recorded external evidence".to_string(),
        _ => "JSON report exists and required owner-recorded fields are present; external claims are not revalidated by evidence-status".to_string(),
    }
}

fn json_field_is_present(value: &serde_json::Value, dotted_path: &str) -> bool {
    let Some(found) = dotted_path
        .split('.')
        .try_fold(value, |current, key| current.get(key))
    else {
        return false;
    };

    match found {
        serde_json::Value::Bool(value) => *value,
        serde_json::Value::String(value) => !value.trim().is_empty(),
        serde_json::Value::Number(_) => true,
        serde_json::Value::Array(values) => !values.is_empty(),
        serde_json::Value::Object(values) => !values.is_empty(),
        serde_json::Value::Null => false,
    }
}

#[cfg(unix)]
fn is_executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn is_executable(metadata: &fs::Metadata) -> bool {
    metadata.is_file()
}

fn release_blocking_manual_gates(
    evidence_status: &ReleaseEvidenceStatusResponse,
    evidence_mode_enabled: bool,
) -> Vec<String> {
    if evidence_mode_enabled && release_required_evidence_complete(evidence_status) {
        return Vec::new();
    }

    let live_device_qa_valid =
        release_evidence_item_present(evidence_status, "live_device_qa_report")
            && evidence_mode_enabled;
    let mut gates = vec![
        "Developer ID Application and Installer signing credentials configured and used for a full signed package run".to_string(),
        "notarization and stapling completed for both app and installer package".to_string(),
        "clean-profile installer run into /Applications".to_string(),
        "Finder/LaunchServices launch validation for the installed app".to_string(),
        "live microphone and Speech permission prompt validation plus spoken transcript handoff on a real Mac".to_string(),
        "live audio-output playback validation on a real Mac".to_string(),
        "manual clean-profile release QA pass covering installed-app command, audit, memory, scheduler, plugin, pause, diagnostics, restart behavior, and user-visible prompts".to_string(),
        "broader installed-plugin marketplace trust, malware analysis, and OS-level sandbox/egress enforcement before marketplace claims".to_string(),
        "final release evidence bundle generated and archived after signed distribution, live-device QA, and plugin-trust QA reports exist".to_string(),
    ];
    if live_device_qa_valid {
        gates.retain(|gate| {
            !gate.contains("clean-profile installer run")
                && !gate.contains("Finder/LaunchServices launch")
                && !gate.contains("live microphone")
                && !gate.contains("live audio-output")
                && !gate.contains("manual clean-profile release QA pass")
        });
    }
    gates
}

fn release_verification_commands() -> Vec<String> {
    vec![
        "./scripts/release-local.sh".to_string(),
        "./scripts/release-ci-workflow-smoke.sh".to_string(),
        "./scripts/release-operator-qa-smoke.sh".to_string(),
        "./scripts/packaged-app-release-smoke.sh".to_string(),
        "./scripts/package-distribution.sh --unsigned-launch-check".to_string(),
        "cargo run -p jarvis-cli -- release signed-distribution-runbook".to_string(),
        "JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' JARVIS_NOTARYTOOL_PROFILE='...' ./scripts/package-distribution.sh".to_string(),
        "cargo run -p jarvis-cli -- release live-device-runbook".to_string(),
        "./scripts/release-live-device-qa.sh --check".to_string(),
        "./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env".to_string(),
        "set -a && source target/release-live-device-qa.env && set +a && ./scripts/release-live-device-qa.sh --assert-complete".to_string(),
        "JARVIS_QA_CLEAN_PROFILE_VALIDATED=true JARVIS_QA_FINDER_LAUNCH_VALIDATED=true JARVIS_QA_MICROPHONE_VALIDATED=true JARVIS_QA_SPEECH_PERMISSION_VALIDATED=true JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=true JARVIS_QA_AUDIO_OUTPUT_VALIDATED=true JARVIS_QA_NOTIFICATION_VALIDATED=true JARVIS_QA_RESTART_VALIDATED=true JARVIS_QA_MANUAL_RELEASE_QA_VALIDATED=true JARVIS_QA_OWNER_NAME='Release Operator' JARVIS_QA_DEVICE_LABEL='Clean-profile release Mac' JARVIS_QA_PROFILE_LABEL='Clean macOS QA profile' JARVIS_QA_VOICE_CHECK_STARTED_AT='2026-05-22T16:00:00Z' JARVIS_QA_VOICE_CHECK_COMPLETED_AT='2026-05-22T16:05:00Z' JARVIS_QA_MICROPHONE_EVIDENCE_NOTE='Microphone prompt and capture observed' JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE='Speech prompt and recognition observed' JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE='Spoken transcript reached the command path' JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE='Speech output playback observed' JARVIS_QA_VOICE_TEST_PHRASE='Jarvis status check' JARVIS_QA_OBSERVED_TRANSCRIPT='Jarvis status check' JARVIS_QA_EXPECTED_COMMAND_TEXT='status check' JARVIS_QA_OBSERVED_COMMAND_TEXT='status check' JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID='task:<uuid-from-live-command>' JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL='Built-in speakers' ./scripts/release-live-device-qa.sh --assert-complete".to_string(),
        "./scripts/release-plugin-trust-qa.sh --check".to_string(),
        "./scripts/release-plugin-trust-qa.sh --write-template target/release-plugin-trust-qa.env".to_string(),
        "set -a && source target/release-plugin-trust-qa.env && set +a && ./scripts/release-plugin-trust-qa.sh --assert-complete".to_string(),
        "JARVIS_PLUGIN_QA_MARKETPLACE_REVIEW_VALIDATED=true JARVIS_PLUGIN_QA_MALWARE_SCAN_VALIDATED=true JARVIS_PLUGIN_QA_OS_SANDBOX_VALIDATED=true JARVIS_PLUGIN_QA_EGRESS_ENFORCEMENT_VALIDATED=true JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_POLICY_VALIDATED=true JARVIS_PLUGIN_QA_MANUAL_TRUST_REVIEW_VALIDATED=true JARVIS_PLUGIN_QA_OWNER_NAME='Release Operator' JARVIS_PLUGIN_QA_REVIEW_STARTED_AT='2026-05-22T16:10:00Z' JARVIS_PLUGIN_QA_REVIEW_COMPLETED_AT='2026-05-22T16:20:00Z' JARVIS_PLUGIN_QA_MARKETPLACE_EVIDENCE_NOTE='Marketplace review evidence archived' JARVIS_PLUGIN_QA_MALWARE_SCAN_EVIDENCE_NOTE='Malware scan evidence archived' JARVIS_PLUGIN_QA_OS_SANDBOX_EVIDENCE_NOTE='OS sandbox validation evidence archived' JARVIS_PLUGIN_QA_EGRESS_EVIDENCE_NOTE='Host-level egress validation evidence archived' JARVIS_PLUGIN_QA_EGRESS_POLICY_LABEL='Host egress policy/profile reviewed' JARVIS_PLUGIN_QA_EGRESS_VALIDATION_COMPLETED_AT='2026-05-22T16:18:00Z' JARVIS_PLUGIN_QA_EGRESS_DENY_FIXTURE_EVIDENCE_NOTE='Undeclared-host deny fixture evidence archived' JARVIS_PLUGIN_QA_EGRESS_ALLOW_FIXTURE_EVIDENCE_NOTE='Declared-host allow fixture evidence archived' JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_EVIDENCE_NOTE='Signed publisher policy evidence archived' JARVIS_PLUGIN_QA_MANUAL_REVIEW_EVIDENCE_NOTE='Manual plugin trust review evidence archived' ./scripts/release-plugin-trust-qa.sh --assert-complete".to_string(),
        "./scripts/release-evidence-bundle.sh --check".to_string(),
        "./scripts/release-evidence-bundle.sh --write-template target/release-evidence-bundle.env".to_string(),
        "./scripts/release-evidence-doctor.sh --check".to_string(),
        "set -a && source target/release-evidence-bundle.env && set +a && ./scripts/release-evidence-bundle.sh --bundle".to_string(),
        "JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true JARVIS_EVIDENCE_NOTARIZATION_VALIDATED=true JARVIS_EVIDENCE_CLEAN_PROFILE_VALIDATED=true JARVIS_EVIDENCE_LIVE_DEVICE_QA_VALIDATED=true JARVIS_EVIDENCE_PLUGIN_TRUST_QA_VALIDATED=true JARVIS_EVIDENCE_REPORTS_ARCHIVED=true ./scripts/release-evidence-bundle.sh --bundle".to_string(),
        "./scripts/release-evidence-doctor.sh --assert-complete".to_string(),
        "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release readiness".to_string(),
    ]
}

fn contract_features() -> Vec<ContractFeature> {
    vec![
        feature(
            "repository_state",
            "implemented",
            "SQLite-backed task, audit, model-route, memory, scheduler, approval, and installed-plugin state is covered by Rust unit tests and local IPC E2E.",
            "Local repository evidence only; no hosted sync or multi-device state claim.",
        ),
        feature(
            "activity_events",
            "implemented",
            "Repository-backed `/activity/events` exposes bounded redacted task metadata, audit event batches, and redacted installed-plugin progress frames and is covered by CLI IPC E2E.",
            "This is bounded state polling over SSE from audit evidence; activity recent tasks omit command bodies and this is not per-token model streaming or unbounded plugin-internal progress streaming.",
        ),
        feature(
            "scheduler_attention",
            "implemented",
            "Repository-backed scheduler jobs expose redacted `/scheduler/attention` due/running/failed handoff, explicit run-due execution, background loop tests, and CLI IPC E2E.",
            "Visibility and app-notification handoff only; scheduler attention does not grant proactive plugin execution, and live OS notification delivery remains a manual release gate.",
        ),
        feature(
            "scheduler_trigger_policy_review",
            "implemented",
            "Active scheduler triggers appear in `/permissions/policy-review` without scheduler command text; due execution emits `scheduler_proactive_policy_checked` using the same trigger classification, and proactive plugin call requests require manifest opt-in plus `proactive_run` permission.",
            "Review visibility only for scheduler policy review; due-run audit and plugin opt-in enforcement are local-only, scheduler command bodies remain redacted, proactive plugin requests that are not opted in fail closed, and live OS notification delivery remains a manual release gate.",
        ),
        feature(
            "scheduler_stale_running_recovery",
            "implemented",
            "Explicit `/scheduler/recover-stale` plus opt-in startup recovery mark stale running jobs failed with redacted audit evidence and are covered by Rust unit plus CLI IPC E2E tests.",
            "Bounded local stale-job cleanup only; no default background recovery or distributed lease claim.",
        ),
        feature(
            "memory_policy_review",
            "implemented",
            "Unreviewed memory items and deleted sensitive retained memory appear in `/permissions/policy-review` with redacted values, and diagnostics export exposes only aggregate memory review counts.",
            "Review visibility and retention-risk surfacing only; no autonomous memory rewrite, purge automation, or vector-index governance claim.",
        ),
        feature(
            "approval_execution",
            "implemented",
            "Approved first-party actions execute through `/approvals/:id/execute` after action/scope verification and are covered by Rust unit and CLI IPC E2E tests.",
            "Explicit replay only for first-party plugin commands; grant/deny remains side-effect-free and this is not broad autonomous execution.",
        ),
        feature(
            "model_tool_catalog_grounding",
            "implemented",
            "`/tools/model` exposes the redacted registered first-party model-tool catalog, Ollama prompts use the same JSON allowlist, ChatGPT/OpenAI-compatible tool schemas are derived from the same catalog, and invalid model-planned plugin IDs/actions are rejected before policy or execution with registered-tool audit guidance and CLI IPC E2E coverage.",
            "First-party model-tool grounding only; installed plugins are excluded from model planning, and this is not broad third-party tool execution, marketplace trust, malware analysis, or OS-level sandboxing.",
        ),
        feature(
            "installed_plugin_execution",
            "implemented",
            "Local subprocess plugins require full source-tree provenance verification plus explicit subprocess_stdio or subprocess_stdio_network grants, run with inherited environment cleared, enforce stdout/stderr byte limits, and are covered by Rust unit and CLI IPC E2E tests.",
            "Constrained local subprocess execution only; not a WASM, OS-level, or marketplace sandbox.",
        ),
        feature(
            "plugin_publisher_signature",
            "implemented",
            "Installed plugin manifests can verify an Ed25519 publisher signature against an explicit trusted public key with audit evidence.",
            "Trusted-key verification only; not marketplace approval, malware analysis, or reputation service trust.",
        ),
        feature(
            "plugin_network_governance",
            "implemented",
            "Network-capable plugin actions must declare exact allowed hosts, appear in permission policy review, and require the explicit subprocess_stdio_network execution grant.",
            "Runtime grant gate plus manifest governance only; not OS-level network sandbox enforcement or host-level egress filtering.",
        ),
        feature(
            "packaged_app_smoke",
            "implemented",
            "Local packaged-app smoke assembles and ad-hoc signs Jarvis.app, launches it with a temp profile, and verifies supervised core health and recovery paths.",
            "Local ad-hoc proof only; Developer ID signing, notarization, installer, Finder, App Store, and entitlement validation remain manual/distribution gates.",
        ),
        feature(
            "operator_release_qa_smoke",
            "implemented",
            "`release-operator-qa-smoke.sh` exercises repository-backed command, audit, route, memory, scheduler, activity, permission, diagnostics, pause, readiness, and restart paths in one local QA lane.",
            "Local CLI/operator QA evidence only; not clean-profile installed-app QA, Finder/LaunchServices validation, live voice/audio validation, live notification delivery, notarization, or marketplace trust.",
        ),
        feature(
            "release_ci_gate",
            "implemented",
            "`.github/workflows/release-local.yml` runs `./scripts/release-local.sh` on macOS for pull requests, pushes to main, and manual dispatch; `release-ci-workflow-smoke.sh` is part of the local gate and verifies the workflow remains wired to the canonical release script.",
            "Public CI evidence for the repo-owned local release gate only; it does not perform Developer ID signing, notarization, clean-profile installation, Finder/LaunchServices validation, live-device QA, plugin-trust QA, malware review, or OS sandbox enforcement.",
        ),
        feature(
            "unsigned_distribution_launch",
            "implemented",
            "`package-distribution.sh --unsigned-launch-check` builds the release app layout, creates an unsigned installer payload, launches the release-built app executable with isolated HOME, and verifies bundled-core IPC smoke.",
            "Unsigned distribution-layout proof only; not Developer ID signing, notarization, stapling, /Applications install, Finder/LaunchServices validation, live device validation, App Store review, or manual QA.",
        ),
        feature(
            "release_evidence_status",
            "implemented",
            "`/release/evidence-status` and `jarvis release evidence-status` expose structured present, missing, or invalid status for standard signed artifacts, QA reports, and final evidence bundle paths, including app bundle metadata matching, bundled core version-marker matching, signed-provenance artifact digest matching, live-device QA bundle/version/non-future timestamp checks, plugin-trust non-future timestamp checks, and final evidence-bundle path/digest/signature-validation/non-future timestamp checks, with Rust, CLI E2E, and Swift model coverage.",
            "Read-only file/report inventory plus report semantic validation only; it does not sign, notarize, install, Finder-launch, run live-device QA, review marketplace trust, scan malware, or enforce OS sandboxing.",
        ),
        feature(
            "release_evidence_bundle",
            "implemented",
            "`release-evidence-bundle.sh --check`, `--write-template`, `--self-test`, and `release-evidence-doctor.sh --check` are part of the release evidence workflow; `--bundle` validates signed/stapled artifact references, live-device QA bundle metadata, plugin-trust QA flags and owner evidence fields, and writes SHA-256-bound evidence manifest entries.",
            "Evidence-bundle mechanics, local artifact/report validation, and release-evidence inventory only; production readiness still depends on owner-recorded external signing, notarization, live-device QA, plugin-trust QA, and archived evidence.",
        ),
        feature(
            "live_voice_loop",
            "pending_manual_validation",
            "Swift voice input and speech-output adapters have deterministic fake-adapter tests, including final transcript staging and opt-in final-transcript auto-submit into the text command path.",
            "Live microphone, Speech permission, spoken transcript handoff, live audio output, and device validation are not proven by automated tests.",
        ),
    ]
}

fn contract_compatibility() -> ContractCompatibility {
    ContractCompatibility {
        minimum_supported_version: 1,
        current_version: IPC_CONTRACT_VERSION,
        additive_changes_allowed: true,
        breaking_change_policy:
            "Breaking IPC response-shape changes require a contract version bump and a release-note migration entry."
                .to_string(),
        deprecation_policy:
            "Deprecated endpoints remain listed in /contract for at least one minor release and must include a replacement before removal."
                .to_string(),
        client_requirements: vec![
            "Clients must ignore unknown JSON fields.".to_string(),
            "Clients must prefer /contract endpoint and feature metadata over hard-coded readiness assumptions.".to_string(),
            "Clients must treat missing required endpoints or unsupported contract versions as degraded mode.".to_string(),
            "Clients must not infer production readiness from feature presence without reading each feature boundary.".to_string(),
        ],
        removed_endpoints: Vec::new(),
        deprecated_endpoints: Vec::new(),
    }
}

fn feature(
    key: impl Into<String>,
    status: impl Into<String>,
    proof: impl Into<String>,
    boundary: impl Into<String>,
) -> ContractFeature {
    ContractFeature {
        key: key.into(),
        status: status.into(),
        proof: proof.into(),
        boundary: boundary.into(),
    }
}

fn sha256_text(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    format!("{digest:x}")
}

fn installed_plugin_grant_allows_action(
    grant: InstalledPluginExecutionGrant,
    action_requires_network_grant: bool,
) -> bool {
    match (grant, action_requires_network_grant) {
        (InstalledPluginExecutionGrant::SubprocessStdioNetwork, true) => true,
        (InstalledPluginExecutionGrant::SubprocessStdio, false) => true,
        (InstalledPluginExecutionGrant::MetadataOnly, _)
        | (InstalledPluginExecutionGrant::SubprocessStdio, true)
        | (InstalledPluginExecutionGrant::SubprocessStdioNetwork, false) => false,
    }
}

fn endpoint(
    method: impl Into<String>,
    path: impl Into<String>,
    repository_required: bool,
    redacted: bool,
) -> ContractEndpoint {
    ContractEndpoint {
        method: method.into(),
        path: path.into(),
        repository_required,
        redacted,
    }
}

fn error_response(error: JarvisError) -> (StatusCode, Json<ErrorResponse>) {
    let status = match error {
        JarvisError::Validation(_) => StatusCode::BAD_REQUEST,
        JarvisError::PolicyBlocked(_) => StatusCode::FORBIDDEN,
        JarvisError::ApprovalRequired(_) => StatusCode::ACCEPTED,
        JarvisError::Model(_) => StatusCode::BAD_GATEWAY,
        JarvisError::Storage(_) | JarvisError::Plugin(_) | JarvisError::Other(_) => {
            StatusCode::INTERNAL_SERVER_ERROR
        }
    };

    (
        status,
        Json(ErrorResponse {
            error: error.to_string(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn command_index(commands: &[String], expected: &str) -> usize {
        commands
            .iter()
            .position(|command| command == expected)
            .unwrap_or_else(|| panic!("missing command: {expected}"))
    }

    fn command_index_containing(commands: &[String], expected: &str) -> usize {
        commands
            .iter()
            .position(|command| command.contains(expected))
            .unwrap_or_else(|| panic!("missing command containing: {expected}"))
    }

    #[cfg(unix)]
    fn write_executable_plugin_script(dir: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;

        let script = dir.join("plugin-runner.py");
        std::fs::write(
            &script,
            r#"#!/usr/bin/env python3
import json
import os
import sys

request = json.load(sys.stdin)
json.dump({
    "path": request["input"]["path"],
    "secret_seen": "JARVIS_SECRET_LEAK_TEST" in os.environ,
    "plugin_id": os.environ.get("JARVIS_PLUGIN_ID"),
    "plugin_action": os.environ.get("JARVIS_PLUGIN_ACTION")
}, sys.stdout)
"#,
        )
        .expect("write plugin runner");
        let mut permissions = std::fs::metadata(&script)
            .expect("script metadata")
            .permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&script, permissions).expect("chmod plugin runner");
    }

    #[cfg(unix)]
    fn write_progress_plugin_script(dir: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;

        let script = dir.join("plugin-runner.py");
        std::fs::write(
            &script,
            r#"#!/usr/bin/env python3
import json
import sys

request = json.load(sys.stdin)
print('{"jarvis_progress":true,"stage":"prepare","message":"validated request"}', file=sys.stderr)
print('raw stderr secret should stay redacted', file=sys.stderr)
print('{"jarvis_progress":true,"stage":"complete","message":"writing validated output","payload":{"ignored":"not exposed"}}', file=sys.stderr)
json.dump({"path": request["input"]["path"]}, sys.stdout)
"#,
        )
        .expect("write progress plugin runner");
        let mut permissions = std::fs::metadata(&script)
            .expect("script metadata")
            .permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&script, permissions).expect("chmod plugin runner");
    }

    #[test]
    fn health_reports_pause_and_scheduler_counts() {
        let state = IpcState::new();
        state
            .scheduler()
            .schedule(SchedulerJobSpec {
                name: "daily".to_string(),
                command: "review calendar".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("schedule");

        let health = state.health();
        assert_eq!(health.status, "ok");
        assert_eq!(health.contract.name, IPC_CONTRACT_NAME);
        assert_eq!(health.contract.version, IPC_CONTRACT_VERSION);
        assert_eq!(health.scheduler_jobs, 1);
        assert!(!health.emergency_paused);
        assert_eq!(health.emergency_pause_reason, None);
        assert_eq!(health.emergency_pause_updated_at, None);
        assert_eq!(
            health.command_runtime,
            "routed-fake-local-model+first-party-plugins"
        );
    }

    #[test]
    fn contract_endpoint_documents_safe_inspection_paths() {
        let state = IpcState::new();
        let contract = state.contract();

        assert_eq!(contract.contract.name, IPC_CONTRACT_NAME);
        assert_eq!(contract.contract.version, IPC_CONTRACT_VERSION);
        assert_eq!(contract.compatibility.minimum_supported_version, 1);
        assert_eq!(contract.compatibility.current_version, IPC_CONTRACT_VERSION);
        assert!(contract.compatibility.additive_changes_allowed);
        assert!(contract
            .compatibility
            .client_requirements
            .iter()
            .any(|requirement| requirement.contains("ignore unknown JSON fields")));
        assert!(contract.compatibility.deprecated_endpoints.is_empty());
        assert!(contract.compatibility.removed_endpoints.is_empty());
        assert!(contract.features.iter().any(|feature| {
            feature.key == "scheduler_trigger_policy_review"
                && feature.status == "implemented"
                && feature.boundary.contains("Review visibility only")
        }));
        assert!(contract.features.iter().any(|feature| {
            feature.key == "live_voice_loop" && feature.status == "pending_manual_validation"
        }));
        assert!(contract
            .safe_inspection_paths
            .contains(&"/release/readiness".to_string()));
        assert!(contract
            .safe_inspection_paths
            .contains(&"/release/evidence-status".to_string()));
        assert!(contract
            .safe_inspection_paths
            .contains(&"/diagnostics/export".to_string()));
        assert!(contract
            .safe_inspection_paths
            .contains(&"/tools/model".to_string()));
        assert!(contract
            .safe_inspection_paths
            .contains(&"/plugins/manifests/:id".to_string()));
        assert!(contract
            .safe_inspection_paths
            .contains(&"/plugins/installed/:id".to_string()));
        assert!(contract
            .safe_inspection_paths
            .contains(&"/memory/classification".to_string()));
        assert!(!contract
            .safe_inspection_paths
            .contains(&"/memory".to_string()));
        assert!(!contract
            .safe_inspection_paths
            .contains(&"/memory/:id".to_string()));
        assert!(!contract
            .safe_inspection_paths
            .contains(&"/plugins/installed/:id/execution".to_string()));
        assert!(contract
            .endpoints
            .iter()
            .any(|endpoint| endpoint.method == "GET"
                && endpoint.path == "/release/readiness"
                && !endpoint.repository_required
                && endpoint.redacted));
        assert!(contract
            .endpoints
            .iter()
            .any(|endpoint| endpoint.method == "GET"
                && endpoint.path == "/release/evidence-status"
                && !endpoint.repository_required
                && endpoint.redacted));
        assert!(contract
            .endpoints
            .iter()
            .any(|endpoint| endpoint.method == "GET"
                && endpoint.path == "/tools/model"
                && !endpoint.repository_required
                && endpoint.redacted));
        assert!(contract
            .endpoints
            .iter()
            .any(|endpoint| endpoint.method == "GET"
                && endpoint.path == "/memory/classification"
                && endpoint.repository_required
                && endpoint.redacted));
        assert!(contract
            .endpoints
            .iter()
            .any(|endpoint| endpoint.method == "GET"
                && endpoint.path == "/memory"
                && endpoint.repository_required
                && !endpoint.redacted));
        assert!(contract
            .endpoints
            .iter()
            .any(|endpoint| endpoint.method == "GET"
                && endpoint.path == "/memory/:id"
                && endpoint.repository_required
                && !endpoint.redacted));
        assert!(contract
            .endpoints
            .iter()
            .any(|endpoint| endpoint.method == "GET"
                && endpoint.path == "/scheduler/jobs/:id"
                && !endpoint.repository_required));
        assert!(contract
            .endpoints
            .iter()
            .any(|endpoint| endpoint.method == "POST"
                && endpoint.path == "/plugins/installed/:id/provenance/verify"
                && endpoint.repository_required));
        assert!(contract
            .endpoints
            .iter()
            .any(|endpoint| endpoint.method == "POST"
                && endpoint.path == "/plugins/installed/:id/publisher/verify"
                && endpoint.repository_required));
        assert!(contract
            .endpoints
            .iter()
            .any(|endpoint| endpoint.method == "POST"
                && endpoint.path == "/plugins/installed/:id/publisher/signature/verify"
                && endpoint.repository_required));
        assert!(contract
            .endpoints
            .iter()
            .any(|endpoint| endpoint.method == "POST"
                && endpoint.path == "/plugins/installed/:id/run"
                && endpoint.repository_required));
        assert!(contract
            .safe_inspection_paths
            .contains(&"/activity/summary".to_string()));
        assert!(contract
            .safe_inspection_paths
            .contains(&"/activity/events".to_string()));
        assert!(contract
            .endpoints
            .iter()
            .any(|endpoint| endpoint.method == "GET"
                && endpoint.path == "/activity/summary"
                && endpoint.repository_required
                && endpoint.redacted));
        assert!(contract
            .endpoints
            .iter()
            .any(|endpoint| endpoint.method == "GET"
                && endpoint.path == "/activity/events"
                && endpoint.repository_required
                && endpoint.redacted));
    }

    #[test]
    fn release_readiness_summarizes_blockers_without_claiming_production_ready() {
        let state = IpcState::new();
        let readiness = state.release_readiness();
        let commands = &readiness.recommended_verification_commands;

        assert!(!readiness.production_ready);
        assert!(readiness
            .readiness_scope
            .contains("local Rust/CLI foundation"));
        assert!(readiness.verified_feature_count > 0);
        assert!(readiness.pending_feature_count > 0);
        assert!(readiness
            .implemented_features
            .iter()
            .any(|feature| feature.key == "installed_plugin_execution"));
        assert!(readiness
            .implemented_features
            .iter()
            .any(|feature| feature.key == "release_evidence_bundle"
                && feature.proof.contains("SHA-256-bound evidence manifest")
                && feature.boundary.contains("owner-recorded external")));
        assert!(readiness
            .implemented_features
            .iter()
            .any(|feature| feature.key == "release_evidence_status"
                && feature.proof.contains("/release/evidence-status")
                && feature.boundary.contains("file/report inventory")));
        assert!(readiness
            .implemented_features
            .iter()
            .any(|feature| feature.key == "release_ci_gate"
                && feature
                    .proof
                    .contains(".github/workflows/release-local.yml")
                && feature.boundary.contains("Public CI evidence")));
        assert!(readiness
            .implemented_features
            .iter()
            .any(|feature| feature.key == "scheduler_stale_running_recovery"
                && feature.proof.contains("opt-in startup recovery")
                && feature.boundary.contains("no default background recovery")));
        assert!(readiness
            .pending_features
            .iter()
            .any(|feature| feature.key == "live_voice_loop"
                && feature.status == "pending_manual_validation"));
        assert!(readiness
            .blocking_manual_gates
            .iter()
            .any(|gate| gate.contains("Developer ID")));
        assert!(readiness
            .blocking_manual_gates
            .iter()
            .any(|gate| gate.contains("live microphone")));
        assert!(readiness
            .blocking_manual_gates
            .iter()
            .any(|gate| gate.contains("final release evidence bundle")));
        assert!(readiness
            .recommended_verification_commands
            .iter()
            .any(|command| command == "./scripts/release-local.sh"));
        assert!(readiness
            .recommended_verification_commands
            .iter()
            .any(|command| command == "./scripts/release-ci-workflow-smoke.sh"));
        assert!(readiness
            .recommended_verification_commands
            .iter()
            .any(|command| command == "./scripts/release-live-device-qa.sh --check"));
        assert!(readiness
            .recommended_verification_commands
            .iter()
            .any(|command| command == "cargo run -p jarvis-cli -- release live-device-runbook"));
        assert!(readiness
            .recommended_verification_commands
            .iter()
            .any(|command| command
                == "./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env"));
        assert!(readiness
            .recommended_verification_commands
            .iter()
            .any(
                |command| command.contains("JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=true")
                    && command.contains("JARVIS_QA_OWNER_NAME=")
                    && command.contains("JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE=")
                    && command.contains("JARVIS_QA_EXPECTED_COMMAND_TEXT=")
                    && command.contains("JARVIS_QA_OBSERVED_COMMAND_TEXT=")
                    && command.contains("./scripts/release-live-device-qa.sh --assert-complete")
            ));
        assert!(readiness
            .recommended_verification_commands
            .iter()
            .any(|command| command == "./scripts/release-plugin-trust-qa.sh --check"));
        assert!(readiness
            .recommended_verification_commands
            .iter()
            .any(|command| command
                == "./scripts/release-plugin-trust-qa.sh --write-template target/release-plugin-trust-qa.env"));
        assert!(readiness
            .recommended_verification_commands
            .iter()
            .any(|command| command
                == "set -a && source target/release-plugin-trust-qa.env && set +a && ./scripts/release-plugin-trust-qa.sh --assert-complete"));
        assert!(readiness
            .recommended_verification_commands
            .iter()
            .any(|command| command.contains("JARVIS_PLUGIN_QA_OWNER_NAME=")
                && command.contains("JARVIS_PLUGIN_QA_EGRESS_EVIDENCE_NOTE=")
                && command.contains("JARVIS_PLUGIN_QA_EGRESS_DENY_FIXTURE_EVIDENCE_NOTE=")
                && command.contains("JARVIS_PLUGIN_QA_EGRESS_ALLOW_FIXTURE_EVIDENCE_NOTE=")
                && command.contains("./scripts/release-plugin-trust-qa.sh --assert-complete")));
        assert!(readiness
            .recommended_verification_commands
            .iter()
            .any(|command| command == "./scripts/release-evidence-bundle.sh --check"));
        assert!(readiness
            .recommended_verification_commands
            .iter()
            .any(|command| command
                == "./scripts/release-evidence-bundle.sh --write-template target/release-evidence-bundle.env"));
        assert!(readiness
            .recommended_verification_commands
            .iter()
            .any(|command| command
                == "set -a && source target/release-evidence-bundle.env && set +a && ./scripts/release-evidence-bundle.sh --bundle"));
        assert!(readiness
            .recommended_verification_commands
            .iter()
            .any(|command| command == "./scripts/release-evidence-doctor.sh --check"));
        assert!(readiness
            .recommended_verification_commands
            .iter()
            .any(|command| command == "./scripts/release-evidence-doctor.sh --assert-complete"));
        let unsigned_distribution_index = command_index(
            commands,
            "./scripts/package-distribution.sh --unsigned-launch-check",
        );
        let workflow_smoke_index =
            command_index(commands, "./scripts/release-ci-workflow-smoke.sh");
        let operator_qa_index = command_index(commands, "./scripts/release-operator-qa-smoke.sh");
        let signed_distribution_index = command_index(
            commands,
            "JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' JARVIS_NOTARYTOOL_PROFILE='...' ./scripts/package-distribution.sh",
        );
        let live_device_runbook_index = command_index(
            commands,
            "cargo run -p jarvis-cli -- release live-device-runbook",
        );
        let live_device_check_index =
            command_index(commands, "./scripts/release-live-device-qa.sh --check");
        let live_device_template_index = command_index(
            commands,
            "./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env",
        );
        let plugin_trust_assert_index = command_index(
            commands,
            "set -a && source target/release-plugin-trust-qa.env && set +a && ./scripts/release-plugin-trust-qa.sh --assert-complete",
        );
        let evidence_bundle_source_index = command_index(
            commands,
            "set -a && source target/release-evidence-bundle.env && set +a && ./scripts/release-evidence-bundle.sh --bundle",
        );
        let evidence_bundle_inline_index = command_index_containing(
            commands,
            "JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true",
        );
        let evidence_doctor_assert_index = command_index(
            commands,
            "./scripts/release-evidence-doctor.sh --assert-complete",
        );
        let external_readiness_index = command_index(
            commands,
            "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release readiness",
        );
        assert!(workflow_smoke_index < operator_qa_index);
        assert!(unsigned_distribution_index < signed_distribution_index);
        assert!(signed_distribution_index < live_device_runbook_index);
        assert!(live_device_runbook_index < live_device_check_index);
        assert!(live_device_check_index < live_device_template_index);
        assert!(plugin_trust_assert_index < evidence_bundle_source_index);
        assert!(evidence_bundle_source_index < evidence_doctor_assert_index);
        assert!(evidence_bundle_inline_index < evidence_doctor_assert_index);
        assert!(evidence_doctor_assert_index < external_readiness_index);
        assert!(readiness
            .proof_boundary
            .contains("does not perform signing"));
    }

    #[test]
    fn release_readiness_requires_explicit_evidence_mode_for_live_voice_completion() {
        let evidence_status = release_evidence_status_fixture(ReleaseEvidenceItemStatus::Present);

        let default_features = release_readiness_features(&evidence_status, false);
        assert!(default_features.iter().any(|feature| {
            feature.key == "live_voice_loop" && feature.status == "pending_manual_validation"
        }));
        let default_gates = release_blocking_manual_gates(&evidence_status, false);
        assert!(default_gates
            .iter()
            .any(|gate| gate.contains("live microphone")));

        let evidence_features = release_readiness_features(&evidence_status, true);
        assert!(evidence_features.iter().any(|feature| {
            feature.key == "live_voice_loop"
                && feature.status == "implemented"
                && feature
                    .proof
                    .contains("valid owner-recorded live-device QA report")
                && feature
                    .boundary
                    .contains("Owner-recorded live-device QA evidence")
        }));
        assert!(!evidence_features
            .iter()
            .any(|feature| feature.key == "live_voice_loop"
                && feature.status == "pending_manual_validation"));
        let evidence_gates = release_blocking_manual_gates(&evidence_status, true);
        assert!(!evidence_gates
            .iter()
            .any(|gate| gate.contains("live microphone")));
        assert!(!evidence_gates
            .iter()
            .any(|gate| gate.contains("live audio-output")));
        assert!(evidence_gates
            .iter()
            .any(|gate| gate.contains("Developer ID")));
        assert!(evidence_gates
            .iter()
            .any(|gate| gate.contains("final release evidence bundle")));
    }

    #[test]
    fn release_readiness_keeps_live_voice_pending_when_evidence_is_missing_or_invalid() {
        for status in [
            ReleaseEvidenceItemStatus::Missing,
            ReleaseEvidenceItemStatus::Invalid,
        ] {
            let evidence_status = release_evidence_status_fixture(status);
            let features = release_readiness_features(&evidence_status, true);
            assert!(
                features.iter().any(|feature| {
                    feature.key == "live_voice_loop"
                        && feature.status == "pending_manual_validation"
                }),
                "live voice should stay pending for {status:?}"
            );
            let gates = release_blocking_manual_gates(&evidence_status, true);
            assert!(
                gates.iter().any(|gate| gate.contains("live microphone")),
                "live microphone gate should remain for {status:?}"
            );
        }
    }

    #[test]
    fn release_readiness_production_ready_requires_explicit_complete_evidence() {
        let complete_evidence = release_complete_evidence_status_fixture();

        assert!(!release_production_ready(&complete_evidence, false, true));
        assert!(!release_production_ready(&complete_evidence, true, false));
        assert!(release_production_ready(&complete_evidence, true, true));
        assert!(release_blocking_manual_gates(&complete_evidence, true).is_empty());

        let missing_evidence = release_evidence_status_fixture(ReleaseEvidenceItemStatus::Present);
        assert!(!release_production_ready(&missing_evidence, true, true));
        assert!(release_blocking_manual_gates(&missing_evidence, true)
            .iter()
            .any(|gate| gate.contains("final release evidence bundle")));

        let invalid_evidence = release_complete_evidence_status_fixture_with_item_status(
            "plugin_trust_qa_report",
            ReleaseEvidenceItemStatus::Invalid,
        );
        assert!(!release_production_ready(&invalid_evidence, true, true));
        assert!(release_blocking_manual_gates(&invalid_evidence, true)
            .iter()
            .any(|gate| gate.contains("marketplace trust")));
    }

    #[test]
    fn release_evidence_status_reports_missing_and_invalid_evidence_without_manual_claims() {
        let missing_path = PathBuf::from("target/jarvis-test-missing-release-report.json");
        let invalid_path = tempfile::NamedTempFile::new().expect("temp invalid report");
        std::fs::write(invalid_path.path(), "{not-json").expect("write invalid json");

        let (missing_status, missing_detail) = inspect_release_json_report(
            "generic_report",
            &missing_path,
            &["validation_flags.clean_profile"],
        );
        let (invalid_status, invalid_detail) = inspect_release_json_report(
            "generic_report",
            invalid_path.path(),
            &["validation_flags.clean_profile"],
        );

        assert_eq!(missing_status, ReleaseEvidenceItemStatus::Missing);
        assert!(missing_detail.contains("missing"));
        assert_eq!(invalid_status, ReleaseEvidenceItemStatus::Invalid);
        assert!(invalid_detail.contains("invalid"));

        let state = IpcState::new();
        let status = state.release_evidence_status();
        assert!(!status.proof_boundary.contains("production ready"));
        assert!(status.proof_boundary.contains("live-device QA"));
        assert!(status
            .proof_boundary
            .contains("non-future timestamp semantics"));
        assert!(status.proof_boundary.contains("plugin-trust"));
        assert!(status.proof_boundary.contains("does not sign"));
        assert!(status.proof_boundary.contains("app bundle metadata"));
        assert!(status.items.iter().any(|item| {
            item.key == "release_evidence_bundle"
                && item.kind == ReleaseEvidenceKind::JsonReport
                && item.required_for_production
                && item.manual_gate
        }));
    }

    #[test]
    fn release_evidence_app_bundle_requires_matching_info_plist_metadata() {
        let temp_dir = tempfile::tempdir().expect("temp app bundle");
        let app_dir = temp_dir.path().join("Jarvis.app");
        let contents_dir = app_dir.join("Contents");
        std::fs::create_dir_all(&contents_dir).expect("create Contents dir");

        let (missing_status, missing_detail) = inspect_release_app_bundle(&app_dir);
        assert_eq!(missing_status, ReleaseEvidenceItemStatus::Invalid);
        assert!(missing_detail.contains("Info.plist"));

        let info_plist = contents_dir.join("Info.plist");
        std::fs::write(
            &info_plist,
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key>
  <string>com.nobiletechnology.jarvis</string>
  <key>CFBundleShortVersionString</key>
  <string>{}</string>
  <key>CFBundleVersion</key>
  <string>{}</string>
</dict>
</plist>
"#,
                env!("CARGO_PKG_VERSION"),
                env!("CARGO_PKG_VERSION")
            ),
        )
        .expect("write matching Info.plist");

        let (present_status, present_detail) = inspect_release_app_bundle(&app_dir);
        assert_eq!(present_status, ReleaseEvidenceItemStatus::Present);
        assert!(present_detail.contains("Info.plist bundle identifier"));
        assert!(present_detail.contains("not validated by evidence-status"));

        std::fs::write(
            &info_plist,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key>
  <string>com.example.StaleJarvis</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.4</string>
  <key>CFBundleVersion</key>
  <string>0.1.4</string>
</dict>
</plist>
"#,
        )
        .expect("write mismatched Info.plist");

        let (invalid_status, invalid_detail) = inspect_release_app_bundle(&app_dir);
        assert_eq!(invalid_status, ReleaseEvidenceItemStatus::Invalid);
        assert!(invalid_detail.contains("CFBundleIdentifier mismatch"));
    }

    #[test]
    #[cfg(unix)]
    fn release_evidence_bundled_core_requires_matching_version_marker() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = tempfile::tempdir().expect("temp bundled core");
        let core_path = temp_dir.path().join("jarvis-cli");
        std::fs::write(&core_path, "#!/bin/sh\nexit 0\n").expect("write bundled core");
        let mut permissions = std::fs::metadata(&core_path)
            .expect("core metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&core_path, permissions).expect("chmod bundled core");

        let (missing_status, missing_detail) = inspect_release_bundled_core(&core_path);
        assert_eq!(missing_status, ReleaseEvidenceItemStatus::Invalid);
        assert!(missing_detail.contains("version marker"));
        assert!(missing_detail.contains("package-distribution.sh --unsigned-launch-check"));

        std::fs::write(
            core_path.with_file_name("jarvis-cli.version"),
            "jarvis 0.0.0\n",
        )
        .expect("write stale version marker");
        let (stale_status, stale_detail) = inspect_release_bundled_core(&core_path);
        assert_eq!(stale_status, ReleaseEvidenceItemStatus::Invalid);
        assert!(stale_detail.contains("version marker mismatch"));
        assert!(stale_detail.contains("package-distribution.sh --unsigned-launch-check"));

        std::fs::write(
            core_path.with_file_name("jarvis-cli.version"),
            format!("jarvis {}\n", env!("CARGO_PKG_VERSION")),
        )
        .expect("write matching version marker");
        let (present_status, present_detail) = inspect_release_bundled_core(&core_path);
        assert_eq!(present_status, ReleaseEvidenceItemStatus::Present);
        assert!(present_detail.contains("version marker matches expected release version"));
        assert!(present_detail.contains("not validated by evidence-status"));
    }

    #[test]
    #[cfg(unix)]
    fn release_evidence_path_presence_details_do_not_claim_signature_validation() {
        use std::os::unix::fs::PermissionsExt;

        let temp_dir = tempfile::tempdir().expect("temp release evidence path dir");
        let app_dir = temp_dir.path().join("Jarvis.app");
        let app_zip = temp_dir.path().join("Jarvis.app.zip");
        let executable = temp_dir.path().join("jarvis");
        std::fs::create_dir(&app_dir).expect("create app dir");
        std::fs::write(&app_zip, "placeholder zip").expect("write app zip");
        std::fs::write(&executable, "#!/bin/sh\nexit 0\n").expect("write executable");
        let mut permissions = std::fs::metadata(&executable)
            .expect("executable metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&executable, permissions).expect("chmod executable");

        for (path, kind) in [
            (&app_dir, ReleaseEvidenceKind::Directory),
            (&app_zip, ReleaseEvidenceKind::File),
            (&executable, ReleaseEvidenceKind::Executable),
        ] {
            let (status, detail) = inspect_release_path(path, kind);
            assert_eq!(status, ReleaseEvidenceItemStatus::Present);
            assert!(detail.contains("presence only"), "{detail}");
            assert!(detail.contains("signing"), "{detail}");
            assert!(detail.contains("notarization"), "{detail}");
            assert!(detail.contains("stapling"), "{detail}");
            assert!(
                detail.contains("not validated by evidence-status"),
                "{detail}"
            );
        }
    }

    #[test]
    fn release_evidence_json_report_details_distinguish_presence_from_revalidation() {
        let live_detail = release_json_present_detail("live_device_qa_report");
        assert!(live_detail.contains("required owner-recorded fields"));
        assert!(live_detail.contains("installed app path"));
        assert!(live_detail.contains("owner-recorded external evidence"));

        let bundle_detail = release_json_present_detail("release_evidence_bundle");
        assert!(bundle_detail.contains("expected release version"));
        assert!(bundle_detail.contains("SHA-256"));
        assert!(bundle_detail.contains("local signature validation"));
        assert!(bundle_detail.contains("owner-recorded external evidence"));

        let plugin_detail = release_json_present_detail("plugin_trust_qa_report");
        assert!(plugin_detail.contains("egress validation timestamps"));
        assert!(plugin_detail.contains("deny/allow egress fixture notes"));
        assert!(plugin_detail.contains("marketplace"));
        assert!(plugin_detail.contains("owner-recorded external evidence"));
    }

    fn valid_live_device_qa_report_json() -> serde_json::Value {
        json!({
            "schema_version": 1,
            "evidence_type": "owner_recorded_live_device_qa",
            "self_test_fixture": false,
            "generated_at": "2026-05-22T16:06:00Z",
            "installed_app_path": "/Applications/Jarvis.app",
            "validation_flags": {
                "clean_profile": true,
                "finder_launch": true,
                "microphone": true,
                "speech_permission": true,
                "transcript_handoff": true,
                "audio_output": true,
                "notification": true,
                "restart": true,
                "manual_release_qa": true
            },
            "voice_loop": {
                "microphone_permission_prompt": true,
                "speech_permission_prompt": true,
                "spoken_transcript_handoff": true,
                "same_command_path": true,
                "speech_output_playback": true
            },
            "app_bundle": {
                "bundle_identifier": "com.nobiletechnology.jarvis",
                "short_version": "0.1.4",
                "build_version": "0.1.4",
                "microphone_usage_description": "fixture",
                "speech_recognition_usage_description": "fixture"
            },
            "owner_recorded_live_voice_evidence": {
                "owner_name": "Release Operator",
                "device_label": "Clean-profile release Mac",
                "profile_label": "Clean macOS QA profile",
                "voice_check_started_at": "2026-05-22T16:00:00Z",
                "voice_check_completed_at": "2026-05-22T16:05:00Z",
                "microphone_evidence_note": "Microphone prompt observed.",
                "speech_permission_evidence_note": "Speech prompt observed.",
                "transcript_handoff_evidence_note": "Transcript handoff observed.",
                "audio_output_evidence_note": "Audio output observed."
            },
            "voice_command_observation": {
                "test_phrase": "Jarvis status check.",
                "observed_transcript": "Jarvis status check.",
                "expected_command_text": "status check",
                "observed_command_text": "status check",
                "command_result_evidence_id": "task:00000000-0000-4000-8000-000000000001",
                "audio_output_device_label": "Built-in speakers"
            },
            "proof_boundary": "Owner-recorded live-device QA fixture."
        })
    }

    fn inspect_live_device_qa_report_value(
        value: serde_json::Value,
    ) -> (ReleaseEvidenceItemStatus, String) {
        let report_path = tempfile::NamedTempFile::new().expect("temp live QA report");
        std::fs::write(
            report_path.path(),
            serde_json::to_string_pretty(&value).expect("serialize live QA fixture"),
        )
        .expect("write live QA fixture");

        inspect_release_json_report(
            "live_device_qa_report",
            report_path.path(),
            LIVE_DEVICE_QA_REQUIRED_FIELDS,
        )
    }

    fn valid_plugin_trust_qa_report_json() -> serde_json::Value {
        json!({
            "schema_version": 1,
            "evidence_type": "owner_recorded_plugin_trust_qa",
            "generated_at": "2026-05-22T16:21:00Z",
            "review_source": "owner-asserted-manual-review",
            "validation_flags": {
                "marketplace_review": true,
                "malware_scan": true,
                "os_sandbox": true,
                "egress_enforcement": true,
                "signed_publisher_policy": true,
                "manual_trust_review": true
            },
            "owner_recorded_plugin_trust_evidence": {
                "owner_name": "Release Operator",
                "review_started_at": "2026-05-22T16:10:00Z",
                "review_completed_at": "2026-05-22T16:20:00Z",
                "marketplace_evidence_note": "Marketplace review evidence archived.",
                "malware_scan_evidence_note": "Malware scan evidence archived.",
                "os_sandbox_evidence_note": "OS sandbox validation evidence archived.",
                "egress_evidence_note": "Host-level egress validation evidence archived.",
                "egress_policy_label": "Host egress policy/profile reviewed.",
                "egress_validation_completed_at": "2026-05-22T16:18:00Z",
                "egress_deny_fixture_evidence_note": "Undeclared-host deny fixture evidence archived.",
                "egress_allow_fixture_evidence_note": "Declared-host allow fixture evidence archived.",
                "signed_publisher_evidence_note": "Signed publisher policy evidence archived.",
                "manual_review_evidence_note": "Manual plugin trust review evidence archived."
            },
            "proof_boundary": "Owner-recorded plugin trust fixture."
        })
    }

    fn inspect_plugin_trust_qa_report_value(
        value: serde_json::Value,
    ) -> (ReleaseEvidenceItemStatus, String) {
        let report_path = tempfile::NamedTempFile::new().expect("temp plugin QA report");
        std::fs::write(
            report_path.path(),
            serde_json::to_string_pretty(&value).expect("serialize plugin QA fixture"),
        )
        .expect("write plugin QA fixture");

        inspect_release_json_report(
            "plugin_trust_qa_report",
            report_path.path(),
            PLUGIN_TRUST_QA_REQUIRED_FIELDS,
        )
    }

    fn valid_release_evidence_bundle_json() -> serde_json::Value {
        let digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        json!({
            "schema_version": 1,
            "evidence_type": "release_evidence_bundle",
            "generated_at": "2026-05-22T17:00:00Z",
            "version": "0.1.4",
            "artifacts": {
                "app_path": "target/distribution/Jarvis.app",
                "zip_path": "target/distribution/Jarvis-0.1.4.zip",
                "pkg_path": "target/distribution/Jarvis-0.1.4.pkg",
                "zip_sha256": digest,
                "pkg_sha256": digest,
                "bundled_core_version": "jarvis 0.1.4"
            },
            "reports": {
                "signed_distribution_provenance_report": "target/distribution/Jarvis-0.1.4-signed-provenance.json",
                "live_device_qa_report": "target/release-live-device-qa-report.json",
                "plugin_trust_qa_report": "target/release-plugin-trust-qa-report.json",
                "signed_distribution_provenance_sha256": digest,
                "live_device_qa_sha256": digest,
                "plugin_trust_qa_sha256": digest
            },
            "validation_flags": {
                "signed_distribution": true,
                "notarization": true,
                "clean_profile": true,
                "live_device_qa": true,
                "plugin_trust_qa": true,
                "reports_archived": true,
                "local_signature_validation": true
            },
            "proof_boundary": "Evidence bundle fixture."
        })
    }

    fn inspect_release_evidence_bundle_value(
        value: serde_json::Value,
    ) -> (ReleaseEvidenceItemStatus, String) {
        let report_path = tempfile::NamedTempFile::new().expect("temp evidence bundle");
        std::fs::write(
            report_path.path(),
            serde_json::to_string_pretty(&value).expect("serialize evidence bundle fixture"),
        )
        .expect("write evidence bundle fixture");

        inspect_release_json_report(
            "release_evidence_bundle",
            report_path.path(),
            RELEASE_EVIDENCE_BUNDLE_REQUIRED_FIELDS,
        )
    }

    fn future_timestamp() -> String {
        (Utc::now() + Duration::days(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string()
    }

    #[test]
    fn live_device_qa_report_accepts_semantically_valid_owner_report() {
        let (status, detail) =
            inspect_live_device_qa_report_value(valid_live_device_qa_report_json());
        assert_eq!(status, ReleaseEvidenceItemStatus::Present);
        assert!(detail.contains("installed app path"), "{detail}");
    }

    #[test]
    fn live_device_qa_report_rejects_self_test_fixture_identity() {
        let mut report = valid_live_device_qa_report_json();
        report["self_test_fixture"] = json!(true);
        let (status, detail) = inspect_live_device_qa_report_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("self-test fixture"));
    }

    #[test]
    fn live_device_qa_report_rejects_wrong_bundle_identifier() {
        let mut report = valid_live_device_qa_report_json();
        report["app_bundle"]["bundle_identifier"] = json!("com.example.StaleJarvis");
        let (status, detail) = inspect_live_device_qa_report_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("app_bundle.bundle_identifier"), "{detail}");
    }

    #[test]
    fn live_device_qa_report_rejects_wrong_short_version() {
        let mut report = valid_live_device_qa_report_json();
        report["app_bundle"]["short_version"] = json!("9.9.9");
        let (status, detail) = inspect_live_device_qa_report_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("app_bundle.short_version"), "{detail}");
    }

    #[test]
    fn live_device_qa_report_rejects_wrong_build_version() {
        let mut report = valid_live_device_qa_report_json();
        report["app_bundle"]["build_version"] = json!("9.9.9");
        let (status, detail) = inspect_live_device_qa_report_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("app_bundle.build_version"), "{detail}");
    }

    #[test]
    fn live_device_qa_report_rejects_wrong_installed_app_path() {
        let mut report = valid_live_device_qa_report_json();
        report["installed_app_path"] = json!("/tmp/Jarvis.app");
        let (status, detail) = inspect_live_device_qa_report_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("installed_app_path"), "{detail}");
    }

    #[test]
    fn live_device_qa_report_rejects_bad_started_timestamp() {
        let mut report = valid_live_device_qa_report_json();
        report["owner_recorded_live_voice_evidence"]["voice_check_started_at"] =
            json!("not-a-timestamp");
        let (status, detail) = inspect_live_device_qa_report_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("voice_check_started_at"), "{detail}");
    }

    #[test]
    fn live_device_qa_report_rejects_bad_completed_timestamp() {
        let mut report = valid_live_device_qa_report_json();
        report["owner_recorded_live_voice_evidence"]["voice_check_completed_at"] =
            json!("2026-05-22T16:05:00-04:00");
        let (status, detail) = inspect_live_device_qa_report_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("voice_check_completed_at"), "{detail}");
    }

    #[test]
    fn live_device_qa_report_rejects_reversed_timestamps() {
        let mut report = valid_live_device_qa_report_json();
        report["owner_recorded_live_voice_evidence"]["voice_check_started_at"] =
            json!("2026-05-22T16:05:00Z");
        report["owner_recorded_live_voice_evidence"]["voice_check_completed_at"] =
            json!("2026-05-22T16:00:00Z");
        let (status, detail) = inspect_live_device_qa_report_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("voice_check_completed_at"), "{detail}");
    }

    #[test]
    fn live_device_qa_report_rejects_generated_before_completion() {
        let mut report = valid_live_device_qa_report_json();
        report["generated_at"] = json!("2026-05-22T16:04:00Z");
        let (status, detail) = inspect_live_device_qa_report_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("generated_at"), "{detail}");
    }

    #[test]
    fn live_device_qa_report_rejects_future_generated_timestamp() {
        let mut report = valid_live_device_qa_report_json();
        report["generated_at"] = json!(future_timestamp());
        let (status, detail) = inspect_live_device_qa_report_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("generated_at"), "{detail}");
        assert!(detail.contains("current time"), "{detail}");
    }

    #[test]
    fn live_device_qa_report_rejects_mismatched_command_observation() {
        let mut report = valid_live_device_qa_report_json();
        report["voice_command_observation"]["observed_command_text"] = json!("different command");
        let (status, detail) = inspect_live_device_qa_report_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("observed_command_text"), "{detail}");
    }

    #[test]
    fn live_device_qa_report_rejects_mismatched_observed_transcript() {
        let mut report = valid_live_device_qa_report_json();
        report["voice_command_observation"]["observed_transcript"] = json!("Jarvis stats check.");
        let (status, detail) = inspect_live_device_qa_report_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("observed_transcript"), "{detail}");
    }

    #[test]
    fn live_device_qa_report_rejects_malformed_command_result_evidence_id() {
        let mut report = valid_live_device_qa_report_json();
        report["voice_command_observation"]["command_result_evidence_id"] = json!("looked good");
        let (status, detail) = inspect_live_device_qa_report_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("command_result_evidence_id"), "{detail}");
    }

    #[test]
    fn live_device_qa_report_rejects_blank_owner_evidence_values() {
        for (field, path) in [
            (
                "owner_recorded_live_voice_evidence.audio_output_evidence_note",
                [
                    "owner_recorded_live_voice_evidence",
                    "audio_output_evidence_note",
                ],
            ),
            (
                "voice_command_observation.audio_output_device_label",
                ["voice_command_observation", "audio_output_device_label"],
            ),
            (
                "voice_command_observation.expected_command_text",
                ["voice_command_observation", "expected_command_text"],
            ),
            ("proof_boundary", ["proof_boundary", ""]),
        ] {
            let mut report = valid_live_device_qa_report_json();
            if path[1].is_empty() {
                report[path[0]] = json!("   ");
            } else {
                report[path[0]][path[1]] = json!("   ");
            }
            let (status, detail) = inspect_live_device_qa_report_value(report);
            assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
            assert!(detail.contains(field), "{field}: {detail}");
            assert!(
                detail.contains("must be non-empty") || detail.contains("missing required fields"),
                "{field}: {detail}"
            );
        }
    }

    #[test]
    fn plugin_trust_qa_report_accepts_semantically_valid_owner_report() {
        let (status, detail) =
            inspect_plugin_trust_qa_report_value(valid_plugin_trust_qa_report_json());
        assert_eq!(status, ReleaseEvidenceItemStatus::Present);
        assert!(detail.contains("egress validation timestamps"), "{detail}");
    }

    #[test]
    fn plugin_trust_qa_report_rejects_blank_egress_deny_fixture_note() {
        let mut report = valid_plugin_trust_qa_report_json();
        report["owner_recorded_plugin_trust_evidence"]["egress_deny_fixture_evidence_note"] =
            json!("   ");
        let (status, detail) = inspect_plugin_trust_qa_report_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(
            detail.contains("egress_deny_fixture_evidence_note"),
            "{detail}"
        );
    }

    #[test]
    fn plugin_trust_qa_report_rejects_wrong_schema_identity() {
        let mut report = valid_plugin_trust_qa_report_json();
        report["schema_version"] = json!(2);
        let (status, detail) = inspect_plugin_trust_qa_report_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("schema_version"), "{detail}");

        let mut report = valid_plugin_trust_qa_report_json();
        report["evidence_type"] = json!("self_test_fixture");
        let (status, detail) = inspect_plugin_trust_qa_report_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("evidence_type"), "{detail}");
    }

    #[test]
    fn plugin_trust_qa_report_rejects_blank_egress_allow_fixture_note() {
        let mut report = valid_plugin_trust_qa_report_json();
        report["owner_recorded_plugin_trust_evidence"]["egress_allow_fixture_evidence_note"] =
            json!("   ");
        let (status, detail) = inspect_plugin_trust_qa_report_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(
            detail.contains("egress_allow_fixture_evidence_note"),
            "{detail}"
        );
    }

    #[test]
    fn plugin_trust_qa_report_rejects_false_egress_enforcement_flag() {
        let mut report = valid_plugin_trust_qa_report_json();
        report["validation_flags"]["egress_enforcement"] = json!(false);
        let (status, detail) = inspect_plugin_trust_qa_report_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("egress_enforcement"), "{detail}");
    }

    #[test]
    fn plugin_trust_qa_report_rejects_egress_completion_after_review_completion() {
        let mut report = valid_plugin_trust_qa_report_json();
        report["owner_recorded_plugin_trust_evidence"]["egress_validation_completed_at"] =
            json!("2026-05-22T16:21:00Z");
        let (status, detail) = inspect_plugin_trust_qa_report_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(
            detail.contains("egress_validation_completed_at"),
            "{detail}"
        );
    }

    #[test]
    fn plugin_trust_qa_report_rejects_bad_started_timestamp() {
        let mut report = valid_plugin_trust_qa_report_json();
        report["owner_recorded_plugin_trust_evidence"]["review_started_at"] =
            json!("not-a-timestamp");
        let (status, detail) = inspect_plugin_trust_qa_report_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("review_started_at"), "{detail}");
    }

    #[test]
    fn plugin_trust_qa_report_rejects_reversed_timestamps() {
        let mut report = valid_plugin_trust_qa_report_json();
        report["owner_recorded_plugin_trust_evidence"]["review_started_at"] =
            json!("2026-05-22T16:20:00Z");
        report["owner_recorded_plugin_trust_evidence"]["review_completed_at"] =
            json!("2026-05-22T16:10:00Z");
        let (status, detail) = inspect_plugin_trust_qa_report_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("review_completed_at"), "{detail}");
    }

    #[test]
    fn plugin_trust_qa_report_rejects_self_test_source() {
        let mut report = valid_plugin_trust_qa_report_json();
        report["review_source"] = json!("self-test-fixture");
        let (status, detail) = inspect_plugin_trust_qa_report_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("self-test-fixture"), "{detail}");
    }

    #[test]
    fn plugin_trust_qa_report_rejects_future_generated_timestamp() {
        let mut report = valid_plugin_trust_qa_report_json();
        report["generated_at"] = json!(future_timestamp());
        let (status, detail) = inspect_plugin_trust_qa_report_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("generated_at"), "{detail}");
        assert!(detail.contains("current time"), "{detail}");
    }

    #[test]
    fn release_evidence_bundle_accepts_semantically_valid_manifest() {
        let (status, detail) =
            inspect_release_evidence_bundle_value(valid_release_evidence_bundle_json());
        assert_eq!(status, ReleaseEvidenceItemStatus::Present);
        assert!(detail.contains("SHA-256"), "{detail}");
    }

    #[test]
    fn release_evidence_bundle_rejects_wrong_version() {
        let mut report = valid_release_evidence_bundle_json();
        report["version"] = json!("9.9.9");
        let (status, detail) = inspect_release_evidence_bundle_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("version"), "{detail}");
    }

    #[test]
    fn release_evidence_bundle_rejects_wrong_schema_identity() {
        let mut report = valid_release_evidence_bundle_json();
        report["schema_version"] = json!(2);
        let (status, detail) = inspect_release_evidence_bundle_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("schema_version"), "{detail}");

        let mut report = valid_release_evidence_bundle_json();
        report["evidence_type"] = json!("self_test_fixture");
        let (status, detail) = inspect_release_evidence_bundle_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("evidence_type"), "{detail}");
    }

    #[test]
    fn release_evidence_bundle_rejects_invalid_sha256() {
        let mut report = valid_release_evidence_bundle_json();
        report["reports"]["plugin_trust_qa_sha256"] = json!("not-a-digest");
        let (status, detail) = inspect_release_evidence_bundle_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("plugin_trust_qa_sha256"), "{detail}");
        assert!(detail.contains("SHA-256"), "{detail}");
    }

    #[test]
    fn release_evidence_bundle_rejects_disabled_local_signature_validation() {
        let mut report = valid_release_evidence_bundle_json();
        report["validation_flags"]["local_signature_validation"] = json!(false);
        let (status, detail) = inspect_release_evidence_bundle_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("local_signature_validation"), "{detail}");
    }

    #[test]
    fn release_evidence_bundle_rejects_future_generated_timestamp() {
        let mut report = valid_release_evidence_bundle_json();
        report["generated_at"] = json!(future_timestamp());
        let (status, detail) = inspect_release_evidence_bundle_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("generated_at"), "{detail}");
        assert!(detail.contains("current time"), "{detail}");
    }

    #[test]
    fn release_evidence_bundle_rejects_stale_report_digest() {
        let zip_file = tempfile::NamedTempFile::new().expect("temp zip artifact");
        let pkg_file = tempfile::NamedTempFile::new().expect("temp package artifact");
        let signed_file = tempfile::NamedTempFile::new().expect("temp signed provenance report");
        let live_file = tempfile::NamedTempFile::new().expect("temp live QA report");
        let plugin_file = tempfile::NamedTempFile::new().expect("temp plugin QA report");
        std::fs::write(zip_file.path(), "current zip").expect("write zip artifact");
        std::fs::write(pkg_file.path(), "current package").expect("write package artifact");
        std::fs::write(signed_file.path(), "current signed provenance")
            .expect("write signed report");
        std::fs::write(live_file.path(), "current live report").expect("write live report");
        std::fs::write(plugin_file.path(), "current plugin report").expect("write plugin report");

        let mut report = valid_release_evidence_bundle_json();
        report["artifacts"]["zip_path"] = json!(zip_file.path().display().to_string());
        report["artifacts"]["pkg_path"] = json!(pkg_file.path().display().to_string());
        report["artifacts"]["zip_sha256"] =
            json!(file_sha256(zip_file.path()).expect("zip digest"));
        report["artifacts"]["pkg_sha256"] =
            json!(file_sha256(pkg_file.path()).expect("package digest"));
        report["reports"]["signed_distribution_provenance_report"] =
            json!(signed_file.path().display().to_string());
        report["reports"]["live_device_qa_report"] = json!(live_file.path().display().to_string());
        report["reports"]["plugin_trust_qa_report"] =
            json!(plugin_file.path().display().to_string());
        report["reports"]["signed_distribution_provenance_sha256"] =
            json!(file_sha256(signed_file.path()).expect("signed digest"));
        report["reports"]["live_device_qa_sha256"] =
            json!(file_sha256(live_file.path()).expect("live digest"));
        report["reports"]["plugin_trust_qa_sha256"] =
            json!("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef");

        let paths = ReleaseEvidenceBundleDigestPaths {
            app_path: FsPath::new("target/distribution/Jarvis.app"),
            zip_path: zip_file.path(),
            pkg_path: pkg_file.path(),
            signed_provenance_report: signed_file.path(),
            live_qa_report: live_file.path(),
            plugin_qa_report: plugin_file.path(),
        };
        let error = validate_release_evidence_bundle_file_bindings(&report, paths)
            .expect_err("stale plugin report digest should fail");
        assert!(
            error.contains(
                "reports.plugin_trust_qa_sha256 does not match current plugin-trust QA report"
            ),
            "{error}"
        );
    }

    fn valid_signed_distribution_provenance_json() -> serde_json::Value {
        let digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        json!({
            "schema_version": 1,
            "evidence_type": "signed_distribution_provenance",
            "generated_at": "2026-05-22T16:40:00Z",
            "version": "0.1.4",
            "bundle_identifier": "com.nobiletechnology.jarvis",
            "artifacts": {
                "app_path": "target/distribution/Jarvis.app",
                "zip_path": "target/distribution/Jarvis-0.1.4.zip",
                "pkg_path": "target/distribution/Jarvis-0.1.4.pkg",
                "zip_sha256": digest,
                "pkg_sha256": digest,
                "bundled_core_version": "jarvis 0.1.4"
            },
            "signing": {
                "developer_id_application_identity": "Developer ID Application: Jarvis QA Fixture",
                "developer_id_installer_identity": "Developer ID Installer: Jarvis QA Fixture",
                "app_bundle_codesign": "Authority=Developer ID Application: Jarvis QA Fixture",
                "app_executable_codesign": "Authority=Developer ID Application: Jarvis QA Fixture",
                "bundled_core_codesign": "Authority=Developer ID Application: Jarvis QA Fixture",
                "installer_pkg_signature": "Developer ID Installer: Jarvis QA Fixture"
            },
            "notarization": {
                "app_zip_submission_id": "00000000-0000-4000-8000-000000000001",
                "installer_pkg_submission_id": "00000000-0000-4000-8000-000000000002",
                "app_zip_notary_log": "target/distribution/notary-logs/app.log",
                "installer_pkg_notary_log": "target/distribution/notary-logs/pkg.log"
            },
            "stapling": {
                "app_bundle_validation": "The validate action worked!",
                "installer_pkg_validation": "The validate action worked!"
            },
            "gatekeeper": {
                "app_bundle_assessment": "accepted",
                "installer_pkg_assessment": "accepted"
            },
            "validation_flags": {
                "developer_id_application_signed": true,
                "developer_id_installer_signed": true,
                "app_zip_notarized": true,
                "installer_pkg_notarized": true,
                "app_stapled": true,
                "installer_pkg_stapled": true,
                "gatekeeper_assessed": true,
                "artifact_digests_recorded": true
            },
            "proof_boundary": "Signed distribution provenance fixture."
        })
    }

    fn inspect_signed_distribution_provenance_value(
        value: serde_json::Value,
    ) -> (ReleaseEvidenceItemStatus, String) {
        let report_path = tempfile::NamedTempFile::new().expect("temp signed provenance report");
        std::fs::write(
            report_path.path(),
            serde_json::to_string_pretty(&value).expect("serialize signed provenance fixture"),
        )
        .expect("write signed provenance fixture");

        inspect_release_json_report(
            "signed_distribution_provenance_report",
            report_path.path(),
            SIGNED_DISTRIBUTION_PROVENANCE_REQUIRED_FIELDS,
        )
    }

    #[test]
    fn signed_distribution_provenance_accepts_semantically_valid_report() {
        let (status, detail) = inspect_signed_distribution_provenance_value(
            valid_signed_distribution_provenance_json(),
        );
        assert_eq!(status, ReleaseEvidenceItemStatus::Present);
        assert!(detail.contains("Gatekeeper"), "{detail}");
    }

    #[test]
    fn signed_distribution_provenance_rejects_stale_artifact_digest() {
        let zip_file = tempfile::NamedTempFile::new().expect("temp zip artifact");
        let pkg_file = tempfile::NamedTempFile::new().expect("temp package artifact");
        std::fs::write(zip_file.path(), "current zip").expect("write zip artifact");
        std::fs::write(pkg_file.path(), "current package").expect("write package artifact");

        let mut report = valid_signed_distribution_provenance_json();
        report["artifacts"]["pkg_sha256"] =
            json!(file_sha256(pkg_file.path()).expect("package digest"));
        let error = validate_signed_distribution_artifact_digests(
            &report,
            zip_file.path(),
            pkg_file.path(),
        )
        .expect_err("stale zip digest should fail");

        assert!(
            error.contains("artifacts.zip_sha256 does not match current app zip artifact"),
            "{error}"
        );
    }

    #[test]
    fn signed_distribution_provenance_rejects_wrong_version() {
        let mut report = valid_signed_distribution_provenance_json();
        report["version"] = json!("9.9.9");
        let (status, detail) = inspect_signed_distribution_provenance_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("version"), "{detail}");
    }

    #[test]
    fn signed_distribution_provenance_rejects_wrong_bundled_core_version() {
        let mut report = valid_signed_distribution_provenance_json();
        report["artifacts"]["bundled_core_version"] = json!("jarvis 9.9.9");
        let (status, detail) = inspect_signed_distribution_provenance_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(
            detail.contains("artifacts.bundled_core_version"),
            "{detail}"
        );
    }

    #[test]
    fn signed_distribution_provenance_rejects_future_generated_timestamp() {
        let mut report = valid_signed_distribution_provenance_json();
        report["generated_at"] = json!(future_timestamp());
        let (status, detail) = inspect_signed_distribution_provenance_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("generated_at"), "{detail}");
        assert!(detail.contains("current time"), "{detail}");
    }

    #[test]
    fn signed_distribution_provenance_rejects_missing_notary_submission() {
        let mut report = valid_signed_distribution_provenance_json();
        report["notarization"]["app_zip_submission_id"] = json!("");
        let (status, detail) = inspect_signed_distribution_provenance_value(report);
        assert_eq!(status, ReleaseEvidenceItemStatus::Invalid);
        assert!(detail.contains("app_zip_submission_id"), "{detail}");
    }

    fn release_evidence_status_fixture(
        live_device_status: ReleaseEvidenceItemStatus,
    ) -> ReleaseEvidenceStatusResponse {
        let live_device_item = ReleaseEvidenceStatusItem {
            key: "live_device_qa_report".to_string(),
            label: "Live-device QA report".to_string(),
            path: "target/release-live-device-qa-report.json".to_string(),
            kind: ReleaseEvidenceKind::JsonReport,
            status: live_device_status,
            required_for_production: true,
            manual_gate: true,
            detail: "test fixture".to_string(),
        };
        let missing_bundle = ReleaseEvidenceStatusItem {
            key: "release_evidence_bundle".to_string(),
            label: "Release evidence bundle".to_string(),
            path: "target/release-evidence-bundle.json".to_string(),
            kind: ReleaseEvidenceKind::JsonReport,
            status: ReleaseEvidenceItemStatus::Missing,
            required_for_production: true,
            manual_gate: true,
            detail: "test fixture".to_string(),
        };
        let items = vec![live_device_item, missing_bundle];
        let satisfied_count = items
            .iter()
            .filter(|item| item.status == ReleaseEvidenceItemStatus::Present)
            .count();
        let missing_count = items
            .iter()
            .filter(|item| item.status == ReleaseEvidenceItemStatus::Missing)
            .count();
        let invalid_count = items
            .iter()
            .filter(|item| item.status == ReleaseEvidenceItemStatus::Invalid)
            .count();
        ReleaseEvidenceStatusResponse {
            generated_at: Utc::now(),
            complete: missing_count == 0 && invalid_count == 0,
            satisfied_count,
            missing_count,
            invalid_count,
            items,
            proof_boundary: "test fixture".to_string(),
        }
    }

    fn release_complete_evidence_status_fixture() -> ReleaseEvidenceStatusResponse {
        release_complete_evidence_status_fixture_with_item_status(
            "signed_app_bundle",
            ReleaseEvidenceItemStatus::Present,
        )
    }

    fn release_complete_evidence_status_fixture_with_item_status(
        target_key: &str,
        target_status: ReleaseEvidenceItemStatus,
    ) -> ReleaseEvidenceStatusResponse {
        let items = [
            ("signed_app_bundle", ReleaseEvidenceKind::Directory),
            ("app_executable", ReleaseEvidenceKind::Executable),
            ("bundled_core_executable", ReleaseEvidenceKind::Executable),
            ("signed_app_zip", ReleaseEvidenceKind::File),
            ("signed_installer_package", ReleaseEvidenceKind::File),
            (
                "signed_distribution_provenance_report",
                ReleaseEvidenceKind::JsonReport,
            ),
            ("live_device_qa_report", ReleaseEvidenceKind::JsonReport),
            ("plugin_trust_qa_report", ReleaseEvidenceKind::JsonReport),
            ("release_evidence_bundle", ReleaseEvidenceKind::JsonReport),
        ]
        .into_iter()
        .map(|(key, kind)| ReleaseEvidenceStatusItem {
            key: key.to_string(),
            label: key.to_string(),
            path: format!("target/release-fixture/{key}"),
            kind,
            status: if key == target_key {
                target_status
            } else {
                ReleaseEvidenceItemStatus::Present
            },
            required_for_production: true,
            manual_gate: true,
            detail: "test fixture".to_string(),
        })
        .collect::<Vec<_>>();
        let satisfied_count = items
            .iter()
            .filter(|item| item.status == ReleaseEvidenceItemStatus::Present)
            .count();
        let missing_count = items
            .iter()
            .filter(|item| item.status == ReleaseEvidenceItemStatus::Missing)
            .count();
        let invalid_count = items
            .iter()
            .filter(|item| item.status == ReleaseEvidenceItemStatus::Invalid)
            .count();

        ReleaseEvidenceStatusResponse {
            generated_at: Utc::now(),
            complete: missing_count == 0 && invalid_count == 0,
            satisfied_count,
            missing_count,
            invalid_count,
            items,
            proof_boundary: "complete evidence fixture".to_string(),
        }
    }

    #[test]
    fn installed_plugin_runner_fails_closed_and_audits_request() {
        let repository = SqliteRepository::in_memory().unwrap();
        let source_dir = tempfile::tempdir().unwrap();
        let source_path = source_dir
            .path()
            .canonicalize()
            .unwrap()
            .display()
            .to_string();
        let installed = InstalledPlugin {
            manifest: local_installed_manifest(&source_path),
            source_path: source_path.clone(),
            provenance: InstalledPluginProvenance::legacy_unverified(
                source_path.clone(),
                Utc::now(),
            ),
            execution_enabled: false,
            execution_grant: crate::InstalledPluginExecutionGrant::MetadataOnly,
        };
        repository.install_plugin_metadata(installed).unwrap();
        let state = IpcState::with_repository(repository).expect("state");

        unsafe {
            std::env::set_var(
                "JARVIS_SECRET_LEAK_TEST",
                "subprocess must not inherit this",
            );
        }
        let response = state
            .run_installed_plugin(
                "local_runner_test",
                InstalledPluginRunRequest {
                    action: "inspect".to_string(),
                    input: json!({ "path": "README.md" }),
                    session_id: Some(Uuid::new_v4()),
                    dry_run: false,
                },
            )
            .expect("fail-closed response");

        assert_eq!(response.status, "blocked");
        assert!(!response.execution_enabled);
        assert_eq!(
            response.execution_grant,
            crate::InstalledPluginExecutionGrant::MetadataOnly
        );
        assert!(response.manifest_valid);
        assert!(response.action_declared);
        assert!(response.input_valid);
        assert!(response.contract_validated);
        assert!(!response.side_effect_executed);
        assert_eq!(
            response.reason,
            "installed plugin execution grant is metadata_only; only contract dry runs are allowed"
        );
        assert_eq!(
            response.audit_entry.event_type,
            "installed_plugin_execution_blocked"
        );
        assert_eq!(
            response.audit_entry.payload["plugin_id"],
            "local_runner_test"
        );
        assert_eq!(response.audit_entry.payload["manifest_schema_version"], 1);
        assert_eq!(response.audit_entry.payload["execution_enabled"], false);
        assert_eq!(
            response.audit_entry.payload["execution_grant"],
            "metadata_only"
        );
        assert_eq!(response.audit_entry.payload["contract_validated"], true);
        assert_eq!(response.audit_entry.payload["side_effect_executed"], false);

        let audit_entries = state
            .using_repository(|repository| repository.list_audit_entries(None))
            .expect("audit entries");
        assert!(audit_entries
            .iter()
            .any(|entry| entry.id == response.audit_entry.id));
    }

    #[test]
    fn installed_plugin_runner_blocks_undeclared_actions_before_execution() {
        let repository = SqliteRepository::in_memory().unwrap();
        let source_dir = tempfile::tempdir().unwrap();
        let source_path = source_dir
            .path()
            .canonicalize()
            .unwrap()
            .display()
            .to_string();
        let installed = InstalledPlugin {
            manifest: local_installed_manifest(&source_path),
            source_path: source_path.clone(),
            provenance: InstalledPluginProvenance::legacy_unverified(source_path, Utc::now()),
            execution_enabled: false,
            execution_grant: crate::InstalledPluginExecutionGrant::MetadataOnly,
        };
        repository.install_plugin_metadata(installed).unwrap();
        let state = IpcState::with_repository(repository).expect("state");

        let response = state
            .run_installed_plugin(
                "local_runner_test",
                InstalledPluginRunRequest {
                    action: "missing".to_string(),
                    input: json!({}),
                    session_id: None,
                    dry_run: false,
                },
            )
            .expect("fail-closed response");

        assert_eq!(response.status, "blocked");
        assert!(response.manifest_valid);
        assert!(!response.action_declared);
        assert!(!response.input_valid);
        assert!(!response.contract_validated);
        assert!(!response.side_effect_executed);
        assert_eq!(
            response.audit_entry.event_type,
            "installed_plugin_action_blocked"
        );
    }

    #[test]
    fn installed_plugin_runner_supports_contract_only_dry_run() {
        let repository = SqliteRepository::in_memory().unwrap();
        let source_dir = tempfile::tempdir().unwrap();
        let source_path = source_dir
            .path()
            .canonicalize()
            .unwrap()
            .display()
            .to_string();
        let installed = InstalledPlugin {
            manifest: local_installed_manifest(&source_path),
            source_path: source_path.clone(),
            provenance: InstalledPluginProvenance::legacy_unverified(source_path, Utc::now()),
            execution_enabled: false,
            execution_grant: crate::InstalledPluginExecutionGrant::MetadataOnly,
        };
        repository.install_plugin_metadata(installed).unwrap();
        let state = IpcState::with_repository(repository).expect("state");

        let response = state
            .run_installed_plugin(
                "local_runner_test",
                InstalledPluginRunRequest {
                    action: "inspect".to_string(),
                    input: json!({ "path": "README.md" }),
                    session_id: None,
                    dry_run: true,
                },
            )
            .expect("dry-run response");

        assert_eq!(response.status, "dry_run");
        assert_eq!(
            response.execution_grant,
            crate::InstalledPluginExecutionGrant::MetadataOnly
        );
        assert!(!response.execution_enabled);
        assert!(response.manifest_valid);
        assert!(response.action_declared);
        assert!(response.input_valid);
        assert!(response.contract_validated);
        assert!(!response.side_effect_executed);
        assert_eq!(
            response.audit_entry.event_type,
            "installed_plugin_contract_dry_run"
        );
        assert_eq!(response.audit_entry.payload["dry_run"], true);
        assert_eq!(response.audit_entry.payload["side_effect_executed"], false);
    }

    #[test]
    fn installed_plugin_runner_rejects_invalid_input_before_dry_run() {
        let repository = SqliteRepository::in_memory().unwrap();
        let source_dir = tempfile::tempdir().unwrap();
        let source_path = source_dir
            .path()
            .canonicalize()
            .unwrap()
            .display()
            .to_string();
        let installed = InstalledPlugin {
            manifest: local_installed_manifest(&source_path),
            source_path: source_path.clone(),
            provenance: InstalledPluginProvenance::legacy_unverified(source_path, Utc::now()),
            execution_enabled: false,
            execution_grant: crate::InstalledPluginExecutionGrant::MetadataOnly,
        };
        repository.install_plugin_metadata(installed).unwrap();
        let state = IpcState::with_repository(repository).expect("state");

        let response = state
            .run_installed_plugin(
                "local_runner_test",
                InstalledPluginRunRequest {
                    action: "inspect".to_string(),
                    input: json!({ "path": "README.md", "extra": true }),
                    session_id: None,
                    dry_run: true,
                },
            )
            .expect("blocked response");

        assert_eq!(response.status, "blocked");
        assert!(response.manifest_valid);
        assert!(response.action_declared);
        assert!(!response.input_valid);
        assert!(!response.contract_validated);
        assert!(!response.side_effect_executed);
        assert_eq!(
            response.audit_entry.event_type,
            "installed_plugin_input_invalid"
        );
        assert!(response.reason.contains("undeclared field extra"));
    }

    #[cfg(unix)]
    #[test]
    fn installed_plugin_runner_executes_enabled_subprocess_with_validated_output() {
        let repository = SqliteRepository::in_memory().unwrap();
        let source_dir = tempfile::tempdir().unwrap();
        let source_path_buf = source_dir.path().canonicalize().unwrap();
        write_executable_plugin_script(&source_path_buf);
        std::fs::write(
            source_path_buf.join("helper-resource.txt"),
            "original helper",
        )
        .unwrap();
        let source_path = source_path_buf.display().to_string();
        let manifest_path = source_path_buf.join("jarvis-plugin.json");
        std::fs::write(
            &manifest_path,
            serde_json::to_string(&local_subprocess_manifest(&source_path)).unwrap(),
        )
        .unwrap();
        let installed = InstalledPlugin::from_local_manifest_path(&manifest_path).unwrap();
        repository.install_plugin_metadata(installed).unwrap();
        let state = IpcState::with_repository(repository).expect("state");

        let unverified_error = state
            .set_installed_plugin_execution(
                "local_runner_test",
                InstalledPluginExecutionRequest {
                    execution_enabled: true,
                    execution_grant: InstalledPluginExecutionGrant::SubprocessStdio,
                },
            )
            .expect_err("unverified provenance cannot execute");
        assert!(unverified_error
            .to_string()
            .contains("requires local provenance verification"));

        let verified = state
            .verify_installed_plugin_provenance("local_runner_test")
            .expect("verify provenance");
        assert_eq!(
            verified.provenance.integrity_status,
            InstalledPluginIntegrityStatus::MatchesInstallSnapshot
        );

        let enabled = state
            .set_installed_plugin_execution(
                "local_runner_test",
                InstalledPluginExecutionRequest {
                    execution_enabled: true,
                    execution_grant: InstalledPluginExecutionGrant::SubprocessStdio,
                },
            )
            .expect("enable subprocess");
        assert!(enabled.execution_enabled);
        assert_eq!(
            enabled.execution_grant,
            InstalledPluginExecutionGrant::SubprocessStdio
        );

        let response = state
            .run_installed_plugin(
                "local_runner_test",
                InstalledPluginRunRequest {
                    action: "inspect".to_string(),
                    input: json!({ "path": "README.md" }),
                    session_id: None,
                    dry_run: false,
                },
            )
            .expect("subprocess response");
        unsafe {
            std::env::remove_var("JARVIS_SECRET_LEAK_TEST");
        }

        assert_eq!(response.status, "completed");
        assert!(response.execution_enabled);
        assert_eq!(
            response.execution_grant,
            InstalledPluginExecutionGrant::SubprocessStdio
        );
        assert!(response.contract_validated);
        assert!(response.side_effect_executed);
        let output = response.output.as_ref().expect("subprocess output");
        assert_eq!(output["path"], "README.md");
        assert_eq!(output["secret_seen"], false);
        assert_eq!(output["plugin_id"], "local_runner_test");
        assert_eq!(output["plugin_action"], "inspect");
        assert_eq!(
            response.audit_entry.event_type,
            "installed_plugin_subprocess_completed"
        );
        assert_eq!(
            response.audit_entry.payload["sandbox_process_started"],
            true
        );
        assert_eq!(response.audit_entry.payload["subprocess_started"], true);
        assert_eq!(response.audit_entry.payload["side_effect_executed"], true);

        std::fs::write(
            source_path_buf.join("helper-resource.txt"),
            "changed helper",
        )
        .unwrap();
        let changed_response = state
            .run_installed_plugin(
                "local_runner_test",
                InstalledPluginRunRequest {
                    action: "inspect".to_string(),
                    input: json!({ "path": "README.md" }),
                    session_id: None,
                    dry_run: false,
                },
            )
            .expect("changed plugin blocks");
        assert_eq!(changed_response.status, "blocked");
        assert_eq!(
            changed_response.provenance.integrity_status,
            InstalledPluginIntegrityStatus::ChangedSinceInstall
        );
        assert!(!changed_response.side_effect_executed);
    }

    #[cfg(unix)]
    #[test]
    fn installed_plugin_runner_records_subprocess_progress_events_without_raw_stderr() {
        let repository = SqliteRepository::in_memory().unwrap();
        let source_dir = tempfile::tempdir().unwrap();
        let source_path_buf = source_dir.path().canonicalize().unwrap();
        write_progress_plugin_script(&source_path_buf);
        let source_path = source_path_buf.display().to_string();
        let manifest_path = source_path_buf.join("jarvis-plugin.json");
        std::fs::write(
            &manifest_path,
            serde_json::to_string(&local_subprocess_manifest(&source_path)).unwrap(),
        )
        .unwrap();
        let installed = InstalledPlugin::from_local_manifest_path(&manifest_path).unwrap();
        repository.install_plugin_metadata(installed).unwrap();
        let state = IpcState::with_repository(repository).expect("state");
        state
            .verify_installed_plugin_provenance("local_runner_test")
            .expect("verify provenance");
        state
            .set_installed_plugin_execution(
                "local_runner_test",
                InstalledPluginExecutionRequest {
                    execution_enabled: true,
                    execution_grant: InstalledPluginExecutionGrant::SubprocessStdio,
                },
            )
            .expect("enable subprocess");

        let response = state
            .run_installed_plugin(
                "local_runner_test",
                InstalledPluginRunRequest {
                    action: "inspect".to_string(),
                    input: json!({ "path": "README.md" }),
                    session_id: None,
                    dry_run: false,
                },
            )
            .expect("subprocess response");

        assert_eq!(response.status, "completed");
        assert_eq!(response.output, Some(json!({ "path": "README.md" })));
        assert_eq!(response.progress_events.len(), 2);
        assert_eq!(response.progress_events[0].sequence, 1);
        assert_eq!(response.progress_events[0].stage, "prepare");
        assert_eq!(response.progress_events[0].message, "validated request");
        assert_eq!(response.progress_events[1].sequence, 2);
        assert_eq!(response.progress_events[1].stage, "complete");
        assert_eq!(
            response.progress_events[1].message,
            "writing validated output"
        );
        assert_eq!(response.audit_entry.payload["progress_event_count"], 2);

        let audit_entries = state
            .using_repository(|repository| repository.list_audit_entries(None))
            .expect("audit entries");
        let progress_entries = audit_entries
            .iter()
            .filter(|entry| entry.event_type == "installed_plugin_progress")
            .collect::<Vec<_>>();
        assert_eq!(progress_entries.len(), 2);
        assert_eq!(progress_entries[0].payload["stage"], "prepare");
        assert_eq!(progress_entries[0].payload["stderr_redacted"], true);

        let summary = state.activity_summary().expect("activity summary");
        let activity_progress = activity_progress_events_from_summary(&summary);
        assert_eq!(activity_progress.len(), 2);
        assert_eq!(
            activity_progress[0].plugin_id.as_deref(),
            Some("local_runner_test")
        );
        assert_eq!(activity_progress[0].action.as_deref(), Some("inspect"));
        assert_eq!(activity_progress[0].stage.as_deref(), Some("complete"));
        assert_eq!(
            activity_progress[0].message.as_deref(),
            Some("writing validated output")
        );
        assert!(activity_progress[0].stderr_redacted);
        assert_eq!(activity_progress[1].stage.as_deref(), Some("prepare"));

        let encoded_response = serde_json::to_string(&response).expect("response JSON");
        let encoded_audit = serde_json::to_string(&audit_entries).expect("audit JSON");
        assert!(!encoded_response.contains("raw stderr secret"));
        assert!(!encoded_response.contains("ignored"));
        assert!(!encoded_audit.contains("raw stderr secret"));
        assert!(!encoded_audit.contains("ignored"));
    }

    #[test]
    fn installed_plugin_execution_enable_fails_for_metadata_grant() {
        let repository = SqliteRepository::in_memory().unwrap();
        let source_dir = tempfile::tempdir().unwrap();
        let source_path = source_dir
            .path()
            .canonicalize()
            .unwrap()
            .display()
            .to_string();
        let installed = InstalledPlugin {
            manifest: local_installed_manifest(&source_path),
            source_path: source_path.clone(),
            provenance: InstalledPluginProvenance::legacy_unverified(source_path, Utc::now()),
            execution_enabled: false,
            execution_grant: InstalledPluginExecutionGrant::MetadataOnly,
        };
        repository.install_plugin_metadata(installed).unwrap();
        let state = IpcState::with_repository(repository).expect("state");

        let error = state
            .set_installed_plugin_execution(
                "local_runner_test",
                InstalledPluginExecutionRequest {
                    execution_enabled: true,
                    execution_grant: InstalledPluginExecutionGrant::MetadataOnly,
                },
            )
            .expect_err("metadata grant cannot execute");
        assert!(error
            .to_string()
            .contains("requires subprocess_stdio or subprocess_stdio_network grant"));
    }

    #[cfg(unix)]
    #[test]
    fn installed_plugin_runner_blocks_network_action_without_network_grant() {
        let repository = SqliteRepository::in_memory().unwrap();
        let source_dir = tempfile::tempdir().unwrap();
        let source_path_buf = source_dir.path().canonicalize().unwrap();
        write_executable_plugin_script(&source_path_buf);
        let source_path = source_path_buf.display().to_string();
        let mut manifest = local_subprocess_manifest(&source_path);
        manifest.actions[0]
            .permissions
            .push(crate::PluginPermission::Network);
        manifest.actions[0].network_access = crate::PluginNetworkAccess {
            mode: crate::PluginNetworkAccessMode::DeclaredHosts,
            allowed_hosts: vec!["api.jarvis.local".to_string()],
        };
        let manifest_path = source_path_buf.join("jarvis-plugin.json");
        std::fs::write(&manifest_path, serde_json::to_string(&manifest).unwrap()).unwrap();
        let installed = InstalledPlugin::from_local_manifest_path(&manifest_path).unwrap();
        repository.install_plugin_metadata(installed).unwrap();
        let state = IpcState::with_repository(repository).expect("state");
        state
            .verify_installed_plugin_provenance("local_runner_test")
            .expect("verify provenance");

        let default_grant_error = state
            .set_installed_plugin_execution(
                "local_runner_test",
                InstalledPluginExecutionRequest {
                    execution_enabled: true,
                    execution_grant: InstalledPluginExecutionGrant::SubprocessStdio,
                },
            )
            .expect_err("network action requires network grant");
        assert!(default_grant_error
            .to_string()
            .contains("subprocess_stdio grant requires at least one non-network action"));

        let stored = state
            .using_repository(|repository| {
                repository.set_installed_plugin_execution(
                    "local_runner_test",
                    true,
                    InstalledPluginExecutionGrant::SubprocessStdio,
                )
            })
            .expect("force legacy grant for runtime regression coverage");
        assert!(stored.execution_enabled);
        assert_eq!(
            stored.execution_grant,
            InstalledPluginExecutionGrant::SubprocessStdio
        );

        let blocked = state
            .run_installed_plugin(
                "local_runner_test",
                InstalledPluginRunRequest {
                    action: "inspect".to_string(),
                    input: json!({ "path": "README.md" }),
                    session_id: None,
                    dry_run: false,
                },
            )
            .expect("network action blocks under stdio grant");
        assert_eq!(blocked.status, "blocked");
        assert!(!blocked.side_effect_executed);
        assert_eq!(
            blocked.execution_grant,
            InstalledPluginExecutionGrant::SubprocessStdio
        );
        assert!(blocked
            .reason
            .contains("requires subprocess_stdio_network grant"));
        assert_eq!(
            blocked.audit_entry.payload["action_requires_network_grant"],
            true
        );

        state
            .using_repository(|repository| {
                repository.set_installed_plugin_execution(
                    "local_runner_test",
                    true,
                    InstalledPluginExecutionGrant::SubprocessStdioNetwork,
                )
            })
            .expect("force network grant");
        let completed = state
            .run_installed_plugin(
                "local_runner_test",
                InstalledPluginRunRequest {
                    action: "inspect".to_string(),
                    input: json!({ "path": "README.md" }),
                    session_id: None,
                    dry_run: false,
                },
            )
            .expect("network action runs under network grant");
        assert_eq!(completed.status, "completed");
        assert!(completed.side_effect_executed);
        assert_eq!(
            completed.execution_grant,
            InstalledPluginExecutionGrant::SubprocessStdioNetwork
        );
    }

    #[cfg(unix)]
    #[test]
    fn installed_plugin_runner_scopes_network_grants_to_declaring_actions() {
        let repository = SqliteRepository::in_memory().unwrap();
        let source_dir = tempfile::tempdir().unwrap();
        let source_path_buf = source_dir.path().canonicalize().unwrap();
        write_executable_plugin_script(&source_path_buf);
        let source_path = source_path_buf.display().to_string();
        let mut manifest = local_subprocess_manifest(&source_path);
        let mut network_action = manifest.actions[0].clone();
        network_action.name = "fetch".to_string();
        network_action
            .permissions
            .push(crate::PluginPermission::Network);
        network_action.network_access = crate::PluginNetworkAccess {
            mode: crate::PluginNetworkAccessMode::DeclaredHosts,
            allowed_hosts: vec!["api.jarvis.local".to_string()],
        };
        manifest.actions.push(network_action);
        let manifest_path = source_path_buf.join("jarvis-plugin.json");
        std::fs::write(&manifest_path, serde_json::to_string(&manifest).unwrap()).unwrap();
        let installed = InstalledPlugin::from_local_manifest_path(&manifest_path).unwrap();
        repository.install_plugin_metadata(installed).unwrap();
        let state = IpcState::with_repository(repository).expect("state");
        state
            .verify_installed_plugin_provenance("local_runner_test")
            .expect("verify provenance");

        let stdio_enabled = state
            .set_installed_plugin_execution(
                "local_runner_test",
                InstalledPluginExecutionRequest {
                    execution_enabled: true,
                    execution_grant: InstalledPluginExecutionGrant::SubprocessStdio,
                },
            )
            .expect("enable non-network execution for mixed plugin");
        assert_eq!(
            stdio_enabled.execution_grant,
            InstalledPluginExecutionGrant::SubprocessStdio
        );
        let inspect = state
            .run_installed_plugin(
                "local_runner_test",
                InstalledPluginRunRequest {
                    action: "inspect".to_string(),
                    input: json!({ "path": "README.md" }),
                    session_id: None,
                    dry_run: false,
                },
            )
            .expect("non-network action runs under stdio grant");
        assert_eq!(inspect.status, "completed");
        assert!(inspect.side_effect_executed);

        let fetch_blocked = state
            .run_installed_plugin(
                "local_runner_test",
                InstalledPluginRunRequest {
                    action: "fetch".to_string(),
                    input: json!({ "path": "README.md" }),
                    session_id: None,
                    dry_run: false,
                },
            )
            .expect("network action returns blocked response under stdio grant");
        assert_eq!(fetch_blocked.status, "blocked");
        assert!(!fetch_blocked.side_effect_executed);
        assert!(fetch_blocked
            .reason
            .contains("requires subprocess_stdio_network grant"));

        let network_enabled = state
            .set_installed_plugin_execution(
                "local_runner_test",
                InstalledPluginExecutionRequest {
                    execution_enabled: true,
                    execution_grant: InstalledPluginExecutionGrant::SubprocessStdioNetwork,
                },
            )
            .expect("enable network execution for mixed plugin");
        assert_eq!(
            network_enabled.execution_grant,
            InstalledPluginExecutionGrant::SubprocessStdioNetwork
        );
        let fetch = state
            .run_installed_plugin(
                "local_runner_test",
                InstalledPluginRunRequest {
                    action: "fetch".to_string(),
                    input: json!({ "path": "README.md" }),
                    session_id: None,
                    dry_run: false,
                },
            )
            .expect("network action runs under network grant");
        assert_eq!(fetch.status, "completed");
        assert!(fetch.side_effect_executed);
        assert_eq!(
            fetch.output.as_ref().expect("fetch output")["plugin_action"],
            "fetch"
        );

        let inspect_blocked = state
            .run_installed_plugin(
                "local_runner_test",
                InstalledPluginRunRequest {
                    action: "inspect".to_string(),
                    input: json!({ "path": "README.md" }),
                    session_id: None,
                    dry_run: false,
                },
            )
            .expect("non-network action returns blocked response under network grant");
        assert_eq!(inspect_blocked.status, "blocked");
        assert!(!inspect_blocked.side_effect_executed);
        assert!(inspect_blocked
            .reason
            .contains("reserved for network-declaring actions"));
    }

    #[tokio::test]
    async fn command_schema_executes_fake_local_runtime() {
        let state = IpcState::new();
        let response = state
            .submit_command(CommandRequest {
                input: "what is next".to_string(),
                session_id: None,
                context: json!({"surface": "test"}),
                dry_run: true,
                proactive: false,
                sensitivity: None,
            })
            .await
            .expect("command");

        assert!(response.accepted);
        assert_eq!(response.task.status, TaskStatus::Completed);
        assert_eq!(response.audit_entry.event_type, "task_completed");
        assert_eq!(response.steps.len(), 1);
        assert!(response.message.contains("what is next"));
        assert_eq!(
            response.route.expect("fake local route").model,
            "fake-local-model"
        );
        let route_evidence = response.route_evidence.expect("route evidence");
        assert_eq!(route_evidence.outcome, crate::RouteOutcome::Selected);
        assert!(!route_evidence.evidence.chatgpt_enabled);
        assert!(response
            .audit_entries
            .iter()
            .any(|entry| entry.event_type == "model_step_completed"));
        assert!(response
            .audit_entries
            .iter()
            .any(|entry| entry.event_type == "model_route_selected"));
    }

    #[tokio::test]
    async fn command_schema_returns_failed_runtime_response_for_model_provider_error() {
        async fn failing_chatgpt() -> (StatusCode, Json<serde_json::Value>) {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "provider failed with token=sk-test" })),
            )
        }

        let app = Router::new().route("/chat/completions", post(failing_chatgpt));
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("listener");
        let address = listener.local_addr().expect("address");
        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("test server");
        });

        let state = IpcState::with_provider_config(ProviderConfig {
            local: crate::LocalModelConfig {
                enabled: false,
                ..crate::LocalModelConfig::default()
            },
            chatgpt: crate::ChatGptProviderConfig {
                enabled: true,
                model: "gpt-test".to_string(),
                base_url: format!("http://{address}"),
                api_key: Some("test-token".to_string()),
                requires_approval: true,
                timeout_ms: 2_000,
            },
        });

        let response = state
            .submit_command(CommandRequest {
                input: "cloud provider should fail structurally".to_string(),
                session_id: None,
                context: json!({"surface": "test"}),
                dry_run: false,
                proactive: false,
                sensitivity: Some(Sensitivity::Workspace),
            })
            .await
            .expect("provider failure should not become an IPC transport error");

        assert!(!response.accepted);
        assert_eq!(response.task.status, TaskStatus::Failed);
        assert!(response
            .message
            .contains("Model execution failed during step 0"));
        assert!(response.plugin_results.is_empty());
        assert_eq!(response.audit_entry.event_type, "model_step_failed");
        let route = response.route_evidence.as_ref().expect("route evidence");
        assert_eq!(route.outcome, crate::RouteOutcome::Selected);
        assert_eq!(
            route.selected_provider,
            Some(crate::RoutedModelProvider::ChatGpt)
        );
        assert!(response
            .audit_entries
            .iter()
            .any(|entry| entry.event_type == "model_route_selected"));
        let encoded = serde_json::to_string(&response).expect("response JSON");
        assert!(!encoded.contains("sk-test"));
        assert!(!encoded.contains("test-token"));
    }

    fn local_installed_manifest(source_path: &str) -> PluginManifest {
        PluginManifest {
            manifest_schema_version: 1,
            id: "local_runner_test".to_string(),
            name: "Local Runner Test".to_string(),
            version: "0.1.0".to_string(),
            source: crate::PluginSource::LocalDevelopment,
            author: "Jarvis Test".to_string(),
            source_path: Some(source_path.to_string()),
            subprocess: None,
            publisher_signature: None,
            actions: vec![crate::PluginActionManifest {
                name: "inspect".to_string(),
                description: "Validate installed runner boundary.".to_string(),
                permissions: vec![crate::PluginPermission::ReadWorkspace],
                risk_tier: crate::RiskTier::Low,
                input_schema: crate::JsonSchema::object(
                    serde_json::Map::from_iter([("path".to_string(), json!({ "type": "string" }))]),
                    vec!["path".to_string()],
                ),
                output_schema: crate::JsonSchema::empty_object(),
                proactive: false,
                memory_access: crate::PluginAccess::None,
                model_access: crate::PluginAccess::None,
                network_access: crate::PluginNetworkAccess::default(),
                audit_fields: vec!["path".to_string()],
                timeout: crate::PluginTimeout::default_for_action(),
                cancellation: crate::CancellationBehavior::Cooperative,
            }],
        }
    }

    fn local_subprocess_manifest(source_path: &str) -> PluginManifest {
        let mut input_properties = serde_json::Map::new();
        input_properties.insert("path".to_string(), json!({ "type": "string" }));
        let mut output_properties = serde_json::Map::new();
        output_properties.insert("path".to_string(), json!({ "type": "string" }));
        output_properties.insert("secret_seen".to_string(), json!({ "type": "boolean" }));
        output_properties.insert("plugin_id".to_string(), json!({ "type": "string" }));
        output_properties.insert("plugin_action".to_string(), json!({ "type": "string" }));

        PluginManifest {
            manifest_schema_version: 1,
            id: "local_runner_test".to_string(),
            name: "Local Runner Test".to_string(),
            version: "0.1.0".to_string(),
            source: crate::PluginSource::LocalSubprocess,
            author: "Jarvis Test".to_string(),
            source_path: Some(source_path.to_string()),
            subprocess: Some(crate::PluginSubprocessManifest {
                command: "plugin-runner.py".to_string(),
                args: Vec::new(),
                stdin: crate::PluginSubprocessStream::Json,
                stdout: crate::PluginSubprocessStream::Json,
            }),
            publisher_signature: None,
            actions: vec![crate::PluginActionManifest {
                name: "inspect".to_string(),
                description: "Validate installed subprocess runner boundary.".to_string(),
                permissions: vec![crate::PluginPermission::ReadWorkspace],
                risk_tier: crate::RiskTier::Low,
                input_schema: crate::JsonSchema::object(input_properties, vec!["path".to_string()]),
                output_schema: crate::JsonSchema::object(
                    output_properties,
                    vec!["path".to_string()],
                ),
                proactive: false,
                memory_access: crate::PluginAccess::None,
                model_access: crate::PluginAccess::None,
                network_access: crate::PluginNetworkAccess::default(),
                audit_fields: vec!["path".to_string()],
                timeout: crate::PluginTimeout::default_for_action(),
                cancellation: crate::CancellationBehavior::Cooperative,
            }],
        }
    }

    #[tokio::test]
    async fn permission_policy_review_summarizes_pending_approvals() {
        let repository = SqliteRepository::in_memory().unwrap();
        let state = IpcState::with_repository(repository).expect("state");
        let response = state
            .submit_command(CommandRequest {
                input: "plugin approval echo review me".to_string(),
                session_id: None,
                context: json!({"surface": "test"}),
                dry_run: false,
                proactive: false,
                sensitivity: Some(Sensitivity::Workspace),
            })
            .await
            .expect("approval command");
        assert_eq!(response.task.status, TaskStatus::WaitingForApproval);

        let review = state.permission_policy_review().expect("policy review");
        assert_eq!(review.status, "review_required");
        assert_eq!(review.high_risk_pending_count, 1);
        assert_eq!(review.review_item_count, 1);
        assert_eq!(review.items[0].item_type, "pending_approval");
        assert_eq!(review.items[0].severity, "high");
        assert_eq!(
            review.items[0].action.as_deref(),
            Some("fake_echo.approval_echo")
        );
        assert!(review.side_effects_require_approval);
    }

    #[tokio::test]
    async fn approved_first_party_action_executes_with_audit_evidence() {
        let repository = SqliteRepository::in_memory().unwrap();
        let state = IpcState::with_repository(repository).expect("state");
        let response = state
            .submit_command(CommandRequest {
                input: "plugin approval echo review me".to_string(),
                session_id: None,
                context: json!({"surface": "test"}),
                dry_run: false,
                proactive: false,
                sensitivity: Some(Sensitivity::Workspace),
            })
            .await
            .expect("approval command");
        assert_eq!(response.task.status, TaskStatus::WaitingForApproval);
        let approval = state
            .list_approvals(Some(ApprovalStatus::Pending))
            .expect("pending approvals")
            .into_iter()
            .next()
            .expect("pending approval");

        state
            .approve_approval(
                approval.id,
                "test".to_string(),
                Some("reviewed".to_string()),
            )
            .expect("approve");
        let executed = state
            .execute_approved_approval(approval.id)
            .expect("execute approved");

        assert!(executed.accepted);
        assert_eq!(executed.task.status, TaskStatus::Completed);
        assert_eq!(executed.audit_entry.event_type, "approval_executed");
        assert_eq!(executed.audit_entry.payload["side_effect_executed"], true);
        assert_eq!(executed.plugin_results.len(), 1);
        assert_eq!(
            executed.plugin_results[0].status,
            PluginCallStatus::Completed
        );
        assert_eq!(
            executed.plugin_results[0].output,
            json!({ "message": "review me" })
        );
        assert!(executed
            .audit_entries
            .iter()
            .any(|entry| entry.event_type == "plugin_completed_after_approval"));

        let replay_error = state
            .execute_approved_approval(approval.id)
            .expect_err("approved action cannot be replayed twice");
        assert!(replay_error
            .to_string()
            .contains("has already been executed"));
    }

    #[tokio::test]
    async fn pending_first_party_action_cannot_execute_without_approval_grant() {
        let repository = SqliteRepository::in_memory().unwrap();
        let state = IpcState::with_repository(repository).expect("state");
        state
            .submit_command(CommandRequest {
                input: "plugin approval echo wait".to_string(),
                session_id: None,
                context: json!({"surface": "test"}),
                dry_run: false,
                proactive: false,
                sensitivity: Some(Sensitivity::Workspace),
            })
            .await
            .expect("approval command");
        let approval = state
            .list_approvals(Some(ApprovalStatus::Pending))
            .expect("pending approvals")
            .into_iter()
            .next()
            .expect("pending approval");

        let error = state
            .execute_approved_approval(approval.id)
            .expect_err("pending approval cannot execute");

        assert!(error
            .to_string()
            .contains("must be approved before execution"));
    }

    #[tokio::test]
    async fn permission_policy_review_summarizes_scheduler_triggers_without_commands() {
        let repository = SqliteRepository::in_memory().unwrap();
        let state = IpcState::with_repository(repository).expect("state");
        let _manual = state
            .schedule_scheduler_job(SchedulerJobSpec {
                name: "manual review job".to_string(),
                command: "do not expose manual command".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("manual scheduler job");
        let interval = state
            .schedule_scheduler_job(SchedulerJobSpec {
                name: "interval review job".to_string(),
                command: "do not expose interval command".to_string(),
                trigger: TriggerKind::Interval { every_seconds: 60 },
            })
            .expect("interval scheduler job");
        let interval_id = interval.id.to_string();

        let review = state.permission_policy_review().expect("policy review");
        assert_eq!(review.status, "review_required");
        assert_eq!(review.review_item_count, 2);
        assert!(review
            .items
            .iter()
            .any(|item| item.item_type == "manual_scheduler_trigger" && item.severity == "low"));
        assert!(review
            .items
            .iter()
            .any(|item| item.item_type == "recurring_scheduler_trigger"
                && item.severity == "medium"
                && item.action.as_deref() == Some(interval_id.as_str())));
        let encoded = serde_json::to_string(&review).expect("review JSON");
        assert!(!encoded.contains("do not expose manual command"));
        assert!(!encoded.contains("do not expose interval command"));
    }

    #[tokio::test]
    async fn permission_policy_review_summarizes_unreviewed_memory_without_values() {
        let repository = SqliteRepository::in_memory().unwrap();
        let memory = repository
            .create_memory_item(NewMemoryItem {
                category: "preference".to_string(),
                key: "voice".to_string(),
                value: "never expose this memory value".to_string(),
                provenance: "test".to_string(),
                sensitivity: Sensitivity::Private,
            })
            .expect("memory");
        repository
            .create_memory_item(NewMemoryItem {
                category: "workflow".to_string(),
                key: "release".to_string(),
                value: "reviewed memory value".to_string(),
                provenance: "test".to_string(),
                sensitivity: Sensitivity::Workspace,
            })
            .and_then(|item| repository.mark_memory_reviewed(item.id))
            .expect("reviewed memory");
        let state = IpcState::with_repository(repository).expect("state");

        let review = state.permission_policy_review().expect("policy review");

        assert_eq!(review.status, "review_required");
        assert_eq!(review.unreviewed_memory_item_count, 1);
        assert_eq!(review.sensitive_memory_item_count, 1);
        let item = review
            .items
            .iter()
            .find(|item| item.item_type == "memory_review")
            .expect("memory review item");
        assert_eq!(item.severity, "high");
        assert_eq!(item.memory_id, Some(memory.id));
        assert_eq!(item.action.as_deref(), Some("preference/voice"));
        let encoded = serde_json::to_string(&review).expect("review JSON");
        assert!(!encoded.contains("never expose this memory value"));
        assert!(!encoded.contains("reviewed memory value"));
    }

    #[tokio::test]
    async fn permission_policy_review_summarizes_deleted_sensitive_memory_without_values() {
        let repository = SqliteRepository::in_memory().unwrap();
        let sensitive = repository
            .create_memory_item(NewMemoryItem {
                category: "credential-adjacent".to_string(),
                key: "token-location".to_string(),
                value: "never expose deleted sensitive memory".to_string(),
                provenance: "test".to_string(),
                sensitivity: Sensitivity::CredentialAdjacent,
            })
            .and_then(|item| repository.delete_memory_item(item.id))
            .expect("deleted sensitive memory");
        let workspace = repository
            .create_memory_item(NewMemoryItem {
                category: "workspace".to_string(),
                key: "old-note".to_string(),
                value: "deleted workspace memory value".to_string(),
                provenance: "test".to_string(),
                sensitivity: Sensitivity::Workspace,
            })
            .and_then(|item| repository.delete_memory_item(item.id))
            .expect("deleted workspace memory");
        let state = IpcState::with_repository(repository).expect("state");

        let review = state.permission_policy_review().expect("policy review");

        let retention_items = review
            .items
            .iter()
            .filter(|item| item.item_type == "memory_retention_review")
            .collect::<Vec<_>>();
        assert_eq!(retention_items.len(), 1);
        assert_eq!(retention_items[0].severity, "high");
        assert_eq!(retention_items[0].memory_id, Some(sensitive.id));
        assert_eq!(
            retention_items[0].action.as_deref(),
            Some("credential-adjacent/token-location")
        );
        assert!(!review
            .items
            .iter()
            .any(|item| item.memory_id == Some(workspace.id)));
        let encoded = serde_json::to_string(&review).expect("review JSON");
        assert!(!encoded.contains("never expose deleted sensitive memory"));
        assert!(!encoded.contains("deleted workspace memory value"));
    }

    #[tokio::test]
    async fn command_schema_executes_first_party_plugin_with_policy_audit() {
        let state = IpcState::new();
        let response = state
            .submit_command(CommandRequest {
                input: "plugin echo hello from ipc".to_string(),
                session_id: None,
                context: json!({"surface": "test", "sensitivity": "workspace"}),
                dry_run: false,
                proactive: false,
                sensitivity: None,
            })
            .await
            .expect("plugin command");

        assert!(response.accepted);
        assert_eq!(response.plugin_results.len(), 1);
        assert_eq!(
            response.plugin_results[0].status,
            PluginCallStatus::Completed
        );
        assert_eq!(
            response.plugin_results[0].output,
            json!({ "message": "hello from ipc" })
        );
        assert!(response
            .audit_entries
            .iter()
            .any(|entry| entry.event_type == "plugin_policy_evaluated"));
        assert_eq!(response.audit_entry.event_type, "plugin_completed");
    }

    #[tokio::test]
    async fn command_dry_run_skips_first_party_plugin_execution() {
        let state = IpcState::new();
        let response = state
            .submit_command(CommandRequest {
                input: "plugin echo dry run".to_string(),
                session_id: None,
                context: json!({"surface": "test"}),
                dry_run: true,
                proactive: false,
                sensitivity: Some(Sensitivity::Workspace),
            })
            .await
            .expect("dry run command");

        assert!(response.accepted);
        assert!(response.plugin_results.is_empty());
        assert_eq!(response.audit_entry.event_type, "plugin_dry_run");
        assert!(response
            .audit_entries
            .iter()
            .any(|entry| entry.event_type == "plugin_policy_evaluated"));
    }

    #[tokio::test]
    async fn repository_backed_command_persists_ipc_task_and_audit_entries() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("jarvis.sqlite");
        let repository = SqliteRepository::open(&db_path).unwrap();
        let state = IpcState::with_repository(repository).expect("state");

        let response = state
            .submit_command(CommandRequest {
                input: "plugin echo persist me".to_string(),
                session_id: None,
                context: json!({"surface": "test"}),
                dry_run: false,
                proactive: false,
                sensitivity: Some(Sensitivity::Workspace),
            })
            .await
            .expect("persisted command");

        assert_eq!(response.task.status, TaskStatus::Completed);
        drop(state);

        let repository = SqliteRepository::open(db_path).unwrap();
        let task = repository
            .get_task(response.task.id)
            .expect("task query")
            .expect("persisted task");
        assert_eq!(task.status, TaskStatus::Completed);

        let audit_entries = repository
            .list_audit_entries(Some(response.task.id))
            .expect("audit query");
        let event_types = audit_entries
            .iter()
            .map(|entry| entry.event_type.as_str())
            .collect::<Vec<_>>();
        assert!(event_types.contains(&"task_created"));
        assert!(event_types.contains(&"model_route_selected"));
        assert!(event_types.contains(&"plugin_policy_evaluated"));
        assert!(event_types.contains(&"plugin_completed"));
    }

    #[tokio::test]
    async fn repository_backed_state_endpoints_expose_tasks_and_audit() {
        let repository = SqliteRepository::in_memory().unwrap();
        let state = IpcState::with_repository(repository).expect("state");
        let response = state
            .submit_command(CommandRequest {
                input: "plugin echo inspect state".to_string(),
                session_id: None,
                context: json!({"surface": "test"}),
                dry_run: false,
                proactive: false,
                sensitivity: Some(Sensitivity::Workspace),
            })
            .await
            .expect("command");

        let Json(tasks) = list_tasks(State(state.clone())).await.expect("tasks");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, response.task.id);

        let Json(task) = get_task(State(state.clone()), Path(response.task.id))
            .await
            .expect("task");
        assert_eq!(task.status, TaskStatus::Completed);

        let Json(entries) = list_task_audit_entries(State(state.clone()), Path(response.task.id))
            .await
            .expect("task audit");
        assert!(entries
            .iter()
            .any(|entry| entry.event_type == "plugin_completed"));

        let Json(summary) = activity_summary(State(state.clone()))
            .await
            .expect("activity summary");
        assert!(summary.repository_backed);
        assert_eq!(summary.task_count, 1);
        assert_eq!(summary.active_task_count, 0);
        assert!(summary.audit_entry_count >= entries.len());
        assert_eq!(summary.recent_tasks[0].id, response.task.id);
        let summary_json = serde_json::to_value(&summary).expect("activity summary json");
        assert!(summary_json["recent_tasks"][0].get("user_input").is_none());
        assert!(summary
            .status_counts
            .iter()
            .any(|count| count.status == TaskStatus::Completed && count.count == 1));
        assert!(summary
            .recent_audit_entries
            .iter()
            .any(|entry| entry.event_type == "plugin_completed"));

        let Json(all_entries) = list_audit_entries(State(state)).await.expect("audit");
        assert_eq!(all_entries.len(), entries.len());
    }

    #[tokio::test]
    async fn repository_backed_memory_endpoints_cover_create_update_review_delete() {
        let repository = SqliteRepository::in_memory().unwrap();
        let state = IpcState::with_repository(repository).expect("state");

        let Json(created) = create_memory_item(
            State(state.clone()),
            Json(CreateMemoryItemRequest {
                category: "workflow".to_string(),
                key: "release-gate".to_string(),
                value: "run local gate before PR".to_string(),
                provenance: "test".to_string(),
                sensitivity: Sensitivity::Workspace,
            }),
        )
        .await
        .expect("create memory");
        assert_eq!(created.key, "release-gate");

        let Json(listed) = list_memory_items(State(state.clone()), Query(HashMap::new()))
            .await
            .expect("list memory");
        assert_eq!(listed.len(), 1);

        let Json(summary) =
            memory_classification_summary(State(state.clone()), Query(HashMap::new()))
                .await
                .expect("memory classification");
        assert_eq!(summary.active_count, 1);
        assert_eq!(summary.unreviewed_active_count, 1);
        assert_eq!(summary.by_sensitivity[0].label, "workspace");

        let Json(updated) = update_memory_item(
            State(state.clone()),
            Path(created.id),
            Json(UpdateMemoryItemRequest {
                value: "run full local gate before PR".to_string(),
                provenance: "test update".to_string(),
                sensitivity: Sensitivity::Workspace,
            }),
        )
        .await
        .expect("update memory");
        assert_eq!(updated.value, "run full local gate before PR");

        let Json(reviewed) = review_memory_item(State(state.clone()), Path(created.id))
            .await
            .expect("review memory");
        assert!(reviewed.reviewed_at.is_some());

        let Json(deleted) = delete_memory_item(State(state.clone()), Path(created.id))
            .await
            .expect("delete memory");
        assert!(deleted.deleted_at.is_some());

        let Json(active) = list_memory_items(State(state.clone()), Query(HashMap::new()))
            .await
            .expect("list active memory");
        assert!(active.is_empty());

        let Json(restored) = restore_memory_item(State(state.clone()), Path(created.id))
            .await
            .expect("restore memory");
        assert!(restored.deleted_at.is_none());

        let Json(active) = list_memory_items(State(state), Query(HashMap::new()))
            .await
            .expect("list restored memory");
        assert_eq!(active.len(), 1);
    }

    #[tokio::test]
    async fn repository_backed_scheduler_jobs_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("jarvis.sqlite");
        let repository = SqliteRepository::open(&db_path).unwrap();
        let state = IpcState::with_repository(repository).expect("state");

        let Json(created) = create_scheduler_job(
            State(state.clone()),
            Json(CreateSchedulerJobRequest {
                name: "durable check".to_string(),
                command: "status check".to_string(),
                trigger: TriggerKind::Manual,
            }),
        )
        .await
        .expect("create scheduler job");
        assert_eq!(state.health().scheduler_jobs, 1);
        drop(state);

        let repository = SqliteRepository::open(&db_path).unwrap();
        let restarted = IpcState::with_repository(repository).expect("restarted state");
        let Json(jobs) = list_scheduler_jobs(State(restarted.clone())).await;
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, created.id);
        assert_eq!(jobs[0].status, crate::SchedulerJobStatus::Scheduled);

        let Json(fetched) = get_scheduler_job(State(restarted.clone()), Path(created.id))
            .await
            .expect("fetch scheduler job");
        assert_eq!(fetched.id, created.id);

        let Json(cancelled) = cancel_scheduler_job(State(restarted), Path(created.id))
            .await
            .expect("cancel scheduler job");
        assert_eq!(cancelled.status, crate::SchedulerJobStatus::Cancelled);

        let repository = SqliteRepository::open(db_path).unwrap();
        let stored = repository.list_scheduler_jobs().expect("stored jobs");
        assert_eq!(stored[0].status, crate::SchedulerJobStatus::Cancelled);
    }

    #[tokio::test]
    async fn scheduler_attention_summarizes_due_and_failed_jobs_without_commands() {
        let state = IpcState::new();
        let _ = create_scheduler_job(
            State(state.clone()),
            Json(CreateSchedulerJobRequest {
                name: "due handoff".to_string(),
                command: "do not expose this due command".to_string(),
                trigger: TriggerKind::Manual,
            }),
        )
        .await
        .expect("create due job");
        let Json(failed) = create_scheduler_job(
            State(state.clone()),
            Json(CreateSchedulerJobRequest {
                name: "failed handoff".to_string(),
                command: "do not expose this failed command".to_string(),
                trigger: TriggerKind::Manual,
            }),
        )
        .await
        .expect("create failed job");
        state.fail_scheduler_job(failed.id).expect("fail job");
        let _ = create_scheduler_job(
            State(state.clone()),
            Json(CreateSchedulerJobRequest {
                name: "future handoff".to_string(),
                command: "do not expose this future command".to_string(),
                trigger: TriggerKind::OnceAt {
                    run_at: Utc::now() + Duration::minutes(10),
                },
            }),
        )
        .await
        .expect("create future job");

        let Json(summary) = scheduler_attention(State(state)).await;
        assert!(summary.attention_required);
        assert_eq!(summary.due_count, 1);
        assert_eq!(summary.scheduled_count, 2);
        assert_eq!(summary.failed_count, 1);
        assert!(summary.next_due_at.is_some());
        assert!(summary
            .items
            .iter()
            .any(|item| item.notification_kind == "due_now" && item.name == "due handoff"));
        assert!(summary
            .items
            .iter()
            .any(|item| item.notification_kind == "failed" && item.name == "failed handoff"));

        let encoded = serde_json::to_string(&summary).expect("encode summary");
        assert!(!encoded.contains("do not expose this"));
    }

    #[tokio::test]
    async fn scheduler_attention_clears_due_jobs_after_emergency_pause_cancels_them() {
        let state = IpcState::new();
        let _ = create_scheduler_job(
            State(state.clone()),
            Json(CreateSchedulerJobRequest {
                name: "paused handoff".to_string(),
                command: "do not expose paused command".to_string(),
                trigger: TriggerKind::Manual,
            }),
        )
        .await
        .expect("create due job");
        state.pause("test pause".to_string()).expect("pause");

        let Json(summary) = scheduler_attention(State(state)).await;
        assert!(summary.emergency_paused);
        assert!(!summary.attention_required);
        assert_eq!(summary.due_count, 0);
        assert_eq!(summary.items.len(), 0);
        assert!(!serde_json::to_string(&summary)
            .expect("encode summary")
            .contains("do not expose paused command"));
    }

    #[tokio::test]
    async fn repository_backed_scheduler_lifecycle_survives_restart() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("jarvis.sqlite");
        let repository = SqliteRepository::open(&db_path).unwrap();
        let state = IpcState::with_repository(repository).expect("state");

        let completed = state
            .schedule_scheduler_job(SchedulerJobSpec {
                name: "completed".to_string(),
                command: "record completed job".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("schedule completed");
        let failed = state
            .schedule_scheduler_job(SchedulerJobSpec {
                name: "failed".to_string(),
                command: "record failed job".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("schedule failed");

        assert_eq!(
            state
                .mark_scheduler_job_running(completed.id)
                .expect("mark running")
                .status,
            SchedulerJobStatus::Running
        );
        assert_eq!(
            state
                .complete_scheduler_job(completed.id)
                .expect("complete")
                .status,
            SchedulerJobStatus::Completed
        );
        assert_eq!(
            state.fail_scheduler_job(failed.id).expect("fail").status,
            SchedulerJobStatus::Failed
        );
        drop(state);

        let repository = SqliteRepository::open(&db_path).unwrap();
        let restarted = IpcState::with_repository(repository).expect("restarted state");
        let jobs = restarted.scheduler().list();
        assert_eq!(jobs.len(), 2);
        assert!(jobs
            .iter()
            .any(|job| job.id == completed.id && job.status == SchedulerJobStatus::Completed));
        assert!(jobs
            .iter()
            .any(|job| job.id == failed.id && job.status == SchedulerJobStatus::Failed));
    }

    #[tokio::test]
    async fn run_due_scheduler_jobs_executes_and_persists_visible_tasks() {
        let repository = SqliteRepository::in_memory().unwrap();
        let scheduler = Scheduler::new();
        let now = Utc::now();
        let mut interval_job = scheduler
            .schedule(SchedulerJobSpec {
                name: "interval status".to_string(),
                command: "plugin status".to_string(),
                trigger: TriggerKind::Interval { every_seconds: 30 },
            })
            .expect("interval");
        interval_job.updated_at = now - chrono::Duration::seconds(31);
        repository
            .upsert_scheduler_job(&interval_job)
            .expect("persist interval");
        let state = IpcState::with_repository(repository).expect("state");
        let manual = state
            .schedule_scheduler_job(SchedulerJobSpec {
                name: "manual status".to_string(),
                command: "plugin status".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("manual");
        let once = state
            .schedule_scheduler_job(SchedulerJobSpec {
                name: "once status".to_string(),
                command: "plugin status".to_string(),
                trigger: TriggerKind::OnceAt {
                    run_at: now - chrono::Duration::seconds(1),
                },
            })
            .expect("once");

        let response = state.run_due_scheduler_jobs(10).await.expect("run due");

        assert!(!response.emergency_paused);
        assert_eq!(response.executions.len(), 3);
        assert!(response
            .executions
            .iter()
            .all(|execution| execution.accepted));
        assert!(response.executions.iter().any(|execution| {
            execution.job.id == manual.id && execution.job.status == SchedulerJobStatus::Completed
        }));
        assert!(response.executions.iter().any(|execution| {
            execution.job.id == once.id && execution.job.status == SchedulerJobStatus::Completed
        }));
        assert!(response.executions.iter().any(|execution| {
            execution.job.id == interval_job.id
                && execution.job.status == SchedulerJobStatus::Scheduled
        }));
        assert!(response.executions.iter().all(|execution| execution
            .audit_entries
            .iter()
            .any(|entry| entry.event_type == "scheduler_job_started")));
        assert!(response.executions.iter().all(|execution| execution
            .audit_entries
            .iter()
            .any(|entry| entry.event_type == "scheduler_job_completed"
                || entry.event_type == "scheduler_job_rescheduled")));

        let tasks = state
            .using_repository(SqliteRepository::list_tasks)
            .expect("tasks");
        assert_eq!(tasks.len(), 3);
        let audit = state
            .using_repository(|repository| repository.list_audit_entries(None))
            .expect("audit");
        let proactive_policy_audits = audit
            .iter()
            .filter(|entry| entry.event_type == "scheduler_proactive_policy_checked")
            .collect::<Vec<_>>();
        assert_eq!(proactive_policy_audits.len(), 3);
        assert!(proactive_policy_audits.iter().all(|entry| entry
            .payload
            .get("command_redacted")
            .and_then(serde_json::Value::as_bool)
            == Some(true)));
        assert!(proactive_policy_audits.iter().any(|entry| entry
            .payload
            .get("policy_review_item_type")
            .and_then(serde_json::Value::as_str)
            == Some("manual_scheduler_trigger")));
        assert!(proactive_policy_audits.iter().any(|entry| entry
            .payload
            .get("policy_review_item_type")
            .and_then(serde_json::Value::as_str)
            == Some("scheduled_scheduler_trigger")));
        assert!(proactive_policy_audits.iter().any(|entry| entry
            .payload
            .get("policy_review_item_type")
            .and_then(serde_json::Value::as_str)
            == Some("recurring_scheduler_trigger")));
        let encoded_policy_audits =
            serde_json::to_string(&proactive_policy_audits).expect("policy audit JSON");
        assert!(!encoded_policy_audits.contains("plugin status"));
        assert!(audit
            .iter()
            .any(|entry| entry.event_type == "scheduler_job_rescheduled"));
        assert!(audit
            .iter()
            .any(|entry| entry.event_type == "plugin_completed"));
        let plugin_completed_audits = audit
            .iter()
            .filter(|entry| entry.event_type == "plugin_completed")
            .collect::<Vec<_>>();
        assert_eq!(plugin_completed_audits.len(), 3);
        assert!(plugin_completed_audits.iter().all(|entry| entry
            .payload
            .get("proactive")
            .and_then(serde_json::Value::as_bool)
            == Some(true)));
    }

    #[tokio::test]
    async fn scheduler_proactive_policy_audit_matches_policy_review_classification() {
        let repository = SqliteRepository::in_memory().unwrap();
        let state = IpcState::with_repository(repository).expect("state");
        let job = state
            .schedule_scheduler_job(SchedulerJobSpec {
                name: "classified scheduled job".to_string(),
                command: "plugin status".to_string(),
                trigger: TriggerKind::OnceAt {
                    run_at: Utc::now() - chrono::Duration::seconds(1),
                },
            })
            .expect("schedule");
        let job_id = job.id.to_string();
        let review = state.permission_policy_review().expect("policy review");
        let review_item = review
            .items
            .iter()
            .find(|item| item.action.as_deref() == Some(job_id.as_str()))
            .expect("scheduler review item")
            .clone();

        let response = state.run_due_scheduler_jobs(1).await.expect("run due");

        let policy_audit = response.executions[0]
            .audit_entries
            .iter()
            .find(|entry| entry.event_type == "scheduler_proactive_policy_checked")
            .expect("policy audit");
        assert_eq!(
            policy_audit
                .payload
                .get("policy_review_item_type")
                .and_then(serde_json::Value::as_str),
            Some(review_item.item_type.as_str())
        );
        assert_eq!(
            policy_audit
                .payload
                .get("severity")
                .and_then(serde_json::Value::as_str),
            Some(review_item.severity.as_str())
        );
        assert_eq!(
            policy_audit
                .payload
                .get("side_effects_require_approval")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        let encoded = serde_json::to_string(policy_audit).expect("policy audit JSON");
        assert!(!encoded.contains("plugin status"));
    }

    #[tokio::test]
    async fn run_due_scheduler_jobs_blocks_non_proactive_plugin_actions() {
        let repository = SqliteRepository::in_memory().unwrap();
        let state = IpcState::with_repository(repository).expect("state");
        let job = state
            .schedule_scheduler_job(SchedulerJobSpec {
                name: "non proactive echo".to_string(),
                command: "plugin echo scheduler should not run".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("schedule");

        let response = state.run_due_scheduler_jobs(1).await.expect("run due");

        assert!(response.emergency_paused);
        assert_eq!(response.executions.len(), 1);
        let execution = &response.executions[0];
        assert!(!execution.accepted);
        assert_eq!(execution.job.id, job.id);
        assert_eq!(execution.job.status, SchedulerJobStatus::Failed);
        assert_eq!(execution.task.status, TaskStatus::Blocked);
        assert!(execution
            .message
            .contains("fake_echo.echo cannot run proactively"));
        assert!(execution
            .audit_entries
            .iter()
            .any(|entry| entry.event_type == "scheduler_fail_closed_emergency_pause"));

        let audit = state
            .using_repository(|repository| repository.list_audit_entries(None))
            .expect("audit");
        assert!(audit.iter().any(|entry| {
            entry.event_type == "plugin_policy_evaluated"
                && entry
                    .payload
                    .get("proactive")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
        }));
        assert!(audit.iter().any(|entry| {
            entry.event_type == "plugin_execution_blocked"
                && entry
                    .payload
                    .get("proactive")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
                && entry
                    .payload
                    .get("side_effect_executed")
                    .and_then(serde_json::Value::as_bool)
                    == Some(false)
                && entry
                    .payload
                    .get("error")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|error| error.contains("fake_echo.echo cannot run proactively"))
        }));
        assert!(!audit
            .iter()
            .any(|entry| entry.event_type == "plugin_completed"));
        let encoded = serde_json::to_string(&audit).expect("audit JSON");
        assert!(!encoded.contains("scheduler should not run"));
    }

    #[tokio::test]
    async fn recover_stale_scheduler_jobs_marks_running_jobs_failed_and_audits_redacted() {
        let repository = SqliteRepository::in_memory().unwrap();
        let state = IpcState::with_repository(repository).expect("state");
        let stale = state
            .schedule_scheduler_job(SchedulerJobSpec {
                name: "stale running".to_string(),
                command: "do not expose stale command".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("schedule stale");
        let scheduled = state
            .schedule_scheduler_job(SchedulerJobSpec {
                name: "still scheduled".to_string(),
                command: "do not recover scheduled".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("schedule");
        state
            .mark_scheduler_job_running(stale.id)
            .expect("mark running");

        let response = state
            .recover_stale_scheduler_jobs(0, 16)
            .expect("recover stale");

        assert_eq!(response.recovered.len(), 1);
        assert_eq!(response.recovered[0].job.id, stale.id);
        assert_eq!(response.recovered[0].job.status, SchedulerJobStatus::Failed);
        assert!(response.recovered[0].stale_for_seconds >= 0);
        assert_eq!(
            state.get_scheduler_job(stale.id).expect("stale").status,
            SchedulerJobStatus::Failed
        );
        assert_eq!(
            state
                .get_scheduler_job(scheduled.id)
                .expect("scheduled")
                .status,
            SchedulerJobStatus::Scheduled
        );
        let audit = state
            .using_repository(|repository| repository.list_audit_entries(None))
            .expect("audit");
        assert!(audit
            .iter()
            .any(|entry| entry.event_type == "scheduler_stale_running_recovered"));
        assert_eq!(
            response.recovered[0].audit_entry.payload["automatic_recovery"],
            false
        );
        let encoded = serde_json::to_string(&response).expect("recovery JSON");
        assert!(!encoded.contains("do not expose stale command"));
        assert!(!encoded.contains("do not recover scheduled"));
    }

    #[tokio::test]
    async fn automatic_stale_scheduler_recovery_marks_audit_without_command_text() {
        let repository = SqliteRepository::in_memory().unwrap();
        let state = IpcState::with_repository(repository).expect("state");
        let stale = state
            .schedule_scheduler_job(SchedulerJobSpec {
                name: "stale automatic running".to_string(),
                command: "do not expose automatic stale command".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("schedule stale");
        state
            .mark_scheduler_job_running(stale.id)
            .expect("mark running");

        let response = state
            .recover_stale_scheduler_jobs_automatically(0, 16)
            .expect("recover stale");

        assert_eq!(response.recovered.len(), 1);
        assert_eq!(response.recovered[0].job.id, stale.id);
        assert_eq!(response.recovered[0].job.status, SchedulerJobStatus::Failed);
        assert_eq!(
            response.recovered[0].audit_entry.payload["automatic_recovery"],
            true
        );
        assert_eq!(
            response.recovered[0].audit_entry.payload["command_redacted"],
            true
        );
        let encoded = serde_json::to_string(&response).expect("recovery JSON");
        assert!(!encoded.contains("do not expose automatic stale command"));
    }

    #[tokio::test]
    async fn run_due_scheduler_jobs_is_blocked_by_persistent_emergency_pause() {
        let repository = SqliteRepository::in_memory().unwrap();
        repository
            .set_emergency_pause(true, Some("paused before startup"), Some("test"))
            .expect("pause");
        let scheduler = Scheduler::new();
        let job = scheduler
            .schedule(SchedulerJobSpec {
                name: "paused job".to_string(),
                command: "plugin status".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("job");
        repository.upsert_scheduler_job(&job).expect("persist job");
        let state = IpcState::with_repository(repository).expect("state");

        let response = state.run_due_scheduler_jobs(10).await.expect("run due");

        assert!(response.emergency_paused);
        assert!(response.executions.is_empty());
        assert_eq!(
            state.get_scheduler_job(job.id).expect("job").status,
            SchedulerJobStatus::Scheduled
        );
        let tasks = state
            .using_repository(SqliteRepository::list_tasks)
            .expect("tasks");
        assert!(tasks.is_empty());
    }

    #[tokio::test]
    async fn background_scheduler_loop_runs_due_jobs_with_bounded_ticks() {
        let repository = SqliteRepository::in_memory().unwrap();
        let state = IpcState::with_repository(repository).expect("state");
        let first = state
            .schedule_scheduler_job(SchedulerJobSpec {
                name: "first background job".to_string(),
                command: "plugin status".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("first job");
        let second = state
            .schedule_scheduler_job(SchedulerJobSpec {
                name: "second background job".to_string(),
                command: "plugin status".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("second job");
        let loop_handle = state.spawn_scheduler_background_loop(
            SchedulerBackgroundConfig::new(std::time::Duration::from_millis(25), 1)
                .expect("config"),
        );

        tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                let tasks = state
                    .using_repository(SqliteRepository::list_tasks)
                    .expect("tasks");
                if tasks.len() >= 2 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("background scheduler should run both bounded ticks");
        loop_handle.abort();

        assert_eq!(
            state.get_scheduler_job(first.id).expect("first").status,
            SchedulerJobStatus::Completed
        );
        assert_eq!(
            state.get_scheduler_job(second.id).expect("second").status,
            SchedulerJobStatus::Completed
        );
        let audit = state
            .using_repository(|repository| repository.list_audit_entries(None))
            .expect("audit");
        assert!(
            audit
                .iter()
                .filter(|entry| entry.event_type == "scheduler_job_due")
                .count()
                >= 2
        );
        assert!(audit
            .iter()
            .any(|entry| entry.event_type == "scheduler_job_completed"));
    }

    #[tokio::test]
    async fn background_scheduler_loop_preserves_emergency_pause_blocking() {
        let repository = SqliteRepository::in_memory().unwrap();
        repository
            .set_emergency_pause(true, Some("paused before startup"), Some("test"))
            .expect("pause");
        let scheduler = Scheduler::new();
        let job = scheduler
            .schedule(SchedulerJobSpec {
                name: "paused background job".to_string(),
                command: "plugin status".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("job");
        repository.upsert_scheduler_job(&job).expect("persist job");
        let state = IpcState::with_repository(repository).expect("state");
        let loop_handle = state.spawn_scheduler_background_loop(
            SchedulerBackgroundConfig::new(std::time::Duration::from_millis(25), 8)
                .expect("config"),
        );

        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        loop_handle.abort();

        assert!(state.pause_status().paused);
        assert_eq!(
            state.get_scheduler_job(job.id).expect("job").status,
            SchedulerJobStatus::Scheduled
        );
        let tasks = state
            .using_repository(SqliteRepository::list_tasks)
            .expect("tasks");
        assert!(tasks.is_empty());
    }

    #[tokio::test]
    async fn run_due_scheduler_jobs_fail_closed_persists_pause_jobs_and_audit() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("jarvis.sqlite");
        let repository = SqliteRepository::open(&db_path).unwrap();
        let state = IpcState::with_repository(repository).expect("state");

        let approval_job = state
            .schedule_scheduler_job(SchedulerJobSpec {
                name: "approval required".to_string(),
                command: "plugin approval echo proactive stop".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("schedule approval job");
        let later_job = state
            .schedule_scheduler_job(SchedulerJobSpec {
                name: "cancel after fail closed".to_string(),
                command: "plugin status".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("schedule later job");

        let response = state.run_due_scheduler_jobs(1).await.expect("run due");

        assert!(response.emergency_paused);
        assert_eq!(response.executions.len(), 1);
        assert!(!response.executions[0].accepted);
        assert_eq!(response.executions[0].job.id, approval_job.id);
        assert_eq!(
            response.executions[0].job.status,
            SchedulerJobStatus::Failed
        );
        assert!(response.executions[0]
            .audit_entries
            .iter()
            .any(|entry| entry.event_type == "scheduler_fail_closed_emergency_pause"));
        assert_eq!(
            state
                .get_scheduler_job(later_job.id)
                .expect("later job")
                .status,
            SchedulerJobStatus::Cancelled
        );
        assert!(state.pause_status().paused);
        drop(state);

        let repository = SqliteRepository::open(&db_path).unwrap();
        let pause = repository.emergency_pause_state().expect("pause state");
        assert!(pause.paused);
        let jobs = repository.list_scheduler_jobs().expect("jobs");
        assert!(jobs
            .iter()
            .any(|job| job.id == approval_job.id && job.status == SchedulerJobStatus::Failed));
        assert!(jobs
            .iter()
            .any(|job| job.id == later_job.id && job.status == SchedulerJobStatus::Cancelled));
        let audit = repository.list_audit_entries(None).expect("audit");
        assert!(audit
            .iter()
            .any(|entry| entry.event_type == "scheduler_job_due"));
        assert!(audit
            .iter()
            .any(|entry| entry.event_type == "scheduler_fail_closed_emergency_pause"));
        assert!(audit
            .iter()
            .any(|entry| entry.event_type == "emergency_pause_activated"));
    }

    #[tokio::test]
    async fn diagnostics_export_is_redacted_and_counts_repository_state() {
        let repository = SqliteRepository::in_memory().unwrap();
        let state = IpcState::with_repository(repository).expect("state");
        state
            .schedule_scheduler_job(SchedulerJobSpec {
                name: "diagnostic schedule".to_string(),
                command: "do not redact scheduler command".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("schedule");
        state
            .using_repository(|repository| {
                repository.create_memory_item(NewMemoryItem {
                    category: "preference".to_string(),
                    key: "diagnostics".to_string(),
                    value: "diagnostic memory value should stay out".to_string(),
                    provenance: "test".to_string(),
                    sensitivity: Sensitivity::Private,
                })
            })
            .expect("memory");
        state
            .submit_command(CommandRequest {
                input: "private command body should stay out of diagnostics".to_string(),
                session_id: None,
                context: json!({"surface": "test"}),
                dry_run: true,
                proactive: false,
                sensitivity: Some(Sensitivity::Private),
            })
            .await
            .expect("command");

        let Json(export) = diagnostics_export(State(state))
            .await
            .expect("diagnostics export");
        assert_eq!(export.health.status, "ok");
        assert!(export.repository_backed);
        assert_eq!(export.schema_version, Some(9));
        assert_eq!(export.task_count, Some(1));
        assert!(export.audit_entry_count.unwrap_or_default() >= 2);
        assert_eq!(export.model_route_record_count, Some(1));
        assert_eq!(export.active_memory_item_count, Some(1));
        assert_eq!(export.unreviewed_memory_item_count, Some(1));
        assert_eq!(export.sensitive_memory_item_count, Some(1));
        assert_eq!(export.scheduler_jobs.len(), 1);
        assert!(export.redaction.contains("omits command bodies"));

        let encoded = serde_json::to_string(&export).unwrap();
        assert!(!encoded.contains("private command body"));
        assert!(!encoded.contains("do not redact scheduler command"));
        assert!(!encoded.contains("diagnostic memory value should stay out"));
    }

    #[tokio::test]
    async fn model_route_inspection_survives_restart_and_omits_context() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("jarvis.sqlite");
        let state =
            IpcState::with_repository(SqliteRepository::open(&db_path).unwrap()).expect("state");
        let response = state
            .submit_command(CommandRequest {
                input: "route secret token=do-not-store".to_string(),
                session_id: None,
                context: json!({ "surface": "test" }),
                dry_run: true,
                proactive: false,
                sensitivity: Some(Sensitivity::Private),
            })
            .await
            .expect("command");
        let task_id = response.task.id;
        let route_id = response.route_evidence.as_ref().expect("route evidence").id;
        drop(state);

        let restarted =
            IpcState::with_repository(SqliteRepository::open(&db_path).unwrap()).expect("restart");
        let Json(routes) = list_model_routes(State(restarted.clone()), Query(HashMap::new()))
            .await
            .expect("routes");
        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0].id, route_id);
        assert_eq!(routes[0].task_id, Some(task_id));
        assert_eq!(routes[0].context_for_model, None);

        let Json(task_routes) = list_model_routes(
            State(restarted.clone()),
            Query(HashMap::from([(
                "task_id".to_string(),
                task_id.to_string(),
            )])),
        )
        .await
        .expect("task routes");
        assert_eq!(task_routes.len(), 1);

        let Json(route) = get_model_route(State(restarted), Path(route_id))
            .await
            .expect("route");
        assert_eq!(route.id, route_id);
        assert_eq!(route.context_for_model, None);
        let encoded = serde_json::to_string(&route).unwrap();
        assert!(!encoded.contains("do-not-store"));
        assert!(!encoded.contains("token="));
    }

    #[tokio::test]
    async fn plugin_manifest_endpoint_lists_first_party_plugins() {
        let Json(manifests) = list_plugin_manifests(State(IpcState::new()))
            .await
            .expect("plugin manifests");

        assert!(manifests.iter().any(|manifest| manifest.id == "fake_echo"));
        assert!(manifests
            .iter()
            .any(|manifest| manifest.id == "fake_status"));

        let Json(manifest) =
            get_plugin_manifest(State(IpcState::new()), Path("fake_echo".to_string()))
                .await
                .expect("fake_echo manifest");
        assert_eq!(manifest.id, "fake_echo");
    }

    #[tokio::test]
    async fn model_tool_catalog_exposes_first_party_tools_without_installed_plugin_metadata() {
        let Json(catalog) = model_tool_catalog(State(IpcState::new()))
            .await
            .expect("model tool catalog");

        assert_eq!(catalog.source, "registered_first_party_plugins");
        assert!(catalog
            .tools
            .iter()
            .any(|tool| tool.plugin_id == "fake_echo" && tool.action == "echo"));
        assert!(catalog
            .tools
            .iter()
            .any(|tool| tool.plugin_id == "fake_status" && tool.action == "status"));
        assert!(catalog.proof_boundary.contains("installed plugins"));

        let encoded_tools = serde_json::to_string(&catalog.tools).expect("catalog tools JSON");
        assert!(!encoded_tools.contains("source_path"));
        assert!(!encoded_tools.contains("provenance"));
        assert!(!encoded_tools.contains("subprocess"));
        assert!(!encoded_tools.contains("manifest_sha256"));
    }

    #[tokio::test]
    async fn emergency_pause_blocks_commands_and_cancels_scheduler_jobs() {
        let state = IpcState::new();
        state
            .scheduler()
            .schedule(SchedulerJobSpec {
                name: "routine".to_string(),
                command: "run routine".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("schedule");

        let pause = state.pause("testing").expect("pause");
        assert!(pause.paused);
        assert_eq!(pause.cancelled_scheduler_jobs, 1);
        assert!(state.health().emergency_paused);
        assert_eq!(
            state.health().emergency_pause_reason.as_deref(),
            Some("testing")
        );

        let response = state
            .submit_command(CommandRequest {
                input: "continue".to_string(),
                session_id: None,
                context: serde_json::Value::Null,
                dry_run: false,
                proactive: false,
                sensitivity: None,
            })
            .await
            .expect("blocked command is still represented");
        assert!(!response.accepted);
        assert_eq!(response.task.status, TaskStatus::Blocked);
        assert_eq!(response.audit_entry.event_type, "emergency_pause_blocked");

        let resume = state.resume().expect("resume");
        assert!(!resume.paused);
    }

    #[tokio::test]
    async fn emergency_pause_loads_and_updates_persistent_state() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("jarvis.sqlite");
        let repository = SqliteRepository::open(&db_path).unwrap();
        let state = IpcState::with_repository(repository).expect("state");
        let scheduled = state
            .schedule_scheduler_job(SchedulerJobSpec {
                name: "persisted pause job".to_string(),
                command: "run persisted pause job".to_string(),
                trigger: TriggerKind::Manual,
            })
            .expect("schedule");

        let pause = state.pause("maintenance window").expect("pause");
        assert!(pause.paused);
        assert_eq!(pause.reason.as_deref(), Some("maintenance window"));
        assert_eq!(pause.cancelled_scheduler_jobs, 1);
        assert!(pause.paused_at.is_some());
        assert!(state.health().emergency_paused);

        drop(state);

        let repository = SqliteRepository::open(&db_path).unwrap();
        let restarted = IpcState::with_repository(repository).expect("restarted state");
        let status = restarted.pause_status();
        assert!(status.paused);
        assert_eq!(status.reason.as_deref(), Some("maintenance window"));
        assert!(restarted.health().emergency_paused);
        assert_eq!(
            restarted.health().emergency_pause_reason.as_deref(),
            Some("maintenance window")
        );

        let response = restarted
            .submit_command(CommandRequest {
                input: "still blocked".to_string(),
                session_id: None,
                context: serde_json::Value::Null,
                dry_run: false,
                proactive: false,
                sensitivity: None,
            })
            .await
            .expect("blocked command");
        assert!(!response.accepted);
        assert_eq!(response.task.status, TaskStatus::Blocked);

        let resume = restarted.resume().expect("resume");
        assert!(!resume.paused);
        assert!(resume.resumed_at.is_some());

        drop(restarted);

        let repository = SqliteRepository::open(db_path).unwrap();
        let stored = repository.emergency_pause_state().unwrap();
        assert!(!stored.paused);
        assert_eq!(stored.reason, None);
        assert_eq!(stored.updated_by.as_deref(), Some("ipc"));
        let jobs = repository.list_scheduler_jobs().unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, scheduled.id);
        assert_eq!(jobs[0].status, SchedulerJobStatus::Cancelled);
        assert!(jobs[0].cancelled_at.is_some());
        assert_eq!(
            jobs[0].cancellation_reason.as_deref(),
            Some("emergency pause: maintenance window")
        );
    }
}
