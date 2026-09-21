//! Owner-selected supervised developer runner. This is not production execution evidence.
mod developer_chat;
mod developer_github_setup;
mod developer_planning;
mod developer_process;
mod developer_publication;
mod developer_review;
mod developer_settings;
mod developer_tools;

use anyhow::{anyhow, bail, Context, Result};
use axum::{
    extract::{DefaultBodyLimit, Query, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read as _,
    path::{Component, Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use uuid::Uuid;

use assemblywright_master::current_time_ms;

use developer_chat::{
    ChatAttachment, ChatModelConfig, ChatRepairHandoff, DeveloperChat, InferenceGate,
    InferenceLease,
};
use developer_github_setup::{
    AccountRecord, CreationRecord, CreationStart, GithubSetupState, RepositoryRecord,
};
use developer_planning::{
    approved_metadata, begin_provider, bind_pending_packet, combined_plan, expected_response,
    finish_unavailable, invalidate_pending, new_session, planning_output_schema, provider_prompt,
    record_request, request_digest, ApprovedPlanMetadata, PlanningContextFile, PlanningPacket,
    PlanningProviderOutput, PlanningSession, MAX_PLANNING_SESSIONS,
};
use developer_publication::{
    validate_binding as validate_publication_binding, CandidateFile, GithubAccountObservation,
    GithubRepositoryLookup, GithubRepositoryObservation, GithubRepositoryPage, GithubSignInOutcome,
    ProjectBinding, PublicationInput, PublicationRecord, Runtime as PublicationRuntime,
};
use developer_review::{
    hex_digest, sanitize_and_validate_cloud_text, validate_cloud_text, DeveloperReviewCallError,
    DeveloperReviewDecisionKind, DeveloperReviewFile, DeveloperReviewFinding,
    DeveloperReviewOutput, DeveloperReviewPacket, DeveloperReviewer,
    PROVIDER_ID as REVIEW_PROVIDER_ID,
};
use developer_settings::{
    load_catalog, validate_model_id as validate_ai_model_id, validate_reasoning_effort,
    validate_selection, AiModelCatalog, AiSelection, DeveloperAiSettings,
    DEFAULT_MODEL as REVIEW_MODEL_ID, DEFAULT_REASONING_EFFORT,
};
use developer_tools::{
    DeveloperTools, OpenCodeRuntimeConfig, ToolChatRequest, ToolModelConfig, ToolProjectMutation,
};

const REPAIR_LIMIT: u32 = 3;
const ESCALATION_LIMIT: u32 = 100;
const DEFAULT_AUTO_AI_REPAIR_MAX_ESCALATIONS: u32 = 100;
const ESCALATION_HISTORY_LIMIT: usize = 300;
const REVIEW_HISTORY_LIMIT: usize = 104;
const REVIEWER_SELECTION_HISTORY_LIMIT: usize = 80;
const AUTO_REPAIR_REASON_LIMIT: usize = 1000;
const CODE_FAILURE_SUMMARY_LIMIT: usize = 4000;
const AUTO_REPAIR_STEP_ELAPSED_LIMIT_MS: u64 = 24 * 60 * 60 * 1_000;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
struct RepairableValidationFailure(String);

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
struct RepairableReviewRejection(String);

#[derive(Debug, thiserror::Error)]
#[error("Could not confirm validation termination; review processes before clearing Emergency Pause: {0}")]
struct UnconfirmedTermination(String);

struct ToolFeatureOutcome {
    edits: Vec<Edit>,
    application_edits: Vec<Edit>,
    workspace_revision: u64,
    applied_to_live_project: bool,
    model: String,
}

#[derive(Parser)]
struct Args {
    #[arg(long)]
    data_dir: PathBuf,
    #[arg(long)]
    workspace_root: PathBuf,
    #[arg(long, default_value = "127.0.0.1:7796")]
    bind: std::net::SocketAddr,
    #[arg(long, default_value = "http://127.0.0.1:18080/v1")]
    model_url: String,
    #[arg(long, default_value = "qwen36-local")]
    model: String,
    #[arg(long, requires = "windows_model")]
    windows_model_url: Option<String>,
    #[arg(long, requires = "windows_model_url")]
    windows_model: Option<String>,
    #[arg(long)]
    review_codex_executable: PathBuf,
    #[arg(long)]
    review_codex_home: PathBuf,
    /// Optional OpenCode executable for developer-only project tools.
    #[arg(long)]
    opencode_executable: Option<PathBuf>,
    /// Optional trusted Git executable for Developer GitHub publication.
    #[arg(long)]
    git_executable: Option<PathBuf>,
    /// Optional trusted GitHub CLI executable for Developer GitHub publication.
    #[arg(long)]
    gh_executable: Option<PathBuf>,
}

#[derive(Clone)]
struct ModelTarget {
    id: &'static str,
    name: &'static str,
    url: String,
    model: String,
}

fn default_model_target() -> String {
    "mac".into()
}
#[derive(Clone, Serialize, Deserialize)]
struct Edit {
    path: String,
    content: String,
    before: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct RepairEditEvidence {
    path: String,
    before: Option<String>,
    content_hash: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct RepairAttemptEvidence {
    attempt: u32,
    prior_checkpoint: String,
    prior_message: String,
    prior_edits: Vec<RepairEditEvidence>,
}

#[derive(Clone, Serialize, Deserialize)]
struct RepairEscalationFile {
    path: String,
    before: Option<String>,
    after: String,
    protected: bool,
}

#[derive(Clone, Serialize, Deserialize, Default)]
struct RepairEscalationProposal {
    proposal_id: String,
    attempt: u32,
    feature_id: String,
    feature_checkpoint: String,
    binding_revision: u64,
    model_target: String,
    model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chat_id: Option<String>,
    chat_request_id: String,
    chat_model_target: String,
    chat_model: String,
    diagnosis: String,
    diagnosis_sha256: String,
    status: String,
    summary: String,
    error: Option<String>,
    files: Vec<RepairEscalationFile>,
    #[serde(default)]
    protected_inputs: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    applied_paths: Vec<String>,
    #[serde(default)]
    apply_request_id: Option<String>,
    #[serde(default = "manual_escalation_source")]
    source: String,
    #[serde(default)]
    automatic_epoch: Option<u64>,
    #[serde(default)]
    policy_revision: Option<u64>,
    #[serde(default)]
    limit_snapshot: Option<u32>,
    #[serde(default)]
    project_state_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    review_slot_terminal: bool,
}

#[derive(Clone, Serialize, Deserialize, Default)]
struct RepairEscalationEvidence {
    proposal_id: String,
    attempt: u32,
    model_target: String,
    model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chat_id: Option<String>,
    chat_request_id: String,
    diagnosis_sha256: String,
    outcome: String,
    proposal_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    candidate_sha256: Option<String>,
    summary: String,
    #[serde(default = "manual_escalation_source")]
    source: String,
    #[serde(default)]
    automatic_epoch: Option<u64>,
    #[serde(default)]
    policy_revision: Option<u64>,
    #[serde(default)]
    limit_snapshot: Option<u32>,
    #[serde(default)]
    project_state_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    authorization_revision: Option<u64>,
}

fn manual_escalation_source() -> String {
    "manual_chat".into()
}

fn default_auto_repair_lifecycle() -> String {
    "inactive".into()
}

fn default_auto_ai_repair_max_escalations() -> u32 {
    DEFAULT_AUTO_AI_REPAIR_MAX_ESCALATIONS
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn legacy_review_status() -> String {
    "legacy_unreviewed".into()
}

#[derive(Clone, Serialize, Deserialize)]
struct ReviewPendingEvidence {
    attempt: u32,
    packet_sha256: String,
    validation_evidence_sha256: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct ReviewAttemptEvidence {
    attempt: u32,
    packet_sha256: String,
    validation_evidence_sha256: String,
    outcome: String,
    decision_sha256: Option<String>,
    #[serde(default)]
    blocking_findings: Vec<DeveloperReviewFinding>,
    summary: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct ReviewerSelectionEvidence {
    revision: u64,
    prior_model: String,
    prior_reasoning_effort: String,
    selected_model: String,
    selected_reasoning_effort: String,
}
#[derive(Clone, Serialize, Deserialize)]
struct FrozenPublicationFile {
    path: String,
    before_sha256: Option<String>,
    content_sha256: String,
    content: String,
    #[serde(default = "legacy_unclassified_review_file")]
    classification: String,
}

fn legacy_unclassified_review_file() -> String {
    "unclassified_legacy".into()
}
#[derive(Clone, Serialize, Deserialize)]
struct Feature {
    id: String,
    project: String,
    instruction: String,
    validation: String,
    status: String,
    checkpoint: String,
    message: String,
    edits: Option<Vec<Edit>>,
    #[serde(default)]
    repair_attempts: u32,
    #[serde(default)]
    repair_pending: bool,
    #[serde(default)]
    repair_history: Vec<RepairAttemptEvidence>,
    #[serde(default)]
    escalation_count: u32,
    #[serde(default)]
    escalation_pending: bool,
    #[serde(default)]
    escalation_proposal: Option<RepairEscalationProposal>,
    #[serde(default)]
    escalation_history: Vec<RepairEscalationEvidence>,
    #[serde(default)]
    auto_ai_repair_limit: Option<u32>,
    #[serde(default = "default_auto_repair_lifecycle")]
    auto_repair_lifecycle: String,
    #[serde(default)]
    auto_repair_reason: String,
    #[serde(default)]
    auto_repair_epoch: u64,
    #[serde(default)]
    auto_repair_step_started_at_ms: Option<u64>,
    #[serde(default)]
    auto_repair_policy_revision: Option<u64>,
    #[serde(default)]
    escalation_evidence_reserved: u32,
    #[serde(default)]
    review_evidence_reserved: u32,
    #[serde(default)]
    last_failure_kind: String,
    #[serde(default)]
    last_code_failure_summary: String,
    #[serde(default)]
    auto_repair_limit_project_baseline: std::collections::BTreeMap<String, String>,
    #[serde(default)]
    auto_repair_limit_unadmitted_sha256: Option<String>,
    #[serde(default)]
    auto_repair_limit_volatile_sha256: Option<String>,
    #[serde(default = "default_model_target")]
    model_target: String,
    #[serde(default = "legacy_review_status")]
    review_status: String,
    #[serde(default = "default_review_model")]
    review_model: String,
    #[serde(default = "default_review_reasoning_effort")]
    review_reasoning_effort: String,
    #[serde(default)]
    review_binding_version: u8,
    #[serde(default)]
    review_attempts: u32,
    #[serde(default)]
    review_summary: String,
    #[serde(default)]
    review_pending: Option<ReviewPendingEvidence>,
    #[serde(default)]
    review_history: Vec<ReviewAttemptEvidence>,
    #[serde(default)]
    reviewer_selection_history: Vec<ReviewerSelectionEvidence>,
    #[serde(default)]
    planning: Option<ApprovedPlanMetadata>,
    #[serde(default)]
    cumulative_evidence_version: u8,
    #[serde(default)]
    tool_workspace_revision: u64,
    #[serde(default)]
    publication_selection_frozen: bool,
    #[serde(default)]
    publication_binding: Option<ProjectBinding>,
    #[serde(default)]
    publication_candidate: Vec<FrozenPublicationFile>,
    #[serde(default)]
    publication: Option<PublicationRecord>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Snapshot {
    revision: u64,
    auto_run: bool,
    #[serde(default)]
    auto_ai_repair_enabled: bool,
    #[serde(default = "default_auto_ai_repair_max_escalations")]
    auto_ai_repair_max_escalations: u32,
    #[serde(default)]
    auto_ai_repair_policy_revision: u64,
    #[serde(default)]
    auto_ai_repair_last_request_sha256: String,
    emergency_paused: bool,
    #[serde(
        rename = "queue_v11",
        alias = "queue_v10",
        alias = "queue_v9",
        alias = "queue_v8",
        alias = "queue_v7",
        alias = "queue_v6",
        alias = "queue_v5",
        alias = "queue_v4",
        alias = "queue_v3",
        alias = "queue_v2",
        alias = "queue"
    )]
    queue: Vec<Feature>,
    #[serde(default)]
    github_connections: Vec<ProjectBinding>,
    #[serde(default)]
    planning_sessions: Vec<PlanningSession>,
    #[serde(default)]
    ai_settings: DeveloperAiSettings,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            revision: 0,
            auto_run: false,
            auto_ai_repair_enabled: false,
            auto_ai_repair_max_escalations: DEFAULT_AUTO_AI_REPAIR_MAX_ESCALATIONS,
            auto_ai_repair_policy_revision: 0,
            auto_ai_repair_last_request_sha256: String::new(),
            emergency_paused: false,
            queue: Vec::new(),
            github_connections: Vec::new(),
            planning_sessions: Vec::new(),
            ai_settings: DeveloperAiSettings::default(),
        }
    }
}
struct Database {
    connection: Connection,
    state: Snapshot,
    github_setup: GithubSetupState,
}

#[derive(Clone)]
enum PlanningCompletion {
    Output(Box<PlanningProviderOutput>),
    Unavailable(String),
}

#[derive(Clone)]
struct PlanningCompletionRecovery {
    packet: PlanningPacket,
    completion: PlanningCompletion,
}

struct PublicationRunningGuard<'a>(&'a AtomicBool);

impl Drop for PublicationRunningGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

struct RunningStartGuard<'a> {
    running: &'a AtomicBool,
    committed: bool,
}

struct EscalationCallGuard<'a> {
    engine: &'a Engine,
    cancellation: Arc<AtomicU8>,
    released: bool,
}

impl EscalationCallGuard<'_> {
    fn release(mut self) {
        self.engine.release_escalation_call(&self.cancellation);
        self.released = true;
    }
}

impl Drop for EscalationCallGuard<'_> {
    fn drop(&mut self) {
        if !self.released {
            self.engine.release_escalation_call(&self.cancellation);
        }
    }
}

impl RunningStartGuard<'_> {
    fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for RunningStartGuard<'_> {
    fn drop(&mut self) {
        if !self.committed {
            self.running.store(false, Ordering::SeqCst);
        }
    }
}

fn mutate_database<T>(
    db: &mut Database,
    f: impl FnOnce(&mut Snapshot, &mut GithubSetupState) -> Result<T>,
) -> Result<T> {
    let mut next_state = db.state.clone();
    let mut next_setup = db.github_setup.clone();
    let result = f(&mut next_state, &mut next_setup)?;
    next_state.revision = next_state
        .revision
        .checked_add(1)
        .context("Revision overflow")?;
    let data = serde_json::to_string(&next_state)?;
    {
        let transaction = db.connection.transaction()?;
        transaction.execute("INSERT INTO developer_state(id,state) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET state=excluded.state", [data])?;
        GithubSetupState::persist_in(&transaction, &next_setup)?;
        transaction.commit()?;
    }
    db.state = next_state;
    db.github_setup = next_setup;
    Ok(result)
}

fn validate_auto_repair_max(max_escalations: u32) -> Result<()> {
    if !(1..=ESCALATION_LIMIT).contains(&max_escalations) {
        bail!("Auto AI repair maximum must be between 1 and {ESCALATION_LIMIT}");
    }
    Ok(())
}

fn bounded_code_failure_summary(value: &str) -> Result<String> {
    if value.trim().is_empty() {
        bail!("Automatic repair code-failure evidence is empty");
    }
    let bounded = value
        .chars()
        .take(CODE_FAILURE_SUMMARY_LIMIT + 1)
        .collect::<String>();
    if bounded.chars().count() > CODE_FAILURE_SUMMARY_LIMIT {
        bail!("Automatic repair code-failure evidence exceeds its bounded limit");
    }
    sanitize_and_validate_cloud_text(&bounded)
        .context("Automatic repair code-failure evidence could not be safely sanitized")
}

fn durable_failure_summary(value: &str, category: &str, limit: usize) -> String {
    let bounded = value.chars().take(limit).collect::<String>();
    sanitize_and_validate_cloud_text(&bounded).unwrap_or_else(|_| {
        format!(
            "{category}; sensitive details remain only in the owner-local log (sha256 {})",
            hash(value.as_bytes())
        )
    })
}

fn set_auto_repair_lifecycle(feature: &mut Feature, lifecycle: &str, reason: &str) -> Result<()> {
    if !matches!(
        lifecycle,
        "inactive" | "running" | "held" | "limit_reached" | "quarantined"
    ) {
        bail!("Unknown automatic repair lifecycle");
    }
    if reason.len() > AUTO_REPAIR_REASON_LIMIT {
        bail!("Automatic repair reason exceeds its bounded limit");
    }
    feature.auto_repair_lifecycle = lifecycle.into();
    feature.auto_repair_reason = reason.into();
    if lifecycle == "running" {
        start_auto_repair_step(feature)?;
    } else {
        feature.auto_repair_step_started_at_ms = None;
    }
    Ok(())
}

fn start_auto_repair_step(feature: &mut Feature) -> Result<()> {
    start_auto_repair_step_at(feature, current_time_ms()?)
}

fn start_auto_repair_step_at(feature: &mut Feature, started_at_ms: u64) -> Result<()> {
    if feature.auto_repair_lifecycle != "running" {
        bail!("Automatic repair step cannot start outside the running lifecycle");
    }
    if started_at_ms == 0 {
        bail!("Automatic repair step timestamp must be nonzero");
    }
    feature.auto_repair_step_started_at_ms = Some(started_at_ms);
    Ok(())
}

fn snapshot_feature_auto_repair_limit(
    feature: &mut Feature,
    max_escalations: u32,
    policy_revision: u64,
) -> Result<bool> {
    validate_auto_repair_max(max_escalations)?;
    if let Some(limit) = feature.auto_ai_repair_limit {
        validate_auto_repair_max(limit)?;
        if feature.escalation_count > limit {
            bail!("Persisted repair escalation count exceeds the feature limit snapshot");
        }
        return Ok(false);
    }
    if feature.escalation_count > max_escalations {
        bail!("Existing repair escalations exceed the selected feature limit");
    }
    feature.auto_ai_repair_limit = Some(max_escalations);
    feature.auto_repair_policy_revision = Some(policy_revision);
    Ok(true)
}

fn arm_auto_repair_at_execution_start(
    feature: &mut Feature,
    enabled: bool,
    max_escalations: u32,
    policy_revision: u64,
) -> Result<bool> {
    let snapshotted =
        snapshot_feature_auto_repair_limit(feature, max_escalations, policy_revision)?;
    if !enabled
        || feature.auto_repair_lifecycle != "inactive"
        || feature.status != "queued"
        || feature.checkpoint != "not_started"
    {
        return Ok(snapshotted);
    }
    feature.auto_repair_policy_revision = Some(policy_revision);
    feature.auto_repair_epoch = feature
        .auto_repair_epoch
        .checked_add(1)
        .context("Automatic repair epoch overflow")?;
    set_auto_repair_lifecycle(
        feature,
        "running",
        "Auto AI repair was armed when this queued feature began execution",
    )?;
    Ok(true)
}

fn arm_selected_queued_feature(state: &mut Snapshot, feature_id: &str) -> Result<bool> {
    let enabled = state.auto_ai_repair_enabled;
    let max_escalations = state.auto_ai_repair_max_escalations;
    let policy_revision = state.auto_ai_repair_policy_revision;
    let feature = state
        .queue
        .iter_mut()
        .find(|candidate| candidate.id == feature_id)
        .context("feature missing")?;
    arm_auto_repair_at_execution_start(feature, enabled, max_escalations, policy_revision)
}

fn reserve_escalation_evidence(feature: &mut Feature) -> Result<()> {
    let limit = feature
        .auto_ai_repair_limit
        .context("Feature has no automatic repair limit snapshot")?;
    validate_auto_repair_max(limit)?;
    if feature.escalation_count >= limit || feature.escalation_count >= ESCALATION_LIMIT {
        set_auto_repair_lifecycle(
            feature,
            "limit_reached",
            &format!(
                "{} of {} AI escalations used. Automatic repair stopped. Correct the project without AI and revalidate, change the reviewer when applicable, or remove the feature.",
                feature.escalation_count, limit
            ),
        )?;
        bail!("Repair escalation limit reached");
    }
    if feature.escalation_history.len().saturating_add(3) > ESCALATION_HISTORY_LIMIT
        || feature
            .escalation_evidence_reserved
            .checked_add(3)
            .context("Repair escalation evidence reservation overflow")?
            > ESCALATION_HISTORY_LIMIT as u32
    {
        set_auto_repair_lifecycle(
            feature,
            "held",
            "Automatic repair stopped because its bounded escalation evidence capacity is unavailable. Inspect the feature evidence before resuming.",
        )?;
        bail!("Repair escalation evidence capacity reached");
    }
    if feature.review_history.len().saturating_add(1) > REVIEW_HISTORY_LIMIT
        || feature
            .review_evidence_reserved
            .checked_add(1)
            .context("Review evidence reservation overflow")?
            > REVIEW_HISTORY_LIMIT as u32
    {
        set_auto_repair_lifecycle(
            feature,
            "held",
            "Automatic repair stopped because its bounded independent-review evidence capacity is unavailable. Inspect the feature evidence before resuming.",
        )?;
        bail!("Independent review evidence capacity reached");
    }
    feature.escalation_evidence_reserved += 3;
    feature.review_evidence_reserved += 1;
    feature.escalation_count += 1;
    Ok(())
}

fn validate_auto_repair_feature(feature: &Feature) -> Result<()> {
    if let Some(limit) = feature.auto_ai_repair_limit {
        validate_auto_repair_max(limit)?;
        if feature.escalation_count > limit {
            bail!("Persisted repair escalation count exceeds its feature limit");
        }
    }
    if !matches!(
        feature.auto_repair_lifecycle.as_str(),
        "inactive" | "running" | "held" | "limit_reached" | "quarantined"
    ) {
        bail!("Persisted automatic repair lifecycle is unsupported");
    }
    if feature.auto_repair_reason.len() > AUTO_REPAIR_REASON_LIMIT {
        bail!("Persisted automatic repair reason exceeds its bounded limit");
    }
    match (
        feature.auto_repair_lifecycle.as_str(),
        feature.auto_repair_step_started_at_ms,
    ) {
        ("running", Some(0)) => {
            bail!("Persisted automatic repair step timestamp is invalid")
        }
        ("running", _) | (_, None) => {}
        (_, Some(_)) => {
            bail!("Persisted inactive automatic repair has an active step timestamp")
        }
    }
    if feature.escalation_history.len() > ESCALATION_HISTORY_LIMIT
        || feature.escalation_evidence_reserved > ESCALATION_HISTORY_LIMIT as u32
        || feature.review_history.len() > REVIEW_HISTORY_LIMIT
        || feature.review_evidence_reserved > REVIEW_HISTORY_LIMIT as u32
    {
        bail!("Persisted automatic repair evidence exceeds its bounded capacity");
    }
    if !matches!(
        feature.last_failure_kind.as_str(),
        "" | "validation_failure" | "review_rejection" | "operational"
    ) {
        bail!("Persisted automatic repair failure classification is unsupported");
    }
    if !feature.last_code_failure_summary.is_empty() {
        let validated = bounded_code_failure_summary(&feature.last_code_failure_summary)?;
        if validated != feature.last_code_failure_summary {
            bail!("Persisted automatic repair code-failure evidence is not safely redacted");
        }
    }
    if feature.auto_repair_limit_project_baseline.len() > 80 {
        bail!("Persisted automatic repair limit baseline exceeds its file bound");
    }
    for (path, digest) in &feature.auto_repair_limit_project_baseline {
        if path.is_empty()
            || path.contains('\\')
            || path.contains(':')
            || Path::new(path)
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
            || repair_sensitive_path(path)
        {
            bail!("Persisted automatic repair limit baseline has an invalid path");
        }
        validate_sha256(digest, "automatic repair limit baseline digest")?;
    }
    if let Some(digest) = &feature.auto_repair_limit_unadmitted_sha256 {
        validate_sha256(digest, "automatic repair unadmitted-data digest")?;
    } else if !feature.auto_repair_limit_project_baseline.is_empty() {
        bail!("Persisted automatic repair limit baseline has no unadmitted-data binding");
    }
    if let Some(digest) = &feature.auto_repair_limit_volatile_sha256 {
        validate_sha256(digest, "automatic repair volatile-data digest")?;
    } else if !feature.auto_repair_limit_project_baseline.is_empty() {
        bail!("Persisted automatic repair limit baseline has no volatile-data binding");
    }
    for evidence in &feature.escalation_history {
        if evidence.authorization_revision.is_some()
            && (evidence.source != "automatic_failure" || evidence.outcome != "policy_authorized")
        {
            bail!("Only automatic policy authorization evidence may carry a runner revision");
        }
    }
    let paused_cancellation_lookalike = feature.status == "paused"
        && (feature
            .checkpoint
            .ends_with("_cancelled_before_authorization")
            || feature
                .escalation_proposal
                .as_ref()
                .is_some_and(|proposal| {
                    proposal.source == "automatic_failure" && proposal.status == "cancelled"
                }));
    if paused_cancellation_lookalike {
        validate_paused_preauthorization_cancellation(feature)
            .context("Persisted paused automatic cancellation is invalid")?;
    }
    Ok(())
}

fn validate_authorization_revision(
    evidence: &RepairEscalationEvidence,
    loaded_queue_version: u8,
    persisted_revision: u64,
) -> Result<()> {
    let requires_revision = loaded_queue_version >= 11
        && evidence.source == "automatic_failure"
        && evidence.outcome == "policy_authorized";
    match evidence.authorization_revision {
        Some(revision)
            if evidence.source == "automatic_failure"
                && evidence.outcome == "policy_authorized"
                && revision > 0
                && revision <= persisted_revision =>
        {
            Ok(())
        }
        None if !requires_revision => Ok(()),
        Some(_) => bail!("Persisted automatic repair policy authorization revision is invalid"),
        None => bail!("Persisted automatic policy authorization has no runner revision"),
    }
}

fn migrate_and_validate_auto_repair_step_timestamp(
    feature: &mut Feature,
    now_ms: u64,
) -> Result<()> {
    if now_ms == 0 {
        bail!("Current time is invalid for automatic repair recovery");
    }
    if feature.auto_repair_lifecycle == "running" {
        match feature.auto_repair_step_started_at_ms {
            Some(0) => bail!("Persisted automatic repair step timestamp is invalid"),
            Some(started_at_ms) if started_at_ms > now_ms => {
                bail!("Persisted automatic repair step timestamp is in the future")
            }
            Some(_) => {}
            None => feature.auto_repair_step_started_at_ms = Some(now_ms),
        }
    } else if feature.auto_repair_step_started_at_ms.is_some() {
        bail!("Persisted inactive automatic repair has an active step timestamp");
    }
    Ok(())
}

fn projected_auto_repair_step_elapsed_ms(feature: &Feature, now_ms: u64) -> Option<u64> {
    if feature.auto_repair_lifecycle != "running" {
        return None;
    }
    feature.auto_repair_step_started_at_ms.map(|started_at_ms| {
        now_ms
            .saturating_sub(started_at_ms)
            .min(AUTO_REPAIR_STEP_ELAPSED_LIMIT_MS)
    })
}

fn validate_escalation_build_binding(
    feature: &Feature,
    proposal: &RepairEscalationProposal,
) -> Result<()> {
    if proposal.feature_id != feature.id {
        bail!("Repair proposal feature binding changed");
    }
    if proposal.source == "automatic_failure" {
        let active_checkpoint = format!("escalation_{}_preparing", proposal.attempt);
        if feature.checkpoint != active_checkpoint {
            bail!("Automatic repair proposal active checkpoint changed");
        }
    } else if proposal.feature_checkpoint != feature.checkpoint {
        bail!("Repair proposal feature binding changed");
    }
    Ok(())
}

fn automatic_code_failure_is_eligible(feature: &Feature) -> bool {
    matches!(
        feature.last_failure_kind.as_str(),
        "validation_failure" | "review_rejection"
    ) || (feature.last_failure_kind.is_empty()
        && (feature.checkpoint == "validation_failed"
            || feature.checkpoint.ends_with("_validation_failed")
            || feature.checkpoint.starts_with("review_")
                && feature.checkpoint.ends_with("_rejected")))
}

fn paused_preauthorization_cancellation_is_resumable(feature: &Feature) -> bool {
    validate_paused_preauthorization_cancellation(feature).is_ok()
}

fn validate_paused_preauthorization_cancellation(feature: &Feature) -> Result<()> {
    let proposal = feature
        .escalation_proposal
        .as_ref()
        .context("Paused automatic cancellation has no proposal evidence")?;
    if feature.status != "paused"
        || feature.auto_repair_lifecycle != "inactive"
        || !automatic_code_failure_is_eligible(feature)
        || feature.escalation_pending
        || proposal.source != "automatic_failure"
        || proposal.status != "cancelled"
        || proposal.apply_request_id.is_some()
        || !proposal.applied_paths.is_empty()
        || feature.checkpoint
            != format!(
                "escalation_{}_cancelled_before_authorization",
                proposal.attempt
            )
        || automatic_post_apply_is_ambiguous(feature)
    {
        bail!("Paused automatic cancellation recovery binding is invalid");
    }
    if feature.escalation_count != proposal.attempt
        || proposal.limit_snapshot != feature.auto_ai_repair_limit
        || proposal.policy_revision.is_none()
        || proposal.policy_revision != feature.auto_repair_policy_revision
        || proposal
            .automatic_epoch
            .and_then(|epoch| epoch.checked_add(1))
            != Some(feature.auto_repair_epoch)
    {
        bail!("Paused automatic cancellation epoch or policy binding is invalid");
    }
    let evidence = feature
        .escalation_history
        .iter()
        .filter(|evidence| evidence.proposal_id == proposal.proposal_id)
        .collect::<Vec<_>>();
    if evidence.len() != 3
        || evidence
            .iter()
            .filter(|evidence| matches!(evidence.outcome.as_str(), "ready" | "cancelled"))
            .count()
            != 1
        || evidence
            .iter()
            .filter(|evidence| evidence.outcome == "authorization_not_run")
            .count()
            != 1
        || evidence
            .iter()
            .filter(|evidence| evidence.outcome == "application_not_run")
            .count()
            != 1
        || evidence.iter().any(|evidence| {
            evidence.attempt != proposal.attempt
                || evidence.source != proposal.source
                || evidence.automatic_epoch != proposal.automatic_epoch
                || evidence.policy_revision != proposal.policy_revision
                || evidence.limit_snapshot != proposal.limit_snapshot
                || evidence.project_state_sha256 != proposal.project_state_sha256
        })
    {
        bail!("Paused automatic cancellation terminal evidence is incomplete or duplicated");
    }
    if !proposal.review_slot_terminal {
        bail!("Paused automatic cancellation review slot is not terminal");
    }
    let candidate_sha256 = repair_escalation_candidate_sha256(proposal)?;
    let matching_review = feature
        .review_history
        .iter()
        .filter(|review| {
            review.attempt == proposal.attempt
                && review.outcome == "not_run"
                && review.packet_sha256 == candidate_sha256
                && review.validation_evidence_sha256 == proposal.diagnosis_sha256
        })
        .count();
    if matching_review != 1 {
        bail!("Paused automatic cancellation review evidence is missing or duplicated");
    }
    Ok(())
}

fn enable_auto_repair_for_failed_feature(
    feature: &mut Feature,
    code_failure_eligible: bool,
    max_escalations: u32,
    policy_revision: u64,
) -> Result<bool> {
    if matches!(
        feature.auto_repair_lifecycle.as_str(),
        "quarantined" | "limit_reached"
    ) {
        bail!("Auto AI repair cannot re-arm a quarantined or limit-reached feature");
    }
    let resumes_cancelled_proposal = paused_preauthorization_cancellation_is_resumable(feature);
    let resumes_ordinary_pre_effect = feature.status == "paused"
        && feature.auto_repair_lifecycle == "held"
        && automatic_ordinary_pre_effect_checkpoint(feature);
    if resumes_cancelled_proposal {
        feature.status = "failed".into();
    }
    snapshot_feature_auto_repair_limit(feature, max_escalations, policy_revision)?;
    // Every accepted off-to-on owner mutation is a new exact authorization.
    // Rebind even an operationally held feature so a later eligible failure
    // cannot inherit the prior policy revision. The snapshotted budget and
    // already-consumed count remain unchanged.
    feature.auto_repair_policy_revision = Some(policy_revision);
    feature.auto_repair_epoch = feature
        .auto_repair_epoch
        .checked_add(1)
        .context("Automatic repair epoch overflow")?;
    if !code_failure_eligible && !resumes_ordinary_pre_effect {
        set_auto_repair_lifecycle(
            feature,
            "held",
            "Auto AI repair did not start because the retained failure is operational, unavailable, or lacks an exact code-failure classification. Correct the condition and explicitly Resume.",
        )?;
        return Ok(false);
    }
    set_auto_repair_lifecycle(
        feature,
        "running",
        "Auto AI repair was enabled for this failed feature",
    )?;
    if feature.repair_attempts < REPAIR_LIMIT && !feature.repair_pending {
        reserve_repair_attempt(feature)?;
    }
    Ok(true)
}

/// Exact bounded inputs for one repair-proposal inference call. The optional
/// inference lease is retained for the entire call so the shared model gate
/// stays held without otherwise affecting proposal generation.
struct EscalationProposalInput<'a> {
    feature_id: &'a str,
    proposal_id: &'a str,
    target: &'a ModelTarget,
    handoff: Option<&'a ChatRepairHandoff>,
    automatic_failure_summary: Option<&'a str>,
    cancellation: &'a AtomicU8,
    inference_lease: Option<InferenceLease>,
}

struct Engine {
    database: Mutex<Database>,
    effect_gate: Mutex<()>,
    running: AtomicBool,
    cancellation: AtomicU8,
    tool_cancellation: Arc<AtomicBool>,
    planning_cancellation: Mutex<Option<Arc<AtomicU8>>>,
    planning_running: AtomicBool,
    planning_completion_recovery: Mutex<Option<PlanningCompletionRecovery>>,
    escalation_cancellation: Mutex<Option<Arc<AtomicU8>>>,
    escalation_running: AtomicBool,
    repair_loop_authorized: AtomicBool,
    publication_running: AtomicBool,
    publication_connection_running: AtomicBool,
    publication_cancellation: AtomicU8,
    shutdown: AtomicBool,
    root: PathBuf,
    data: PathBuf,
    token: String,
    model_targets: Vec<ModelTarget>,
    inference_gate: Arc<InferenceGate>,
    chat: Arc<DeveloperChat>,
    tools: Arc<DeveloperTools>,
    reviewer: DeveloperReviewer,
    ai_catalog: AiModelCatalog,
    publication_runtime: Option<Arc<PublicationRuntime>>,
    publication_unavailable_reason: Option<String>,
}

fn default_review_model() -> String {
    REVIEW_MODEL_ID.into()
}

fn default_review_reasoning_effort() -> String {
    DEFAULT_REASONING_EFFORT.into()
}

fn github_account_record(observation: &GithubAccountObservation) -> AccountRecord {
    match observation {
        GithubAccountObservation::SignedIn { login } => AccountRecord {
            state: "signed_in".into(),
            login: Some(login.clone()),
            message: format!("Signed in to GitHub as {login}"),
        },
        GithubAccountObservation::SignedOut { message } => AccountRecord {
            state: "signed_out".into(),
            login: None,
            message: (*message).into(),
        },
        GithubAccountObservation::Unavailable { message } => AccountRecord {
            state: "unavailable".into(),
            login: None,
            message: (*message).into(),
        },
    }
}

fn github_repository_record(observation: GithubRepositoryObservation) -> RepositoryRecord {
    RepositoryRecord {
        name_with_owner: observation.name_with_owner,
        url: observation.url,
        visibility: observation.visibility,
        default_branch: observation.default_branch,
        can_push: observation.can_push,
    }
}

fn record_observed_repository(
    creation: &mut CreationRecord,
    observation: &GithubRepositoryObservation,
) {
    creation.repository_url = Some(observation.url.clone());
    creation.repository_id = Some(observation.repository_id);
    creation.default_branch = Some(observation.default_branch.clone());
}

fn github_sign_in_completion(outcome: &GithubSignInOutcome) -> (&'static str, &'static str) {
    if !outcome.credentials_consistent {
        return (
            "attention",
            "Environment credentials mask or obscure the saved GitHub login; reconcile the retained operation after removing the conflict",
        );
    }
    match &outcome.account {
        GithubAccountObservation::SignedIn { .. } => (
            "succeeded",
            "GitHub sign-in completed and the live account was verified",
        ),
        GithubAccountObservation::SignedOut { .. } if outcome.cancelled => (
            "cancelled",
            "GitHub sign-in was cancelled and no signed-in account was observed",
        ),
        GithubAccountObservation::SignedOut { .. } => (
            "failed",
            "GitHub sign-in did not produce a verified signed-in account",
        ),
        GithubAccountObservation::Unavailable { .. } => (
            "attention",
            "GitHub sign-in ended but the live account could not be verified; reconcile the retained operation",
        ),
    }
}

fn apply_github_creation_observation(
    creation: &mut CreationRecord,
    observation: Result<GithubRepositoryLookup>,
) {
    match observation {
        Ok(GithubRepositoryLookup::Absent) => {
            creation.repository_url = None;
            creation.repository_id = None;
            creation.default_branch = None;
            creation.state = "absent".into();
            creation.message =
                "The exact GitHub repository target was observed absent; a new operation ID may be used"
                    .into();
        }
        Ok(GithubRepositoryLookup::Present(repository)) => {
            let exact_identity = repository
                .name_with_owner
                .eq_ignore_ascii_case(&creation.name_with_owner)
                && repository.visibility == creation.visibility;
            record_observed_repository(creation, &repository);
            if creation.preflight_absent && creation.command_succeeded && exact_identity {
                creation.state = "succeeded".into();
                creation.message =
                    "Repository creation was verified by immutable GitHub repository identity"
                        .into();
            } else {
                creation.state = "attention".into();
                creation.message = "The repository exists, but the exact creation effect could not be proved; reconcile without replaying creation".into();
            }
        }
        Err(_) => {
            creation.state = "attention".into();
            creation.message = "Repository creation may have changed GitHub, but the exact target could not be observed; reconcile without replaying creation".into();
        }
    }
}

impl Engine {
    fn reserve_escalation_call(&self) -> Result<Arc<AtomicU8>> {
        let mut active = self
            .escalation_cancellation
            .lock()
            .map_err(|_| anyhow!("repair escalation cancellation lock failed"))?;
        if active.is_some() {
            bail!("Another repair escalation is running");
        }
        let cancellation = Arc::new(AtomicU8::new(0));
        *active = Some(cancellation.clone());
        self.escalation_running.store(true, Ordering::SeqCst);
        Ok(cancellation)
    }

    fn cancel_escalation_call(&self, emergency: bool) {
        if let Ok(active) = self.escalation_cancellation.lock() {
            if let Some(cancellation) = active.as_ref() {
                cancellation.fetch_max(if emergency { 2 } else { 1 }, Ordering::SeqCst);
            }
        }
    }

    fn release_escalation_call(&self, cancellation: &Arc<AtomicU8>) {
        if let Ok(mut active) = self.escalation_cancellation.lock() {
            if active
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, cancellation))
            {
                *active = None;
                self.escalation_running.store(false, Ordering::SeqCst);
            }
        }
    }

    fn reserve_planning_call(&self) -> Result<Arc<AtomicU8>> {
        if self
            .planning_completion_recovery
            .lock()
            .map_err(|_| anyhow!("planning completion recovery lock failed"))?
            .is_some()
        {
            bail!("Wait for the previous planning completion to finish recovery");
        }
        let mut active = self
            .planning_cancellation
            .lock()
            .map_err(|_| anyhow!("planning cancellation lock failed"))?;
        if active.is_some() {
            bail!("Another planning request is running");
        }
        let cancellation = Arc::new(AtomicU8::new(0));
        *active = Some(cancellation.clone());
        self.planning_running.store(true, Ordering::SeqCst);
        Ok(cancellation)
    }

    fn cancel_planning_call(&self, emergency: bool) {
        if let Ok(active) = self.planning_cancellation.lock() {
            if let Some(cancellation) = active.as_ref() {
                cancellation.fetch_max(if emergency { 2 } else { 1 }, Ordering::SeqCst);
            }
        }
    }

    fn release_planning_call(&self, cancellation: &Arc<AtomicU8>) {
        if let Ok(mut active) = self.planning_cancellation.lock() {
            if active
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, cancellation))
            {
                *active = None;
                self.planning_running.store(false, Ordering::SeqCst);
            }
        }
    }

    fn finish_planning_completion(
        &self,
        packet: &PlanningPacket,
        completion: PlanningCompletion,
    ) -> Result<()> {
        let completion = if self.cancellation.load(Ordering::SeqCst) == 2 {
            PlanningCompletion::Unavailable(
                "Emergency Pause interrupted the planning request; retry after clearing the pause."
                    .into(),
            )
        } else {
            completion
        };
        let persisted = self.change(|state| {
            if self.emergency_paused(state) {
                bail!("Clear Emergency Pause before completing brainstorming");
            }
            let session = state
                .planning_sessions
                .iter_mut()
                .find(|session| session.feature_id == packet.feature_id)
                .context("Planning session not found")?;
            match &completion {
                PlanningCompletion::Output(output) => {
                    let mut next = session.clone();
                    match developer_planning::apply_provider_output(
                        &mut next,
                        packet,
                        (**output).clone(),
                    ) {
                        Ok(()) => {
                            *session = next;
                            Ok(())
                        }
                        Err(error) => finish_unavailable(
                            session,
                            packet.revision,
                            &format!("Planning response rejected: {error}"),
                        ),
                    }
                }
                PlanningCompletion::Unavailable(error) => {
                    finish_unavailable(session, packet.revision, error)
                }
            }
        });
        let retryable =
            persisted.is_err() && self.planning_packet_is_pending(packet).unwrap_or(true);
        let recovery_completion =
            if persisted.is_err() && self.cancellation.load(Ordering::SeqCst) == 2 {
                PlanningCompletion::Unavailable(
                "Emergency Pause interrupted the planning request; retry after clearing the pause."
                    .into(),
            )
            } else {
                completion
            };
        let mut recovery = self
            .planning_completion_recovery
            .lock()
            .map_err(|_| anyhow!("planning completion recovery lock failed"))?;
        if persisted.is_ok() {
            if recovery.as_ref().is_some_and(|recovery| {
                recovery.packet.feature_id == packet.feature_id
                    && recovery.packet.request_id == packet.request_id
            }) {
                *recovery = None;
            }
        } else if retryable {
            *recovery = Some(PlanningCompletionRecovery {
                packet: packet.clone(),
                completion: recovery_completion,
            });
        } else if recovery.as_ref().is_some_and(|recovery| {
            recovery.packet.feature_id == packet.feature_id
                && recovery.packet.request_id == packet.request_id
        }) {
            *recovery = None;
        }
        persisted
    }

    fn planning_packet_is_pending(&self, packet: &PlanningPacket) -> Result<bool> {
        let database = self
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?;
        Ok(database
            .state
            .planning_sessions
            .iter()
            .find(|session| session.feature_id == packet.feature_id)
            .is_some_and(|session| {
                session.running
                    && session.pending_revision == Some(packet.revision)
                    && session.pending_request_id.as_deref() == Some(packet.request_id.as_str())
                    && packet.sha256().is_ok_and(|digest| {
                        session.pending_packet_sha256.as_deref() == Some(&digest)
                    })
            }))
    }

    fn retry_planning_completion(&self, feature_id: &str, request_id: &str) -> Result<bool> {
        let recovery = self
            .planning_completion_recovery
            .lock()
            .map_err(|_| anyhow!("planning completion recovery lock failed"))?
            .as_ref()
            .filter(|recovery| {
                recovery.packet.feature_id == feature_id && recovery.packet.request_id == request_id
            })
            .cloned();
        let Some(recovery) = recovery else {
            return Ok(false);
        };
        self.finish_planning_completion(&recovery.packet, recovery.completion)?;
        Ok(true)
    }

    fn retry_planning_completion_for_feature(&self, feature_id: &str) -> Result<bool> {
        {
            let database = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            if self.emergency_paused(&database.state) {
                return Ok(false);
            }
        }
        let recovery = self
            .planning_completion_recovery
            .lock()
            .map_err(|_| anyhow!("planning completion recovery lock failed"))?
            .as_ref()
            .filter(|recovery| recovery.packet.feature_id == feature_id)
            .cloned();
        let Some(recovery) = recovery else {
            return Ok(false);
        };
        self.finish_planning_completion(&recovery.packet, recovery.completion)?;
        Ok(true)
    }

    fn publication_unresolved(state: &Snapshot) -> bool {
        state.queue.iter().any(|feature| {
            feature.publication.as_ref().is_some_and(|publication| {
                matches!(
                    publication.status.as_str(),
                    "pending" | "running" | "attention"
                )
            })
        })
    }

    fn ensure_publication_barrier_clear(
        &self,
        state: &Snapshot,
        github_setup: &GithubSetupState,
    ) -> Result<()> {
        if self.publication_connection_running.load(Ordering::SeqCst) {
            bail!("Wait for the GitHub setup or connection operation to finish");
        }
        if github_setup.blocks_dependent_work() {
            bail!("Reconcile the unfinished GitHub setup operation before changing developer work");
        }
        if self.publication_running.load(Ordering::SeqCst) || Self::publication_unresolved(state) {
            bail!("Resolve the unfinished GitHub publication before changing developer work");
        }
        Ok(())
    }

    fn change<T>(&self, f: impl FnOnce(&mut Snapshot) -> Result<T>) -> Result<T> {
        self.change_database(|state, _github_setup| f(state))
    }

    fn change_with_commit<T>(
        &self,
        f: impl FnOnce(&mut Snapshot) -> Result<T>,
        after_commit: impl FnOnce(),
    ) -> Result<T> {
        let result = self.change(f)?;
        after_commit();
        Ok(result)
    }

    fn change_database<T>(
        &self,
        f: impl FnOnce(&mut Snapshot, &mut GithubSetupState) -> Result<T>,
    ) -> Result<T> {
        let mut db = self
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?;
        mutate_database(&mut db, f)
    }
    fn feature(
        &self,
        id: &str,
        status: &str,
        checkpoint: Option<&str>,
        message: &str,
    ) -> Result<()> {
        self.change(|s| {
            let f = s
                .queue
                .iter_mut()
                .find(|f| f.id == id)
                .context("feature missing")?;
            f.status = status.to_owned();
            if let Some(checkpoint) = checkpoint {
                f.checkpoint = checkpoint.to_owned();
            }
            f.message = message.chars().take(4000).collect();
            Ok(())
        })
    }
    fn snapshot(&self) -> Result<Value> {
        let db = self
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?;
        self.snapshot_locked(&db)
    }

    fn snapshot_locked(&self, db: &Database) -> Result<Value> {
        let emergency_paused = self.emergency_paused(&db.state);
        let publication_unresolved = Self::publication_unresolved(&db.state);
        let github_setup_unresolved = db.github_setup.blocks_dependent_work();
        let github_setup_busy = self.publication_connection_running.load(Ordering::SeqCst);
        let reviewer_selection_idle = !self.shutdown.load(Ordering::SeqCst)
            && !emergency_paused
            && !publication_unresolved
            && !github_setup_unresolved
            && self.developer_work_is_idle();
        let projection_now_ms = current_time_ms().context("System clock is invalid")?;
        let queue: Vec<Value> = db
            .state
            .queue
            .iter()
            .filter(|f| f.status != "removed")
            .map(|f| {
                let effective_binding = f.publication_binding.as_ref().or_else(|| {
                    (!f.publication_selection_frozen && f.status != "succeeded")
                        .then(|| db.state.github_connections.iter().find(|binding| binding.project == f.project))
                        .flatten()
                });
                let publication_expected = f.publication.is_none() && effective_binding.is_some();
                let mut projected = json!({
                "id":f.id,"project":f.project,"instruction":f.instruction,"validation":f.validation,
                "status":f.status,"checkpoint":f.checkpoint,"message":f.message,
                "repair_attempts":f.repair_attempts,"model_target":f.model_target,
                "escalation_count":f.escalation_count,
                "escalation_status":f.escalation_proposal.as_ref().map(|proposal| proposal.status.as_str()).unwrap_or("idle"),
                "escalation_active":f.escalation_proposal.as_ref().is_some_and(|proposal| proposal.status == "preparing"),
                "review_status":f.review_status,"review_model":f.review_model,
                "review_reasoning_effort":f.review_reasoning_effort,
                "review_summary":f.review_summary,"review_attempts":f.review_attempts,
                "can_change_reviewer":reviewer_selection_idle && feature_reviewer_state_is_changeable(f),
                "planning_status":if f.planning.is_some() { "approved" } else { "legacy_unplanned" },
                "planning":f.planning,
                "tool_workspace_revision":f.tool_workspace_revision,
                "publication_status":f.publication.as_ref().map(|publication| publication.status.as_str()).unwrap_or(if publication_expected { "pending" } else { "local_only" }),
                "publication_stage":f.publication.as_ref().map(|publication| publication.stage.as_str()).unwrap_or(if publication_expected { "prepare_candidate" } else { "local_only" }),
                "publication_message":f.publication.as_ref().map(|publication| publication.message.as_str()).unwrap_or(if publication_expected { "GitHub publication starts after validation and independent review" } else { "This feature remains local to the Developer workspace" }),
                "publication_repository_url":f.publication.as_ref().map(|publication| publication.repository_url.as_str()).or_else(|| effective_binding.map(|binding| binding.repository_url.as_str())),
                "publication_base_branch":f.publication.as_ref().map(|publication| publication.base_branch.as_str()).or_else(|| effective_binding.map(|binding| binding.base_branch.as_str())),
                "publication_branch":f.publication.as_ref().map(|publication| publication.feature_branch.as_str()),
                "publication_commit_sha":f.publication.as_ref().and_then(|publication| publication.commit_sha.as_deref()),
                "publication_pr_url":f.publication.as_ref().and_then(|publication| publication.pr_url.as_deref()),
                "publication_merged_sha":f.publication.as_ref().and_then(|publication| publication.merged_sha.as_deref()),
                "can_reconcile_publication":self.publication_runtime.is_some()
                    && !emergency_paused
                    && !self.shutdown.load(Ordering::SeqCst)
                    && !self.publication_running.load(Ordering::SeqCst)
                    && !self.publication_connection_running.load(Ordering::SeqCst)
                    && f.publication.as_ref().is_some_and(|publication| publication.status == "attention"),
                "changed_files":f.edits.as_ref().map(|e| e.iter().map(|e| &e.path).collect::<Vec<_>>()).unwrap_or_default()
                });
                let object = projected.as_object_mut().unwrap();
                object.insert("auto_ai_repair_limit".into(), json!(f.auto_ai_repair_limit));
                object.insert("auto_repair_lifecycle".into(), json!(f.auto_repair_lifecycle));
                object.insert("auto_repair_reason".into(), json!(f.auto_repair_reason));
                object.insert("auto_repair_epoch".into(), json!(f.auto_repair_epoch));
                object.insert(
                    "auto_repair_step_started_at_ms".into(),
                    json!(f
                        .auto_repair_step_started_at_ms
                        .map(|started_at_ms| started_at_ms.min(projection_now_ms))),
                );
                if let Some(elapsed_ms) =
                    projected_auto_repair_step_elapsed_ms(f, projection_now_ms)
                {
                    object.insert("auto_repair_step_elapsed_ms".into(), json!(elapsed_ms));
                }
                object.insert("last_failure_kind".into(), json!(f.last_failure_kind));
                object.insert(
                    "can_revalidate_after_auto_repair_limit".into(),
                    json!(f.auto_repair_lifecycle == "limit_reached"
                        && matches!(f.status.as_str(), "failed" | "paused")),
                );
                projected
            })
            .collect();
        let model_targets: Vec<Value> = self
            .model_targets
            .iter()
            .map(|target| json!({"id":target.id,"name":target.name,"model":target.model}))
            .collect();
        let running = self.running.load(Ordering::SeqCst);
        let planning_running = self.planning_running.load(Ordering::SeqCst);
        let planning_sessions: Vec<Value> = db.state.planning_sessions.iter().map(|session| json!({
            "feature_id":session.feature_id,"project":session.project,"instruction":session.instruction,
            "stage":session.stage,"revision":session.revision,"running":session.running,
            "model":session.model,"reasoning_effort":session.reasoning_effort
        })).collect();
        let repair_active = running && db.state.queue.iter().any(|f| f.repair_pending);
        let github_connections: Vec<Value> = db
            .state
            .github_connections
            .iter()
            .map(|binding| {
                json!({
                    "project":binding.project,
                    "repository_url":binding.repository_url,
                    "base_branch":binding.base_branch,
                    "automatic_merge":true
                })
            })
            .collect();
        let publication_running = self.publication_running.load(Ordering::SeqCst);
        let can_manage_github_connections = !publication_unresolved
            && !github_setup_unresolved
            && !emergency_paused
            && !self.shutdown.load(Ordering::SeqCst)
            && self.developer_work_is_idle();
        let mut projected = json!({"mode":"supervised_developer","host":std::env::var("COMPUTERNAME").unwrap_or_else(|_|"local".into()),
            "workspace_root":self.root,"revision":db.state.revision,"auto_run":db.state.auto_run,
            "emergency_paused":emergency_paused,"running":running,"queue":queue,
            "planning_required":true,"planning_provider":REVIEW_PROVIDER_ID,
            "planning_model":db.state.ai_settings.orchestrator.model,
            "planning_reasoning_effort":db.state.ai_settings.orchestrator.reasoning_effort,
            "planning_running":planning_running,"planning_sessions":planning_sessions,
            "repair_limit":REPAIR_LIMIT,"repair_active":repair_active,"model_targets":model_targets,
            "escalation_running":self.escalation_running.load(Ordering::SeqCst),
            "chat_model_selection":true,
            "chat_history":true,
            "chat_running":self.chat.is_running() || self.tools.is_running(),
            "tools_running":self.tools.is_running(),
            "tools_need_attention":self.tools.needs_attention(),
            "review_provider":REVIEW_PROVIDER_ID,
            "review_model":db.state.ai_settings.reviewer.model,
            "review_reasoning_effort":db.state.ai_settings.reviewer.reasoning_effort,
            "review_required":true,
            "feature_reviewer_selection":true,
            "github_publication_supported":true,
            "github_publication_available":self.publication_runtime.is_some(),
            "github_publication_running":publication_running,
            "github_publication_unresolved":publication_unresolved,
            "can_manage_github_connections":can_manage_github_connections,
            "github_publication_message":self.publication_unavailable_reason,
            "github_connections":github_connections,
            "github_setup_busy":github_setup_busy,
            "github_setup_unresolved":github_setup_unresolved,
            "ai_settings":db.state.ai_settings,
            "ai_models":self.ai_catalog.models,
            "ai_catalog_source":self.ai_catalog.source});
        let object = projected.as_object_mut().unwrap();
        object.insert(
            "auto_ai_repair_enabled".into(),
            json!(db.state.auto_ai_repair_enabled),
        );
        object.insert(
            "auto_ai_repair_max_escalations".into(),
            json!(db.state.auto_ai_repair_max_escalations),
        );
        object.insert(
            "auto_ai_repair_policy_revision".into(),
            json!(db.state.auto_ai_repair_policy_revision),
        );
        Ok(projected)
    }

    fn update_settings(
        &self,
        expected_revision: u64,
        orchestrator: AiSelection,
        reviewer: AiSelection,
    ) -> Result<Value> {
        validate_selection(&self.ai_catalog, &orchestrator)?;
        validate_selection(&self.ai_catalog, &reviewer)?;
        {
            let mut db = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            self.ensure_publication_barrier_clear(&db.state, &db.github_setup)?;
            if self.running.load(Ordering::SeqCst)
                || self.planning_running.load(Ordering::SeqCst)
                || self.escalation_running.load(Ordering::SeqCst)
                || self.chat.is_running()
                || self.tools.blocks_work()
            {
                bail!("Stop active developer work before changing AI settings");
            }
            if db.state.ai_settings.revision != expected_revision {
                bail!("AI settings revision changed; refresh before saving");
            }
            let mut next = db.state.clone();
            next.ai_settings = DeveloperAiSettings {
                revision: expected_revision
                    .checked_add(1)
                    .context("AI settings revision overflow")?,
                orchestrator,
                reviewer,
            };
            next.revision = next.revision.checked_add(1).context("Revision overflow")?;
            let data = serde_json::to_string(&next)?;
            db.connection.execute("INSERT INTO developer_state(id,state) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET state=excluded.state", [data])?;
            db.state = next;
        }
        self.snapshot()
    }

    fn update_auto_ai_repair(
        self: &Arc<Self>,
        expected_revision: u64,
        enabled: bool,
        max_escalations: u32,
    ) -> Result<Value> {
        validate_auto_repair_max(max_escalations)?;
        let request_sha256 = hash(&serde_json::to_vec(&json!({
            "schema_version": 1,
            "enabled": enabled,
            "max_escalations": max_escalations,
            "expected_revision": expected_revision,
        }))?);
        let mut inference_lease = None;
        let mut start_loop = false;
        let mut cancel_loop = false;
        let disabling_transition = if !enabled {
            let db = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            db.state.revision == expected_revision && db.state.auto_ai_repair_enabled
        } else {
            false
        };
        let policy_effect_guard = if disabling_transition {
            Some(
                self.effect_gate
                    .lock()
                    .map_err(|_| anyhow!("effect gate failed"))?,
            )
        } else {
            None
        };
        let accepted;
        {
            let mut db = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            self.ensure_publication_barrier_clear(&db.state, &db.github_setup)?;
            if db.state.revision != expected_revision {
                let exact_replay = expected_revision.checked_add(1) == Some(db.state.revision)
                    && db.state.auto_ai_repair_enabled == enabled
                    && db.state.auto_ai_repair_max_escalations == max_escalations
                    && db.state.auto_ai_repair_last_request_sha256 == request_sha256;
                if exact_replay {
                    drop(db);
                    return self.snapshot();
                }
                bail!("Runner revision changed; refresh before changing Auto AI repair");
            }
            if db.state.auto_ai_repair_enabled
                && db.state.auto_ai_repair_max_escalations != max_escalations
            {
                bail!("Disable Auto AI repair before changing the maximum escalation limit");
            }
            if db.state.auto_ai_repair_enabled == enabled
                && db.state.auto_ai_repair_max_escalations == max_escalations
            {
                drop(db);
                return self.snapshot();
            }
            if self.shutdown.load(Ordering::SeqCst) {
                bail!("Developer runner is shutting down");
            }
            if disabling_transition {
                self.repair_loop_authorized.store(false, Ordering::SeqCst);
                self.cancellation.fetch_max(1, Ordering::SeqCst);
                self.tool_cancellation.store(true, Ordering::SeqCst);
            }
            let first =
                db.state.queue.iter().position(|feature| {
                    feature.status != "succeeded" && feature.status != "removed"
                });
            let enabling = enabled && !db.state.auto_ai_repair_enabled;
            let automatic_start_eligible = first.is_some_and(|index| {
                let feature = &db.state.queue[index];
                !matches!(
                    feature.auto_repair_lifecycle.as_str(),
                    "quarantined" | "limit_reached"
                ) && ((feature.status == "failed" && automatic_code_failure_is_eligible(feature))
                    || paused_preauthorization_cancellation_is_resumable(feature)
                    || (feature.status == "paused"
                        && feature.auto_repair_lifecycle == "held"
                        && automatic_ordinary_pre_effect_checkpoint(feature)))
            });
            if enabling {
                if self.running.load(Ordering::SeqCst)
                    || self.planning_running.load(Ordering::SeqCst)
                    || self.escalation_running.load(Ordering::SeqCst)
                    || self.chat.is_running()
                    || self.tools.blocks_work()
                    || self.publication_running.load(Ordering::SeqCst)
                    || self.publication_connection_running.load(Ordering::SeqCst)
                {
                    bail!("Stop active developer work before enabling Auto AI repair");
                }
                if self.emergency_paused(&db.state) {
                    bail!("Clear Emergency Pause before enabling Auto AI repair");
                }
                if automatic_start_eligible {
                    if first
                        .is_some_and(|index| db.state.queue[index].repair_attempts < REPAIR_LIMIT)
                    {
                        inference_lease = Some(self.inference_gate.try_acquire().context(
                            "Automatic repair cannot start while another model call is active",
                        )?);
                    }
                    start_loop = true;
                }
            }
            let next_revision = db
                .state
                .revision
                .checked_add(1)
                .context("Revision overflow")?;
            let mut next = db.state.clone();
            next.auto_ai_repair_enabled = enabled;
            next.auto_ai_repair_max_escalations = max_escalations;
            next.auto_ai_repair_policy_revision = next_revision;
            next.auto_ai_repair_last_request_sha256 = request_sha256;
            if let Some(index) = first {
                let feature = &mut next.queue[index];
                if enabling
                    && matches!(
                        feature.auto_repair_lifecycle.as_str(),
                        "quarantined" | "limit_reached"
                    )
                {
                    // The global preference may be enabled for future features,
                    // but it cannot clear a durable recovery boundary.
                } else if enabling && automatic_start_eligible {
                    enable_auto_repair_for_failed_feature(
                        feature,
                        true,
                        max_escalations,
                        next_revision,
                    )?;
                } else if enabling && feature.status == "failed" {
                    enable_auto_repair_for_failed_feature(
                        feature,
                        false,
                        max_escalations,
                        next_revision,
                    )?;
                } else if !enabled && feature.auto_repair_lifecycle == "running" {
                    pause_preapply_automatic_proposal_for_disable(
                        feature,
                        "Auto AI repair was disabled before proposal authorization. The unapplied proposal was retired without review; Resume revalidates the retained project without replaying proposal generation or writes.",
                    )?;
                    persist_automatic_cancellation_intent(
                        feature,
                        self.running.load(Ordering::SeqCst),
                        "Auto AI repair was disabled; cancellation was recorded before active work was signalled",
                        "Auto AI repair was disabled after project files were applied. Validation or review completion is ambiguous, so ordinary Resume is blocked.",
                    )?;
                    cancel_loop = true;
                }
            }
            next.revision = next_revision;
            let data = serde_json::to_string(&next)?;
            db.connection.execute(
                "INSERT INTO developer_state(id,state) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET state=excluded.state",
                [data],
            )?;
            db.state = next;
            accepted = self.snapshot_locked(&db)?;
        }
        let launch_guard = if start_loop {
            let guard = self
                .effect_gate
                .lock()
                .map_err(|_| anyhow!("effect gate failed"))?;
            let db = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            let feature_binding_matches = db.state.queue.iter().any(|feature| {
                feature.status != "succeeded"
                    && feature.status != "removed"
                    && feature.auto_repair_lifecycle == "running"
                    && feature.auto_repair_policy_revision
                        == Some(db.state.auto_ai_repair_policy_revision)
            });
            if db.state.emergency_paused
                || !db.state.auto_ai_repair_enabled
                || db.state.auto_ai_repair_policy_revision != expected_revision + 1
                || !feature_binding_matches
                || self.cancellation.load(Ordering::SeqCst) >= 2
            {
                self.repair_loop_authorized.store(false, Ordering::SeqCst);
                bail!("Auto AI repair launch was superseded by Emergency Pause or policy drift; the durable state requires explicit reconciliation");
            }
            drop(db);
            Some(guard)
        } else {
            None
        };
        if cancel_loop {
            self.repair_loop_authorized.store(false, Ordering::SeqCst);
            self.cancellation.fetch_max(1, Ordering::SeqCst);
            self.tool_cancellation.store(true, Ordering::SeqCst);
            self.cancel_escalation_call(false);
        } else if start_loop {
            self.cancellation.store(0, Ordering::SeqCst);
            self.tool_cancellation.store(false, Ordering::SeqCst);
            self.repair_loop_authorized.store(true, Ordering::SeqCst);
            if self
                .running
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                self.repair_loop_authorized.store(false, Ordering::SeqCst);
                self.cancellation.fetch_max(1, Ordering::SeqCst);
                bail!("Another developer run started after policy commit; automatic repair is fail-closed and requires explicit reconciliation");
            }
            self.launch(inference_lease);
        }
        drop(launch_guard);
        drop(policy_effect_guard);
        Ok(accepted)
    }

    fn update_feature_reviewer(
        &self,
        id: &str,
        expected_revision: u64,
        expected_checkpoint: &str,
        expected_model: &str,
        expected_reasoning_effort: &str,
        reviewer: AiSelection,
    ) -> Result<Value> {
        Uuid::parse_str(id).context("Invalid feature ID")?;
        validate_selection(&self.ai_catalog, &reviewer)?;
        {
            let mut db = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            self.ensure_publication_barrier_clear(&db.state, &db.github_setup)?;
            if self.shutdown.load(Ordering::SeqCst) {
                bail!("Developer runner is shutting down");
            }
            if self.running.load(Ordering::SeqCst)
                || self.planning_running.load(Ordering::SeqCst)
                || self.escalation_running.load(Ordering::SeqCst)
                || self.chat.is_running()
                || self.tools.blocks_work()
            {
                bail!("Stop active developer work before changing a feature reviewer");
            }
            if self.emergency_paused(&db.state) {
                bail!("Clear Emergency Pause before changing a feature reviewer");
            }
            if db.state.revision != expected_revision {
                bail!("Runner revision changed; refresh before changing the feature reviewer");
            }
            let mut next = db.state.clone();
            let next_revision = next.revision.checked_add(1).context("Revision overflow")?;
            let feature = next
                .queue
                .iter_mut()
                .find(|feature| feature.id == id)
                .context("Feature not found")?;
            if feature.checkpoint != expected_checkpoint
                || feature.review_model != expected_model
                || feature.review_reasoning_effort != expected_reasoning_effort
            {
                bail!("Feature reviewer binding changed; refresh before saving");
            }
            change_feature_reviewer(feature, reviewer, next_revision)?;
            next.revision = next_revision;
            let data = serde_json::to_string(&next)?;
            db.connection.execute("INSERT INTO developer_state(id,state) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET state=excluded.state", [data])?;
            db.state = next;
        }
        self.snapshot()
    }

    fn model_target(&self, id: &str) -> Result<&ModelTarget> {
        self.model_targets
            .iter()
            .find(|target| target.id == id)
            .with_context(|| {
                format!(
                    "{} model target is unavailable because this runner was not started with its model configuration",
                    model_target_name(id)
                )
            })
    }
    fn cancelled(&self) -> bool {
        self.cancellation.load(Ordering::SeqCst) != 0
    }

    fn quarantine_unconfirmed_validation_cleanup(
        &self,
        feature_id: &str,
        candidate_evidence: Option<(u64, &[Edit])>,
    ) -> Result<()> {
        let _effect_guard = self
            .effect_gate
            .lock()
            .map_err(|_| anyhow!("effect gate failed"))?;
        // Cleanup ambiguity is itself an emergency boundary. Latch every
        // process-local admission gate before persistence so a failed database
        // write cannot reopen the runner while a validation process may live.
        self.repair_loop_authorized.store(false, Ordering::SeqCst);
        self.cancellation.fetch_max(2, Ordering::SeqCst);
        self.publication_cancellation.fetch_max(2, Ordering::SeqCst);
        self.tool_cancellation.store(true, Ordering::SeqCst);

        let summary = "Validation process-tree cleanup could not be confirmed. Emergency Pause is active; inspect and terminate any remaining validation processes before clearing it. The feature is quarantined and will not replay automatically.";
        let persisted = self.change(|state| {
            state.emergency_paused = true;
            let binding_revision = state.revision + 1;
            let current = state
                .queue
                .iter_mut()
                .find(|candidate| candidate.id == feature_id)
                .context("feature missing")?;
            if current.escalation_pending {
                finish_escalation_application(
                    current,
                    binding_revision,
                    "interrupted",
                    summary,
                )?;
            }
            if let Some(pending) = current.review_pending.as_ref() {
                let attempt = pending.attempt;
                interrupt_pending_review(current, summary)?;
                if current.checkpoint != format!("review_{attempt}_interrupted") {
                    bail!("Interrupted validation review evidence was not durably terminalized");
                }
            }
            if let Some((workspace_revision, live_edits)) = candidate_evidence {
                current.tool_workspace_revision = workspace_revision;
                if !live_edits.is_empty() {
                    current.edits = Some(merge_review_edits(
                        current.edits.as_deref().unwrap_or_default(),
                        live_edits,
                    )?);
                }
            }
            current.status = "failed".into();
            current.checkpoint = "validation_cleanup_unconfirmed".into();
            current.message = summary.into();
            current.repair_pending = false;
            current.last_failure_kind = "operational".into();
            set_auto_repair_lifecycle(
                current,
                "quarantined",
                "Validation cleanup is unconfirmed. Emergency Pause blocks all work; inspect remaining processes before clearing it.",
            )?;
            for session in &mut state.planning_sessions {
                invalidate_pending(
                    session,
                    "Emergency Pause followed unconfirmed validation cleanup; retry only after process inspection and clearing the pause.",
                )?;
            }
            Ok(())
        });

        self.cancel_planning_call(true);
        self.cancel_escalation_call(true);
        self.chat.cancel_for_emergency();
        self.tools.cancel_for_emergency();
        if let Ok(mut recovery) = self.planning_completion_recovery.lock() {
            *recovery = None;
        }
        persisted
    }

    fn emergency_paused(&self, state: &Snapshot) -> bool {
        state.emergency_paused || self.cancellation.load(Ordering::SeqCst) == 2
    }

    fn developer_work_is_idle(&self) -> bool {
        !self.running.load(Ordering::SeqCst)
            && !self.planning_running.load(Ordering::SeqCst)
            && !self.escalation_running.load(Ordering::SeqCst)
            && !self.chat.is_running()
            && !self.tools.blocks_work()
            && !self.publication_running.load(Ordering::SeqCst)
            && !self.publication_connection_running.load(Ordering::SeqCst)
    }

    fn reconcile_completed_tool_mutations(&self) -> Result<()> {
        if self.running.load(Ordering::SeqCst)
            || self.tools.is_running()
            || self.chat.is_running()
            || self.publication_running.load(Ordering::SeqCst)
            || self.publication_connection_running.load(Ordering::SeqCst)
        {
            return Ok(());
        }
        let database = self
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?;
        if Self::publication_unresolved(&database.state)
            || database.github_setup.blocks_dependent_work()
        {
            return Ok(());
        }
        drop(database);
        let features = self
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?
            .state
            .queue
            .iter()
            .filter(|feature| feature.status != "removed")
            .cloned()
            .collect::<Vec<_>>();
        let mut side_targets = std::collections::HashMap::<String, String>::new();
        for feature in &features {
            if feature.status != "succeeded" {
                side_targets
                    .entry(feature.project.clone())
                    .or_insert_with(|| feature.id.clone());
            }
        }
        for feature in features.iter().rev() {
            side_targets
                .entry(feature.project.clone())
                .or_insert_with(|| feature.id.clone());
        }
        for feature in features {
            if !self.root.join(&feature.project).exists()
                && never_started_project_has_no_tool_ledger(&feature)
            {
                // Planning may enqueue a brand-new project before the Assembly Line
                // creates its directory. There can be no project-tool mutation yet.
                continue;
            }
            let mutations = self
                .tools
                .project_mutations(&feature.project, feature.tool_workspace_revision)?;
            let latest_revision = mutations
                .last()
                .map_or(feature.tool_workspace_revision, |mutation| {
                    mutation.revision
                });
            if latest_revision == feature.tool_workspace_revision {
                continue;
            }
            let owned = mutations
                .iter()
                .filter(|mutation| match mutation.feature_id.as_deref() {
                    Some(feature_id) => feature_id == feature.id,
                    None => side_targets.get(&feature.project) == Some(&feature.id),
                })
                .cloned()
                .collect::<Vec<_>>();
            let edits = owned_tool_mutation_edits(&owned, &feature.id);
            self.change(|state| {
                let current = state
                    .queue
                    .iter_mut()
                    .find(|candidate| candidate.id == feature.id)
                    .context("feature missing")?;
                reconcile_feature_tool_mutation(current, latest_revision, &edits)
            })?;
        }
        Ok(())
    }
    fn approved_plan_text(&self, feature: &Feature) -> Result<Option<String>> {
        let Some(metadata) = &feature.planning else {
            return Ok(None);
        };
        if metadata.feature_id != feature.id || metadata.original_instruction != feature.instruction
        {
            bail!("Approved plan no longer matches the feature identity and request");
        }
        let database = self
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?;
        let session = database
            .state
            .planning_sessions
            .iter()
            .find(|session| session.feature_id == feature.id)
            .context("Approved planning session is missing")?;
        if session.stage != "enqueued" {
            bail!("Approved planning session is not enqueued");
        }
        let mut ready = session.clone();
        ready.stage = "ready".into();
        ready.revision = metadata.approved_revision;
        if approved_metadata(&ready)? != *metadata {
            bail!("Approved planning evidence changed");
        }
        Ok(Some(combined_plan(metadata)))
    }
    fn start(
        self: &Arc<Self>,
        expected_feature_id: Option<&str>,
        expected_model_target: Option<&str>,
        expected_status: Option<&str>,
        expected_checkpoint: Option<&str>,
    ) -> Result<()> {
        self.reconcile_completed_tool_mutations()?;
        let mut db = self
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?;
        self.ensure_publication_barrier_clear(&db.state, &db.github_setup)?;
        if self.shutdown.load(Ordering::SeqCst) {
            bail!("Developer runner is shutting down");
        }
        if self.emergency_paused(&db.state) {
            bail!("Clear Emergency Pause before resuming");
        }
        if self.planning_running.load(Ordering::SeqCst) {
            bail!("Wait for brainstorming to finish before starting the Assembly Line");
        }
        if self.escalation_running.load(Ordering::SeqCst) {
            bail!("Wait for the repair proposal to finish before starting the Assembly Line");
        }
        if self.tools.blocks_work() {
            bail!("Wait for the active project tool action to finish before starting the Assembly Line");
        }
        let feature = db
            .state
            .queue
            .iter()
            .find(|feature| feature.status != "succeeded" && feature.status != "removed")
            .cloned();
        let binding_supplied = expected_feature_id.is_some()
            || expected_model_target.is_some()
            || expected_status.is_some()
            || expected_checkpoint.is_some();
        let binding_required = feature.is_some();
        if binding_required || binding_supplied {
            let expected_feature_id = expected_feature_id
                .context("Start or Resume requires the expected feature ID for a bound feature")?;
            let expected_model_target = expected_model_target.context(
                "Start or Resume requires the expected model target for a bound feature",
            )?;
            let expected_status = expected_status
                .context("Start or Resume requires the expected status for a bound feature")?;
            let expected_checkpoint = expected_checkpoint
                .context("Start or Resume requires the expected checkpoint for a bound feature")?;
            Uuid::parse_str(expected_feature_id).context("Invalid expected feature ID")?;
            let feature = feature.as_ref().context("No unfinished feature to start")?;
            if feature.id != expected_feature_id
                || feature.model_target != expected_model_target
                || feature.status != expected_status
                || feature.checkpoint != expected_checkpoint
            {
                bail!("The first queued feature state changed; refresh status before starting");
            }
        }
        if let Some(feature) = feature.as_ref() {
            validate_selection(
                &self.ai_catalog,
                &AiSelection {
                    model: feature.review_model.clone(),
                    reasoning_effort: feature.review_reasoning_effort.clone(),
                },
            )
            .context("Pinned feature reviewer is unavailable")?;
            if escalation_apply_is_quarantined(feature) {
                bail!("Prepare and explicitly approve a new repair proposal after the interrupted application");
            }
            if feature.auto_repair_lifecycle == "quarantined" {
                bail!("Automatic repair cannot Resume from a quarantine state");
            }
            if feature.status == "failed"
                && feature.repair_attempts > 0
                && feature.edits.is_none()
                && !feature.repair_pending
                && feature.auto_repair_lifecycle != "held"
            {
                bail!("Use Repair and retry for another bounded model correction");
            }
        }
        if self.running.load(Ordering::SeqCst) {
            return Ok(());
        }
        let resumes_at_escalation = feature.as_ref().is_some_and(|feature| {
            db.state.auto_ai_repair_enabled
                && feature.repair_attempts >= REPAIR_LIMIT
                && ((feature.auto_repair_lifecycle == "held" && feature.status == "failed")
                    || paused_preauthorization_cancellation_is_resumable(feature))
        });
        let revalidates_at_limit = feature
            .as_ref()
            .is_some_and(|feature| feature.auto_repair_lifecycle == "limit_reached");
        let limit_recovery_snapshot = if revalidates_at_limit {
            let expected_revision = db.state.revision;
            let expected_feature = feature.as_ref().context("feature missing")?.clone();
            let project = fs::canonicalize(self.root.join(&expected_feature.project))?;
            if !project.starts_with(&self.root) {
                bail!("Project escapes the workspace root");
            }
            drop(db);
            if self.cancellation.load(Ordering::SeqCst) != 0 {
                bail!("Auto AI repair limit recovery was cancelled before project inspection");
            }
            let snapshot =
                admitted_project_snapshot_with_cancellation(&project, Some(&self.cancellation))
                    .context("Auto AI repair limit recovery is unreviewable")?;
            db = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            let current = db
                .state
                .queue
                .iter()
                .find(|candidate| candidate.id == expected_feature.id)
                .context("feature missing")?;
            if db.state.revision != expected_revision
                || db.state.emergency_paused
                || self.cancellation.load(Ordering::SeqCst) != 0
                || current.status != expected_feature.status
                || current.checkpoint != expected_feature.checkpoint
                || current.auto_repair_lifecycle != "limit_reached"
                || current.auto_ai_repair_limit != expected_feature.auto_ai_repair_limit
                || current.escalation_count != expected_feature.escalation_count
                || current.auto_repair_limit_project_baseline
                    != expected_feature.auto_repair_limit_project_baseline
                || current.auto_repair_limit_unadmitted_sha256
                    != expected_feature.auto_repair_limit_unadmitted_sha256
                || current.auto_repair_limit_volatile_sha256
                    != expected_feature.auto_repair_limit_volatile_sha256
            {
                bail!("Automatic repair limit recovery changed during project inspection");
            }
            Some(snapshot)
        } else {
            None
        };
        let inference_lease = feature
            .as_ref()
            .filter(|_| !resumes_at_escalation && !revalidates_at_limit)
            .map(|_| {
                self.inference_gate
                    .try_acquire()
                    .context("Selected local model feature cannot start")
            })
            .transpose()?;
        if self.publication_connection_running.load(Ordering::SeqCst) {
            bail!("Wait for the GitHub connection operation to finish");
        }
        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Ok(());
        }
        let mut running_start = RunningStartGuard {
            running: &self.running,
            committed: false,
        };
        if let Some(feature) = feature
            .as_ref()
            .filter(|feature| !feature.publication_selection_frozen)
        {
            let mut next = db.state.clone();
            let binding = next
                .github_connections
                .iter()
                .find(|binding| binding.project == feature.project)
                .cloned();
            let current = next
                .queue
                .iter_mut()
                .find(|candidate| candidate.id == feature.id)
                .context("feature missing")?;
            if current.status != feature.status || current.checkpoint != feature.checkpoint {
                self.running.store(false, Ordering::SeqCst);
                bail!("Feature state changed before its publication destination was frozen");
            }
            if let Err(error) = freeze_publication_selection(current, binding) {
                self.running.store(false, Ordering::SeqCst);
                return Err(error);
            }
            next.revision = next.revision.checked_add(1).context("Revision overflow")?;
            let data = match serde_json::to_string(&next) {
                Ok(data) => data,
                Err(error) => {
                    self.running.store(false, Ordering::SeqCst);
                    return Err(error.into());
                }
            };
            if let Err(error) = db.connection.execute("INSERT INTO developer_state(id,state) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET state=excluded.state", [data]) {
                self.running.store(false, Ordering::SeqCst);
                return Err(error.into());
            }
            db.state = next;
        }
        if revalidates_at_limit {
            let feature_id = feature.as_ref().unwrap().id.clone();
            let recovery_snapshot = limit_recovery_snapshot
                .as_ref()
                .context("Automatic repair limit recovery snapshot is missing")?;
            let mut next = db.state.clone();
            let current = next
                .queue
                .iter_mut()
                .find(|candidate| candidate.id == feature_id)
                .context("feature missing")?;
            if current.auto_repair_lifecycle != "limit_reached"
                || !matches!(current.status.as_str(), "failed" | "paused")
            {
                self.running.store(false, Ordering::SeqCst);
                bail!("Automatic repair limit recovery binding changed");
            }
            let recovered_edits = match prepare_limit_recovery_edits(current, recovery_snapshot) {
                Ok(edits) => edits,
                Err(error) => {
                    self.running.store(false, Ordering::SeqCst);
                    return Err(error);
                }
            };
            current.edits = Some(recovered_edits);
            current.status = "paused".into();
            current.checkpoint = "auto_repair_limit_revalidating".into();
            current.message = "AI escalation limit remains exhausted. Resume is running only the immutable validation command and required independent review; no repair model attempt is authorized.".into();
            current.review_pending = None;
            current.review_status = "pending".into();
            current.review_summary =
                "Validation-only limit recovery has not started independent review".into();
            next.revision = next.revision.checked_add(1).context("Revision overflow")?;
            let data = serde_json::to_string(&next)?;
            db.connection.execute(
                "INSERT INTO developer_state(id,state) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET state=excluded.state",
                [data],
            )?;
            db.state = next;
        }
        if let Some(feature_id) = feature.as_ref().map(|feature| feature.id.as_str()) {
            let max_escalations = db.state.auto_ai_repair_max_escalations;
            let policy_revision = db.state.auto_ai_repair_policy_revision;
            let policy_enabled = db.state.auto_ai_repair_enabled;
            let needs_execution_binding = db
                .state
                .queue
                .iter()
                .find(|candidate| candidate.id == feature_id)
                .is_some_and(|candidate| {
                    candidate.auto_ai_repair_limit.is_none()
                        || (policy_enabled
                            && candidate.auto_repair_lifecycle == "inactive"
                            && candidate.status == "queued"
                            && candidate.checkpoint == "not_started")
                        || (policy_enabled && candidate.auto_repair_lifecycle == "held")
                        || (policy_enabled
                            && paused_preauthorization_cancellation_is_resumable(candidate))
                });
            if needs_execution_binding {
                let mut next = db.state.clone();
                let current = next
                    .queue
                    .iter_mut()
                    .find(|candidate| candidate.id == feature_id)
                    .context("feature missing")?;
                let changed = if policy_enabled
                    && paused_preauthorization_cancellation_is_resumable(current)
                {
                    enable_auto_repair_for_failed_feature(
                        current,
                        true,
                        max_escalations,
                        policy_revision,
                    )?
                } else if current.auto_repair_lifecycle == "held" && policy_enabled {
                    snapshot_feature_auto_repair_limit(current, max_escalations, policy_revision)?;
                    current.auto_repair_epoch = current
                        .auto_repair_epoch
                        .checked_add(1)
                        .context("Automatic repair epoch overflow")?;
                    set_auto_repair_lifecycle(
                        current,
                        "running",
                        "The owner explicitly resumed Auto AI repair after an operational hold",
                    )?;
                    true
                } else {
                    arm_auto_repair_at_execution_start(
                        current,
                        policy_enabled,
                        max_escalations,
                        policy_revision,
                    )?
                };
                if !changed {
                    self.running.store(false, Ordering::SeqCst);
                    bail!("Feature execution binding did not change as expected");
                }
                next.revision = next.revision.checked_add(1).context("Revision overflow")?;
                let data = serde_json::to_string(&next)?;
                db.connection.execute(
                    "INSERT INTO developer_state(id,state) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET state=excluded.state",
                    [data],
                )?;
                db.state = next;
            }
        }
        self.cancellation.store(0, Ordering::SeqCst);
        self.tool_cancellation.store(false, Ordering::SeqCst);
        let automatic_authorized = feature.as_ref().is_some_and(|observed| {
            db.state.auto_ai_repair_enabled
                && db
                    .state
                    .queue
                    .iter()
                    .find(|current| current.id == observed.id)
                    .is_some_and(|current| current.auto_repair_lifecycle == "running")
        });
        self.repair_loop_authorized
            .store(automatic_authorized, Ordering::SeqCst);
        drop(db);
        running_start.commit();
        self.launch(inference_lease);
        Ok(())
    }

    fn freeze_feature_publication(&self, observed: Feature) -> Result<Feature> {
        if observed.publication_selection_frozen {
            return Ok(observed);
        }
        self.change(|state| {
            let binding = state
                .github_connections
                .iter()
                .find(|binding| binding.project == observed.project)
                .cloned();
            let current = state
                .queue
                .iter_mut()
                .find(|feature| feature.id == observed.id)
                .context("Feature not found")?;
            if current.status != observed.status || current.checkpoint != observed.checkpoint {
                bail!("Feature state changed before its publication destination was frozen");
            }
            if current.publication_selection_frozen {
                bail!("Feature publication selection changed during admission");
            }
            freeze_publication_selection(current, binding)?;
            Ok(current.clone())
        })
    }
    fn launch(self: &Arc<Self>, inference_lease: Option<InferenceLease>) {
        let engine = self.clone();
        tokio::spawn(async move {
            if let Err(error) = engine.run_queue(inference_lease).await {
                eprintln!("developer run: {error:#}");
            }
            engine.repair_loop_authorized.store(false, Ordering::SeqCst);
            engine.running.store(false, Ordering::SeqCst);
        });
    }
    fn repair(self: &Arc<Self>, id: &str, expected_attempts: u32) -> Result<()> {
        Uuid::parse_str(id).context("Invalid feature ID")?;
        let mut db = self
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?;
        self.ensure_publication_barrier_clear(&db.state, &db.github_setup)?;
        if self.shutdown.load(Ordering::SeqCst) {
            bail!("Developer runner is shutting down");
        }
        if self.running.load(Ordering::SeqCst) {
            bail!("Stop the active run before repairing a feature");
        }
        if self.tools.blocks_work() {
            bail!("Resolve the active or uncertain project tool action before repairing a feature");
        }
        if self.planning_running.load(Ordering::SeqCst) {
            bail!("Wait for brainstorming to finish before repairing a feature");
        }
        if self.escalation_running.load(Ordering::SeqCst) {
            bail!("Wait for the repair proposal to finish before repairing a feature");
        }
        if self.emergency_paused(&db.state) {
            bail!("Clear Emergency Pause before repairing a feature");
        }
        let mut next = db.state.clone();
        let first = next
            .queue
            .iter()
            .position(|feature| feature.status != "succeeded" && feature.status != "removed")
            .context("No unfinished feature to repair")?;
        if next.queue[first].id != id {
            bail!("Only the first unfinished feature can be repaired");
        }
        let max_escalations = next.auto_ai_repair_max_escalations;
        let policy_revision = next.auto_ai_repair_policy_revision;
        let feature = &mut next.queue[first];
        snapshot_feature_auto_repair_limit(feature, max_escalations, policy_revision)?;
        if feature.status != "failed" {
            bail!("Only a failed feature can start a new repair attempt");
        }
        if escalation_apply_is_quarantined(feature) {
            bail!("Prepare and explicitly approve a new repair proposal after the interrupted application");
        }
        if feature.repair_attempts != expected_attempts {
            bail!("Repair attempt count changed; refresh status before retrying");
        }
        if feature.repair_attempts >= REPAIR_LIMIT {
            bail!("Repair attempt limit reached");
        }
        if matches!(
            feature.review_status.as_str(),
            "unavailable" | "interrupted" | "reviewing"
        ) {
            bail!("Resume retries required review without using a local repair attempt");
        }
        self.model_target(&feature.model_target)?;
        let inference_lease = Some(
            self.inference_gate
                .try_acquire()
                .context("Selected local model repair cannot start")?,
        );
        reserve_repair_attempt(feature)?;
        next.revision += 1;
        let data = serde_json::to_string(&next)?;
        db.connection.execute(
            "INSERT INTO developer_state(id,state) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET state=excluded.state",
            [data],
        )?;
        db.state = next;
        self.cancellation.store(0, Ordering::SeqCst);
        self.tool_cancellation.store(false, Ordering::SeqCst);
        self.repair_loop_authorized.store(true, Ordering::SeqCst);
        self.running.store(true, Ordering::SeqCst);
        drop(db);
        self.launch(inference_lease);
        Ok(())
    }

    fn escalation_snapshot(&self, feature_id: &str) -> Result<Value> {
        Uuid::parse_str(feature_id).context("Invalid feature ID")?;
        let database = self
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?;
        let feature = database
            .state
            .queue
            .iter()
            .find(|feature| feature.id == feature_id)
            .context("Feature not found")?;
        let Some(proposal) = &feature.escalation_proposal else {
            return Ok(json!({
                "status":"none",
                "feature_id":feature.id,
                "project":feature.project,
                "count":feature.escalation_count,
            }));
        };
        Ok(json!({
            "status":proposal.status,
            "feature_id":feature.id,
            "project":feature.project,
            "count":feature.escalation_count,
            "proposal_id":proposal.proposal_id,
            "model_target":proposal.model_target,
            "model":proposal.model,
            "chat_request_id":proposal.chat_request_id,
            "chat_model_target":proposal.chat_model_target,
            "chat_model":proposal.chat_model,
            "diagnosis":proposal.diagnosis,
            "diagnosis_sha256":proposal.diagnosis_sha256,
            "summary":proposal.summary,
            "error":proposal.error,
            "files":proposal.files,
            "binding":{
                "feature_id":proposal.feature_id,
                "checkpoint":proposal.feature_checkpoint,
                "revision":proposal.binding_revision,
            },
        }))
    }

    fn prepare_escalation(self: &Arc<Self>, request: RepairEscalationMutation) -> Result<Value> {
        enum ManualEscalationPreparation {
            Reserved,
            Rejected(&'static str),
        }

        let model_target = request
            .model_target
            .as_deref()
            .context("Missing repair model target")?;
        let chat_request_id = request
            .chat_request_id
            .as_deref()
            .context("Missing chat request ID")?;
        let diagnosis_sha256 = request
            .diagnosis_sha256
            .as_deref()
            .context("Missing diagnosis SHA-256")?;
        validate_sha256(diagnosis_sha256, "diagnosis SHA-256")?;
        Uuid::parse_str(&request.feature_id).context("Invalid feature ID")?;
        Uuid::parse_str(chat_request_id).context("Invalid chat request ID")?;
        let expected_checkpoint = request
            .expected_checkpoint
            .as_deref()
            .context("Missing expected checkpoint")?;
        let target = self.model_target(model_target)?.clone();

        let project = {
            let database = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            self.ensure_publication_barrier_clear(&database.state, &database.github_setup)?;
            database
                .state
                .queue
                .iter()
                .find(|feature| feature.id == request.feature_id)
                .context("Feature not found")?
                .project
                .clone()
        };
        let chat_id = self
            .chat
            .resolve_chat_id(&project, request.chat_id.as_deref())?;
        let handoff = self
            .chat
            .repair_handoff(&project, &chat_id, chat_request_id)?;
        validate_repair_handoff(
            &handoff,
            &project,
            &chat_id,
            chat_request_id,
            diagnosis_sha256,
        )?;

        let inference_lease = match self.inference_gate.try_acquire() {
            Ok(lease) => Some(lease),
            Err(error) => {
                return Err(error.context("Selected local model repair proposal cannot start"))
            }
        };
        let mut database = self
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?;
        self.ensure_publication_barrier_clear(&database.state, &database.github_setup)?;
        if self.shutdown.load(Ordering::SeqCst) {
            bail!("Developer runner is shutting down");
        }
        if self.running.load(Ordering::SeqCst)
            || self.planning_running.load(Ordering::SeqCst)
            || self.chat.is_running()
            || self.tools.blocks_work()
        {
            bail!("Stop active developer work before preparing a repair proposal");
        }
        if self.emergency_paused(&database.state) {
            bail!("Clear Emergency Pause before preparing a repair proposal");
        }
        let cancellation = self.reserve_escalation_call()?;
        let proposal_id = Uuid::new_v4().to_string();
        let prepared = mutate_database(&mut database, |state, _github_setup| {
            if state.revision != request.expected_revision {
                bail!("Runner revision changed; refresh before preparing a repair proposal");
            }
            if self.emergency_paused(state) {
                bail!("Clear Emergency Pause before preparing a repair proposal");
            }
            let first = state
                .queue
                .iter()
                .position(|feature| feature.status != "succeeded" && feature.status != "removed")
                .context("No unfinished feature to repair")?;
            if state.queue[first].id != request.feature_id {
                bail!("Only the first unfinished feature can be repaired");
            }
            let max_escalations = state.auto_ai_repair_max_escalations;
            let policy_revision = state.auto_ai_repair_policy_revision;
            let feature = &mut state.queue[first];
            if feature.status != "failed" || feature.checkpoint != expected_checkpoint {
                bail!("Failed feature binding changed; refresh before preparing a repair proposal");
            }
            if feature.escalation_pending {
                bail!("An approved repair escalation is already being applied");
            }
            if feature
                .escalation_proposal
                .as_ref()
                .is_some_and(|proposal| matches!(proposal.status.as_str(), "preparing" | "ready"))
            {
                bail!("Cancel or apply the current repair proposal first");
            }
            snapshot_feature_auto_repair_limit(feature, max_escalations, policy_revision)?;

            let limit = feature
                .auto_ai_repair_limit
                .context("Feature has no automatic repair limit snapshot")?;
            if feature.escalation_count >= limit || feature.escalation_count >= ESCALATION_LIMIT {
                set_auto_repair_lifecycle(
                    feature,
                    "limit_reached",
                    &format!(
                        "{} of {} AI escalations used. Automatic repair stopped. Correct the project without AI and revalidate, change the reviewer when applicable, or remove the feature.",
                        feature.escalation_count, limit
                    ),
                )?;
                return Ok(ManualEscalationPreparation::Rejected(
                    "Repair escalation limit reached",
                ));
            }
            if feature.escalation_history.len().saturating_add(3) > ESCALATION_HISTORY_LIMIT
                || feature
                    .escalation_evidence_reserved
                    .checked_add(3)
                    .context("Repair escalation evidence reservation overflow")?
                    > ESCALATION_HISTORY_LIMIT as u32
            {
                set_auto_repair_lifecycle(
                    feature,
                    "held",
                    "Automatic repair stopped because its bounded escalation evidence capacity is unavailable. Inspect the feature evidence before resuming.",
                )?;
                return Ok(ManualEscalationPreparation::Rejected(
                    "Repair escalation evidence capacity reached",
                ));
            }
            if feature.review_history.len().saturating_add(1) > REVIEW_HISTORY_LIMIT
                || feature
                    .review_evidence_reserved
                    .checked_add(1)
                    .context("Review evidence reservation overflow")?
                    > REVIEW_HISTORY_LIMIT as u32
            {
                set_auto_repair_lifecycle(
                    feature,
                    "held",
                    "Automatic repair stopped because its bounded independent-review evidence capacity is unavailable. Inspect the feature evidence before resuming.",
                )?;
                return Ok(ManualEscalationPreparation::Rejected(
                    "Independent review evidence capacity reached",
                ));
            }

            reserve_escalation_evidence(feature)?;
            let attempt = feature.escalation_count;
            feature.escalation_proposal = Some(RepairEscalationProposal {
                proposal_id: proposal_id.clone(),
                attempt,
                feature_id: feature.id.clone(),
                feature_checkpoint: feature.checkpoint.clone(),
                binding_revision: state.revision + 1,
                model_target: model_target.into(),
                model: target.model.clone(),
                chat_id: Some(handoff.chat_id.clone()),
                chat_request_id: handoff.request_id.clone(),
                chat_model_target: handoff.model_target.clone(),
                chat_model: handoff.model.clone(),
                diagnosis: handoff.response.clone(),
                diagnosis_sha256: handoff.response_sha256.clone(),
                status: "preparing".into(),
                summary: "Selected local AI is preparing a reviewable repair proposal".into(),
                error: None,
                files: Vec::new(),
                protected_inputs: std::collections::BTreeMap::new(),
                applied_paths: Vec::new(),
                apply_request_id: None,
                source: manual_escalation_source(),
                automatic_epoch: None,
                policy_revision: None,
                limit_snapshot: feature.auto_ai_repair_limit,
                project_state_sha256: None,
                review_slot_terminal: false,
            });
            Ok(ManualEscalationPreparation::Reserved)
        });
        drop(database);
        match prepared {
            Err(error) => {
                self.release_escalation_call(&cancellation);
                return Err(error);
            }
            Ok(ManualEscalationPreparation::Rejected(message)) => {
                self.release_escalation_call(&cancellation);
                return Err(anyhow!(message));
            }
            Ok(ManualEscalationPreparation::Reserved) => {}
        }

        let engine = self.clone();
        let feature_id = request.feature_id.clone();
        tokio::spawn(async move {
            let result = engine
                .build_escalation_proposal(EscalationProposalInput {
                    feature_id: &feature_id,
                    proposal_id: &proposal_id,
                    target: &target,
                    handoff: Some(&handoff),
                    automatic_failure_summary: None,
                    cancellation: &cancellation,
                    inference_lease,
                })
                .await;
            if let Err(error) =
                engine.finish_escalation_proposal(&feature_id, &proposal_id, result, &cancellation)
            {
                eprintln!("developer repair proposal completion: {error:#}");
            }
            engine.release_escalation_call(&cancellation);
        });
        self.escalation_snapshot(&request.feature_id)
    }

    async fn build_escalation_proposal(
        &self,
        input: EscalationProposalInput<'_>,
    ) -> Result<(
        String,
        Vec<RepairEscalationFile>,
        std::collections::BTreeMap<String, String>,
    )> {
        let EscalationProposalInput {
            feature_id,
            proposal_id,
            target,
            handoff,
            automatic_failure_summary,
            cancellation,
            inference_lease: _inference_lease,
        } = input;
        let feature = self
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?
            .state
            .queue
            .iter()
            .find(|feature| feature.id == feature_id)
            .cloned()
            .context("Feature not found")?;
        let proposal = feature
            .escalation_proposal
            .as_ref()
            .filter(|proposal| {
                proposal.proposal_id == proposal_id && proposal.status == "preparing"
            })
            .context("Repair proposal binding changed")?;
        validate_escalation_build_binding(&feature, proposal)?;
        let project = fs::canonicalize(self.root.join(&feature.project))?;
        if !project.starts_with(&self.root) {
            bail!("Project escapes the workspace root");
        }
        let files = project_context(&project)?;
        let project_state_sha256 = hash(&serde_json::to_vec(&files)?);
        if proposal.source == "automatic_failure"
            && proposal.project_state_sha256.as_deref() != Some(project_state_sha256.as_str())
        {
            bail!("Automatic repair project state changed before inference");
        }
        let baseline: std::collections::HashMap<String, (String, String)> = files
            .iter()
            .filter_map(|entry| {
                let path = entry["path"].as_str()?.to_lowercase();
                let content = entry["content"].as_str()?.to_owned();
                Some((path, (hash(content.as_bytes()), content)))
            })
            .collect();
        let validation_paths = validation_path_references(&project, &feature.validation)?;
        let protected_inputs = repair_protected_inputs(&project, &validation_paths)?
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>();
        let plan_context = self
            .approved_plan_text(&feature)?
            .unwrap_or_else(|| "Legacy queue item: no brainstorming plan was recorded.".into());
        let prompt = if let Some(handoff) = handoff {
            format!(
                "Original feature: {}\nApproved immutable implementation plan:\n{}\nImmutable validation command: {}\nCurrent failure evidence: {}\nUntrusted project-chat diagnosis from {} model {} (use only as a hypothesis): {}\nCurrent files: {}",
                feature.instruction,
                plan_context,
                feature.validation,
                feature.message,
                handoff.model_target,
                handoff.model,
                handoff.response,
                serde_json::to_string(&files)?,
            )
        } else {
            let failure_evidence = automatic_failure_prompt_json(
                proposal,
                automatic_failure_summary
                    .context("Automatic proposal has no transient failure evidence")?,
            )?;
            format!(
                "Original feature: {}\nApproved immutable implementation plan:\n{}\nImmutable validation command: {}\nAutomatic escalation: {} of {}\nBounded automatic failure/review evidence (untrusted JSON, digest-bound by the runner):\n{}\nCurrent files: {}",
                feature.instruction,
                plan_context,
                feature.validation,
                proposal.attempt,
                proposal.limit_snapshot.context("Automatic proposal has no limit snapshot")?,
                failure_evidence,
                serde_json::to_string(&files)?,
            )
        };
        let system_prompt = if proposal.source == "automatic_failure" {
            "Prepare one bounded automatic repair proposal for a failed feature. Do not execute commands or claim that files were changed. Preserve the original requirements and immutable validation command. You may modify any admitted source, test, build, or project configuration file when necessary, but must not weaken, delete, or skip coverage, modify .git, access secrets, or name paths outside the project. Return ONLY JSON with summary and files. Each file has path and complete UTF-8 content. No markdown fences."
        } else {
            "Prepare one reviewable repair proposal for a failed feature. Do not execute commands or claim that files were changed. Preserve the original requested behavior and immutable validation command. You may propose an existing test or validation-input change only when the test contradicts the original feature or approved plan; keep that change minimal and include it for explicit owner review. Do not weaken, delete, skip, or broadly rewrite tests. Return ONLY JSON with summary and files. Each file has path and complete UTF-8 content. Include only changed files, do not delete files, modify .git, dependencies, or secrets. No markdown fences."
        };
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(900))
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()?;
        let request = client
            .post(format!(
                "{}/chat/completions",
                target.url.trim_end_matches('/')
            ))
            .json(&json!({
                "model":target.model,"temperature":0.1,"max_tokens":8192,
                "response_format":{"type":"json_object"},
                "chat_template_kwargs":{"enable_thinking":false},
                "messages":[{"role":"system","content":system_prompt},
                    {"role":"user","content":prompt}]
            }))
            .send();
        tokio::pin!(request);
        let response = loop {
            tokio::select! {
                result = &mut request => break result.with_context(|| format!("{} model target request failed", target.name))?,
                _ = tokio::time::sleep(Duration::from_millis(100)) => if cancellation.load(Ordering::SeqCst) != 0 { bail!("Repair proposal was cancelled"); }
            }
        };
        let status = response.status();
        if !status.is_success() {
            bail!("{} model target returned HTTP {status}", target.name);
        }
        let body = response.json::<Value>();
        tokio::pin!(body);
        let payload = loop {
            tokio::select! {
                result = &mut body => break result?,
                _ = tokio::time::sleep(Duration::from_millis(100)) => if cancellation.load(Ordering::SeqCst) != 0 { bail!("Repair proposal was cancelled"); }
            }
        };
        let content = payload["choices"][0]["message"]["content"]
            .as_str()
            .context("Model returned no repair proposal")?;
        if content.len() > 1024 * 1024 {
            bail!("Model repair proposal exceeds 1 MiB");
        }
        let normalized = content
            .trim()
            .strip_prefix("```json")
            .or_else(|| content.trim().strip_prefix("```"))
            .and_then(|value| value.trim().strip_suffix("```"))
            .unwrap_or(content)
            .trim();
        let generated: Value = serde_json::from_str(normalized).with_context(|| {
            format!(
                "Model did not return valid repair proposal JSON ({} bytes; finish reason {})",
                content.len(),
                payload["choices"][0]["finish_reason"]
            )
        })?;
        let summary = generated["summary"]
            .as_str()
            .context("Repair proposal has no summary")?;
        if summary.trim().is_empty() || summary.len() > 4000 {
            bail!("Repair proposal summary must be 1 to 4000 characters");
        }
        validate_cloud_text(summary)
            .context("Repair proposal summary contains secret-shaped text")?;
        let entries = generated["files"]
            .as_array()
            .context("Repair proposal has no files array")?;
        if entries.is_empty() || entries.len() > 40 {
            bail!("Expected 1 to 40 proposed files");
        }
        if repair_protected_inputs(&project, &validation_paths)?
            .into_iter()
            .collect::<std::collections::BTreeMap<_, _>>()
            != protected_inputs
        {
            bail!("A protected test or validation input changed while the proposal was prepared");
        }
        let mut proposed = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for entry in entries {
            let path = entry["path"]
                .as_str()
                .context("Missing proposed file path")?;
            let after = entry["content"]
                .as_str()
                .context("Missing proposed file content")?;
            let full = checked_path(&project, path)?;
            validate_repair_file_admission(path, after)?;
            let normalized_path = path.to_lowercase();
            if !seen.insert(normalized_path.clone()) {
                bail!("Duplicate proposed file");
            }
            let before = if let Some((expected, content)) = baseline.get(&normalized_path) {
                if !full.exists() || hash(&fs::read(&full)?) != *expected {
                    bail!("{path} changed while the repair proposal was prepared");
                }
                Some(content.clone())
            } else if full.exists() {
                bail!("{path} was outside the bounded repair context");
            } else {
                None
            };
            if before.as_deref() == Some(after) {
                continue;
            }
            proposed.push(RepairEscalationFile {
                path: path.into(),
                before,
                after: after.into(),
                protected: is_protected_repair_input(path, &validation_paths),
            });
        }
        if proposed.is_empty() && proposal.source != "automatic_failure" {
            bail!("Repair proposal contains no file changes");
        }
        if cancellation.load(Ordering::SeqCst) != 0 {
            bail!("Repair proposal was cancelled");
        }
        Ok((summary.into(), proposed, protected_inputs))
    }

    fn finish_escalation_proposal(
        &self,
        feature_id: &str,
        proposal_id: &str,
        result: Result<(
            String,
            Vec<RepairEscalationFile>,
            std::collections::BTreeMap<String, String>,
        )>,
        cancellation: &AtomicU8,
    ) -> Result<()> {
        self.change(|state| {
            let policy_enabled = state.auto_ai_repair_enabled;
            let feature = state
                .queue
                .iter_mut()
                .find(|feature| feature.id == feature_id)
                .context("Feature not found")?;
            let feature_epoch = feature.auto_repair_epoch;
            let feature_lifecycle = feature.auto_repair_lifecycle.clone();
            let proposal = feature
                .escalation_proposal
                .as_mut()
                .filter(|proposal| proposal.proposal_id == proposal_id)
                .context("Repair proposal not found")?;
            if proposal.status != "preparing" {
                return Ok(());
            }
            proposal.binding_revision = state.revision + 1;
            let mut candidate_sha256 = None;
            let stale_automatic = proposal.source == "automatic_failure"
                && (proposal.automatic_epoch != Some(feature_epoch)
                    || feature_lifecycle != "running"
                    || !policy_enabled);
            let outcome = if cancellation.load(Ordering::SeqCst) != 0 || stale_automatic {
                proposal.status = "cancelled".into();
                proposal.summary = if stale_automatic {
                    "Repair proposal completion was retired by a newer automatic-repair policy epoch"
                        .into()
                } else {
                    "Repair proposal preparation was cancelled".into()
                };
                proposal.error = None;
                "cancelled"
            } else {
                match result {
                    Ok((summary, files, protected_inputs)) => {
                        proposal.summary = summary;
                        proposal.files = files;
                        proposal.protected_inputs = protected_inputs;
                        proposal.error = None;
                        let digest = repair_escalation_candidate_sha256(proposal)?;
                        candidate_sha256 = Some(digest.clone());
                        if proposal.source == "automatic_failure" && proposal.files.is_empty() {
                            proposal.status = "no_op".into();
                            "no_op"
                        } else if proposal.source == "automatic_failure"
                            && feature.escalation_history.iter().any(|evidence| {
                                evidence.candidate_sha256.as_deref() == Some(digest.as_str())
                            })
                        {
                            proposal.status = "duplicate".into();
                            "duplicate"
                        } else {
                            proposal.status = "ready".into();
                            "ready"
                        }
                    }
                    Err(error) => {
                        proposal.status = "unavailable".into();
                        proposal.summary =
                            "The selected local AI could not prepare a repair proposal".into();
                        proposal.error = Some(durable_failure_summary(
                            &error.to_string(),
                            "Repair proposal generation failed",
                            1000,
                        ));
                        "unavailable"
                    }
                }
            };
            let proposal_sha256 = if outcome == "ready" {
                Some(repair_escalation_proposal_sha256(proposal)?)
            } else {
                None
            };
            feature.escalation_history.push(RepairEscalationEvidence {
                proposal_id: proposal.proposal_id.clone(),
                attempt: proposal.attempt,
                model_target: proposal.model_target.clone(),
                model: proposal.model.clone(),
                chat_id: proposal.chat_id.clone(),
                chat_request_id: proposal.chat_request_id.clone(),
                diagnosis_sha256: proposal.diagnosis_sha256.clone(),
                outcome: outcome.into(),
                proposal_sha256,
                candidate_sha256,
                summary: proposal.summary.clone(),
                source: proposal.source.clone(),
                automatic_epoch: proposal.automatic_epoch,
                policy_revision: proposal.policy_revision,
                limit_snapshot: proposal.limit_snapshot,
                project_state_sha256: proposal.project_state_sha256.clone(),
                authorization_revision: None,
            });
            let automatic_transition = (
                proposal.source == "automatic_failure",
                proposal.automatic_epoch,
                proposal.attempt,
            );
            if outcome != "ready" {
                terminalize_unapplied_escalation(
                    feature,
                    outcome,
                    "authorization_not_run",
                    outcome,
                    "Proposal preparation did not produce an authorized application; independent review was not run",
                )?;
                if automatic_transition.0
                    && automatic_transition.1 == Some(feature.auto_repair_epoch)
                {
                    feature.status = "failed".into();
                    feature.escalation_pending = false;
                    feature.checkpoint =
                        format!("escalation_{}_{}", automatic_transition.2, outcome);
                    match outcome {
                        "no_op" | "duplicate" => set_auto_repair_lifecycle(
                            feature,
                            "running",
                            "The no-op or duplicate candidate consumed its escalation and automatic repair will continue",
                        )?,
                        "cancelled" => set_auto_repair_lifecycle(
                            feature,
                            "inactive",
                            "Automatic proposal generation was cancelled after its attempt was reserved",
                        )?,
                        "unavailable" => set_auto_repair_lifecycle(
                            feature,
                            "held",
                            "The selected model could not produce a valid automatic repair proposal. Inspect the provider and retained evidence, then explicitly Resume.",
                        )?,
                        _ => bail!("Automatic proposal has an unsupported terminal outcome"),
                    }
                    feature.message = feature.auto_repair_reason.clone();
                }
            } else if feature.auto_repair_lifecycle == "running" {
                start_auto_repair_step(feature)?;
            }
            Ok(())
        })
    }

    fn terminalize_automatic_reservation_preparation_failure(
        &self,
        observed: &Feature,
        at_limit: bool,
    ) -> Result<()> {
        self.change(|state| {
            let current = state
                .queue
                .iter_mut()
                .find(|feature| feature.id == observed.id)
                .context("Feature not found")?;
            if current.auto_repair_lifecycle != "running"
                || current.status != "failed"
                || current.auto_repair_epoch != observed.auto_repair_epoch
                || current.auto_repair_policy_revision != observed.auto_repair_policy_revision
                || current.escalation_count != observed.escalation_count
                || current.auto_ai_repair_limit != observed.auto_ai_repair_limit
            {
                return Ok(());
            }
            current.status = "failed".into();
            current.repair_pending = false;
            current.last_failure_kind = "operational".into();
            if at_limit {
                set_auto_repair_lifecycle(
                    current,
                    "limit_reached",
                    "Automatic repair reached its escalation limit, but the bounded recovery baseline could not be captured. Reduce or correct the unreviewable project data, then retry recovery or remove the feature.",
                )?;
            } else {
                set_auto_repair_lifecycle(
                    current,
                    "held",
                    "Automatic repair could not safely prepare the next bounded escalation. Correct the project or model availability condition, then explicitly Resume.",
                )?;
            }
            Ok(())
        })
    }

    fn reserve_automatic_escalation(
        &self,
        feature_id: &str,
    ) -> Result<Option<(String, ModelTarget, u64, String)>> {
        let observed = self
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?
            .state
            .queue
            .iter()
            .find(|feature| feature.id == feature_id)
            .cloned()
            .context("Feature not found")?;
        let at_limit = observed
            .auto_ai_repair_limit
            .is_some_and(|limit| observed.escalation_count >= limit);
        let preparation = (|| -> Result<_> {
            if self.cancelled() {
                bail!("Automatic repair escalation preparation was cancelled");
            }
            let project = fs::canonicalize(self.root.join(&observed.project))?;
            if !project.starts_with(&self.root) {
                bail!("Project escapes the workspace root");
            }
            if at_limit {
                let snapshot = admitted_project_snapshot_with_cancellation(
                    &project,
                    Some(&self.cancellation),
                )?;
                if self.cancelled() {
                    bail!("Automatic repair escalation preparation was cancelled");
                }
                return Ok((None, None, Some(snapshot)));
            }
            let target = self.model_target(&observed.model_target)?.clone();
            let project_state_sha256 = hash(&serde_json::to_vec(&project_context(&project)?)?);
            Ok((Some(target), Some(project_state_sha256), None))
        })();
        let (target, project_state_sha256, limit_recovery_snapshot) = match preparation {
            Ok(prepared) => prepared,
            Err(error) => {
                self.terminalize_automatic_reservation_preparation_failure(&observed, at_limit)?;
                return Err(error);
            }
        };
        let proposal_id = Uuid::new_v4().to_string();
        let target_for_state = target.clone();
        let reserved = self.change(|state| {
            if !state.auto_ai_repair_enabled {
                return Ok(None);
            }
            let policy_revision = state.auto_ai_repair_policy_revision;
            let feature = state
                .queue
                .iter_mut()
                .find(|feature| feature.id == feature_id)
                .context("Feature not found")?;
            if feature.auto_repair_lifecycle != "running" || feature.status != "failed" {
                return Ok(None);
            }
            if feature.auto_repair_epoch != observed.auto_repair_epoch
                || feature.auto_repair_policy_revision != observed.auto_repair_policy_revision
                || feature.escalation_count != observed.escalation_count
                || feature.auto_ai_repair_limit != observed.auto_ai_repair_limit
                || feature.auto_repair_policy_revision != Some(policy_revision)
            {
                bail!("Automatic repair escalation binding changed during preparation");
            }
            if feature.repair_attempts < REPAIR_LIMIT {
                bail!("Ordinary repair attempts must be exhausted before automatic escalation");
            }
            let limit = feature
                .auto_ai_repair_limit
                .context("Automatic repair feature limit was not snapshotted")?;
            if feature.escalation_count >= limit {
                let limit_recovery_snapshot = limit_recovery_snapshot
                    .as_ref()
                    .context("Automatic repair limit recovery snapshot is missing")?;
                feature.auto_repair_limit_project_baseline = limit_recovery_snapshot
                    .files
                    .iter()
                    .map(|(path, (digest, _))| (path.clone(), digest.clone()))
                    .collect();
                feature.auto_repair_limit_unadmitted_sha256 =
                    Some(limit_recovery_snapshot.unadmitted_sha256.clone());
                feature.auto_repair_limit_volatile_sha256 =
                    Some(limit_recovery_snapshot.volatile_sha256.clone());
                let reason = format!(
                    "{} of {} AI escalations used. Automatic repair stopped. Correct the project without AI and revalidate, change the reviewer when applicable, or remove the feature.",
                    feature.escalation_count, limit
                );
                set_auto_repair_lifecycle(feature, "limit_reached", &reason)?;
                return Ok(None);
            }
            if feature.escalation_history.len().saturating_add(3) > ESCALATION_HISTORY_LIMIT
                || feature.review_history.len().saturating_add(1) > REVIEW_HISTORY_LIMIT
                || feature.escalation_evidence_reserved.saturating_add(3)
                    > ESCALATION_HISTORY_LIMIT as u32
                || feature.review_evidence_reserved.saturating_add(1)
                    > REVIEW_HISTORY_LIMIT as u32
            {
                set_auto_repair_lifecycle(
                    feature,
                    "held",
                    "Automatic repair stopped because bounded escalation or review evidence capacity is unavailable. Inspect the retained evidence before resuming.",
                )?;
                return Ok(None);
            }
            if feature.escalation_pending
                || feature.escalation_proposal.as_ref().is_some_and(|proposal| {
                    matches!(proposal.status.as_str(), "preparing" | "ready" | "approved" | "applying")
                })
            {
                bail!("Automatic escalation already has active proposal evidence");
            }
            let epoch = feature.auto_repair_epoch;
            let target_for_state = target_for_state
                .as_ref()
                .context("Automatic repair model target is missing")?;
            let project_state_sha256 = project_state_sha256
                .as_ref()
                .context("Automatic repair project-state binding is missing")?;
            let failure_summary = if feature.last_code_failure_summary.is_empty() {
                bounded_code_failure_summary(&feature.message)?
            } else {
                bounded_code_failure_summary(&feature.last_code_failure_summary)?
            };
            feature.last_code_failure_summary = failure_summary.clone();
            let failure_sha256 = hash(failure_summary.as_bytes());
            reserve_escalation_evidence(feature)?;
            let attempt = feature.escalation_count;
            feature.escalation_proposal = Some(RepairEscalationProposal {
                proposal_id: proposal_id.clone(),
                attempt,
                feature_id: feature.id.clone(),
                feature_checkpoint: feature.checkpoint.clone(),
                binding_revision: state.revision + 1,
                model_target: target_for_state.id.into(),
                model: target_for_state.model.clone(),
                chat_id: None,
                chat_request_id: String::new(),
                chat_model_target: String::new(),
                chat_model: String::new(),
                diagnosis: String::new(),
                diagnosis_sha256: failure_sha256,
                status: "preparing".into(),
                summary: format!(
                    "Automatic AI escalation {attempt} of {limit} is preparing a bounded proposal"
                ),
                error: None,
                files: Vec::new(),
                protected_inputs: std::collections::BTreeMap::new(),
                applied_paths: Vec::new(),
                apply_request_id: None,
                source: "automatic_failure".into(),
                automatic_epoch: Some(epoch),
                policy_revision: Some(policy_revision),
                limit_snapshot: Some(limit),
                project_state_sha256: Some(project_state_sha256.clone()),
                review_slot_terminal: false,
            });
            feature.checkpoint = format!("escalation_{attempt}_preparing");
            feature.message = format!("AI escalation {attempt} of {limit} is preparing repair");
            start_auto_repair_step(feature)?;
            Ok(Some((epoch, failure_summary)))
        })?;
        match reserved {
            Some((epoch, failure_summary)) => Ok(Some((
                proposal_id,
                target.context("Automatic repair model target is missing")?,
                epoch,
                failure_summary,
            ))),
            None => Ok(None),
        }
    }

    fn finish_automatic_without_application(
        &self,
        feature_id: &str,
        proposal_id: &str,
        epoch: u64,
    ) -> Result<bool> {
        self.change(|state| {
            let feature = state
                .queue
                .iter_mut()
                .find(|feature| feature.id == feature_id)
                .context("Feature not found")?;
            let proposal = feature
                .escalation_proposal
                .as_ref()
                .filter(|proposal| proposal.proposal_id == proposal_id)
                .context("Automatic proposal not found")?
                .clone();
            if proposal.source != "automatic_failure"
                || proposal.automatic_epoch != Some(epoch)
                || feature.auto_repair_epoch != epoch
            {
                bail!("Late automatic proposal completion was rejected by its epoch binding");
            }
            let retryable = matches!(proposal.status.as_str(), "no_op" | "duplicate");
            if !matches!(proposal.status.as_str(), "no_op" | "duplicate" | "unavailable" | "cancelled") {
                bail!("Automatic proposal is not terminal without application");
            }
            terminalize_unapplied_escalation(
                feature,
                &proposal.status,
                "authorization_not_run",
                &proposal.status,
                "No proposal was authorized or applied; independent review was not run",
            )?;
            feature.status = "failed".into();
            feature.escalation_pending = false;
            if retryable {
                set_auto_repair_lifecycle(
                    feature,
                    "running",
                    "The no-op or duplicate candidate consumed its escalation and automatic repair will continue",
                )?;
            } else if proposal.status == "cancelled" {
                set_auto_repair_lifecycle(
                    feature,
                    "inactive",
                    "Automatic proposal generation was cancelled after its attempt was reserved",
                )?;
            } else {
                set_auto_repair_lifecycle(
                    feature,
                    "held",
                    "The selected model could not produce a valid automatic repair proposal. Inspect the provider and retained evidence, then explicitly Resume.",
                )?;
            }
            feature.checkpoint = format!("escalation_{}_{}", proposal.attempt, proposal.status);
            feature.message = feature.auto_repair_reason.clone();
            Ok(retryable)
        })
    }

    fn authorize_automatic_proposal(
        &self,
        feature_id: &str,
        proposal_id: &str,
        epoch: u64,
    ) -> Result<()> {
        let authorization = self.change(|state| {
            if !state.auto_ai_repair_enabled {
                bail!("Auto AI repair was disabled before policy authorization");
            }
            let policy_revision = state.auto_ai_repair_policy_revision;
            let feature = state
                .queue
                .iter_mut()
                .find(|feature| feature.id == feature_id)
                .context("Feature not found")?;
            let proposal = feature
                .escalation_proposal
                .as_ref()
                .filter(|proposal| proposal.proposal_id == proposal_id)
                .context("Automatic proposal not found")?
                .clone();
            if proposal.status != "ready"
                || proposal.source != "automatic_failure"
                || proposal.automatic_epoch != Some(epoch)
                || feature.auto_repair_epoch != epoch
                || feature.auto_repair_lifecycle != "running"
                || proposal.policy_revision != feature.auto_repair_policy_revision
                || proposal.policy_revision != Some(policy_revision)
                || proposal.limit_snapshot != feature.auto_ai_repair_limit
            {
                bail!("Automatic policy authorization binding changed");
            }
            if feature.escalation_count > proposal.limit_snapshot.unwrap_or(0) {
                bail!("Automatic escalation exceeds its feature limit snapshot");
            }
            let proposal_sha256 = repair_escalation_proposal_sha256(&proposal)?;
            let ready_sha256 = feature
                .escalation_history
                .iter()
                .rev()
                .find(|evidence| evidence.proposal_id == proposal_id && evidence.outcome == "ready")
                .and_then(|evidence| evidence.proposal_sha256.as_deref())
                .context("Ready automatic proposal evidence is missing")?;
            if ready_sha256 != proposal_sha256 {
                bail!("Automatic proposal digest changed before policy authorization");
            }
            let project = fs::canonicalize(self.root.join(&feature.project))?;
            if !project.starts_with(&self.root) {
                bail!("Project escapes the workspace root");
            }
            let project_state_sha256 = hash(&serde_json::to_vec(&project_context(&project)?)?);
            if proposal.project_state_sha256.as_deref() != Some(project_state_sha256.as_str()) {
                bail!("Project bytes changed before automatic policy authorization");
            }
            for file in &proposal.files {
                validate_repair_file_admission(&file.path, &file.after)?;
                let path = checked_path(&project, &file.path)?;
                let current = if path.exists() {
                    Some(fs::read_to_string(&path).with_context(|| {
                        format!("Proposed file {} is not UTF-8", file.path)
                    })?)
                } else {
                    None
                };
                if current != file.before {
                    bail!("{} changed after automatic proposal preparation", file.path);
                }
            }
            if proposal.files.is_empty() {
                bail!("Automatic policy cannot authorize an empty proposal");
            }
            merge_escalation_review_edits(
                feature.edits.as_deref().unwrap_or_default(),
                &proposal.files,
                &project,
            )?;
            let current = feature.escalation_proposal.as_mut().unwrap();
            current.status = "approved".into();
            current.apply_request_id = Some(Uuid::new_v4().to_string());
            current.binding_revision = state.revision + 1;
            feature.escalation_history.push(RepairEscalationEvidence {
                proposal_id: proposal.proposal_id.clone(),
                attempt: proposal.attempt,
                model_target: proposal.model_target.clone(),
                model: proposal.model.clone(),
                chat_id: None,
                chat_request_id: String::new(),
                diagnosis_sha256: proposal.diagnosis_sha256.clone(),
                outcome: "policy_authorized".into(),
                proposal_sha256: Some(proposal_sha256),
                candidate_sha256: Some(repair_escalation_candidate_sha256(&proposal)?),
                summary: "The enabled Auto AI repair policy authorized these exact proposal bytes for one application attempt".into(),
                source: proposal.source.clone(),
                automatic_epoch: proposal.automatic_epoch,
                policy_revision: proposal.policy_revision,
                limit_snapshot: proposal.limit_snapshot,
                project_state_sha256: proposal.project_state_sha256.clone(),
                authorization_revision: Some(state.revision + 1),
            });
            feature.status = "paused".into();
            feature.checkpoint = format!("escalation_{}_approved", proposal.attempt);
            feature.message = format!(
                "AI escalation {} of {} is authorized by the enabled repair policy",
                proposal.attempt,
                proposal.limit_snapshot.unwrap_or(ESCALATION_LIMIT)
            );
            feature.repair_pending = false;
            feature.escalation_pending = true;
            feature.review_status = "pending".into();
            feature.review_summary = "Required ChatGPT Codex review has not started".into();
            feature.review_pending = None;
            start_auto_repair_step(feature)?;
            Ok(())
        });
        if let Err(error) = authorization {
            let rejection_is_current = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?
                .state
                .queue
                .iter()
                .find(|feature| feature.id == feature_id)
                .and_then(|feature| {
                    feature
                        .escalation_proposal
                        .as_ref()
                        .map(|proposal| (feature, proposal))
                })
                .is_some_and(|(feature, proposal)| {
                    proposal.proposal_id == proposal_id
                        && proposal.status == "ready"
                        && proposal.source == "automatic_failure"
                        && proposal.automatic_epoch == Some(epoch)
                        && feature.auto_repair_epoch == epoch
                        && feature.auto_repair_lifecycle == "running"
                });
            if rejection_is_current {
                self.change(|state| {
                    let feature = state
                        .queue
                        .iter_mut()
                        .find(|feature| feature.id == feature_id)
                        .context("Feature not found")?;
                    let attempt = feature
                        .escalation_proposal
                        .as_ref()
                        .filter(|proposal| proposal.proposal_id == proposal_id)
                        .context("Automatic proposal not found")?
                        .attempt;
                    terminalize_unapplied_escalation(
                        feature,
                        "unavailable",
                        "authorization_rejected",
                        "authorization_rejected",
                        "Automatic proposal authorization rejected workspace, path, or evidence drift; no files were applied and independent review was not run",
                    )?;
                    set_auto_repair_lifecycle(
                        feature,
                        "held",
                        "Automatic repair stopped because proposal authorization rejected workspace, path, or evidence drift. Correct the condition and explicitly Resume to reserve a new attempt.",
                    )?;
                    feature.status = "failed".into();
                    feature.checkpoint = format!("escalation_{attempt}_authorization_rejected");
                    feature.message = feature.auto_repair_reason.clone();
                    Ok(())
                })?;
            }
            return Err(error);
        }
        Ok(())
    }

    async fn prepare_next_automatic_escalation(&self, feature_id: &str) -> Result<bool> {
        loop {
            let already_at_limit = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?
                .state
                .queue
                .iter()
                .find(|feature| feature.id == feature_id)
                .and_then(|feature| {
                    feature
                        .auto_ai_repair_limit
                        .map(|limit| feature.escalation_count >= limit)
                })
                .unwrap_or(false);
            if already_at_limit {
                let reserved = self.reserve_automatic_escalation(feature_id)?;
                if reserved.is_some() {
                    bail!("Automatic repair limit preflight unexpectedly reserved model work");
                }
                return Ok(false);
            }
            let cancellation = self.reserve_escalation_call()?;
            let escalation_call = EscalationCallGuard {
                engine: self,
                cancellation: cancellation.clone(),
                released: false,
            };
            let inference_lease = match self.inference_gate.try_acquire() {
                Ok(lease) => Some(lease),
                Err(error) => {
                    self.change(|state| {
                        let feature = state
                            .queue
                            .iter_mut()
                            .find(|feature| feature.id == feature_id)
                            .context("Feature not found")?;
                        set_auto_repair_lifecycle(
                            feature,
                            "held",
                            "Automatic repair could not acquire the shared model gate. Explicitly Resume after other model work finishes.",
                        )
                    })?;
                    return Err(error.context("Automatic repair model gate is unavailable"));
                }
            };
            let reserved = self.reserve_automatic_escalation(feature_id);
            let Some((proposal_id, target, epoch, failure_summary)) = reserved? else {
                escalation_call.release();
                return Ok(false);
            };
            let result = self
                .build_escalation_proposal(EscalationProposalInput {
                    feature_id,
                    proposal_id: &proposal_id,
                    target: &target,
                    handoff: None,
                    automatic_failure_summary: Some(&failure_summary),
                    cancellation: &cancellation,
                    inference_lease,
                })
                .await;
            self.finish_escalation_proposal(feature_id, &proposal_id, result, &cancellation)?;
            escalation_call.release();
            let status = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?
                .state
                .queue
                .iter()
                .find(|feature| feature.id == feature_id)
                .and_then(|feature| feature.escalation_proposal.as_ref())
                .filter(|proposal| proposal.proposal_id == proposal_id)
                .map(|proposal| proposal.status.clone())
                .context("Automatic proposal completion disappeared")?;
            if status == "ready" {
                if let Err(error) =
                    self.authorize_automatic_proposal(feature_id, &proposal_id, epoch)
                {
                    self.change(|state| {
                        let feature = state
                            .queue
                            .iter_mut()
                            .find(|feature| feature.id == feature_id)
                            .context("Feature not found")?;
                        if feature.auto_repair_epoch == epoch {
                            set_auto_repair_lifecycle(
                                feature,
                                "held",
                                "Automatic repair stopped because proposal authorization detected workspace, path, evidence, or capacity drift. Inspect retained evidence before resuming.",
                            )?;
                            feature.status = "failed".into();
                            feature.message = feature.auto_repair_reason.clone();
                        }
                        Ok(())
                    })?;
                    return Err(error);
                }
                return Ok(true);
            }
            if !self.finish_automatic_without_application(feature_id, &proposal_id, epoch)? {
                return Ok(false);
            }
        }
    }

    fn resume_automatic_after_restart(self: &Arc<Self>) -> Result<()> {
        let observed = {
            let database = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            if !database.state.auto_ai_repair_enabled {
                return Ok(());
            }
            database
                .state
                .queue
                .iter()
                .find(|feature| feature.status != "succeeded" && feature.status != "removed")
                .filter(|feature| feature.auto_repair_lifecycle == "running")
                .cloned()
        };
        let Some(mut feature) = observed else {
            return Ok(());
        };
        if let Some(proposal) = feature
            .escalation_proposal
            .as_ref()
            .filter(|proposal| proposal.source == "automatic_failure" && proposal.status == "ready")
        {
            let proposal_id = proposal.proposal_id.clone();
            let epoch = proposal
                .automatic_epoch
                .context("Restartable automatic proposal has no epoch")?;
            if let Err(error) = self.authorize_automatic_proposal(&feature.id, &proposal_id, epoch)
            {
                self.change(|state| {
                    let current = state
                        .queue
                        .iter_mut()
                        .find(|current| current.id == feature.id)
                        .context("Feature not found")?;
                    set_auto_repair_lifecycle(
                        current,
                        "quarantined",
                        "A ready automatic proposal did not match its exact restart bindings. Inspect the workspace; it was not applied.",
                    )?;
                    current.status = "failed".into();
                    current.message = current.auto_repair_reason.clone();
                    Ok(())
                })?;
                return Err(error.context("Automatic repair restart reconciliation failed"));
            }
            feature = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?
                .state
                .queue
                .iter()
                .find(|current| current.id == feature.id)
                .cloned()
                .context("Feature not found")?;
        }
        if !matches!(feature.status.as_str(), "failed" | "queued" | "paused")
            || (feature.status == "paused" && !feature.escalation_pending)
        {
            return Ok(());
        }
        self.running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| anyhow!("Another developer run started during restart recovery"))?;
        self.cancellation.store(0, Ordering::SeqCst);
        self.tool_cancellation.store(false, Ordering::SeqCst);
        self.repair_loop_authorized.store(true, Ordering::SeqCst);
        self.launch(None);
        Ok(())
    }

    fn approve_escalation(self: &Arc<Self>, request: RepairEscalationMutation) -> Result<Value> {
        Uuid::parse_str(&request.feature_id).context("Invalid feature ID")?;
        let proposal_id = request
            .proposal_id
            .as_deref()
            .context("Missing repair proposal ID")?;
        Uuid::parse_str(proposal_id).context("Invalid repair proposal ID")?;
        let expected_checkpoint = request
            .expected_checkpoint
            .as_deref()
            .context("Missing expected checkpoint")?;
        if self.shutdown.load(Ordering::SeqCst) {
            bail!("Developer runner is shutting down");
        }
        {
            let database = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            self.ensure_publication_barrier_clear(&database.state, &database.github_setup)?;
        }
        if self.running.load(Ordering::SeqCst)
            || self.planning_running.load(Ordering::SeqCst)
            || self.chat.is_running()
            || self.tools.blocks_work()
            || self.escalation_running.load(Ordering::SeqCst)
        {
            bail!("Stop active developer work before applying a repair proposal");
        }
        let (project, proposal_chat_id, chat_request_id) = {
            let database = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            let feature = database
                .state
                .queue
                .iter()
                .find(|feature| feature.id == request.feature_id)
                .context("Feature not found")?;
            let proposal = feature
                .escalation_proposal
                .as_ref()
                .filter(|proposal| proposal.proposal_id == proposal_id)
                .context("Repair proposal not found")?;
            (
                feature.project.clone(),
                proposal.chat_id.clone(),
                proposal.chat_request_id.clone(),
            )
        };
        let chat_id = self
            .chat
            .resolve_chat_id(&project, proposal_chat_id.as_deref())?;
        if request
            .chat_id
            .as_deref()
            .is_some_and(|requested| requested != chat_id)
        {
            bail!("Repair proposal belongs to a different conversation");
        }
        let handoff = self
            .chat
            .repair_handoff(&project, &chat_id, &chat_request_id)?;

        let inference_lease = match self.inference_gate.try_acquire() {
            Ok(lease) => lease,
            Err(error) => return Err(error.context("Repair proposal application cannot start")),
        };
        let mut database = self
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?;
        self.ensure_publication_barrier_clear(&database.state, &database.github_setup)?;
        if self.emergency_paused(&database.state) {
            bail!("Clear Emergency Pause before applying a repair proposal");
        }
        if self.planning_running.load(Ordering::SeqCst)
            || self.chat.is_running()
            || self.tools.blocks_work()
            || self.escalation_running.load(Ordering::SeqCst)
        {
            bail!("Stop active developer work before applying a repair proposal");
        }
        self.running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| anyhow!("Another developer run started"))?;
        self.cancellation.store(0, Ordering::SeqCst);
        self.tool_cancellation.store(false, Ordering::SeqCst);
        self.repair_loop_authorized.store(false, Ordering::SeqCst);
        let applied = mutate_database(&mut database, |state, _github_setup| {
            if state.revision != request.expected_revision {
                bail!("Runner revision changed; refresh before applying the repair proposal");
            }
            if self.emergency_paused(state) {
                bail!("Clear Emergency Pause before applying a repair proposal");
            }
            let first = state
                .queue
                .iter()
                .position(|feature| feature.status != "succeeded" && feature.status != "removed")
                .context("No unfinished feature to repair")?;
            if state.queue[first].id != request.feature_id {
                bail!("Only the first unfinished feature can be repaired");
            }
            let feature = &mut state.queue[first];
            if feature.status != "failed" || feature.checkpoint != expected_checkpoint {
                bail!(
                    "Failed feature binding changed; refresh before applying the repair proposal"
                );
            }
            let proposal = feature
                .escalation_proposal
                .as_mut()
                .filter(|proposal| proposal.proposal_id == proposal_id)
                .context("Repair proposal not found")?;
            if proposal.status != "ready" {
                bail!("Repair proposal is not ready for approval");
            }
            if proposal.binding_revision != request.expected_revision
                || proposal.feature_checkpoint != expected_checkpoint
                || proposal.feature_id != feature.id
            {
                bail!("Repair proposal binding changed; refresh before applying it");
            }
            validate_repair_handoff(
                &handoff,
                &feature.project,
                &chat_id,
                &proposal.chat_request_id,
                &proposal.diagnosis_sha256,
            )?;
            let expected_proposal_sha256 = feature
                .escalation_history
                .iter()
                .rev()
                .find(|evidence| evidence.proposal_id == proposal_id && evidence.outcome == "ready")
                .and_then(|evidence| evidence.proposal_sha256.as_deref())
                .context("Ready repair proposal evidence is missing")?;
            let approved_proposal_sha256 = repair_escalation_proposal_sha256(proposal)?;
            if approved_proposal_sha256 != expected_proposal_sha256 {
                bail!("Repair proposal evidence changed before approval");
            }
            let project_root = fs::canonicalize(self.root.join(&feature.project))?;
            if !project_root.starts_with(&self.root) {
                bail!("Project escapes the workspace root");
            }
            let validation_paths = validation_path_references(&project_root, &feature.validation)?;
            let protected_inputs = repair_protected_inputs(&project_root, &validation_paths)?
                .into_iter()
                .collect::<std::collections::BTreeMap<_, _>>();
            if protected_inputs != proposal.protected_inputs {
                bail!("A protected test or validation input changed after proposal preparation");
            }
            for file in &proposal.files {
                validate_repair_file_admission(&file.path, &file.after)?;
                let path = checked_path(&project_root, &file.path)?;
                let current = if path.exists() {
                    Some(
                        fs::read_to_string(&path)
                            .with_context(|| format!("Proposed file {} is not UTF-8", file.path))?,
                    )
                } else {
                    None
                };
                if current != file.before {
                    bail!("{} changed after proposal preparation", file.path);
                }
                if file.protected != is_protected_repair_input(&file.path, &validation_paths) {
                    bail!("Protected-file classification changed before approval");
                }
            }
            if proposal.files.is_empty() {
                bail!("Repair proposal contains no file changes");
            }
            // The final review must retain every current generated file, including
            // the last ordinary repair. Validate that cumulative evidence before
            // granting this one proposal application; proposal files are merged
            // only after their writes are durably recorded.
            merge_escalation_review_edits(
                feature.edits.as_deref().unwrap_or_default(),
                &proposal.files,
                &project_root,
            )?;
            let apply_request_id = Uuid::new_v4().to_string();
            let proposal = feature
                .escalation_proposal
                .as_mut()
                .filter(|proposal| proposal.proposal_id == proposal_id)
                .context("Repair proposal not found")?;
            proposal.status = "approved".into();
            proposal.apply_request_id = Some(apply_request_id);
            proposal.binding_revision = state.revision + 1;
            feature.escalation_history.push(RepairEscalationEvidence {
                proposal_id: proposal.proposal_id.clone(),
                attempt: proposal.attempt,
                model_target: proposal.model_target.clone(),
                model: proposal.model.clone(),
                chat_id: proposal.chat_id.clone(),
                chat_request_id: proposal.chat_request_id.clone(),
                diagnosis_sha256: proposal.diagnosis_sha256.clone(),
                outcome: "approved_to_apply".into(),
                proposal_sha256: Some(approved_proposal_sha256),
                candidate_sha256: Some(repair_escalation_candidate_sha256(proposal)?),
                summary:
                    "Owner approved these exact repair proposal bytes for one application attempt"
                        .into(),
                source: proposal.source.clone(),
                automatic_epoch: proposal.automatic_epoch,
                policy_revision: proposal.policy_revision,
                limit_snapshot: proposal.limit_snapshot,
                project_state_sha256: proposal.project_state_sha256.clone(),
                authorization_revision: None,
            });
            feature.status = "paused".into();
            feature.checkpoint = format!("escalation_{}_approved", proposal.attempt);
            feature.message =
                "Approved repair proposal is ready for its one bounded application attempt".into();
            feature.repair_pending = false;
            feature.escalation_pending = true;
            feature.review_status = "pending".into();
            feature.review_summary = "Required ChatGPT Codex review has not started".into();
            feature.review_pending = None;
            Ok(())
        });
        drop(database);
        if let Err(error) = applied {
            self.running.store(false, Ordering::SeqCst);
            return Err(error);
        }
        self.launch(Some(inference_lease));
        self.escalation_snapshot(&request.feature_id)
    }

    fn cancel_escalation(&self, request: RepairEscalationMutation) -> Result<Value> {
        Uuid::parse_str(&request.feature_id).context("Invalid feature ID")?;
        let proposal_id = request
            .proposal_id
            .as_deref()
            .context("Missing repair proposal ID")?;
        Uuid::parse_str(proposal_id).context("Invalid repair proposal ID")?;
        let expected_checkpoint = request
            .expected_checkpoint
            .as_deref()
            .context("Missing expected checkpoint")?;
        self.change_database(|state, github_setup| {
            self.ensure_publication_barrier_clear(state, github_setup)?;
            if state.revision != request.expected_revision {
                bail!("Runner revision changed; refresh before cancelling the repair proposal");
            }
            let feature = state
                .queue
                .iter_mut()
                .find(|feature| feature.id == request.feature_id)
                .context("Feature not found")?;
            if feature.checkpoint != expected_checkpoint {
                bail!("Feature checkpoint changed; refresh before cancelling the repair proposal");
            }
            let proposal = feature
                .escalation_proposal
                .as_mut()
                .filter(|proposal| proposal.proposal_id == proposal_id)
                .context("Repair proposal not found")?;
            if !matches!(
                proposal.status.as_str(),
                "preparing" | "ready" | "unavailable"
            ) {
                bail!("Repair proposal cannot be cancelled in its current state");
            }
            proposal.status = "cancelled".into();
            proposal.summary = "Repair proposal was cancelled without changing files".into();
            proposal.error = None;
            proposal.binding_revision = state.revision + 1;
            terminalize_unapplied_escalation(
                feature,
                "cancelled",
                "authorization_not_run",
                "cancelled",
                "Independent review was not run because the repair proposal was cancelled before application",
            )?;
            Ok(())
        })?;
        self.cancel_escalation_call(false);
        self.escalation_snapshot(&request.feature_id)
    }
    fn remove(&self, id: &str) -> Result<()> {
        let mut db = self
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?;
        self.ensure_publication_barrier_clear(&db.state, &db.github_setup)?;
        if self.running.load(Ordering::SeqCst) {
            bail!("Stop the active run before removing a feature");
        }
        if self.chat.is_running() || self.tools.blocks_work() {
            bail!("Stop project chat and resolve project tool actions before removing a feature");
        }
        if self.planning_running.load(Ordering::SeqCst) {
            bail!("Wait for brainstorming to finish before removing a feature");
        }
        if self.escalation_running.load(Ordering::SeqCst) {
            bail!("Cancel the active repair proposal before removing a feature");
        }
        let mut next = db.state.clone();
        if !remove_feature(&mut next, id)? {
            return Ok(());
        }
        next.revision += 1;
        let data = serde_json::to_string(&next)?;
        db.connection.execute(
            "INSERT INTO developer_state(id,state) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET state=excluded.state",
            [data],
        )?;
        db.state = next;
        Ok(())
    }
    fn begin_shutdown(&self) -> Result<()> {
        let _database = self
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?;
        if self.running.load(Ordering::SeqCst)
            || self.chat.is_running()
            || self.tools.is_running()
            || self.planning_running.load(Ordering::SeqCst)
            || self.escalation_running.load(Ordering::SeqCst)
            || self.publication_running.load(Ordering::SeqCst)
            || self.publication_connection_running.load(Ordering::SeqCst)
        {
            bail!("Stop active feature and chat work before shutting down");
        }
        self.publication_cancellation.fetch_max(1, Ordering::SeqCst);
        self.shutdown.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn github_setup_snapshot(&self) -> Result<Value> {
        let database = self
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?;
        let busy = self.publication_connection_running.load(Ordering::SeqCst);
        let can_mutate = !self.shutdown.load(Ordering::SeqCst)
            && !self.emergency_paused(&database.state)
            && !Self::publication_unresolved(&database.state)
            && !self.running.load(Ordering::SeqCst)
            && !self.planning_running.load(Ordering::SeqCst)
            && !self.escalation_running.load(Ordering::SeqCst)
            && !self.chat.is_running()
            && !self.tools.blocks_work()
            && !self.publication_running.load(Ordering::SeqCst);
        Ok(database
            .github_setup
            .snapshot(database.state.revision, busy, can_mutate))
    }

    fn ensure_github_setup_action_admitted(
        &self,
        database: &Database,
        expected_revision: u64,
        allow_unresolved: bool,
    ) -> Result<()> {
        if database.state.revision != expected_revision {
            bail!("Runner revision changed; refresh GitHub setup before continuing");
        }
        if self.shutdown.load(Ordering::SeqCst) || self.emergency_paused(&database.state) {
            bail!("Clear shutdown or Emergency Pause before changing GitHub setup");
        }
        if !self.developer_work_is_idle() || Self::publication_unresolved(&database.state) {
            bail!("Stop developer work and resolve publication before changing GitHub setup");
        }
        if !allow_unresolved && database.github_setup.blocks_dependent_work() {
            bail!("Reconcile the unfinished GitHub setup operation first");
        }
        Ok(())
    }

    fn reserve_github_setup_operation(
        &self,
        database: &Database,
        expected_revision: u64,
        allow_unresolved: bool,
    ) -> Result<()> {
        self.ensure_github_setup_action_admitted(database, expected_revision, allow_unresolved)?;
        self.publication_connection_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| anyhow!("Another GitHub setup or connection operation is running"))?;
        self.publication_cancellation.store(0, Ordering::SeqCst);
        Ok(())
    }

    fn release_github_setup_operation(&self) {
        self.publication_connection_running
            .store(false, Ordering::SeqCst);
    }

    async fn refresh_github_account(&self, expected_revision: u64) -> Result<Value> {
        {
            let database = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            self.reserve_github_setup_operation(&database, expected_revision, false)?;
        }
        let observation = match self.publication_runtime.as_ref() {
            Some(runtime) => {
                runtime
                    .inspect_account(&self.publication_cancellation)
                    .await
            }
            None => Ok(GithubAccountObservation::Unavailable {
                message: "Trusted Git and GitHub CLI tools are unavailable",
            }),
        }
        .unwrap_or(GithubAccountObservation::Unavailable {
            message: "GitHub account verification is unavailable",
        });
        let result: Result<()> = {
            let account = github_account_record(&observation);
            self.change_database(|state, setup| {
                if state.revision != expected_revision {
                    bail!("Runner revision changed while refreshing the GitHub account");
                }
                setup.reset_repositories_if_account_changed(&account);
                Ok(())
            })
        };
        self.release_github_setup_operation();
        result?;
        self.github_setup_snapshot()
    }

    async fn list_github_repositories(&self, page: u32, expected_revision: u64) -> Result<Value> {
        let expected_login = {
            let database = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            if page == 0
                || page > 100
                || page != 1 && page != database.github_setup.repository_page.saturating_add(1)
            {
                bail!("Request the first or next GitHub repository page");
            }
            let expected_login = database
                .github_setup
                .account
                .login
                .clone()
                .filter(|_| database.github_setup.account.state == "signed_in")
                .context("Sign in to GitHub before listing repositories")?;
            self.reserve_github_setup_operation(&database, expected_revision, false)?;
            expected_login
        };
        let runtime = self.publication_runtime.as_ref().cloned();
        let result: Result<()> = async {
            let runtime = runtime.with_context(|| {
                self.publication_unavailable_reason
                    .clone()
                    .unwrap_or_else(|| "GitHub setup tools are unavailable".into())
            })?;
            let page_result = runtime
                .list_accessible_repositories(page, &self.publication_cancellation)
                .await?;
            if !page_result.login.eq_ignore_ascii_case(&expected_login) {
                bail!("GitHub account changed while listing repositories");
            }
            self.store_github_repository_page(expected_revision, page, &expected_login, page_result)
        }
        .await;
        self.release_github_setup_operation();
        result?;
        self.github_setup_snapshot()
    }

    fn store_github_repository_page(
        &self,
        expected_revision: u64,
        page: u32,
        expected_login: &str,
        page_result: GithubRepositoryPage,
    ) -> Result<()> {
        self.change_database(|state, setup| {
            if state.revision != expected_revision {
                bail!("Runner revision changed while listing GitHub repositories");
            }
            if setup.account.state != "signed_in"
                || !setup
                    .account
                    .login
                    .as_deref()
                    .is_some_and(|login| login.eq_ignore_ascii_case(expected_login))
            {
                bail!("GitHub account changed while listing repositories");
            }
            setup.repositories = page_result
                .repositories
                .into_iter()
                .map(github_repository_record)
                .collect();
            setup.repository_page = page;
            setup.has_more = page_result.has_more;
            Ok(())
        })
    }

    fn begin_github_sign_in(
        self: &Arc<Self>,
        operation_id: &str,
        expected_revision: u64,
    ) -> Result<Value> {
        if self.publication_runtime.is_none() {
            bail!(self
                .publication_unavailable_reason
                .clone()
                .unwrap_or_else(|| "GitHub setup tools are unavailable".into()));
        }
        let prepared = (|| -> Result<()> {
            let mut database = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            self.reserve_github_setup_operation(&database, expected_revision, false)?;
            let committed = mutate_database(&mut database, |_state, setup| {
                if setup.account.state != "signed_out" {
                    bail!("Refresh a signed-out GitHub account before starting sign-in");
                }
                setup.begin_sign_in(operation_id)
            });
            if committed.is_err() {
                self.release_github_setup_operation();
            }
            committed
        })();
        prepared?;
        let accepted = self.github_setup_snapshot()?;
        let engine = self.clone();
        let operation_id = operation_id.to_owned();
        tokio::spawn(async move {
            if let Err(error) = engine.execute_github_sign_in(&operation_id).await {
                eprintln!("developer GitHub sign-in: {error:#}");
            }
            engine.release_github_setup_operation();
        });
        Ok(accepted)
    }

    async fn execute_github_sign_in(&self, operation_id: &str) -> Result<()> {
        let runtime = self
            .publication_runtime
            .as_ref()
            .cloned()
            .with_context(|| {
                self.publication_unavailable_reason
                    .clone()
                    .unwrap_or_else(|| "GitHub setup tools are unavailable".into())
            })?;
        let outcome = runtime
            .sign_in(&self.publication_cancellation, |challenge| {
                if challenge.verification_url != developer_github_setup::DEVICE_VERIFICATION_URL {
                    bail!("GitHub sign-in returned an unexpected verification URL");
                }
                self.change_database(|state, setup| {
                    if self.publication_cancellation.load(Ordering::SeqCst) != 0
                        || state.emergency_paused
                        || self.shutdown.load(Ordering::SeqCst)
                    {
                        bail!("GitHub sign-in was cancelled before its challenge was accepted");
                    }
                    setup.record_sign_in_challenge(operation_id, &challenge.user_code)
                })
            })
            .await;
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => {
                let observation_cancellation = AtomicU8::new(0);
                let account = runtime
                    .inspect_account(&observation_cancellation)
                    .await
                    .unwrap_or(GithubAccountObservation::Unavailable {
                        message: "GitHub account truth could not be verified after sign-in stopped",
                    });
                self.finish_github_sign_in(
                    operation_id,
                    "attention",
                    "GitHub sign-in stopped without a confirmed process result; reconcile the retained operation",
                    &account,
                )?;
                return Err(
                    error.context("GitHub sign-in process failed before a confirmed result")
                );
            }
        };
        self.finish_github_sign_in_outcome(operation_id, &outcome)
    }

    fn finish_github_sign_in_outcome(
        &self,
        operation_id: &str,
        outcome: &GithubSignInOutcome,
    ) -> Result<()> {
        let (state, message) = github_sign_in_completion(outcome);
        self.finish_github_sign_in(operation_id, state, message, &outcome.account)
    }

    fn finish_github_sign_in(
        &self,
        operation_id: &str,
        state: &str,
        message: &str,
        account: &GithubAccountObservation,
    ) -> Result<()> {
        let account = github_account_record(account);
        self.change_database(|_state, setup| {
            setup.finish_sign_in(operation_id, state, message, &account)
        })
    }

    async fn cancel_github_sign_in(
        &self,
        operation_id: &str,
        expected_revision: u64,
    ) -> Result<Value> {
        {
            let database = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            if database.state.revision != expected_revision {
                bail!("Runner revision changed; refresh before cancelling GitHub sign-in");
            }
            let sign_in = database
                .github_setup
                .sign_in
                .as_ref()
                .filter(|record| record.operation_id == operation_id)
                .context("GitHub sign-in operation does not match the retained operation")?;
            if !matches!(sign_in.state.as_str(), "starting" | "waiting") {
                bail!("Only an active GitHub sign-in can be cancelled");
            }
            if !self.publication_connection_running.load(Ordering::SeqCst) {
                bail!("GitHub sign-in is not running; reconcile its retained state");
            }
            self.publication_cancellation.fetch_max(1, Ordering::SeqCst);
        }
        tokio::time::timeout(Duration::from_secs(30), async {
            while self.publication_connection_running.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .context("GitHub sign-in cancellation did not finish before the deadline")?;
        let snapshot = self.github_setup_snapshot()?;
        let state = snapshot["sign_in"]["state"]
            .as_str()
            .context("GitHub sign-in completion is missing")?;
        if !matches!(state, "cancelled" | "succeeded" | "failed" | "attention") {
            bail!("GitHub sign-in cancellation has no terminal observation");
        }
        Ok(snapshot)
    }

    async fn reconcile_github_sign_in(
        &self,
        operation_id: &str,
        expected_revision: u64,
    ) -> Result<Value> {
        {
            let database = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            let sign_in = database
                .github_setup
                .sign_in
                .as_ref()
                .filter(|record| record.operation_id == operation_id)
                .context("GitHub sign-in operation does not match the retained operation")?;
            if sign_in.state != "attention" {
                bail!("Only a GitHub sign-in needing attention can be reconciled");
            }
            self.reserve_github_setup_operation(&database, expected_revision, true)?;
        }
        let runtime = self.publication_runtime.as_ref().cloned();
        let result = async {
            let (account, credentials_consistent) = match runtime {
                Some(runtime) => runtime
                    .inspect_sign_in_credentials(&self.publication_cancellation)
                    .await
                    .unwrap_or((
                        GithubAccountObservation::Unavailable {
                            message: "GitHub account verification is unavailable",
                        },
                        false,
                    )),
                None => (
                    GithubAccountObservation::Unavailable {
                        message: "Trusted Git and GitHub CLI tools are unavailable",
                    },
                    false,
                ),
            };
            let (state, message) = if !credentials_consistent {
                (
                    "attention",
                    "Environment credentials still mask or obscure the saved GitHub login; reconciliation remains required",
                )
            } else {
                match &account {
                GithubAccountObservation::SignedIn { .. } => (
                    "succeeded",
                    "The retained GitHub sign-in now has a verified live account",
                ),
                GithubAccountObservation::SignedOut { .. } => (
                    "failed",
                    "The retained GitHub sign-in has no signed-in account",
                ),
                GithubAccountObservation::Unavailable { .. } => (
                    "attention",
                    "GitHub account verification remains unavailable; reconciliation is still required",
                ),
                }
            };
            self.finish_github_sign_in(operation_id, state, message, &account)
        }
        .await;
        self.release_github_setup_operation();
        result?;
        self.github_setup_snapshot()
    }

    fn begin_github_repository_creation(
        self: &Arc<Self>,
        operation_id: &str,
        expected_login: &str,
        name: &str,
        visibility: &str,
        expected_revision: u64,
    ) -> Result<Value> {
        if self.publication_runtime.is_none() {
            bail!(self
                .publication_unavailable_reason
                .clone()
                .unwrap_or_else(|| "GitHub setup tools are unavailable".into()));
        }
        let start = {
            let mut database = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            self.reserve_github_setup_operation(&database, expected_revision, false)?;
            let result = mutate_database(&mut database, |_state, setup| {
                if setup.account.state != "signed_in"
                    || !setup
                        .account
                        .login
                        .as_deref()
                        .is_some_and(|login| login.eq_ignore_ascii_case(expected_login))
                {
                    bail!("The confirmed GitHub account changed; refresh before creating");
                }
                setup.start_creation(operation_id, expected_login, name, visibility)
            });
            if result.is_err() {
                self.release_github_setup_operation();
            }
            result?
        };
        let accepted = self.github_setup_snapshot()?;
        if start == CreationStart::ExistingOperation {
            self.release_github_setup_operation();
            return Ok(accepted);
        }
        let engine = self.clone();
        let operation_id = operation_id.to_owned();
        let expected_login = expected_login.to_owned();
        let name = name.to_owned();
        let visibility = visibility.to_owned();
        tokio::spawn(async move {
            if let Err(error) = engine
                .execute_github_repository_creation(
                    &operation_id,
                    &expected_login,
                    &name,
                    &visibility,
                )
                .await
            {
                eprintln!("developer GitHub repository creation: {error:#}");
            }
            engine.release_github_setup_operation();
        });
        Ok(accepted)
    }

    async fn execute_github_repository_creation(
        &self,
        operation_id: &str,
        expected_login: &str,
        name: &str,
        visibility: &str,
    ) -> Result<()> {
        let runtime = self
            .publication_runtime
            .as_ref()
            .cloned()
            .context("GitHub setup tools are unavailable")?;
        let name_with_owner = format!("{expected_login}/{name}");
        match runtime
            .observe_repository(&name_with_owner, &self.publication_cancellation)
            .await
        {
            Ok(GithubRepositoryLookup::Present(observation)) => {
                return self.update_github_creation(operation_id, |creation| {
                    record_observed_repository(creation, &observation);
                    creation.state = "existing".into();
                    creation.message =
                        "A repository already exists at the confirmed target; it was not adopted or changed"
                            .into();
                    Ok(())
                });
            }
            Ok(GithubRepositoryLookup::Absent) => {
                self.update_github_creation(operation_id, |creation| {
                    creation.preflight_absent = true;
                    creation.message =
                        "The confirmed repository target was absent; creation is starting".into();
                    Ok(())
                })?;
            }
            Err(error) => {
                self.update_github_creation(operation_id, |creation| {
                    creation.state = "attention".into();
                    creation.message = "Repository absence could not be verified, so no creation request was authorized; reconcile the retained operation".into();
                    Ok(())
                })?;
                return Err(error.context("GitHub repository preflight was unavailable"));
            }
        }
        let command_result = runtime
            .create_repository(
                expected_login,
                name,
                visibility,
                &self.publication_cancellation,
            )
            .await;
        if let Ok(command_succeeded) = command_result {
            self.update_github_creation(operation_id, |creation| {
                creation.command_succeeded = command_succeeded;
                creation.message = if command_succeeded {
                    "GitHub accepted the creation command; verifying the immutable repository receipt"
                        .into()
                } else {
                    "GitHub did not confirm the creation command; observing the exact target"
                        .into()
                };
                Ok(())
            })?;
        }
        let observation_cancellation = AtomicU8::new(0);
        let observation = runtime
            .observe_repository(&name_with_owner, &observation_cancellation)
            .await;
        self.finish_github_creation_observation(operation_id, observation)?;
        command_result
            .map(|_| ())
            .context("GitHub repository creation command was unavailable")
    }

    fn update_github_creation(
        &self,
        operation_id: &str,
        f: impl FnOnce(&mut CreationRecord) -> Result<()>,
    ) -> Result<()> {
        self.change_database(|_state, setup| {
            let creation = setup.creation_mut(operation_id)?;
            f(creation)
        })
    }

    fn finish_github_creation_observation(
        &self,
        operation_id: &str,
        observation: Result<GithubRepositoryLookup>,
    ) -> Result<()> {
        self.update_github_creation(operation_id, |creation| {
            apply_github_creation_observation(creation, observation);
            Ok(())
        })
    }

    async fn reconcile_github_creation(
        &self,
        operation_id: &str,
        expected_revision: u64,
    ) -> Result<Value> {
        let name_with_owner = {
            let database = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            let creation = database
                .github_setup
                .active_creation()
                .filter(|record| record.operation_id == operation_id)
                .context(
                    "GitHub repository creation operation does not match the retained operation",
                )?;
            if creation.state != "attention" {
                bail!("Only a GitHub repository creation needing attention can be reconciled");
            }
            let name_with_owner = creation.name_with_owner.clone();
            self.reserve_github_setup_operation(&database, expected_revision, true)?;
            name_with_owner
        };
        let runtime = self.publication_runtime.as_ref().cloned();
        let result = async {
            let runtime = runtime.with_context(|| {
                self.publication_unavailable_reason
                    .clone()
                    .unwrap_or_else(|| "GitHub setup tools are unavailable".into())
            })?;
            let observation = runtime
                .observe_repository(&name_with_owner, &self.publication_cancellation)
                .await;
            self.finish_github_creation_observation(operation_id, observation)
        }
        .await;
        self.release_github_setup_operation();
        result?;
        self.github_setup_snapshot()
    }

    async fn save_github_connection(
        &self,
        project: &str,
        repository_url: &str,
        base_branch: &str,
        expected_revision: u64,
    ) -> Result<Value> {
        let runtime = self
            .publication_runtime
            .as_ref()
            .cloned()
            .with_context(|| {
                self.publication_unavailable_reason
                    .clone()
                    .unwrap_or_else(|| "GitHub publication tools are unavailable".into())
            })?;
        {
            let database = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            if database.state.revision != expected_revision {
                bail!("Runner revision changed; refresh before saving the GitHub connection");
            }
            if self.shutdown.load(Ordering::SeqCst) || self.emergency_paused(&database.state) {
                bail!("Clear shutdown or Emergency Pause before saving a GitHub connection");
            }
            if self.running.load(Ordering::SeqCst)
                || self.planning_running.load(Ordering::SeqCst)
                || self.escalation_running.load(Ordering::SeqCst)
                || self.chat.is_running()
                || self.tools.blocks_work()
                || self.publication_running.load(Ordering::SeqCst)
                || Self::publication_unresolved(&database.state)
                || database.github_setup.blocks_dependent_work()
            {
                bail!("Stop developer work and resolve publication before changing a GitHub connection");
            }
            let path = fs::canonicalize(self.root.join(project))
                .context("GitHub connections require an existing Developer project")?;
            if !path.starts_with(&self.root) || path == self.root {
                bail!("GitHub connection project escapes the workspace root");
            }
            self.publication_connection_running
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .map_err(|_| anyhow!("Another GitHub connection operation is running"))?;
            self.publication_cancellation.store(0, Ordering::SeqCst);
        }
        let result: Result<()> = async {
            let binding = runtime
                .validate_connection(
                    project,
                    repository_url,
                    base_branch,
                    &self.publication_cancellation,
                )
                .await?;
            self.change_database(|state, _github_setup| {
                if state.revision != expected_revision {
                    bail!("Runner revision changed while validating the GitHub connection");
                }
                if Self::publication_unresolved(state) {
                    bail!("Publication became unresolved while validating the GitHub connection");
                }
                state
                    .github_connections
                    .retain(|current| current.project != project);
                state.github_connections.push(binding);
                state
                    .github_connections
                    .sort_by(|left, right| left.project.cmp(&right.project));
                Ok(())
            })?;
            Ok(())
        }
        .await;
        self.publication_connection_running
            .store(false, Ordering::SeqCst);
        result?;
        self.snapshot()
    }

    fn disconnect_github_connection(&self, project: &str, expected_revision: u64) -> Result<Value> {
        self.publication_connection_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| anyhow!("Another GitHub connection operation is running"))?;
        let result = (|| -> Result<()> {
            self.change_database(|state, github_setup| {
                if state.revision != expected_revision {
                    bail!("Runner revision changed; refresh before disconnecting GitHub");
                }
                if self.shutdown.load(Ordering::SeqCst)
                    || self.emergency_paused(state)
                    || self.running.load(Ordering::SeqCst)
                    || self.planning_running.load(Ordering::SeqCst)
                    || self.escalation_running.load(Ordering::SeqCst)
                    || self.chat.is_running()
                    || self.tools.blocks_work()
                    || self.publication_running.load(Ordering::SeqCst)
                    || Self::publication_unresolved(state)
                    || github_setup.blocks_dependent_work()
                {
                    bail!(
                        "Stop developer work and resolve publication before disconnecting GitHub"
                    );
                }
                let before = state.github_connections.len();
                state
                    .github_connections
                    .retain(|binding| binding.project != project);
                if state.github_connections.len() == before {
                    bail!("GitHub connection was not found");
                }
                Ok(())
            })?;
            Ok(())
        })();
        self.publication_connection_running
            .store(false, Ordering::SeqCst);
        result?;
        self.snapshot()
    }

    fn reconcile_publication(
        self: &Arc<Self>,
        feature_id: &str,
        expected_revision: u64,
        expected_checkpoint: &str,
    ) -> Result<Value> {
        Uuid::parse_str(feature_id).context("Invalid feature ID")?;
        if self.publication_runtime.is_none() {
            bail!(self
                .publication_unavailable_reason
                .clone()
                .unwrap_or_else(|| "GitHub publication tools are unavailable".into()));
        }
        let mut database = self
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?;
        if self.publication_connection_running.load(Ordering::SeqCst) {
            bail!("Wait for the GitHub connection operation to finish");
        }
        if database.github_setup.blocks_dependent_work() {
            bail!("Reconcile the unfinished GitHub setup operation before publication");
        }
        self.publication_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| anyhow!("Another GitHub publication is running"))?;
        self.publication_cancellation.store(0, Ordering::SeqCst);
        let prepared = mutate_database(&mut database, |state, github_setup| {
            if state.revision != expected_revision {
                bail!("Runner revision changed; refresh before reconciling publication");
            }
            if self.shutdown.load(Ordering::SeqCst) || self.emergency_paused(state) {
                bail!("Clear shutdown or Emergency Pause before reconciling publication");
            }
            if github_setup.blocks_dependent_work() {
                bail!("Reconcile the unfinished GitHub setup operation before publication");
            }
            if self.running.load(Ordering::SeqCst)
                || self.planning_running.load(Ordering::SeqCst)
                || self.escalation_running.load(Ordering::SeqCst)
                || self.chat.is_running()
                || self.tools.blocks_work()
            {
                bail!("Stop other developer work before reconciling publication");
            }
            let feature = state
                .queue
                .iter_mut()
                .find(|feature| feature.id == feature_id)
                .context("Feature not found")?;
            if feature.checkpoint != expected_checkpoint
                || expected_checkpoint != "publication_attention"
            {
                bail!("Publication checkpoint changed; refresh before reconciling");
            }
            publication_input(feature)?;
            let publication = feature
                .publication
                .as_mut()
                .context("Publication record is missing")?;
            if publication.status != "attention" {
                bail!("Only a publication needing attention can be reconciled");
            }
            publication.status = "pending".into();
            publication.message = "Explicit reconciliation is inspecting the existing branch, pull request, checks, and merge".into();
            feature.status = "running".into();
            feature.checkpoint = "publication_reconciling".into();
            feature.message = publication.message.clone();
            Ok(())
        });
        drop(database);
        if let Err(error) = prepared {
            self.publication_running.store(false, Ordering::SeqCst);
            return Err(error);
        }
        let accepted = self.snapshot()?;
        let engine = self.clone();
        let feature_id = feature_id.to_owned();
        tokio::spawn(async move {
            if let Err(error) = engine.execute_publication_reserved(&feature_id).await {
                eprintln!("developer publication reconciliation: {error:#}");
            }
        });
        Ok(accepted)
    }
    async fn run_queue(&self, mut inference_lease: Option<InferenceLease>) -> Result<()> {
        loop {
            let feature = self.change(|s| {
                let selected = s
                    .queue
                    .iter()
                    .find(|f| f.status != "succeeded" && f.status != "removed")
                    .map(|feature| feature.id.clone());
                if let Some(feature_id) = selected.as_deref() {
                    let should_bind = s.queue.iter().any(|feature| {
                        feature.id == feature_id
                            && feature.status == "queued"
                            && feature.checkpoint == "not_started"
                            && feature.auto_repair_lifecycle == "inactive"
                    });
                    if should_bind {
                        if s.emergency_paused || self.cancellation.load(Ordering::SeqCst) != 0 {
                            bail!(
                                "Emergency Pause or cancellation blocks queued feature admission"
                            );
                        }
                        arm_selected_queued_feature(s, feature_id)?;
                    }
                }
                Ok(selected.and_then(|feature_id| {
                    s.queue
                        .iter()
                        .find(|feature| feature.id == feature_id)
                        .cloned()
                }))
            })?;
            let Some(feature) = feature else {
                break;
            };
            if feature.auto_repair_lifecycle == "running" {
                let policy_enabled = self
                    .database
                    .lock()
                    .map_err(|_| anyhow!("state lock failed"))?
                    .state
                    .auto_ai_repair_enabled;
                self.repair_loop_authorized
                    .store(policy_enabled, Ordering::SeqCst);
            }
            let feature = self.freeze_feature_publication(feature)?;
            if feature.status == "failed"
                && feature.auto_repair_lifecycle == "running"
                && feature.repair_attempts >= REPAIR_LIMIT
                && !feature.repair_pending
                && !feature.escalation_pending
                && automatic_code_failure_is_eligible(&feature)
            {
                if self.prepare_next_automatic_escalation(&feature.id).await? {
                    continue;
                }
                break;
            }
            if self.cancelled() {
                self.change(|state| {
                    let current = state
                        .queue
                        .iter_mut()
                        .find(|candidate| candidate.id == feature.id)
                        .context("feature missing")?;
                    if automatic_post_apply_is_ambiguous(current)
                        && (feature.auto_repair_lifecycle == "running"
                            || current.auto_repair_lifecycle == "quarantined")
                    {
                        quarantine_automatic_post_apply(
                            current,
                            state.revision + 1,
                            "Automatic repair was cancelled after project files were applied. Validation or review completion is ambiguous; inspect retained evidence because ordinary Resume cannot replay this attempt.",
                        )?;
                    } else if current.escalation_pending
                        && (current.checkpoint.ends_with("_approved")
                            || current.checkpoint.ends_with("_applying"))
                    {
                        let project = fs::canonicalize(self.root.join(&current.project))?;
                        if !project.starts_with(&self.root) {
                            bail!("Project escapes the workspace root");
                        }
                        quarantine_escalation_application(
                            current,
                            &project,
                            state.revision + 1,
                            "Repair proposal application was cancelled before it completed. Exact current bytes were reconciled; prepare and explicitly approve a new proposal because Resume will not replay it.",
                        )?;
                    }
                    Ok(())
                })?;
                break;
            }
            let validation_only_limit_recovery =
                feature.checkpoint == "auto_repair_limit_revalidating";
            let active_inference_lease = if validation_only_limit_recovery {
                None
            } else {
                Some(match inference_lease.take() {
                    Some(lease) => lease,
                    None => match self.inference_gate.try_acquire() {
                        Ok(lease) => lease,
                        Err(_) => {
                            self.change(|state| {
                            let current = state
                                .queue
                                .iter_mut()
                                .find(|candidate| candidate.id == feature.id)
                                .context("feature missing")?;
                            current.message = "A local model is answering project chat. Start this feature after the reply finishes.".into();
                            Ok(())
                        })?;
                            break;
                        }
                    },
                })
            };
            let result = self.run_feature(&feature).await;
            drop(active_inference_lease);
            if result
                .as_ref()
                .is_err_and(|error| error.downcast_ref::<UnconfirmedTermination>().is_some())
            {
                // validate_command already performed the fail-closed emergency
                // transition. Preserve its cleanup ambiguity without allowing a
                // later cancellation/evidence write to replace it.
                return result;
            }
            if self.cancelled() {
                self.change(|state| {
                    let current = state
                        .queue
                        .iter_mut()
                        .find(|candidate| candidate.id == feature.id)
                        .context("feature missing")?;
                    if current.checkpoint == "validation_cleanup_unconfirmed" {
                        // validate_command already durably engaged Emergency Pause
                        // and quarantined this exact cleanup ambiguity.
                    } else if automatic_post_apply_is_ambiguous(current)
                        && (feature.auto_repair_lifecycle == "running"
                            || current.auto_repair_lifecycle == "quarantined")
                    {
                        quarantine_automatic_post_apply(
                            current,
                            state.revision + 1,
                            "Automatic repair was interrupted after project files were applied. Validation or review completion is ambiguous; inspect retained evidence because ordinary Resume cannot replay this attempt.",
                        )?;
                    } else if current.escalation_pending
                        && (current.checkpoint.ends_with("_approved")
                            || current.checkpoint.ends_with("_applying"))
                    {
                        let project = fs::canonicalize(self.root.join(&current.project))?;
                        if !project.starts_with(&self.root) {
                            bail!("Project escapes the workspace root");
                        }
                        quarantine_escalation_application(
                            current,
                            &project,
                            state.revision + 1,
                            "Repair proposal application was interrupted before all approved files were durably recorded. Inspect the workspace and prepare a new proposal; Resume will not replay the remaining edits.",
                        )?;
                    } else if let Some(publication) = current
                        .publication
                        .as_ref()
                        .filter(|publication| publication.status == "attention")
                    {
                        current.status = "failed".into();
                        current.checkpoint = "publication_attention".into();
                        current.message = publication.message.clone();
                    } else {
                        pause_after_post_run_cancellation(current);
                    }
                    Ok(())
                })?;
                break;
            }
            if let Err(error) = result {
                let escalation_attempt = feature.escalation_pending;
                let repairable_validation = error
                    .downcast_ref::<RepairableValidationFailure>()
                    .is_some();
                let review_rejection = error.downcast_ref::<RepairableReviewRejection>().is_some();
                let reserved_next = self.change(|s| {
                    let next_revision = s.revision + 1;
                    let current = s
                        .queue
                        .iter_mut()
                        .find(|candidate| candidate.id == feature.id)
                        .context("feature missing")?;
                    current.status = "failed".into();
                    let latest_error = format!("{error:#}");
                    let code_failure = repairable_validation || review_rejection;
                    let durable_error = if code_failure {
                        let bounded_error = latest_error
                            .chars()
                            .take(CODE_FAILURE_SUMMARY_LIMIT)
                            .collect::<String>();
                        bounded_code_failure_summary(&bounded_error).unwrap_or_else(|_| {
                            durable_failure_summary(
                                &latest_error,
                                "Validation or independent review failed",
                                CODE_FAILURE_SUMMARY_LIMIT,
                            )
                        })
                    } else {
                        durable_failure_summary(
                            &latest_error,
                            "Developer operation failed",
                            CODE_FAILURE_SUMMARY_LIMIT,
                        )
                    };
                    if code_failure {
                        current.last_code_failure_summary = durable_error.clone();
                    }
                    current.message =
                        if repairable_validation
                            && !escalation_attempt
                            && current.repair_attempts >= REPAIR_LIMIT
                        {
                            format!("Repair stopped after {REPAIR_LIMIT} attempts.\n{durable_error}")
                        } else {
                            durable_error.clone()
                        }
                        .chars()
                        .take(4000)
                        .collect();
                    current.repair_pending = false;
                    current.last_failure_kind = if repairable_validation {
                        "validation_failure"
                    } else if review_rejection {
                        "review_rejection"
                    } else {
                        "operational"
                    }
                    .into();
                    if escalation_attempt {
                        if current.checkpoint.ends_with("_approved")
                            || current.checkpoint.ends_with("_applying")
                        {
                            let project = fs::canonicalize(self.root.join(&current.project))?;
                            if !project.starts_with(&self.root) {
                                bail!("Project escapes the workspace root");
                            }
                            quarantine_escalation_application(
                                current,
                                &project,
                                next_revision,
                                &format!(
                                    "Repair proposal application did not complete: {durable_error}. Inspect the workspace and prepare a new proposal; Resume will not replay remaining edits."
                                ),
                            )?;
                        } else {
                            finish_escalation_application(
                                current,
                                next_revision,
                                "failed",
                                &durable_error,
                            )?;
                        }
                    }
                    if review_rejection && !self.cancelled() {
                        self.repair_loop_authorized
                            .store(!escalation_attempt, Ordering::SeqCst);
                    }
                    if !escalation_attempt
                        && (repairable_validation || review_rejection)
                        && !self.cancelled()
                        && self.repair_loop_authorized.load(Ordering::SeqCst)
                        && current.repair_attempts < REPAIR_LIMIT
                    {
                        reserve_repair_attempt(current)?;
                        Ok(true)
                    } else {
                        if current.auto_repair_lifecycle == "running" && !code_failure {
                            set_auto_repair_lifecycle(
                                current,
                                "held",
                                "Automatic repair stopped on an operational failure. Inspect the retained evidence and explicitly Resume when the condition is corrected.",
                            )?;
                        }
                        Ok(false)
                    }
                })?;
                if reserved_next {
                    continue;
                }
                let automatic_continue = {
                    let database = self
                        .database
                        .lock()
                        .map_err(|_| anyhow!("state lock failed"))?;
                    database.state.auto_ai_repair_enabled
                        && (repairable_validation || review_rejection)
                        && database
                            .state
                            .queue
                            .iter()
                            .find(|current| current.id == feature.id)
                            .is_some_and(|current| current.auto_repair_lifecycle == "running")
                };
                if automatic_continue {
                    continue;
                }
                break;
            }
            if let Err(error) = self.complete_reviewed_feature(&feature.id).await {
                let message = format!("Feature completion failed closed: {error:#}");
                self.change(|state| {
                    let current = state
                        .queue
                        .iter_mut()
                        .find(|candidate| candidate.id == feature.id)
                        .context("feature missing")?;
                    if current.checkpoint != "publication_attention" {
                        current.status = "failed".into();
                        current.message = message.chars().take(4000).collect();
                    }
                    if current.auto_repair_lifecycle == "running" {
                        set_auto_repair_lifecycle(
                            current,
                            "held",
                            "Automatic repair stopped because reviewed completion failed. Inspect the retained validation and review evidence, correct the operational condition, then explicitly Resume.",
                        )?;
                    }
                    Ok(())
                })?;
                return Err(error);
            }
            if feature.repair_pending {
                self.repair_loop_authorized.store(false, Ordering::SeqCst);
            }
            if !self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?
                .state
                .auto_run
            {
                break;
            }
        }
        Ok(())
    }

    async fn run_tool_feature_candidate(
        &self,
        feature: &Feature,
        project: &Path,
        approved_plan: &str,
        validation_paths: &[String],
        protected_repair_inputs: &std::collections::HashMap<String, String>,
    ) -> Result<ToolFeatureOutcome> {
        let target = self.model_target(&feature.model_target)?;
        let model = ToolModelConfig {
            target: target.id.into(),
            url: target.url.clone(),
            model: target.model.clone(),
        };
        let mut workspace_revision = self.tools.snapshot(&feature.project)?["workspace_revision"]
            .as_u64()
            .context("Tool workspace revision is unavailable")?;
        let mut edits = Vec::new();

        if feature.repair_pending && repair_needs_environment_preparation(feature) {
            let result = self
                .tools
                .run_chat(ToolChatRequest {
                    request_id: Uuid::new_v4().to_string(),
                    project: feature.project.clone(),
                    chat_id: None,
                    prompt: tool_environment_prompt(feature)?,
                    model: model.clone(),
                    attachments: Vec::new(),
                    feature_id: Some(feature.id.clone()),
                    forbidden_write_paths: repair_forbidden_tool_paths(validation_paths),
                    working_project: None,
                    cancellation: self.tool_cancellation.clone(),
                })
                .await;
            let mutations = self
                .tools
                .project_mutations(&feature.project, workspace_revision)?;
            workspace_revision = mutations
                .last()
                .map_or(workspace_revision, |mutation| mutation.revision);
            edits = match tool_mutation_edits(&mutations, Some(&feature.id)) {
                Ok(edits) => edits,
                Err(error) => {
                    self.record_tool_candidate_failure(
                        &feature.id,
                        workspace_revision,
                        &[],
                        false,
                        &error.to_string(),
                    )?;
                    return Err(error);
                }
            };
            if let Some(edit) = edits
                .iter()
                .find(|edit| !is_dependency_manifest(&edit.path))
            {
                let error = anyhow!(
                    "Environment preparation changed {} outside the dependency manifest allowlist; effects are quarantined",
                    edit.path
                );
                self.record_tool_candidate_failure(
                    &feature.id,
                    workspace_revision,
                    &edits,
                    false,
                    &error.to_string(),
                )?;
                return Err(error);
            }
            if let Err(error) = result {
                self.record_tool_candidate_failure(
                    &feature.id,
                    workspace_revision,
                    &edits,
                    false,
                    &error.to_string(),
                )?;
                return Err(error);
            }
            if repair_protected_inputs(project, validation_paths)? != *protected_repair_inputs {
                let error =
                    anyhow!("Environment preparation changed a protected test or validation input");
                self.record_tool_candidate_failure(
                    &feature.id,
                    workspace_revision,
                    &edits,
                    false,
                    &error.to_string(),
                )?;
                return Err(error);
            }
            if let Err(error) = require_project_virtual_environment(project) {
                self.record_tool_candidate_failure(
                    &feature.id,
                    workspace_revision,
                    &edits,
                    false,
                    &error.to_string(),
                )?;
                return Err(error);
            }
            match self
                .validate_command_with_candidate_evidence(
                    feature,
                    project,
                    workspace_revision,
                    &edits,
                )
                .await
            {
                Ok(_) => {
                    return Ok(ToolFeatureOutcome {
                        edits,
                        application_edits: Vec::new(),
                        workspace_revision,
                        applied_to_live_project: true,
                        model: model.model,
                    });
                }
                Err(error)
                    if error
                        .downcast_ref::<RepairableValidationFailure>()
                        .is_some() => {}
                Err(error) => {
                    return self.finish_environment_preparation_validation_error(
                        &feature.id,
                        workspace_revision,
                        &edits,
                        error,
                    );
                }
            }
        }

        let repair_stage = if feature.repair_pending {
            Some(RepairStage::create(&self.root, project)?)
        } else {
            None
        };
        let result = self
            .tools
            .run_chat(ToolChatRequest {
                request_id: Uuid::new_v4().to_string(),
                project: feature.project.clone(),
                chat_id: None,
                prompt: tool_feature_prompt(feature, approved_plan)?,
                model: model.clone(),
                attachments: Vec::new(),
                feature_id: Some(feature.id.clone()),
                forbidden_write_paths: if feature.repair_pending {
                    repair_forbidden_tool_paths(validation_paths)
                } else {
                    Vec::new()
                },
                working_project: repair_stage
                    .as_ref()
                    .map(|stage| stage.project_name.clone()),
                cancellation: self.tool_cancellation.clone(),
            })
            .await;
        let mutations = self
            .tools
            .project_mutations(&feature.project, workspace_revision)?;
        workspace_revision = mutations
            .last()
            .map_or(workspace_revision, |mutation| mutation.revision);
        let live_environment_edits = edits.clone();
        let candidate_edits = match tool_mutation_edits(&mutations, Some(&feature.id)) {
            Ok(edits) => edits,
            Err(error) => {
                self.record_tool_candidate_failure(
                    &feature.id,
                    workspace_revision,
                    &edits,
                    repair_stage.is_some(),
                    &error.to_string(),
                )?;
                return Err(error);
            }
        };
        let (combined_edits, stage_application_edits) =
            tool_review_and_application_edits(&edits, &candidate_edits, repair_stage.is_some())?;
        edits = combined_edits;
        if let Err(error) = result {
            self.record_tool_candidate_failure(
                &feature.id,
                workspace_revision,
                if repair_stage.is_some() {
                    &live_environment_edits
                } else {
                    &edits
                },
                repair_stage.is_some(),
                &error.to_string(),
            )?;
            return Err(error);
        }
        if self.cancelled() {
            let error = anyhow!("Stopped");
            self.record_tool_candidate_failure(
                &feature.id,
                workspace_revision,
                if repair_stage.is_some() {
                    &live_environment_edits
                } else {
                    &edits
                },
                repair_stage.is_some(),
                &error.to_string(),
            )?;
            return Err(error);
        }
        if let Some(stage) = &repair_stage {
            let protected_unchanged = (|| -> Result<bool> {
                Ok(
                    repair_protected_inputs(stage.project_path(), validation_paths)?
                        == *protected_repair_inputs
                        && repair_protected_inputs(project, validation_paths)?
                            == *protected_repair_inputs,
                )
            })();
            if !matches!(protected_unchanged, Ok(true)) {
                let detail = protected_unchanged
                    .err()
                    .map(|error| format!(" ({error})"))
                    .unwrap_or_default();
                let error = anyhow!(
                    "A protected test or validation input changed during staged tool-assisted repair; no candidate files were applied{detail}"
                );
                self.record_tool_candidate_failure(
                    &feature.id,
                    workspace_revision,
                    &live_environment_edits,
                    true,
                    &error.to_string(),
                )?;
                return Err(error);
            }
        }
        Ok(ToolFeatureOutcome {
            edits,
            application_edits: if feature.repair_pending {
                stage_application_edits
            } else {
                Vec::new()
            },
            workspace_revision,
            applied_to_live_project: !feature.repair_pending,
            model: model.model,
        })
    }

    fn record_tool_candidate_failure(
        &self,
        feature_id: &str,
        workspace_revision: u64,
        live_edits: &[Edit],
        staged_candidate: bool,
        reason: &str,
    ) -> Result<()> {
        self.change(|state| {
            let feature = state
                .queue
                .iter_mut()
                .find(|feature| feature.id == feature_id)
                .context("feature missing")?;
            quarantine_tool_candidate(
                feature,
                workspace_revision,
                live_edits,
                staged_candidate,
                reason,
            )
        })
        .map(|_| ())
    }

    fn finish_environment_preparation_validation_error(
        &self,
        feature_id: &str,
        workspace_revision: u64,
        live_edits: &[Edit],
        error: anyhow::Error,
    ) -> Result<ToolFeatureOutcome> {
        if error.downcast_ref::<UnconfirmedTermination>().is_some() {
            // validate_command has already latched and attempted to persist the
            // global Emergency Pause. Do not let a secondary candidate-evidence
            // write replace the cleanup ambiguity or its owner action.
            return Err(error);
        }
        self.record_tool_candidate_failure(
            feature_id,
            workspace_revision,
            live_edits,
            false,
            &error.to_string(),
        )?;
        Err(error)
    }

    async fn complete_reviewed_feature(&self, feature_id: &str) -> Result<()> {
        if !self.prepare_reviewed_completion(feature_id)? {
            return Ok(());
        }
        self.execute_publication(feature_id).await
    }

    fn prepare_reviewed_completion(&self, feature_id: &str) -> Result<bool> {
        // Keep the durable state lock across the final filesystem rebind and success
        // transition. A review decision alone is never sufficient: the current bytes
        // must still match the exact packet approved by Codex immediately before the
        // queue may advance.
        let mut db = self
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?;
        let mut next = db.state.clone();
        let completion_cancelled = self.cancelled() || next.emergency_paused;
        let completion_revision = next.revision.checked_add(1).context("Revision overflow")?;
        let current = next
            .queue
            .iter_mut()
            .find(|candidate| candidate.id == feature_id)
            .context("feature missing")?;
        if completion_cancelled {
            current.status = "paused".into();
            current.review_status = "interrupted".into();
            current.review_summary = "ChatGPT Codex approval was invalidated because the run stopped before completion. Resume revalidates the current bytes and starts a fresh review.".into();
            current.checkpoint = "review_completion_interrupted".into();
            current.message = current.review_summary.clone();
            next.revision = completion_revision;
            let data = serde_json::to_string(&next)?;
            db.connection.execute("INSERT INTO developer_state(id,state) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET state=excluded.state", [data])?;
            db.state = next;
            bail!("Cloud review completion was stopped");
        }
        let binding_result = (|| {
            let project = fs::canonicalize(self.root.join(&current.project))?;
            if !project.starts_with(&self.root) {
                bail!("Project escapes the workspace root");
            }
            verify_approved_review_binding(current, &project)
        })();
        if let Err(error) = binding_result {
            if current.auto_repair_lifecycle == "running" {
                quarantine_automatic_post_apply(
                    current,
                    completion_revision,
                    "Generated files changed after ChatGPT Codex approval. Automatic repair stopped at an ambiguous post-review binding boundary; prepare a fresh owner-approved proposal after inspection.",
                )?;
                current.review_status = "interrupted".into();
                current.review_summary = current.message.clone();
            } else {
                current.status = "failed".into();
                current.review_status = "interrupted".into();
                current.review_summary = "Generated files changed after ChatGPT Codex approval. Resume revalidates the current bytes and starts a fresh review.".into();
                current.checkpoint = "review_binding_changed".into();
                current.message = current.review_summary.clone();
            }
            next.revision = completion_revision;
            let data = serde_json::to_string(&next)?;
            db.connection.execute("INSERT INTO developer_state(id,state) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET state=excluded.state", [data])?;
            db.state = next;
            return Err(error);
        }
        if !current.publication_selection_frozen {
            bail!("Feature publication selection was not frozen before implementation");
        }
        if current.publication_binding.is_none() {
            current.status = "succeeded".into();
            current.checkpoint = format!("review_{}_approved", current.review_attempts);
            current.message = "Changes applied, validation passed, and the fixed ChatGPT Codex reviewer approved the exact generated files. This feature remains local because no GitHub repository was connected when the run started.".into();
            current.repair_pending = false;
            if current.escalation_pending {
                finish_escalation_application(
                    current,
                    completion_revision,
                    "succeeded",
                    "Repair proposal was applied once, validation passed, and ChatGPT Codex approved the exact files; the feature remains local",
                )?;
            }
            set_auto_repair_lifecycle(
                current,
                "inactive",
                "Automatic repair completed after immutable validation and independent Codex approval",
            )?;
            next.revision = completion_revision;
            let data = serde_json::to_string(&next)?;
            db.connection.execute("INSERT INTO developer_state(id,state) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET state=excluded.state", [data])?;
            db.state = next;
            return Ok(false);
        }
        if current.publication.is_some() || !current.publication_candidate.is_empty() {
            bail!("Publication was already prepared; use explicit reconciliation");
        }
        let project = fs::canonicalize(self.root.join(&current.project))?;
        let approved = current
            .review_history
            .iter()
            .rev()
            .find(|attempt| attempt.outcome == "approved" && attempt.decision_sha256.is_some())
            .context("Required Codex review approval evidence is missing")?;
        let packet = developer_review_packet(
            current,
            &project,
            current
                .edits
                .as_deref()
                .context("Approved review has no generated-file evidence")?,
            &approved.validation_evidence_sha256,
        )?;
        current.publication_candidate = packet
            .files
            .iter()
            .map(|file| FrozenPublicationFile {
                path: file.path.clone(),
                before_sha256: file.before_sha256.clone(),
                content_sha256: file.content_sha256.clone(),
                content: file.content.clone(),
                classification: file.classification.clone(),
            })
            .collect();
        let input = publication_input(current)?;
        current.publication = Some(PublicationRecord::pending(&input)?);
        current.status = "running".into();
        current.checkpoint = "publication_pending".into();
        current.message =
            "The exact reviewed candidate is ready for automatic GitHub publication".into();
        current.repair_pending = false;
        set_auto_repair_lifecycle(
            current,
            "inactive",
            "Automatic repair completed after immutable validation and independent Codex approval; publication remains independently gated",
        )?;
        next.revision = completion_revision;
        let data = serde_json::to_string(&next)?;
        db.connection.execute("INSERT INTO developer_state(id,state) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET state=excluded.state", [data])?;
        db.state = next;
        Ok(true)
    }

    async fn execute_publication(&self, feature_id: &str) -> Result<()> {
        {
            let database = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            if self.publication_connection_running.load(Ordering::SeqCst)
                || database.github_setup.blocks_dependent_work()
            {
                bail!("GitHub setup changed before publication admission");
            }
            self.publication_running
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .map_err(|_| anyhow!("Another GitHub publication is running"))?;
            self.publication_cancellation.store(0, Ordering::SeqCst);
        }
        self.execute_publication_reserved(feature_id).await
    }

    async fn execute_publication_reserved(&self, feature_id: &str) -> Result<()> {
        let _running_guard = PublicationRunningGuard(&self.publication_running);
        let runtime = match self.publication_runtime.as_ref() {
            Some(runtime) => runtime.clone(),
            None => {
                let reason = self
                    .publication_unavailable_reason
                    .as_deref()
                    .unwrap_or("Git and GitHub CLI are unavailable");
                self.publication_attention(feature_id, reason)?;
                bail!(reason.to_owned());
            }
        };
        let prepared = (|| -> Result<(PublicationInput, PublicationRecord)> {
            let database = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            let feature = database
                .state
                .queue
                .iter()
                .find(|feature| feature.id == feature_id)
                .context("Feature not found")?;
            let input = publication_input(feature)?;
            let record = feature
                .publication
                .clone()
                .context("Publication record is missing")?;
            Ok((input, record))
        })();
        let (input, mut record) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                self.publication_attention(
                    feature_id,
                    &format!("GitHub publication evidence needs attention: {error:#}"),
                )?;
                return Err(error);
            }
        };
        let result = runtime
            .publish(
                &input,
                &mut record,
                &self.publication_cancellation,
                |updated| {
                    self.change(|state| {
                        let feature = state
                            .queue
                            .iter_mut()
                            .find(|feature| feature.id == feature_id)
                            .context("Feature not found")?;
                        let current = feature
                            .publication
                            .as_ref()
                            .context("Publication record is missing")?;
                        if current.repository_url != updated.repository_url
                            || current.feature_branch != updated.feature_branch
                        {
                            bail!("Publication binding changed while recording evidence");
                        }
                        feature.publication = Some(updated.clone());
                        feature.status = "running".into();
                        if feature.checkpoint != "publication_reconciling" {
                            feature.checkpoint = "publication_pending".into();
                        }
                        feature.message = updated.message.clone();
                        Ok(())
                    })
                },
            )
            .await;
        let outcome = match result {
            Ok(()) => self.finish_publication(feature_id),
            Err(error) => {
                let message = format!("GitHub publication needs attention: {error:#}");
                self.publication_attention(feature_id, &message)?;
                Err(error)
            }
        };
        outcome
    }

    fn publication_attention(&self, feature_id: &str, message: &str) -> Result<()> {
        self.change(|state| {
            let feature = state
                .queue
                .iter_mut()
                .find(|feature| feature.id == feature_id)
                .context("Feature not found")?;
            let publication = feature
                .publication
                .as_mut()
                .context("Publication record is missing")?;
            publication.status = "attention".into();
            if publication.stage == "complete" {
                publication.stage = "verify_remote_base".into();
            }
            publication.message = message.chars().take(1000).collect();
            feature.status = "failed".into();
            feature.checkpoint = "publication_attention".into();
            feature.message = publication.message.clone();
            feature.repair_pending = false;
            if feature.auto_repair_lifecycle == "running" {
                set_auto_repair_lifecycle(
                    feature,
                    "held",
                    "Automatic repair stopped because GitHub publication requires explicit reconciliation",
                )?;
            }
            Ok(())
        })
    }

    fn finish_publication(&self, feature_id: &str) -> Result<()> {
        let completed = self.change(|state| {
            let next_revision = state.revision.checked_add(1).context("Revision overflow")?;
            let feature = state.queue.iter_mut().find(|feature| feature.id == feature_id).context("Feature not found")?;
            publication_input(feature)?;
            let publication = feature.publication.as_mut().context("Publication record is missing")?;
            publication.validate()?;
            if publication.status != "succeeded" || publication.stage != "complete" {
                bail!("GitHub publication did not produce complete merge evidence");
            }
            if publication_completion_is_cancelled(
                &self.publication_cancellation,
                state.emergency_paused,
            ) {
                publication.status = "attention".into();
                publication.stage = "verify_remote_base".into();
                publication.message = "Stop or Emergency Pause arrived before local completion was recorded. The verified remote receipt is retained; use explicit reconciliation before queue advancement.".into();
                feature.status = "failed".into();
                feature.checkpoint = "publication_attention".into();
                feature.message = publication.message.clone();
                return Ok(false);
            }
            feature.status = "succeeded".into();
            feature.checkpoint = "publication_merged".into();
            feature.message = "The exact reviewed candidate was committed on its feature branch, passed required checks, merged normally, and was verified on the remote base.".into();
            feature.repair_pending = false;
            if feature.escalation_pending {
                finish_escalation_application(
                    feature,
                    next_revision,
                    "succeeded",
                    "Repair proposal was applied once, independently approved, and verified merged on GitHub",
                )?;
                feature.checkpoint = "publication_merged".into();
            }
            set_auto_repair_lifecycle(
                feature,
                "inactive",
                "Automatic repair and independently gated publication completed",
            )?;
            Ok(true)
        })?;
        if !completed {
            bail!("GitHub publication completion was stopped and requires reconciliation");
        }
        Ok(())
    }
    async fn run_feature(&self, feature: &Feature) -> Result<()> {
        let project = self.root.join(&feature.project);
        fs::create_dir_all(&project)?;
        let project = fs::canonicalize(project)?;
        if !project.starts_with(&self.root) {
            bail!("Project escapes the workspace root");
        }
        let approved_plan = self.approved_plan_text(feature)?;
        self.change(|state| {
            let current = state
                .queue
                .iter_mut()
                .find(|candidate| candidate.id == feature.id)
                .context("feature missing")?;
            current.status = "running".into();
            current.message = "Preparing the project".into();
            if current.review_pending.is_none() {
                current.review_status = "pending".into();
                current.review_summary = "Required ChatGPT Codex review has not started".into();
            }
            Ok(())
        })?;
        let mut tool_session_applied = false;
        let mut tool_application_edits = None;
        let mut edits = if feature.escalation_pending {
            feature.edits.clone().unwrap_or_default()
        } else if let Some(edits) = &feature.edits {
            if review_checkpoint_requires_revalidation(&feature.checkpoint) {
                let refreshed = edits
                    .iter()
                    .map(|edit| {
                        let path = checked_path(&project, &edit.path)?;
                        let content = fs::read_to_string(&path).with_context(|| {
                            format!(
                                "Generated file {} is unavailable for resumed validation and review",
                                edit.path
                            )
                        })?;
                        Ok(Edit {
                            path: edit.path.clone(),
                            content,
                            before: edit.before.clone(),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                self.change(|state| {
                    let current = state
                        .queue
                        .iter_mut()
                        .find(|candidate| candidate.id == feature.id)
                        .context("feature missing")?;
                    current.edits = Some(refreshed.clone());
                    Ok(())
                })?;
                refreshed
            } else {
                edits.clone()
            }
        } else {
            self.feature(
                &feature.id,
                "running",
                None,
                if feature.repair_pending {
                    "Local model is preparing a targeted repair"
                } else {
                    "Local model is preparing changes"
                },
            )?;
            let files = project_context(&project)?;
            let repair_baseline: std::collections::HashMap<String, String> =
                if feature.repair_pending {
                    files
                        .iter()
                        .filter_map(|entry| {
                            Some((
                                entry["path"].as_str()?.to_lowercase(),
                                hash(entry["content"].as_str()?.as_bytes()),
                            ))
                        })
                        .collect()
                } else {
                    std::collections::HashMap::new()
                };
            let validation_paths = if feature.repair_pending {
                validation_path_references(&project, &feature.validation)?
            } else {
                Vec::new()
            };
            let protected_repair_inputs = if feature.repair_pending {
                repair_protected_inputs(&project, &validation_paths)?
            } else {
                std::collections::HashMap::new()
            };
            let system_prompt = if feature.repair_pending {
                "Repair the failed feature in this project. Preserve the original requested behavior. Existing tests and files named by the validation command are read-only repair inputs: do not output or modify them. Do not create files in conventional test/spec locations. Do not delete, weaken, skip, or rewrite tests merely to pass. Do not change the validation command. Fix the implementation and return ONLY a JSON object with a files array. Each item has path (relative path) and content (complete UTF-8 file contents). Include only new or changed implementation files. Do not delete files, modify .git, dependencies, or secrets. No markdown fences."
            } else {
                "Implement the requested feature in this project. Return ONLY a JSON object with a files array. Each item has path (relative path) and content (complete UTF-8 file contents). Include only new or changed files. Do not delete files, modify .git, dependencies, or secrets. Preserve existing behavior. Add meaningful tests. No markdown fences. The user supplies the validation command; implement code that really passes it."
            };
            let plan_context = approved_plan
                .as_deref()
                .unwrap_or("Legacy queue item: no brainstorming plan was recorded.");
            let user_prompt = if feature.repair_pending {
                let evidence = feature
                    .repair_history
                    .last()
                    .context("Reserved repair is missing its prior failure evidence")?;
                format!(
                    "Original feature: {}\nApproved immutable implementation plan:\n{}\nUnchanged validation command: {}\nRepair attempt: {} of {}\nPrior failure evidence: {}\nCurrent files: {}",
                    feature.instruction,
                    plan_context,
                    feature.validation,
                    feature.repair_attempts,
                    REPAIR_LIMIT,
                    serde_json::to_string(evidence)?,
                    serde_json::to_string(&files)?
                )
            } else {
                format!(
                    "Original feature: {}\nApproved immutable implementation plan:\n{}\nValidation command: {}\nExisting files: {}",
                    feature.instruction,
                    plan_context,
                    feature.validation,
                    serde_json::to_string(&files)?
                )
            };
            let target = self.model_target(&feature.model_target)?;
            if self.tools.available() {
                self.feature(
                    &feature.id,
                    "running",
                    None,
                    "Local model is working with project tools",
                )?;
                let outcome = self
                    .run_tool_feature_candidate(
                        feature,
                        &project,
                        plan_context,
                        &validation_paths,
                        &protected_repair_inputs,
                    )
                    .await?;
                let ToolFeatureOutcome {
                    edits: prepared,
                    application_edits,
                    workspace_revision,
                    applied_to_live_project,
                    model,
                } = outcome;
                if feature.repair_pending
                    && prepared.is_empty()
                    && feature
                        .repair_history
                        .last()
                        .is_none_or(|attempt| attempt.prior_edits.is_empty())
                {
                    bail!("Tool-assisted repair produced no reviewable implementation or environment change");
                }
                self.change(|state| {
                    let current = state
                        .queue
                        .iter_mut()
                        .find(|candidate| candidate.id == feature.id)
                        .context("feature missing")?;
                    current.edits = Some(prepared.clone());
                    current.tool_workspace_revision = workspace_revision;
                    current.checkpoint = if current.repair_pending {
                        format!("repair_{}_applied", current.repair_attempts)
                    } else {
                        "applied".into()
                    };
                    current.message = format!(
                        "Tool-assisted changes finished with {} model {}; running immutable validation",
                        model_target_name(target.id),
                        model
                    );
                    Ok(())
                })?;
                tool_session_applied = applied_to_live_project;
                tool_application_edits = Some(application_edits);
                prepared
            } else {
                let client = reqwest::Client::builder()
                    .timeout(Duration::from_secs(900))
                    .redirect(reqwest::redirect::Policy::none())
                    .no_proxy()
                    .build()?;
                let request = client
                    .post(format!(
                        "{}/chat/completions",
                        target.url.trim_end_matches('/')
                    ))
                    .json(&json!({
                        "model":target.model,"temperature":0.1,"max_tokens":8192,
                        "response_format":{"type":"json_object"},
                        "chat_template_kwargs":{"enable_thinking":false},
                        "messages":[{"role":"system","content":system_prompt},
                            {"role":"user","content":user_prompt}]
                    }))
                    .send();
                tokio::pin!(request);
                let response = loop {
                    tokio::select! {
                        response = &mut request => break response.with_context(|| format!("{} model target request failed", target.name))?,
                        _ = tokio::time::sleep(Duration::from_millis(100)) => if self.cancelled() { bail!("Stopped"); }
                    }
                };
                let status = response.status();
                if !status.is_success() {
                    bail!("{} model target returned HTTP {status}", target.name);
                }
                let body = response.json::<Value>();
                tokio::pin!(body);
                let payload = loop {
                    tokio::select! {
                        result = &mut body => break result?,
                        _ = tokio::time::sleep(Duration::from_millis(100)) => if self.cancelled() { bail!("Stopped"); }
                    }
                };
                let content = payload["choices"][0]["message"]["content"]
                    .as_str()
                    .context("Model returned no file changes")?;
                if content.len() > 1024 * 1024 {
                    bail!("Model change set exceeds 1 MiB");
                }
                let normalized = content
                    .trim()
                    .strip_prefix("```json")
                    .or_else(|| content.trim().strip_prefix("```"))
                    .and_then(|s| s.trim().strip_suffix("```"))
                    .unwrap_or(content)
                    .trim();
                let generated: Value = serde_json::from_str(normalized).with_context(|| format!(
                "Model did not return valid file JSON ({} bytes; finish reason {}); no files were changed", content.len(), payload["choices"][0]["finish_reason"]))?;
                let entries = generated["files"]
                    .as_array()
                    .context("Model response has no files array")?;
                if entries.is_empty() || entries.len() > 40 {
                    bail!("Expected 1 to 40 changed files");
                }
                if feature.repair_pending
                    && repair_protected_inputs(&project, &validation_paths)?
                        != protected_repair_inputs
                {
                    bail!(
                    "A protected test or validation input changed while the repair model was running; preserve it and retry"
                );
                }
                let mut edits = Vec::new();
                let mut seen = std::collections::HashSet::new();
                for entry in entries {
                    let path = entry["path"].as_str().context("Missing file path")?;
                    let content = entry["content"].as_str().context("Missing file content")?;
                    let full = checked_path(&project, path)?;
                    validate_repair_file_admission(path, content)?;
                    if !seen.insert(path.to_lowercase()) {
                        bail!("Duplicate output file");
                    }
                    if feature.repair_pending && is_protected_repair_input(path, &validation_paths)
                    {
                        match protected_repair_inputs.get(&path.to_lowercase()) {
                            Some(expected) if hash(content.as_bytes()) == *expected => continue,
                            Some(_) => bail!(
                                "Repair cannot modify existing test or validation input {}",
                                path
                            ),
                            None => bail!("Repair cannot create test or validation input {}", path),
                        }
                    }
                    let before = if feature.repair_pending {
                        if let Some(expected) = repair_baseline.get(&path.to_lowercase()) {
                            if !full.exists() || hash(&fs::read(&full)?) != *expected {
                                bail!(
                                "{} changed while the repair model was preparing changes; preserve it and retry",
                                path
                            );
                            }
                            Some(expected.clone())
                        } else if full.exists() {
                            bail!(
                                "{} was not in the repair baseline; preserve it and retry",
                                path
                            );
                        } else {
                            None
                        }
                    } else if full.exists() {
                        Some(hash(&fs::read(&full)?))
                    } else {
                        None
                    };
                    edits.push(Edit {
                        path: path.into(),
                        content: content.into(),
                        before,
                    });
                }
                if feature.repair_pending && edits.is_empty() {
                    bail!("Repair proposed no mutable implementation changes");
                }
                if self.cancelled() {
                    bail!("Stopped");
                }
                self.change(|s| {
                    let f = s
                        .queue
                        .iter_mut()
                        .find(|f| f.id == feature.id)
                        .context("feature missing")?;
                    f.edits = Some(edits.clone());
                    f.checkpoint = if f.repair_pending {
                        format!("repair_{}_prepared", f.repair_attempts)
                    } else {
                        "prepared".into()
                    };
                    Ok(())
                })?;
                edits
            }
        };
        if !tool_session_applied && !checkpoint_reuses_retained_edits(&feature.checkpoint) {
            self.feature(&feature.id, "running", None, "Applying saved file changes")?;
            let application_edits = if feature.escalation_pending {
                validate_current_review_edits(&edits, &project)?;
                feature
                    .escalation_proposal
                    .as_ref()
                    .context("Approved repair proposal evidence is missing")?
                    .files
                    .iter()
                    .map(|file| Edit {
                        path: file.path.clone(),
                        content: file.after.clone(),
                        before: file.before.as_ref().map(|before| hash(before.as_bytes())),
                    })
                    .collect::<Vec<_>>()
            } else {
                tool_application_edits
                    .clone()
                    .unwrap_or_else(|| edits.clone())
            };
            if !feature.escalation_pending
                && feature.repair_pending
                && feature.auto_repair_lifecycle == "running"
            {
                self.change(|state| {
                    let policy_enabled = state.auto_ai_repair_enabled;
                    let current = state
                        .queue
                        .iter_mut()
                        .find(|candidate| candidate.id == feature.id)
                        .context("feature missing")?;
                    begin_automatic_ordinary_repair_application(
                        current,
                        policy_enabled,
                        feature.auto_repair_epoch,
                        feature.repair_attempts,
                    )
                })?;
            }
            for edit in &application_edits {
                let effect_guard = self
                    .effect_gate
                    .lock()
                    .map_err(|_| anyhow!("effect gate failed"))?;
                if self.cancelled() {
                    bail!("Stopped");
                }
                if feature.auto_repair_lifecycle == "running" {
                    let database = self
                        .database
                        .lock()
                        .map_err(|_| anyhow!("state lock failed"))?;
                    let current = database
                        .state
                        .queue
                        .iter()
                        .find(|candidate| candidate.id == feature.id)
                        .context("feature missing")?;
                    let proposal_matches = if feature.escalation_pending {
                        let expected = feature
                            .escalation_proposal
                            .as_ref()
                            .context("Automatic escalation proposal binding is missing")?;
                        current
                            .escalation_proposal
                            .as_ref()
                            .is_some_and(|proposal| {
                                proposal.proposal_id == expected.proposal_id
                                    && proposal.automatic_epoch == Some(feature.auto_repair_epoch)
                                    && matches!(proposal.status.as_str(), "approved" | "applying")
                            })
                    } else {
                        current.repair_pending == feature.repair_pending
                            && current.repair_attempts == feature.repair_attempts
                    };
                    if !database.state.auto_ai_repair_enabled
                        || current.auto_repair_lifecycle != "running"
                        || current.auto_repair_epoch != feature.auto_repair_epoch
                        || current.auto_repair_policy_revision
                            != Some(database.state.auto_ai_repair_policy_revision)
                        || !proposal_matches
                    {
                        bail!("Automatic repair effect binding changed before file application");
                    }
                }
                apply_edit(&project, edit)?;
                if feature.escalation_pending {
                    self.change(|state| {
                        let current = state
                            .queue
                            .iter_mut()
                            .find(|candidate| candidate.id == feature.id)
                            .context("feature missing")?;
                        record_escalation_file_application(current, edit, state.revision + 1)
                    })?;
                }
                drop(effect_guard);
            }
            let applied_checkpoint = if feature.escalation_pending {
                let attempt = feature
                    .escalation_proposal
                    .as_ref()
                    .context("Approved repair proposal evidence is missing")?
                    .attempt;
                format!("escalation_{attempt}_applied")
            } else if feature.repair_pending {
                format!("repair_{}_applied", feature.repair_attempts)
            } else {
                "applied".into()
            };
            let cumulative_edits = self.change(|state| {
                let current = state
                    .queue
                    .iter_mut()
                    .find(|candidate| candidate.id == feature.id)
                    .context("feature missing")?;
                current.status = "running".into();
                current.checkpoint = applied_checkpoint.clone();
                current.message = "Files saved; running validation".into();
                if feature.escalation_pending {
                    let proposal = current
                        .escalation_proposal
                        .as_mut()
                        .context("Approved repair proposal evidence is missing")?;
                    if proposal.applied_paths.len() != proposal.files.len() {
                        bail!("Approved repair proposal application evidence is incomplete");
                    }
                    proposal.status = "applied".into();
                    proposal.binding_revision = state.revision + 1;
                }
                if automatic_escalation_is_running(current) {
                    start_auto_repair_step(current)?;
                }
                Ok(current.edits.clone().unwrap_or_default())
            })?;
            if feature.escalation_pending {
                edits = cumulative_edits;
            }
        }
        if self.cancelled() {
            bail!("Stopped");
        }
        let validation_evidence_sha256 = self.validate_command(feature, &project).await?;
        self.review_feature(feature, &project, &edits, &validation_evidence_sha256)
            .await
    }
    async fn review_feature(
        &self,
        feature: &Feature,
        project: &Path,
        edits: &[Edit],
        validation_evidence_sha256: &str,
    ) -> Result<()> {
        let durable_feature = self
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?
            .state
            .queue
            .iter()
            .find(|candidate| candidate.id == feature.id)
            .cloned()
            .context("feature missing")?;
        let packet =
            developer_review_packet(&durable_feature, project, edits, validation_evidence_sha256)?;
        let packet_sha256 = packet.sha256()?;
        let pending = self.change(|state| {
            let policy_enabled = state.auto_ai_repair_enabled;
            let current = state
                .queue
                .iter_mut()
                .find(|candidate| candidate.id == feature.id)
                .context("feature missing")?;
            if feature.auto_repair_lifecycle == "running"
                && (current.auto_repair_epoch != feature.auto_repair_epoch
                    || current.auto_repair_lifecycle != "running"
                    || !policy_enabled)
            {
                bail!("Automatic repair review was cancelled by a newer policy epoch");
            }
            if let Some(pending) = &current.review_pending {
                if pending.packet_sha256 != packet_sha256
                    || pending.validation_evidence_sha256 != validation_evidence_sha256
                {
                    bail!("Saved review binding no longer matches the validated generated files");
                }
                current.status = "running".into();
                current.review_status = "reviewing".into();
                current.review_summary =
                    "ChatGPT Codex is reviewing the exact validated generated files".into();
                return Ok(pending.clone());
            }
            if current.review_history.len() >= REVIEW_HISTORY_LIMIT {
                if current.auto_repair_lifecycle == "running" {
                    set_auto_repair_lifecycle(
                        current,
                        "held",
                        "Automatic repair stopped because the bounded independent-review history is full",
                    )?;
                }
                bail!("Independent review history limit reached");
            }
            let attempt = current
                .review_attempts
                .checked_add(1)
                .context("Review attempt count overflow")?;
            let pending = ReviewPendingEvidence {
                attempt,
                packet_sha256: packet_sha256.clone(),
                validation_evidence_sha256: validation_evidence_sha256.into(),
            };
            current.review_attempts = attempt;
            current.review_pending = Some(pending.clone());
            current.review_status = "reviewing".into();
            current.review_summary =
                "ChatGPT Codex is reviewing the exact validated generated files".into();
            current.checkpoint = format!("review_{attempt}_pending");
            current.message = current.review_summary.clone();
            if current.auto_repair_lifecycle == "running" {
                start_auto_repair_step(current)?;
            }
            Ok(pending)
        })?;

        let decision = match self.reviewer.review(&packet, &self.cancellation).await {
            Ok(decision) => decision,
            Err(DeveloperReviewCallError::Cancelled) => {
                let summary = "ChatGPT Codex review was interrupted. Resume revalidates the generated files and starts a fresh review attempt.";
                self.change(|state| {
                    let current = state
                        .queue
                        .iter_mut()
                        .find(|candidate| candidate.id == feature.id)
                        .context("feature missing")?;
                    interrupt_pending_review(current, summary)?;
                    Ok(())
                })?;
                bail!("Cloud review was stopped")
            }
            Err(DeveloperReviewCallError::Unavailable(error)) => {
                let summary = format!(
                    "ChatGPT Codex review is unavailable; Resume retries review without using a local repair attempt. {error}"
                );
                self.change(|state| {
                    let current = state
                        .queue
                        .iter_mut()
                        .find(|candidate| candidate.id == feature.id)
                        .context("feature missing")?;
                    if current
                        .review_pending
                        .as_ref()
                        .map(|value| value.packet_sha256.as_str())
                        != Some(packet_sha256.as_str())
                    {
                        bail!("Review binding changed before unavailable outcome was recorded");
                    }
                    current.review_history.push(ReviewAttemptEvidence {
                        attempt: pending.attempt,
                        packet_sha256: packet_sha256.clone(),
                        validation_evidence_sha256: validation_evidence_sha256.into(),
                        outcome: "unavailable".into(),
                        decision_sha256: None,
                        blocking_findings: Vec::new(),
                        summary: summary.chars().take(1000).collect(),
                    });
                    current.review_pending = None;
                    current.review_status = "unavailable".into();
                    current.review_summary = summary.chars().take(1000).collect();
                    current.checkpoint = format!("review_{}_unavailable", pending.attempt);
                    Ok(())
                })?;
                bail!(summary)
            }
        };

        let latest_feature = self
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?
            .state
            .queue
            .iter()
            .find(|candidate| candidate.id == feature.id)
            .cloned()
            .context("feature missing")?;
        let rebound =
            developer_review_packet(&latest_feature, project, edits, validation_evidence_sha256);
        let rebound = match rebound {
            Ok(rebound) if rebound.sha256()? == packet_sha256 => rebound,
            Ok(_) | Err(_) => {
                let summary = "Generated files changed while ChatGPT Codex was reviewing. Resume revalidates the current bytes and starts a fresh review.";
                self.change(|state| {
                    let binding_revision = state.revision + 1;
                    let current = state
                        .queue
                        .iter_mut()
                        .find(|candidate| candidate.id == feature.id)
                        .context("feature missing")?;
                    if current.auto_repair_lifecycle == "running" {
                        quarantine_automatic_post_apply(current, binding_revision, summary)?;
                    } else {
                        interrupt_pending_review(current, summary)?;
                    }
                    Ok(())
                })?;
                bail!(summary)
            }
        };
        if let Err(error) = decision.validate_exact(&rebound) {
            let summary = "The Codex review receipt did not match the exact trusted packet. Automatic repair stopped with applied bytes retained; prepare a fresh owner-approved proposal after inspection.";
            self.change(|state| {
                let binding_revision = state.revision + 1;
                let current = state
                    .queue
                    .iter_mut()
                    .find(|candidate| candidate.id == feature.id)
                    .context("feature missing")?;
                if current.auto_repair_lifecycle == "running" {
                    quarantine_automatic_post_apply(current, binding_revision, summary)?;
                } else if current.review_pending.is_some() {
                    interrupt_pending_review(current, summary)?;
                }
                Ok(())
            })?;
            return Err(error.context(summary));
        }
        let decision_sha256 = decision.sha256()?;
        let approved = decision.decision == DeveloperReviewDecisionKind::Approved;
        let summary = review_summary(&decision);
        let decision_recorded = self.change(|state| {
            let policy_enabled = state.auto_ai_repair_enabled;
            let current = state
                .queue
                .iter_mut()
                .find(|candidate| candidate.id == feature.id)
                .context("feature missing")?;
            if current
                .review_pending
                .as_ref()
                .map(|value| value.packet_sha256.as_str())
                != Some(packet_sha256.as_str())
            {
                bail!("Review binding changed before decision was recorded");
            }
            if feature.auto_repair_lifecycle == "running"
                && (current.auto_repair_epoch != feature.auto_repair_epoch
                    || current.auto_repair_lifecycle != "running"
                    || !policy_enabled)
            {
                interrupt_pending_review(
                    current,
                    "A newer Auto AI repair policy epoch cancelled this review before its decision became durable.",
                )?;
                return Ok(false);
            }
            if self.cancelled() {
                interrupt_pending_review(
                    current,
                    "ChatGPT Codex review was interrupted before its decision became durable. Resume revalidates the generated files and starts a fresh review attempt.",
                )?;
                return Ok(false);
            }
            current.review_history.push(ReviewAttemptEvidence {
                attempt: pending.attempt,
                packet_sha256: packet_sha256.clone(),
                validation_evidence_sha256: validation_evidence_sha256.into(),
                outcome: if approved { "approved" } else { "rejected" }.into(),
                decision_sha256: Some(decision_sha256.clone()),
                blocking_findings: decision.blocking_findings.clone(),
                summary: summary.clone(),
            });
            current.review_pending = None;
            current.review_status = if approved { "approved" } else { "rejected" }.into();
            current.review_summary = summary.clone();
            current.checkpoint = format!(
                "review_{}_{}",
                pending.attempt,
                if approved { "approved" } else { "rejected" }
            );
            current.message = summary.clone();
            Ok(true)
        })?;
        if !decision_recorded {
            bail!("Cloud review was stopped");
        }
        if approved {
            Ok(())
        } else {
            Err(RepairableReviewRejection(summary).into())
        }
    }

    async fn validate_command(&self, feature: &Feature, project: &Path) -> Result<String> {
        let result = self.validate_command_inner(feature, project).await;
        self.fail_closed_validation_result(&feature.id, None, result)
    }

    async fn validate_command_with_candidate_evidence(
        &self,
        feature: &Feature,
        project: &Path,
        workspace_revision: u64,
        live_edits: &[Edit],
    ) -> Result<String> {
        let result = self.validate_command_inner(feature, project).await;
        self.fail_closed_validation_result(
            &feature.id,
            Some((workspace_revision, live_edits)),
            result,
        )
    }

    fn fail_closed_validation_result<T>(
        &self,
        feature_id: &str,
        candidate_evidence: Option<(u64, &[Edit])>,
        result: Result<T>,
    ) -> Result<T> {
        match result {
            Err(error) if error.downcast_ref::<UnconfirmedTermination>().is_some() => {
                if let Err(persistence_error) =
                    self.quarantine_unconfirmed_validation_cleanup(feature_id, candidate_evidence)
                {
                    return Err(error.context(format!(
                        "Emergency Pause persistence also failed: {persistence_error:#}"
                    )));
                }
                Err(error)
            }
            result => result,
        }
    }

    async fn validate_command_inner(&self, feature: &Feature, project: &Path) -> Result<String> {
        let log_path = self.data.join(format!("{}.log", feature.id));
        let log = fs::File::create(&log_path)?;
        let limit_unadmitted_binding = if feature.checkpoint == "auto_repair_limit_revalidating" {
            let current =
                admitted_project_snapshot_with_cancellation(project, Some(&self.cancellation))?;
            let expected = feature
                .auto_repair_limit_unadmitted_sha256
                .as_deref()
                .context("Automatic repair limit has no unadmitted-data binding")?;
            if current.unadmitted_sha256 != expected {
                bail!("Unadmitted project data changed before limit-recovery validation");
            }
            let expected_volatile = feature
                .auto_repair_limit_volatile_sha256
                .as_deref()
                .context("Automatic repair limit has no volatile-data binding")?;
            if current.volatile_sha256 != expected_volatile {
                bail!(
                    "Build, cache, or dependency output changed before limit-recovery validation"
                );
            }
            Some((current.unadmitted_sha256, current.volatile_sha256))
        } else {
            None
        };
        let mut environment = developer_process::ValidationEnvironment::capture()?;
        configure_project_environment(&mut environment, project)?;
        let spawn_guard = self
            .effect_gate
            .lock()
            .map_err(|_| anyhow!("effect gate failed"))?;
        if self.cancelled() {
            bail!("Validation was cancelled before process creation");
        }
        {
            let database = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            if database.state.emergency_paused {
                bail!("Emergency Pause blocks validation process creation");
            }
            if feature.auto_repair_lifecycle == "running" {
                let current = database
                    .state
                    .queue
                    .iter()
                    .find(|candidate| candidate.id == feature.id)
                    .context("feature missing")?;
                if !database.state.auto_ai_repair_enabled
                    || current.auto_repair_lifecycle != "running"
                    || current.auto_repair_epoch != feature.auto_repair_epoch
                    || current.auto_repair_policy_revision
                        != Some(database.state.auto_ai_repair_policy_revision)
                {
                    bail!("Automatic repair validation binding changed before process creation");
                }
            } else if limit_unadmitted_binding.is_some() {
                let current = database
                    .state
                    .queue
                    .iter()
                    .find(|candidate| candidate.id == feature.id)
                    .context("feature missing")?;
                if current.auto_repair_lifecycle != "limit_reached"
                    || current.checkpoint != "auto_repair_limit_revalidating"
                    || current.auto_repair_limit_project_baseline
                        != feature.auto_repair_limit_project_baseline
                    || current.auto_repair_limit_unadmitted_sha256
                        != feature.auto_repair_limit_unadmitted_sha256
                    || current.auto_repair_limit_volatile_sha256
                        != feature.auto_repair_limit_volatile_sha256
                {
                    bail!("Limit-recovery snapshot binding changed before process creation");
                }
            }
        }
        let mut child = developer_process::spawn(&feature.validation, project, log, &environment)
            .map_err(|error| {
            if error.is::<developer_process::CleanupUnconfirmed>() {
                anyhow!(UnconfirmedTermination(format!("{error:#}")))
            } else {
                error
            }
        })?;
        drop(spawn_guard);
        let started = std::time::Instant::now();
        loop {
            let log_size = match fs::metadata(&log_path) {
                Ok(metadata) => metadata.len(),
                Err(error) => {
                    child
                        .terminate()
                        .await
                        .map_err(|cleanup| UnconfirmedTermination(format!("{cleanup:#}")))?;
                    return Err(error).context("read validation log size after confirmed cleanup");
                }
            };
            if self.cancelled()
                || started.elapsed() > Duration::from_secs(900)
                || log_size > 2 * 1024 * 1024
            {
                child
                    .terminate()
                    .await
                    .map_err(|error| UnconfirmedTermination(format!("{error:#}")))?;
                bail!("Validation stopped or exceeded its time/output limit");
            }
            let observed = match child.try_wait() {
                Ok(status) => status,
                Err(error) => {
                    child
                        .terminate()
                        .await
                        .map_err(|cleanup| UnconfirmedTermination(format!("{cleanup:#}")))?;
                    return Err(error).context("poll validation process after confirmed cleanup");
                }
            };
            if let Some(status) = observed {
                child
                    .terminate()
                    .await
                    .map_err(|error| UnconfirmedTermination(format!("{error:#}")))?;
                let terminal_limit_binding = if limit_unadmitted_binding.is_some() {
                    Some(self.refresh_limit_recovery_volatile_binding(feature, project)?)
                } else {
                    None
                };
                if status.success() {
                    let mut evidence = Sha256::new();
                    evidence.update(b"assemblywright.developer-validation.v1\0");
                    evidence.update(feature.id.as_bytes());
                    evidence.update(feature.validation.as_bytes());
                    if let Some((unadmitted_sha256, _)) = &limit_unadmitted_binding {
                        evidence.update(b"\0limit_unadmitted_sha256=");
                        evidence.update(unadmitted_sha256.as_bytes());
                    }
                    if let Some(volatile_sha256) = &terminal_limit_binding {
                        evidence.update(b"\0limit_volatile_sha256=");
                        evidence.update(volatile_sha256.as_bytes());
                    }
                    evidence.update(b"exit_status=0");
                    return Ok(format!("{:x}", evidence.finalize()));
                }
                let bytes = fs::read(&log_path)?;
                let tail = String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(3500)..]);
                return Err(RepairableValidationFailure(format!(
                    "Validation failed ({status}):\n{tail}"
                ))
                .into());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    fn refresh_limit_recovery_volatile_binding(
        &self,
        feature: &Feature,
        project: &Path,
    ) -> Result<String> {
        let snapshot =
            admitted_project_snapshot_with_cancellation(project, Some(&self.cancellation))
                .context("Limit-recovery validation left an unreviewable project state")?;
        let mut post_validation = feature.clone();
        post_validation.auto_repair_limit_volatile_sha256 = Some(snapshot.volatile_sha256.clone());
        let reconciled = reconcile_limit_recovery_edits(&post_validation, &snapshot)?;
        if hash(&serde_json::to_vec(&reconciled)?)
            != hash(&serde_json::to_vec(
                feature.edits.as_deref().unwrap_or_default(),
            )?)
        {
            bail!(
                "Validation changed admitted project data outside the owner-approved recovery set"
            );
        }
        let volatile_sha256 = snapshot.volatile_sha256.clone();
        self.change(|state| {
            let current = state
                .queue
                .iter_mut()
                .find(|candidate| candidate.id == feature.id)
                .context("feature missing")?;
            if current.auto_repair_lifecycle != "limit_reached"
                || current.checkpoint != "auto_repair_limit_revalidating"
                || current.auto_repair_limit_project_baseline
                    != feature.auto_repair_limit_project_baseline
                || current.auto_repair_limit_unadmitted_sha256
                    != feature.auto_repair_limit_unadmitted_sha256
                || current.auto_repair_limit_volatile_sha256
                    != feature.auto_repair_limit_volatile_sha256
                || hash(&serde_json::to_vec(
                    current.edits.as_deref().unwrap_or_default(),
                )?) != hash(&serde_json::to_vec(
                    feature.edits.as_deref().unwrap_or_default(),
                )?)
            {
                bail!("Limit-recovery binding changed after validation");
            }
            current.auto_repair_limit_volatile_sha256 = Some(volatile_sha256.clone());
            Ok(())
        })?;
        Ok(volatile_sha256)
    }
}

fn configure_project_environment(
    process_environment: &mut developer_process::ValidationEnvironment,
    project: &Path,
) -> Result<()> {
    let virtual_environment = project.join(".venv");
    if !virtual_environment.exists() {
        return Ok(());
    }
    let environment_metadata = fs::symlink_metadata(&virtual_environment)?;
    if environment_metadata.file_type().is_symlink() || !environment_metadata.is_dir() {
        bail!("Project .venv must be an ordinary directory");
    }
    let virtual_environment = fs::canonicalize(virtual_environment)?;
    let expected_environment = project.join(".venv");
    if virtual_environment != expected_environment || !virtual_environment.starts_with(project) {
        bail!("Project .venv leaves or redirects within the project");
    }
    #[cfg(windows)]
    let bin = virtual_environment.join("Scripts");
    #[cfg(not(windows))]
    let bin = virtual_environment.join("bin");
    let bin_metadata =
        fs::symlink_metadata(&bin).context("Project .venv has no interpreter bin")?;
    if bin_metadata.file_type().is_symlink() || !bin_metadata.is_dir() {
        bail!("Project .venv interpreter bin must be an ordinary directory");
    }
    let bin = fs::canonicalize(bin)?;
    if !bin.starts_with(&virtual_environment) {
        bail!("Project .venv interpreter bin leaves the environment");
    }
    #[cfg(windows)]
    let interpreter = bin.join("python.exe");
    #[cfg(not(windows))]
    let interpreter = bin.join("python");
    let interpreter_metadata =
        fs::metadata(&interpreter).context("Project .venv has no usable Python interpreter")?;
    if !interpreter_metadata.is_file() {
        bail!("Project .venv Python interpreter must resolve to a file");
    }
    let mut paths = vec![bin];
    if let Some(existing) = process_environment.get(std::ffi::OsStr::new("PATH")) {
        paths.extend(std::env::split_paths(&existing));
    }
    process_environment.set("PATH", std::env::join_paths(paths)?)?;
    process_environment.set("VIRTUAL_ENV", virtual_environment)?;
    Ok(())
}

fn require_project_virtual_environment(project: &Path) -> Result<()> {
    if !project.join(".venv").exists() {
        bail!("Dependency preparation did not create the required project-local .venv");
    }
    configure_project_environment(
        &mut developer_process::ValidationEnvironment::capture()?,
        project,
    )
}

struct RepairStage {
    root: PathBuf,
    project_name: String,
}

impl RepairStage {
    fn create(workspace_root: &Path, source: &Path) -> Result<Self> {
        if !source.starts_with(workspace_root) {
            bail!("Repair staging source leaves the workspace root");
        }
        let project_name = format!("aw-repair-stage-{}", Uuid::new_v4().simple());
        let root = workspace_root.join(&project_name);
        fs::create_dir(&root)?;
        let stage = Self { root, project_name };
        copy_repair_stage(source, &stage.root)?;
        Ok(stage)
    }

    fn project_path(&self) -> &Path {
        &self.root
    }
}

impl Drop for RepairStage {
    fn drop(&mut self) {
        if self.project_name.starts_with("aw-repair-stage-")
            && self.root.file_name().and_then(|name| name.to_str())
                == Some(self.project_name.as_str())
        {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

fn copy_repair_stage(source: &Path, destination: &Path) -> Result<()> {
    copy_repair_stage_with_directory_opened(source, destination, None)
}

fn copy_repair_stage_with_directory_opened(
    source: &Path,
    destination: &Path,
    directory_opened: Option<&dyn Fn(&Path)>,
) -> Result<()> {
    #[cfg(unix)]
    fn visit_unix(
        source: &fs::File,
        relative_dir: &Path,
        destination: &Path,
        file_count: &mut usize,
        byte_count: &mut u64,
        directory_opened: Option<&dyn Fn(&Path)>,
        depth: usize,
    ) -> Result<()> {
        if depth > 20 {
            bail!("Repair staging tree exceeds its depth limit");
        }
        if let Some(hook) = directory_opened {
            hook(relative_dir);
        }
        for entry in unix_recovery_entries(source)? {
            let name = entry.name.to_string_lossy().into_owned();
            if [
                ".git",
                ".venv",
                "venv",
                "target",
                "node_modules",
                "__pycache__",
                "dist",
            ]
            .contains(&name.as_str())
            {
                continue;
            }
            let relative = relative_dir.join(&entry.name);
            match unix_recovery_entry_kind(&entry) {
                libc::S_IFLNK => continue,
                libc::S_IFDIR => {
                    let target = destination.join(&relative);
                    fs::create_dir(&target)?;
                    let (child, _) = open_unix_recovery_entry(source, &entry, true)?;
                    visit_unix(
                        &child,
                        &relative,
                        destination,
                        file_count,
                        byte_count,
                        directory_opened,
                        depth + 1,
                    )?;
                }
                libc::S_IFREG => {
                    *file_count = file_count
                        .checked_add(1)
                        .context("Repair staging count overflow")?;
                    let (mut input, metadata) = open_unix_recovery_entry(source, &entry, false)?;
                    *byte_count = byte_count
                        .checked_add(metadata.len())
                        .context("Repair staging size overflow")?;
                    if *file_count > 10_000 || *byte_count > 256 * 1024 * 1024 {
                        bail!("Repair staging project exceeds its bounded copy limit");
                    }
                    let target = destination.join(&relative);
                    let mut output = fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(target)?;
                    let copied = std::io::copy(&mut input, &mut output)?;
                    let final_metadata = input.metadata()?;
                    if copied != metadata.len()
                        || final_metadata.len() != metadata.len()
                        || !final_metadata.is_file()
                    {
                        bail!("Repair staging source changed while copying");
                    }
                    output.sync_all()?;
                }
                _ => continue,
            }
        }
        Ok(())
    }

    #[cfg(windows)]
    fn visit(
        source: &Path,
        relative_dir: &Path,
        destination: &Path,
        file_count: &mut usize,
        byte_count: &mut u64,
        directory_opened: Option<&dyn Fn(&Path)>,
        depth: usize,
    ) -> Result<()> {
        if depth > 20 {
            bail!("Repair staging tree exceeds its depth limit");
        }
        let _directory_guard = hold_windows_recovery_directory(source)?;
        if let Some(hook) = directory_opened {
            hook(relative_dir);
        }
        let mut entries = fs::read_dir(source)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            if [
                ".git",
                ".venv",
                "venv",
                "target",
                "node_modules",
                "__pycache__",
                "dist",
            ]
            .contains(&name.as_str())
            {
                continue;
            }
            let direct_metadata = fs::symlink_metadata(entry.path())?;
            if planning_metadata_is_reparse(&direct_metadata) {
                bail!("Repair staging refuses a Windows reparse point");
            }
            let file_type = direct_metadata.file_type();
            if file_type.is_symlink() {
                continue;
            }
            let relative = relative_dir.join(entry.file_name());
            let target = destination.join(relative);
            if file_type.is_dir() {
                fs::create_dir(&target)?;
                visit(
                    &entry.path(),
                    &relative_dir.join(entry.file_name()),
                    destination,
                    file_count,
                    byte_count,
                    directory_opened,
                    depth + 1,
                )?;
            } else if file_type.is_file() {
                let (mut input, metadata) = open_windows_recovery_file(&entry.path())?;
                *file_count = file_count
                    .checked_add(1)
                    .context("Repair staging count overflow")?;
                *byte_count = byte_count
                    .checked_add(metadata.len())
                    .context("Repair staging size overflow")?;
                if *file_count > 10_000 || *byte_count > 256 * 1024 * 1024 {
                    bail!("Repair staging project exceeds its bounded copy limit");
                }
                let mut output = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(target)?;
                let copied = std::io::copy(&mut input, &mut output)?;
                let final_metadata = input.metadata()?;
                if copied != metadata.len()
                    || final_metadata.len() != metadata.len()
                    || !final_metadata.is_file()
                    || planning_metadata_is_reparse(&final_metadata)
                {
                    bail!("Repair staging source changed while copying");
                }
                output.sync_all()?;
            }
        }
        Ok(())
    }

    #[cfg(unix)]
    {
        let source = hold_unix_recovery_root(source)?;
        visit_unix(
            &source,
            Path::new(""),
            destination,
            &mut 0,
            &mut 0,
            directory_opened,
            0,
        )
    }
    #[cfg(windows)]
    {
        visit(
            source,
            Path::new(""),
            destination,
            &mut 0,
            &mut 0,
            directory_opened,
            0,
        )
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (source, destination, directory_opened);
        bail!("Repair staging is unsupported on this host")
    }
}

fn reserve_repair_attempt(feature: &mut Feature) -> Result<()> {
    if feature.repair_attempts >= REPAIR_LIMIT {
        bail!("Repair attempt limit reached");
    }
    if feature.repair_history.len() != feature.repair_attempts as usize {
        bail!("Repair history is inconsistent");
    }
    let attempt = feature.repair_attempts + 1;
    let prior_edits = feature
        .edits
        .as_ref()
        .into_iter()
        .flatten()
        .take(40)
        .map(|edit| RepairEditEvidence {
            path: edit.path.clone(),
            before: edit.before.clone(),
            content_hash: hash(edit.content.as_bytes()),
        })
        .collect();
    let evidence = RepairAttemptEvidence {
        attempt,
        prior_checkpoint: feature.checkpoint.chars().take(128).collect(),
        prior_message: feature.message.chars().take(3500).collect(),
        prior_edits,
    };
    feature.repair_history.push(evidence);
    feature.repair_attempts = attempt;
    feature.repair_pending = true;
    feature.last_failure_kind.clear();
    feature.review_status = "pending".into();
    feature.review_summary = "Required ChatGPT Codex review has not started".into();
    feature.review_pending = None;
    feature.status = "queued".into();
    feature.checkpoint = format!("repair_{attempt}_reserved");
    feature.message =
        format!("Repair attempt {attempt} of {REPAIR_LIMIT} reserved by owner control");
    feature.edits = None;
    if feature.auto_repair_lifecycle == "running" {
        start_auto_repair_step(feature)?;
    }
    Ok(())
}

fn begin_automatic_ordinary_repair_application(
    feature: &mut Feature,
    policy_enabled: bool,
    observed_epoch: u64,
    observed_attempt: u32,
) -> Result<()> {
    if !policy_enabled
        || feature.auto_repair_lifecycle != "running"
        || feature.auto_repair_epoch != observed_epoch
        || !feature.repair_pending
        || feature.repair_attempts != observed_attempt
    {
        bail!("Automatic ordinary-repair application was cancelled by a newer policy epoch");
    }
    feature.checkpoint = format!("repair_{}_applying", feature.repair_attempts);
    feature.message = format!(
        "Automatic repair {} application started at policy epoch {}; interruption now requires quarantine",
        feature.repair_attempts, feature.auto_repair_epoch
    );
    start_auto_repair_step(feature)?;
    Ok(())
}

fn tool_feature_prompt(feature: &Feature, approved_plan: &str) -> Result<String> {
    let repair = if feature.repair_pending {
        let evidence = feature
            .repair_history
            .last()
            .context("Reserved tool-assisted repair has no failure evidence")?;
        format!(
            "This is repair attempt {} of {}. Preserve existing tests and every file or directory referenced by the validation command. Do not create or modify conventional test/spec files. Prior failure evidence: {}",
            feature.repair_attempts,
            REPAIR_LIMIT,
            serde_json::to_string(evidence)?
        )
    } else {
        "Implement the feature and add meaningful tests when needed.".into()
    };
    Ok(format!(
        "Implement this approved developer-app feature directly in the selected Windows project using the available project tools. You may inspect and edit project files, run bounded commands, access public internet resources, and install declared dependencies into a project-local environment when required. Never change Assemblywright queue state, approval records, or the immutable validation command. Never weaken, delete, skip, or broadly rewrite tests to make validation pass. Prefer the interpreter or package manager used by the immutable validation command. Run that exact command before finishing when possible; Assemblywright will run it independently and require Codex review before success. Report the files, commands, dependency changes, web resources, and actual outcomes plainly.\n\nOriginal feature: {}\nApproved immutable implementation plan:\n{}\nImmutable validation command: {}\n{}",
        feature.instruction, approved_plan, feature.validation, repair
    ))
}

fn repair_needs_environment_preparation(feature: &Feature) -> bool {
    let evidence = feature
        .repair_history
        .last()
        .map(|attempt| attempt.prior_message.to_ascii_lowercase())
        .unwrap_or_default();
    [
        "modulenotfounderror",
        "no module named",
        "importerror",
        "cannot find module",
        "command not found",
        "is not recognized as an internal or external command",
        "missing dependency",
    ]
    .iter()
    .any(|marker| evidence.contains(marker))
}

fn tool_environment_prompt(feature: &Feature) -> Result<String> {
    let evidence = feature
        .repair_history
        .last()
        .context("Environment preparation requires prior failure evidence")?;
    Ok(format!(
        "Prepare only this selected Windows project's dependency environment for the immutable validation command. Diagnose the missing command, module, or declared package from the failure evidence. You may access public package sources, create or update the project-local .venv, install the needed dependency there, and minimally update an existing dependency manifest or lock file. The finished project must contain a usable .venv. After creating it, invoke Python package installation explicitly through `.venv\\Scripts\\python.exe -m pip` on Windows or `.venv/bin/python -m pip` on Unix; never use bare `pip`, `pip3`, or a global Python package install. Do not edit implementation source, tests, validation inputs, or the validation command. Use the same interpreter family named by the validation command and run the exact validation command after preparation. Report the dependency, source, command, and actual outcome.\n\nImmutable validation command: {}\nPrior failure evidence: {}",
        feature.validation,
        serde_json::to_string(evidence)?
    ))
}

fn is_dependency_manifest(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    let name = normalized.rsplit('/').next().unwrap_or_default();
    name == "pyproject.toml"
        || name == "poetry.lock"
        || name == "uv.lock"
        || name == "pipfile"
        || name == "pipfile.lock"
        || name == "package.json"
        || name == "package-lock.json"
        || name == "pnpm-lock.yaml"
        || name == "yarn.lock"
        || name == "cargo.toml"
        || name == "cargo.lock"
        || name.starts_with("requirements") && name.ends_with(".txt")
}

fn is_project_configuration_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    let name = normalized.rsplit('/').next().unwrap_or_default();
    is_dependency_manifest(&normalized)
        || matches!(
            name,
            "build.rs"
                | "setup.py"
                | "makefile"
                | "cmakelists.txt"
                | "meson.build"
                | "build.gradle"
                | "build.gradle.kts"
                | "settings.gradle"
                | "settings.gradle.kts"
                | "gradle.properties"
                | "tsconfig.json"
                | "pytest.ini"
                | "setup.cfg"
                | "tox.ini"
                | "vite.config.js"
                | "vite.config.ts"
        )
        || name.starts_with("jest.config.")
        || name.starts_with("webpack.config.")
        || name.ends_with(".config.js")
        || name.ends_with(".config.ts")
        || name.ends_with(".config.mjs")
        || name.ends_with(".config.cjs")
        || matches!(
            Path::new(name).extension().and_then(|value| value.to_str()),
            Some("sh" | "bash" | "zsh" | "fish" | "ps1" | "bat" | "cmd")
        )
}

fn is_known_ordinary_source_path(path: &str) -> bool {
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    let name = normalized.rsplit('/').next().unwrap_or_default();
    matches!(
        Path::new(name).extension().and_then(|value| value.to_str()),
        Some(
            "rs" | "swift"
                | "py"
                | "pyi"
                | "js"
                | "jsx"
                | "mjs"
                | "cjs"
                | "ts"
                | "tsx"
                | "java"
                | "kt"
                | "kts"
                | "go"
                | "c"
                | "h"
                | "cc"
                | "cpp"
                | "cxx"
                | "hh"
                | "hpp"
                | "cs"
                | "rb"
                | "php"
                | "scala"
                | "vue"
                | "svelte"
                | "html"
                | "css"
                | "scss"
                | "sql"
        )
    )
}

fn trusted_review_file_classification(path: &str, validation_paths: &[String]) -> String {
    let classification = if is_protected_repair_input(path, validation_paths) {
        "test_or_validation_input"
    } else if is_project_configuration_path(path) || !is_known_ordinary_source_path(path) {
        "project_configuration"
    } else {
        "ordinary_source"
    };
    classification.into()
}

fn migrate_legacy_publication_classifications(
    feature: &mut Feature,
    project: &Path,
    loaded_queue_version: u8,
) -> Result<()> {
    if loaded_queue_version >= 11
        || !feature
            .publication_candidate
            .iter()
            .any(|file| file.classification == "unclassified_legacy")
    {
        return Ok(());
    }
    let validation_paths = validation_path_references(project, &feature.validation)?;
    for file in &mut feature.publication_candidate {
        if file.classification == "unclassified_legacy" {
            file.classification = trusted_review_file_classification(&file.path, &validation_paths);
        }
    }
    Ok(())
}

fn interrupt_pending_review(feature: &mut Feature, summary: &str) -> Result<()> {
    let ReviewPendingEvidence {
        attempt,
        packet_sha256,
        validation_evidence_sha256,
    } = feature
        .review_pending
        .take()
        .context("No pending review to interrupt")?;
    feature.review_history.push(ReviewAttemptEvidence {
        attempt,
        packet_sha256,
        validation_evidence_sha256,
        outcome: "interrupted".into(),
        decision_sha256: None,
        blocking_findings: Vec::new(),
        summary: summary.chars().take(1000).collect(),
    });
    feature.review_status = "interrupted".into();
    feature.review_summary = summary.chars().take(1000).collect();
    feature.checkpoint = format!("review_{attempt}_interrupted");
    Ok(())
}

fn feature_reviewer_state_is_changeable(feature: &Feature) -> bool {
    matches!(feature.status.as_str(), "queued" | "paused" | "failed")
        && !feature.publication.as_ref().is_some_and(|publication| {
            matches!(
                publication.status.as_str(),
                "pending" | "running" | "attention"
            )
        })
        && !feature.escalation_pending
        && feature_reviewer_recovery_is_safe(&feature.checkpoint)
        && !feature
            .escalation_proposal
            .as_ref()
            .is_some_and(|proposal| {
                matches!(
                    proposal.status.as_str(),
                    "preparing" | "approved" | "applying"
                )
            })
}

fn freeze_publication_selection(
    feature: &mut Feature,
    binding: Option<ProjectBinding>,
) -> Result<()> {
    if feature.publication_selection_frozen {
        bail!("Feature publication selection is already frozen");
    }
    if let Some(binding) = &binding {
        validate_publication_binding(binding)?;
        if binding.project != feature.project {
            bail!("GitHub connection does not match the feature project");
        }
    }
    feature.publication_selection_frozen = true;
    feature.publication_binding = binding;
    Ok(())
}

fn publication_completion_is_cancelled(cancellation: &AtomicU8, emergency_paused: bool) -> bool {
    emergency_paused || cancellation.load(Ordering::SeqCst) != 0
}

fn pause_after_post_run_cancellation(feature: &mut Feature) {
    if feature.status == "succeeded" {
        return;
    }
    feature.status = "paused".into();
    feature.message =
        "Stopped. Resume continues from the saved checkpoint; applied files are retained.".into();
}

fn feature_reviewer_recovery_is_safe(checkpoint: &str) -> bool {
    if matches!(
        checkpoint,
        "not_started"
            | "prepared"
            | "applied"
            | "validated"
            | "validation_failed"
            | "review_binding_changed"
            | "review_completion_interrupted"
            | "review_tool_workspace_changed"
    ) {
        return true;
    }
    let indexed = |prefix: &str, allowed: &[&str]| {
        checkpoint
            .strip_prefix(prefix)
            .and_then(|value| value.split_once('_'))
            .is_some_and(|(attempt, stage)| {
                !attempt.is_empty()
                    && attempt.bytes().all(|byte| byte.is_ascii_digit())
                    && allowed.contains(&stage)
            })
    };
    indexed(
        "repair_",
        &[
            "reserved",
            "prepared",
            "applied",
            "validated",
            "validation_failed",
        ],
    ) || indexed(
        "review_",
        &[
            "pending",
            "unavailable",
            "interrupted",
            "rejected",
            "approved",
        ],
    ) || indexed(
        "escalation_",
        &["applied", "validated", "validation_failed"],
    )
}

fn change_feature_reviewer(
    feature: &mut Feature,
    reviewer: AiSelection,
    revision: u64,
) -> Result<()> {
    if !feature_reviewer_state_is_changeable(feature) {
        bail!("Feature reviewer cannot change in its current state");
    }
    if feature.review_model == reviewer.model
        && feature.review_reasoning_effort == reviewer.reasoning_effort
    {
        bail!("Feature already uses the selected reviewer");
    }
    if feature.reviewer_selection_history.len() >= REVIEWER_SELECTION_HISTORY_LIMIT {
        bail!("Feature reviewer selection history limit reached");
    }
    let prior_model = feature.review_model.clone();
    let prior_reasoning_effort = feature.review_reasoning_effort.clone();
    if feature.review_pending.is_some() {
        interrupt_pending_review(
            feature,
            "The feature reviewer changed before its pending decision completed. Resume revalidates the generated files and starts a fresh review attempt.",
        )?;
    }
    if let Some(proposal) = feature
        .escalation_proposal
        .as_mut()
        .filter(|proposal| proposal.status == "ready")
    {
        proposal.status = "cancelled".into();
        proposal.summary = "The feature reviewer changed after this repair proposal was prepared. Prepare a fresh proposal before requesting owner approval.".into();
        proposal.error = None;
        proposal.binding_revision = revision;
        feature.escalation_history.push(RepairEscalationEvidence {
            proposal_id: proposal.proposal_id.clone(),
            attempt: proposal.attempt,
            model_target: proposal.model_target.clone(),
            model: proposal.model.clone(),
            chat_id: proposal.chat_id.clone(),
            chat_request_id: proposal.chat_request_id.clone(),
            diagnosis_sha256: proposal.diagnosis_sha256.clone(),
            outcome: "cancelled".into(),
            proposal_sha256: None,
            candidate_sha256: None,
            summary: proposal.summary.clone(),
            source: proposal.source.clone(),
            automatic_epoch: proposal.automatic_epoch,
            policy_revision: proposal.policy_revision,
            limit_snapshot: proposal.limit_snapshot,
            project_state_sha256: proposal.project_state_sha256.clone(),
            authorization_revision: None,
        });
    }
    feature
        .reviewer_selection_history
        .push(ReviewerSelectionEvidence {
            revision,
            prior_model,
            prior_reasoning_effort,
            selected_model: reviewer.model.clone(),
            selected_reasoning_effort: reviewer.reasoning_effort.clone(),
        });
    feature.review_model = reviewer.model;
    feature.review_reasoning_effort = reviewer.reasoning_effort;
    feature.review_binding_version = 1;
    feature.review_pending = None;
    feature.review_status = "pending".into();
    feature.review_summary =
        "Feature reviewer changed. Resume revalidates the current files and starts a fresh independent review without consuming a repair attempt."
            .into();
    Ok(())
}

fn review_checkpoint_requires_revalidation(checkpoint: &str) -> bool {
    checkpoint.starts_with("review_")
        || checkpoint == "review_binding_changed"
        || checkpoint == "auto_repair_limit_revalidating"
}

fn checkpoint_has_applied_edits(checkpoint: &str) -> bool {
    checkpoint == "applied"
        || checkpoint == "validated"
        || checkpoint.ends_with("_applied")
        || checkpoint.ends_with("_validated")
        || review_checkpoint_requires_revalidation(checkpoint)
}

fn checkpoint_reuses_retained_edits(checkpoint: &str) -> bool {
    checkpoint_has_applied_edits(checkpoint)
        || checkpoint.ends_with("_cancelled_before_authorization")
}

fn escalation_apply_is_quarantined(feature: &Feature) -> bool {
    feature.status == "failed" && feature.checkpoint.ends_with("_apply_interrupted")
}

fn validate_current_review_edits(edits: &[Edit], project: &Path) -> Result<()> {
    if edits.len() > 40 {
        bail!("Review requires at most 40 cumulative locally generated files");
    }
    let mut seen = std::collections::HashSet::new();
    for edit in edits {
        if !seen.insert(edit.path.to_lowercase()) {
            bail!("Cumulative generated-file evidence contains a duplicate path");
        }
        let full = checked_path(project, &edit.path)?;
        let bytes = fs::read(&full).with_context(|| {
            format!(
                "Generated file {} changed before escalation approval",
                edit.path
            )
        })?;
        if hash(&bytes) != hash(edit.content.as_bytes()) {
            bail!(
                "Generated file {} changed before escalation approval",
                edit.path
            );
        }
    }
    Ok(())
}

fn merge_review_edits(current: &[Edit], applied: &[Edit]) -> Result<Vec<Edit>> {
    let mut merged = current.to_vec();
    let mut indices = std::collections::HashMap::new();
    for (index, edit) in merged.iter().enumerate() {
        if indices.insert(edit.path.to_lowercase(), index).is_some() {
            bail!("Cumulative generated-file evidence contains a duplicate path");
        }
    }
    let mut applied_paths = std::collections::HashSet::new();
    for edit in applied {
        let normalized = edit.path.to_lowercase();
        if !applied_paths.insert(normalized.clone()) {
            bail!("Applied repair evidence contains a duplicate path");
        }
        if let Some(index) = indices.get(&normalized).copied() {
            // `before` is the earliest known feature baseline. Later repairs or
            // escalations update only the expected final bytes.
            merged[index].content = edit.content.clone();
        } else {
            indices.insert(normalized, merged.len());
            merged.push(edit.clone());
        }
    }
    if merged.is_empty() || merged.len() > 40 {
        bail!("Review requires 1 to 40 cumulative locally generated files");
    }
    Ok(merged)
}

fn reconcile_limit_recovery_edits(
    feature: &Feature,
    current: &AdmittedProjectSnapshot,
) -> Result<Vec<Edit>> {
    let expected_unadmitted = feature
        .auto_repair_limit_unadmitted_sha256
        .as_deref()
        .context("Automatic repair limit has no unadmitted-data binding")?;
    if expected_unadmitted != current.unadmitted_sha256 {
        bail!("Sensitive or unadmitted project data changed after the AI repair limit was reached");
    }
    let expected_volatile = feature
        .auto_repair_limit_volatile_sha256
        .as_deref()
        .context("Automatic repair limit has no volatile-data binding")?;
    if expected_volatile != current.volatile_sha256 {
        bail!("Build, cache, or dependency output changed after the AI repair limit was reached");
    }
    for path in feature.auto_repair_limit_project_baseline.keys() {
        if !current.files.contains_key(path) {
            bail!("An admitted project file was deleted after the AI repair limit was reached");
        }
    }
    let owner_edits = current
        .files
        .iter()
        .filter_map(|(path, (digest, content))| {
            let before = feature.auto_repair_limit_project_baseline.get(path);
            (before != Some(digest)).then(|| Edit {
                path: path.clone(),
                content: content.clone(),
                before: before.cloned(),
            })
        })
        .collect::<Vec<_>>();
    let prior = feature.edits.as_deref().unwrap_or_default();
    if owner_edits.is_empty() {
        if prior.is_empty() {
            bail!("Limit recovery has no generated or owner-corrected files to review");
        }
        return Ok(prior.to_vec());
    }
    merge_review_edits(prior, &owner_edits)
}

fn prepare_limit_recovery_edits(
    feature: &mut Feature,
    current: &AdmittedProjectSnapshot,
) -> Result<Vec<Edit>> {
    let missing_baseline = feature.auto_repair_limit_project_baseline.is_empty()
        && feature.auto_repair_limit_unadmitted_sha256.is_none()
        && feature.auto_repair_limit_volatile_sha256.is_none();
    if !missing_baseline {
        return reconcile_limit_recovery_edits(feature, current);
    }
    if current.files.is_empty() {
        bail!("Limit recovery has no bounded current project files to review");
    }
    if current.files.len() > 40 {
        bail!("Limit recovery cannot review more than 40 current project files");
    }
    feature.auto_repair_limit_project_baseline = current
        .files
        .iter()
        .map(|(path, (digest, _))| (path.clone(), digest.clone()))
        .collect();
    feature.auto_repair_limit_unadmitted_sha256 = Some(current.unadmitted_sha256.clone());
    feature.auto_repair_limit_volatile_sha256 = Some(current.volatile_sha256.clone());
    Ok(current
        .files
        .iter()
        .map(|(path, (digest, content))| Edit {
            path: path.clone(),
            content: content.clone(),
            // The post-limit current bytes are the newly established recovery
            // baseline. Recording their exact digest avoids claiming they are
            // new while still forcing every admitted byte through Codex review.
            before: Some(digest.clone()),
        })
        .collect())
}

fn tool_review_and_application_edits(
    live_environment_edits: &[Edit],
    candidate_edits: &[Edit],
    staged_candidate: bool,
) -> Result<(Vec<Edit>, Vec<Edit>)> {
    let review_edits = if live_environment_edits.is_empty() {
        candidate_edits.to_vec()
    } else if candidate_edits.is_empty() {
        live_environment_edits.to_vec()
    } else {
        merge_review_edits(live_environment_edits, candidate_edits)?
    };
    let application_edits = if staged_candidate {
        candidate_edits.to_vec()
    } else {
        Vec::new()
    };
    Ok((review_edits, application_edits))
}

fn merge_escalation_review_edits(
    current: &[Edit],
    proposal_files: &[RepairEscalationFile],
    project: &Path,
) -> Result<Vec<Edit>> {
    validate_current_review_edits(current, project)?;
    let proposed = proposal_files
        .iter()
        .map(|file| Edit {
            path: file.path.clone(),
            content: file.after.clone(),
            before: file.before.as_ref().map(|before| hash(before.as_bytes())),
        })
        .collect::<Vec<_>>();
    merge_review_edits(current, &proposed)
}

fn tool_mutation_edits(
    mutations: &[ToolProjectMutation],
    expected_feature_id: Option<&str>,
) -> Result<Vec<Edit>> {
    let mut edits = Vec::<Edit>::new();
    let mut indices = std::collections::HashMap::<String, usize>::new();
    for mutation in mutations {
        if mutation.feature_id.as_deref() != expected_feature_id {
            bail!("Tool mutation attribution changed; review the project before retrying");
        }
        let unreviewable = mutation
            .unreviewable_paths
            .iter()
            .filter(|path| !is_validation_environment_artifact(path))
            .cloned()
            .collect::<Vec<_>>();
        if !unreviewable.is_empty() {
            bail!(
                "Tool session changed files that cannot enter bounded review: {}",
                unreviewable.join(", ")
            );
        }
        for mutation_edit in &mutation.edits {
            let after = mutation_edit.after.as_ref().with_context(|| {
                format!(
                    "Tool-assisted feature deleted {}; deletion cannot enter bounded review",
                    mutation_edit.path
                )
            })?;
            if mutation_edit.before_sha256.as_ref().is_some_and(|digest| {
                digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
            }) {
                bail!("Tool mutation has an invalid prior-content digest");
            }
            let normalized = mutation_edit.path.replace('\\', "/").to_lowercase();
            if let Some(index) = indices.get(&normalized).copied() {
                edits[index].content = after.clone();
            } else {
                indices.insert(normalized, edits.len());
                edits.push(Edit {
                    path: mutation_edit.path.clone(),
                    content: after.clone(),
                    before: mutation_edit.before_sha256.clone(),
                });
            }
        }
    }
    if edits.len() > 40 {
        bail!("Tool-assisted feature changed more than 40 reviewable files");
    }
    Ok(edits)
}

fn is_validation_environment_artifact(path: &str) -> bool {
    matches!(
        path.replace('\\', "/")
            .trim_matches('/')
            .to_ascii_lowercase()
            .as_str(),
        ".venv" | "venv" | "node_modules" | "target"
    )
}

fn owned_tool_mutation_edits(
    mutations: &[ToolProjectMutation],
    feature_id: &str,
) -> Result<Vec<Edit>> {
    let mut combined = Vec::new();
    for mutation in mutations {
        let expected = mutation.feature_id.as_deref().map(|_| feature_id);
        let edits = tool_mutation_edits(std::slice::from_ref(mutation), expected)?;
        if edits.is_empty() {
            continue;
        }
        combined = if combined.is_empty() {
            edits
        } else {
            merge_review_edits(&combined, &edits)?
        };
    }
    Ok(combined)
}

fn quarantine_tool_candidate(
    feature: &mut Feature,
    workspace_revision: u64,
    live_edits: &[Edit],
    staged_candidate: bool,
    reason: &str,
) -> Result<()> {
    feature.tool_workspace_revision = workspace_revision;
    if !live_edits.is_empty() {
        feature.edits = Some(merge_review_edits(
            feature.edits.as_deref().unwrap_or_default(),
            live_edits,
        )?);
    }
    feature.status = "failed".into();
    feature.review_status = "interrupted".into();
    feature.review_pending = None;
    feature.review_summary = reason.chars().take(1000).collect();
    feature.checkpoint = if staged_candidate {
        "staged_tool_candidate_quarantined"
    } else {
        "tool_effects_quarantined"
    }
    .into();
    feature.message = if staged_candidate {
        format!("The staged tool candidate was not applied to the live project. {reason}")
    } else {
        format!("Project tool effects require inspection. {reason}")
    }
    .chars()
    .take(4000)
    .collect();
    Ok(())
}

fn never_started_project_has_no_tool_ledger(feature: &Feature) -> bool {
    feature.tool_workspace_revision == 0 && feature.edits.is_none() && feature.review_attempts == 0
}

fn reconcile_feature_tool_mutation(
    feature: &mut Feature,
    latest_revision: u64,
    owned_edits: &Result<Vec<Edit>>,
) -> Result<()> {
    feature.tool_workspace_revision = latest_revision;
    let has_review_evidence = feature.edits.is_some()
        || feature.review_attempts > 0
        || feature.status == "succeeded"
        || checkpoint_has_applied_edits(&feature.checkpoint);
    if !has_review_evidence {
        return Ok(());
    }
    let edits = match owned_edits {
        Ok(edits) => edits,
        Err(error) => {
            feature.status = "failed".into();
            feature.checkpoint = "tool_effects_quarantined".into();
            feature.review_status = "interrupted".into();
            feature.review_pending = None;
            feature.review_summary = error.to_string().chars().take(1000).collect();
            feature.message = format!(
                "Project tools changed bytes that cannot enter bounded review. {}",
                feature.review_summary
            )
            .chars()
            .take(4000)
            .collect();
            return Ok(());
        }
    };
    if !edits.is_empty() {
        feature.edits = Some(merge_review_edits(
            feature.edits.as_deref().unwrap_or_default(),
            edits,
        )?);
    }
    feature.review_status = "interrupted".into();
    feature.review_pending = None;
    feature.review_summary =
        "Project tools changed the workspace; immutable validation and Codex review must run again."
            .into();
    if feature.status == "succeeded" || feature.status == "queued" {
        feature.status = "paused".into();
    }
    feature.checkpoint = if feature.repair_attempts >= REPAIR_LIMIT && feature.status == "failed" {
        "tool_workspace_changed_requires_proposal".into()
    } else {
        "review_tool_workspace_changed".into()
    };
    feature.message = feature.review_summary.clone();
    Ok(())
}

fn developer_review_packet(
    feature: &Feature,
    project: &Path,
    current_edits: &[Edit],
    validation_evidence_sha256: &str,
) -> Result<DeveloperReviewPacket> {
    let validation_paths = validation_path_references(project, &feature.validation)?;
    let mut generated = std::collections::BTreeMap::<String, (Option<String>, String)>::new();
    for attempt in &feature.repair_history {
        for edit in &attempt.prior_edits {
            generated
                .entry(edit.path.clone())
                .and_modify(|evidence| evidence.1 = edit.content_hash.clone())
                .or_insert_with(|| (edit.before.clone(), edit.content_hash.clone()));
        }
    }
    for edit in current_edits {
        let content_hash = hash(edit.content.as_bytes());
        generated
            .entry(edit.path.clone())
            .and_modify(|evidence| evidence.1 = content_hash.clone())
            .or_insert_with(|| (edit.before.clone(), content_hash));
    }
    if generated.is_empty() || generated.len() > 40 {
        bail!("Review requires 1 to 40 cumulative locally generated files");
    }
    let mut files = Vec::with_capacity(generated.len());
    for (path, (before_sha256, expected_content_sha256)) in generated {
        let full = checked_path(project, &path)?;
        let bytes = fs::read(&full)
            .with_context(|| format!("Generated file {path} is unavailable for required review"))?;
        let content = String::from_utf8(bytes)
            .with_context(|| format!("Generated file {path} is not UTF-8 review input"))?;
        let content_sha256 = hex_digest(content.as_bytes());
        if content_sha256 != expected_content_sha256 {
            bail!("Generated file {path} changed after the local model prepared it");
        }
        files.push(DeveloperReviewFile {
            classification: trusted_review_file_classification(&path, &validation_paths),
            path,
            before_sha256,
            content_sha256,
            content,
        });
    }
    Ok(DeveloperReviewPacket {
        schema_version: 1,
        feature_id: feature.id.clone(),
        project: feature.project.clone(),
        instruction: feature.instruction.clone(),
        approved_plan_sha256: feature
            .planning
            .as_ref()
            .map(|plan| plan.plan_sha256.clone()),
        approved_plan: feature.planning.as_ref().map(combined_plan),
        validation_command: feature.validation.clone(),
        validation_evidence_sha256: validation_evidence_sha256.into(),
        provider_id: REVIEW_PROVIDER_ID.into(),
        model_id: feature.review_model.clone(),
        reasoning_effort: feature.review_reasoning_effort.clone(),
        files,
    })
}

fn verify_approved_review_binding(feature: &Feature, project: &Path) -> Result<()> {
    if feature.review_status != "approved" || feature.review_pending.is_some() {
        bail!("Required Codex review approval is missing");
    }
    let approved = feature
        .review_history
        .last()
        .filter(|attempt| attempt.outcome == "approved" && attempt.decision_sha256.is_some())
        .context("Required Codex review approval evidence is missing")?;
    let edits = feature
        .edits
        .as_deref()
        .context("Approved review has no generated-file evidence")?;
    let rebound = developer_review_packet(
        feature,
        project,
        edits,
        &approved.validation_evidence_sha256,
    )?;
    let current_sha256 = rebound.sha256()?;
    let legacy_match = feature.review_binding_version == 0
        && rebound.legacy_sha256_without_reasoning()? == approved.packet_sha256;
    let prior_classification_match = feature.review_binding_version == 1
        && rebound.legacy_sha256_without_classification()? == approved.packet_sha256;
    if current_sha256 != approved.packet_sha256 && !legacy_match && !prior_classification_match {
        bail!("Generated files changed after ChatGPT Codex approval; validation and review must run again");
    }
    Ok(())
}

fn publication_input(feature: &Feature) -> Result<PublicationInput> {
    if !feature.publication_selection_frozen {
        bail!("Feature publication selection is not frozen");
    }
    let binding = feature
        .publication_binding
        .clone()
        .context("Feature has no frozen GitHub repository connection")?;
    validate_publication_binding(&binding)?;
    if binding.project != feature.project {
        bail!("Frozen GitHub connection no longer matches the feature project");
    }
    if feature.publication_candidate.is_empty() || feature.publication_candidate.len() > 40 {
        bail!("Frozen publication candidate is missing or exceeds its file bound");
    }
    let approved = feature
        .review_history
        .iter()
        .rev()
        .find(|attempt| attempt.outcome == "approved" && attempt.decision_sha256.is_some())
        .context("Required Codex review approval evidence is missing")?;
    let files = feature
        .publication_candidate
        .iter()
        .map(|file| {
            if hash(file.content.as_bytes()) != file.content_sha256 {
                bail!("Frozen publication candidate content changed");
            }
            Ok(DeveloperReviewFile {
                classification: file.classification.clone(),
                path: file.path.clone(),
                before_sha256: file.before_sha256.clone(),
                content_sha256: file.content_sha256.clone(),
                content: file.content.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let packet = DeveloperReviewPacket {
        schema_version: 1,
        feature_id: feature.id.clone(),
        project: feature.project.clone(),
        instruction: feature.instruction.clone(),
        approved_plan_sha256: feature
            .planning
            .as_ref()
            .map(|plan| plan.plan_sha256.clone()),
        approved_plan: feature.planning.as_ref().map(combined_plan),
        validation_command: feature.validation.clone(),
        validation_evidence_sha256: approved.validation_evidence_sha256.clone(),
        provider_id: REVIEW_PROVIDER_ID.into(),
        model_id: feature.review_model.clone(),
        reasoning_effort: feature.review_reasoning_effort.clone(),
        files,
    };
    let exact = packet.sha256()? == approved.packet_sha256;
    let legacy = feature.review_binding_version == 0
        && packet.legacy_sha256_without_reasoning()? == approved.packet_sha256;
    let prior_classification = feature.review_binding_version == 1
        && packet.legacy_sha256_without_classification()? == approved.packet_sha256;
    if !exact && !legacy && !prior_classification {
        bail!("Frozen publication candidate does not match the exact approved review packet");
    }
    Ok(PublicationInput {
        feature_id: feature.id.clone(),
        title: format!("Developer feature {}", feature.id),
        body: "Automated publication of an exact validated and independently reviewed Developer candidate.".into(),
        binding,
        files: feature
            .publication_candidate
            .iter()
            .map(|file| CandidateFile {
                path: file.path.clone(),
                before_sha256: file.before_sha256.clone(),
                content_sha256: file.content_sha256.clone(),
                content: file.content.clone(),
            })
            .collect(),
    })
}

fn review_summary(output: &DeveloperReviewOutput) -> String {
    if output.decision == DeveloperReviewDecisionKind::Approved {
        if output.non_blocking_findings.is_empty() {
            return "ChatGPT Codex approved the exact generated files after validation passed."
                .into();
        }
        return format!(
            "ChatGPT Codex approved with {} non-blocking finding(s): {}",
            output.non_blocking_findings.len(),
            output
                .non_blocking_findings
                .iter()
                .map(|finding| finding.message.as_str())
                .collect::<Vec<_>>()
                .join("; ")
        )
        .chars()
        .take(3500)
        .collect();
    }
    format!(
        "ChatGPT Codex rejected the exact generated files with {} blocking finding(s): {}",
        output.blocking_findings.len(),
        output
            .blocking_findings
            .iter()
            .map(|finding| format!("{}: {}", finding.path, finding.message))
            .collect::<Vec<_>>()
            .join("; ")
    )
    .chars()
    .take(3500)
    .collect()
}

fn remove_feature(state: &mut Snapshot, id: &str) -> Result<bool> {
    Uuid::parse_str(id).context("Invalid feature ID")?;
    let feature = state
        .queue
        .iter_mut()
        .find(|feature| feature.id == id)
        .context("Feature not found")?;
    if feature.publication.as_ref().is_some_and(|publication| {
        matches!(
            publication.status.as_str(),
            "pending" | "running" | "attention"
        )
    }) {
        bail!("Resolve the unfinished GitHub publication before removing this feature");
    }
    match feature.status.as_str() {
        "removed" => Ok(false),
        "queued" | "failed" | "paused" => {
            feature.status = "removed".into();
            Ok(true)
        }
        "running" => bail!("Stop the active run before removing this feature"),
        "succeeded" => bail!("Completed features cannot be removed"),
        _ => bail!("Feature has an invalid status and cannot be removed"),
    }
}
fn terminate_escalation_review_not_run(feature: &mut Feature, summary: &str) -> Result<()> {
    let proposal = feature
        .escalation_proposal
        .as_ref()
        .context("Reserved repair proposal evidence is missing")?;
    if proposal.review_slot_terminal {
        return Ok(());
    }
    if feature.review_history.len() >= REVIEW_HISTORY_LIMIT {
        bail!("Reserved independent-review evidence slot is unavailable");
    }
    let attempt = proposal.attempt;
    let packet_sha256 = repair_escalation_candidate_sha256(proposal)?;
    let validation_evidence_sha256 = proposal.diagnosis_sha256.clone();
    feature.review_history.push(ReviewAttemptEvidence {
        attempt,
        packet_sha256,
        validation_evidence_sha256,
        outcome: "not_run".into(),
        decision_sha256: None,
        blocking_findings: Vec::new(),
        summary: summary.chars().take(1000).collect(),
    });
    feature
        .escalation_proposal
        .as_mut()
        .context("Reserved repair proposal evidence is missing")?
        .review_slot_terminal = true;
    Ok(())
}

fn escalation_evidence_stage(outcome: &str) -> Option<&'static str> {
    if matches!(
        outcome,
        "ready" | "no_op" | "duplicate" | "unavailable" | "cancelled" | "proposal_interrupted"
    ) {
        Some("proposal")
    } else if matches!(
        outcome,
        "approved_to_apply"
            | "policy_authorized"
            | "authorization_not_run"
            | "authorization_rejected"
    ) {
        Some("authorization")
    } else if matches!(
        outcome,
        "succeeded" | "failed" | "interrupted" | "application_not_run"
    ) {
        Some("application")
    } else {
        None
    }
}

fn terminalize_unapplied_escalation(
    feature: &mut Feature,
    proposal_outcome_if_missing: &str,
    authorization_outcome: &str,
    terminal_status: &str,
    summary: &str,
) -> Result<()> {
    if escalation_evidence_stage(proposal_outcome_if_missing) != Some("proposal")
        || !matches!(
            authorization_outcome,
            "authorization_not_run" | "authorization_rejected"
        )
    {
        bail!("Invalid unapplied escalation terminal evidence classification");
    }
    let proposal = feature
        .escalation_proposal
        .as_ref()
        .context("Reserved repair proposal evidence is missing")?
        .clone();
    let proposal_id = proposal.proposal_id.as_str();
    let stages = ["proposal", "authorization", "application"];
    let stage_counts = stages.map(|stage| {
        feature
            .escalation_history
            .iter()
            .filter(|evidence| {
                evidence.proposal_id == proposal_id
                    && escalation_evidence_stage(&evidence.outcome) == Some(stage)
            })
            .count()
    });
    for (stage, count) in stages.iter().zip(stage_counts) {
        if count > 1 {
            bail!("Repair escalation contains duplicate {stage} evidence");
        }
    }
    let ready_sha256 = feature
        .escalation_history
        .iter()
        .find(|evidence| evidence.proposal_id == proposal_id && evidence.outcome == "ready")
        .and_then(|evidence| evidence.proposal_sha256.clone());
    let candidate_sha256 = if proposal_outcome_if_missing == "proposal_interrupted" {
        None
    } else {
        Some(repair_escalation_candidate_sha256(&proposal)?)
    };
    for (index, (_stage, outcome, proposal_sha256)) in [
        ("proposal", proposal_outcome_if_missing, None),
        ("authorization", authorization_outcome, ready_sha256.clone()),
        ("application", "application_not_run", ready_sha256),
    ]
    .into_iter()
    .enumerate()
    {
        if stage_counts[index] != 0 {
            continue;
        }
        feature.escalation_history.push(RepairEscalationEvidence {
            proposal_id: proposal.proposal_id.clone(),
            attempt: proposal.attempt,
            model_target: proposal.model_target.clone(),
            model: proposal.model.clone(),
            chat_id: proposal.chat_id.clone(),
            chat_request_id: proposal.chat_request_id.clone(),
            diagnosis_sha256: proposal.diagnosis_sha256.clone(),
            outcome: outcome.into(),
            proposal_sha256,
            candidate_sha256: candidate_sha256.clone(),
            summary: summary.chars().take(1000).collect(),
            source: proposal.source.clone(),
            automatic_epoch: proposal.automatic_epoch,
            policy_revision: proposal.policy_revision,
            limit_snapshot: proposal.limit_snapshot,
            project_state_sha256: proposal.project_state_sha256.clone(),
            authorization_revision: None,
        });
    }
    terminate_escalation_review_not_run(feature, summary)?;
    let current = feature
        .escalation_proposal
        .as_mut()
        .context("Reserved repair proposal evidence is missing")?;
    current.status = terminal_status.into();
    current.error = (authorization_outcome == "authorization_rejected")
        .then(|| summary.chars().take(1000).collect());
    feature.escalation_pending = false;
    Ok(())
}

fn recover_interrupted_escalation_preparation(
    feature: &mut Feature,
    recovery_revision: u64,
) -> Result<bool> {
    let Some(proposal) = feature
        .escalation_proposal
        .as_mut()
        .filter(|proposal| proposal.status == "preparing")
    else {
        return Ok(false);
    };
    let automatic = proposal.source == "automatic_failure";
    proposal.summary =
        "Runner restarted before the selected local AI finished the repair proposal; prepare a new proposal"
            .into();
    proposal.error = None;
    proposal.binding_revision = recovery_revision;
    let summary = "Independent review was not run because proposal generation was interrupted by runner restart; authorization and application were also not run";
    terminalize_unapplied_escalation(
        feature,
        "proposal_interrupted",
        "authorization_not_run",
        "interrupted",
        summary,
    )?;
    if automatic {
        set_auto_repair_lifecycle(
            feature,
            "running",
            "The interrupted proposal generation was recorded; the next attempt may be reserved from the same feature budget",
        )?;
    }
    Ok(true)
}

fn retire_ready_automatic_proposal_for_cancellation(
    feature: &mut Feature,
    summary: &str,
) -> Result<bool> {
    let Some(attempt) = feature.escalation_proposal.as_ref().and_then(|proposal| {
        (proposal.source == "automatic_failure" && proposal.status == "ready")
            .then_some(proposal.attempt)
    }) else {
        return Ok(false);
    };
    terminalize_unapplied_escalation(
        feature,
        "ready",
        "authorization_not_run",
        "cancelled",
        summary,
    )?;
    feature.status = "failed".into();
    feature.checkpoint = format!("escalation_{attempt}_cancelled_before_authorization");
    feature.message = summary.chars().take(4000).collect();
    Ok(true)
}

fn pause_preapply_automatic_proposal_for_disable(
    feature: &mut Feature,
    summary: &str,
) -> Result<bool> {
    let Some((attempt, proposal_outcome)) =
        feature.escalation_proposal.as_ref().and_then(|proposal| {
            (proposal.source == "automatic_failure"
                && matches!(proposal.status.as_str(), "preparing" | "ready"))
            .then(|| {
                (
                    proposal.attempt,
                    if proposal.status == "ready" {
                        "ready"
                    } else {
                        "cancelled"
                    },
                )
            })
        })
    else {
        return Ok(false);
    };
    terminalize_unapplied_escalation(
        feature,
        proposal_outcome,
        "authorization_not_run",
        "cancelled",
        summary,
    )?;
    feature.status = "paused".into();
    feature.checkpoint = format!("escalation_{attempt}_cancelled_before_authorization");
    feature.message = summary.chars().take(4000).collect();
    Ok(true)
}

fn automatic_escalation_is_running(feature: &Feature) -> bool {
    feature.auto_repair_lifecycle == "running"
        && feature
            .escalation_proposal
            .as_ref()
            .is_some_and(|proposal| proposal.source == "automatic_failure")
}

fn record_escalation_file_application(
    feature: &mut Feature,
    edit: &Edit,
    binding_revision: u64,
) -> Result<()> {
    feature.edits = Some(merge_review_edits(
        feature.edits.as_deref().unwrap_or_default(),
        std::slice::from_ref(edit),
    )?);
    let automatic_application = automatic_escalation_is_running(feature);
    let proposal = feature
        .escalation_proposal
        .as_mut()
        .filter(|proposal| proposal.status == "approved" || proposal.status == "applying")
        .context("Approved repair proposal evidence is missing")?;
    let application_started = proposal.status == "approved";
    if !proposal.files.iter().any(|file| file.path == edit.path) {
        bail!("Applied file is outside the approved repair proposal");
    }
    if !proposal.applied_paths.iter().any(|path| path == &edit.path) {
        proposal.applied_paths.push(edit.path.clone());
    }
    proposal.status = "applying".into();
    proposal.binding_revision = binding_revision;
    feature.checkpoint = format!("escalation_{}_applying", proposal.attempt);
    feature.message = format!(
        "Applied {} of {} explicitly approved repair files",
        proposal.applied_paths.len(),
        proposal.files.len()
    );
    if application_started && automatic_application {
        start_auto_repair_step(feature)?;
    }
    Ok(())
}

fn finish_escalation_application(
    feature: &mut Feature,
    binding_revision: u64,
    outcome: &str,
    summary: &str,
) -> Result<()> {
    if matches!(outcome, "failed" | "interrupted") {
        if feature.checkpoint.starts_with("review_") {
            feature
                .escalation_proposal
                .as_mut()
                .context("Applied repair proposal evidence is missing")?
                .review_slot_terminal = true;
        } else {
            terminate_escalation_review_not_run(
                feature,
                "Independent review was not run because proposal application or validation did not complete",
            )?;
        }
    }
    let current_proposal_id = feature
        .escalation_proposal
        .as_ref()
        .context("Applied repair proposal evidence is missing")?
        .proposal_id
        .clone();
    let approved_proposal_sha256 = feature
        .escalation_history
        .iter()
        .rev()
        .find(|evidence| {
            evidence.proposal_id == current_proposal_id
                && matches!(
                    evidence.outcome.as_str(),
                    "approved_to_apply" | "policy_authorized"
                )
                && evidence.proposal_sha256.is_some()
        })
        .and_then(|evidence| evidence.proposal_sha256.clone())
        .context("Approved repair proposal digest evidence is missing")?;
    let proposal = feature
        .escalation_proposal
        .as_mut()
        .context("Applied repair proposal evidence is missing")?;
    proposal.status = outcome.into();
    proposal.error = if outcome == "failed" {
        Some(summary.chars().take(1000).collect())
    } else {
        None
    };
    proposal.binding_revision = binding_revision;
    let evidence = RepairEscalationEvidence {
        proposal_id: proposal.proposal_id.clone(),
        attempt: proposal.attempt,
        model_target: proposal.model_target.clone(),
        model: proposal.model.clone(),
        chat_id: proposal.chat_id.clone(),
        chat_request_id: proposal.chat_request_id.clone(),
        diagnosis_sha256: proposal.diagnosis_sha256.clone(),
        outcome: outcome.into(),
        proposal_sha256: Some(approved_proposal_sha256),
        candidate_sha256: Some(repair_escalation_candidate_sha256(proposal)?),
        summary: summary.chars().take(1000).collect(),
        source: proposal.source.clone(),
        automatic_epoch: proposal.automatic_epoch,
        policy_revision: proposal.policy_revision,
        limit_snapshot: proposal.limit_snapshot,
        project_state_sha256: proposal.project_state_sha256.clone(),
        authorization_revision: None,
    };
    feature.escalation_pending = false;
    feature.escalation_history.push(evidence);
    Ok(())
}

fn quarantine_escalation_application(
    feature: &mut Feature,
    project: &Path,
    binding_revision: u64,
    summary: &str,
) -> Result<()> {
    let attempt = feature
        .escalation_proposal
        .as_ref()
        .context("Interrupted repair proposal evidence is missing")?
        .attempt;
    reconcile_interrupted_escalation_edits(feature, project)?;
    finish_escalation_application(feature, binding_revision, "interrupted", summary)?;
    feature.status = "failed".into();
    feature.checkpoint = format!("escalation_{attempt}_apply_interrupted");
    feature.message = summary.into();
    feature.review_pending = None;
    feature.review_status = "pending".into();
    feature.review_summary = "Required ChatGPT Codex review has not started".into();
    if feature.auto_repair_lifecycle == "running" {
        set_auto_repair_lifecycle(
            feature,
            "quarantined",
            "Automatic repair was interrupted across an uncertain application boundary. Inspect the workspace; automatic replay is blocked.",
        )?;
    }
    Ok(())
}

fn automatic_post_apply_is_ambiguous(feature: &Feature) -> bool {
    let ordinary_application_started = feature
        .checkpoint
        .strip_prefix("repair_")
        .and_then(|value| value.split_once('_'))
        .is_some_and(|(attempt, stage)| {
            !attempt.is_empty()
                && attempt.bytes().all(|byte| byte.is_ascii_digit())
                && stage == "applying"
        });
    (checkpoint_has_applied_edits(&feature.checkpoint) || ordinary_application_started)
        && !matches!(feature.status.as_str(), "succeeded" | "removed")
}

fn automatic_ordinary_pre_effect_checkpoint(feature: &Feature) -> bool {
    feature.repair_pending
        && feature
            .checkpoint
            .strip_prefix("repair_")
            .and_then(|value| value.split_once('_'))
            .is_some_and(|(attempt, stage)| {
                attempt.parse::<u32>().ok() == Some(feature.repair_attempts)
                    && matches!(stage, "reserved" | "prepared")
            })
}

fn persist_automatic_cancellation_intent(
    feature: &mut Feature,
    work_active: bool,
    inactive_reason: &str,
    quarantine_reason: &str,
) -> Result<()> {
    feature.auto_repair_epoch = feature
        .auto_repair_epoch
        .checked_add(1)
        .context("Automatic repair epoch overflow")?;
    if work_active && automatic_ordinary_pre_effect_checkpoint(feature) {
        feature.status = "paused".into();
        set_auto_repair_lifecycle(
            feature,
            "held",
            "Automatic repair stopped before its ordinary repair effect boundary. Resume or re-enable may continue the exact reserved or prepared attempt under a fresh policy epoch.",
        )
    } else if work_active && automatic_post_apply_is_ambiguous(feature) {
        set_auto_repair_lifecycle(feature, "quarantined", quarantine_reason)
    } else {
        set_auto_repair_lifecycle(feature, "inactive", inactive_reason)
    }
}

fn quarantine_automatic_post_apply(
    feature: &mut Feature,
    binding_revision: u64,
    summary: &str,
) -> Result<()> {
    if let Some(pending) = feature.review_pending.as_ref() {
        let pending_attempt = pending.attempt;
        interrupt_pending_review(feature, summary)?;
        if feature.checkpoint != format!("review_{pending_attempt}_interrupted") {
            bail!("Interrupted review evidence was not durably terminalized");
        }
    }
    if feature.escalation_pending {
        finish_escalation_application(feature, binding_revision, "interrupted", summary)?;
    }
    feature.status = "failed".into();
    feature.checkpoint = "auto_repair_effects_quarantined".into();
    feature.message = summary.chars().take(4000).collect();
    feature.review_pending = None;
    if feature.review_status == "pending" || feature.review_status == "reviewing" {
        feature.review_status = "interrupted".into();
        feature.review_summary = "Automatic repair stopped after files were applied; ordinary Resume is blocked because validation or review completion is ambiguous".into();
    }
    set_auto_repair_lifecycle(
        feature,
        "quarantined",
        "Automatic repair stopped after project files were applied. Inspect the retained workspace and evidence; ordinary Resume cannot replay this ambiguous boundary.",
    )
}

fn reconcile_interrupted_escalation_edits(feature: &mut Feature, project: &Path) -> Result<()> {
    let proposal = feature
        .escalation_proposal
        .as_ref()
        .context("Interrupted repair proposal evidence is missing")?
        .clone();
    let proposal_paths = proposal
        .files
        .iter()
        .map(|file| file.path.to_lowercase())
        .collect::<std::collections::HashSet<_>>();
    if proposal_paths.len() != proposal.files.len() {
        bail!("Interrupted repair proposal contains duplicate paths");
    }
    let recorded_paths = proposal
        .applied_paths
        .iter()
        .map(|path| path.to_lowercase())
        .collect::<std::collections::HashSet<_>>();
    if recorded_paths.len() != proposal.applied_paths.len()
        || !recorded_paths.is_subset(&proposal_paths)
    {
        bail!("Interrupted repair proposal has invalid applied-path evidence");
    }

    let prior_edits = feature.edits.clone().unwrap_or_default();
    let mut reconciled = prior_edits
        .iter()
        .filter(|edit| !proposal_paths.contains(&edit.path.to_lowercase()))
        .cloned()
        .collect::<Vec<_>>();
    for file in &proposal.files {
        let normalized = file.path.to_lowercase();
        let full = checked_path(project, &file.path)?;
        let current = if full.exists() {
            Some(fs::read(&full)?)
        } else {
            None
        };
        let matches_after = current.as_deref() == Some(file.after.as_bytes());
        let matches_before = match (&current, &file.before) {
            (None, None) => true,
            (Some(current), Some(before)) => current.as_slice() == before.as_bytes(),
            _ => false,
        };
        let was_recorded = recorded_paths.contains(&normalized);
        if was_recorded && !matches_after {
            bail!("{} recorded applied bytes changed", file.path);
        }
        if matches_after {
            let matching_prior = prior_edits.iter().find(|edit| {
                edit.path.eq_ignore_ascii_case(&file.path)
                    && file.before.as_ref().is_some_and(|before| {
                        hash(edit.content.as_bytes()) == hash(before.as_bytes())
                    })
            });
            let applied = Edit {
                path: file.path.clone(),
                before: matching_prior.map_or_else(
                    || file.before.as_ref().map(|before| hash(before.as_bytes())),
                    |edit| edit.before.clone(),
                ),
                content: file.after.clone(),
            };
            reconciled = merge_review_edits(&reconciled, std::slice::from_ref(&applied))?;
            if !feature
                .escalation_proposal
                .as_ref()
                .unwrap()
                .applied_paths
                .iter()
                .any(|path| path.eq_ignore_ascii_case(&file.path))
            {
                feature
                    .escalation_proposal
                    .as_mut()
                    .unwrap()
                    .applied_paths
                    .push(file.path.clone());
            }
        } else if matches_before {
            let was_generated_before_proposal = feature.repair_history.iter().any(|attempt| {
                attempt
                    .prior_edits
                    .iter()
                    .any(|edit| edit.path.eq_ignore_ascii_case(&file.path))
            }) || prior_edits.iter().any(|edit| {
                edit.path.eq_ignore_ascii_case(&file.path)
                    && file.before.as_ref().is_some_and(|before| {
                        hash(edit.content.as_bytes()) == hash(before.as_bytes())
                    })
            });
            if was_generated_before_proposal {
                let prior = prior_edits
                    .iter()
                    .find(|edit| edit.path.eq_ignore_ascii_case(&file.path));
                let before = file
                    .before
                    .as_ref()
                    .context("Generated file disappeared during interrupted repair application")?;
                let restored = Edit {
                    path: file.path.clone(),
                    before: prior.and_then(|edit| edit.before.clone()),
                    content: before.clone(),
                };
                reconciled = merge_review_edits(&reconciled, std::slice::from_ref(&restored))?;
            }
        } else {
            bail!(
                "{} has ambiguous bytes after interrupted repair application",
                file.path
            );
        }
    }
    feature.edits = if reconciled.is_empty() {
        None
    } else {
        Some(reconciled)
    };
    Ok(())
}

fn recover_v7_cumulative_evidence(
    feature: &mut Feature,
    prior: &Feature,
    project: &Path,
) -> Result<()> {
    if feature.id != prior.id
        || feature.project != prior.project
        || feature.instruction != prior.instruction
        || feature.validation != prior.validation
        || feature.repair_attempts != prior.repair_attempts
        || serde_json::to_vec(&feature.repair_history)?
            != serde_json::to_vec(&prior.repair_history)?
        || prior.escalation_count != 0
    {
        bail!("Legacy escalation evidence does not match its v6 backup");
    }
    let proposal_id = feature
        .escalation_proposal
        .as_ref()
        .context("Legacy escalation evidence has no current proposal")?
        .proposal_id
        .clone();
    let proposal_checkpoint = feature
        .escalation_proposal
        .as_ref()
        .unwrap()
        .feature_checkpoint
        .clone();
    if prior.status != "failed" || prior.repair_pending || prior.checkpoint != proposal_checkpoint {
        bail!("Legacy v6 backup is not the exact failed-feature escalation baseline");
    }
    let approved_ids = feature
        .escalation_history
        .iter()
        .filter(|evidence| evidence.outcome == "approved_to_apply")
        .map(|evidence| evidence.proposal_id.clone())
        .collect::<std::collections::HashSet<_>>();
    if approved_ids.len() > 1
        || approved_ids
            .iter()
            .any(|approved_id| *approved_id != proposal_id)
    {
        bail!("Legacy cumulative escalation evidence cannot be reconstructed exactly");
    }

    feature.edits = prior.edits.clone();
    if approved_ids.contains(proposal_id.as_str()) {
        reconcile_interrupted_escalation_edits(feature, project)?;
        let may_be_partial = feature.escalation_pending
            && (feature.checkpoint.ends_with("_approved")
                || feature.checkpoint.ends_with("_applying"));
        let proposal = feature.escalation_proposal.as_ref().unwrap();
        if !may_be_partial && proposal.applied_paths.len() != proposal.files.len() {
            bail!("Legacy terminal escalation has incomplete applied-file evidence");
        }
    } else {
        let proposal = feature.escalation_proposal.as_ref().unwrap();
        for file in &proposal.files {
            let full = checked_path(project, &file.path)?;
            let current = if full.exists() {
                Some(fs::read(&full)?)
            } else {
                None
            };
            let unchanged = match (&current, &file.before) {
                (None, None) => true,
                (Some(current), Some(before)) => current.as_slice() == before.as_bytes(),
                _ => false,
            };
            if !unchanged {
                bail!("Legacy unapplied repair proposal bytes changed");
            }
        }
    }
    if let Some(edits) = feature.edits.as_deref() {
        validate_current_review_edits(edits, project)?;
    }
    if !feature.escalation_pending
        && !approved_ids.is_empty()
        && (feature.review_attempts > 0 || feature.status == "succeeded")
    {
        feature.status = "failed".into();
        feature.checkpoint = "review_binding_changed".into();
        feature.message = "Cumulative review evidence was reconstructed from the exact v6 backup and applied proposal bytes. Resume must revalidate and obtain a fresh independent review.".into();
        feature.review_status = "interrupted".into();
        feature.review_summary = feature.message.clone();
        feature.review_pending = None;
    }
    feature.cumulative_evidence_version = 1;
    Ok(())
}
fn hash(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

fn automatic_failure_prompt_json(
    proposal: &RepairEscalationProposal,
    failure_summary: &str,
) -> Result<String> {
    if proposal.source != "automatic_failure" {
        bail!("Only automatic repair proposals may bind automatic failure evidence");
    }
    let failure_summary = bounded_code_failure_summary(failure_summary)?;
    if hash(failure_summary.as_bytes()) != proposal.diagnosis_sha256 {
        bail!("Automatic repair failure evidence digest changed before inference");
    }
    Ok(serde_json::to_string(&json!({
        "sha256": proposal.diagnosis_sha256,
        "text": failure_summary,
    }))?)
}

fn validate_sha256(value: &str, label: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("Invalid {label}");
    }
    Ok(())
}
fn validate_repair_handoff(
    handoff: &ChatRepairHandoff,
    project: &str,
    chat_id: &str,
    request_id: &str,
    diagnosis_sha256: &str,
) -> Result<()> {
    if handoff.project != project
        || handoff.chat_id != chat_id
        || handoff.request_id != request_id
        || handoff.response_sha256 != diagnosis_sha256
        || hash(handoff.response.as_bytes()) != handoff.response_sha256
    {
        bail!("Selected chat diagnosis binding changed; refresh project chat");
    }
    Ok(())
}
fn repair_escalation_proposal_sha256(proposal: &RepairEscalationProposal) -> Result<String> {
    if proposal.source == "manual_chat" {
        #[derive(Serialize)]
        struct LegacyManualProposalDigest<'a> {
            proposal_id: &'a str,
            attempt: u32,
            feature_id: &'a str,
            feature_checkpoint: &'a str,
            binding_revision: u64,
            model_target: &'a str,
            model: &'a str,
            #[serde(skip_serializing_if = "Option::is_none")]
            chat_id: &'a Option<String>,
            chat_request_id: &'a str,
            chat_model_target: &'a str,
            chat_model: &'a str,
            diagnosis: &'a str,
            diagnosis_sha256: &'a str,
            status: &'a str,
            summary: &'a str,
            error: Option<&'a str>,
            files: &'a [RepairEscalationFile],
            protected_inputs: &'a std::collections::BTreeMap<String, String>,
            applied_paths: &'a [String],
            apply_request_id: Option<&'a str>,
        }
        return Ok(hash(&serde_json::to_vec(&LegacyManualProposalDigest {
            proposal_id: &proposal.proposal_id,
            attempt: proposal.attempt,
            feature_id: &proposal.feature_id,
            feature_checkpoint: &proposal.feature_checkpoint,
            binding_revision: 0,
            model_target: &proposal.model_target,
            model: &proposal.model,
            chat_id: &proposal.chat_id,
            chat_request_id: &proposal.chat_request_id,
            chat_model_target: &proposal.chat_model_target,
            chat_model: &proposal.chat_model,
            diagnosis: &proposal.diagnosis,
            diagnosis_sha256: &proposal.diagnosis_sha256,
            status: "",
            summary: &proposal.summary,
            error: None,
            files: &proposal.files,
            protected_inputs: &proposal.protected_inputs,
            applied_paths: &[],
            apply_request_id: None,
        })?));
    }
    let mut canonical = proposal.clone();
    canonical.binding_revision = 0;
    canonical.status.clear();
    canonical.error = None;
    canonical.applied_paths.clear();
    canonical.apply_request_id = None;
    canonical.review_slot_terminal = false;
    Ok(hash(&serde_json::to_vec(&canonical)?))
}
fn repair_escalation_candidate_sha256(proposal: &RepairEscalationProposal) -> Result<String> {
    Ok(hash(&serde_json::to_vec(&json!({
        "files": proposal.files,
    }))?))
}
fn model_target_name(id: &str) -> &str {
    match id {
        "mac" => "Mac",
        "windows" => "Windows",
        _ => "Selected",
    }
}
fn validate_model_id(id: &str) -> Result<()> {
    if id.trim().is_empty() || id.len() > 128 || id.chars().any(|character| character.is_control())
    {
        bail!("Model ID must be 1 to 128 non-control characters");
    }
    Ok(())
}
fn validate_model_url(value: &str) -> Result<()> {
    let url = reqwest::Url::parse(value).context("Model URL must be an absolute URL")?;
    let loopback = url
        .host_str()
        .map(|host| host.trim_start_matches('[').trim_end_matches(']'))
        .and_then(|host| host.parse::<std::net::IpAddr>().ok())
        .is_some_and(|address| address.is_loopback());
    if url.scheme() != "http"
        || !loopback
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        bail!(
            "Model URL must be credential-free HTTP with a literal loopback IP and no query or fragment"
        );
    }
    Ok(())
}
fn configured_model_targets(args: &Args) -> Result<Vec<ModelTarget>> {
    validate_model_url(&args.model_url)?;
    validate_model_id(&args.model)?;
    let mut targets = vec![ModelTarget {
        id: "mac",
        name: "Mac",
        url: args.model_url.clone(),
        model: args.model.clone(),
    }];
    match (&args.windows_model_url, &args.windows_model) {
        (Some(url), Some(model)) => {
            validate_model_url(url)?;
            validate_model_id(model)?;
            let mac_url = reqwest::Url::parse(&args.model_url)?;
            let windows_url = reqwest::Url::parse(url)?;
            if same_loopback_listener(&mac_url, &windows_url) {
                bail!("Mac and Windows model targets must use distinct loopback endpoints");
            }
            targets.push(ModelTarget {
                id: "windows",
                name: "Windows",
                url: url.clone(),
                model: model.clone(),
            });
        }
        (None, None) => {}
        _ => bail!("Windows model URL and model ID must be configured together"),
    }
    Ok(targets)
}
fn same_loopback_listener(left: &reqwest::Url, right: &reqwest::Url) -> bool {
    left.port_or_known_default() == right.port_or_known_default()
}
fn repair_sensitive_path(path: &str) -> bool {
    path.replace('\\', "/").split('/').any(|component| {
        let lower = component.to_ascii_lowercase();
        matches!(
            lower.as_str(),
            ".env"
                | "credentials"
                | "credentials.toml"
                | ".git-credentials"
                | ".netrc"
                | "id_rsa"
                | "id_ed25519"
        ) || lower.contains("credential")
            || lower.contains("secret")
            || lower.contains("password")
            || lower.ends_with(".pem")
            || lower.ends_with(".key")
            || lower.ends_with(".p12")
    })
}
fn validate_repair_file_admission(path: &str, content: &str) -> Result<()> {
    if repair_sensitive_path(path) {
        bail!("Repair file targets a credential-like or sensitive path");
    }
    validate_cloud_text(content).context("Repair file contains secret-shaped text")
}
fn checked_path(root: &Path, relative: &str) -> Result<PathBuf> {
    if relative.is_empty()
        || relative.contains('\\')
        || relative.contains(':')
        || relative.len() > 240
    {
        bail!("Invalid relative path");
    }
    let path = Path::new(relative);
    if path
        .components()
        .any(|c| !matches!(c, Component::Normal(_)))
    {
        bail!("File path escapes project");
    }
    let mut full = root.to_path_buf();
    for component in path.components() {
        let name = component.as_os_str().to_string_lossy();
        if name.starts_with('.') || ["node_modules", "target"].contains(&name.as_ref()) {
            bail!("Generated file targets reserved directory");
        }
        full.push(component);
        if full
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink())
        {
            bail!("Symbolic link is not a project file");
        }
        if full.exists() && !fs::canonicalize(&full)?.starts_with(root) {
            bail!("Path leaves project");
        }
    }
    Ok(full)
}
fn apply_edit(root: &Path, edit: &Edit) -> Result<()> {
    apply_edit_with_parent_opened(root, edit, None)
}

fn apply_edit_with_parent_opened(
    root: &Path,
    edit: &Edit,
    parent_opened: Option<&dyn Fn(&Path)>,
) -> Result<()> {
    validate_repair_file_admission(&edit.path, &edit.content)?;
    if edit.path.is_empty()
        || edit.path.contains('\\')
        || edit.path.contains(':')
        || edit.path.len() > 240
    {
        bail!("Invalid relative path");
    }
    let relative = Path::new(&edit.path);
    let mut components = Vec::new();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            bail!("File path escapes project");
        };
        let display = name.to_string_lossy();
        if display.starts_with('.') || ["node_modules", "target"].contains(&display.as_ref()) {
            bail!("Generated file targets reserved directory");
        }
        components.push(name.to_os_string());
    }
    let (leaf, parents) = components
        .split_last()
        .context("Repair path has no file leaf")?;

    #[cfg(unix)]
    {
        use std::ffi::CString;
        use std::io::{Seek as _, Write as _};
        use std::os::fd::{AsRawFd as _, FromRawFd as _};
        use std::os::unix::ffi::OsStrExt as _;
        use std::os::unix::fs::MetadataExt as _;

        fn open_directory_at(
            parent: &fs::File,
            name: &std::ffi::OsStr,
            create: bool,
        ) -> Result<fs::File> {
            use std::ffi::CString;
            use std::os::fd::{AsRawFd as _, FromRawFd as _};
            use std::os::unix::ffi::OsStrExt as _;

            let name = CString::new(name.as_bytes()).context("Repair path contains NUL")?;
            let flags = libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC;
            let mut descriptor = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
            if descriptor < 0
                && create
                && std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT)
            {
                if unsafe { libc::mkdirat(parent.as_raw_fd(), name.as_ptr(), 0o777) } != 0 {
                    return Err(std::io::Error::last_os_error().into());
                }
                descriptor = unsafe { libc::openat(parent.as_raw_fd(), name.as_ptr(), flags) };
            }
            if descriptor < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            let directory = unsafe { fs::File::from_raw_fd(descriptor) };
            if !directory.metadata()?.is_dir() {
                bail!("Repair path component is not a direct directory");
            }
            Ok(directory)
        }

        fn reopen_parent(root: &fs::File, parents: &[std::ffi::OsString]) -> Result<fs::File> {
            let mut directory = root.try_clone()?;
            for component in parents {
                directory = open_directory_at(&directory, component, false)?;
            }
            Ok(directory)
        }

        fn same_unix_identity(left: &fs::Metadata, right: &fs::Metadata) -> bool {
            left.dev() == right.dev() && left.ino() == right.ino()
        }

        let root_handle = hold_unix_recovery_root(root)?;
        let mut parent = root_handle.try_clone()?;
        for component in parents {
            parent = open_directory_at(&parent, component, true)?;
        }
        if let Some(hook) = parent_opened {
            hook(relative.parent().unwrap_or_else(|| Path::new("")));
        }
        let rebound_parent = reopen_parent(&root_handle, parents)?;
        if !same_unix_identity(&parent.metadata()?, &rebound_parent.metadata()?) {
            bail!("Repair parent changed before file application");
        }

        let leaf_c = CString::new(leaf.as_bytes()).context("Repair path contains NUL")?;
        let flags = libc::O_RDWR | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK;
        let mut descriptor = unsafe { libc::openat(parent.as_raw_fd(), leaf_c.as_ptr(), flags) };
        let mut created = false;
        if descriptor < 0 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ENOENT) {
            if edit.before.is_some() {
                bail!(
                    "{} changed after planning; preserve it and reconcile manually",
                    edit.path
                );
            }
            descriptor = unsafe {
                libc::openat(
                    parent.as_raw_fd(),
                    leaf_c.as_ptr(),
                    flags | libc::O_CREAT | libc::O_EXCL,
                    0o666,
                )
            };
            created = true;
        }
        if descriptor < 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let mut file = unsafe { fs::File::from_raw_fd(descriptor) };
        let metadata = file.metadata()?;
        if !metadata.is_file() || metadata.nlink() != 1 {
            bail!("Repair target is not a direct single-link file");
        }
        let mut current_bytes = Vec::new();
        file.read_to_end(&mut current_bytes)?;
        let current = if created {
            None
        } else {
            Some(hash(&current_bytes))
        };
        if current == Some(hash(edit.content.as_bytes())) {
            return Ok(());
        }
        if current != edit.before {
            bail!(
                "{} changed after planning; preserve it and reconcile manually",
                edit.path
            );
        }
        let rebound_parent = reopen_parent(&root_handle, parents)?;
        if !same_unix_identity(&parent.metadata()?, &rebound_parent.metadata()?) {
            bail!("Repair parent changed before file write");
        }
        let mut leaf_stat = std::mem::MaybeUninit::<libc::stat>::zeroed();
        if unsafe {
            libc::fstatat(
                parent.as_raw_fd(),
                leaf_c.as_ptr(),
                leaf_stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        let leaf_stat = unsafe { leaf_stat.assume_init() };
        if metadata.dev() != leaf_stat.st_dev as u64 || metadata.ino() != leaf_stat.st_ino {
            bail!("Repair target changed before file write");
        }
        file.seek(std::io::SeekFrom::Start(0))?;
        file.set_len(0)?;
        file.write_all(edit.content.as_bytes())?;
        file.sync_all()?;
        let rebound_parent = reopen_parent(&root_handle, parents)?;
        if !same_unix_identity(&parent.metadata()?, &rebound_parent.metadata()?) {
            bail!("Repair parent changed during file write");
        }
        Ok(())
    }

    #[cfg(windows)]
    {
        use std::io::{Seek as _, Write as _};
        use std::os::windows::fs::OpenOptionsExt as _;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };

        let mut guards = vec![hold_windows_recovery_directory(root)?];
        let mut parent_path = root.to_path_buf();
        for component in parents {
            parent_path.push(component);
            match fs::symlink_metadata(&parent_path) {
                Ok(metadata) => {
                    if planning_metadata_is_reparse(&metadata) || !metadata.is_dir() {
                        bail!("Repair path component is not a direct directory");
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    fs::create_dir(&parent_path)?;
                }
                Err(error) => return Err(error.into()),
            }
            guards.push(hold_windows_recovery_directory(&parent_path)?);
        }
        if let Some(hook) = parent_opened {
            hook(relative.parent().unwrap_or_else(|| Path::new("")));
        }
        let path = parent_path.join(leaf);
        let mut options = fs::OpenOptions::new();
        options
            .read(true)
            .write(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
        let (mut file, created) = match options.open(&path) {
            Ok(file) => (file, false),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if edit.before.is_some() {
                    bail!(
                        "{} changed after planning; preserve it and reconcile manually",
                        edit.path
                    );
                }
                options.create_new(true);
                (options.open(&path)?, true)
            }
            Err(error) => return Err(error.into()),
        };
        let metadata = file.metadata()?;
        if !metadata.is_file() || planning_metadata_is_reparse(&metadata) {
            bail!("Repair target is not a direct file");
        }
        ensure_windows_recovery_file_single_link(&file)?;
        let mut current_bytes = Vec::new();
        file.read_to_end(&mut current_bytes)?;
        let current = if created {
            None
        } else {
            Some(hash(&current_bytes))
        };
        if current == Some(hash(edit.content.as_bytes())) {
            return Ok(());
        }
        if current != edit.before {
            bail!(
                "{} changed after planning; preserve it and reconcile manually",
                edit.path
            );
        }
        file.seek(std::io::SeekFrom::Start(0))?;
        file.set_len(0)?;
        file.write_all(edit.content.as_bytes())?;
        file.sync_all()?;
        let _ = guards;
        Ok(())
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = (root, parent_opened, leaf, parents);
        bail!("Repair file application is unsupported on this host")
    }
}
fn validation_path_references(root: &Path, validation: &str) -> Result<Vec<String>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    for character in validation.chars() {
        if let Some(expected) = quote {
            if character == expected {
                quote = None;
            } else {
                current.push(character);
            }
        } else if matches!(character, '\'' | '"') {
            quote = Some(character);
        } else if character.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            current.push(character);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    let mut references = std::collections::BTreeSet::new();
    for token in tokens {
        let candidates = token
            .split_once('=')
            .map_or(vec![token.as_str()], |(_, value)| {
                vec![token.as_str(), value]
            });
        for candidate in candidates {
            let candidate = candidate
                .split("::")
                .next()
                .unwrap_or_default()
                .trim_matches(|character: char| {
                    matches!(character, ';' | ',' | '(' | ')' | '[' | ']' | '{' | '}')
                })
                .replace('\\', "/");
            let candidate = candidate.trim_start_matches("./");
            if candidate.is_empty() || candidate.starts_with('-') || candidate.contains(':') {
                continue;
            }
            let wildcard = candidate.find(['*', '?', '[']);
            let candidate = if let Some(index) = wildcard {
                let prefix = &candidate[..index];
                if prefix.ends_with('/') {
                    prefix.trim_end_matches('/')
                } else {
                    Path::new(prefix)
                        .parent()
                        .and_then(Path::to_str)
                        .unwrap_or_default()
                }
            } else {
                candidate
            };
            if candidate.is_empty() {
                continue;
            }
            let relative = Path::new(candidate);
            if relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
            {
                continue;
            }
            let mut full = root.to_path_buf();
            let mut has_link = false;
            for component in relative.components() {
                full.push(component);
                if full
                    .symlink_metadata()
                    .is_ok_and(|metadata| metadata.file_type().is_symlink())
                {
                    has_link = true;
                    break;
                }
            }
            if has_link {
                bail!("Validation input path contains a symbolic link");
            }
            if !full.exists() {
                continue;
            }
            let canonical = fs::canonicalize(&full)?;
            if !canonical.starts_with(root) {
                bail!("Validation input path leaves the project");
            }
            references.insert(candidate.replace('\\', "/").to_lowercase());
        }
    }
    Ok(references.into_iter().collect())
}
fn is_protected_repair_input(relative: &str, validation_paths: &[String]) -> bool {
    let normalized = relative.replace('\\', "/").to_lowercase();
    let parts: Vec<_> = normalized.split('/').collect();
    if parts[..parts.len().saturating_sub(1)].iter().any(|part| {
        matches!(
            *part,
            "test" | "tests" | "spec" | "specs" | "__tests__" | "testing"
        )
    }) {
        return true;
    }
    let filename = parts.last().copied().unwrap_or_default();
    if filename == "conftest.py" || filename.starts_with("test_helpers.") {
        return true;
    }
    let stem = filename.rsplit_once('.').map_or(filename, |(stem, _)| stem);
    if matches!(stem, "test" | "tests" | "spec" | "specs")
        || stem.starts_with("test_")
        || stem.ends_with("_test")
        || stem.ends_with("tests")
        || stem.ends_with("_spec")
        || stem.ends_with("specs")
        || filename.contains(".test.")
        || filename.contains(".spec.")
    {
        return true;
    }
    validation_paths.iter().any(|protected| {
        normalized == *protected || normalized.starts_with(&format!("{protected}/"))
    })
}

fn repair_forbidden_tool_paths(validation_paths: &[String]) -> Vec<String> {
    let mut paths = std::collections::BTreeSet::from([
        "test/**".to_owned(),
        "tests/**".to_owned(),
        "spec/**".to_owned(),
        "specs/**".to_owned(),
        "**/test/**".to_owned(),
        "**/tests/**".to_owned(),
        "**/spec/**".to_owned(),
        "**/specs/**".to_owned(),
        "**/__tests__/**".to_owned(),
        "**/test_*".to_owned(),
        "**/*_test.*".to_owned(),
        "**/*.test.*".to_owned(),
        "**/*_spec.*".to_owned(),
        "**/*.spec.*".to_owned(),
    ]);
    for path in validation_paths {
        paths.insert(path.clone());
        paths.insert(format!("{path}/**"));
    }
    paths.into_iter().collect()
}

fn repair_protected_inputs(
    root: &Path,
    validation_paths: &[String],
) -> Result<std::collections::HashMap<String, String>> {
    repair_protected_inputs_with_directory_opened(root, validation_paths, None)
}

const PROTECTED_REPAIR_INPUT_BYTE_LIMIT: usize = 64 * 1024 * 1024;

fn reserve_protected_input_bytes(bytes_seen: &mut usize, file_length: u64) -> Result<usize> {
    let length = usize::try_from(file_length)
        .context("Protected repair input is too large for this host")?;
    if length > PROTECTED_REPAIR_INPUT_BYTE_LIMIT {
        bail!("Protected repair input exceeds 64 MiB");
    }
    let next = bytes_seen
        .checked_add(length)
        .context("Protected repair input size overflow")?;
    if next > PROTECTED_REPAIR_INPUT_BYTE_LIMIT {
        bail!("Protected repair inputs exceed 64 MiB");
    }
    *bytes_seen = next;
    Ok(length)
}

fn repair_protected_inputs_with_directory_opened(
    root: &Path,
    validation_paths: &[String],
    directory_opened: Option<&dyn Fn(&Path)>,
) -> Result<std::collections::HashMap<String, String>> {
    // Shared bounded scan state: aggregate byte/file caps plus the admitted
    // protected-input digests.
    struct ProtectedInputScan {
        out: std::collections::HashMap<String, String>,
        bytes_seen: usize,
        files_seen: usize,
    }

    fn intersects_validation_path(relative: &str, validation_paths: &[String]) -> bool {
        let normalized = relative.replace('\\', "/").to_lowercase();
        validation_paths.iter().any(|protected| {
            normalized == *protected
                || normalized.starts_with(&format!("{protected}/"))
                || protected.starts_with(&format!("{normalized}/"))
        })
    }

    #[cfg(unix)]
    fn visit_unix(
        directory: &fs::File,
        relative_dir: &Path,
        validation_paths: &[String],
        scan: &mut ProtectedInputScan,
        directory_opened: Option<&dyn Fn(&Path)>,
        depth: usize,
    ) -> Result<()> {
        if depth > 12 {
            bail!("Protected repair input tree exceeds depth limit");
        }
        if let Some(hook) = directory_opened {
            hook(relative_dir);
        }
        for entry in unix_recovery_entries(directory)? {
            let name = entry.name.to_string_lossy().into_owned();
            let relative_path = relative_dir.join(&entry.name);
            let relative = relative_path.to_string_lossy().replace('\\', "/");
            match unix_recovery_entry_kind(&entry) {
                libc::S_IFLNK => continue,
                libc::S_IFDIR => {
                    let normally_skipped = name.starts_with('.')
                        || ["target", "node_modules", "__pycache__", "venv", "dist"]
                            .contains(&name.as_str());
                    if normally_skipped && !intersects_validation_path(&relative, validation_paths)
                    {
                        continue;
                    }
                    let (child, _) = open_unix_recovery_entry(directory, &entry, true)?;
                    visit_unix(
                        &child,
                        &relative_path,
                        validation_paths,
                        scan,
                        directory_opened,
                        depth + 1,
                    )?;
                }
                libc::S_IFREG => {
                    if !is_protected_repair_input(&relative, validation_paths) {
                        continue;
                    }
                    scan.files_seen += 1;
                    if scan.files_seen > 2000 {
                        bail!("Protected repair input count exceeds 2000 files");
                    }
                    let (mut file, held_metadata) =
                        open_unix_recovery_entry(directory, &entry, false)?;
                    let length =
                        reserve_protected_input_bytes(&mut scan.bytes_seen, held_metadata.len())?;
                    let mut content = Vec::with_capacity(length);
                    file.read_to_end(&mut content)?;
                    let final_metadata = file.metadata()?;
                    if content.len() != length
                        || final_metadata.len() != held_metadata.len()
                        || !final_metadata.is_file()
                    {
                        bail!("Protected repair input changed while reading");
                    }
                    scan.out.insert(relative.to_lowercase(), hash(&content));
                }
                _ => continue,
            }
        }
        Ok(())
    }

    #[cfg(windows)]
    fn visit(
        dir: &Path,
        relative_dir: &Path,
        validation_paths: &[String],
        scan: &mut ProtectedInputScan,
        directory_opened: Option<&dyn Fn(&Path)>,
        depth: usize,
    ) -> Result<()> {
        if depth > 12 {
            bail!("Protected repair input tree exceeds depth limit");
        }
        let _directory_guard = hold_windows_recovery_directory(dir)?;
        if let Some(hook) = directory_opened {
            hook(relative_dir);
        }
        let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<std::io::Result<_>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            let direct_metadata = fs::symlink_metadata(entry.path())?;
            if planning_metadata_is_reparse(&direct_metadata) {
                bail!("Protected repair input refuses a Windows reparse point");
            }
            let kind = direct_metadata.file_type();
            if kind.is_symlink() {
                continue;
            }
            let relative_path = relative_dir.join(entry.file_name());
            let relative = relative_path.to_string_lossy().replace('\\', "/");
            if kind.is_dir() {
                let normally_skipped = name.starts_with('.')
                    || ["target", "node_modules", "__pycache__", "venv", "dist"]
                        .contains(&name.as_str());
                if normally_skipped && !intersects_validation_path(&relative, validation_paths) {
                    continue;
                }
                visit(
                    &entry.path(),
                    &relative_path,
                    validation_paths,
                    scan,
                    directory_opened,
                    depth + 1,
                )?;
                continue;
            }
            if !kind.is_file() {
                continue;
            }
            if !is_protected_repair_input(&relative, validation_paths) {
                continue;
            }
            scan.files_seen += 1;
            if scan.files_seen > 2000 {
                bail!("Protected repair input count exceeds 2000 files");
            }
            let (mut file, held_metadata) = open_windows_recovery_file(&entry.path())?;
            let length = reserve_protected_input_bytes(&mut scan.bytes_seen, held_metadata.len())?;
            let mut content = Vec::with_capacity(length);
            file.read_to_end(&mut content)?;
            let final_metadata = file.metadata()?;
            if content.len() != length
                || final_metadata.len() != held_metadata.len()
                || !final_metadata.is_file()
                || planning_metadata_is_reparse(&final_metadata)
            {
                bail!("Protected repair input changed while reading");
            }
            scan.out.insert(relative.to_lowercase(), hash(&content));
        }
        Ok(())
    }
    let mut scan = ProtectedInputScan {
        out: std::collections::HashMap::new(),
        bytes_seen: 0,
        files_seen: 0,
    };
    #[cfg(unix)]
    {
        let root = hold_unix_recovery_root(root)?;
        visit_unix(
            &root,
            Path::new(""),
            validation_paths,
            &mut scan,
            directory_opened,
            0,
        )?;
    }
    #[cfg(windows)]
    visit(
        root,
        Path::new(""),
        validation_paths,
        &mut scan,
        directory_opened,
        0,
    )?;
    #[cfg(not(any(unix, windows)))]
    bail!("Protected repair input scanning is unsupported on this host");
    Ok(scan.out)
}
fn project_context(root: &Path) -> Result<Vec<Value>> {
    project_context_with_directory_opened(root, None)
}

fn project_context_with_directory_opened(
    root: &Path,
    directory_opened: Option<&dyn Fn(&Path)>,
) -> Result<Vec<Value>> {
    #[cfg(unix)]
    {
        project_context_unix(root, directory_opened)
    }

    #[cfg(windows)]
    {
        fn visit(
            dir: &Path,
            relative_dir: &Path,
            out: &mut Vec<Value>,
            budget: &mut usize,
            directory_opened: Option<&dyn Fn(&Path)>,
            depth: usize,
        ) -> Result<()> {
            if depth > 6 {
                return Ok(());
            }
            let _directory_guard = hold_windows_recovery_directory(dir)?;
            if let Some(hook) = directory_opened {
                hook(relative_dir);
            }
            let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<std::io::Result<_>>()?;
            entries.sort_by_key(|e| e.file_name());
            for entry in entries {
                if *budget >= 64000 || out.len() >= 80 {
                    break;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.starts_with('.')
                    || ["target", "node_modules", "__pycache__", "venv", "dist"]
                        .contains(&name.as_str())
                    || repair_sensitive_path(&name)
                {
                    continue;
                }
                let metadata = fs::symlink_metadata(entry.path())?;
                if planning_metadata_is_reparse(&metadata) {
                    bail!("Automatic repair project context refuses a Windows reparse point");
                }
                let kind = metadata.file_type();
                if kind.is_symlink() {
                    continue;
                }
                let relative = relative_dir.join(entry.file_name());
                if kind.is_dir() {
                    visit(
                        &entry.path(),
                        &relative,
                        out,
                        budget,
                        directory_opened,
                        depth + 1,
                    )?;
                } else if kind.is_file() {
                    let (mut file, held_metadata) = open_windows_recovery_file(&entry.path())?;
                    if held_metadata.len() > 16_000 {
                        continue;
                    }
                    let mut bytes = Vec::with_capacity(held_metadata.len() as usize);
                    file.by_ref().take(16_001).read_to_end(&mut bytes)?;
                    let final_metadata = file.metadata()?;
                    if bytes.len() as u64 != held_metadata.len()
                        || final_metadata.len() != held_metadata.len()
                        || !final_metadata.is_file()
                        || planning_metadata_is_reparse(&final_metadata)
                    {
                        bail!("Automatic repair project context file changed while reading");
                    }
                    let Ok(content) = String::from_utf8(bytes) else {
                        continue;
                    };
                    let relative = relative.to_string_lossy().replace('\\', "/");
                    if repair_sensitive_path(&relative) || validate_cloud_text(&content).is_err() {
                        continue;
                    }
                    *budget += content.len();
                    out.push(json!({"path":relative,"content":content}));
                }
            }
            Ok(())
        }
        let mut out = Vec::new();
        visit(root, Path::new(""), &mut out, &mut 0, directory_opened, 0)?;
        Ok(out)
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = (root, directory_opened);
        bail!("Automatic repair project context is unsupported on this host")
    }
}

struct AdmittedProjectSnapshot {
    files: std::collections::BTreeMap<String, (String, String)>,
    unadmitted_sha256: String,
    volatile_sha256: String,
}

// Redacted diagnostics: admitted content and excluded paths never enter Debug
// output, so a failed expect_err panic can only name counts and digests.
impl std::fmt::Debug for AdmittedProjectSnapshot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AdmittedProjectSnapshot")
            .field("admitted_file_count", &self.files.len())
            .field("unadmitted_sha256", &self.unadmitted_sha256)
            .field("volatile_sha256", &self.volatile_sha256)
            .finish()
    }
}

#[cfg(unix)]
struct UnixRecoveryEntry {
    name: std::ffi::OsString,
    name_c: std::ffi::CString,
    mode: libc::mode_t,
}

#[cfg(unix)]
fn unix_recovery_errno() -> *mut libc::c_int {
    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
    unsafe {
        libc::__error()
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "freebsd")))]
    unsafe {
        libc::__errno_location()
    }
}

#[cfg(unix)]
fn hold_unix_recovery_root(path: &Path) -> Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt as _;

    let mut options = fs::OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let directory = options.open(path)?;
    if !directory.metadata()?.is_dir() {
        bail!("Automatic repair recovery refuses a non-direct directory");
    }
    Ok(directory)
}

#[cfg(unix)]
fn unix_recovery_entries(directory: &fs::File) -> Result<Vec<UnixRecoveryEntry>> {
    use std::ffi::{CStr, CString, OsString};
    use std::os::fd::AsRawFd as _;
    use std::os::unix::ffi::OsStringExt as _;

    struct DirectoryStream(*mut libc::DIR);
    impl Drop for DirectoryStream {
        fn drop(&mut self) {
            unsafe {
                libc::closedir(self.0);
            }
        }
    }

    let duplicated = unsafe { libc::dup(directory.as_raw_fd()) };
    if duplicated < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let stream = unsafe { libc::fdopendir(duplicated) };
    if stream.is_null() {
        let error = std::io::Error::last_os_error();
        unsafe {
            libc::close(duplicated);
        }
        return Err(error.into());
    }
    let stream = DirectoryStream(stream);
    let mut entries = Vec::new();
    loop {
        unsafe {
            *unix_recovery_errno() = 0;
        }
        let raw = unsafe { libc::readdir(stream.0) };
        if raw.is_null() {
            let errno = unsafe { *unix_recovery_errno() };
            if errno != 0 {
                return Err(std::io::Error::from_raw_os_error(errno).into());
            }
            break;
        }
        let bytes = unsafe { CStr::from_ptr((*raw).d_name.as_ptr()) }.to_bytes();
        if bytes == b"." || bytes == b".." {
            continue;
        }
        let name_c = CString::new(bytes).context("Project entry name contains NUL")?;
        let mut stat = std::mem::MaybeUninit::<libc::stat>::zeroed();
        if unsafe {
            libc::fstatat(
                directory.as_raw_fd(),
                name_c.as_ptr(),
                stat.as_mut_ptr(),
                libc::AT_SYMLINK_NOFOLLOW,
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
        entries.push(UnixRecoveryEntry {
            name: OsString::from_vec(bytes.to_vec()),
            name_c,
            mode: unsafe { stat.assume_init() }.st_mode,
        });
    }
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(entries)
}

#[cfg(unix)]
fn unix_recovery_entry_kind(entry: &UnixRecoveryEntry) -> libc::mode_t {
    entry.mode & libc::S_IFMT
}

#[cfg(unix)]
fn open_unix_recovery_entry(
    directory: &fs::File,
    entry: &UnixRecoveryEntry,
    directory_required: bool,
) -> Result<(fs::File, fs::Metadata)> {
    use std::os::fd::{AsRawFd as _, FromRawFd as _};

    let mut flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK;
    if directory_required {
        flags |= libc::O_DIRECTORY;
    }
    let descriptor = unsafe { libc::openat(directory.as_raw_fd(), entry.name_c.as_ptr(), flags) };
    if descriptor < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let file = unsafe { fs::File::from_raw_fd(descriptor) };
    let metadata = file.metadata()?;
    if (directory_required && !metadata.is_dir()) || (!directory_required && !metadata.is_file()) {
        bail!("Automatic repair recovery entry changed type before it was opened");
    }
    if !directory_required {
        use std::os::unix::fs::MetadataExt as _;
        if metadata.nlink() != 1 {
            bail!("Automatic repair recovery refuses a file with multiple hard links");
        }
    }
    Ok((file, metadata))
}

#[cfg(windows)]
fn ensure_windows_recovery_file_single_link(file: &fs::File) -> Result<()> {
    use std::os::windows::io::AsRawHandle as _;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };

    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    if unsafe {
        GetFileInformationByHandle(
            file.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE,
            &mut information,
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    if information.nNumberOfLinks != 1 {
        bail!("Automatic repair recovery refuses a file with multiple hard links");
    }
    Ok(())
}

#[cfg(windows)]
fn open_windows_recovery_file(path: &Path) -> Result<(fs::File, fs::Metadata)> {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    let mut options = fs::OpenOptions::new();
    options
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || planning_metadata_is_reparse(&metadata) {
        bail!("Automatic repair recovery refuses a non-direct file");
    }
    ensure_windows_recovery_file_single_link(&file)?;
    Ok((file, metadata))
}

#[cfg(windows)]
fn hold_windows_recovery_directory(path: &Path) -> Result<fs::File> {
    use std::os::windows::fs::OpenOptionsExt as _;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    let mut options = fs::OpenOptions::new();
    options
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
    let directory = options.open(path)?;
    let metadata = directory.metadata()?;
    if !metadata.is_dir() || planning_metadata_is_reparse(&metadata) {
        bail!("Automatic repair recovery refuses a non-direct directory");
    }
    Ok(directory)
}

#[cfg(unix)]
fn read_unix_recovery_link(
    directory: &fs::File,
    entry: &UnixRecoveryEntry,
) -> Result<std::ffi::OsString> {
    use std::os::fd::AsRawFd as _;
    use std::os::unix::ffi::OsStringExt as _;

    let mut bytes = vec![0_u8; 64 * 1024];
    let length = unsafe {
        libc::readlinkat(
            directory.as_raw_fd(),
            entry.name_c.as_ptr(),
            bytes.as_mut_ptr().cast(),
            bytes.len(),
        )
    };
    if length < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let length = usize::try_from(length).context("Project symlink length is invalid")?;
    if length == bytes.len() {
        bail!("Project symlink target exceeds the recovery bound");
    }
    bytes.truncate(length);
    Ok(std::ffi::OsString::from_vec(bytes))
}

#[cfg(unix)]
fn project_context_unix(
    root: &Path,
    directory_opened: Option<&dyn Fn(&Path)>,
) -> Result<Vec<Value>> {
    fn visit(
        directory: &fs::File,
        relative_dir: &Path,
        out: &mut Vec<Value>,
        budget: &mut usize,
        directory_opened: Option<&dyn Fn(&Path)>,
        depth: usize,
    ) -> Result<()> {
        if depth > 6 {
            return Ok(());
        }
        if let Some(hook) = directory_opened {
            hook(relative_dir);
        }
        for entry in unix_recovery_entries(directory)? {
            if *budget >= 64_000 || out.len() >= 80 {
                break;
            }
            let name = entry.name.to_string_lossy().into_owned();
            if name.starts_with('.')
                || ["target", "node_modules", "__pycache__", "venv", "dist"]
                    .contains(&name.as_str())
                || repair_sensitive_path(&name)
            {
                continue;
            }
            let relative = relative_dir.join(&entry.name);
            match unix_recovery_entry_kind(&entry) {
                libc::S_IFLNK => continue,
                libc::S_IFDIR => {
                    let (child, _) = open_unix_recovery_entry(directory, &entry, true)?;
                    visit(&child, &relative, out, budget, directory_opened, depth + 1)?;
                }
                libc::S_IFREG => {
                    let (mut file, held_metadata) =
                        open_unix_recovery_entry(directory, &entry, false)?;
                    if held_metadata.len() > 16_000 {
                        continue;
                    }
                    let mut bytes = Vec::with_capacity(held_metadata.len() as usize);
                    file.by_ref().take(16_001).read_to_end(&mut bytes)?;
                    let final_metadata = file.metadata()?;
                    if bytes.len() as u64 != held_metadata.len()
                        || final_metadata.len() != held_metadata.len()
                        || !final_metadata.is_file()
                    {
                        bail!("Automatic repair project context file changed while reading");
                    }
                    let Ok(content) = String::from_utf8(bytes) else {
                        continue;
                    };
                    let relative = relative.to_string_lossy().replace('\\', "/");
                    if repair_sensitive_path(&relative) || validate_cloud_text(&content).is_err() {
                        continue;
                    }
                    *budget += content.len();
                    out.push(json!({"path":relative,"content":content}));
                }
                _ => continue,
            }
        }
        Ok(())
    }

    let root = hold_unix_recovery_root(root)?;
    let mut out = Vec::new();
    visit(&root, Path::new(""), &mut out, &mut 0, directory_opened, 0)?;
    Ok(out)
}

#[cfg(unix)]
fn admitted_project_snapshot_unix(
    root: &Path,
    cancellation: Option<&AtomicU8>,
    directory_opened: Option<&dyn Fn(&Path)>,
) -> Result<AdmittedProjectSnapshot> {
    // Complete bounded ledger for one recovery traversal. Admitted files
    // accumulate under the exact file/byte caps while every excluded or
    // unreviewable entry contributes only to the stable or volatile unadmitted
    // digest and the bounded count/byte totals, never to retained content.
    struct AdmittedSnapshotLedger<'a> {
        files: std::collections::BTreeMap<String, (String, String)>,
        admitted_bytes: usize,
        stable_unadmitted: Sha256,
        volatile_unadmitted: Sha256,
        unadmitted_count: usize,
        unadmitted_bytes: usize,
        cancellation: Option<&'a AtomicU8>,
        directory_opened: Option<&'a dyn Fn(&Path)>,
    }

    impl AdmittedSnapshotLedger<'_> {
        fn unadmitted(&mut self, stream: UnadmittedStream) -> &mut Sha256 {
            match stream {
                UnadmittedStream::Stable => &mut self.stable_unadmitted,
                UnadmittedStream::Volatile => &mut self.volatile_unadmitted,
            }
        }
    }

    // Selects which unadmitted digest absorbs an excluded entry: durable
    // project data binds the stable digest while caches and transient
    // directories bind the volatile digest.
    #[derive(Clone, Copy)]
    enum UnadmittedStream {
        Stable,
        Volatile,
    }

    fn bind_unadmitted(
        directory: &fs::File,
        entry: &UnixRecoveryEntry,
        relative: &Path,
        stream: UnadmittedStream,
        ledger: &mut AdmittedSnapshotLedger<'_>,
        depth: usize,
    ) -> Result<()> {
        if ledger
            .cancellation
            .is_some_and(|value| value.load(Ordering::SeqCst) != 0)
        {
            bail!("Automatic repair project inspection was cancelled");
        }
        if depth > 8 {
            bail!("Unadmitted project data exceeds the bounded recovery depth");
        }
        ledger.unadmitted_count = ledger
            .unadmitted_count
            .checked_add(1)
            .context("Unadmitted project file count overflow")?;
        if ledger.unadmitted_count > 100_000 {
            bail!("Project has too much unadmitted data for bounded recovery");
        }
        let relative_text = relative.to_string_lossy().replace('\\', "/");
        ledger
            .unadmitted(stream)
            .update(hash(relative_text.as_bytes()).as_bytes());
        match unix_recovery_entry_kind(entry) {
            libc::S_IFLNK => {
                let target = read_unix_recovery_link(directory, entry)?;
                ledger.unadmitted(stream).update(b"\0symlink\0");
                ledger
                    .unadmitted(stream)
                    .update(hash(target.to_string_lossy().as_bytes()).as_bytes());
            }
            libc::S_IFDIR => {
                ledger.unadmitted(stream).update(b"\0directory\0");
                let (child, _) = open_unix_recovery_entry(directory, entry, true)?;
                if let Some(hook) = ledger.directory_opened {
                    hook(relative);
                }
                for child_entry in unix_recovery_entries(&child)? {
                    bind_unadmitted(
                        &child,
                        &child_entry,
                        &relative.join(&child_entry.name),
                        stream,
                        ledger,
                        depth + 1,
                    )?;
                }
            }
            libc::S_IFREG => {
                let (mut file, held_metadata) = open_unix_recovery_entry(directory, entry, false)?;
                let byte_len = usize::try_from(held_metadata.len())
                    .context("Unadmitted project file is too large for this host")?;
                ledger.unadmitted_bytes = ledger
                    .unadmitted_bytes
                    .checked_add(byte_len)
                    .context("Unadmitted project byte count overflow")?;
                if ledger.unadmitted_bytes > 4 * 1024 * 1024 * 1024usize {
                    bail!("Project has too much unadmitted data for bounded recovery");
                }
                let mut file_digest = Sha256::new();
                let mut buffer = [0_u8; 64 * 1024];
                let mut bytes_read = 0_u64;
                loop {
                    if ledger
                        .cancellation
                        .is_some_and(|value| value.load(Ordering::SeqCst) != 0)
                    {
                        bail!("Automatic repair project inspection was cancelled");
                    }
                    let read = file.read(&mut buffer)?;
                    if read == 0 {
                        break;
                    }
                    bytes_read = bytes_read
                        .checked_add(read as u64)
                        .context("Unadmitted project byte count overflow")?;
                    file_digest.update(&buffer[..read]);
                }
                let final_metadata = file.metadata()?;
                if bytes_read != held_metadata.len()
                    || final_metadata.len() != held_metadata.len()
                    || !final_metadata.is_file()
                {
                    bail!("Automatic repair recovery file changed while reading");
                }
                ledger.unadmitted(stream).update(b"\0file\0");
                ledger
                    .unadmitted(stream)
                    .update(format!("{:x}", file_digest.finalize()).as_bytes());
            }
            _ => bail!("Automatic repair recovery refuses a special project entry"),
        }
        Ok(())
    }

    fn visit(
        directory: &fs::File,
        relative_dir: &Path,
        ledger: &mut AdmittedSnapshotLedger<'_>,
        depth: usize,
    ) -> Result<()> {
        if ledger
            .cancellation
            .is_some_and(|value| value.load(Ordering::SeqCst) != 0)
        {
            bail!("Automatic repair project inspection was cancelled");
        }
        if depth > 6 {
            bail!("Project exceeds the bounded Auto AI repair recovery depth");
        }
        for entry in unix_recovery_entries(directory)? {
            if ledger
                .cancellation
                .is_some_and(|value| value.load(Ordering::SeqCst) != 0)
            {
                bail!("Automatic repair project inspection was cancelled");
            }
            let relative = relative_dir.join(&entry.name);
            let name = entry.name.to_string_lossy().into_owned();
            let sensitive_name = repair_sensitive_path(&name);
            let kind = unix_recovery_entry_kind(&entry);
            if ["target", "node_modules", "__pycache__", "venv", "dist"].contains(&name.as_str()) {
                bind_unadmitted(
                    directory,
                    &entry,
                    &relative,
                    UnadmittedStream::Volatile,
                    ledger,
                    depth,
                )?;
                continue;
            }
            if name == ".git" {
                bind_unadmitted(
                    directory,
                    &entry,
                    &relative,
                    UnadmittedStream::Stable,
                    ledger,
                    depth,
                )?;
                continue;
            }
            if name.starts_with('.') || sensitive_name || kind == libc::S_IFLNK {
                bind_unadmitted(
                    directory,
                    &entry,
                    &relative,
                    UnadmittedStream::Stable,
                    ledger,
                    depth,
                )?;
                continue;
            }
            if kind == libc::S_IFDIR {
                let (child, _) = open_unix_recovery_entry(directory, &entry, true)?;
                if let Some(hook) = ledger.directory_opened {
                    hook(&relative);
                }
                visit(&child, &relative, ledger, depth + 1)?;
                continue;
            }
            if kind != libc::S_IFREG {
                bind_unadmitted(
                    directory,
                    &entry,
                    &relative,
                    UnadmittedStream::Stable,
                    ledger,
                    depth,
                )?;
                continue;
            }
            let relative_text = relative.to_string_lossy().replace('\\', "/");
            let (mut file, held_metadata) = open_unix_recovery_entry(directory, &entry, false)?;
            let mut bytes = Vec::with_capacity(
                usize::try_from(held_metadata.len().min(16_001))
                    .context("Project recovery file is too large for this host")?,
            );
            file.by_ref().take(16_001).read_to_end(&mut bytes)?;
            let final_metadata = file.metadata()?;
            if final_metadata.len() != held_metadata.len() || !final_metadata.is_file() {
                bail!("Automatic repair recovery file changed while reading");
            }
            let content = String::from_utf8(bytes.clone()).ok();
            let admitted = !repair_sensitive_path(&relative_text)
                && held_metadata.len() <= 16_000
                && bytes.len() as u64 == held_metadata.len()
                && content
                    .as_deref()
                    .is_some_and(|content| validate_cloud_text(content).is_ok());
            if admitted {
                if ledger.files.len() >= 80 {
                    bail!("Project exceeds the bounded Auto AI repair recovery file count");
                }
                ledger.admitted_bytes = ledger
                    .admitted_bytes
                    .checked_add(bytes.len())
                    .context("Project recovery byte count overflow")?;
                if ledger.admitted_bytes > 64_000 {
                    bail!("Project exceeds the bounded Auto AI repair recovery byte count");
                }
                let content = content.unwrap();
                ledger
                    .files
                    .insert(relative_text, (hash(content.as_bytes()), content));
            } else {
                bind_unadmitted(
                    directory,
                    &entry,
                    &relative,
                    UnadmittedStream::Stable,
                    ledger,
                    depth,
                )?;
            }
        }
        Ok(())
    }

    let root = hold_unix_recovery_root(root)?;
    let mut ledger = AdmittedSnapshotLedger {
        files: std::collections::BTreeMap::new(),
        admitted_bytes: 0,
        stable_unadmitted: Sha256::new(),
        volatile_unadmitted: Sha256::new(),
        unadmitted_count: 0,
        unadmitted_bytes: 0,
        cancellation,
        directory_opened,
    };
    ledger
        .stable_unadmitted
        .update(b"assemblywright.auto-repair-stable-unadmitted.v2\0");
    ledger
        .volatile_unadmitted
        .update(b"assemblywright.auto-repair-volatile-unadmitted.v2\0");
    visit(&root, Path::new(""), &mut ledger, 0)?;
    Ok(AdmittedProjectSnapshot {
        files: ledger.files,
        unadmitted_sha256: format!("{:x}", ledger.stable_unadmitted.finalize()),
        volatile_sha256: format!("{:x}", ledger.volatile_unadmitted.finalize()),
    })
}

/// Captures the complete bounded set that can enter limit-recovery review.
/// Content excluded because it is sensitive, binary, oversized, or otherwise
/// unreviewable contributes only to a local digest, so restricted bytes and
/// paths never become durable state or cloud input.
#[cfg(test)]
fn admitted_project_snapshot(root: &Path) -> Result<AdmittedProjectSnapshot> {
    admitted_project_snapshot_with_cancellation(root, None)
}

fn admitted_project_snapshot_with_cancellation(
    root: &Path,
    cancellation: Option<&AtomicU8>,
) -> Result<AdmittedProjectSnapshot> {
    #[cfg(unix)]
    {
        admitted_project_snapshot_unix(root, cancellation, None)
    }

    #[cfg(not(unix))]
    {
        fn open_direct_recovery_file(path: &Path) -> Result<(fs::File, fs::Metadata)> {
            let mut options = fs::OpenOptions::new();
            options.read(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt as _;
                // O_NONBLOCK prevents a regular-file-to-FIFO substitution from
                // hanging recovery before the held-handle metadata check rejects it.
                options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt as _;
                use windows_sys::Win32::Storage::FileSystem::{
                    FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, FILE_SHARE_WRITE,
                };
                options
                    .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                    .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
            }
            let file = options.open(path)?;
            let metadata = file.metadata()?;
            if !metadata.is_file() || planning_metadata_is_reparse(&metadata) {
                bail!("Automatic repair recovery refuses a non-direct file");
            }
            #[cfg(windows)]
            ensure_windows_recovery_file_single_link(&file)?;
            Ok((file, metadata))
        }

        #[cfg(windows)]
        fn hold_direct_recovery_directory(path: &Path) -> Result<fs::File> {
            use std::os::windows::fs::OpenOptionsExt as _;
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
                FILE_SHARE_WRITE,
            };

            let mut options = fs::OpenOptions::new();
            options
                .read(true)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                // Excluding delete sharing keeps the checked directory from being
                // renamed or replaced while read_dir resolves the same path.
                .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE);
            let directory = options.open(path)?;
            let metadata = directory.metadata()?;
            if !metadata.is_dir() || planning_metadata_is_reparse(&metadata) {
                bail!("Automatic repair recovery refuses a non-direct directory");
            }
            Ok(directory)
        }

        #[cfg(not(windows))]
        fn validate_direct_recovery_directory(path: &Path) -> Result<()> {
            let metadata = fs::symlink_metadata(path)?;
            if metadata.file_type().is_symlink()
                || planning_metadata_is_reparse(&metadata)
                || !metadata.is_dir()
            {
                bail!("Automatic repair recovery refuses a non-direct directory");
            }
            Ok(())
        }

        fn bind_unadmitted_entry(
            root: &Path,
            path: &Path,
            unadmitted: &mut Sha256,
            unadmitted_count: &mut usize,
            unadmitted_bytes: &mut usize,
            cancellation: Option<&AtomicU8>,
            depth: usize,
        ) -> Result<()> {
            if cancellation.is_some_and(|value| value.load(Ordering::SeqCst) != 0) {
                bail!("Automatic repair project inspection was cancelled");
            }
            if depth > 8 {
                bail!("Unadmitted project data exceeds the bounded recovery depth");
            }
            *unadmitted_count = unadmitted_count
                .checked_add(1)
                .context("Unadmitted project file count overflow")?;
            if *unadmitted_count > 100_000 {
                bail!("Project has too much unadmitted data for bounded recovery");
            }
            let relative = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            let metadata = fs::symlink_metadata(path)?;
            unadmitted.update(hash(relative.as_bytes()).as_bytes());
            if planning_metadata_is_reparse(&metadata) {
                bail!("Automatic repair recovery refuses a Windows reparse point");
            }
            if metadata.file_type().is_symlink() {
                let target = fs::read_link(path)?;
                unadmitted.update(b"\0symlink\0");
                unadmitted.update(hash(target.to_string_lossy().as_bytes()).as_bytes());
                return Ok(());
            }
            if metadata.is_dir() {
                unadmitted.update(b"\0directory\0");
                #[cfg(windows)]
                let _directory_guard = hold_direct_recovery_directory(path)?;
                #[cfg(not(windows))]
                validate_direct_recovery_directory(path)?;
                let mut entries: Vec<_> = fs::read_dir(path)?.collect::<std::io::Result<_>>()?;
                entries.sort_by_key(|entry| entry.file_name());
                for entry in entries {
                    bind_unadmitted_entry(
                        root,
                        &entry.path(),
                        unadmitted,
                        unadmitted_count,
                        unadmitted_bytes,
                        cancellation,
                        depth + 1,
                    )?;
                }
                return Ok(());
            }
            if metadata.is_file() {
                let (mut file, held_metadata) = open_direct_recovery_file(path)?;
                let byte_len = usize::try_from(held_metadata.len())
                    .context("Unadmitted project file is too large for this host")?;
                *unadmitted_bytes = unadmitted_bytes
                    .checked_add(byte_len)
                    .context("Unadmitted project byte count overflow")?;
                if *unadmitted_bytes > 4 * 1024 * 1024 * 1024usize {
                    bail!("Project has too much unadmitted data for bounded recovery");
                }
                let mut file_digest = Sha256::new();
                let mut buffer = [0_u8; 64 * 1024];
                let mut bytes_read = 0_u64;
                loop {
                    if cancellation.is_some_and(|value| value.load(Ordering::SeqCst) != 0) {
                        bail!("Automatic repair project inspection was cancelled");
                    }
                    let read = file.read(&mut buffer)?;
                    if read == 0 {
                        break;
                    }
                    bytes_read = bytes_read
                        .checked_add(read as u64)
                        .context("Unadmitted project byte count overflow")?;
                    file_digest.update(&buffer[..read]);
                }
                let final_metadata = file.metadata()?;
                if bytes_read != held_metadata.len()
                    || final_metadata.len() != held_metadata.len()
                    || !final_metadata.is_file()
                    || planning_metadata_is_reparse(&final_metadata)
                {
                    bail!("Automatic repair recovery file changed while reading");
                }
                unadmitted.update(b"\0file\0");
                unadmitted.update(format!("{:x}", file_digest.finalize()).as_bytes());
                return Ok(());
            }
            bail!("Automatic repair recovery refuses a special project entry")
        }

        #[allow(clippy::too_many_arguments)]
        fn visit(
            root: &Path,
            dir: &Path,
            files: &mut std::collections::BTreeMap<String, (String, String)>,
            admitted_bytes: &mut usize,
            stable_unadmitted: &mut Sha256,
            volatile_unadmitted: &mut Sha256,
            unadmitted_count: &mut usize,
            unadmitted_bytes: &mut usize,
            cancellation: Option<&AtomicU8>,
            depth: usize,
        ) -> Result<()> {
            if cancellation.is_some_and(|value| value.load(Ordering::SeqCst) != 0) {
                bail!("Automatic repair project inspection was cancelled");
            }
            if depth > 6 {
                bail!("Project exceeds the bounded Auto AI repair recovery depth");
            }
            #[cfg(windows)]
            let _directory_guard = hold_direct_recovery_directory(dir)?;
            #[cfg(not(windows))]
            validate_direct_recovery_directory(dir)?;
            let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<std::io::Result<_>>()?;
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                if cancellation.is_some_and(|value| value.load(Ordering::SeqCst) != 0) {
                    bail!("Automatic repair project inspection was cancelled");
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                let sensitive_name = repair_sensitive_path(&name);
                let direct_metadata = fs::symlink_metadata(entry.path())?;
                if planning_metadata_is_reparse(&direct_metadata) {
                    bail!("Automatic repair recovery refuses a Windows reparse point");
                }
                let kind = direct_metadata.file_type();
                if ["target", "node_modules", "__pycache__", "venv", "dist"]
                    .contains(&name.as_str())
                {
                    bind_unadmitted_entry(
                        root,
                        &entry.path(),
                        volatile_unadmitted,
                        unadmitted_count,
                        unadmitted_bytes,
                        cancellation,
                        depth,
                    )?;
                    continue;
                }
                if name == ".git" {
                    bind_unadmitted_entry(
                        root,
                        &entry.path(),
                        stable_unadmitted,
                        unadmitted_count,
                        unadmitted_bytes,
                        cancellation,
                        depth,
                    )?;
                    continue;
                }
                if name.starts_with('.') || sensitive_name || kind.is_symlink() {
                    bind_unadmitted_entry(
                        root,
                        &entry.path(),
                        stable_unadmitted,
                        unadmitted_count,
                        unadmitted_bytes,
                        cancellation,
                        depth,
                    )?;
                    continue;
                }
                if kind.is_dir() {
                    visit(
                        root,
                        &entry.path(),
                        files,
                        admitted_bytes,
                        stable_unadmitted,
                        volatile_unadmitted,
                        unadmitted_count,
                        unadmitted_bytes,
                        cancellation,
                        depth + 1,
                    )?;
                    continue;
                }
                if !kind.is_file() {
                    bind_unadmitted_entry(
                        root,
                        &entry.path(),
                        stable_unadmitted,
                        unadmitted_count,
                        unadmitted_bytes,
                        cancellation,
                        depth,
                    )?;
                    continue;
                }
                let relative = entry
                    .path()
                    .strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/");
                let (mut file, held_metadata) = open_direct_recovery_file(&entry.path())?;
                let mut bytes = Vec::with_capacity(
                    usize::try_from(held_metadata.len().min(16_001))
                        .context("Project recovery file is too large for this host")?,
                );
                file.by_ref().take(16_001).read_to_end(&mut bytes)?;
                let final_metadata = file.metadata()?;
                if final_metadata.len() != held_metadata.len()
                    || !final_metadata.is_file()
                    || planning_metadata_is_reparse(&final_metadata)
                {
                    bail!("Automatic repair recovery file changed while reading");
                }
                let content = String::from_utf8(bytes.clone()).ok();
                let admitted = !repair_sensitive_path(&relative)
                    && held_metadata.len() <= 16_000
                    && bytes.len() as u64 == held_metadata.len()
                    && content
                        .as_deref()
                        .is_some_and(|content| validate_cloud_text(content).is_ok());
                if admitted {
                    if files.len() >= 80 {
                        bail!("Project exceeds the bounded Auto AI repair recovery file count");
                    }
                    *admitted_bytes = admitted_bytes
                        .checked_add(bytes.len())
                        .context("Project recovery byte count overflow")?;
                    if *admitted_bytes > 64_000 {
                        bail!("Project exceeds the bounded Auto AI repair recovery byte count");
                    }
                    let content = content.unwrap();
                    files.insert(relative, (hash(content.as_bytes()), content));
                } else {
                    bind_unadmitted_entry(
                        root,
                        &entry.path(),
                        stable_unadmitted,
                        unadmitted_count,
                        unadmitted_bytes,
                        cancellation,
                        depth,
                    )?;
                }
            }
            Ok(())
        }

        let mut files = std::collections::BTreeMap::new();
        let mut stable_unadmitted = Sha256::new();
        let mut volatile_unadmitted = Sha256::new();
        let mut admitted_bytes = 0;
        let mut unadmitted_count = 0;
        let mut unadmitted_bytes = 0;
        stable_unadmitted.update(b"assemblywright.auto-repair-stable-unadmitted.v2\0");
        volatile_unadmitted.update(b"assemblywright.auto-repair-volatile-unadmitted.v2\0");
        visit(
            root,
            root,
            &mut files,
            &mut admitted_bytes,
            &mut stable_unadmitted,
            &mut volatile_unadmitted,
            &mut unadmitted_count,
            &mut unadmitted_bytes,
            cancellation,
            0,
        )?;
        Ok(AdmittedProjectSnapshot {
            files,
            unadmitted_sha256: format!("{:x}", stable_unadmitted.finalize()),
            volatile_sha256: format!("{:x}", volatile_unadmitted.finalize()),
        })
    }
}
fn authorize(engine: &Engine, headers: &HeaderMap) -> Result<()> {
    if headers.get("authorization").and_then(|h| h.to_str().ok())
        != Some(format!("Bearer {}", engine.token).as_str())
    {
        bail!("Unauthorized");
    }
    Ok(())
}

fn collect_planning_context(
    root: &Path,
    project: &str,
    cancellation: Option<&AtomicU8>,
) -> Result<(Vec<PlanningContextFile>, usize)> {
    developer_planning::validate_project(project)?;
    let path = root.join(project);
    if !path.exists() {
        return Ok((Vec::new(), 0));
    }
    let metadata = fs::symlink_metadata(&path)?;
    if metadata.file_type().is_symlink()
        || planning_metadata_is_reparse(&metadata)
        || !metadata.is_dir()
    {
        bail!("Planning project is not a direct directory");
    }
    let canonical = fs::canonicalize(&path)?;
    if !canonical.starts_with(root) {
        bail!("Planning project leaves the configured workspace");
    }
    fn visit(
        root: &Path,
        dir: &Path,
        scan: &mut (Vec<PlanningContextFile>, usize, usize, usize),
        cancellation: Option<&AtomicU8>,
        depth: usize,
    ) -> Result<()> {
        if depth > 6 {
            scan.2 += 1;
            return Ok(());
        }
        let mut entries = Vec::new();
        for entry in fs::read_dir(dir)? {
            if cancellation.is_some_and(|value| value.load(Ordering::SeqCst) != 0) {
                bail!("Planning context collection was stopped");
            }
            if entries.len() >= 4_096 {
                scan.2 += 1;
                return Ok(());
            }
            entries.push(entry?);
        }
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            scan.3 += 1;
            if scan.3 > 4_096 {
                scan.2 += 1;
                break;
            }
            if cancellation.is_some_and(|value| value.load(Ordering::SeqCst) != 0) {
                bail!("Planning context collection was stopped");
            }
            if scan.0.len() >= 64 || scan.1 >= 512 * 1024 {
                scan.2 += 1;
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let lower = name.to_ascii_lowercase();
            if name.starts_with('.')
                || ["target", "node_modules", "__pycache__", "venv", "dist"]
                    .contains(&lower.as_str())
                || lower.contains("credential")
                || lower.contains("secret")
                || lower.contains("password")
                || lower.ends_with(".pem")
                || lower.ends_with(".key")
                || lower.ends_with(".p12")
            {
                scan.2 += 1;
                continue;
            }
            let kind = entry.file_type()?;
            let direct_metadata = fs::symlink_metadata(entry.path())?;
            if kind.is_symlink() || planning_metadata_is_reparse(&direct_metadata) {
                scan.2 += 1;
                continue;
            }
            if kind.is_dir() {
                let child = fs::canonicalize(entry.path())?;
                if !child.starts_with(root) {
                    scan.2 += 1;
                    continue;
                }
                visit(root, &child, scan, cancellation, depth + 1)?;
            } else if kind.is_file() {
                let length = entry.metadata()?.len();
                if length > 32 * 1024 {
                    scan.2 += 1;
                    continue;
                }
                match read_planning_context_file(&entry.path()) {
                    Ok(content) if scan.1 + content.len() <= 512 * 1024 => {
                        scan.1 += content.len();
                        scan.0.push(PlanningContextFile {
                            path: entry
                                .path()
                                .strip_prefix(root)?
                                .to_string_lossy()
                                .replace('\\', "/"),
                            content,
                        });
                    }
                    _ => scan.2 += 1,
                }
            }
        }
        Ok(())
    }
    let mut scan = (Vec::new(), 0, 0, 0);
    visit(&canonical, &canonical, &mut scan, cancellation, 0)?;
    Ok((scan.0, scan.2))
}

fn planning_metadata_is_reparse(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        metadata.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
    }
    #[cfg(not(windows))]
    {
        let _ = metadata;
        false
    }
}

fn read_planning_context_file(path: &Path) -> Result<String> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > 32 * 1024 {
        bail!("Planning context leaf is not a bounded direct file");
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        if metadata.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            bail!("Planning context leaf is a reparse point");
        }
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(32 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 != metadata.len() {
        bail!("Planning context leaf changed while reading");
    }
    Ok(String::from_utf8(bytes)?)
}
type Api = (StatusCode, Json<Value>);
fn api(result: Result<Value>) -> Api {
    match result {
        Ok(value) => (StatusCode::OK, Json(value)),
        Err(error) => (
            StatusCode::CONFLICT,
            Json(json!({"error":error.to_string()})),
        ),
    }
}
async fn status(State(engine): State<Arc<Engine>>, headers: HeaderMap) -> Api {
    if authorize(&engine, &headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Unauthorized"})),
        );
    }
    api(engine
        .reconcile_completed_tool_mutations()
        .and_then(|_| engine.snapshot()))
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum GithubSetupMutation {
    RefreshAccount {
        expected_revision: u64,
    },
    ListRepositories {
        page: u32,
        expected_revision: u64,
    },
    BeginSignIn {
        operation_id: String,
        expected_revision: u64,
    },
    CancelSignIn {
        operation_id: String,
        expected_revision: u64,
    },
    ReconcileSignIn {
        operation_id: String,
        expected_revision: u64,
    },
    CreateRepository {
        operation_id: String,
        expected_login: String,
        name: String,
        visibility: String,
        expected_revision: u64,
    },
    ReconcileCreation {
        operation_id: String,
        expected_revision: u64,
    },
}

async fn github_status(State(engine): State<Arc<Engine>>, headers: HeaderMap) -> Api {
    if authorize(&engine, &headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Unauthorized"})),
        );
    }
    api(engine.github_setup_snapshot())
}

async fn github_control(
    State(engine): State<Arc<Engine>>,
    headers: HeaderMap,
    Json(raw): Json<Value>,
) -> Api {
    if authorize(&engine, &headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Unauthorized"})),
        );
    }
    let request = match serde_json::from_value::<GithubSetupMutation>(raw) {
        Ok(request) => request,
        Err(error) => return api(Err(error.into())),
    };
    let result = match request {
        GithubSetupMutation::RefreshAccount { expected_revision } => {
            engine.refresh_github_account(expected_revision).await
        }
        GithubSetupMutation::ListRepositories {
            page,
            expected_revision,
        } => {
            engine
                .list_github_repositories(page, expected_revision)
                .await
        }
        GithubSetupMutation::BeginSignIn {
            operation_id,
            expected_revision,
        } => engine.begin_github_sign_in(&operation_id, expected_revision),
        GithubSetupMutation::CancelSignIn {
            operation_id,
            expected_revision,
        } => {
            engine
                .cancel_github_sign_in(&operation_id, expected_revision)
                .await
        }
        GithubSetupMutation::ReconcileSignIn {
            operation_id,
            expected_revision,
        } => {
            engine
                .reconcile_github_sign_in(&operation_id, expected_revision)
                .await
        }
        GithubSetupMutation::CreateRepository {
            operation_id,
            expected_login,
            name,
            visibility,
            expected_revision,
        } => engine.begin_github_repository_creation(
            &operation_id,
            &expected_login,
            &name,
            &visibility,
            expected_revision,
        ),
        GithubSetupMutation::ReconcileCreation {
            operation_id,
            expected_revision,
        } => {
            engine
                .reconcile_github_creation(&operation_id, expected_revision)
                .await
        }
    };
    api(result)
}

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
enum PublicationMutation {
    SaveConnection {
        project: String,
        repository_url: String,
        base_branch: String,
        expected_revision: u64,
    },
    Disconnect {
        project: String,
        expected_revision: u64,
    },
    Reconcile {
        feature_id: String,
        expected_revision: u64,
        expected_checkpoint: String,
    },
}

async fn publication_status(State(engine): State<Arc<Engine>>, headers: HeaderMap) -> Api {
    if authorize(&engine, &headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Unauthorized"})),
        );
    }
    api(engine.snapshot())
}

async fn publication_control(
    State(engine): State<Arc<Engine>>,
    headers: HeaderMap,
    Json(raw): Json<Value>,
) -> Api {
    if authorize(&engine, &headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Unauthorized"})),
        );
    }
    let request = match serde_json::from_value::<PublicationMutation>(raw) {
        Ok(request) => request,
        Err(error) => return api(Err(error.into())),
    };
    let result = match request {
        PublicationMutation::SaveConnection {
            project,
            repository_url,
            base_branch,
            expected_revision,
        } => {
            engine
                .save_github_connection(&project, &repository_url, &base_branch, expected_revision)
                .await
        }
        PublicationMutation::Disconnect {
            project,
            expected_revision,
        } => engine.disconnect_github_connection(&project, expected_revision),
        PublicationMutation::Reconcile {
            feature_id,
            expected_revision,
            expected_checkpoint,
        } => engine.reconcile_publication(&feature_id, expected_revision, &expected_checkpoint),
    };
    api(result)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsMutation {
    expected_revision: u64,
    orchestrator: AiSelection,
    reviewer: AiSelection,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AutoAiRepairMutation {
    enabled: bool,
    max_escalations: u32,
    expected_revision: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FeatureReviewerMutation {
    id: String,
    expected_revision: u64,
    expected_checkpoint: String,
    expected_model: String,
    expected_reasoning_effort: String,
    reviewer: AiSelection,
}

async fn settings_status(State(engine): State<Arc<Engine>>, headers: HeaderMap) -> Api {
    status(State(engine), headers).await
}

async fn settings_control(
    State(engine): State<Arc<Engine>>,
    headers: HeaderMap,
    Json(request): Json<SettingsMutation>,
) -> Api {
    if authorize(&engine, &headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Unauthorized"})),
        );
    }
    api(engine.update_settings(
        request.expected_revision,
        request.orchestrator,
        request.reviewer,
    ))
}

async fn auto_ai_repair_control(
    State(engine): State<Arc<Engine>>,
    headers: HeaderMap,
    Json(request): Json<AutoAiRepairMutation>,
) -> Api {
    if authorize(&engine, &headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Unauthorized"})),
        );
    }
    api(engine.update_auto_ai_repair(
        request.expected_revision,
        request.enabled,
        request.max_escalations,
    ))
}

async fn feature_reviewer_control(
    State(engine): State<Arc<Engine>>,
    headers: HeaderMap,
    Json(request): Json<FeatureReviewerMutation>,
) -> Api {
    if authorize(&engine, &headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Unauthorized"})),
        );
    }
    api(engine.update_feature_reviewer(
        &request.id,
        request.expected_revision,
        &request.expected_checkpoint,
        &request.expected_model,
        &request.expected_reasoning_effort,
        request.reviewer,
    ))
}

#[derive(Deserialize)]
struct ChatQuery {
    project: String,
    chat_id: Option<String>,
    before: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChatConversationQuery {
    project: Option<String>,
    cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChatConversationCreate {
    id: String,
    project: String,
    reuse_chat_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChatConversationRename {
    project: String,
    chat_id: String,
    title: String,
    expected_revision: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepairEscalationMutation {
    action: String,
    feature_id: String,
    expected_revision: u64,
    expected_checkpoint: Option<String>,
    model_target: Option<String>,
    chat_id: Option<String>,
    chat_request_id: Option<String>,
    diagnosis_sha256: Option<String>,
    proposal_id: Option<String>,
}

#[derive(Deserialize)]
struct RepairEscalationQuery {
    id: String,
}

async fn repair_escalation_status(
    State(engine): State<Arc<Engine>>,
    headers: HeaderMap,
    Query(query): Query<RepairEscalationQuery>,
) -> Api {
    if authorize(&engine, &headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Unauthorized"})),
        );
    }
    api(engine.escalation_snapshot(&query.id))
}

async fn repair_escalation_control(
    State(engine): State<Arc<Engine>>,
    headers: HeaderMap,
    Json(request): Json<RepairEscalationMutation>,
) -> Api {
    if authorize(&engine, &headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Unauthorized"})),
        );
    }
    let result = match request.action.as_str() {
        "prepare" => engine.prepare_escalation(request),
        "approve_and_apply" => engine.approve_escalation(request),
        "cancel" => engine.cancel_escalation(request),
        _ => Err(anyhow!("Unknown repair escalation action")),
    };
    api(result)
}

async fn chat_status(
    State(engine): State<Arc<Engine>>,
    headers: HeaderMap,
    Query(query): Query<ChatQuery>,
) -> Api {
    if authorize(&engine, &headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Unauthorized"})),
        );
    }
    api(engine.reconcile_completed_tool_mutations().and_then(|_| {
        let chat_id = engine
            .chat
            .resolve_chat_id(&query.project, query.chat_id.as_deref())?;
        engine
            .chat
            .snapshot_chat(&query.project, &chat_id, query.before.as_deref())
    }))
}

async fn chat_projects(State(engine): State<Arc<Engine>>, headers: HeaderMap) -> Api {
    if authorize(&engine, &headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Unauthorized"})),
        );
    }
    api(engine.chat.projects())
}

async fn chat_conversations(
    State(engine): State<Arc<Engine>>,
    headers: HeaderMap,
    Query(query): Query<ChatConversationQuery>,
) -> Api {
    if authorize(&engine, &headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Unauthorized"})),
        );
    }
    api(engine
        .chat
        .list_conversations(query.project.as_deref(), query.cursor.as_deref()))
}

async fn chat_conversation_create(
    State(engine): State<Arc<Engine>>,
    headers: HeaderMap,
    Json(request): Json<ChatConversationCreate>,
) -> Api {
    if authorize(&engine, &headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Unauthorized"})),
        );
    }
    api(engine.chat.create_conversation(
        &request.id,
        &request.project,
        request.reuse_chat_id.as_deref(),
    ))
}

async fn chat_conversation_rename(
    State(engine): State<Arc<Engine>>,
    headers: HeaderMap,
    Json(request): Json<ChatConversationRename>,
) -> Api {
    if authorize(&engine, &headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Unauthorized"})),
        );
    }
    api(engine.chat.rename_conversation(
        &request.project,
        &request.chat_id,
        &request.title,
        request.expected_revision,
    ))
}

async fn chat_start(
    State(engine): State<Arc<Engine>>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> Api {
    if authorize(&engine, &headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Unauthorized"})),
        );
    }
    let result = (|| -> Result<Value> {
        engine.reconcile_completed_tool_mutations()?;
        let project = request["project"]
            .as_str()
            .context("Missing chat project")?;
        let message = request["message"]
            .as_str()
            .context("Missing chat message")?;
        let id = request["id"].as_str().context("Missing chat request ID")?;
        let chat_id = engine
            .chat
            .resolve_chat_id(project, request.get("chat_id").and_then(Value::as_str))?;
        let model_target = request
            .get("model_target")
            .and_then(Value::as_str)
            .unwrap_or("windows");
        let attachments: Vec<ChatAttachment> = serde_json::from_value(
            request
                .get("attachments")
                .cloned()
                .unwrap_or_else(|| json!([])),
        )
        .context("Invalid chat attachments")?;
        let database = engine
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?;
        engine.ensure_publication_barrier_clear(&database.state, &database.github_setup)?;
        if engine.shutdown.load(Ordering::SeqCst) {
            bail!("Developer runner is shutting down");
        }
        if engine.emergency_paused(&database.state) {
            bail!("Clear Emergency Pause before starting project chat");
        }
        if engine.planning_running.load(Ordering::SeqCst) {
            bail!("Wait for brainstorming to finish before starting project chat");
        }
        if engine.escalation_running.load(Ordering::SeqCst) {
            bail!("Wait for the repair proposal to finish before starting project chat");
        }
        let queue: Vec<Value> = database
            .state
            .queue
            .iter()
            .filter(|feature| feature.project == project && feature.status != "removed")
            .take(100)
            .map(|feature| {
                json!({
                    "id":feature.id,"instruction":feature.instruction,"status":feature.status,
                    "checkpoint":feature.checkpoint,"message":feature.message,
                    "model_target":feature.model_target,
                    "changed_files":feature.edits.as_ref().map(|edits| edits.iter().map(|edit| &edit.path).collect::<Vec<_>>()).unwrap_or_default()
                })
            })
            .collect();
        let snapshot = engine.chat.start(
            project,
            &chat_id,
            message,
            id,
            model_target,
            attachments,
            json!({"features":queue}),
        )?;
        drop(database);
        Ok(snapshot)
    })();
    api(result)
}

async fn chat_cancel(
    State(engine): State<Arc<Engine>>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> Api {
    if authorize(&engine, &headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Unauthorized"})),
        );
    }
    api((|| -> Result<Value> {
        let id = request["id"].as_str().context("Missing chat request ID")?;
        engine.chat.cancel(
            request.get("project").and_then(Value::as_str),
            request.get("chat_id").and_then(Value::as_str),
            id,
        )
    })())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChatAccessMutation {
    project: String,
    chat_id: Option<String>,
    mode: String,
    expected_revision: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChatApprovalMutation {
    project: String,
    chat_id: Option<String>,
    request_id: String,
    approval_id: String,
    access_revision: u64,
    decision: String,
}

async fn chat_access(
    State(engine): State<Arc<Engine>>,
    headers: HeaderMap,
    Json(request): Json<ChatAccessMutation>,
) -> Api {
    if authorize(&engine, &headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Unauthorized"})),
        );
    }
    api((|| -> Result<Value> {
        let database = engine
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?;
        engine.ensure_publication_barrier_clear(&database.state, &database.github_setup)?;
        if engine.emergency_paused(&database.state) {
            bail!("Clear Emergency Pause before changing tool access");
        }
        let idle = engine.developer_work_is_idle();
        engine.tools.set_access(
            &request.project,
            &request.mode,
            request.expected_revision,
            idle,
        )?;
        drop(database);
        let chat_id = engine
            .chat
            .resolve_chat_id(&request.project, request.chat_id.as_deref())?;
        engine.chat.snapshot_chat(&request.project, &chat_id, None)
    })())
}

async fn chat_approval(
    State(engine): State<Arc<Engine>>,
    headers: HeaderMap,
    Json(request): Json<ChatApprovalMutation>,
) -> Api {
    if authorize(&engine, &headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Unauthorized"})),
        );
    }
    api((|| -> Result<Value> {
        let database = engine
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?;
        engine.ensure_publication_barrier_clear(&database.state, &database.github_setup)?;
        if engine.emergency_paused(&database.state) {
            bail!("Clear Emergency Pause before approving a tool action");
        }
        let chat_id = engine
            .chat
            .resolve_chat_id(&request.project, request.chat_id.as_deref())?;
        engine.tools.decide(
            &request.project,
            Some(&chat_id),
            &request.request_id,
            &request.approval_id,
            request.access_revision,
            &request.decision,
        )?;
        drop(database);
        engine.chat.snapshot_chat(&request.project, &chat_id, None)
    })())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanningMutation {
    action: String,
    feature_id: String,
    request_id: String,
    expected_revision: u64,
    project: Option<String>,
    instruction: Option<String>,
    validation: Option<String>,
    model_target: Option<String>,
    answer: Option<String>,
    approach_id: Option<String>,
}

#[derive(Deserialize)]
struct PlanningQuery {
    id: String,
}

impl Engine {
    fn planning_snapshot(&self, feature_id: &str) -> Result<Value> {
        Uuid::parse_str(feature_id).context("Invalid planning feature ID")?;
        let database = self
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?;
        let session = database
            .state
            .planning_sessions
            .iter()
            .find(|session| session.feature_id == feature_id)
            .context("Planning session not found")?;
        Ok(serde_json::to_value(session)?)
    }

    fn planning_mutate(self: &Arc<Self>, raw: Value) -> Result<Value> {
        {
            let database = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            self.ensure_publication_barrier_clear(&database.state, &database.github_setup)?;
        }
        let digest = request_digest(&raw)?;
        let request: PlanningMutation = serde_json::from_value(raw)?;
        developer_planning::validate_identifier(&request.feature_id)?;
        developer_planning::validate_identifier(&request.request_id)?;

        let existing = {
            let database = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            database
                .state
                .planning_sessions
                .iter()
                .find(|session| session.feature_id == request.feature_id)
                .cloned()
        };
        if let Some(ref existing) = existing {
            if let Some(record) = existing
                .requests
                .iter()
                .find(|record| record.request_id == request.request_id)
            {
                if record.request_sha256 != digest {
                    bail!("Planning request ID reused with different contents");
                }
                self.retry_planning_completion(&request.feature_id, &request.request_id)?;
                return self.planning_snapshot(&request.feature_id);
            }
        }

        let asynchronous = matches!(
            request.action.as_str(),
            "start"
                | "answer"
                | "confirm_understanding"
                | "select_approach"
                | "confirm_design"
                | "revise"
                | "retry"
        );
        let planning_cancellation = if asynchronous {
            let database = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            self.ensure_publication_barrier_clear(&database.state, &database.github_setup)?;
            if self.shutdown.load(Ordering::SeqCst) {
                bail!("Developer runner is shutting down");
            }
            if self.running.load(Ordering::SeqCst)
                || self.chat.is_running()
                || self.tools.blocks_work()
                || self.escalation_running.load(Ordering::SeqCst)
            {
                bail!("Stop active developer work before brainstorming");
            }
            let selected_orchestrator = if request.action == "start" {
                database.state.ai_settings.orchestrator.clone()
            } else {
                let session = existing.as_ref().context("Planning session not found")?;
                AiSelection {
                    model: session.model.clone(),
                    reasoning_effort: session.reasoning_effort.clone(),
                }
            };
            validate_selection(&self.ai_catalog, &selected_orchestrator)
                .context("Selected orchestrator is unavailable")?;
            let cancellation = self.reserve_planning_call()?;
            drop(database);
            Some(cancellation)
        } else {
            if request.action == "approve_and_enqueue"
                && (self.running.load(Ordering::SeqCst)
                    || self.chat.is_running()
                    || self.tools.blocks_work()
                    || self.escalation_running.load(Ordering::SeqCst))
            {
                bail!("Stop active developer work before approving and enqueueing another feature");
            }
            None
        };

        let result = (|| -> Result<Option<PlanningPacket>> {
            let project = if request.action == "start" {
                request
                    .project
                    .as_deref()
                    .context("Missing project")?
                    .to_owned()
            } else {
                self.database
                    .lock()
                    .map_err(|_| anyhow!("state lock failed"))?
                    .state
                    .planning_sessions
                    .iter()
                    .find(|session| session.feature_id == request.feature_id)
                    .context("Planning session not found")?
                    .project
                    .clone()
            };
            let (context_files, omitted_context_files) = if asynchronous {
                collect_planning_context(&self.root, &project, planning_cancellation.as_deref())?
            } else {
                (Vec::new(), 0)
            };
            let tool_workspace_revision = if request.action == "approve_and_enqueue" {
                let project_path = self.root.join(&project);
                if project_path.is_dir() {
                    self.tools.snapshot(&project)?["workspace_revision"]
                        .as_u64()
                        .context("Tool workspace revision is unavailable")?
                } else {
                    0
                }
            } else {
                0
            };
            self.change_database(|state, github_setup| {
                self.ensure_publication_barrier_clear(state, github_setup)?;
                if self.emergency_paused(state) {
                    bail!("Clear Emergency Pause before brainstorming");
                }
                if request.action == "start" {
                    if request.expected_revision != 0 {
                        bail!("New planning session must start at revision 0");
                    }
                    if state.planning_sessions.len() >= MAX_PLANNING_SESSIONS {
                        bail!("Planning session limit reached");
                    }
                    if state
                        .planning_sessions
                        .iter()
                        .any(|session| session.feature_id == request.feature_id)
                        || state
                            .queue
                            .iter()
                            .any(|feature| feature.id == request.feature_id)
                    {
                        bail!("Feature ID already exists");
                    }
                    let model_target = request.model_target.as_deref().unwrap_or("mac");
                    self.model_target(model_target)?;
                    let orchestrator = state.ai_settings.orchestrator.clone();
                    state.planning_sessions.push(new_session(
                        &request.feature_id,
                        &project,
                        request
                            .instruction
                            .as_deref()
                            .context("Missing feature description")?,
                        request
                            .validation
                            .as_deref()
                            .context("Missing validation command")?,
                        model_target,
                        &orchestrator.model,
                        &orchestrator.reasoning_effort,
                    )?);
                }
                let selected_reviewer = state.ai_settings.reviewer.clone();
                let session = state
                    .planning_sessions
                    .iter_mut()
                    .find(|session| session.feature_id == request.feature_id)
                    .context("Planning session not found")?;
                if request.action != "start" && session.revision != request.expected_revision {
                    bail!("Planning revision changed; refresh before continuing");
                }
                if !record_request(
                    session,
                    &request.request_id,
                    &digest,
                    &request.action,
                    request.expected_revision,
                )? {
                    return Ok(None);
                }
                match request.action.as_str() {
                    "start" => {
                        begin_provider(session, "start")?;
                    }
                    "answer" => {
                        if session.stage != "questions" || session.running {
                            bail!("No planning question is ready for an answer");
                        }
                        let question = session
                            .question
                            .take()
                            .context("No planning question is ready for an answer")?;
                        let answer = request
                            .answer
                            .as_deref()
                            .context("Missing planning answer")?;
                        developer_planning::validate_input(answer, 4_000, "planning answer")?;
                        session.answers.push(developer_planning::PlanningAnswer {
                            question: question.text,
                            answer: answer.into(),
                        });
                        begin_provider(session, "answer")?;
                    }
                    "confirm_understanding" => {
                        if session.stage != "understanding"
                            || session.running
                            || session.assumptions.is_none()
                            || !session.open_questions.is_empty()
                        {
                            bail!("Understanding is not ready for confirmation");
                        }
                        begin_provider(session, "confirm_understanding")?;
                    }
                    "select_approach" => {
                        if session.stage != "approaches" || session.running {
                            bail!("Approaches are not ready for selection");
                        }
                        let selected = request
                            .approach_id
                            .as_deref()
                            .context("Missing approach ID")?;
                        if !session
                            .approaches
                            .iter()
                            .any(|approach| approach.id == selected)
                        {
                            bail!("Selected approach is unavailable");
                        }
                        session.selected_approach_id = Some(selected.into());
                        begin_provider(session, "select_approach")?;
                    }
                    "confirm_design" => {
                        if session.stage != "design" || session.running {
                            bail!("Design is not ready for confirmation");
                        }
                        let current = session
                            .design_sections
                            .iter_mut()
                            .find(|section| !section.confirmed)
                            .context("No design section is ready for confirmation")?;
                        current.confirmed = true;
                        begin_provider(session, "confirm_design")?;
                    }
                    "revise" => {
                        if matches!(
                            session.stage.as_str(),
                            "questions" | "enqueued" | "cancelled"
                        ) || session.running
                        {
                            bail!("Planning stage cannot be revised");
                        }
                        let answer = request
                            .answer
                            .as_deref()
                            .context("Missing revision guidance")?;
                        developer_planning::validate_input(answer, 4_000, "revision guidance")?;
                        session.answers.push(developer_planning::PlanningAnswer {
                            question: "Owner correction".into(),
                            answer: answer.into(),
                        });
                        session.stage = "questions".into();
                        session.question = None;
                        session.understanding_summary.clear();
                        session.assumptions = None;
                        session.open_questions.clear();
                        session.approaches.clear();
                        session.selected_approach_id = None;
                        session.design_sections.clear();
                        session.decision_log.clear();
                        session.documents = None;
                        begin_provider(session, "revise")?;
                    }
                    "retry" => {
                        if session.running
                            || session.availability != "unavailable"
                            || matches!(session.stage.as_str(), "ready" | "enqueued" | "cancelled")
                        {
                            bail!("Planning request is not retryable");
                        }
                        begin_provider(session, "retry")?;
                    }
                    "cancel" => {
                        if session.stage == "enqueued" {
                            bail!("An enqueued plan cannot be cancelled through planning");
                        }
                        let was_running = session.running;
                        session.running = false;
                        session.stage = "cancelled".into();
                        session.availability = "available".into();
                        session.pending_kind = None;
                        session.pending_revision = None;
                        session.pending_request_id = None;
                        session.pending_packet_sha256 = None;
                        session.error = None;
                        session.revision = session
                            .revision
                            .checked_add(1)
                            .context("Planning revision overflow")?;
                        if was_running {
                            self.cancel_planning_call(false);
                        }
                        return Ok(None);
                    }
                    "approve_and_enqueue" => {
                        let metadata = approved_metadata(session)?;
                        validate_selection(&self.ai_catalog, &selected_reviewer)
                            .context("Selected reviewer is unavailable")?;
                        if state
                            .queue
                            .iter()
                            .any(|feature| feature.id == request.feature_id)
                        {
                            bail!("Feature ID already exists in the queue");
                        }
                        if state.queue.len() >= 100 {
                            bail!("Queue limit is 100");
                        }
                        let feature = Feature {
                            id: session.feature_id.clone(),
                            project: session.project.clone(),
                            instruction: session.instruction.clone(),
                            validation: session.validation.clone(),
                            status: "queued".into(),
                            checkpoint: "not_started".into(),
                            message: "Approved plan is ready to start".into(),
                            edits: None,
                            repair_attempts: 0,
                            repair_pending: false,
                            repair_history: Vec::new(),
                            escalation_count: 0,
                            escalation_pending: false,
                            escalation_proposal: None,
                            escalation_history: Vec::new(),
                            auto_ai_repair_limit: None,
                            auto_repair_lifecycle: default_auto_repair_lifecycle(),
                            auto_repair_reason: String::new(),
                            auto_repair_epoch: 0,
                            auto_repair_step_started_at_ms: None,
                            auto_repair_policy_revision: None,
                            escalation_evidence_reserved: 0,
                            review_evidence_reserved: 0,
                            last_failure_kind: String::new(),
                            last_code_failure_summary: String::new(),
                            auto_repair_limit_project_baseline: Default::default(),
                            auto_repair_limit_unadmitted_sha256: None,
                            auto_repair_limit_volatile_sha256: None,
                            model_target: session.model_target.clone(),
                            review_status: "pending".into(),
                            review_model: selected_reviewer.model,
                            review_reasoning_effort: selected_reviewer.reasoning_effort,
                            review_binding_version: 1,
                            review_attempts: 0,
                            review_summary: "Required ChatGPT Codex review has not started".into(),
                            review_pending: None,
                            review_history: Vec::new(),
                            reviewer_selection_history: Vec::new(),
                            planning: Some(metadata),
                            cumulative_evidence_version: 1,
                            tool_workspace_revision,
                            publication_selection_frozen: false,
                            publication_binding: None,
                            publication_candidate: Vec::new(),
                            publication: None,
                        };
                        session.stage = "enqueued".into();
                        session.revision = session
                            .revision
                            .checked_add(1)
                            .context("Planning revision overflow")?;
                        state.queue.push(feature);
                        return Ok(None);
                    }
                    _ => bail!("Unknown planning action"),
                }
                let expected = expected_response(session)?.to_owned();
                let packet = PlanningPacket {
                    schema_version: 1,
                    feature_id: session.feature_id.clone(),
                    request_id: request.request_id.clone(),
                    skill_sha256: developer_planning::brainstorming_skill_sha256(),
                    revision: session.revision,
                    expected_response: expected,
                    provider_id: session.provider.clone(),
                    model_id: session.model.clone(),
                    reasoning_effort: session.reasoning_effort.clone(),
                    project: session.project.clone(),
                    instruction: session.instruction.clone(),
                    validation: session.validation.clone(),
                    model_target: session.model_target.clone(),
                    answers: session.answers.clone(),
                    understanding_summary: session.understanding_summary.clone(),
                    assumptions: session.assumptions.clone(),
                    approaches: session.approaches.clone(),
                    selected_approach_id: session.selected_approach_id.clone(),
                    design_sections: session.design_sections.clone(),
                    decision_log: session.decision_log.clone(),
                    context_files: context_files.clone(),
                    omitted_context_files,
                };
                bind_pending_packet(session, &request.request_id, &packet.sha256()?)?;
                Ok(Some(packet))
            })
        })();

        let packet = match result {
            Ok(packet) => packet,
            Err(error) => {
                if let Some(cancellation) = &planning_cancellation {
                    self.release_planning_call(cancellation);
                }
                return Err(error);
            }
        };
        if let Some(packet) = packet {
            let planning_cancellation =
                planning_cancellation.context("Planning cancellation binding missing")?;
            let engine = self.clone();
            tokio::spawn(async move {
                let result = match provider_prompt(&packet).and_then(|prompt| {
                    let schema =
                        planning_output_schema(&packet.model_id, &packet.reasoning_effort)?;
                    let filename = format!(
                        "developer-planning-output-schema-{}.json",
                        &hex_digest(schema.as_bytes())[..16]
                    );
                    Ok((prompt, schema, filename))
                }) {
                    Ok((prompt, schema, filename)) => engine
                        .reviewer
                        .call_tool_free(
                            &prompt,
                            &filename,
                            &schema,
                            &packet.model_id,
                            &packet.reasoning_effort,
                            &planning_cancellation,
                        )
                        .await
                        .and_then(|bytes| {
                            serde_json::from_slice::<PlanningProviderOutput>(&bytes).map_err(|_| {
                                DeveloperReviewCallError::Unavailable(
                                    "Codex returned malformed planning JSON".into(),
                                )
                            })
                        }),
                    Err(error) => Err(DeveloperReviewCallError::Unavailable(format!(
                        "planning disclosure blocked: {error}"
                    ))),
                };
                let completion = engine.finish_planning_completion(
                    &packet,
                    match result {
                        Ok(output) => PlanningCompletion::Output(Box::new(output)),
                        Err(error) => PlanningCompletion::Unavailable(error.to_string()),
                    },
                );
                if let Err(error) = completion {
                    eprintln!("developer planning completion: {error:#}");
                }
                engine.release_planning_call(&planning_cancellation);
            });
        } else if let Some(cancellation) = &planning_cancellation {
            self.release_planning_call(cancellation);
        }
        self.planning_snapshot(&request.feature_id)
    }
}

async fn planning_status(
    State(engine): State<Arc<Engine>>,
    headers: HeaderMap,
    Query(query): Query<PlanningQuery>,
) -> Api {
    if authorize(&engine, &headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Unauthorized"})),
        );
    }
    api((|| -> Result<Value> {
        engine.retry_planning_completion_for_feature(&query.id)?;
        engine.planning_snapshot(&query.id)
    })())
}

async fn planning_control(
    State(engine): State<Arc<Engine>>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> Api {
    if authorize(&engine, &headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Unauthorized"})),
        );
    }
    api(engine.planning_mutate(request))
}

async fn control(
    State(engine): State<Arc<Engine>>,
    headers: HeaderMap,
    Json(request): Json<Value>,
) -> Api {
    if authorize(&engine, &headers).is_err() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(json!({"error":"Unauthorized"})),
        );
    }
    let result = (|| -> Result<Value> {
        let action = request["action"].as_str().context("Missing action")?;
        if !matches!(
            action,
            "stop" | "emergency" | "shutdown" | "clear_emergency"
        ) {
            let database = engine
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            engine.ensure_publication_barrier_clear(&database.state, &database.github_setup)?;
        }
        match action {
            "enqueue" => {
                let id = request["id"].as_str().context("Missing request ID")?;
                Uuid::parse_str(id)?;
                let project = request["project"].as_str().context("Missing project")?;
                if project.is_empty()
                    || project.len() > 80
                    || !project
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                {
                    bail!("Use a simple project folder name");
                }
                let instruction = request["instruction"]
                    .as_str()
                    .context("Missing feature description")?;
                let validation = request["validation"]
                    .as_str()
                    .context("Missing validation command")?;
                let model_target = match request.get("model_target") {
                    None | Some(Value::Null) => "mac",
                    Some(value) => value.as_str().context("Invalid model target")?,
                };
                if !matches!(model_target, "mac" | "windows") {
                    bail!("Unknown model target");
                }
                engine.model_target(model_target)?;
                if instruction.trim().is_empty()
                    || instruction.len() > 16000
                    || validation.trim().is_empty()
                    || validation.len() > 2000
                {
                    bail!("Feature description and validation command are required");
                }
                engine.change_database(|s, github_setup| {
                    engine.ensure_publication_barrier_clear(s, github_setup)?;
                    let old = s.queue.iter().find(|f| f.id == id).context(
                        "Mandatory brainstorming, design confirmation, and explicit approve-and-enqueue are required before a new feature can enter the queue",
                    )?;
                    if old.project != project
                        || old.instruction != instruction
                        || old.validation != validation
                        || old.model_target != model_target
                    { bail!("Request ID reused with different contents"); }
                    Ok(())
                })?;
            }
            "start" | "resume" => {
                let expected_feature_id = request
                    .get("expected_feature_id")
                    .filter(|value| !value.is_null())
                    .map(|value| value.as_str().context("Invalid expected feature ID"))
                    .transpose()?;
                let expected_model_target = request
                    .get("expected_model_target")
                    .filter(|value| !value.is_null())
                    .map(|value| value.as_str().context("Invalid expected model target"))
                    .transpose()?;
                let expected_status = request
                    .get("expected_status")
                    .filter(|value| !value.is_null())
                    .map(|value| value.as_str().context("Invalid expected status"))
                    .transpose()?;
                let expected_checkpoint = request
                    .get("expected_checkpoint")
                    .filter(|value| !value.is_null())
                    .map(|value| value.as_str().context("Invalid expected checkpoint"))
                    .transpose()?;
                engine.start(
                    expected_feature_id,
                    expected_model_target,
                    expected_status,
                    expected_checkpoint,
                )?;
            }
            "remove" => {
                let id = request["id"].as_str().context("Missing feature ID")?;
                engine.remove(id)?;
            }
            "repair" => {
                let id = request["id"].as_str().context("Missing feature ID")?;
                let expected_attempts: u32 = request["expected_attempts"]
                    .as_u64()
                    .context("Missing expected repair attempt count")?
                    .try_into()
                    .context("Invalid expected repair attempt count")?;
                engine.repair(id, expected_attempts)?;
            }
            "stop" | "emergency" => {
                let emergency = request["action"] == "emergency";
                let cancellation_effect_guard = {
                    let guard = engine
                        .effect_gate
                        .lock()
                        .map_err(|_| anyhow!("effect gate failed"))?;
                    // Linearize Stop and Emergency Pause against every project-file
                    // effect before attempting durable persistence. A failed write
                    // intentionally leaves the process-local latch fail closed.
                    engine.repair_loop_authorized.store(false, Ordering::SeqCst);
                    engine
                        .cancellation
                        .fetch_max(if emergency { 2 } else { 1 }, Ordering::SeqCst);
                    engine
                        .publication_cancellation
                        .fetch_max(if emergency { 2 } else { 1 }, Ordering::SeqCst);
                    engine.tool_cancellation.store(true, Ordering::SeqCst);
                    guard
                };
                let persisted = engine.change(|s| {
                    if emergency {
                        s.emergency_paused = true;
                    }
                    for feature in &mut s.queue {
                        if feature.auto_repair_lifecycle == "running" {
                            if emergency {
                                retire_ready_automatic_proposal_for_cancellation(
                                    feature,
                                    "Emergency Pause arrived after proposal generation and before authorization. The unapplied automatic proposal was retired without review."
                                )?;
                            } else {
                                pause_preapply_automatic_proposal_for_disable(
                                    feature,
                                    "Stop cancelled automatic proposal work before authorization. The unapplied proposal was retired without review; Resume or re-enable may start fresh work without replaying proposal bytes.",
                                )?;
                            }
                            persist_automatic_cancellation_intent(
                                feature,
                                engine.running.load(Ordering::SeqCst),
                                if emergency {
                                    "Emergency Pause cancellation intent was persisted before active automatic repair was signalled"
                                } else {
                                    "Stop cancellation intent was persisted before active automatic repair was signalled"
                                },
                                if emergency {
                                    "Emergency Pause was recorded after automatic repair application began. The uncertain write boundary is quarantined before process termination."
                                } else {
                                    "Stop was recorded after automatic repair application began. The uncertain write boundary is quarantined before process termination."
                                },
                            )?;
                        }
                    }
                    for session in &mut s.planning_sessions {
                        invalidate_pending(session, if emergency {
                            "Emergency Pause interrupted the planning request; retry after clearing the pause."
                        } else {
                            "Stop interrupted the planning request; retry when ready."
                        })?;
                    }
                    Ok(())
                });
                if let Err(error) = persisted {
                    if emergency {
                        // A failed durable Emergency Pause write still latches the
                        // process-local stop signal so no further work can start.
                        // Automatic repair remains ineligible until the owner
                        // durably clears the fail-closed latch.
                        engine.cancellation.fetch_max(2, Ordering::SeqCst);
                        engine
                            .publication_cancellation
                            .fetch_max(2, Ordering::SeqCst);
                        engine.tool_cancellation.store(true, Ordering::SeqCst);
                        engine.cancel_planning_call(true);
                        engine.cancel_escalation_call(true);
                        engine.chat.cancel_for_emergency();
                        engine.tools.cancel_for_emergency();
                    }
                    return Err(error);
                }
                engine.repair_loop_authorized.store(false, Ordering::SeqCst);
                engine
                    .cancellation
                    .fetch_max(if emergency { 2 } else { 1 }, Ordering::SeqCst);
                engine
                    .publication_cancellation
                    .fetch_max(if emergency { 2 } else { 1 }, Ordering::SeqCst);
                engine.tool_cancellation.store(true, Ordering::SeqCst);
                engine.cancel_planning_call(emergency);
                engine.cancel_escalation_call(emergency);
                engine.chat.cancel_active();
                engine.tools.cancel_active();
                if emergency {
                    engine.chat.cancel_for_emergency();
                    engine.tools.cancel_for_emergency();
                }
                if emergency {
                    if let Ok(mut recovery) = engine.planning_completion_recovery.lock() {
                        *recovery = None;
                    }
                }
                drop(cancellation_effect_guard);
            }
            "shutdown" => {
                engine.begin_shutdown()?;
            }
            "clear_emergency" => {
                if engine.running.load(Ordering::SeqCst)
                    || engine.planning_running.load(Ordering::SeqCst)
                    || engine.chat.is_running()
                    || engine.tools.blocks_work()
                    || engine.escalation_running.load(Ordering::SeqCst)
                    || engine.publication_running.load(Ordering::SeqCst)
                    || engine.publication_connection_running.load(Ordering::SeqCst)
                {
                    bail!("Wait for active developer work to stop");
                }
                engine.change_with_commit(
                    |s| {
                        s.emergency_paused = false;
                        Ok(())
                    },
                    || {
                        engine.cancellation.store(0, Ordering::SeqCst);
                        engine.publication_cancellation.store(0, Ordering::SeqCst);
                        engine.tool_cancellation.store(false, Ordering::SeqCst);
                    },
                )?;
            }
            "auto_run" => {
                let enabled = request["enabled"]
                    .as_bool()
                    .context("Missing enabled value")?;
                engine.change_database(|s, github_setup| {
                    engine.ensure_publication_barrier_clear(s, github_setup)?;
                    s.auto_run = enabled;
                    Ok(())
                })?;
            }
            _ => bail!("Unknown action"),
        }
        engine.snapshot()
    })();
    api(result)
}
#[tokio::main]
async fn main() -> Result<()> {
    #[cfg(windows)]
    if let Some(exit_code) = developer_review::review_launcher_exit_code() {
        std::process::exit(exit_code);
    }
    let args = Args::parse();
    if !args.bind.ip().is_loopback() {
        bail!("Developer runner binds only to loopback; use SSH forwarding");
    }
    let model_targets = configured_model_targets(&args)?;
    let ai_catalog = load_catalog(&args.review_codex_home)?;
    fs::create_dir_all(&args.data_dir)?;
    fs::create_dir_all(&args.workspace_root)?;
    let instance_lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(args.data_dir.join("developer.lock"))?;
    fs2::FileExt::try_lock_exclusive(&instance_lock)
        .context("A developer runner already owns this state directory")?;
    let token_path = args.data_dir.join("developer-token");
    if !token_path.exists() {
        fs::write(&token_path, format!("{}{}", Uuid::new_v4(), Uuid::new_v4()))?;
    }
    let token = fs::read_to_string(&token_path)?;
    let connection = Connection::open(args.data_dir.join("developer.sqlite3"))?;
    connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS developer_state(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL); CREATE TABLE IF NOT EXISTS developer_state_v1_backup(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL); CREATE TABLE IF NOT EXISTS developer_state_v2_backup(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL); CREATE TABLE IF NOT EXISTS developer_state_v3_backup(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL); CREATE TABLE IF NOT EXISTS developer_state_v4_backup(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL); CREATE TABLE IF NOT EXISTS developer_state_v5_backup(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL); CREATE TABLE IF NOT EXISTS developer_state_v6_backup(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL); CREATE TABLE IF NOT EXISTS developer_state_v7_backup(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL); CREATE TABLE IF NOT EXISTS developer_state_v8_backup(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL); CREATE TABLE IF NOT EXISTS developer_state_v9_backup(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL); CREATE TABLE IF NOT EXISTS developer_state_v10_backup(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL);")?;
    let github_setup = GithubSetupState::initialize_and_load(&connection)?;
    let (mut state, loaded_queue_version): (Snapshot, u8) =
        match connection.query_row("SELECT state FROM developer_state WHERE id=1", [], |r| {
            r.get::<_, String>(0)
        }) {
            Ok(data) => {
                let value: Value = serde_json::from_str(&data)?;
                let loaded_queue_version = if value.get("queue_v11").is_some() {
                    11
                } else if value.get("queue_v10").is_some() {
                    10
                } else if value.get("queue_v9").is_some() {
                    9
                } else if value.get("queue_v8").is_some() {
                    8
                } else if value.get("queue_v7").is_some() {
                    7
                } else {
                    6
                };
                if value.get("queue").is_some() && value.get("queue_v2").is_none() {
                    connection.execute(
                        "INSERT OR IGNORE INTO developer_state_v1_backup(id,state) VALUES(1,?1)",
                        [&data],
                    )?;
                }
                if value.get("queue_v2").is_some() && value.get("queue_v3").is_none() {
                    connection.execute(
                        "INSERT OR IGNORE INTO developer_state_v2_backup(id,state) VALUES(1,?1)",
                        [&data],
                    )?;
                }
                if value.get("queue_v3").is_some() && value.get("queue_v4").is_none() {
                    connection.execute(
                        "INSERT OR IGNORE INTO developer_state_v3_backup(id,state) VALUES(1,?1)",
                        [&data],
                    )?;
                }
                if value.get("queue_v4").is_some() && value.get("queue_v5").is_none() {
                    connection.execute(
                        "INSERT OR IGNORE INTO developer_state_v4_backup(id,state) VALUES(1,?1)",
                        [&data],
                    )?;
                }
                if value.get("queue_v5").is_some() && value.get("queue_v6").is_none() {
                    connection.execute(
                        "INSERT OR IGNORE INTO developer_state_v5_backup(id,state) VALUES(1,?1)",
                        [&data],
                    )?;
                }
                if value.get("queue_v6").is_some() && value.get("queue_v7").is_none() {
                    connection.execute(
                        "INSERT OR IGNORE INTO developer_state_v6_backup(id,state) VALUES(1,?1)",
                        [&data],
                    )?;
                }
                if value.get("queue_v7").is_some() && value.get("queue_v8").is_none() {
                    connection.execute(
                        "INSERT OR IGNORE INTO developer_state_v7_backup(id,state) VALUES(1,?1)",
                        [&data],
                    )?;
                }
                if value.get("queue_v8").is_some() && value.get("queue_v9").is_none() {
                    connection.execute(
                        "INSERT OR IGNORE INTO developer_state_v8_backup(id,state) VALUES(1,?1)",
                        [&data],
                    )?;
                }
                if value.get("queue_v9").is_some() && value.get("queue_v10").is_none() {
                    connection.execute(
                        "INSERT OR IGNORE INTO developer_state_v9_backup(id,state) VALUES(1,?1)",
                        [&data],
                    )?;
                }
                if value.get("queue_v10").is_some() && value.get("queue_v11").is_none() {
                    connection.execute(
                        "INSERT OR IGNORE INTO developer_state_v10_backup(id,state) VALUES(1,?1)",
                        [&data],
                    )?;
                }
                (serde_json::from_value(value)?, loaded_queue_version)
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => (
                Snapshot {
                    auto_run: true,
                    ..Default::default()
                },
                11,
            ),
            Err(e) => return Err(e.into()),
        };
    let root = fs::canonicalize(&args.workspace_root)?;
    validate_auto_repair_max(state.auto_ai_repair_max_escalations)
        .context("Persisted Auto AI repair maximum is invalid")?;
    validate_ai_model_id(&state.ai_settings.orchestrator.model)
        .context("Persisted orchestrator model setting is invalid")?;
    validate_reasoning_effort(&state.ai_settings.orchestrator.reasoning_effort)
        .context("Persisted orchestrator reasoning setting is invalid")?;
    validate_ai_model_id(&state.ai_settings.reviewer.model)
        .context("Persisted reviewer model setting is invalid")?;
    validate_reasoning_effort(&state.ai_settings.reviewer.reasoning_effort)
        .context("Persisted reviewer reasoning setting is invalid")?;
    if loaded_queue_version < 7 {
        for feature in &mut state.queue {
            feature.cumulative_evidence_version = 1;
        }
    } else if loaded_queue_version == 7 {
        let v6_backup = connection
            .query_row(
                "SELECT state FROM developer_state_v6_backup WHERE id=1",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let v6_state = v6_backup
            .as_deref()
            .map(serde_json::from_str::<Snapshot>)
            .transpose()?;
        for feature in &mut state.queue {
            if feature.escalation_count == 0 {
                feature.cumulative_evidence_version = 1;
                continue;
            }
            let prior = v6_state
                .as_ref()
                .and_then(|snapshot| snapshot.queue.iter().find(|prior| prior.id == feature.id))
                .context("Legacy escalation evidence has no matching v6 backup")?;
            let project = fs::canonicalize(root.join(&feature.project))?;
            if !project.starts_with(&root) {
                bail!("Project escapes the workspace root");
            }
            recover_v7_cumulative_evidence(feature, prior, &project)?;
        }
    } else if state
        .queue
        .iter()
        .any(|feature| feature.cumulative_evidence_version != 1)
    {
        bail!("Persisted cumulative review evidence has an unsupported version");
    }
    let recovery_revision = state.revision.checked_add(1).context("Revision overflow")?;
    let recovery_now_ms = current_time_ms().context("System clock is invalid")?;
    let mut connected_projects = std::collections::BTreeSet::new();
    for binding in &state.github_connections {
        validate_publication_binding(binding).context("Persisted GitHub connection is invalid")?;
        if !connected_projects.insert(binding.project.clone()) {
            bail!("Persisted GitHub connections contain a duplicate project");
        }
    }
    let persisted_revision = state.revision;
    for feature in &mut state.queue {
        if loaded_queue_version < 11 {
            feature.escalation_evidence_reserved = feature
                .escalation_count
                .saturating_mul(3)
                .max(feature.escalation_history.len() as u32);
            feature.review_evidence_reserved = feature.escalation_count;
        }
        migrate_and_validate_auto_repair_step_timestamp(feature, recovery_now_ms)
            .context("Persisted automatic repair step timestamp is invalid")?;
        validate_auto_repair_feature(feature)
            .context("Persisted automatic repair feature state is invalid")?;
        for evidence in &feature.escalation_history {
            validate_authorization_revision(evidence, loaded_queue_version, persisted_revision)?;
        }
        validate_ai_model_id(&feature.review_model)
            .context("Persisted feature reviewer model binding is invalid")?;
        validate_reasoning_effort(&feature.review_reasoning_effort)
            .context("Persisted feature reviewer reasoning binding is invalid")?;
        if feature.review_binding_version > 1 {
            bail!("Persisted feature reviewer binding version is unsupported");
        }
        if !matches!(feature.model_target.as_str(), "mac" | "windows") {
            bail!("Persisted feature has an unknown model target");
        }
        if let Some(binding) = &feature.publication_binding {
            validate_publication_binding(binding)
                .context("Persisted feature GitHub connection is invalid")?;
            if !feature.publication_selection_frozen || binding.project != feature.project {
                bail!("Persisted feature GitHub connection is not frozen to its project");
            }
        }
        if loaded_queue_version < 11 && !feature.publication_candidate.is_empty() {
            let project = fs::canonicalize(root.join(&feature.project))?;
            if !project.starts_with(&root) {
                bail!("Project escapes the workspace root");
            }
            migrate_legacy_publication_classifications(feature, &project, loaded_queue_version)
                .context("Legacy frozen publication classification migration failed")?;
        }
        if let Some(publication) = feature.publication.as_ref() {
            publication
                .validate()
                .context("Persisted Developer publication is invalid")?;
            publication_input(feature).context("Persisted publication candidate is invalid")?;
            let publication = feature.publication.as_mut().unwrap();
            if matches!(publication.status.as_str(), "pending" | "running") {
                publication.status = "attention".into();
                publication.message = "Runner restarted during GitHub publication. Use explicit reconciliation to inspect the existing branch, pull request, checks, and merge; no external effect was replayed.".into();
                feature.status = "failed".into();
                feature.checkpoint = "publication_attention".into();
                feature.message = publication.message.clone();
            } else if publication.status == "attention" {
                feature.status = "failed".into();
                feature.checkpoint = "publication_attention".into();
                feature.message = publication.message.clone();
            } else {
                feature.status = "succeeded".into();
                feature.checkpoint = "publication_merged".into();
            }
        }
        recover_interrupted_escalation_preparation(feature, recovery_revision)?;
        if feature.escalation_pending
            && (feature.checkpoint.ends_with("_approved")
                || feature.checkpoint.ends_with("_applying"))
        {
            let project = fs::canonicalize(root.join(&feature.project))?;
            if !project.starts_with(&root) {
                bail!("Project escapes the workspace root");
            }
            quarantine_escalation_application(
                feature,
                &project,
                recovery_revision,
                "Runner restarted during repair proposal application. Inspect the workspace and prepare a new proposal; Resume will not replay unrecorded or partially applied edits.",
            )?;
            set_auto_repair_lifecycle(
                feature,
                "quarantined",
                "Runner restart interrupted an automatic repair effect boundary. Inspect the workspace; automatic replay is blocked.",
            )?;
        } else if feature.status == "running"
            && feature.auto_repair_lifecycle == "running"
            && automatic_ordinary_pre_effect_checkpoint(feature)
        {
            feature.status = "failed".into();
            feature.message = if feature.checkpoint.ends_with("_reserved") {
                "Runner restarted before the reserved ordinary repair made any change. The same reserved attempt may continue without consuming another attempt."
            } else {
                "Runner restarted after an ordinary repair was prepared but before its effect boundary. The exact saved candidate may continue once without re-running inference."
            }
            .into();
            start_auto_repair_step(feature)?;
        } else if feature.status == "running" {
            feature.status = if feature.auto_repair_lifecycle == "running" {
                "failed"
            } else {
                "paused"
            }
            .into();
            if feature.review_pending.is_some() {
                interrupt_pending_review(
                    feature,
                    "Runner restarted during ChatGPT Codex review. Resume revalidates the generated files and starts a fresh review attempt.",
                )?;
            }
            feature.message =
                "Runner restarted. Review the workspace, then Resume from the saved checkpoint."
                    .into();
            if feature.auto_repair_lifecycle == "running" {
                set_auto_repair_lifecycle(
                    feature,
                    "quarantined",
                    "Runner restart interrupted validation or independent review. Automatic replay is blocked until the owner inspects the workspace.",
                )?;
            }
        }
        if feature.review_status == "legacy_unreviewed"
            && !matches!(feature.status.as_str(), "succeeded" | "removed")
        {
            feature.review_status = "pending".into();
            feature.review_summary = "Required ChatGPT Codex review has not started".into();
        }
    }
    for session in &mut state.planning_sessions {
        validate_ai_model_id(&session.model)
            .context("Persisted planning model binding is invalid")?;
        validate_reasoning_effort(&session.reasoning_effort)
            .context("Persisted planning reasoning binding is invalid")?;
        if session.running {
            invalidate_pending(
                session,
                "Runner restarted before ChatGPT Codex replied; retry the interrupted planning step.",
            )?;
        }
    }
    let reviewer = DeveloperReviewer::new(
        args.review_codex_executable,
        args.review_codex_home,
        &args.data_dir,
    )?;
    let (publication_runtime, publication_unavailable_reason) = match PublicationRuntime::new(
        &args.data_dir,
        args.git_executable.clone(),
        args.gh_executable.clone(),
    ) {
        Ok(runtime) => (Some(Arc::new(runtime)), None),
        Err(error) => (
            None,
            Some(format!("GitHub publication is unavailable: {error:#}")),
        ),
    };
    let inference_gate = InferenceGate::new();
    let tool_runtime = args
        .opencode_executable
        .clone()
        .map(|executable| OpenCodeRuntimeConfig {
            executable,
            data_dir: args.data_dir.join("opencode"),
        });
    let tools = DeveloperTools::open(
        &args.data_dir.join("developer.sqlite3"),
        root.clone(),
        tool_runtime,
    )?;
    let chat_models = model_targets
        .iter()
        .map(|target| ChatModelConfig {
            target: target.id.into(),
            url: target.url.clone(),
            model: target.model.clone(),
        })
        .collect();
    let chat = DeveloperChat::open(
        &args.data_dir.join("developer.sqlite3"),
        root.clone(),
        chat_models,
        inference_gate.clone(),
        tools.clone(),
    )?;
    let engine = Arc::new(Engine {
        database: Mutex::new(Database {
            connection,
            state,
            github_setup,
        }),
        effect_gate: Mutex::new(()),
        running: AtomicBool::new(false),
        cancellation: AtomicU8::new(0),
        tool_cancellation: Arc::new(AtomicBool::new(false)),
        planning_cancellation: Mutex::new(None),
        planning_running: AtomicBool::new(false),
        planning_completion_recovery: Mutex::new(None),
        escalation_cancellation: Mutex::new(None),
        escalation_running: AtomicBool::new(false),
        repair_loop_authorized: AtomicBool::new(false),
        publication_running: AtomicBool::new(false),
        publication_connection_running: AtomicBool::new(false),
        publication_cancellation: AtomicU8::new(0),
        shutdown: AtomicBool::new(false),
        root,
        data: args.data_dir,
        token,
        model_targets,
        inference_gate,
        chat,
        tools,
        reviewer,
        ai_catalog,
        publication_runtime,
        publication_unavailable_reason,
    });
    engine.change(|_| Ok(()))?;
    if let Err(error) = engine.resume_automatic_after_restart() {
        eprintln!("developer automatic repair restart recovery: {error:#}");
    }
    let app = Router::new()
        .route("/status", get(status))
        .route(
            "/publication",
            get(publication_status).post(publication_control),
        )
        .route("/github", get(github_status).post(github_control))
        .route("/settings", get(settings_status).post(settings_control))
        .route("/auto-ai-repair", post(auto_ai_repair_control))
        .route("/feature-reviewer", post(feature_reviewer_control))
        .route("/control", post(control))
        .route(
            "/chat",
            get(chat_status)
                .post(chat_start)
                .layer(DefaultBodyLimit::max(9 * 1024 * 1024)),
        )
        .route("/chat/projects", get(chat_projects))
        .route(
            "/chat/conversations",
            get(chat_conversations).post(chat_conversation_create),
        )
        .route("/chat/rename", post(chat_conversation_rename))
        .route("/chat/cancel", post(chat_cancel))
        .route("/chat/access", post(chat_access))
        .route("/chat/approval", post(chat_approval))
        .route(
            "/repair/escalation",
            get(repair_escalation_status).post(repair_escalation_control),
        )
        .route("/planning", get(planning_status).post(planning_control))
        .layer(DefaultBodyLimit::max(32768))
        .with_state(engine.clone());
    let listener = tokio::net::TcpListener::bind(args.bind).await?;
    println!(
        "Assemblywright supervised developer runner ready at {}",
        args.bind
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            while !engine.shutdown.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        })
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn control_test_engine() -> (tempfile::TempDir, Arc<Engine>) {
        let dir = tempfile::tempdir().unwrap();
        let workspace = dir.path().join("workspace");
        let data = dir.path().join("data");
        let codex_home = dir.path().join("codex-home");
        fs::create_dir_all(workspace.join("example")).unwrap();
        fs::create_dir_all(&data).unwrap();
        fs::create_dir_all(&codex_home).unwrap();
        let codex_executable = dir
            .path()
            .join(if cfg!(windows) { "codex.exe" } else { "codex" });
        fs::write(&codex_executable, b"fixture codex executable").unwrap();
        let database_path = data.join("developer.sqlite3");
        let connection = Connection::open(&database_path).unwrap();
        connection
            .execute_batch(
                "CREATE TABLE developer_state(id INTEGER PRIMARY KEY,state TEXT NOT NULL);",
            )
            .unwrap();
        let github_setup = GithubSetupState::initialize_and_load(&connection).unwrap();
        let inference_gate = InferenceGate::new();
        let model_targets = vec![ModelTarget {
            id: "mac",
            name: "Mac local AI",
            url: "http://127.0.0.1:1/v1".into(),
            model: "fixture".into(),
        }];
        let tools =
            DeveloperTools::open(&database_path, fs::canonicalize(&workspace).unwrap(), None)
                .unwrap();
        let chat = DeveloperChat::open(
            &database_path,
            fs::canonicalize(&workspace).unwrap(),
            vec![ChatModelConfig {
                target: "mac".into(),
                url: "http://127.0.0.1:1/v1".into(),
                model: "fixture".into(),
            }],
            inference_gate.clone(),
            tools.clone(),
        )
        .unwrap();
        let ai_catalog = load_catalog(&codex_home).unwrap();
        let reviewer = DeveloperReviewer::new(codex_executable, codex_home, &data).unwrap();
        let engine = Arc::new(Engine {
            database: Mutex::new(Database {
                connection,
                state: Snapshot {
                    auto_run: true,
                    queue: vec![feature_with_status("failed")],
                    ..Default::default()
                },
                github_setup,
            }),
            effect_gate: Mutex::new(()),
            running: AtomicBool::new(false),
            cancellation: AtomicU8::new(0),
            tool_cancellation: Arc::new(AtomicBool::new(false)),
            planning_cancellation: Mutex::new(None),
            planning_running: AtomicBool::new(false),
            planning_completion_recovery: Mutex::new(None),
            escalation_cancellation: Mutex::new(None),
            escalation_running: AtomicBool::new(false),
            repair_loop_authorized: AtomicBool::new(false),
            publication_running: AtomicBool::new(false),
            publication_connection_running: AtomicBool::new(false),
            publication_cancellation: AtomicU8::new(0),
            shutdown: AtomicBool::new(false),
            root: fs::canonicalize(workspace).unwrap(),
            data,
            token: "test-token".into(),
            model_targets,
            inference_gate,
            chat,
            tools,
            reviewer,
            ai_catalog,
            publication_runtime: None,
            publication_unavailable_reason: Some("fixture publication unavailable".into()),
        });
        engine.change(|_| Ok(())).unwrap();
        (dir, engine)
    }

    fn authorized_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer test-token".parse().unwrap());
        headers
    }

    fn setup_database(revision: u64) -> Database {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE developer_state(\
                 id INTEGER PRIMARY KEY CHECK(id=1), state TEXT NOT NULL);",
            )
            .unwrap();
        let state = Snapshot {
            revision,
            ..Default::default()
        };
        connection
            .execute(
                "INSERT INTO developer_state(id,state) VALUES(1,?1)",
                [serde_json::to_string(&state).unwrap()],
            )
            .unwrap();
        let github_setup = GithubSetupState::initialize_and_load(&connection).unwrap();
        Database {
            connection,
            state,
            github_setup,
        }
    }

    #[test]
    fn github_setup_and_runner_revision_commit_atomically() {
        let mut database = setup_database(7);
        let original_state: String = database
            .connection
            .query_row("SELECT state FROM developer_state WHERE id=1", [], |row| {
                row.get(0)
            })
            .unwrap();
        let original_setup: String = database
            .connection
            .query_row(
                "SELECT state FROM developer_github_setup WHERE id=1",
                [],
                |row| row.get(0),
            )
            .unwrap();

        let rejected: Result<()> = mutate_database(&mut database, |state, setup| {
            state.auto_run = true;
            setup.account.state = "untrusted".into();
            Ok(())
        });
        assert!(rejected.is_err());
        assert_eq!(database.state.revision, 7);
        assert!(!database.state.auto_run);
        assert_eq!(database.github_setup.account.state, "unknown");
        assert_eq!(
            database
                .connection
                .query_row::<String, _, _>(
                    "SELECT state FROM developer_state WHERE id=1",
                    [],
                    |row| row.get(0),
                )
                .unwrap(),
            original_state
        );
        assert_eq!(
            database
                .connection
                .query_row::<String, _, _>(
                    "SELECT state FROM developer_github_setup WHERE id=1",
                    [],
                    |row| row.get(0),
                )
                .unwrap(),
            original_setup
        );

        mutate_database(&mut database, |state, setup| {
            state.auto_run = true;
            setup.reset_repositories_if_account_changed(&AccountRecord {
                state: "signed_out".into(),
                login: None,
                message: "Sign in to continue".into(),
            });
            Ok(())
        })
        .unwrap();
        assert_eq!(database.state.revision, 8);
        assert!(database.state.auto_run);
        assert_eq!(database.github_setup.account.state, "signed_out");
        let persisted: String = database
            .connection
            .query_row(
                "SELECT state FROM developer_github_setup WHERE id=1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let persisted: GithubSetupState = serde_json::from_str(&persisted).unwrap();
        assert_eq!(persisted.account.state, "signed_out");
    }

    #[test]
    fn github_setup_requests_reject_unknown_or_ambiguous_fields() {
        assert!(serde_json::from_value::<GithubSetupMutation>(json!({
            "action":"refresh_account","expected_revision":4
        }))
        .is_ok());
        for malformed in [
            json!({"action":"refresh_account","expected_revision":4,"operation_id":"ignored"}),
            json!({"action":"list_repositories","expected_revision":4}),
            json!({"action":"create_repository","operation_id":"a8e78ac7-c9a9-47f0-92dc-b35777880967","expected_login":"owner","name":"repo","visibility":"private","expected_revision":4,"connect":true}),
            json!({"action":"unknown","expected_revision":4}),
        ] {
            assert!(serde_json::from_value::<GithubSetupMutation>(malformed).is_err());
        }
    }

    #[test]
    fn github_setup_outcomes_fail_closed_on_ambiguous_identity_or_effect() {
        let inconsistent = GithubSignInOutcome {
            account: GithubAccountObservation::SignedIn {
                login: "owner".into(),
            },
            command_succeeded: true,
            cancelled: false,
            challenge_seen: true,
            credentials_consistent: false,
        };
        assert_eq!(github_sign_in_completion(&inconsistent).0, "attention");

        let operation = "a8e78ac7-c9a9-47f0-92dc-b35777880967";
        let mut setup = GithubSetupState::default();
        setup
            .start_creation(operation, "owner", "repo", "private")
            .unwrap();
        let observed = GithubRepositoryObservation {
            repository_id: 42,
            name_with_owner: "owner/repo".into(),
            url: "https://github.com/owner/repo".into(),
            visibility: "private".into(),
            default_branch: "main".into(),
            can_push: true,
        };
        let creation = setup.creation_mut(operation).unwrap();
        creation.preflight_absent = true;
        apply_github_creation_observation(
            creation,
            Ok(GithubRepositoryLookup::Present(observed.clone())),
        );
        assert_eq!(creation.state, "attention");

        creation.command_succeeded = true;
        apply_github_creation_observation(creation, Ok(GithubRepositoryLookup::Present(observed)));
        assert_eq!(creation.state, "succeeded");
        assert_eq!(creation.repository_id, Some(42));
    }

    fn feature_with_status(status: &str) -> Feature {
        Feature {
            id: "a8e78ac7-c9a9-47f0-92dc-b35777880967".into(),
            project: "example".into(),
            instruction: "Implement the example".into(),
            validation: "true".into(),
            status: status.into(),
            checkpoint: "applied".into(),
            message: "Original validation evidence".into(),
            edits: Some(vec![Edit {
                path: "result.txt".into(),
                content: "saved".into(),
                before: None,
            }]),
            repair_attempts: 0,
            repair_pending: false,
            repair_history: Vec::new(),
            escalation_count: 0,
            escalation_pending: false,
            escalation_proposal: None,
            escalation_history: Vec::new(),
            auto_ai_repair_limit: None,
            auto_repair_lifecycle: default_auto_repair_lifecycle(),
            auto_repair_reason: String::new(),
            auto_repair_epoch: 0,
            auto_repair_step_started_at_ms: None,
            auto_repair_policy_revision: None,
            escalation_evidence_reserved: 0,
            review_evidence_reserved: 0,
            last_failure_kind: String::new(),
            last_code_failure_summary: String::new(),
            auto_repair_limit_project_baseline: Default::default(),
            auto_repair_limit_unadmitted_sha256: None,
            auto_repair_limit_volatile_sha256: None,
            model_target: "mac".into(),
            review_status: "pending".into(),
            review_model: REVIEW_MODEL_ID.into(),
            review_reasoning_effort: DEFAULT_REASONING_EFFORT.into(),
            review_binding_version: 1,
            review_attempts: 0,
            review_summary: String::new(),
            review_pending: None,
            review_history: Vec::new(),
            reviewer_selection_history: Vec::new(),
            planning: None,
            cumulative_evidence_version: 1,
            tool_workspace_revision: 0,
            publication_selection_frozen: false,
            publication_binding: None,
            publication_candidate: Vec::new(),
            publication: None,
        }
    }

    fn feature_with_escalation(status: &str) -> Feature {
        let mut feature = feature_with_status(status);
        feature.repair_attempts = REPAIR_LIMIT;
        feature.escalation_count = 1;
        feature.escalation_pending = true;
        feature.checkpoint = "escalation_1_applied".into();
        feature.escalation_proposal = Some(RepairEscalationProposal {
            proposal_id: "3680c592-7d0b-4662-95a8-1ec303d08219".into(),
            attempt: 1,
            feature_id: feature.id.clone(),
            feature_checkpoint: "failed_validation".into(),
            binding_revision: 8,
            model_target: "mac".into(),
            model: "mac-coder".into(),
            chat_id: None,
            chat_request_id: "07fcd785-a83e-4a60-9941-f9150f78b4db".into(),
            chat_model_target: "windows".into(),
            chat_model: "windows-coder".into(),
            diagnosis: "The existing test contradicts the approved GUI change.".into(),
            diagnosis_sha256: "1".repeat(64),
            status: "applied".into(),
            summary: "Correct the exact widget assertion".into(),
            error: None,
            files: vec![RepairEscalationFile {
                path: "tests/test_gui.py".into(),
                before: Some("assert widget == 'old'\n".into()),
                after: "assert widget == 'new'\n".into(),
                protected: true,
            }],
            protected_inputs: std::collections::BTreeMap::from([(
                "tests/test_gui.py".into(),
                "2".repeat(64),
            )]),
            applied_paths: vec!["tests/test_gui.py".into()],
            apply_request_id: Some("b35972e8-b6a8-4cb9-96fa-cc7a68d2e8b2".into()),
            source: manual_escalation_source(),
            automatic_epoch: None,
            policy_revision: None,
            limit_snapshot: Some(ESCALATION_LIMIT),
            project_state_sha256: None,
            review_slot_terminal: false,
        });
        let proposal = feature.escalation_proposal.as_ref().unwrap();
        feature.escalation_history.push(RepairEscalationEvidence {
            proposal_id: "a6ea29b7-8800-4506-986f-448005687215".into(),
            attempt: 0,
            model_target: "mac".into(),
            model: "older-model".into(),
            chat_id: None,
            chat_request_id: "418ab506-8295-4b4c-90c7-30f8dd7dcedd".into(),
            diagnosis_sha256: "0".repeat(64),
            outcome: "approved_to_apply".into(),
            proposal_sha256: Some("0".repeat(64)),
            candidate_sha256: None,
            summary: "older approval".into(),
            source: manual_escalation_source(),
            automatic_epoch: None,
            policy_revision: None,
            limit_snapshot: Some(ESCALATION_LIMIT),
            project_state_sha256: None,
            authorization_revision: None,
        });
        feature.escalation_history.push(RepairEscalationEvidence {
            proposal_id: proposal.proposal_id.clone(),
            attempt: proposal.attempt,
            model_target: proposal.model_target.clone(),
            model: proposal.model.clone(),
            chat_id: proposal.chat_id.clone(),
            chat_request_id: proposal.chat_request_id.clone(),
            diagnosis_sha256: proposal.diagnosis_sha256.clone(),
            outcome: "approved_to_apply".into(),
            proposal_sha256: Some(repair_escalation_proposal_sha256(proposal).unwrap()),
            candidate_sha256: Some(repair_escalation_candidate_sha256(proposal).unwrap()),
            summary: "approved".into(),
            source: proposal.source.clone(),
            automatic_epoch: proposal.automatic_epoch,
            policy_revision: proposal.policy_revision,
            limit_snapshot: proposal.limit_snapshot,
            project_state_sha256: proposal.project_state_sha256.clone(),
            authorization_revision: None,
        });
        feature
    }

    fn feature_with_publication(status: &str) -> Feature {
        let mut feature = feature_with_status(status);
        feature.review_status = "approved".into();
        feature.review_attempts = 1;
        feature.publication_selection_frozen = true;
        feature.publication_binding = Some(ProjectBinding {
            project: feature.project.clone(),
            repository_url: "https://github.com/owner/example.git".into(),
            repository_slug: "owner/example".into(),
            base_branch: "main".into(),
            strict_required_checks: true,
            required_checks: vec![developer_publication::RequiredCheck {
                context: "build".into(),
                integration_id: Some(7),
            }],
        });
        feature.publication_candidate = vec![FrozenPublicationFile {
            path: "result.txt".into(),
            before_sha256: None,
            content_sha256: hash(b"saved"),
            content: "saved".into(),
            classification: "ordinary_source".into(),
        }];
        let validation_evidence_sha256 = "1".repeat(64);
        let packet = DeveloperReviewPacket {
            schema_version: 1,
            feature_id: feature.id.clone(),
            project: feature.project.clone(),
            instruction: feature.instruction.clone(),
            approved_plan_sha256: None,
            approved_plan: None,
            validation_command: feature.validation.clone(),
            validation_evidence_sha256: validation_evidence_sha256.clone(),
            provider_id: REVIEW_PROVIDER_ID.into(),
            model_id: feature.review_model.clone(),
            reasoning_effort: feature.review_reasoning_effort.clone(),
            files: vec![DeveloperReviewFile {
                classification: "ordinary_source".into(),
                path: "result.txt".into(),
                before_sha256: None,
                content_sha256: hash(b"saved"),
                content: "saved".into(),
            }],
        };
        feature.review_history.push(ReviewAttemptEvidence {
            attempt: 1,
            packet_sha256: packet.sha256().unwrap(),
            validation_evidence_sha256,
            outcome: "approved".into(),
            decision_sha256: Some("2".repeat(64)),
            blocking_findings: Vec::new(),
            summary: "approved".into(),
        });
        let input = publication_input(&feature).unwrap();
        feature.publication = Some(PublicationRecord::pending(&input).unwrap());
        feature
    }

    #[test]
    fn publication_request_is_action_specific_and_rejects_unknown_fields() {
        let save = json!({
            "action":"save_connection",
            "project":"example",
            "repository_url":"https://github.com/owner/example",
            "base_branch":"main",
            "expected_revision":7
        });
        assert!(matches!(
            serde_json::from_value::<PublicationMutation>(save.clone()).unwrap(),
            PublicationMutation::SaveConnection {
                expected_revision: 7,
                ..
            }
        ));
        let mut unknown = save;
        unknown["merge_without_checks"] = Value::Bool(true);
        assert!(serde_json::from_value::<PublicationMutation>(unknown).is_err());
        assert!(serde_json::from_value::<PublicationMutation>(json!({
            "action":"reconcile",
            "feature_id":"a8e78ac7-c9a9-47f0-92dc-b35777880967",
            "expected_revision":8
        }))
        .is_err());
    }

    #[test]
    fn frozen_publication_candidate_is_the_exact_approved_cumulative_packet() {
        let feature = feature_with_publication("running");
        let input = publication_input(&feature).unwrap();
        assert_eq!(input.files.len(), 1);
        assert_eq!(input.files[0].before_sha256, None);
        assert_eq!(input.files[0].content, "saved");

        let mut tampered = feature;
        tampered.publication_candidate[0].content = "different".into();
        assert!(publication_input(&tampered)
            .unwrap_err()
            .to_string()
            .contains("content changed"));
    }

    #[test]
    fn unresolved_publication_blocks_queue_mutations_and_requires_reconciliation() {
        for publication_status in ["pending", "running", "attention"] {
            let mut feature = feature_with_publication("failed");
            feature.publication.as_mut().unwrap().status = publication_status.into();
            if publication_status == "attention" {
                feature.publication.as_mut().unwrap().stage = "wait_required_checks".into();
            }
            let mut state = Snapshot {
                queue: vec![feature.clone()],
                ..Default::default()
            };
            assert!(Engine::publication_unresolved(&state));
            assert!(!feature_reviewer_state_is_changeable(&feature));
            assert!(remove_feature(&mut state, &feature.id).is_err());
        }
    }

    #[test]
    fn publication_selection_freezes_connected_or_local_only_once() {
        let binding = feature_with_publication("running")
            .publication_binding
            .unwrap();
        let mut connected = feature_with_status("queued");
        freeze_publication_selection(&mut connected, Some(binding)).unwrap();
        assert!(connected.publication_selection_frozen);
        assert!(connected.publication_binding.is_some());
        assert!(freeze_publication_selection(&mut connected, None).is_err());

        let mut local = feature_with_status("queued");
        freeze_publication_selection(&mut local, None).unwrap();
        assert!(local.publication_selection_frozen);
        assert!(local.publication_binding.is_none());
    }

    #[test]
    fn stop_or_emergency_dominates_verified_publication_completion() {
        let cancellation = AtomicU8::new(0);
        assert!(!publication_completion_is_cancelled(&cancellation, false));
        cancellation.store(1, Ordering::SeqCst);
        assert!(publication_completion_is_cancelled(&cancellation, false));
        cancellation.store(0, Ordering::SeqCst);
        assert!(publication_completion_is_cancelled(&cancellation, true));
    }

    #[test]
    fn late_stop_does_not_overwrite_durable_publication_success() {
        let mut feature = feature_with_publication("succeeded");
        feature.checkpoint = "publication_merged".into();
        let publication = feature.publication.as_mut().unwrap();
        publication.status = "succeeded".into();
        publication.stage = "complete".into();
        publication.merged_sha = Some("a".repeat(40));
        let retained_publication = serde_json::to_value(&feature.publication).unwrap();

        pause_after_post_run_cancellation(&mut feature);

        assert_eq!(feature.status, "succeeded");
        assert_eq!(feature.checkpoint, "publication_merged");
        assert_eq!(
            serde_json::to_value(&feature.publication).unwrap(),
            retained_publication
        );

        let mut incomplete = feature_with_status("running");
        pause_after_post_run_cancellation(&mut incomplete);
        assert_eq!(incomplete.status, "paused");
    }

    #[test]
    fn rejected_shutdown_does_not_cancel_active_publication() {
        let (_dir, engine) = control_test_engine();
        engine.publication_running.store(true, Ordering::SeqCst);
        engine.publication_cancellation.store(0, Ordering::SeqCst);

        assert!(engine.begin_shutdown().is_err());
        assert_eq!(engine.publication_cancellation.load(Ordering::SeqCst), 0);
        assert!(!engine.shutdown.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn publication_running_clears_when_attention_persistence_fails() {
        let (_dir, engine) = control_test_engine();
        let feature = feature_with_publication("failed");
        let feature_id = feature.id.clone();
        engine
            .change(|state| {
                state.queue[0] = feature;
                Ok(())
            })
            .unwrap();
        engine
            .database
            .lock()
            .unwrap()
            .connection
            .execute_batch(
                "CREATE TRIGGER reject_publication_write BEFORE UPDATE ON developer_state BEGIN SELECT RAISE(ABORT,'fixture publication write failure'); END;",
            )
            .unwrap();
        engine.publication_running.store(true, Ordering::SeqCst);

        let error = engine
            .execute_publication_reserved(&feature_id)
            .await
            .unwrap_err();

        assert!(format!("{error:#}").contains("fixture publication write failure"));
        assert!(!engine.publication_running.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn status_reconciles_side_chat_mutation_before_exposing_review_evidence() {
        let (directory, engine) = control_test_engine();
        engine
            .change(|state| {
                let feature = &mut state.queue[0];
                feature.status = "succeeded".into();
                feature.review_status = "approved".into();
                feature.checkpoint = "review_1_approved".into();
                Ok(())
            })
            .unwrap();
        let mutation = ToolProjectMutation {
            revision: 1,
            request_id: Uuid::new_v4().to_string(),
            feature_id: None,
            edits: vec![developer_tools::ToolMutationEdit {
                path: "sidechat.py".into(),
                before_sha256: None,
                after: Some("VALUE = 2\n".into()),
            }],
            unreviewable_paths: Vec::new(),
        };
        let connection =
            Connection::open(directory.path().join("data").join("developer.sqlite3")).unwrap();
        connection
            .execute(
                "INSERT INTO developer_tool_workspace(project,revision) VALUES('example',1)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO developer_tool_mutation(project,revision,request_id,feature_id,evidence)
                 VALUES('example',1,?1,NULL,?2)",
                (
                    &mutation.request_id,
                    serde_json::to_string(&mutation).unwrap(),
                ),
            )
            .unwrap();

        let response = status(State(engine.clone()), authorized_headers()).await;
        assert_eq!(response.0, StatusCode::OK);
        assert_eq!(response.1["queue"][0]["status"], "paused");
        assert_eq!(response.1["queue"][0]["review_status"], "interrupted");
        assert_eq!(
            response.1["queue"][0]["checkpoint"],
            "review_tool_workspace_changed"
        );
        assert_eq!(response.1["queue"][0]["tool_workspace_revision"], 1);
        assert!(response.1["queue"][0]["changed_files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|path| path == "sidechat.py"));
    }

    #[test]
    fn unreviewable_side_chat_mutation_quarantines_prior_evidence() {
        let mut feature = feature_with_status("succeeded");
        feature.review_status = "approved".into();
        let error = anyhow!("Tool session changed an unreviewable file");
        reconcile_feature_tool_mutation(&mut feature, 4, &Err(error)).unwrap();
        assert_eq!(feature.status, "failed");
        assert_eq!(feature.checkpoint, "tool_effects_quarantined");
        assert_eq!(feature.review_status, "interrupted");
        assert_eq!(feature.tool_workspace_revision, 4);
        assert!(feature.message.contains("cannot enter bounded review"));
    }

    #[tokio::test]
    async fn failed_emergency_persistence_blocks_all_work_until_durable_clear_succeeds() {
        let (_dir, engine) = control_test_engine();
        engine
            .database
            .lock()
            .unwrap()
            .connection
            .execute_batch(
                "CREATE TRIGGER reject_write BEFORE UPDATE ON developer_state BEGIN SELECT RAISE(ABORT,'fixture write failure'); END;",
            )
            .unwrap();

        assert_eq!(
            control(
                State(engine.clone()),
                authorized_headers(),
                Json(json!({"action":"emergency"})),
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        assert_eq!(engine.snapshot().unwrap()["emergency_paused"], true);
        assert!(engine.start(None, None, None, None).is_err());
        assert!(engine
            .repair("a8e78ac7-c9a9-47f0-92dc-b35777880967", 0)
            .is_err());
        assert_eq!(
            chat_start(
                State(engine.clone()),
                authorized_headers(),
                Json(json!({
                    "project":"example",
                    "message":"diagnose the failure",
                    "id":"418ab506-8295-4b4c-90c7-30f8dd7dcedd",
                    "model_target":"mac",
                })),
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        assert!(engine
            .planning_mutate(json!({
                "action":"start",
                "feature_id":"123faf2a-6a8e-411b-af3e-267d9ea49747",
                "request_id":"b35972e8-b6a8-4cb9-96fa-cc7a68d2e8b2",
                "expected_revision":0,
                "project":"example",
                "instruction":"Plan the repair",
                "validation":"true",
                "model_target":"mac",
            }))
            .is_err());
        for action in ["prepare", "approve_and_apply"] {
            let request = RepairEscalationMutation {
                action: action.into(),
                feature_id: "a8e78ac7-c9a9-47f0-92dc-b35777880967".into(),
                expected_revision: 1,
                expected_checkpoint: Some("applied".into()),
                model_target: Some("mac".into()),
                chat_request_id: Some("07fcd785-a83e-4a60-9941-f9150f78b4db".into()),
                chat_id: None,
                diagnosis_sha256: Some("1".repeat(64)),
                proposal_id: Some("3680c592-7d0b-4662-95a8-1ec303d08219".into()),
            };
            let result = if action == "prepare" {
                engine.prepare_escalation(request)
            } else {
                engine.approve_escalation(request)
            };
            assert!(result.is_err(), "{action} must remain blocked");
        }

        assert_eq!(
            control(
                State(engine.clone()),
                authorized_headers(),
                Json(json!({"action":"clear_emergency"})),
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        assert_eq!(engine.snapshot().unwrap()["emergency_paused"], true);
        assert_eq!(engine.cancellation.load(Ordering::SeqCst), 2);

        engine
            .database
            .lock()
            .unwrap()
            .connection
            .execute_batch("DROP TRIGGER reject_write;")
            .unwrap();
        assert_eq!(
            control(
                State(engine.clone()),
                authorized_headers(),
                Json(json!({"action":"clear_emergency"})),
            )
            .await
            .0,
            StatusCode::OK
        );
        assert_eq!(engine.snapshot().unwrap()["emergency_paused"], false);
        assert_eq!(engine.cancellation.load(Ordering::SeqCst), 0);
        assert!(!engine.running.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn unconfirmed_validation_cleanup_durably_pauses_and_quarantines_all_work() {
        let (_dir, engine) = control_test_engine();
        let feature_id = engine.database.lock().unwrap().state.queue[0].id.clone();
        engine
            .change(|state| {
                let feature = &mut state.queue[0];
                feature.status = "running".into();
                feature.checkpoint = "repair_1_applied".into();
                feature.repair_attempts = 1;
                feature.repair_pending = true;
                set_auto_repair_lifecycle(feature, "running", "fixture repair is running")?;
                Ok(())
            })
            .unwrap();
        engine.repair_loop_authorized.store(true, Ordering::SeqCst);

        engine
            .quarantine_unconfirmed_validation_cleanup(&feature_id, None)
            .unwrap();

        let database = engine.database.lock().unwrap();
        assert!(database.state.emergency_paused);
        let feature = &database.state.queue[0];
        assert_eq!(feature.status, "failed");
        assert_eq!(feature.checkpoint, "validation_cleanup_unconfirmed");
        assert_eq!(feature.last_failure_kind, "operational");
        assert_eq!(feature.auto_repair_lifecycle, "quarantined");
        assert!(!feature.repair_pending);
        assert!(feature.message.contains("Emergency Pause is active"));
        assert!(feature.message.contains("will not replay automatically"));
        drop(database);
        assert_eq!(engine.cancellation.load(Ordering::SeqCst), 2);
        assert_eq!(engine.publication_cancellation.load(Ordering::SeqCst), 2);
        assert!(engine.tool_cancellation.load(Ordering::SeqCst));
        assert!(!engine.repair_loop_authorized.load(Ordering::SeqCst));
        assert!(engine.start(None, None, None, None).is_err());
        assert!(engine.repair(&feature_id, 1).is_err());
        assert!(engine
            .planning_mutate(json!({
                "action":"start",
                "feature_id":"123faf2a-6a8e-411b-af3e-267d9ea49747",
                "request_id":"b35972e8-b6a8-4cb9-96fa-cc7a68d2e8b2",
                "expected_revision":0,
                "project":"example",
                "instruction":"Plan the repair",
                "validation":"true",
                "model_target":"mac",
            }))
            .is_err());
        assert_eq!(
            chat_start(
                State(engine.clone()),
                authorized_headers(),
                Json(json!({
                    "project":"example",
                    "message":"diagnose the failure",
                    "id":"418ab506-8295-4b4c-90c7-30f8dd7dcedd",
                    "model_target":"mac",
                })),
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
    }

    #[test]
    fn unconfirmed_validation_cleanup_persistence_failure_keeps_volatile_emergency_latch() {
        let (_dir, engine) = control_test_engine();
        let feature_id = engine.database.lock().unwrap().state.queue[0].id.clone();
        engine
            .database
            .lock()
            .unwrap()
            .connection
            .execute_batch(
                "CREATE TRIGGER reject_cleanup_pause BEFORE UPDATE ON developer_state BEGIN SELECT RAISE(ABORT,'fixture write failure'); END;",
            )
            .unwrap();

        assert!(engine
            .quarantine_unconfirmed_validation_cleanup(&feature_id, None)
            .is_err());
        assert_eq!(engine.cancellation.load(Ordering::SeqCst), 2);
        assert_eq!(engine.publication_cancellation.load(Ordering::SeqCst), 2);
        assert!(engine.tool_cancellation.load(Ordering::SeqCst));
        assert_eq!(engine.snapshot().unwrap()["emergency_paused"], true);
        assert!(engine.start(None, None, None, None).is_err());
    }

    #[test]
    fn cleanup_ambiguity_survives_candidate_evidence_persistence_failure() {
        let (_dir, engine) = control_test_engine();
        let feature_id = engine.database.lock().unwrap().state.queue[0].id.clone();
        let live_edits = vec![Edit {
            path: "requirements.txt".into(),
            content: "pytest==9.0.0\n".into(),
            before: None,
        }];
        let cleanup_error = engine
            .fail_closed_validation_result::<()>(
                &feature_id,
                Some((17, &live_edits)),
                Err(anyhow!(UnconfirmedTermination(
                    "fixture cleanup query failed".into()
                ))),
            )
            .unwrap_err();
        assert!(cleanup_error
            .downcast_ref::<UnconfirmedTermination>()
            .is_some());
        engine
            .database
            .lock()
            .unwrap()
            .connection
            .execute_batch(
                "CREATE TRIGGER reject_candidate_evidence BEFORE UPDATE ON developer_state BEGIN SELECT RAISE(ABORT,'fixture candidate evidence write failure'); END;",
            )
            .unwrap();
        assert!(engine
            .record_tool_candidate_failure(
                &feature_id,
                1,
                &[],
                false,
                "fixture ordinary candidate failure",
            )
            .is_err());

        let returned = engine
            .finish_environment_preparation_validation_error(&feature_id, 1, &[], cleanup_error)
            .err()
            .unwrap();
        assert!(returned.downcast_ref::<UnconfirmedTermination>().is_some());
        let database = engine.database.lock().unwrap();
        assert!(database.state.emergency_paused);
        assert_eq!(
            database.state.queue[0].checkpoint,
            "validation_cleanup_unconfirmed"
        );
        assert_eq!(database.state.queue[0].auto_repair_lifecycle, "quarantined");
        assert_eq!(database.state.queue[0].tool_workspace_revision, 17);
        let retained = database.state.queue[0].edits.as_deref().unwrap();
        let retained_live_edit = retained
            .iter()
            .filter(|edit| edit.path == live_edits[0].path)
            .collect::<Vec<_>>();
        assert_eq!(retained_live_edit.len(), 1);
        assert_eq!(retained_live_edit[0].content, live_edits[0].content);
        assert_eq!(retained_live_edit[0].before, live_edits[0].before);
    }

    #[tokio::test]
    async fn failed_planning_completion_persistence_recovers_on_status_poll_without_provider_reexecution(
    ) {
        let (_dir, engine) = control_test_engine();
        let raw = json!({
            "action":"start",
            "feature_id":"123faf2a-6a8e-411b-af3e-267d9ea49747",
            "request_id":"b35972e8-b6a8-4cb9-96fa-cc7a68d2e8b2",
            "expected_revision":0,
            "project":"example",
            "instruction":"Plan the repair",
            "validation":"true",
            "model_target":"mac",
        });
        let mut session = new_session(
            raw["feature_id"].as_str().unwrap(),
            "example",
            "Plan the repair",
            "true",
            "mac",
            REVIEW_MODEL_ID,
            DEFAULT_REASONING_EFFORT,
        )
        .unwrap();
        record_request(
            &mut session,
            raw["request_id"].as_str().unwrap(),
            &request_digest(&raw).unwrap(),
            "start",
            0,
        )
        .unwrap();
        let revision = begin_provider(&mut session, "question").unwrap();
        let packet = PlanningPacket {
            schema_version: 1,
            feature_id: session.feature_id.clone(),
            request_id: raw["request_id"].as_str().unwrap().into(),
            skill_sha256: developer_planning::brainstorming_skill_sha256(),
            revision,
            expected_response: expected_response(&session).unwrap().into(),
            provider_id: REVIEW_PROVIDER_ID.into(),
            model_id: REVIEW_MODEL_ID.into(),
            reasoning_effort: DEFAULT_REASONING_EFFORT.into(),
            project: session.project.clone(),
            instruction: session.instruction.clone(),
            validation: session.validation.clone(),
            model_target: session.model_target.clone(),
            answers: Vec::new(),
            understanding_summary: Vec::new(),
            assumptions: None,
            approaches: Vec::new(),
            selected_approach_id: None,
            design_sections: Vec::new(),
            decision_log: Vec::new(),
            context_files: Vec::new(),
            omitted_context_files: 0,
        };
        bind_pending_packet(
            &mut session,
            raw["request_id"].as_str().unwrap(),
            &packet.sha256().unwrap(),
        )
        .unwrap();
        engine
            .change(|state| {
                state.planning_sessions.push(session);
                Ok(())
            })
            .unwrap();
        engine
            .database
            .lock()
            .unwrap()
            .connection
            .execute_batch(
                "CREATE TRIGGER reject_planning_write BEFORE UPDATE ON developer_state BEGIN SELECT RAISE(ABORT,'fixture write failure'); END;",
            )
            .unwrap();

        assert!(engine
            .finish_planning_completion(
                &packet,
                PlanningCompletion::Unavailable("fixture provider unavailable".into()),
            )
            .is_err());
        assert!(engine
            .planning_completion_recovery
            .lock()
            .unwrap()
            .is_some());
        assert!(engine.database.lock().unwrap().state.planning_sessions[0].running);

        engine
            .database
            .lock()
            .unwrap()
            .connection
            .execute_batch("DROP TRIGGER reject_planning_write;")
            .unwrap();
        let recovered = planning_status(
            State(engine.clone()),
            authorized_headers(),
            Query(PlanningQuery {
                id: raw["feature_id"].as_str().unwrap().into(),
            }),
        )
        .await
        .1
         .0;
        assert_eq!(recovered["running"], false);
        assert_eq!(recovered["availability"], "unavailable");
        assert_eq!(recovered["error"], "fixture provider unavailable");
        assert!(engine
            .planning_completion_recovery
            .lock()
            .unwrap()
            .is_none());
        assert_eq!(recovered["history"].as_array().unwrap().len(), 1);

        let raw = json!({
            "action":"start",
            "feature_id":"c3749598-bf55-4d10-a9d7-79f48c866621",
            "request_id":"42edccfe-39da-428e-abba-2210e8e399f4",
            "expected_revision":0,
            "project":"example",
            "instruction":"Plan another repair",
            "validation":"true",
            "model_target":"mac",
        });
        let mut session = new_session(
            raw["feature_id"].as_str().unwrap(),
            "example",
            "Plan another repair",
            "true",
            "mac",
            REVIEW_MODEL_ID,
            DEFAULT_REASONING_EFFORT,
        )
        .unwrap();
        record_request(
            &mut session,
            raw["request_id"].as_str().unwrap(),
            &request_digest(&raw).unwrap(),
            "start",
            0,
        )
        .unwrap();
        let revision = begin_provider(&mut session, "question").unwrap();
        let packet = PlanningPacket {
            schema_version: 1,
            feature_id: session.feature_id.clone(),
            request_id: raw["request_id"].as_str().unwrap().into(),
            skill_sha256: developer_planning::brainstorming_skill_sha256(),
            revision,
            expected_response: expected_response(&session).unwrap().into(),
            provider_id: REVIEW_PROVIDER_ID.into(),
            model_id: REVIEW_MODEL_ID.into(),
            reasoning_effort: DEFAULT_REASONING_EFFORT.into(),
            project: session.project.clone(),
            instruction: session.instruction.clone(),
            validation: session.validation.clone(),
            model_target: session.model_target.clone(),
            answers: Vec::new(),
            understanding_summary: Vec::new(),
            assumptions: None,
            approaches: Vec::new(),
            selected_approach_id: None,
            design_sections: Vec::new(),
            decision_log: Vec::new(),
            context_files: Vec::new(),
            omitted_context_files: 0,
        };
        bind_pending_packet(
            &mut session,
            raw["request_id"].as_str().unwrap(),
            &packet.sha256().unwrap(),
        )
        .unwrap();
        engine
            .change(|state| {
                state.planning_sessions.push(session);
                Ok(())
            })
            .unwrap();
        engine
            .database
            .lock()
            .unwrap()
            .connection
            .execute_batch(
                "CREATE TRIGGER reject_planning_emergency_write BEFORE UPDATE ON developer_state BEGIN SELECT RAISE(ABORT,'fixture emergency write failure'); END;",
            )
            .unwrap();
        assert_eq!(
            control(
                State(engine.clone()),
                authorized_headers(),
                Json(json!({"action":"emergency"})),
            )
            .await
            .0,
            StatusCode::CONFLICT
        );
        let output = PlanningProviderOutput {
            schema_version: 1,
            planning_packet_sha256: packet.sha256().unwrap(),
            provider_id: REVIEW_PROVIDER_ID.into(),
            model_id: REVIEW_MODEL_ID.into(),
            response_kind: "question".into(),
            question: Some(developer_planning::PlanningQuestion {
                text: "Which behavior should change?".into(),
                choices: Vec::new(),
            }),
            understanding_summary: Vec::new(),
            assumptions: None,
            open_questions: Vec::new(),
            approaches: Vec::new(),
            design_section: None,
            design_complete: false,
            decision_log: Vec::new(),
            implementation_plan: None,
            reasoning_effort: DEFAULT_REASONING_EFFORT.into(),
        };
        assert!(engine
            .finish_planning_completion(&packet, PlanningCompletion::Output(Box::new(output)))
            .is_err());
        engine
            .database
            .lock()
            .unwrap()
            .connection
            .execute_batch("DROP TRIGGER reject_planning_emergency_write;")
            .unwrap();
        assert_eq!(
            control(
                State(engine.clone()),
                authorized_headers(),
                Json(json!({"action":"clear_emergency"})),
            )
            .await
            .0,
            StatusCode::OK
        );
        let recovered = planning_status(
            State(engine.clone()),
            authorized_headers(),
            Query(PlanningQuery {
                id: raw["feature_id"].as_str().unwrap().into(),
            }),
        )
        .await
        .1
         .0;
        assert_eq!(recovered["running"], false);
        assert_eq!(recovered["availability"], "unavailable");
        assert!(recovered["error"]
            .as_str()
            .unwrap()
            .contains("Emergency Pause interrupted"));
        assert_eq!(recovered["question"], Value::Null);
    }

    #[test]
    fn planning_retry_rejects_a_retired_session_pinned_orchestrator() {
        let (_dir, engine) = control_test_engine();
        let feature_id = "123faf2a-6a8e-411b-af3e-267d9ea49747";
        let mut session = new_session(
            feature_id,
            "example",
            "Plan the repair",
            "true",
            "mac",
            REVIEW_MODEL_ID,
            DEFAULT_REASONING_EFFORT,
        )
        .unwrap();
        session.model = "gpt-retired-planner".into();
        session.availability = "unavailable".into();
        session.error = Some("fixture provider unavailable".into());
        engine
            .change(|state| {
                state.planning_sessions.push(session);
                Ok(())
            })
            .unwrap();

        let result = engine.planning_mutate(json!({
            "action":"retry",
            "feature_id":feature_id,
            "request_id":"b35972e8-b6a8-4cb9-96fa-cc7a68d2e8b2",
            "expected_revision":0,
        }));

        assert!(
            format!("{:#}", result.unwrap_err()).contains("Selected orchestrator is unavailable")
        );
        assert!(!engine.planning_running.load(Ordering::SeqCst));
    }

    #[test]
    fn removal_is_a_durable_idempotent_tombstone_for_removable_states() {
        for status in ["queued", "failed", "paused"] {
            let mut state = Snapshot {
                queue: vec![feature_with_status(status)],
                ..Default::default()
            };
            assert!(remove_feature(&mut state, "a8e78ac7-c9a9-47f0-92dc-b35777880967").unwrap());
            let removed = &state.queue[0];
            assert_eq!(removed.status, "removed");
            assert_eq!(removed.checkpoint, "applied");
            assert_eq!(removed.message, "Original validation evidence");
            assert_eq!(removed.edits.as_ref().unwrap()[0].path, "result.txt");
            assert!(!remove_feature(&mut state, "a8e78ac7-c9a9-47f0-92dc-b35777880967").unwrap());
        }

        let uppercase_id = "A8E78AC7-C9A9-47F0-92DC-B35777880967";
        let mut uppercase = feature_with_status("queued");
        uppercase.id = uppercase_id.into();
        let mut state = Snapshot {
            queue: vec![uppercase],
            ..Default::default()
        };
        assert!(remove_feature(&mut state, uppercase_id).unwrap());
    }

    #[test]
    fn removal_rejects_malformed_unknown_running_and_completed_ids() {
        for status in ["running", "succeeded"] {
            let mut state = Snapshot {
                queue: vec![feature_with_status(status)],
                ..Default::default()
            };
            assert!(remove_feature(&mut state, "a8e78ac7-c9a9-47f0-92dc-b35777880967").is_err());
            assert_eq!(state.queue[0].status, status);
        }
        let mut state = Snapshot::default();
        assert!(remove_feature(&mut state, "not-a-uuid").is_err());
        assert!(remove_feature(&mut state, "123faf2a-6a8e-411b-af3e-267d9ea49747").is_err());
    }

    #[test]
    fn exhausted_feature_reviewer_change_preserves_work_and_requires_a_fresh_packet() {
        let project = tempfile::tempdir().unwrap();
        fs::write(project.path().join("result.txt"), "saved").unwrap();
        let project_root = fs::canonicalize(project.path()).unwrap();
        let mut feature = feature_with_status("failed");
        feature.checkpoint = "review_3_unavailable".into();
        feature.repair_attempts = REPAIR_LIMIT;
        feature.repair_history = (1..=REPAIR_LIMIT)
            .map(|attempt| RepairAttemptEvidence {
                attempt,
                prior_checkpoint: format!("repair_{attempt}_applied"),
                prior_message: format!("repair {attempt} evidence"),
                prior_edits: Vec::new(),
            })
            .collect();
        feature.review_attempts = 3;
        feature.review_status = "unavailable".into();
        feature.review_history.push(ReviewAttemptEvidence {
            attempt: 3,
            packet_sha256: "1".repeat(64),
            validation_evidence_sha256: "2".repeat(64),
            outcome: "unavailable".into(),
            decision_sha256: None,
            blocking_findings: Vec::new(),
            summary: "provider unavailable".into(),
        });
        let preserved = serde_json::to_value((
            &feature.edits,
            &feature.repair_history,
            &feature.review_history,
            &feature.planning,
        ))
        .unwrap();

        change_feature_reviewer(
            &mut feature,
            AiSelection {
                model: "gpt-5.3-codex-spark".into(),
                reasoning_effort: "high".into(),
            },
            42,
        )
        .unwrap();

        assert_eq!(feature.status, "failed");
        assert_eq!(feature.checkpoint, "review_3_unavailable");
        assert_eq!(feature.repair_attempts, REPAIR_LIMIT);
        assert!(!feature.repair_pending);
        assert_eq!(
            serde_json::to_value((
                &feature.edits,
                &feature.repair_history,
                &feature.review_history,
                &feature.planning,
            ))
            .unwrap(),
            preserved
        );
        assert_eq!(feature.review_status, "pending");
        assert!(feature.review_pending.is_none());
        assert_eq!(feature.review_model, "gpt-5.3-codex-spark");
        assert_eq!(feature.review_reasoning_effort, "high");
        assert_eq!(feature.reviewer_selection_history.len(), 1);
        assert_eq!(feature.reviewer_selection_history[0].revision, 42);
        let packet = developer_review_packet(
            &feature,
            &project_root,
            feature.edits.as_deref().unwrap(),
            &"3".repeat(64),
        )
        .unwrap();
        assert_eq!(packet.model_id, "gpt-5.3-codex-spark");
        assert_eq!(packet.reasoning_effort, "high");
    }

    #[test]
    fn reviewer_change_rejects_terminal_and_unsafe_escalation_states() {
        for status in ["running", "succeeded", "removed"] {
            let mut feature = feature_with_status(status);
            assert!(change_feature_reviewer(
                &mut feature,
                AiSelection {
                    model: "gpt-5.3-codex-spark".into(),
                    reasoning_effort: "high".into(),
                },
                9,
            )
            .is_err());
        }
        let mut interrupted = feature_with_status("failed");
        interrupted.checkpoint = "escalation_1_apply_interrupted".into();
        assert!(!feature_reviewer_state_is_changeable(&interrupted));
        for checkpoint in [
            "staged_tool_candidate_quarantined",
            "tool_effects_quarantined",
            "tool_workspace_changed_requires_proposal",
            "future_recovery_state",
        ] {
            let mut quarantined = feature_with_status("failed");
            quarantined.checkpoint = checkpoint.into();
            assert!(!feature_reviewer_state_is_changeable(&quarantined));
        }
        let mut applying = feature_with_escalation("paused");
        assert!(!feature_reviewer_state_is_changeable(&applying));
        applying.escalation_pending = false;
        applying.escalation_proposal.as_mut().unwrap().status = "applying".into();
        assert!(!feature_reviewer_state_is_changeable(&applying));
    }

    #[test]
    fn reviewer_change_interrupts_pending_review_and_cancels_stale_ready_proposal() {
        let mut feature = feature_with_escalation("failed");
        feature.escalation_pending = false;
        feature.checkpoint = "review_4_pending".into();
        feature.review_attempts = 4;
        feature.review_pending = Some(ReviewPendingEvidence {
            attempt: 4,
            packet_sha256: "4".repeat(64),
            validation_evidence_sha256: "5".repeat(64),
        });
        feature.escalation_proposal.as_mut().unwrap().status = "ready".into();
        let review_history_len = feature.review_history.len();
        let escalation_history_len = feature.escalation_history.len();
        change_feature_reviewer(
            &mut feature,
            AiSelection {
                model: "gpt-5.3-codex-spark".into(),
                reasoning_effort: "medium".into(),
            },
            17,
        )
        .unwrap();
        assert_eq!(feature.checkpoint, "review_4_interrupted");
        assert_eq!(feature.review_history.len(), review_history_len + 1);
        assert_eq!(
            feature.review_history.last().unwrap().outcome,
            "interrupted"
        );
        assert_eq!(feature.escalation_history.len(), escalation_history_len + 1);
        assert_eq!(
            feature.escalation_history.last().unwrap().outcome,
            "cancelled"
        );
        assert_eq!(
            feature.escalation_proposal.as_ref().unwrap().status,
            "cancelled"
        );
        assert_eq!(feature.review_status, "pending");
    }

    #[test]
    fn feature_reviewer_request_is_strict_and_revision_bound() {
        let value = json!({
            "id":"a8e78ac7-c9a9-47f0-92dc-b35777880967",
            "expected_revision":8,
            "expected_checkpoint":"review_3_unavailable",
            "expected_model":"gpt-5.6-sol",
            "expected_reasoning_effort":"high",
            "reviewer":{"model":"gpt-5.3-codex-spark","reasoning_effort":"high"}
        });
        let request: FeatureReviewerMutation = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(request.expected_revision, 8);
        assert_eq!(request.expected_model, "gpt-5.6-sol");
        let mut unknown = value;
        unknown["start"] = Value::Bool(true);
        assert!(serde_json::from_value::<FeatureReviewerMutation>(unknown).is_err());
    }

    #[test]
    fn approved_review_binding_rejects_file_drift_before_success() {
        let project = tempfile::tempdir().unwrap();
        fs::write(project.path().join("result.txt"), "saved").unwrap();
        let project_root = fs::canonicalize(project.path()).unwrap();
        let validation_evidence_sha256 = "1".repeat(64);
        let mut feature = feature_with_status("running");
        feature.review_status = "approved".into();
        feature.review_attempts = 1;
        let packet = developer_review_packet(
            &feature,
            &project_root,
            feature.edits.as_deref().unwrap(),
            &validation_evidence_sha256,
        )
        .unwrap();
        feature.review_history.push(ReviewAttemptEvidence {
            attempt: 1,
            packet_sha256: packet.sha256().unwrap(),
            validation_evidence_sha256,
            outcome: "approved".into(),
            decision_sha256: Some("2".repeat(64)),
            blocking_findings: Vec::new(),
            summary: "approved".into(),
        });
        verify_approved_review_binding(&feature, &project_root).unwrap();

        fs::write(project.path().join("result.txt"), "owner drift").unwrap();
        let error = verify_approved_review_binding(&feature, &project_root).unwrap_err();
        assert!(error.to_string().contains("changed"));
    }

    #[test]
    fn review_packet_preserves_the_earliest_baseline_across_noop_repairs() {
        let project = tempfile::tempdir().unwrap();
        let current_gui = "struct DeveloperView { let title = \"Feature Conveyor\" }\n";
        fs::write(project.path().join("DeveloperView.swift"), current_gui).unwrap();
        let project_root = fs::canonicalize(project.path()).unwrap();
        let original_gui = "struct DeveloperView {}\n";
        let first_generated = "struct DeveloperView { let title = \"Developer\" }\n";

        for earliest_before in [None, Some(hash(original_gui.as_bytes()))] {
            let mut feature = feature_with_status("running");
            feature.repair_attempts = 3;
            feature.repair_history = vec![
                RepairAttemptEvidence {
                    attempt: 1,
                    prior_checkpoint: "applied".into(),
                    prior_message: "first validation failure".into(),
                    prior_edits: vec![RepairEditEvidence {
                        path: "DeveloperView.swift".into(),
                        before: earliest_before.clone(),
                        content_hash: hash(first_generated.as_bytes()),
                    }],
                },
                RepairAttemptEvidence {
                    attempt: 2,
                    prior_checkpoint: "repair_1_applied".into(),
                    prior_message: "second validation failure".into(),
                    prior_edits: vec![RepairEditEvidence {
                        path: "DeveloperView.swift".into(),
                        before: Some(hash(first_generated.as_bytes())),
                        content_hash: hash(current_gui.as_bytes()),
                    }],
                },
                RepairAttemptEvidence {
                    attempt: 3,
                    prior_checkpoint: "repair_2_applied".into(),
                    prior_message: "third validation failure".into(),
                    prior_edits: vec![RepairEditEvidence {
                        path: "DeveloperView.swift".into(),
                        before: Some(hash(current_gui.as_bytes())),
                        content_hash: hash(current_gui.as_bytes()),
                    }],
                },
            ];
            feature.edits = Some(vec![Edit {
                path: "DeveloperView.swift".into(),
                before: Some(hash(current_gui.as_bytes())),
                content: current_gui.into(),
            }]);

            let packet = developer_review_packet(
                &feature,
                &project_root,
                feature.edits.as_deref().unwrap(),
                &"1".repeat(64),
            )
            .unwrap();
            assert_eq!(packet.files.len(), 1);
            assert_eq!(packet.files[0].before_sha256, earliest_before);
            assert_eq!(packet.files[0].content_sha256, hash(current_gui.as_bytes()));

            let mut legacy_feature = feature.clone();
            legacy_feature.repair_history = vec![feature.repair_history[2].clone()];
            let legacy_packet = developer_review_packet(
                &legacy_feature,
                &project_root,
                legacy_feature.edits.as_deref().unwrap(),
                &"1".repeat(64),
            )
            .unwrap();
            feature.review_status = "approved".into();
            feature.review_history.push(ReviewAttemptEvidence {
                attempt: 1,
                packet_sha256: legacy_packet.sha256().unwrap(),
                validation_evidence_sha256: "1".repeat(64),
                outcome: "approved".into(),
                decision_sha256: Some("2".repeat(64)),
                blocking_findings: Vec::new(),
                summary: "approved under overwritten baseline evidence".into(),
            });
            assert!(verify_approved_review_binding(&feature, &project_root).is_err());
        }
    }

    #[test]
    fn escalation_review_edits_retain_the_last_repair_and_test_only_correction() {
        let project = tempfile::tempdir().unwrap();
        fs::create_dir(project.path().join("tests")).unwrap();
        let current_gui = "struct DeveloperView { let title = \"Feature Conveyor\" }\n";
        let old_test = "assert title == 'Developer'\n";
        let corrected_test = "assert title == 'Feature Conveyor'\n";
        fs::write(project.path().join("DeveloperView.swift"), current_gui).unwrap();
        fs::write(project.path().join("tests/test_gui.py"), old_test).unwrap();
        let project_root = fs::canonicalize(project.path()).unwrap();
        let mut feature = feature_with_status("failed");
        feature.repair_attempts = 3;
        feature.repair_history = vec![RepairAttemptEvidence {
            attempt: 1,
            prior_checkpoint: "applied".into(),
            prior_message: "validation failed".into(),
            prior_edits: vec![RepairEditEvidence {
                path: "DeveloperView.swift".into(),
                before: None,
                content_hash: hash("first GUI revision".as_bytes()),
            }],
        }];
        feature.edits = Some(vec![Edit {
            path: "DeveloperView.swift".into(),
            before: Some(hash("prior GUI revision".as_bytes())),
            content: current_gui.into(),
        }]);
        let proposal_files = vec![RepairEscalationFile {
            path: "tests/test_gui.py".into(),
            before: Some(old_test.into()),
            after: corrected_test.into(),
            protected: true,
        }];

        let merged = merge_escalation_review_edits(
            feature.edits.as_deref().unwrap(),
            &proposal_files,
            &project_root,
        )
        .unwrap();
        assert_eq!(
            merged
                .iter()
                .map(|edit| edit.path.as_str())
                .collect::<Vec<_>>(),
            ["DeveloperView.swift", "tests/test_gui.py"]
        );
        feature.edits = Some(merged);
        fs::write(project.path().join("tests/test_gui.py"), corrected_test).unwrap();
        let packet = developer_review_packet(
            &feature,
            &project_root,
            feature.edits.as_deref().unwrap(),
            &"3".repeat(64),
        )
        .unwrap();
        assert_eq!(packet.files.len(), 2);
        assert_eq!(packet.files[0].path, "DeveloperView.swift");
        assert_eq!(packet.files[0].before_sha256, None);
        assert_eq!(packet.files[0].content, current_gui);
        assert_eq!(packet.files[1].path, "tests/test_gui.py");
        assert_eq!(
            packet.files[1].before_sha256,
            Some(hash(old_test.as_bytes()))
        );
        assert_eq!(packet.files[1].content, corrected_test);

        fs::write(project.path().join("DeveloperView.swift"), "owner drift\n").unwrap();
        let error = merge_escalation_review_edits(
            feature.edits.as_deref().unwrap(),
            &proposal_files,
            &project_root,
        )
        .err()
        .unwrap();
        assert!(error
            .to_string()
            .contains("changed before escalation approval"));
    }

    #[test]
    fn cumulative_escalation_merge_preserves_new_and_existing_file_baselines_without_history() {
        let new_file = Edit {
            path: "NewView.swift".into(),
            before: None,
            content: "first revision\n".into(),
        };
        let updated_new_file = Edit {
            path: "NewView.swift".into(),
            before: Some(hash(new_file.content.as_bytes())),
            content: "second revision\n".into(),
        };
        let merged = merge_review_edits(
            std::slice::from_ref(&new_file),
            std::slice::from_ref(&updated_new_file),
        )
        .unwrap();
        assert_eq!(merged[0].before, None);
        assert_eq!(merged[0].content, "second revision\n");

        let original_sha256 = hash(b"owner baseline\n");
        let existing = Edit {
            path: "ExistingView.swift".into(),
            before: Some(original_sha256.clone()),
            content: "first generated revision\n".into(),
        };
        let updated_existing = Edit {
            path: "ExistingView.swift".into(),
            before: Some(hash(existing.content.as_bytes())),
            content: "second generated revision\n".into(),
        };
        let merged = merge_review_edits(
            std::slice::from_ref(&existing),
            std::slice::from_ref(&updated_existing),
        )
        .unwrap();
        assert_eq!(merged[0].before, Some(original_sha256));
        assert_eq!(merged[0].content, "second generated revision\n");
    }

    #[test]
    fn review_retry_checkpoints_revalidate_without_reapplying_stale_edits() {
        for checkpoint in [
            "review_1_unavailable",
            "review_2_interrupted",
            "review_3_rejected",
            "review_binding_changed",
        ] {
            assert!(review_checkpoint_requires_revalidation(checkpoint));
            assert!(checkpoint_has_applied_edits(checkpoint));
        }
        assert!(!review_checkpoint_requires_revalidation("prepared"));
        assert!(!checkpoint_has_applied_edits("prepared"));
    }

    #[test]
    fn auto_repair_step_timestamp_migrates_running_legacy_state_and_rejects_future_state() {
        let mut legacy = feature_with_status("failed");
        legacy.auto_repair_lifecycle = "running".into();
        legacy.auto_repair_step_started_at_ms = None;
        migrate_and_validate_auto_repair_step_timestamp(&mut legacy, 42_000).unwrap();
        assert_eq!(legacy.auto_repair_step_started_at_ms, Some(42_000));
        assert_eq!(
            projected_auto_repair_step_elapsed_ms(&legacy, 42_000),
            Some(0)
        );

        legacy.auto_repair_step_started_at_ms = Some(42_001);
        assert!(
            migrate_and_validate_auto_repair_step_timestamp(&mut legacy, 42_000)
                .unwrap_err()
                .to_string()
                .contains("future")
        );

        legacy.auto_repair_step_started_at_ms = Some(0);
        assert!(migrate_and_validate_auto_repair_step_timestamp(&mut legacy, 42_000).is_err());

        legacy.auto_repair_lifecycle = "held".into();
        legacy.auto_repair_step_started_at_ms = Some(41_999);
        assert!(migrate_and_validate_auto_repair_step_timestamp(&mut legacy, 42_000).is_err());

        let mut malformed = serde_json::to_value(feature_with_status("failed")).unwrap();
        malformed["auto_repair_step_started_at_ms"] = json!("yesterday");
        assert!(serde_json::from_value::<Feature>(malformed).is_err());
    }

    #[test]
    fn auto_repair_step_timestamp_tracks_phase_start_and_clears_for_every_nonrunning_state() {
        let mut feature = feature_with_status("failed");
        feature.auto_repair_lifecycle = "running".into();
        start_auto_repair_step_at(&mut feature, 101).unwrap();
        assert_eq!(feature.auto_repair_step_started_at_ms, Some(101));
        start_auto_repair_step_at(&mut feature, 202).unwrap();
        assert_eq!(feature.auto_repair_step_started_at_ms, Some(202));
        assert_eq!(
            projected_auto_repair_step_elapsed_ms(&feature, 303),
            Some(101)
        );
        feature.auto_repair_step_started_at_ms = Some(1);
        reserve_repair_attempt(&mut feature).unwrap();
        assert!(feature.auto_repair_step_started_at_ms.unwrap() > 1);

        for lifecycle in ["inactive", "held", "limit_reached", "quarantined"] {
            feature.auto_repair_lifecycle = "running".into();
            feature.auto_repair_step_started_at_ms = Some(202);
            set_auto_repair_lifecycle(&mut feature, lifecycle, "test transition").unwrap();
            assert_eq!(feature.auto_repair_step_started_at_ms, None, "{lifecycle}");
            assert_eq!(
                projected_auto_repair_step_elapsed_ms(&feature, 303),
                None,
                "{lifecycle}"
            );
        }
    }

    #[test]
    fn auto_repair_step_timestamp_projection_is_reconnect_stable() {
        let (_dir, engine) = control_test_engine();
        engine
            .change(|state| {
                let feature = &mut state.queue[0];
                feature.auto_repair_lifecycle = "running".into();
                start_auto_repair_step_at(feature, 303)?;
                Ok(())
            })
            .unwrap();
        let first = engine.snapshot().unwrap();
        let second = engine.snapshot().unwrap();
        assert_eq!(first["queue"][0]["auto_repair_step_started_at_ms"], 303);
        assert_eq!(
            first["queue"][0]["auto_repair_step_elapsed_ms"],
            AUTO_REPAIR_STEP_ELAPSED_LIMIT_MS
        );
        assert_eq!(
            second["queue"][0]["auto_repair_step_started_at_ms"],
            first["queue"][0]["auto_repair_step_started_at_ms"]
        );

        let mut future = feature_with_status("failed");
        future.auto_repair_lifecycle = "running".into();
        future.auto_repair_step_started_at_ms = Some(u64::MAX);
        assert_eq!(projected_auto_repair_step_elapsed_ms(&future, 1), Some(0));

        engine
            .change(|state| set_auto_repair_lifecycle(&mut state.queue[0], "held", "fixture hold"))
            .unwrap();
        let held = engine.snapshot().unwrap();
        assert!(held["queue"][0]
            .get("auto_repair_step_elapsed_ms")
            .is_none());
    }

    #[test]
    fn queue_v11_reads_legacy_state_defaults_and_fails_closed_for_old_parsers() {
        let legacy_v1 = r#"{"revision":7,"auto_run":true,"emergency_paused":false,"queue":[]}"#;
        let legacy_v2 = r#"{"revision":8,"auto_run":true,"emergency_paused":false,"queue_v2":[]}"#;
        let legacy_v3 = format!(
            "{{\"revision\":9,\"auto_run\":true,\"emergency_paused\":false,\"queue_v3\":[{}]}}",
            serde_json::to_string(&feature_with_status("queued"))
                .unwrap()
                .replace(",\"model_target\":\"mac\"", "")
        );
        let decoded: Snapshot = serde_json::from_str(legacy_v1).unwrap();
        assert_eq!(decoded.revision, 7);
        assert!(decoded.queue.is_empty());
        assert_eq!(decoded.ai_settings, DeveloperAiSettings::default());
        assert!(!decoded.auto_ai_repair_enabled);
        assert_eq!(
            decoded.auto_ai_repair_max_escalations,
            DEFAULT_AUTO_AI_REPAIR_MAX_ESCALATIONS
        );
        let mut legacy_feature = serde_json::to_value(feature_with_status("queued")).unwrap();
        let legacy_feature = legacy_feature.as_object_mut().unwrap();
        legacy_feature.remove("review_model");
        legacy_feature.remove("review_reasoning_effort");
        legacy_feature.remove("review_binding_version");
        legacy_feature.remove("reviewer_selection_history");
        legacy_feature.remove("publication_selection_frozen");
        legacy_feature.remove("publication_binding");
        legacy_feature.remove("publication_candidate");
        legacy_feature.remove("publication");
        legacy_feature.remove("auto_ai_repair_limit");
        legacy_feature.remove("auto_repair_lifecycle");
        legacy_feature.remove("auto_repair_reason");
        legacy_feature.remove("auto_repair_epoch");
        legacy_feature.remove("auto_repair_step_started_at_ms");
        legacy_feature.remove("auto_repair_policy_revision");
        legacy_feature.remove("escalation_evidence_reserved");
        legacy_feature.remove("review_evidence_reserved");
        legacy_feature.remove("last_failure_kind");
        let legacy_feature: Feature =
            serde_json::from_value(Value::Object(legacy_feature.clone())).unwrap();
        assert_eq!(legacy_feature.review_model, REVIEW_MODEL_ID);
        assert_eq!(
            legacy_feature.review_reasoning_effort,
            DEFAULT_REASONING_EFFORT
        );
        assert_eq!(legacy_feature.review_binding_version, 0);
        assert!(legacy_feature.reviewer_selection_history.is_empty());
        assert!(!legacy_feature.publication_selection_frozen);
        assert!(legacy_feature.publication_binding.is_none());
        assert!(legacy_feature.publication_candidate.is_empty());
        assert!(legacy_feature.publication.is_none());
        assert!(legacy_feature.last_failure_kind.is_empty());
        assert!(legacy_feature.auto_repair_step_started_at_ms.is_none());
        assert!(serde_json::from_str::<Snapshot>(r#"{"revision":1,"auto_run":true,"emergency_paused":false,"queue_v8":[],"ai_settings":{"revision":1,"orchestrator":{"model":"gpt-5.6-sol"}}}"#).is_err());
        assert_eq!(
            serde_json::from_str::<Snapshot>(legacy_v2)
                .unwrap()
                .revision,
            8
        );
        let decoded_v3 = serde_json::from_str::<Snapshot>(&legacy_v3).unwrap();
        assert_eq!(decoded_v3.revision, 9);
        assert_eq!(decoded_v3.queue[0].model_target, "mac");
        let legacy_v5 = format!(
            "{{\"revision\":10,\"auto_run\":true,\"emergency_paused\":false,\"queue_v5\":[{}]}}",
            serde_json::to_string(&feature_with_status("queued")).unwrap()
        );
        assert_eq!(
            serde_json::from_str::<Snapshot>(&legacy_v5)
                .unwrap()
                .revision,
            10
        );
        let legacy_v6 = format!(
            "{{\"revision\":11,\"auto_run\":true,\"emergency_paused\":false,\"queue_v6\":[{}]}}",
            serde_json::to_string(&feature_with_status("queued")).unwrap()
        );
        assert_eq!(
            serde_json::from_str::<Snapshot>(&legacy_v6)
                .unwrap()
                .revision,
            11
        );

        let current = serde_json::to_string(&decoded).unwrap();
        let current_value: Value = serde_json::from_str(&current).unwrap();
        assert!(current_value.get("queue_v11").is_some());
        assert!(current_value.get("queue_v10").is_none());
        assert!(current_value.get("queue_v9").is_none());
        assert!(current_value.get("queue_v8").is_none());
        assert!(current_value.get("queue_v7").is_none());
        assert!(current_value.get("queue_v6").is_none());
        assert!(current_value.get("queue_v5").is_none());
        assert!(current_value.get("queue_v4").is_none());
        assert!(current_value.get("queue_v3").is_none());
        assert!(current_value.get("queue_v2").is_none());
        assert!(current_value.get("queue").is_none());

        #[derive(Deserialize)]
        struct LegacySnapshot {
            #[allow(dead_code)]
            queue: Vec<Feature>,
        }
        assert!(serde_json::from_str::<LegacySnapshot>(&current).is_err());
        #[derive(Deserialize)]
        struct V2Snapshot {
            #[allow(dead_code)]
            queue_v2: Vec<Feature>,
        }
        assert!(serde_json::from_str::<V2Snapshot>(&current).is_err());
        #[derive(Deserialize)]
        struct V3Snapshot {
            #[allow(dead_code)]
            queue_v3: Vec<Feature>,
        }
        assert!(serde_json::from_str::<V3Snapshot>(&current).is_err());
        #[derive(Deserialize)]
        struct V4Snapshot {
            #[allow(dead_code)]
            #[serde(
                rename = "queue_v4",
                alias = "queue_v3",
                alias = "queue_v2",
                alias = "queue"
            )]
            queue: Vec<Feature>,
        }
        assert!(serde_json::from_str::<V4Snapshot>(&current).is_err());
        #[derive(Deserialize)]
        struct V5Snapshot {
            #[allow(dead_code)]
            #[serde(rename = "queue_v5", alias = "queue_v4")]
            queue: Vec<Feature>,
        }
        assert!(serde_json::from_str::<V5Snapshot>(&current).is_err());
        #[derive(Deserialize)]
        struct V6Snapshot {
            #[allow(dead_code)]
            #[serde(rename = "queue_v6", alias = "queue_v5")]
            queue: Vec<Feature>,
        }
        assert!(serde_json::from_str::<V6Snapshot>(&current).is_err());
        #[derive(Deserialize)]
        struct V7Snapshot {
            #[allow(dead_code)]
            #[serde(rename = "queue_v7", alias = "queue_v6")]
            queue: Vec<Feature>,
        }
        assert!(serde_json::from_str::<V7Snapshot>(&current).is_err());
        #[derive(Deserialize)]
        struct V8Snapshot {
            #[allow(dead_code)]
            #[serde(rename = "queue_v8", alias = "queue_v7")]
            queue: Vec<Feature>,
        }
        assert!(serde_json::from_str::<V8Snapshot>(&current).is_err());
        #[derive(Deserialize)]
        struct V9Snapshot {
            #[allow(dead_code)]
            #[serde(rename = "queue_v9", alias = "queue_v8")]
            queue: Vec<Feature>,
        }
        assert!(serde_json::from_str::<V9Snapshot>(&current).is_err());
    }

    #[test]
    fn model_configuration_requires_bounded_ids_and_literal_loopback_http() {
        for accepted in ["http://127.0.0.1:8080/v1", "http://[::1]:8080/v1"] {
            validate_model_url(accepted).unwrap();
        }
        for rejected in [
            "https://127.0.0.1:8080/v1",
            "http://localhost:8080/v1",
            "http://user:password@127.0.0.1:8080/v1",
            "http://127.0.0.1:8080/v1?token=secret",
            "http://192.168.1.10:8080/v1",
            "http://[2001:db8::1]:8080/v1",
        ] {
            assert!(validate_model_url(rejected).is_err(), "{rejected}");
        }
        validate_model_id("qwen-local").unwrap();
        assert!(validate_model_id("").is_err());
        assert!(validate_model_id("   ").is_err());
        assert!(validate_model_id(&"x".repeat(129)).is_err());
        assert!(validate_model_id("model\nname").is_err());

        let duplicate = Args {
            data_dir: PathBuf::from("state"),
            workspace_root: PathBuf::from("projects"),
            bind: "127.0.0.1:7796".parse().unwrap(),
            model_url: "http://127.0.0.1:8080/v1/".into(),
            model: "mac-model".into(),
            windows_model_url: Some("http://127.0.0.1:8080/different-api-path".into()),
            windows_model: Some("windows-model".into()),
            review_codex_executable: PathBuf::from("/private/review/codex"),
            review_codex_home: PathBuf::from("/private/review/home"),
            opencode_executable: None,
            git_executable: None,
            gh_executable: None,
        };
        assert!(configured_model_targets(&duplicate).is_err());
        let ipv4 = reqwest::Url::parse("http://127.0.0.1:8080/v1").unwrap();
        let ipv6_alias = reqwest::Url::parse("http://[::1]:8080/other").unwrap();
        let distinct_port = reqwest::Url::parse("http://127.0.0.1:8081/v1").unwrap();
        assert!(same_loopback_listener(&ipv4, &ipv6_alias));
        assert!(!same_loopback_listener(&ipv4, &distinct_port));
    }

    #[test]
    fn chat_tool_mutations_are_exact_revision_bound_contracts() {
        let access: ChatAccessMutation = serde_json::from_value(json!({
            "project":"example",
            "mode":"ask",
            "expected_revision":4
        }))
        .unwrap();
        assert_eq!(access.project, "example");
        assert_eq!(access.mode, "ask");
        assert_eq!(access.expected_revision, 4);
        assert!(serde_json::from_value::<ChatAccessMutation>(json!({
            "project":"example",
            "mode":"ask",
            "expected_revision":4,
            "approved":true
        }))
        .is_err());

        let approval: ChatApprovalMutation = serde_json::from_value(json!({
            "project":"example",
            "request_id":"8d919ad1-449f-4089-a6ef-2c6ea4806f1e",
            "approval_id":"44642d68-f970-4480-b31b-49c52826a50d",
            "access_revision":7,
            "decision":"deny"
        }))
        .unwrap();
        assert_eq!(approval.decision, "deny");
        assert_eq!(approval.access_revision, 7);
        assert!(serde_json::from_value::<ChatApprovalMutation>(json!({
            "project":"example",
            "request_id":"8d919ad1-449f-4089-a6ef-2c6ea4806f1e",
            "approval_id":"44642d68-f970-4480-b31b-49c52826a50d",
            "access_revision":7,
            "decision":"approve",
            "remember":true
        }))
        .is_err());
    }

    #[test]
    fn tool_session_changes_become_exact_review_edits_and_reject_deletions() {
        let changes = vec![ToolProjectMutation {
            revision: 3,
            request_id: "8d919ad1-449f-4089-a6ef-2c6ea4806f1e".into(),
            feature_id: Some("feature-1".into()),
            edits: vec![
                developer_tools::ToolMutationEdit {
                    path: "App.py".into(),
                    before_sha256: Some(hash(b"VALUE = 1\n")),
                    after: Some("VALUE = 2\n".into()),
                },
                developer_tools::ToolMutationEdit {
                    path: "tests/test_app.py".into(),
                    before_sha256: None,
                    after: Some("assert True\n".into()),
                },
            ],
            unreviewable_paths: Vec::new(),
        }];
        let edits = tool_mutation_edits(&changes, Some("feature-1")).unwrap();
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[0].path, "App.py");
        assert_eq!(edits[0].before, Some(hash(b"VALUE = 1\n")));
        assert_eq!(edits[0].content, "VALUE = 2\n");
        assert_eq!(edits[1].path, "tests/test_app.py");
        assert_eq!(edits[1].before, None);

        let mut deleted = changes.clone();
        deleted[0].edits[0].after = None;
        assert!(tool_mutation_edits(&deleted, Some("feature-1"))
            .err()
            .unwrap()
            .to_string()
            .contains("deletion cannot enter bounded review"));
        assert!(tool_mutation_edits(&changes, None)
            .err()
            .unwrap()
            .to_string()
            .contains("attribution changed"));
        let mut environment = changes.clone();
        environment[0].unreviewable_paths = vec![".venv".into(), "target".into()];
        assert_eq!(
            tool_mutation_edits(&environment, Some("feature-1"))
                .unwrap()
                .len(),
            2
        );
        environment[0]
            .unreviewable_paths
            .push("build/app.bin".into());
        assert!(tool_mutation_edits(&environment, Some("feature-1"))
            .err()
            .unwrap()
            .to_string()
            .contains("cannot enter bounded review"));

        let protected = repair_forbidden_tool_paths(&["qa/acceptance.py".into()]);
        assert!(protected.contains(&"tests/**".into()));
        assert!(protected.contains(&"qa/acceptance.py".into()));
        assert!(protected.contains(&"qa/acceptance.py/**".into()));
    }

    #[test]
    fn one_project_tool_mutation_invalidates_every_prior_review_evidence() {
        let mut first = feature_with_status("succeeded");
        first.review_status = "approved".into();
        let mut second = first.clone();
        second.id = "34a65755-5607-4b1e-a425-7a3763e85950".into();
        let owned = vec![Edit {
            path: "sidechat.py".into(),
            before: None,
            content: "VALUE = 2\n".into(),
        }];
        reconcile_feature_tool_mutation(&mut first, 9, &Ok(owned)).unwrap();
        reconcile_feature_tool_mutation(&mut second, 9, &Ok(Vec::new())).unwrap();
        for feature in [&first, &second] {
            assert_eq!(feature.status, "paused");
            assert_eq!(feature.review_status, "interrupted");
            assert_eq!(feature.checkpoint, "review_tool_workspace_changed");
            assert_eq!(feature.tool_workspace_revision, 9);
        }
        assert!(first
            .edits
            .as_ref()
            .unwrap()
            .iter()
            .any(|edit| edit.path == "sidechat.py"));
        assert!(!second
            .edits
            .as_ref()
            .unwrap()
            .iter()
            .any(|edit| edit.path == "sidechat.py"));
    }

    #[test]
    fn never_started_missing_project_does_not_require_a_tool_snapshot() {
        let mut feature = feature_with_status("queued");
        feature.edits = None;
        feature.review_attempts = 0;
        feature.tool_workspace_revision = 0;
        assert!(never_started_project_has_no_tool_ledger(&feature));

        feature.tool_workspace_revision = 1;
        assert!(!never_started_project_has_no_tool_ledger(&feature));
        feature.tool_workspace_revision = 0;
        feature.review_attempts = 1;
        assert!(!never_started_project_has_no_tool_ledger(&feature));
    }

    #[test]
    fn validation_uses_only_an_ordinary_project_virtual_environment() {
        let directory = tempfile::tempdir().unwrap();
        let project = fs::canonicalize(directory.path()).unwrap();
        #[cfg(windows)]
        let bin = project.join(".venv/Scripts");
        #[cfg(not(windows))]
        let bin = project.join(".venv/bin");
        fs::create_dir_all(&bin).unwrap();
        #[cfg(windows)]
        fs::write(bin.join("python.exe"), b"fixture").unwrap();
        #[cfg(not(windows))]
        fs::write(bin.join("python"), b"fixture").unwrap();
        let mut environment = developer_process::ValidationEnvironment::capture().unwrap();
        configure_project_environment(&mut environment, &project).unwrap();
        let virtual_environment = environment
            .get(std::ffi::OsStr::new("VIRTUAL_ENV"))
            .unwrap();
        assert_eq!(virtual_environment, project.join(".venv"));
        let configured_path = environment.get(std::ffi::OsStr::new("PATH")).unwrap();
        assert_eq!(std::env::split_paths(configured_path).next(), Some(bin));

        #[cfg(unix)]
        {
            let escaped = tempfile::tempdir().unwrap();
            let linked_project = tempfile::tempdir().unwrap();
            std::os::unix::fs::symlink(escaped.path(), linked_project.path().join(".venv"))
                .unwrap();
            let linked_project = fs::canonicalize(linked_project.path()).unwrap();
            assert!(configure_project_environment(
                &mut developer_process::ValidationEnvironment::capture().unwrap(),
                &linked_project,
            )
            .unwrap_err()
            .to_string()
            .contains("ordinary directory"));
        }
    }

    #[test]
    fn dependency_preparation_requires_a_usable_project_virtual_environment() {
        let directory = tempfile::tempdir().unwrap();
        let project = fs::canonicalize(directory.path()).unwrap();
        assert!(require_project_virtual_environment(&project)
            .unwrap_err()
            .to_string()
            .contains("required project-local .venv"));

        #[cfg(unix)]
        {
            let bin = project.join(".venv/bin");
            fs::create_dir_all(&bin).unwrap();
            let system_interpreter = project.join("python-fixture");
            fs::write(&system_interpreter, b"fixture").unwrap();
            std::os::unix::fs::symlink(&system_interpreter, bin.join("python")).unwrap();
            require_project_virtual_environment(&project).unwrap();
        }
    }

    #[test]
    fn dependency_environment_detection_and_commands_are_generic_and_project_local() {
        let mut feature = feature_with_status("failed");
        feature.repair_history.push(RepairAttemptEvidence {
            attempt: 1,
            prior_checkpoint: "validation_failed".into(),
            prior_message: "ImportError: cannot import name sample from another_package".into(),
            prior_edits: Vec::new(),
        });
        assert!(repair_needs_environment_preparation(&feature));
        let prompt = tool_environment_prompt(&feature).unwrap();
        assert!(prompt.contains(".venv\\Scripts\\python.exe -m pip"));
        assert!(prompt.contains("never use bare `pip`"));
        assert!(is_dependency_manifest("config/requirements-dev.txt"));
        assert!(is_dependency_manifest("Cargo.lock"));
        assert!(!is_dependency_manifest("converter/gui.py"));
    }

    #[test]
    fn staged_tool_failure_records_only_live_environment_edits() {
        let mut feature = feature_with_status("running");
        feature.edits = None;
        let environment_edit = Edit {
            path: "requirements.txt".into(),
            before: Some(hash(b"old\n")),
            content: "new\n".into(),
        };
        quarantine_tool_candidate(
            &mut feature,
            12,
            std::slice::from_ref(&environment_edit),
            true,
            "tool process stopped",
        )
        .unwrap();
        assert_eq!(feature.tool_workspace_revision, 12);
        assert_eq!(feature.checkpoint, "staged_tool_candidate_quarantined");
        assert_eq!(feature.review_status, "interrupted");
        assert_eq!(feature.edits.as_ref().unwrap().len(), 1);
        assert_eq!(feature.edits.as_ref().unwrap()[0].path, "requirements.txt");
        assert!(feature.message.contains("was not applied"));
    }

    #[test]
    fn staged_repair_applies_only_candidate_bytes_but_reviews_environment_too() {
        let environment = Edit {
            path: "requirements.txt".into(),
            before: Some(hash(b"old\n")),
            content: "new\n".into(),
        };
        let candidate = Edit {
            path: "converter/core.py".into(),
            before: Some(hash(b"VALUE = 1\n")),
            content: "VALUE = 2\n".into(),
        };
        let (review, application) = tool_review_and_application_edits(
            std::slice::from_ref(&environment),
            std::slice::from_ref(&candidate),
            true,
        )
        .unwrap();
        assert_eq!(review.len(), 2);
        assert_eq!(application.len(), 1);
        assert_eq!(application[0].path, candidate.path);
        assert!(!application.iter().any(|edit| edit.path == environment.path));
    }

    #[test]
    fn tool_assisted_repairs_use_a_disposable_project_copy() {
        let workspace = tempfile::tempdir().unwrap();
        let workspace_root = fs::canonicalize(workspace.path()).unwrap();
        let source = workspace_root.join("project");
        fs::create_dir_all(source.join("tests")).unwrap();
        fs::create_dir_all(source.join(".venv/bin")).unwrap();
        fs::write(source.join("app.py"), b"VALUE = 1\n").unwrap();
        fs::write(source.join("tests/test_app.py"), b"assert VALUE == 1\n").unwrap();
        fs::write(source.join(".venv/bin/python"), b"environment\n").unwrap();
        #[cfg(unix)]
        {
            let outside = workspace_root.join("outside");
            fs::create_dir(&outside).unwrap();
            fs::write(outside.join("owner.txt"), b"outside\n").unwrap();
            std::os::unix::fs::symlink(&outside, source.join("external-link")).unwrap();
        }
        let source = fs::canonicalize(source).unwrap();
        let stage_path;
        {
            let stage = RepairStage::create(&workspace_root, &source).unwrap();
            stage_path = stage.project_path().to_path_buf();
            assert_eq!(fs::read(stage_path.join("app.py")).unwrap(), b"VALUE = 1\n");
            assert_eq!(
                fs::read(stage_path.join("tests/test_app.py")).unwrap(),
                b"assert VALUE == 1\n"
            );
            assert!(!stage_path.join(".venv").exists());
            assert!(!stage_path.join("external-link").exists());
            fs::write(stage_path.join("tests/test_app.py"), b"weakened\n").unwrap();
            assert_eq!(
                fs::read(source.join("tests/test_app.py")).unwrap(),
                b"assert VALUE == 1\n"
            );
        }
        assert!(!stage_path.exists());
    }

    #[test]
    fn repair_attempts_are_reserved_once_and_preserve_bounded_failure_evidence() {
        let mut feature = feature_with_status("failed");
        for expected_attempt in 1..=REPAIR_LIMIT {
            feature.message = format!("validation failure {expected_attempt}");
            reserve_repair_attempt(&mut feature).unwrap();
            assert_eq!(feature.repair_attempts, expected_attempt);
            assert!(feature.repair_pending);
            assert_eq!(feature.status, "queued");
            assert_eq!(
                feature.checkpoint,
                format!("repair_{expected_attempt}_reserved")
            );
            let evidence = feature.repair_history.last().unwrap();
            assert_eq!(evidence.attempt, expected_attempt);
            assert_eq!(
                evidence.prior_message,
                format!("validation failure {expected_attempt}")
            );
            feature.status = "failed".into();
            feature.repair_pending = false;
        }
        assert!(reserve_repair_attempt(&mut feature).is_err());
        assert_eq!(feature.repair_attempts, REPAIR_LIMIT);
        assert_eq!(feature.repair_history.len(), REPAIR_LIMIT as usize);
    }

    #[test]
    fn escalation_proposal_digest_binds_diagnosis_summary_files_and_protection() {
        let proposal = feature_with_escalation("running")
            .escalation_proposal
            .unwrap();
        let digest = repair_escalation_proposal_sha256(&proposal).unwrap();
        assert_eq!(
            digest, "ebb43dca80d6a12faf8bb784bf18da5f2ad8c8e380acb0e87cd173ea75e61618",
            "schema-v10 manual approval receipts must remain byte-verifiable"
        );

        let mut lifecycle_only = proposal.clone();
        lifecycle_only.status = "approved".into();
        lifecycle_only.binding_revision += 1;
        lifecycle_only.applied_paths.clear();
        lifecycle_only.apply_request_id = None;
        assert_eq!(
            repair_escalation_proposal_sha256(&lifecycle_only).unwrap(),
            digest
        );

        for mutate in ["diagnosis", "summary", "file", "protected"] {
            let mut changed = proposal.clone();
            match mutate {
                "diagnosis" => changed.diagnosis.push_str(" changed"),
                "summary" => changed.summary.push_str(" changed"),
                "file" => changed.files[0].after.push_str("# changed\n"),
                "protected" => changed.files[0].protected = false,
                _ => unreachable!(),
            }
            assert_ne!(
                repair_escalation_proposal_sha256(&changed).unwrap(),
                digest,
                "{mutate} must be part of owner approval"
            );
        }
    }

    #[test]
    fn escalated_validation_or_review_failure_is_terminal_without_legacy_retry() {
        let mut feature = feature_with_escalation("running");
        let lifetime_repairs = feature.repair_attempts;
        finish_escalation_application(&mut feature, 12, "failed", "validation still fails")
            .unwrap();
        assert!(!feature.escalation_pending);
        assert_eq!(feature.repair_attempts, lifetime_repairs);
        assert_eq!(
            feature.escalation_proposal.as_ref().unwrap().status,
            "failed"
        );
        assert_eq!(feature.escalation_history.last().unwrap().outcome, "failed");
        let approved_hash = feature
            .escalation_history
            .iter()
            .find(|evidence| evidence.proposal_id == "3680c592-7d0b-4662-95a8-1ec303d08219")
            .and_then(|evidence| evidence.proposal_sha256.clone());
        assert_eq!(
            feature.escalation_history.last().unwrap().proposal_sha256,
            approved_hash
        );
    }

    #[test]
    fn owner_approved_manual_escalation_applies_without_entering_automatic_timing() {
        let project = tempfile::tempdir().unwrap();
        fs::create_dir(project.path().join("tests")).unwrap();
        fs::write(
            project.path().join("tests/test_gui.py"),
            "assert widget == 'old'\n",
        )
        .unwrap();
        let project_root = fs::canonicalize(project.path()).unwrap();
        let mut feature = feature_with_escalation("running");
        feature.checkpoint = "escalation_1_approved".into();
        feature.auto_repair_lifecycle = "inactive".into();
        feature.auto_repair_step_started_at_ms = None;
        let proposal = feature.escalation_proposal.as_mut().unwrap();
        proposal.status = "approved".into();
        proposal.applied_paths.clear();
        assert_eq!(proposal.source, manual_escalation_source());
        let edit = Edit {
            path: proposal.files[0].path.clone(),
            content: proposal.files[0].after.clone(),
            before: proposal.files[0]
                .before
                .as_ref()
                .map(|before| hash(before.as_bytes())),
        };

        apply_edit(&project_root, &edit).unwrap();
        record_escalation_file_application(&mut feature, &edit, 12).unwrap();

        assert_eq!(
            fs::read_to_string(project.path().join("tests/test_gui.py")).unwrap(),
            "assert widget == 'new'\n"
        );
        assert_eq!(feature.checkpoint, "escalation_1_applying");
        assert_eq!(feature.auto_repair_lifecycle, "inactive");
        assert_eq!(feature.auto_repair_step_started_at_ms, None);
        let proposal = feature.escalation_proposal.as_ref().unwrap();
        assert_eq!(proposal.status, "applying");
        assert_eq!(proposal.applied_paths, ["tests/test_gui.py"]);
    }

    #[test]
    fn interrupted_escalation_apply_is_quarantined_and_cannot_resume_saved_edits() {
        let mut feature = feature_with_escalation("running");
        let project = tempfile::tempdir().unwrap();
        fs::create_dir(project.path().join("tests")).unwrap();
        fs::write(project.path().join("result.txt"), "saved").unwrap();
        fs::write(
            project.path().join("tests/test_gui.py"),
            "assert widget == 'new'\n",
        )
        .unwrap();
        let project_root = fs::canonicalize(project.path()).unwrap();
        feature.checkpoint = "escalation_1_applying".into();
        quarantine_escalation_application(
            &mut feature,
            &project_root,
            13,
            "application interrupted",
        )
        .unwrap();
        assert_eq!(feature.status, "failed");
        assert_eq!(feature.checkpoint, "escalation_1_apply_interrupted");
        assert_eq!(
            feature.edits.as_ref().unwrap()[0].path,
            "result.txt",
            "previously recorded generated-file evidence must survive quarantine"
        );
        assert!(!feature.escalation_pending);
        let proposal = feature.escalation_proposal.as_ref().unwrap();
        assert_eq!(proposal.status, "interrupted");
        assert_eq!(proposal.applied_paths, ["tests/test_gui.py"]);
        for repair_attempts in [0, REPAIR_LIMIT - 1] {
            feature.repair_attempts = repair_attempts;
            assert!(escalation_apply_is_quarantined(&feature));
        }
    }

    #[test]
    fn interrupted_escalation_reconciles_exact_writes_without_retaining_unapplied_files() {
        let project = tempfile::tempdir().unwrap();
        fs::create_dir(project.path().join("tests")).unwrap();
        fs::write(project.path().join("prior.txt"), "retained\n").unwrap();
        fs::write(project.path().join("first.txt"), "after first\n").unwrap();
        fs::write(project.path().join("second.txt"), "before second\n").unwrap();
        let project_root = fs::canonicalize(project.path()).unwrap();
        let mut feature = feature_with_escalation("running");
        feature.checkpoint = "escalation_1_applying".into();
        feature.edits = Some(vec![Edit {
            path: "prior.txt".into(),
            before: None,
            content: "retained\n".into(),
        }]);
        let proposal = feature.escalation_proposal.as_mut().unwrap();
        proposal.files = vec![
            RepairEscalationFile {
                path: "first.txt".into(),
                before: Some("before first\n".into()),
                after: "after first\n".into(),
                protected: false,
            },
            RepairEscalationFile {
                path: "second.txt".into(),
                before: Some("before second\n".into()),
                after: "after second\n".into(),
                protected: false,
            },
        ];
        proposal.applied_paths.clear();

        quarantine_escalation_application(
            &mut feature,
            &project_root,
            14,
            "application interrupted",
        )
        .unwrap();
        assert_eq!(
            feature
                .edits
                .as_ref()
                .unwrap()
                .iter()
                .map(|edit| edit.path.as_str())
                .collect::<Vec<_>>(),
            ["prior.txt", "first.txt"]
        );
        assert!(!feature
            .edits
            .as_ref()
            .unwrap()
            .iter()
            .any(|edit| edit.path == "second.txt"));

        fs::write(project.path().join("overlap.txt"), "after overlap\n").unwrap();
        let mut overlap = feature_with_escalation("running");
        overlap.checkpoint = "escalation_1_applying".into();
        overlap.edits = Some(vec![Edit {
            path: "overlap.txt".into(),
            before: None,
            content: "before overlap\n".into(),
        }]);
        let proposal = overlap.escalation_proposal.as_mut().unwrap();
        proposal.files = vec![RepairEscalationFile {
            path: "overlap.txt".into(),
            before: Some("before overlap\n".into()),
            after: "after overlap\n".into(),
            protected: false,
        }];
        proposal.applied_paths.clear();
        quarantine_escalation_application(
            &mut overlap,
            &project_root,
            15,
            "application interrupted",
        )
        .unwrap();
        assert_eq!(overlap.edits.as_ref().unwrap()[0].before, None);
        assert_eq!(
            overlap.edits.as_ref().unwrap()[0].content,
            "after overlap\n"
        );

        let mut inconsistent = feature_with_escalation("running");
        inconsistent.checkpoint = "escalation_1_applying".into();
        inconsistent
            .escalation_proposal
            .as_mut()
            .unwrap()
            .applied_paths = vec!["tests/test_gui.py".into()];
        fs::write(
            project.path().join("tests/test_gui.py"),
            "assert widget == 'old'\n",
        )
        .unwrap();
        let error = quarantine_escalation_application(
            &mut inconsistent,
            &project_root,
            16,
            "application interrupted",
        )
        .unwrap_err();
        assert!(error.to_string().contains("recorded applied bytes changed"));
    }

    #[test]
    fn escalation_write_receipt_commits_before_a_stop_can_cross_the_effect_gate() {
        let (_directory, engine) = control_test_engine();
        let project = fs::canonicalize(engine.root.join("example")).unwrap();
        fs::write(project.join("result.txt"), "saved").unwrap();
        let edit = Edit {
            path: "result.txt".into(),
            content: "repaired".into(),
            before: Some(hash(b"saved")),
        };
        engine
            .change(|state| {
                let mut feature = feature_with_escalation("running");
                feature.checkpoint = "escalation_1_approved".into();
                let proposal = feature.escalation_proposal.as_mut().unwrap();
                proposal.status = "approved".into();
                proposal.applied_paths.clear();
                proposal.files = vec![RepairEscalationFile {
                    path: edit.path.clone(),
                    before: Some("saved".into()),
                    after: edit.content.clone(),
                    protected: false,
                }];
                state.queue[0] = feature;
                Ok(())
            })
            .unwrap();

        let (write_done_tx, write_done_rx) = std::sync::mpsc::channel();
        let (record_tx, record_rx) = std::sync::mpsc::channel();
        let worker_engine = engine.clone();
        let worker_project = project.clone();
        let worker_edit = edit.clone();
        let worker = std::thread::spawn(move || {
            let effect_guard = worker_engine.effect_gate.lock().unwrap();
            apply_edit(&worker_project, &worker_edit).unwrap();
            write_done_tx.send(()).unwrap();
            record_rx.recv().unwrap();
            worker_engine
                .change(|state| {
                    let revision = state.revision + 1;
                    record_escalation_file_application(&mut state.queue[0], &worker_edit, revision)
                })
                .unwrap();
            drop(effect_guard);
        });
        write_done_rx.recv().unwrap();

        let (stop_started_tx, stop_started_rx) = std::sync::mpsc::channel();
        let (stop_done_tx, stop_done_rx) = std::sync::mpsc::channel();
        let stop_engine = engine.clone();
        let stop = std::thread::spawn(move || {
            stop_started_tx.send(()).unwrap();
            let _effect_guard = stop_engine.effect_gate.lock().unwrap();
            stop_engine
                .change(|state| {
                    state.queue[0].status = "paused".into();
                    Ok(())
                })
                .unwrap();
            stop_done_tx.send(()).unwrap();
        });
        stop_started_rx.recv().unwrap();
        assert!(stop_done_rx
            .recv_timeout(Duration::from_millis(25))
            .is_err());
        record_tx.send(()).unwrap();
        worker.join().unwrap();
        stop_done_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        stop.join().unwrap();

        let database = engine.database.lock().unwrap();
        let feature = &database.state.queue[0];
        assert_eq!(feature.status, "paused");
        assert_eq!(
            feature.escalation_proposal.as_ref().unwrap().applied_paths,
            ["result.txt"]
        );
        assert_eq!(
            fs::read_to_string(project.join("result.txt")).unwrap(),
            "repaired"
        );
    }

    #[test]
    fn cancellation_before_first_escalation_write_quarantines_without_proposal_edits() {
        let project = tempfile::tempdir().unwrap();
        fs::create_dir(project.path().join("tests")).unwrap();
        fs::write(
            project.path().join("tests/test_gui.py"),
            "assert widget == 'old'\n",
        )
        .unwrap();
        let project_root = fs::canonicalize(project.path()).unwrap();
        let mut feature = feature_with_escalation("paused");
        feature.checkpoint = "escalation_1_approved".into();
        feature
            .escalation_proposal
            .as_mut()
            .unwrap()
            .applied_paths
            .clear();

        quarantine_escalation_application(
            &mut feature,
            &project_root,
            16,
            "cancelled before application",
        )
        .unwrap();
        assert_eq!(feature.status, "failed");
        assert_eq!(
            feature.escalation_proposal.as_ref().unwrap().status,
            "interrupted"
        );
        assert_eq!(
            feature
                .edits
                .as_ref()
                .unwrap()
                .iter()
                .map(|edit| edit.path.as_str())
                .collect::<Vec<_>>(),
            ["result.txt"],
            "an unapplied proposal must contribute no generated-file evidence"
        );
    }

    #[test]
    fn v7_escalation_recovery_requires_the_exact_v6_exhausted_repair_baseline() {
        let project = tempfile::tempdir().unwrap();
        fs::create_dir(project.path().join("tests")).unwrap();
        let gui = "struct DeveloperView { let title = \"Feature Conveyor\" }\n";
        let corrected_test = "assert widget == 'new'\n";
        fs::write(project.path().join("DeveloperView.swift"), gui).unwrap();
        fs::write(project.path().join("tests/test_gui.py"), corrected_test).unwrap();
        let project_root = fs::canonicalize(project.path()).unwrap();

        let mut prior = feature_with_status("failed");
        prior.checkpoint = "repair_3_applied".into();
        prior.repair_attempts = REPAIR_LIMIT;
        prior.repair_history = (1..=REPAIR_LIMIT)
            .map(|attempt| RepairAttemptEvidence {
                attempt,
                prior_checkpoint: format!("repair_{attempt}_prepared"),
                prior_message: format!("repair {attempt} failed"),
                prior_edits: Vec::new(),
            })
            .collect();
        prior.edits = Some(vec![Edit {
            path: "DeveloperView.swift".into(),
            before: None,
            content: gui.into(),
        }]);

        let mut current = feature_with_escalation("failed");
        current.cumulative_evidence_version = 0;
        current.repair_history = prior.repair_history.clone();
        current.edits = Some(vec![Edit {
            path: "tests/test_gui.py".into(),
            before: Some(hash(b"assert widget == 'old'\n")),
            content: corrected_test.into(),
        }]);
        let proposal = current.escalation_proposal.as_mut().unwrap();
        proposal.feature_checkpoint = prior.checkpoint.clone();
        proposal.status = "failed".into();
        current
            .escalation_history
            .retain(|evidence| evidence.proposal_id == proposal.proposal_id);
        current.review_attempts = 1;
        current.review_status = "rejected".into();
        current.checkpoint = "review_1_rejected".into();
        current.escalation_pending = false;

        recover_v7_cumulative_evidence(&mut current, &prior, &project_root).unwrap();
        assert_eq!(current.cumulative_evidence_version, 1);
        assert_eq!(current.status, "failed");
        assert_eq!(current.checkpoint, "review_binding_changed");
        assert_eq!(current.review_status, "interrupted");
        assert_eq!(
            current
                .edits
                .as_ref()
                .unwrap()
                .iter()
                .map(|edit| edit.path.as_str())
                .collect::<Vec<_>>(),
            ["DeveloperView.swift", "tests/test_gui.py"]
        );

        let mut mismatched = current.clone();
        mismatched.cumulative_evidence_version = 0;
        let mut wrong_prior = prior.clone();
        wrong_prior.checkpoint = "repair_3_reserved".into();
        let error = recover_v7_cumulative_evidence(&mut mismatched, &wrong_prior, &project_root)
            .err()
            .unwrap();
        assert!(error.to_string().contains("exact failed-feature"));

        let mut zero_prior = feature_with_status("failed");
        zero_prior.checkpoint = "validation_failed".into();
        zero_prior.edits = None;
        let mut zero_current = feature_with_escalation("failed");
        zero_current.cumulative_evidence_version = 0;
        zero_current.repair_attempts = 0;
        zero_current.repair_history.clear();
        let zero_proposal_id = zero_current
            .escalation_proposal
            .as_ref()
            .unwrap()
            .proposal_id
            .clone();
        zero_current
            .escalation_history
            .retain(|evidence| evidence.proposal_id == zero_proposal_id);
        zero_current
            .escalation_proposal
            .as_mut()
            .unwrap()
            .feature_checkpoint = zero_prior.checkpoint.clone();
        recover_v7_cumulative_evidence(&mut zero_current, &zero_prior, &project_root).unwrap();
        assert_eq!(zero_current.cumulative_evidence_version, 1);
        assert_eq!(
            zero_current.edits.as_ref().unwrap()[0].path,
            "tests/test_gui.py"
        );

        let mut pending = feature_with_escalation("running");
        pending.cumulative_evidence_version = 0;
        pending.repair_history = prior.repair_history.clone();
        pending.edits = Some(vec![Edit {
            path: "tests/test_gui.py".into(),
            before: Some(hash(b"assert widget == 'old'\n")),
            content: corrected_test.into(),
        }]);
        pending.review_attempts = 1;
        pending.checkpoint = "escalation_1_applying".into();
        let pending_proposal_id = pending
            .escalation_proposal
            .as_ref()
            .unwrap()
            .proposal_id
            .clone();
        pending
            .escalation_history
            .retain(|evidence| evidence.proposal_id == pending_proposal_id);
        pending
            .escalation_proposal
            .as_mut()
            .unwrap()
            .feature_checkpoint = prior.checkpoint.clone();
        recover_v7_cumulative_evidence(&mut pending, &prior, &project_root).unwrap();
        assert_eq!(pending.checkpoint, "escalation_1_applying");
        quarantine_escalation_application(
            &mut pending,
            &project_root,
            17,
            "restart interrupted application",
        )
        .unwrap();
        assert_eq!(pending.checkpoint, "escalation_1_apply_interrupted");
        assert!(!pending.escalation_pending);
    }

    #[test]
    fn repair_protects_conventional_tests_and_validation_entrypoints() {
        for path in [
            "tests/test_gui.py",
            "Tests/FeatureTests.swift",
            "FeatureTests.swift",
            "src/widget_test.rs",
            "web/widget.spec.ts",
            "web/widget.test.js",
        ] {
            assert!(is_protected_repair_input(path, &[]), "{path}");
        }

        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("tests")).unwrap();
        fs::create_dir_all(dir.path().join("scripts")).unwrap();
        fs::create_dir_all(dir.path().join("checks")).unwrap();
        fs::create_dir_all(dir.path().join(".config")).unwrap();
        fs::write(dir.path().join("tests/test_gui.py"), "assert True\n").unwrap();
        fs::write(dir.path().join("scripts/verify.py"), "print('verify')\n").unwrap();
        fs::write(dir.path().join("checks/arbitrary.py"), "EXPECTED = 1\n").unwrap();
        fs::write(dir.path().join(".config/check.py"), "EXPECTED = 2\n").unwrap();
        fs::write(dir.path().join("implementation.py"), "VALUE = 1\n").unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let validation = "python scripts/verify.py -s checks .config/check.py";
        let validation_paths = validation_path_references(&root, validation).unwrap();
        assert!(is_protected_repair_input(
            "scripts/verify.py",
            &validation_paths
        ));
        assert!(is_protected_repair_input(
            "checks/arbitrary.py",
            &validation_paths
        ));
        assert!(is_protected_repair_input(
            ".config/check.py",
            &validation_paths
        ));
        assert!(!is_protected_repair_input(
            "implementation.py",
            &validation_paths
        ));
        let protected = repair_protected_inputs(&root, &validation_paths).unwrap();
        assert_eq!(protected.len(), 4);
        assert!(protected.contains_key("tests/test_gui.py"));
        assert!(protected.contains_key("scripts/verify.py"));
        assert!(protected.contains_key("checks/arbitrary.py"));
        assert!(protected.contains_key(".config/check.py"));
        assert!(!protected.contains_key("implementation.py"));
    }

    #[test]
    fn protected_input_byte_reservation_fails_before_mutating_the_read_budget() {
        let mut bytes_seen = PROTECTED_REPAIR_INPUT_BYTE_LIMIT - 1;
        assert_eq!(
            reserve_protected_input_bytes(&mut bytes_seen, 1).unwrap(),
            1
        );
        assert_eq!(bytes_seen, PROTECTED_REPAIR_INPUT_BYTE_LIMIT);

        let error = reserve_protected_input_bytes(&mut bytes_seen, 1).unwrap_err();
        assert!(error
            .to_string()
            .contains("Protected repair inputs exceed 64 MiB"));
        assert_eq!(bytes_seen, PROTECTED_REPAIR_INPUT_BYTE_LIMIT);

        let mut per_file = 0;
        let error = reserve_protected_input_bytes(
            &mut per_file,
            u64::try_from(PROTECTED_REPAIR_INPUT_BYTE_LIMIT).unwrap() + 1,
        )
        .unwrap_err();
        assert!(error
            .to_string()
            .contains("Protected repair input exceeds 64 MiB"));
        assert_eq!(per_file, 0);

        let mut overflow = usize::MAX;
        let error = reserve_protected_input_bytes(&mut overflow, 1).unwrap_err();
        assert!(error
            .to_string()
            .contains("Protected repair input size overflow"));
        assert_eq!(overflow, usize::MAX);
    }

    #[cfg(windows)]
    #[test]
    fn windows_oversized_protected_input_is_rejected_from_metadata_before_content_admission() {
        let directory = tempfile::tempdir().unwrap();
        fs::create_dir(directory.path().join("tests")).unwrap();
        let path = directory.path().join("tests/oversized.bin");
        let file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .unwrap();
        // Establish only the oversized logical length. The scanner must reject
        // the held metadata before allocating a content buffer or reading it.
        file.set_len(u64::try_from(PROTECTED_REPAIR_INPUT_BYTE_LIMIT).unwrap() + 1)
            .unwrap();
        file.sync_all().unwrap();
        drop(file);

        let root = fs::canonicalize(directory.path()).unwrap();
        let error = repair_protected_inputs(&root, &[]).unwrap_err();
        assert!(error
            .to_string()
            .contains("Protected repair input exceeds 64 MiB"));
    }

    #[test]
    fn prepared_edits_resume_once_and_preserve_owner_changes() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let edit = Edit {
            path: "src/example.txt".into(),
            content: "implemented".into(),
            before: None,
        };
        apply_edit(&root, &edit).unwrap();
        apply_edit(&root, &edit).unwrap();
        fs::write(root.join(&edit.path), "owner edit").unwrap();
        assert!(apply_edit(&root, &edit).is_err());
        assert_eq!(
            fs::read_to_string(root.join(&edit.path)).unwrap(),
            "owner edit"
        );
    }

    #[test]
    fn auto_ai_repair_request_is_strict_and_bounds_the_shared_cap() {
        let request: AutoAiRepairMutation = serde_json::from_value(json!({
            "enabled":true,
            "max_escalations":100,
            "expected_revision":7
        }))
        .unwrap();
        assert!(request.enabled);
        assert_eq!(request.max_escalations, 100);
        assert_eq!(request.expected_revision, 7);
        assert!(serde_json::from_value::<AutoAiRepairMutation>(json!({
            "enabled":true,
            "max_escalations":100,
            "expected_revision":7,
            "approve":true
        }))
        .is_err());
        for invalid in [0, 101, u32::MAX] {
            assert!(validate_auto_repair_max(invalid).is_err(), "{invalid}");
        }
        for valid in [1, 100] {
            validate_auto_repair_max(valid).unwrap();
        }
    }

    #[test]
    fn disabled_maximum_edit_does_not_cancel_an_ordinary_running_feature() {
        let (_directory, engine) = control_test_engine();
        engine
            .change(|state| {
                state.auto_ai_repair_enabled = false;
                state.queue[0].status = "running".into();
                state.queue[0].checkpoint = "prepared".into();
                state.queue[0].auto_repair_lifecycle = "inactive".into();
                Ok(())
            })
            .unwrap();
        engine.running.store(true, Ordering::SeqCst);
        let revision = engine.database.lock().unwrap().state.revision;
        let result = engine.update_auto_ai_repair(revision, false, 12).unwrap();
        assert_eq!(result["auto_ai_repair_max_escalations"], 12);
        assert_eq!(engine.cancellation.load(Ordering::SeqCst), 0);
        assert!(!engine.tool_cancellation.load(Ordering::SeqCst));
        assert_eq!(result["queue"][0]["status"], "running");
        assert_eq!(result["queue"][0]["auto_repair_lifecycle"], "inactive");
        engine.running.store(false, Ordering::SeqCst);
    }

    #[test]
    fn enabled_policy_arms_fresh_execution_before_three_ordinary_repairs() {
        let mut feature = feature_with_status("queued");
        feature.checkpoint = "not_started".into();
        feature.edits = None;
        assert!(arm_auto_repair_at_execution_start(&mut feature, true, 7, 41).unwrap());
        assert_eq!(feature.auto_ai_repair_limit, Some(7));
        assert_eq!(feature.auto_repair_policy_revision, Some(41));
        assert_eq!(feature.auto_repair_lifecycle, "running");
        assert_eq!(feature.auto_repair_epoch, 1);

        for expected_attempt in 1..=REPAIR_LIMIT {
            feature.status = "failed".into();
            feature.checkpoint = format!("repair_{}_validation_failed", expected_attempt - 1);
            feature.repair_pending = false;
            reserve_repair_attempt(&mut feature).unwrap();
            assert_eq!(feature.repair_attempts, expected_attempt);
            assert_eq!(feature.escalation_count, 0);
        }
        feature.status = "failed".into();
        feature.repair_pending = false;
        reserve_escalation_evidence(&mut feature).unwrap();
        assert_eq!(feature.escalation_count, 1);
        assert_eq!(feature.escalation_evidence_reserved, 3);
        assert_eq!(feature.review_evidence_reserved, 1);
    }

    #[test]
    fn enable_time_repair_requires_exact_code_failure_classification() {
        for kind in ["validation_failure", "review_rejection"] {
            let mut feature = feature_with_status("failed");
            feature.last_failure_kind = kind.into();
            assert!(automatic_code_failure_is_eligible(&feature), "{kind}");
        }
        for (kind, checkpoint) in [
            ("operational", "review_1_unavailable"),
            ("", "review_1_unavailable"),
            ("", "applied"),
        ] {
            let mut feature = feature_with_status("failed");
            feature.last_failure_kind = kind.into();
            feature.checkpoint = checkpoint.into();
            assert!(
                !automatic_code_failure_is_eligible(&feature),
                "{kind} {checkpoint}"
            );
        }

        let (_directory, engine) = control_test_engine();
        engine
            .change(|state| {
                let feature = &mut state.queue[0];
                feature.status = "failed".into();
                feature.checkpoint = "review_1_unavailable".into();
                feature.last_failure_kind = "operational".into();
                Ok(())
            })
            .unwrap();
        let revision = engine.snapshot().unwrap()["revision"].as_u64().unwrap();
        let snapshot = engine.update_auto_ai_repair(revision, true, 8).unwrap();
        assert_eq!(snapshot["queue"][0]["repair_attempts"], 0);
        assert_eq!(snapshot["queue"][0]["auto_repair_lifecycle"], "held");
        assert!(!engine.running.load(Ordering::SeqCst));
        assert_eq!(engine.cancellation.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn ordinary_auto_repair_persists_applying_epoch_before_any_write_and_quarantines() {
        let mut feature = feature_with_status("running");
        feature.repair_attempts = 2;
        feature.repair_pending = true;
        feature.auto_repair_epoch = 11;
        set_auto_repair_lifecycle(&mut feature, "running", "automatic ordinary repair").unwrap();

        begin_automatic_ordinary_repair_application(&mut feature, true, 11, 2).unwrap();
        assert_eq!(feature.checkpoint, "repair_2_applying");
        assert!(feature.message.contains("policy epoch 11"));
        assert!(automatic_post_apply_is_ambiguous(&feature));
        assert!(begin_automatic_ordinary_repair_application(&mut feature, true, 12, 2).is_err());

        persist_automatic_cancellation_intent(
            &mut feature,
            true,
            "Stop cancellation intent",
            "Stop quarantined an uncertain ordinary-repair write boundary",
        )
        .unwrap();
        assert_eq!(feature.auto_repair_epoch, 12);
        assert_eq!(feature.auto_repair_lifecycle, "quarantined");

        quarantine_automatic_post_apply(
            &mut feature,
            24,
            "Stop arrived after an ordinary repair write may have occurred",
        )
        .unwrap();
        assert_eq!(feature.status, "failed");
        assert_eq!(feature.auto_repair_lifecycle, "quarantined");
        assert_eq!(feature.checkpoint, "auto_repair_effects_quarantined");
        assert!(feature.edits.is_some());
    }

    #[test]
    fn repair_admission_excludes_secrets_and_rejects_credential_like_outputs() {
        let directory = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(directory.path()).unwrap();
        fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
        fs::write(root.join("credentials.json"), "{}\n").unwrap();
        fs::write(root.join("leaked.txt"), "api_key = supersecretvalue123\n").unwrap();

        let context = project_context(&root).unwrap();
        assert_eq!(context.len(), 1);
        assert_eq!(context[0]["path"], "main.rs");

        for edit in [
            Edit {
                path: "new-credentials.json".into(),
                content: "{}\n".into(),
                before: None,
            },
            Edit {
                path: "src/config.rs".into(),
                content: "api_key = supersecretvalue123\n".into(),
                before: None,
            },
        ] {
            assert!(validate_repair_file_admission(&edit.path, &edit.content).is_err());
            assert!(apply_edit(&root, &edit).is_err());
            assert!(!root.join(&edit.path).exists());
        }
    }

    #[cfg(unix)]
    #[test]
    fn project_context_holds_unix_ancestor_during_concurrent_symlink_replacement() {
        use std::os::unix::fs::symlink;
        use std::sync::mpsc;

        let project_directory = tempfile::tempdir().unwrap();
        let outside_directory = tempfile::tempdir().unwrap();
        let project = fs::canonicalize(project_directory.path()).unwrap();
        let outside = fs::canonicalize(outside_directory.path()).unwrap();
        fs::create_dir(project.join("nested")).unwrap();
        fs::write(project.join("nested/safe.rs"), "fn safe() {}\n").unwrap();
        let secret = "out-of-project-sensitive-prompt-content";
        fs::write(outside.join("exposed.rs"), secret).unwrap();

        let (opened_tx, opened_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let scan_root = project.clone();
        let scan = std::thread::spawn(move || {
            let directory_opened = |relative: &Path| {
                if relative == Path::new("nested") {
                    opened_tx.send(()).unwrap();
                    release_rx
                        .recv_timeout(Duration::from_secs(5))
                        .expect("project-context replacement barrier was not released");
                }
            };
            project_context_with_directory_opened(&scan_root, Some(&directory_opened))
        });

        opened_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("project context did not hold the nested directory");
        fs::rename(project.join("nested"), project.join("held-nested")).unwrap();
        symlink(&outside, project.join("nested")).unwrap();
        release_tx.send(()).unwrap();
        let context = scan.join().unwrap().unwrap();

        fs::remove_file(project.join("nested")).unwrap();
        fs::rename(project.join("held-nested"), project.join("nested")).unwrap();
        assert_eq!(context.len(), 1);
        assert_eq!(context[0]["path"], "nested/safe.rs");
        assert_eq!(context[0]["content"], "fn safe() {}\n");
        assert!(context
            .iter()
            .all(|file| !file["content"].as_str().unwrap().contains(secret)));
    }

    #[cfg(windows)]
    #[test]
    fn project_context_refuses_windows_junction_before_alias_traversal() {
        let project_directory = tempfile::tempdir().unwrap();
        let outside_directory = tempfile::tempdir().unwrap();
        let project = fs::canonicalize(project_directory.path()).unwrap();
        fs::write(project.join("main.rs"), "fn main() {}\n").unwrap();
        fs::write(
            outside_directory.path().join("exposed.rs"),
            "out-of-project-sensitive-prompt-content",
        )
        .unwrap();
        let junction = project_directory.path().join("aliased-directory");
        let creation = std::process::Command::new("cmd.exe")
            .args(["/D", "/C", "mklink", "/J"])
            .arg(&junction)
            .arg(outside_directory.path())
            .output()
            .unwrap();
        assert!(creation.status.success());
        let junction_metadata = fs::symlink_metadata(&junction).unwrap();
        assert!(planning_metadata_is_reparse(&junction_metadata));

        let result = project_context(&project);
        let cleanup = std::process::Command::new("cmd.exe")
            .args(["/D", "/C", "rmdir"])
            .arg(&junction)
            .output()
            .unwrap();
        assert!(cleanup.status.success());
        let error = result.expect_err("junction must fail closed");
        assert!(error
            .to_string()
            .contains("refuses a Windows reparse point"));
    }

    #[cfg(windows)]
    #[test]
    fn project_context_windows_directory_handle_blocks_ancestor_replacement() {
        use std::sync::mpsc;

        let project_directory = tempfile::tempdir().unwrap();
        let project = fs::canonicalize(project_directory.path()).unwrap();
        fs::create_dir(project.join("nested")).unwrap();
        fs::write(project.join("nested/safe.rs"), "fn safe() {}\n").unwrap();
        let (opened_tx, opened_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let scan_root = project.clone();
        let scan = std::thread::spawn(move || {
            let directory_opened = |relative: &Path| {
                if relative == Path::new("nested") {
                    opened_tx.send(()).unwrap();
                    release_rx
                        .recv_timeout(Duration::from_secs(5))
                        .expect("Windows project-context barrier was not released");
                }
            };
            project_context_with_directory_opened(&scan_root, Some(&directory_opened))
        });

        opened_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("project context did not hold the nested directory");
        let replacement = fs::rename(project.join("nested"), project.join("held-nested"));
        release_tx.send(()).unwrap();
        let context = scan.join().unwrap().unwrap();

        assert!(replacement.is_err());
        assert_eq!(context.len(), 1);
        assert_eq!(context[0]["path"], "nested/safe.rs");
    }

    #[test]
    fn post_apply_automatic_cancellation_quarantines_and_terminalizes_review_slot() {
        let mut feature = feature_with_escalation("running");
        feature.auto_ai_repair_limit = Some(4);
        feature.auto_repair_epoch = 3;
        feature.review_evidence_reserved = 1;
        set_auto_repair_lifecycle(&mut feature, "running", "automatic attempt").unwrap();
        quarantine_automatic_post_apply(
            &mut feature,
            19,
            "Cancellation arrived during immutable validation",
        )
        .unwrap();

        assert_eq!(feature.status, "failed");
        assert_eq!(feature.auto_repair_lifecycle, "quarantined");
        assert_eq!(feature.checkpoint, "auto_repair_effects_quarantined");
        assert!(!feature.escalation_pending);
        assert_eq!(feature.review_history.last().unwrap().outcome, "not_run");
        assert!(
            feature
                .escalation_proposal
                .as_ref()
                .unwrap()
                .review_slot_terminal
        );
    }

    #[test]
    fn disabling_during_applied_validation_records_epoch_and_quarantine_atomically() {
        let (_directory, engine) = control_test_engine();
        engine
            .change(|state| {
                state.auto_ai_repair_enabled = true;
                state.auto_ai_repair_max_escalations = 9;
                state.auto_ai_repair_policy_revision = state.revision + 1;
                let feature = &mut state.queue[0];
                feature.status = "running".into();
                feature.checkpoint = "repair_3_applied".into();
                feature.auto_ai_repair_limit = Some(9);
                feature.auto_repair_epoch = 6;
                set_auto_repair_lifecycle(feature, "running", "validating automatic repair")
            })
            .unwrap();
        engine.running.store(true, Ordering::SeqCst);
        let revision = engine.snapshot().unwrap()["revision"].as_u64().unwrap();
        let snapshot = engine.update_auto_ai_repair(revision, false, 9).unwrap();
        engine.running.store(false, Ordering::SeqCst);

        assert_eq!(snapshot["queue"][0]["auto_repair_epoch"], 7);
        assert_eq!(snapshot["queue"][0]["auto_repair_lifecycle"], "quarantined");
        assert_eq!(engine.cancellation.load(Ordering::SeqCst), 1);
        let queued = &snapshot["queue"][0];
        let resume = engine.start(
            queued["id"].as_str(),
            queued["model_target"].as_str(),
            queued["status"].as_str(),
            queued["checkpoint"].as_str(),
        );
        assert!(resume
            .unwrap_err()
            .to_string()
            .contains("cannot Resume from a quarantine state"));
    }

    #[test]
    fn feature_limit_snapshot_is_immutable_and_manual_counts_reduce_automatic_budget() {
        let mut feature = feature_with_status("failed");
        feature.escalation_count = 3;
        assert!(snapshot_feature_auto_repair_limit(&mut feature, 4, 8).unwrap());
        assert_eq!(feature.auto_ai_repair_limit, Some(4));
        assert_eq!(feature.auto_repair_policy_revision, Some(8));
        assert!(!snapshot_feature_auto_repair_limit(&mut feature, 100, 9).unwrap());
        assert_eq!(feature.auto_ai_repair_limit, Some(4));

        reserve_escalation_evidence(&mut feature).unwrap();
        assert_eq!(feature.escalation_count, 4);
        assert_eq!(feature.escalation_evidence_reserved, 3);
        assert_eq!(feature.review_evidence_reserved, 1);
        assert!(reserve_escalation_evidence(&mut feature).is_err());
        assert_eq!(feature.auto_repair_lifecycle, "limit_reached");
        assert!(feature.auto_repair_reason.contains("4 of 4"));
    }

    #[test]
    fn escalation_admission_reserves_exact_300_and_104_evidence_capacity() {
        let mut feature = feature_with_status("failed");
        feature.auto_ai_repair_limit = Some(100);
        feature.escalation_count = 99;
        feature.escalation_evidence_reserved = 297;
        feature.review_evidence_reserved = 99;
        reserve_escalation_evidence(&mut feature).unwrap();
        assert_eq!(feature.escalation_count, 100);
        assert_eq!(feature.escalation_evidence_reserved, 300);
        assert_eq!(feature.review_evidence_reserved, 100);
        assert!(reserve_escalation_evidence(&mut feature).is_err());
        assert_eq!(feature.escalation_count, 100, "no 101st model attempt");
        assert_eq!(feature.auto_repair_lifecycle, "limit_reached");

        let mut no_escalation_capacity = feature_with_status("failed");
        no_escalation_capacity.auto_ai_repair_limit = Some(100);
        no_escalation_capacity.escalation_evidence_reserved = 298;
        assert!(reserve_escalation_evidence(&mut no_escalation_capacity).is_err());
        assert_eq!(no_escalation_capacity.auto_repair_lifecycle, "held");

        let mut no_review_capacity = feature_with_status("failed");
        no_review_capacity.auto_ai_repair_limit = Some(100);
        no_review_capacity.review_evidence_reserved = 104;
        assert!(reserve_escalation_evidence(&mut no_review_capacity).is_err());
        assert_eq!(no_review_capacity.auto_repair_lifecycle, "held");
    }

    #[test]
    fn revision_bound_auto_repair_control_replays_exactly_and_cancels_epoch_first() {
        let (_dir, engine) = control_test_engine();
        let initial_revision = engine.snapshot().unwrap()["revision"].as_u64().unwrap();
        engine
            .change(|state| {
                state.auto_ai_repair_enabled = true;
                state.auto_ai_repair_max_escalations = 12;
                state.auto_ai_repair_policy_revision = state.revision + 1;
                let feature = &mut state.queue[0];
                feature.status = "paused".into();
                feature.auto_ai_repair_limit = Some(12);
                feature.auto_repair_epoch = 9;
                set_auto_repair_lifecycle(feature, "running", "active automatic repair")
            })
            .unwrap();
        let revision = engine.snapshot().unwrap()["revision"].as_u64().unwrap();
        assert!(engine.update_auto_ai_repair(revision, true, 13).is_err());
        assert_eq!(
            engine.snapshot().unwrap()["revision"].as_u64(),
            Some(revision)
        );
        let accepted = engine.update_auto_ai_repair(revision, false, 12).unwrap();
        assert_eq!(accepted["revision"].as_u64(), Some(revision + 1));
        assert_eq!(accepted["auto_ai_repair_enabled"], false);
        assert_eq!(accepted["queue"][0]["auto_repair_epoch"], 10);
        assert_eq!(accepted["queue"][0]["auto_repair_lifecycle"], "inactive");
        assert_eq!(engine.cancellation.load(Ordering::SeqCst), 1);

        let replay = engine.update_auto_ai_repair(revision, false, 12).unwrap();
        assert_eq!(replay["revision"], accepted["revision"]);
        assert!(engine.update_auto_ai_repair(revision, false, 13).is_err());
        assert!(engine.update_auto_ai_repair(revision + 1, true, 0).is_err());
        assert_eq!(
            engine.snapshot().unwrap()["revision"].as_u64(),
            Some(initial_revision + 2)
        );
    }

    #[test]
    fn automatic_reservation_uses_failure_evidence_without_chat_diagnosis() {
        let (_dir, engine) = control_test_engine();
        engine
            .change(|state| {
                state.auto_ai_repair_enabled = true;
                state.auto_ai_repair_max_escalations = 7;
                state.auto_ai_repair_policy_revision = state.revision + 1;
                let feature = &mut state.queue[0];
                feature.repair_attempts = REPAIR_LIMIT;
                feature.auto_ai_repair_limit = Some(7);
                feature.auto_repair_policy_revision = Some(state.revision + 1);
                feature.auto_repair_epoch = 3;
                set_auto_repair_lifecycle(feature, "running", "fixture")
            })
            .unwrap();
        let feature_id = engine.database.lock().unwrap().state.queue[0].id.clone();
        let (_, _, epoch, _) = engine
            .reserve_automatic_escalation(&feature_id)
            .unwrap()
            .unwrap();
        assert_eq!(epoch, 3);
        let database = engine.database.lock().unwrap();
        let feature = &database.state.queue[0];
        let proposal = feature.escalation_proposal.as_ref().unwrap();
        assert_eq!(proposal.source, "automatic_failure");
        assert!(proposal.chat_id.is_none());
        assert!(proposal.chat_request_id.is_empty());
        assert!(proposal.diagnosis.is_empty());
        assert_eq!(proposal.diagnosis_sha256.len(), 64);
        assert_eq!(proposal.limit_snapshot, Some(7));
        assert_eq!(proposal.automatic_epoch, Some(3));
        assert_eq!(feature.escalation_count, 1);
        assert_eq!(feature.escalation_evidence_reserved, 3);
        assert_eq!(feature.review_evidence_reserved, 1);
    }

    #[tokio::test]
    async fn automatic_reservation_builds_from_active_checkpoint_and_retains_failure_binding() {
        let (_dir, engine) = control_test_engine();
        engine
            .change(|state| {
                state.auto_ai_repair_enabled = true;
                state.auto_ai_repair_max_escalations = 7;
                state.auto_ai_repair_policy_revision = state.revision + 1;
                let feature = &mut state.queue[0];
                feature.repair_attempts = REPAIR_LIMIT;
                feature.checkpoint = "repair_3_validation_failed".into();
                feature.message = "exact retained validation failure".into();
                feature.last_failure_kind = "validation_failure".into();
                feature.auto_ai_repair_limit = Some(7);
                feature.auto_repair_policy_revision = Some(state.revision + 1);
                feature.auto_repair_epoch = 3;
                set_auto_repair_lifecycle(feature, "running", "fixture")
            })
            .unwrap();
        let feature_id = engine.database.lock().unwrap().state.queue[0].id.clone();
        let (proposal_id, target, _, failure_summary) = engine
            .reserve_automatic_escalation(&feature_id)
            .unwrap()
            .unwrap();
        {
            let database = engine.database.lock().unwrap();
            let feature = &database.state.queue[0];
            let proposal = feature.escalation_proposal.as_ref().unwrap();
            assert_eq!(proposal.feature_checkpoint, "repair_3_validation_failed");
            assert_eq!(feature.checkpoint, "escalation_1_preparing");
            validate_escalation_build_binding(feature, proposal).unwrap();
        }

        let error = match engine
            .build_escalation_proposal(EscalationProposalInput {
                feature_id: &feature_id,
                proposal_id: &proposal_id,
                target: &target,
                handoff: None,
                automatic_failure_summary: Some(&failure_summary),
                cancellation: &AtomicU8::new(1),
                inference_lease: None,
            })
            .await
        {
            Err(error) => error,
            Ok(_) => panic!("cancelled fixture unexpectedly completed proposal inference"),
        };
        assert!(!error.to_string().contains("checkpoint changed"));
        assert!(!error.to_string().contains("feature binding changed"));

        engine
            .change(|state| {
                state.queue[0].checkpoint = "repair_3_validation_failed".into();
                Ok(())
            })
            .unwrap();
        let error = match engine
            .build_escalation_proposal(EscalationProposalInput {
                feature_id: &feature_id,
                proposal_id: &proposal_id,
                target: &target,
                handoff: None,
                automatic_failure_summary: Some(&failure_summary),
                cancellation: &AtomicU8::new(1),
                inference_lease: None,
            })
            .await
        {
            Err(error) => error,
            Ok(_) => panic!("drifted checkpoint unexpectedly passed build admission"),
        };
        assert!(error.to_string().contains("active checkpoint changed"));
    }

    #[test]
    fn automatic_no_op_consumes_attempt_and_records_all_unused_terminal_slots() {
        let (_dir, engine) = control_test_engine();
        engine
            .change(|state| {
                state.auto_ai_repair_enabled = true;
                state.auto_ai_repair_max_escalations = 2;
                state.auto_ai_repair_policy_revision = state.revision + 1;
                let feature = &mut state.queue[0];
                feature.repair_attempts = REPAIR_LIMIT;
                feature.auto_ai_repair_limit = Some(2);
                feature.auto_repair_policy_revision = Some(state.revision + 1);
                feature.auto_repair_epoch = 1;
                set_auto_repair_lifecycle(feature, "running", "fixture")
            })
            .unwrap();
        let feature_id = engine.database.lock().unwrap().state.queue[0].id.clone();
        let (proposal_id, _, epoch, _) = engine
            .reserve_automatic_escalation(&feature_id)
            .unwrap()
            .unwrap();
        engine
            .finish_escalation_proposal(
                &feature_id,
                &proposal_id,
                Ok((
                    "No changes were needed".into(),
                    Vec::new(),
                    Default::default(),
                )),
                &AtomicU8::new(0),
            )
            .unwrap();
        assert!(engine
            .finish_automatic_without_application(&feature_id, &proposal_id, epoch)
            .unwrap());
        let database = engine.database.lock().unwrap();
        let feature = &database.state.queue[0];
        assert_eq!(feature.escalation_count, 1);
        assert_eq!(
            feature
                .escalation_history
                .iter()
                .filter(|evidence| evidence.proposal_id == proposal_id)
                .map(|evidence| evidence.outcome.as_str())
                .collect::<Vec<_>>(),
            ["no_op", "authorization_not_run", "application_not_run"]
        );
        assert_eq!(feature.review_history.last().unwrap().outcome, "not_run");
        assert_eq!(feature.auto_repair_lifecycle, "running");
    }

    #[tokio::test]
    async fn automatic_unavailable_completion_is_durably_held_across_reload_until_resume() {
        let (_directory, engine) = control_test_engine();
        engine
            .change(|state| {
                state.auto_ai_repair_enabled = true;
                state.auto_ai_repair_max_escalations = 3;
                state.auto_ai_repair_policy_revision = state.revision + 1;
                let feature = &mut state.queue[0];
                feature.repair_attempts = REPAIR_LIMIT;
                feature.last_failure_kind = "validation_failure".into();
                feature.last_code_failure_summary = "fixture validation failed".into();
                feature.auto_ai_repair_limit = Some(3);
                feature.auto_repair_policy_revision = Some(state.revision + 1);
                feature.auto_repair_epoch = 4;
                set_auto_repair_lifecycle(feature, "running", "fixture")
            })
            .unwrap();
        let feature_id = engine.database.lock().unwrap().state.queue[0].id.clone();
        let (proposal_id, _, _, _) = engine
            .reserve_automatic_escalation(&feature_id)
            .unwrap()
            .unwrap();
        engine
            .finish_escalation_proposal(
                &feature_id,
                &proposal_id,
                Err(anyhow!("fixture provider unavailable")),
                &AtomicU8::new(0),
            )
            .unwrap();

        let persisted = {
            let database = engine.database.lock().unwrap();
            let encoded: String = database
                .connection
                .query_row("SELECT state FROM developer_state WHERE id=1", [], |row| {
                    row.get(0)
                })
                .unwrap();
            serde_json::from_str::<Snapshot>(&encoded).unwrap()
        };
        let held = &persisted.queue[0];
        assert_eq!(held.status, "failed");
        assert_eq!(held.auto_repair_lifecycle, "held");
        assert_eq!(held.checkpoint, "escalation_1_unavailable");
        assert!(held.auto_repair_reason.contains("explicitly Resume"));
        assert_eq!(
            held.escalation_history
                .iter()
                .filter(|evidence| evidence.proposal_id == proposal_id)
                .map(|evidence| evidence.outcome.as_str())
                .collect::<Vec<_>>(),
            [
                "unavailable",
                "authorization_not_run",
                "application_not_run"
            ]
        );
        assert_eq!(
            held.review_history
                .iter()
                .filter(|evidence| evidence.outcome == "not_run")
                .count(),
            1
        );

        let (_reload_directory, reloaded) = control_test_engine();
        reloaded
            .change(|state| {
                *state = persisted.clone();
                Ok(())
            })
            .unwrap();
        reloaded.resume_automatic_after_restart().unwrap();
        let after_restart = reloaded.snapshot().unwrap();
        assert!(!after_restart["running"].as_bool().unwrap());
        assert_eq!(after_restart["queue"][0]["auto_repair_lifecycle"], "held");
        assert_eq!(after_restart["queue"][0]["escalation_count"], 1);
        assert_eq!(
            reloaded.database.lock().unwrap().state.queue[0]
                .escalation_proposal
                .as_ref()
                .unwrap()
                .proposal_id,
            proposal_id,
        );

        reloaded
            .start(
                Some(after_restart["queue"][0]["id"].as_str().unwrap()),
                Some(after_restart["queue"][0]["model_target"].as_str().unwrap()),
                Some(after_restart["queue"][0]["status"].as_str().unwrap()),
                Some(after_restart["queue"][0]["checkpoint"].as_str().unwrap()),
            )
            .unwrap();
        reloaded.cancellation.store(1, Ordering::SeqCst);
        assert!(
            reloaded.snapshot().unwrap()["queue"][0]["auto_repair_epoch"]
                .as_u64()
                .unwrap()
                > held.auto_repair_epoch
        );
    }

    #[test]
    fn interrupted_preparation_fills_each_reserved_stage_for_manual_and_automatic_attempts() {
        for automatic in [false, true] {
            let mut feature = feature_with_status("failed");
            feature.repair_attempts = REPAIR_LIMIT;
            feature.escalation_count = 1;
            feature.escalation_evidence_reserved = 3;
            feature.review_evidence_reserved = 1;
            feature.auto_ai_repair_limit = Some(3);
            feature.auto_repair_epoch = 7;
            feature.auto_repair_policy_revision = Some(11);
            if automatic {
                set_auto_repair_lifecycle(&mut feature, "running", "fixture").unwrap();
            }
            let proposal_id = if automatic {
                "79ce2a2e-4bb8-469e-8dc1-b6d7cf57098f"
            } else {
                "3680c592-7d0b-4662-95a8-1ec303d08219"
            };
            feature.escalation_proposal = Some(RepairEscalationProposal {
                proposal_id: proposal_id.into(),
                attempt: 1,
                feature_id: feature.id.clone(),
                feature_checkpoint: "repair_3_validation_failed".into(),
                binding_revision: 8,
                model_target: "mac".into(),
                model: "fixture".into(),
                chat_id: (!automatic).then(|| "5fcc2572-e2c7-436c-8bd9-3ed7c380e7f3".into()),
                chat_request_id: if automatic {
                    String::new()
                } else {
                    "b35972e8-b6a8-4cb9-96fa-cc7a68d2e8b2".into()
                },
                chat_model_target: if automatic {
                    String::new()
                } else {
                    "windows".into()
                },
                chat_model: if automatic {
                    String::new()
                } else {
                    "fixture-chat".into()
                },
                diagnosis: String::new(),
                diagnosis_sha256: "1".repeat(64),
                status: "preparing".into(),
                summary: "preparing".into(),
                error: None,
                files: Vec::new(),
                protected_inputs: Default::default(),
                applied_paths: Vec::new(),
                apply_request_id: None,
                source: if automatic {
                    "automatic_failure".into()
                } else {
                    manual_escalation_source()
                },
                automatic_epoch: automatic.then_some(7),
                policy_revision: automatic.then_some(11),
                limit_snapshot: Some(3),
                project_state_sha256: automatic.then(|| "2".repeat(64)),
                review_slot_terminal: false,
            });

            assert!(recover_interrupted_escalation_preparation(&mut feature, 19).unwrap());
            assert!(!recover_interrupted_escalation_preparation(&mut feature, 20).unwrap());
            assert_eq!(
                feature
                    .escalation_history
                    .iter()
                    .filter(|evidence| evidence.proposal_id == proposal_id)
                    .map(|evidence| evidence.outcome.as_str())
                    .collect::<Vec<_>>(),
                [
                    "proposal_interrupted",
                    "authorization_not_run",
                    "application_not_run"
                ]
            );
            assert!(feature
                .escalation_history
                .iter()
                .filter(|evidence| evidence.proposal_id == proposal_id)
                .all(|evidence| evidence.candidate_sha256.is_none()));
            assert_eq!(feature.review_history.len(), 1);
            assert_eq!(feature.review_history[0].outcome, "not_run");
            assert!(feature.review_history[0]
                .summary
                .contains("proposal generation was interrupted"));
            assert!(
                feature
                    .escalation_proposal
                    .as_ref()
                    .unwrap()
                    .review_slot_terminal
            );
            assert_eq!(
                feature.auto_repair_lifecycle,
                if automatic { "running" } else { "inactive" }
            );
        }
    }

    #[test]
    fn ready_to_authorize_cancel_terminalizes_once_and_reenable_reserves_fresh_work() {
        let (_directory, engine) = control_test_engine();
        fs::write(engine.root.join("example/result.txt"), "saved").unwrap();
        engine
            .change(|state| {
                state.auto_ai_repair_enabled = true;
                state.auto_ai_repair_max_escalations = 3;
                state.auto_ai_repair_policy_revision = state.revision + 1;
                let feature = &mut state.queue[0];
                feature.repair_attempts = REPAIR_LIMIT;
                feature.last_failure_kind = "validation_failure".into();
                feature.auto_ai_repair_limit = Some(3);
                feature.auto_repair_policy_revision = Some(state.revision + 1);
                feature.auto_repair_epoch = 5;
                set_auto_repair_lifecycle(feature, "running", "fixture")
            })
            .unwrap();
        let feature_id = engine.database.lock().unwrap().state.queue[0].id.clone();
        let (proposal_id, _, _, _) = engine
            .reserve_automatic_escalation(&feature_id)
            .unwrap()
            .unwrap();
        engine
            .finish_escalation_proposal(
                &feature_id,
                &proposal_id,
                Ok((
                    "repair source".into(),
                    vec![RepairEscalationFile {
                        path: "result.txt".into(),
                        before: Some("saved".into()),
                        after: "repaired".into(),
                        protected: false,
                    }],
                    Default::default(),
                )),
                &AtomicU8::new(0),
            )
            .unwrap();
        let revision = engine.snapshot().unwrap()["revision"].as_u64().unwrap();
        let cancelled = engine.update_auto_ai_repair(revision, false, 3).unwrap();
        assert_eq!(cancelled["queue"][0]["auto_repair_epoch"], 6);
        assert_eq!(cancelled["queue"][0]["auto_repair_lifecycle"], "inactive");
        let replay = engine.update_auto_ai_repair(revision, false, 3).unwrap();
        assert_eq!(replay["revision"], cancelled["revision"]);

        {
            let database = engine.database.lock().unwrap();
            let feature = &database.state.queue[0];
            assert_eq!(
                feature
                    .escalation_history
                    .iter()
                    .filter(|evidence| evidence.proposal_id == proposal_id)
                    .map(|evidence| evidence.outcome.as_str())
                    .collect::<Vec<_>>(),
                ["ready", "authorization_not_run", "application_not_run"]
            );
            assert_eq!(feature.review_history.len(), 1);
            assert_eq!(feature.review_history[0].outcome, "not_run");
            assert_eq!(
                feature.escalation_proposal.as_ref().unwrap().status,
                "cancelled"
            );
        }

        engine
            .change(|state| {
                state.auto_ai_repair_enabled = true;
                state.auto_ai_repair_policy_revision = state.revision + 1;
                let max = state.auto_ai_repair_max_escalations;
                enable_auto_repair_for_failed_feature(
                    &mut state.queue[0],
                    true,
                    max,
                    state.revision + 1,
                )?;
                Ok(())
            })
            .unwrap();
        engine.cancellation.store(0, Ordering::SeqCst);
        let (fresh_proposal_id, _, _, _) = engine
            .reserve_automatic_escalation(&feature_id)
            .unwrap()
            .unwrap();
        assert_ne!(fresh_proposal_id, proposal_id);
        assert_eq!(
            engine.database.lock().unwrap().state.queue[0].escalation_count,
            2
        );
    }

    #[tokio::test]
    async fn disabling_during_preparing_pauses_and_late_completion_cannot_restore_work() {
        let (_directory, engine) = control_test_engine();
        engine
            .change(|state| {
                state.auto_ai_repair_enabled = true;
                state.auto_ai_repair_max_escalations = 3;
                state.auto_ai_repair_policy_revision = state.revision + 1;
                let feature = &mut state.queue[0];
                feature.repair_attempts = REPAIR_LIMIT;
                feature.checkpoint = "repair_3_validation_failed".into();
                feature.last_failure_kind = "validation_failure".into();
                feature.auto_ai_repair_limit = Some(3);
                feature.auto_repair_policy_revision = Some(state.revision + 1);
                feature.auto_repair_epoch = 5;
                set_auto_repair_lifecycle(feature, "running", "fixture")
            })
            .unwrap();
        let feature_id = engine.database.lock().unwrap().state.queue[0].id.clone();
        let (proposal_id, _, _, _) = engine
            .reserve_automatic_escalation(&feature_id)
            .unwrap()
            .unwrap();
        engine.running.store(true, Ordering::SeqCst);
        let revision = engine.snapshot().unwrap()["revision"].as_u64().unwrap();
        let disabled = engine.update_auto_ai_repair(revision, false, 3).unwrap();
        engine.running.store(false, Ordering::SeqCst);

        let feature = &disabled["queue"][0];
        assert_eq!(feature["status"], "paused");
        assert_eq!(feature["auto_repair_lifecycle"], "inactive");
        assert_eq!(
            feature["checkpoint"],
            "escalation_1_cancelled_before_authorization"
        );
        assert!(feature["message"]
            .as_str()
            .unwrap()
            .contains("Resume revalidates"));

        engine
            .finish_escalation_proposal(
                &feature_id,
                &proposal_id,
                Ok((
                    "late untrusted proposal".into(),
                    vec![RepairEscalationFile {
                        path: "late.txt".into(),
                        before: None,
                        after: "must not survive cancellation".into(),
                        protected: false,
                    }],
                    Default::default(),
                )),
                &AtomicU8::new(0),
            )
            .unwrap();
        let snapshot = engine.snapshot().unwrap();
        let feature = &snapshot["queue"][0];
        assert_eq!(feature["status"], "paused");
        assert_eq!(
            feature["checkpoint"],
            "escalation_1_cancelled_before_authorization"
        );
        assert_eq!(feature["escalation_count"], 1);
        {
            let database = engine.database.lock().unwrap();
            let feature = &database.state.queue[0];
            assert_eq!(
                feature
                    .escalation_history
                    .iter()
                    .filter(|evidence| evidence.proposal_id == proposal_id)
                    .map(|evidence| evidence.outcome.as_str())
                    .collect::<Vec<_>>(),
                ["cancelled", "authorization_not_run", "application_not_run"]
            );
            assert_eq!(feature.review_history.last().unwrap().outcome, "not_run");
            assert!(checkpoint_reuses_retained_edits(&feature.checkpoint));
            assert!(!automatic_post_apply_is_ambiguous(feature));
        }

        assert!(engine
            .start(
                feature["id"].as_str(),
                feature["model_target"].as_str(),
                feature["status"].as_str(),
                feature["checkpoint"].as_str(),
            )
            .is_ok());
        engine.cancellation.store(1, Ordering::SeqCst);
    }

    #[tokio::test]
    async fn public_stop_pauses_preparing_once_and_reenable_reserves_fresh_work() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        let (_directory, mut engine) = control_test_engine();
        fs::write(engine.root.join("example/result.txt"), "saved").unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        Arc::get_mut(&mut engine).unwrap().model_targets[0].url =
            format!("http://{}", listener.local_addr().unwrap());
        engine
            .change(|state| {
                state.auto_ai_repair_enabled = true;
                state.auto_ai_repair_max_escalations = 3;
                state.auto_ai_repair_policy_revision = state.revision + 1;
                let feature = &mut state.queue[0];
                feature.repair_attempts = REPAIR_LIMIT;
                feature.checkpoint = "repair_3_validation_failed".into();
                feature.last_failure_kind = "validation_failure".into();
                feature.auto_ai_repair_limit = Some(3);
                feature.auto_repair_policy_revision = Some(state.revision + 1);
                feature.auto_repair_epoch = 5;
                set_auto_repair_lifecycle(feature, "running", "fixture")
            })
            .unwrap();
        let feature_id = engine.database.lock().unwrap().state.queue[0].id.clone();
        let (proposal_id, _, _, _) = engine
            .reserve_automatic_escalation(&feature_id)
            .unwrap()
            .unwrap();
        engine.running.store(true, Ordering::SeqCst);
        let stopped = control(
            State(engine.clone()),
            authorized_headers(),
            Json(json!({"action":"stop"})),
        )
        .await;
        engine.running.store(false, Ordering::SeqCst);
        assert_eq!(stopped.0, StatusCode::OK);
        assert_eq!(stopped.1["queue"][0]["status"], "paused");
        assert_eq!(
            stopped.1["queue"][0]["checkpoint"],
            "escalation_1_cancelled_before_authorization"
        );

        engine
            .finish_escalation_proposal(
                &feature_id,
                &proposal_id,
                Ok((
                    "late proposal".into(),
                    vec![RepairEscalationFile {
                        path: "late.txt".into(),
                        before: None,
                        after: "late".into(),
                        protected: false,
                    }],
                    Default::default(),
                )),
                &AtomicU8::new(0),
            )
            .unwrap();
        let stopped_again = control(
            State(engine.clone()),
            authorized_headers(),
            Json(json!({"action":"stop"})),
        )
        .await;
        assert_eq!(stopped_again.0, StatusCode::OK);
        {
            let database = engine.database.lock().unwrap();
            let feature = &database.state.queue[0];
            assert!(paused_preauthorization_cancellation_is_resumable(feature));
            assert_eq!(
                feature
                    .escalation_history
                    .iter()
                    .filter(|evidence| evidence.proposal_id == proposal_id)
                    .map(|evidence| evidence.outcome.as_str())
                    .collect::<Vec<_>>(),
                ["cancelled", "authorization_not_run", "application_not_run"]
            );
            assert_eq!(feature.review_history.len(), 1);

            let assert_invalid = |inexact: Feature| {
                assert!(!paused_preauthorization_cancellation_is_resumable(&inexact));
                assert!(validate_auto_repair_feature(&inexact).is_err());
            };
            let mut inexact = feature.clone();
            inexact.checkpoint = "escalation_2_cancelled_before_authorization".into();
            assert_invalid(inexact);
            inexact = feature.clone();
            inexact
                .escalation_proposal
                .as_mut()
                .unwrap()
                .applied_paths
                .push("result.txt".into());
            assert_invalid(inexact);

            let mut stale_epoch = feature.clone();
            stale_epoch.auto_repair_epoch += 1;
            assert_invalid(stale_epoch);

            let mut stale_policy = feature.clone();
            stale_policy.auto_repair_policy_revision = Some(
                stale_policy
                    .auto_repair_policy_revision
                    .unwrap()
                    .checked_add(1)
                    .unwrap(),
            );
            assert_invalid(stale_policy);

            let mut requested_application = feature.clone();
            requested_application
                .escalation_proposal
                .as_mut()
                .unwrap()
                .apply_request_id = Some(Uuid::new_v4().to_string());
            assert_invalid(requested_application);

            let mut missing_stage = feature.clone();
            missing_stage
                .escalation_history
                .retain(|evidence| evidence.outcome != "authorization_not_run");
            assert_invalid(missing_stage);

            let mut duplicate_stage = feature.clone();
            duplicate_stage
                .escalation_history
                .push(duplicate_stage.escalation_history[0].clone());
            assert_invalid(duplicate_stage);

            let mut unterminated_review = feature.clone();
            unterminated_review
                .escalation_proposal
                .as_mut()
                .unwrap()
                .review_slot_terminal = false;
            assert_invalid(unterminated_review);

            let mut duplicate_review = feature.clone();
            duplicate_review
                .review_history
                .push(duplicate_review.review_history[0].clone());
            assert_invalid(duplicate_review);
        }

        let revision = stopped_again.1["revision"].as_u64().unwrap();
        let disabled = engine.update_auto_ai_repair(revision, false, 3).unwrap();
        let response_content = json!({
            "summary": "repair the retained implementation",
            "files": [{"path": "result.txt", "content": "repaired"}]
        })
        .to_string();
        let response_body = json!({
            "choices": [{
                "message": {"content": response_content},
                "finish_reason": "stop"
            }]
        })
        .to_string();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = vec![0_u8; 65_536];
            let _ = socket.read(&mut request).await.unwrap();
            socket
                .write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        response_body.len(),
                        response_body
                    )
                    .as_bytes(),
                )
                .await
                .unwrap();
        });
        let reenabling = engine
            .update_auto_ai_repair(disabled["revision"].as_u64().unwrap(), true, 3)
            .unwrap();
        assert_eq!(reenabling["auto_ai_repair_enabled"], true);
        let new_policy_revision = reenabling["auto_ai_repair_policy_revision"]
            .as_u64()
            .unwrap();
        for _ in 0..200 {
            let authorized = engine.database.lock().unwrap().state.queue[0]
                .escalation_history
                .iter()
                .any(|evidence| evidence.outcome == "policy_authorized");
            if authorized
                && fs::read_to_string(engine.root.join("example/result.txt")).unwrap() == "repaired"
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        server.await.unwrap();
        let database = engine.database.lock().unwrap();
        let feature = &database.state.queue[0];
        assert_eq!(feature.escalation_count, 2);
        assert_eq!(feature.auto_ai_repair_limit, Some(3));
        assert_eq!(
            feature.auto_repair_policy_revision,
            Some(new_policy_revision)
        );
        assert_ne!(
            feature.escalation_proposal.as_ref().unwrap().proposal_id,
            proposal_id
        );
        assert_eq!(
            feature
                .escalation_proposal
                .as_ref()
                .unwrap()
                .policy_revision,
            Some(new_policy_revision)
        );
        assert!(feature
            .escalation_history
            .iter()
            .any(|evidence| evidence.outcome == "policy_authorized"
                && evidence.policy_revision == Some(new_policy_revision)));
        assert_eq!(
            fs::read_to_string(engine.root.join("example/result.txt")).unwrap(),
            "repaired"
        );
    }

    #[test]
    fn automatic_policy_authorization_accepts_tests_but_rejects_stale_epoch() {
        let (_dir, engine) = control_test_engine();
        fs::write(engine.root.join("example/result.txt"), "saved").unwrap();
        fs::create_dir_all(engine.root.join("example/tests")).unwrap();
        fs::write(
            engine.root.join("example/tests/test_app.py"),
            "assert False\n",
        )
        .unwrap();
        engine
            .change(|state| {
                state.auto_ai_repair_enabled = true;
                state.auto_ai_repair_max_escalations = 2;
                state.auto_ai_repair_policy_revision = state.revision + 1;
                let feature = &mut state.queue[0];
                feature.repair_attempts = REPAIR_LIMIT;
                feature.auto_ai_repair_limit = Some(2);
                feature.auto_repair_policy_revision = Some(state.revision + 1);
                feature.auto_repair_epoch = 4;
                set_auto_repair_lifecycle(feature, "running", "fixture")
            })
            .unwrap();
        let feature_id = engine.database.lock().unwrap().state.queue[0].id.clone();
        let (proposal_id, _, epoch, _) = engine
            .reserve_automatic_escalation(&feature_id)
            .unwrap()
            .unwrap();
        engine
            .finish_escalation_proposal(
                &feature_id,
                &proposal_id,
                Ok((
                    "repair source and test".into(),
                    vec![RepairEscalationFile {
                        path: "tests/test_app.py".into(),
                        before: Some("assert False\n".into()),
                        after: "assert True\n".into(),
                        protected: true,
                    }],
                    std::collections::BTreeMap::new(),
                )),
                &AtomicU8::new(0),
            )
            .unwrap();
        assert!(engine
            .authorize_automatic_proposal(&feature_id, &proposal_id, epoch + 1)
            .is_err());
        engine
            .authorize_automatic_proposal(&feature_id, &proposal_id, epoch)
            .unwrap();
        let database = engine.database.lock().unwrap();
        let feature = &database.state.queue[0];
        assert!(feature.escalation_pending);
        assert_eq!(
            feature.escalation_history.last().unwrap().outcome,
            "policy_authorized"
        );
        assert_eq!(
            feature.escalation_history.last().unwrap().source,
            "automatic_failure"
        );
    }

    #[test]
    fn automatic_authorization_drift_terminalizes_once_and_recovery_reserves_fresh_attempt() {
        let (_directory, engine) = control_test_engine();
        fs::write(engine.root.join("example/result.txt"), "saved").unwrap();
        engine
            .change(|state| {
                state.auto_ai_repair_enabled = true;
                state.auto_ai_repair_max_escalations = 3;
                state.auto_ai_repair_policy_revision = state.revision + 1;
                let feature = &mut state.queue[0];
                feature.repair_attempts = REPAIR_LIMIT;
                feature.auto_ai_repair_limit = Some(3);
                feature.auto_repair_policy_revision = Some(state.revision + 1);
                feature.auto_repair_epoch = 7;
                set_auto_repair_lifecycle(feature, "running", "fixture")
            })
            .unwrap();
        let feature_id = engine.database.lock().unwrap().state.queue[0].id.clone();
        let (proposal_id, _, epoch, _) = engine
            .reserve_automatic_escalation(&feature_id)
            .unwrap()
            .unwrap();
        engine
            .finish_escalation_proposal(
                &feature_id,
                &proposal_id,
                Ok((
                    "repair source".into(),
                    vec![RepairEscalationFile {
                        path: "result.txt".into(),
                        before: Some("saved".into()),
                        after: "repaired".into(),
                        protected: false,
                    }],
                    Default::default(),
                )),
                &AtomicU8::new(0),
            )
            .unwrap();
        fs::write(engine.root.join("example/result.txt"), "owner drift").unwrap();
        assert!(engine
            .authorize_automatic_proposal(&feature_id, &proposal_id, epoch)
            .is_err());

        let assert_terminal = || {
            let database = engine.database.lock().unwrap();
            let feature = &database.state.queue[0];
            assert_eq!(feature.auto_repair_lifecycle, "held");
            assert_eq!(
                feature.escalation_proposal.as_ref().unwrap().status,
                "authorization_rejected"
            );
            assert_eq!(
                feature
                    .escalation_history
                    .iter()
                    .filter(|evidence| evidence.proposal_id == proposal_id)
                    .map(|evidence| evidence.outcome.as_str())
                    .collect::<Vec<_>>(),
                ["ready", "authorization_rejected", "application_not_run"]
            );
            assert_eq!(
                feature
                    .review_history
                    .iter()
                    .filter(|evidence| evidence.outcome == "not_run")
                    .count(),
                1
            );
        };
        assert_terminal();
        assert!(engine
            .authorize_automatic_proposal(&feature_id, &proposal_id, epoch)
            .is_err());
        assert_terminal();

        engine
            .change(|state| {
                set_auto_repair_lifecycle(
                    &mut state.queue[0],
                    "running",
                    "Owner explicitly resumed after correcting authorization drift",
                )
            })
            .unwrap();
        let (next_proposal_id, _, _, _) = engine
            .reserve_automatic_escalation(&feature_id)
            .unwrap()
            .unwrap();
        assert_ne!(next_proposal_id, proposal_id);
        assert_eq!(
            engine.database.lock().unwrap().state.queue[0].escalation_count,
            2
        );
    }

    #[test]
    fn manual_cancel_fills_each_reserved_escalation_slot_exactly_once() {
        let (_directory, engine) = control_test_engine();
        let mut feature = feature_with_escalation("failed");
        feature.escalation_pending = false;
        feature.checkpoint = "failed_validation".into();
        feature.escalation_evidence_reserved = 3;
        feature.review_evidence_reserved = 1;
        let proposal_id = feature
            .escalation_proposal
            .as_ref()
            .unwrap()
            .proposal_id
            .clone();
        feature
            .escalation_history
            .retain(|evidence| evidence.proposal_id != proposal_id);
        let proposal = feature.escalation_proposal.as_mut().unwrap();
        proposal.status = "ready".into();
        proposal.applied_paths.clear();
        proposal.apply_request_id = None;
        feature.escalation_history.push(RepairEscalationEvidence {
            proposal_id: proposal.proposal_id.clone(),
            attempt: proposal.attempt,
            model_target: proposal.model_target.clone(),
            model: proposal.model.clone(),
            chat_id: proposal.chat_id.clone(),
            chat_request_id: proposal.chat_request_id.clone(),
            diagnosis_sha256: proposal.diagnosis_sha256.clone(),
            outcome: "ready".into(),
            proposal_sha256: Some(repair_escalation_proposal_sha256(proposal).unwrap()),
            candidate_sha256: Some(repair_escalation_candidate_sha256(proposal).unwrap()),
            summary: proposal.summary.clone(),
            source: proposal.source.clone(),
            automatic_epoch: proposal.automatic_epoch,
            policy_revision: proposal.policy_revision,
            limit_snapshot: proposal.limit_snapshot,
            project_state_sha256: proposal.project_state_sha256.clone(),
            authorization_revision: None,
        });
        engine
            .change(|state| {
                state.queue[0] = feature.clone();
                Ok(())
            })
            .unwrap();
        let revision = engine.snapshot().unwrap()["revision"].as_u64().unwrap();
        let request = RepairEscalationMutation {
            action: "cancel".into(),
            feature_id: feature.id.clone(),
            expected_revision: revision,
            expected_checkpoint: Some(feature.checkpoint.clone()),
            model_target: None,
            chat_id: None,
            chat_request_id: None,
            diagnosis_sha256: None,
            proposal_id: Some(proposal_id.clone()),
        };
        engine.cancel_escalation(request).unwrap();
        let database = engine.database.lock().unwrap();
        let current = &database.state.queue[0];
        assert_eq!(
            current
                .escalation_history
                .iter()
                .filter(|evidence| evidence.proposal_id == proposal_id)
                .map(|evidence| evidence.outcome.as_str())
                .collect::<Vec<_>>(),
            ["ready", "authorization_not_run", "application_not_run"]
        );
        assert_eq!(current.review_history.len(), 1);
        assert_eq!(current.review_history[0].outcome, "not_run");
        assert_eq!(
            current.escalation_proposal.as_ref().unwrap().status,
            "cancelled"
        );
    }

    #[test]
    fn trusted_review_classification_is_conservative_and_validation_first() {
        let validation = vec!["scripts/custom-build.sh".to_owned()];
        for path in [
            "tests/widget.rs",
            "conftest.py",
            "test_helpers.py",
            "scripts/custom-build.sh",
        ] {
            assert_eq!(
                trusted_review_file_classification(path, &validation),
                "test_or_validation_input",
                "{path}"
            );
        }
        for path in [
            "pytest.ini",
            "setup.cfg",
            "tox.ini",
            "setup.py",
            "next.config.js",
            "webpack.config.ts",
            "scripts/build.sh",
            "unknown.custom",
        ] {
            assert_eq!(
                trusted_review_file_classification(path, &[]),
                "project_configuration",
                "{path}"
            );
        }
        for path in ["src/main.rs", "Sources/App.swift", "lib/widget.tsx"] {
            assert_eq!(
                trusted_review_file_classification(path, &[]),
                "ordinary_source",
                "{path}"
            );
        }
    }

    #[test]
    fn cumulative_review_packet_classifies_earlier_test_and_configuration_inputs() {
        let directory = tempfile::tempdir().unwrap();
        let project = fs::canonicalize(directory.path()).unwrap();
        fs::write(project.join("main.rs"), "fn main() {}\n").unwrap();
        fs::write(project.join("conftest.py"), "VALUE = 1\n").unwrap();
        fs::write(project.join("setup.py"), "from setuptools import setup\n").unwrap();
        let mut feature = feature_with_status("running");
        feature.validation = "python conftest.py".into();
        feature.repair_history.push(RepairAttemptEvidence {
            attempt: 1,
            prior_checkpoint: "validation_failed".into(),
            prior_message: "failed".into(),
            prior_edits: vec![
                RepairEditEvidence {
                    path: "conftest.py".into(),
                    before: None,
                    content_hash: hash(b"VALUE = 1\n"),
                },
                RepairEditEvidence {
                    path: "setup.py".into(),
                    before: None,
                    content_hash: hash(b"from setuptools import setup\n"),
                },
            ],
        });
        let packet = developer_review_packet(
            &feature,
            &project,
            &[Edit {
                path: "main.rs".into(),
                content: "fn main() {}\n".into(),
                before: None,
            }],
            &"1".repeat(64),
        )
        .unwrap();
        let classes = packet
            .files
            .iter()
            .map(|file| (file.path.as_str(), file.classification.as_str()))
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(classes["conftest.py"], "test_or_validation_input");
        assert_eq!(classes["setup.py"], "project_configuration");
        assert_eq!(classes["main.rs"], "ordinary_source");
        let digest = packet.sha256().unwrap();
        let mut changed = packet;
        changed.files[0].classification = "ordinary_source".into();
        assert_ne!(changed.sha256().unwrap(), digest);
    }

    #[test]
    fn queue_v10_frozen_candidate_migrates_trusted_classification_with_legacy_digest() {
        let directory = tempfile::tempdir().unwrap();
        let project = fs::canonicalize(directory.path()).unwrap();
        fs::write(project.join("result.txt"), "saved").unwrap();
        let mut feature = feature_with_publication("failed");
        feature.publication_candidate[0].classification = "unclassified_legacy".into();
        let approved = feature.review_history.last_mut().unwrap();
        let packet = DeveloperReviewPacket {
            schema_version: 1,
            feature_id: feature.id.clone(),
            project: feature.project.clone(),
            instruction: feature.instruction.clone(),
            approved_plan_sha256: None,
            approved_plan: None,
            validation_command: feature.validation.clone(),
            validation_evidence_sha256: approved.validation_evidence_sha256.clone(),
            provider_id: REVIEW_PROVIDER_ID.into(),
            model_id: feature.review_model.clone(),
            reasoning_effort: feature.review_reasoning_effort.clone(),
            files: vec![DeveloperReviewFile {
                classification: "project_configuration".into(),
                path: "result.txt".into(),
                before_sha256: None,
                content_sha256: hash(b"saved"),
                content: "saved".into(),
            }],
        };
        approved.packet_sha256 = packet.legacy_sha256_without_classification().unwrap();
        migrate_legacy_publication_classifications(&mut feature, &project, 10).unwrap();
        assert_eq!(
            feature.publication_candidate[0].classification,
            "project_configuration"
        );
        for status in ["pending", "attention", "succeeded"] {
            feature.publication.as_mut().unwrap().status = status.into();
            publication_input(&feature).unwrap();
        }
    }

    #[test]
    fn authorization_revision_is_exact_for_v11_automatic_policy_evidence_only() {
        let mut evidence = feature_with_escalation("failed").escalation_history[0].clone();
        evidence.source = "automatic_failure".into();
        evidence.outcome = "policy_authorized".into();
        evidence.authorization_revision = Some(7);
        validate_authorization_revision(&evidence, 11, 7).unwrap();
        assert!(validate_authorization_revision(&evidence, 11, 6).is_err());
        evidence.authorization_revision = None;
        assert!(validate_authorization_revision(&evidence, 11, 7).is_err());
        validate_authorization_revision(&evidence, 10, 7).unwrap();
        evidence.authorization_revision = Some(7);
        evidence.source = "manual_chat".into();
        assert!(validate_authorization_revision(&evidence, 11, 7).is_err());
        evidence.source = "automatic_failure".into();
        evidence.outcome = "ready".into();
        assert!(validate_authorization_revision(&evidence, 11, 7).is_err());
    }

    #[test]
    fn durable_validation_failure_summary_redacts_secret_bearing_output() {
        let secret = "Bearer eyJhbGciOiJIUzI1NiJ9.test.signature123";
        let raw = format!("validation failed while printing {secret}");
        let durable = durable_failure_summary(&raw, "Validation failed", 4000);
        assert!(!durable.contains(secret));
        assert!(!serde_json::to_string(&json!({"message": durable}))
            .unwrap()
            .contains(secret));
        let automatic = bounded_code_failure_summary(&raw).unwrap();
        assert!(!automatic.contains(secret));
    }

    #[test]
    fn no_op_escalation_preserves_digest_bound_failure_for_the_next_attempt() {
        let (_directory, engine) = control_test_engine();
        let failure = "validation failed: expected 2, received 3";
        engine
            .change(|state| {
                state.auto_ai_repair_enabled = true;
                state.auto_ai_repair_max_escalations = 3;
                state.auto_ai_repair_policy_revision = state.revision + 1;
                let feature = &mut state.queue[0];
                feature.repair_attempts = REPAIR_LIMIT;
                feature.last_failure_kind = "validation_failure".into();
                feature.last_code_failure_summary = bounded_code_failure_summary(failure)?;
                feature.auto_ai_repair_limit = Some(3);
                feature.auto_repair_policy_revision = Some(state.revision + 1);
                feature.auto_repair_epoch = 4;
                set_auto_repair_lifecycle(feature, "running", "fixture")
            })
            .unwrap();
        let feature_id = engine.database.lock().unwrap().state.queue[0].id.clone();
        let (first_id, _, epoch, first_failure) = engine
            .reserve_automatic_escalation(&feature_id)
            .unwrap()
            .unwrap();
        assert_eq!(first_failure, failure);
        engine
            .finish_escalation_proposal(
                &feature_id,
                &first_id,
                Ok(("no change".into(), Vec::new(), Default::default())),
                &AtomicU8::new(0),
            )
            .unwrap();
        assert!(engine
            .finish_automatic_without_application(&feature_id, &first_id, epoch)
            .unwrap());
        let (_second_id, _, _, second_failure) = engine
            .reserve_automatic_escalation(&feature_id)
            .unwrap()
            .unwrap();
        let feature = &engine.database.lock().unwrap().state.queue[0];
        let second = feature.escalation_proposal.as_ref().unwrap();
        assert_eq!(second_failure, failure);
        assert_eq!(second.diagnosis_sha256, hash(failure.as_bytes()));
        assert!(automatic_failure_prompt_json(second, &second_failure)
            .unwrap()
            .contains(failure));
    }

    #[test]
    fn below_limit_reservation_skips_recovery_scan_but_cap_failure_terminalizes() {
        let (_directory, engine) = control_test_engine();
        let project = engine.root.join("example");
        for index in 0..90 {
            fs::write(project.join(format!("bounded-{index:03}.txt")), "x").unwrap();
        }
        engine
            .change(|state| {
                state.auto_ai_repair_enabled = true;
                state.auto_ai_repair_max_escalations = 1;
                state.auto_ai_repair_policy_revision = state.revision + 1;
                let feature = &mut state.queue[0];
                feature.status = "failed".into();
                feature.checkpoint = "failed_validation".into();
                feature.repair_attempts = REPAIR_LIMIT;
                feature.last_failure_kind = "validation_failure".into();
                feature.last_code_failure_summary = "compile failed".into();
                feature.auto_ai_repair_limit = Some(1);
                feature.auto_repair_policy_revision = Some(state.revision + 1);
                set_auto_repair_lifecycle(feature, "running", "fixture")
            })
            .unwrap();
        let feature_id = engine.database.lock().unwrap().state.queue[0].id.clone();
        assert!(engine
            .reserve_automatic_escalation(&feature_id)
            .unwrap()
            .is_some());

        engine
            .change(|state| {
                let feature = &mut state.queue[0];
                feature.escalation_pending = false;
                feature.escalation_proposal = None;
                feature.escalation_count = 1;
                feature.escalation_evidence_reserved = 0;
                feature.review_evidence_reserved = 0;
                feature.auto_repair_step_started_at_ms = None;
                feature.checkpoint = "failed_validation".into();
                set_auto_repair_lifecycle(feature, "running", "fixture")
            })
            .unwrap();
        assert!(engine.reserve_automatic_escalation(&feature_id).is_err());
        let database = engine.database.lock().unwrap();
        let feature = &database.state.queue[0];
        assert_eq!(feature.auto_repair_lifecycle, "limit_reached");
        assert_eq!(feature.escalation_count, 1);
        assert_eq!(feature.auto_repair_step_started_at_ms, None);
        assert!(!engine.running.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn cap_transition_does_not_wait_for_the_shared_inference_gate() {
        let (_directory, engine) = control_test_engine();
        engine
            .change(|state| {
                state.auto_ai_repair_enabled = true;
                state.auto_ai_repair_max_escalations = 1;
                state.auto_ai_repair_policy_revision = state.revision + 1;
                let feature = &mut state.queue[0];
                feature.status = "failed".into();
                feature.checkpoint = "failed_validation".into();
                feature.repair_attempts = REPAIR_LIMIT;
                feature.escalation_count = 1;
                feature.last_failure_kind = "validation_failure".into();
                feature.last_code_failure_summary = "compile failed".into();
                feature.auto_ai_repair_limit = Some(1);
                feature.auto_repair_policy_revision = Some(state.revision + 1);
                set_auto_repair_lifecycle(feature, "running", "fixture")
            })
            .unwrap();
        let feature_id = engine.database.lock().unwrap().state.queue[0].id.clone();
        let _occupied = engine.inference_gate.try_acquire().unwrap();
        assert!(!engine
            .prepare_next_automatic_escalation(&feature_id)
            .await
            .unwrap());
        assert_eq!(
            engine.database.lock().unwrap().state.queue[0].auto_repair_lifecycle,
            "limit_reached"
        );
    }

    #[test]
    fn revalidation_after_limit_is_offered_only_for_exhausted_failed_or_paused_features() {
        for (lifecycle, status, expected) in [
            ("limit_reached", "failed", true),
            ("limit_reached", "paused", true),
            ("limit_reached", "running", false),
            ("held", "failed", false),
            ("quarantined", "paused", false),
            ("inactive", "failed", false),
            ("running", "failed", false),
        ] {
            let (_directory, engine) = control_test_engine();
            engine
                .change(|state| {
                    state.queue[0].status = status.into();
                    state.queue[0].auto_ai_repair_limit = Some(3);
                    state.queue[0].escalation_count = 3;
                    set_auto_repair_lifecycle(
                        &mut state.queue[0],
                        lifecycle,
                        "fixture recovery boundary",
                    )
                })
                .unwrap();
            let projected = engine.snapshot().unwrap();
            assert_eq!(
                projected["queue"][0]["can_revalidate_after_auto_repair_limit"],
                json!(expected),
                "{lifecycle}/{status}"
            );
        }
    }

    #[tokio::test]
    async fn stopped_limit_recovery_is_rejected_before_project_inspection() {
        let (_directory, engine) = control_test_engine();
        let project = fs::canonicalize(engine.root.join("example")).unwrap();
        fs::write(project.join("main.rs"), "fn value() -> i32 { 1 }\n").unwrap();
        let baseline = admitted_project_snapshot(&project).unwrap();
        let mut feature = engine.database.lock().unwrap().state.queue[0].clone();
        feature.status = "failed".into();
        feature.checkpoint = "escalation_1_failed".into();
        feature.auto_ai_repair_limit = Some(1);
        feature.escalation_count = 1;
        feature.auto_repair_limit_project_baseline = baseline
            .files
            .iter()
            .map(|(path, (digest, _))| (path.clone(), digest.clone()))
            .collect();
        feature.auto_repair_limit_unadmitted_sha256 = Some(baseline.unadmitted_sha256);
        feature.auto_repair_limit_volatile_sha256 = Some(baseline.volatile_sha256);
        set_auto_repair_lifecycle(
            &mut feature,
            "limit_reached",
            "1 of 1 AI escalations used. Automatic repair stopped. Correct the project without AI and revalidate, change the reviewer when applicable, or remove the feature.",
        )
        .unwrap();
        engine
            .change(|state| {
                state.queue[0] = feature.clone();
                Ok(())
            })
            .unwrap();
        let revision = engine.snapshot().unwrap()["revision"].as_u64().unwrap();
        let feature_id = feature.id.clone();
        engine.cancellation.store(1, Ordering::SeqCst);

        let error = engine
            .start(
                Some(&feature_id),
                Some("mac"),
                Some("failed"),
                Some("escalation_1_failed"),
            )
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("cancelled before project inspection"),
            "unexpected recovery error: {error:#}"
        );
        assert_eq!(
            engine.snapshot().unwrap()["revision"].as_u64().unwrap(),
            revision
        );
        assert_eq!(
            engine.database.lock().unwrap().state.queue[0].checkpoint,
            "escalation_1_failed"
        );
    }

    #[tokio::test]
    async fn limit_recovery_resume_revalidates_owner_bytes_without_inference_or_new_escalation() {
        let (_directory, engine) = control_test_engine();
        let project = fs::canonicalize(engine.root.join("example")).unwrap();
        fs::write(project.join("main.rs"), "fn value() -> i32 { 1 }\n").unwrap();
        let baseline = admitted_project_snapshot(&project).unwrap();
        let baseline_digest = baseline.files.get("main.rs").unwrap().0.clone();
        let mut feature = engine.database.lock().unwrap().state.queue[0].clone();
        feature.status = "failed".into();
        feature.checkpoint = "escalation_1_failed".into();
        feature.repair_attempts = REPAIR_LIMIT;
        feature.escalation_count = 1;
        feature.auto_ai_repair_limit = Some(1);
        feature.auto_repair_policy_revision = Some(7);
        feature.edits = Some(vec![Edit {
            path: "main.rs".into(),
            content: "fn value() -> i32 { 4 }\n".into(),
            before: Some(baseline_digest.clone()),
        }]);
        feature.auto_repair_limit_project_baseline = baseline
            .files
            .iter()
            .map(|(path, (digest, _))| (path.clone(), digest.clone()))
            .collect();
        feature.auto_repair_limit_unadmitted_sha256 = Some(baseline.unadmitted_sha256);
        feature.auto_repair_limit_volatile_sha256 = Some(baseline.volatile_sha256);
        set_auto_repair_lifecycle(
            &mut feature,
            "limit_reached",
            "1 of 1 AI escalations used. Automatic repair stopped. Correct the project without AI and revalidate, change the reviewer when applicable, or remove the feature.",
        )
        .unwrap();
        engine
            .change(|state| {
                state.auto_ai_repair_enabled = true;
                state.auto_ai_repair_max_escalations = 1;
                state.auto_ai_repair_policy_revision = 7;
                state.queue[0] = feature.clone();
                Ok(())
            })
            .unwrap();
        let revision = engine.snapshot().unwrap()["revision"].as_u64().unwrap();
        let feature_id = feature.id.clone();

        // The owner corrects project bytes without AI before resuming.
        fs::write(project.join("main.rs"), "fn value() -> i32 { 2 }\n").unwrap();

        // Validation-only limit recovery is a durable pre-spawn transition: the
        // exhausted escalation budget stays exhausted, no model call is
        // authorized, and no new escalation evidence is reserved. Assertions
        // stop at the admitted durable checkpoint; spawned validation-process
        // completion is covered by native E2E suites.
        engine
            .start(
                Some(&feature_id),
                Some("mac"),
                Some("failed"),
                Some("escalation_1_failed"),
            )
            .unwrap();
        // The recovery run owns the single start slot without any escalation
        // call or automatic repair authorization.
        assert!(engine.running.load(Ordering::SeqCst));
        assert!(!engine.escalation_running.load(Ordering::SeqCst));
        assert!(engine.escalation_cancellation.lock().unwrap().is_none());
        assert!(!engine.repair_loop_authorized.load(Ordering::SeqCst));
        assert!(!engine.chat.is_running());
        assert!(!engine.tools.is_running());

        // The exact resume cost is two revisions: one durable publication-selection
        // freeze and one durable validation-only recovery checkpoint.
        let recovered = engine.snapshot().unwrap();
        assert_eq!(recovered["revision"].as_u64().unwrap(), revision + 2);
        let projected = &recovered["queue"][0];
        assert_eq!(projected["status"], "paused");
        assert_eq!(projected["checkpoint"], "auto_repair_limit_revalidating");
        assert_eq!(projected["auto_repair_lifecycle"], "limit_reached");
        assert_eq!(
            projected["auto_repair_reason"],
            "1 of 1 AI escalations used. Automatic repair stopped. Correct the project without AI and revalidate, change the reviewer when applicable, or remove the feature."
        );
        assert_eq!(projected["auto_ai_repair_limit"], 1);
        assert_eq!(projected["escalation_count"], 1);
        assert_eq!(projected["review_status"], "pending");
        assert_eq!(projected["can_revalidate_after_auto_repair_limit"], true);
        assert!(projected["changed_files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|path| path == "main.rs"));
        assert!(projected["message"]
            .as_str()
            .unwrap()
            .contains("no repair model attempt is authorized"));
        {
            let database = engine.database.lock().unwrap();
            let durable = &database.state.queue[0];
            // Owner-corrected bytes are admitted for validation-only review.
            let edits = durable.edits.as_deref().unwrap();
            assert_eq!(edits.len(), 1);
            assert_eq!(edits[0].path, "main.rs");
            assert_eq!(edits[0].content, "fn value() -> i32 { 2 }\n");
            assert_eq!(edits[0].before.as_deref(), Some(baseline_digest.as_str()));
            // The exhausted cap, counter, and policy binding are immutable.
            assert_eq!(durable.auto_ai_repair_limit, Some(1));
            assert_eq!(durable.escalation_count, 1);
            assert_eq!(durable.auto_repair_policy_revision, Some(7));
            assert_eq!(durable.auto_repair_lifecycle, "limit_reached");
            assert!(!durable.escalation_pending);
            assert!(!durable.repair_pending);
            assert_eq!(durable.escalation_evidence_reserved, 0);
            assert_eq!(durable.review_evidence_reserved, 0);
            assert!(durable.escalation_proposal.is_none());
            assert!(durable.escalation_history.is_empty());
            // Review state is reset to a fresh validation-only pre-review binding.
            assert!(durable.review_pending.is_none());
            assert_eq!(durable.review_status, "pending");
            assert_eq!(durable.review_attempts, 0);
            assert!(durable.review_history.is_empty());
            assert!(durable
                .review_summary
                .contains("Validation-only limit recovery"));
        }

        // Neither the resume route nor the recovery transition reserved the
        // shared inference slot: validation-only limit recovery never infers.
        assert!(engine.inference_gate.try_acquire().is_ok());
        {
            let database = engine.database.lock().unwrap();
            assert_eq!(database.state.auto_ai_repair_max_escalations, 1);
            assert_eq!(database.state.auto_ai_repair_policy_revision, 7);
        }

        // A stale resume that observed pre-recovery state is rejected without
        // mutating anything.
        let stale = engine
            .start(
                Some(&feature_id),
                Some("mac"),
                Some("failed"),
                Some("escalation_1_failed"),
            )
            .unwrap_err();
        assert!(
            stale.to_string().contains("state changed"),
            "unexpected stale-resume error: {stale:#}"
        );

        // A parallel resume that observed the recovered state cannot re-enter
        // while the recovery run holds the start flag, and mutates nothing.
        engine
            .start(
                Some(&feature_id),
                Some("mac"),
                Some("paused"),
                Some("auto_repair_limit_revalidating"),
            )
            .unwrap();
        let after_parallel = engine.snapshot().unwrap();
        assert_eq!(after_parallel["revision"].as_u64().unwrap(), revision + 2);
        assert_eq!(after_parallel["queue"][0]["status"], "paused");
        assert_eq!(
            after_parallel["queue"][0]["checkpoint"],
            "auto_repair_limit_revalidating"
        );
        assert_eq!(after_parallel["queue"][0]["escalation_count"], 1);
        assert!(engine.inference_gate.try_acquire().is_ok());
    }

    #[test]
    fn missing_limit_baseline_recovers_only_by_reviewing_all_current_admitted_files() {
        let directory = tempfile::tempdir().unwrap();
        let project = fs::canonicalize(directory.path()).unwrap();
        for index in 0..81 {
            fs::write(
                project.join(format!("source-{index:03}.rs")),
                "fn value() {}\n",
            )
            .unwrap();
        }
        assert!(admitted_project_snapshot(&project).is_err());
        for index in 30..81 {
            fs::remove_file(project.join(format!("source-{index:03}.rs"))).unwrap();
        }
        let reduced = admitted_project_snapshot(&project).unwrap();
        let mut feature = feature_with_status("failed");
        feature.auto_repair_lifecycle = "limit_reached".into();
        feature.auto_ai_repair_limit = Some(1);
        feature.escalation_count = 1;
        let edits = prepare_limit_recovery_edits(&mut feature, &reduced).unwrap();
        assert_eq!(edits.len(), reduced.files.len());
        assert!(edits.iter().all(|edit| edit.before.is_some()));
        feature.edits = Some(edits.clone());
        let packet = developer_review_packet(&feature, &project, &edits, &"9".repeat(64)).unwrap();
        assert_eq!(packet.files.len(), reduced.files.len());
        assert!(packet.files.iter().all(|file| {
            file.before_sha256.as_deref() == Some(file.content_sha256.as_str())
                && !file.content.is_empty()
        }));
    }

    #[cfg(unix)]
    #[test]
    fn limit_recovery_refuses_unix_special_entries_without_reading_them() {
        use std::os::unix::net::UnixListener;

        let directory = tempfile::tempdir().unwrap();
        let project = fs::canonicalize(directory.path()).unwrap();
        fs::write(project.join("main.rs"), "fn main() {}\n").unwrap();
        let socket_path = project.join("untrusted.sock");
        let _listener = UnixListener::bind(&socket_path).unwrap();

        let error = admitted_project_snapshot(&project)
            .expect_err("special project entry must fail closed");
        assert!(
            error
                .to_string()
                .contains("refuses a special project entry"),
            "unexpected recovery error: {error:#}"
        );
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn limit_recovery_refuses_native_hard_link_before_content_admission() {
        let container = tempfile::tempdir().unwrap();
        let project = container.path().join("project");
        let outside = container.path().join("outside");
        fs::create_dir(&project).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::write(project.join("main.rs"), "fn main() {}\n").unwrap();
        let sensitive = outside.join("sensitive.rs");
        fs::write(
            &sensitive,
            "const OUT_OF_PROJECT_CREDENTIAL: &str = \"must-not-enter-review\";\n",
        )
        .unwrap();
        fs::hard_link(&sensitive, project.join("aliased.rs")).unwrap();
        let project = fs::canonicalize(project).unwrap();

        let context_error = project_context(&project)
            .expect_err("multi-link project file must not enter an automatic prompt");
        assert!(
            context_error.to_string().contains("multiple hard links"),
            "unexpected project-context error: {context_error:#}"
        );
        let error = admitted_project_snapshot(&project)
            .expect_err("multi-link project file must fail closed");
        assert!(
            error.to_string().contains("multiple hard links"),
            "unexpected recovery error: {error:#}"
        );
    }

    #[cfg(unix)]
    #[test]
    fn limit_recovery_holds_unix_ancestor_during_concurrent_symlink_replacement() {
        use std::os::unix::fs::symlink;
        use std::sync::mpsc;

        let project_directory = tempfile::tempdir().unwrap();
        let outside_directory = tempfile::tempdir().unwrap();
        let project = fs::canonicalize(project_directory.path()).unwrap();
        let outside = fs::canonicalize(outside_directory.path()).unwrap();
        fs::create_dir(project.join("nested")).unwrap();
        fs::write(
            project.join("nested/safe.rs"),
            "fn held_directory_content() {}\n",
        )
        .unwrap();
        let aliased_secret = "outside-sensitive-content-must-not-be-read";
        fs::write(outside.join("secret.rs"), aliased_secret).unwrap();

        let (opened_tx, opened_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let scan_root = project.clone();
        let scan = std::thread::spawn(move || {
            let directory_opened = |relative: &Path| {
                if relative == Path::new("nested") {
                    opened_tx.send(()).unwrap();
                    release_rx
                        .recv_timeout(Duration::from_secs(5))
                        .expect("ancestor replacement barrier was not released");
                }
            };
            admitted_project_snapshot_unix(&scan_root, None, Some(&directory_opened))
        });

        opened_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("scanner did not hold the nested directory before traversal");
        fs::rename(project.join("nested"), project.join("held-nested")).unwrap();
        symlink(&outside, project.join("nested")).unwrap();
        release_tx.send(()).unwrap();
        let snapshot = scan.join().unwrap().unwrap();

        fs::remove_file(project.join("nested")).unwrap();
        fs::rename(project.join("held-nested"), project.join("nested")).unwrap();
        assert_eq!(
            snapshot
                .files
                .get("nested/safe.rs")
                .map(|(_, content)| content.as_str()),
            Some("fn held_directory_content() {}\n")
        );
        assert!(!snapshot.files.contains_key("nested/secret.rs"));
        assert!(snapshot
            .files
            .values()
            .all(|(_, content)| !content.contains(aliased_secret)));
    }

    #[cfg(windows)]
    #[test]
    fn limit_recovery_refuses_directory_junction_before_alias_traversal() {
        let project_directory = tempfile::tempdir().unwrap();
        let outside_directory = tempfile::tempdir().unwrap();
        let project = fs::canonicalize(project_directory.path()).unwrap();
        let outside = fs::canonicalize(outside_directory.path()).unwrap();
        fs::write(project.join("main.rs"), "fn main() {}\n").unwrap();
        fs::write(
            outside.join("must-not-enter-recovery.txt"),
            "aliased content must never be read or persisted\n",
        )
        .unwrap();
        // cmd.exe's mklink parser is not reliable with Rust's extended-length
        // canonical path spelling, so use the equivalent ordinary temp paths
        // only for this disposable native junction ceremony.
        let junction = project_directory.path().join("aliased-directory");
        let creation = std::process::Command::new("cmd.exe")
            .args(["/D", "/C", "mklink", "/J"])
            .arg(&junction)
            .arg(outside_directory.path())
            .output()
            .unwrap();
        assert!(
            creation.status.success(),
            "failed to create disposable test junction: {}",
            String::from_utf8_lossy(&creation.stderr)
        );
        assert!(planning_metadata_is_reparse(
            &fs::symlink_metadata(&junction).unwrap()
        ));

        let result = admitted_project_snapshot(&project);
        let cleanup = std::process::Command::new("cmd.exe")
            .args(["/D", "/C", "rmdir"])
            .arg(&junction)
            .output()
            .unwrap();
        assert!(
            cleanup.status.success(),
            "failed to remove disposable test junction: {}",
            String::from_utf8_lossy(&cleanup.stderr)
        );
        let error = result.expect_err("directory junction must fail closed");
        assert!(
            error
                .to_string()
                .contains("refuses a Windows reparse point"),
            "unexpected recovery error: {error:#}"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn failed_limit_validation_refreshes_only_runner_mutated_volatile_evidence() {
        let (_directory, engine) = control_test_engine();
        let project = fs::canonicalize(engine.root.join("example")).unwrap();
        fs::write(project.join("main.rs"), "fn value() -> i32 { 1 }\n").unwrap();
        let baseline = admitted_project_snapshot(&project).unwrap();
        fs::write(project.join("main.rs"), "fn value() -> i32 { 2 }\n").unwrap();
        let current = admitted_project_snapshot(&project).unwrap();
        let mut feature = engine.database.lock().unwrap().state.queue[0].clone();
        feature.status = "paused".into();
        feature.checkpoint = "auto_repair_limit_revalidating".into();
        feature.auto_repair_lifecycle = "limit_reached".into();
        feature.auto_ai_repair_limit = Some(1);
        feature.escalation_count = 1;
        feature.auto_repair_limit_project_baseline = baseline
            .files
            .iter()
            .map(|(path, (digest, _))| (path.clone(), digest.clone()))
            .collect();
        feature.auto_repair_limit_unadmitted_sha256 = Some(baseline.unadmitted_sha256);
        feature.auto_repair_limit_volatile_sha256 = Some(baseline.volatile_sha256.clone());
        feature.edits = Some(reconcile_limit_recovery_edits(&feature, &current).unwrap());
        feature.validation = "mkdir -p target && printf runner > target/output && exit 1".into();
        engine
            .change(|state| {
                state.queue[0] = feature.clone();
                Ok(())
            })
            .unwrap();
        assert!(engine.validate_command(&feature, &project).await.is_err());
        let refreshed = engine.database.lock().unwrap().state.queue[0].clone();
        assert_ne!(
            refreshed.auto_repair_limit_volatile_sha256.as_deref(),
            Some(baseline.volatile_sha256.as_str())
        );

        fs::write(project.join("main.rs"), "fn value() -> i32 { 3 }\n").unwrap();
        let corrected = admitted_project_snapshot(&project).unwrap();
        assert!(reconcile_limit_recovery_edits(&refreshed, &corrected).is_ok());
        fs::write(project.join("target/output"), "owner drift").unwrap();
        let tampered = admitted_project_snapshot(&project).unwrap();
        assert!(reconcile_limit_recovery_edits(&refreshed, &tampered).is_err());
    }

    #[test]
    fn limit_recovery_admits_bounded_owner_changes_and_rejects_deletion_or_hidden_drift() {
        let directory = tempfile::tempdir().unwrap();
        let project = fs::canonicalize(directory.path()).unwrap();
        fs::create_dir_all(project.join("tests")).unwrap();
        fs::create_dir_all(project.join(".github")).unwrap();
        fs::write(project.join("main.rs"), "fn value() -> i32 { 1 }\n").unwrap();
        fs::write(project.join("setup.py"), "from setuptools import setup\n").unwrap();
        fs::write(project.join(".github/workflow.yml"), "name: check\n").unwrap();
        let baseline = admitted_project_snapshot(&project).unwrap();
        let mut feature = feature_with_status("failed");
        feature.auto_ai_repair_limit = Some(2);
        feature.escalation_count = 2;
        feature.auto_repair_lifecycle = "limit_reached".into();
        feature.auto_repair_limit_project_baseline = baseline
            .files
            .iter()
            .map(|(path, (digest, _))| (path.clone(), digest.clone()))
            .collect();
        feature.auto_repair_limit_unadmitted_sha256 = Some(baseline.unadmitted_sha256.clone());
        feature.auto_repair_limit_volatile_sha256 = Some(baseline.volatile_sha256.clone());
        feature.edits = Some(vec![Edit {
            path: "main.rs".into(),
            content: "fn value() -> i32 { 1 }\n".into(),
            before: None,
        }]);

        fs::write(project.join("main.rs"), "fn value() -> i32 { 2 }\n").unwrap();
        fs::write(project.join("next.config.js"), "export default {}\n").unwrap();
        fs::write(project.join("tests/new_test.py"), "assert True\n").unwrap();
        let current = admitted_project_snapshot(&project).unwrap();
        let edits = reconcile_limit_recovery_edits(&feature, &current).unwrap();
        assert_eq!(edits.len(), 3);
        feature.edits = Some(edits.clone());
        feature.validation = "python tests/new_test.py".into();
        let packet = developer_review_packet(&feature, &project, &edits, &"2".repeat(64)).unwrap();
        let classes = packet
            .files
            .iter()
            .map(|file| (file.path.as_str(), file.classification.as_str()))
            .collect::<std::collections::BTreeMap<_, _>>();
        assert_eq!(classes["next.config.js"], "project_configuration");
        assert_eq!(classes["tests/new_test.py"], "test_or_validation_input");

        fs::remove_file(project.join("setup.py")).unwrap();
        let deleted = admitted_project_snapshot(&project).unwrap();
        assert!(reconcile_limit_recovery_edits(&feature, &deleted).is_err());
        fs::write(project.join("setup.py"), "from setuptools import setup\n").unwrap();
        fs::write(project.join(".github/workflow.yml"), "name: changed\n").unwrap();
        let hidden_drift = admitted_project_snapshot(&project).unwrap();
        assert!(reconcile_limit_recovery_edits(&feature, &hidden_drift).is_err());
        fs::write(project.join(".github/workflow.yml"), "name: check\n").unwrap();
        fs::create_dir_all(project.join("target/debug")).unwrap();
        fs::write(project.join("target/debug/build-output"), "changed\n").unwrap();
        let target_drift = admitted_project_snapshot(&project).unwrap();
        assert!(reconcile_limit_recovery_edits(&feature, &target_drift).is_err());

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink("main.rs", project.join("link-to-source")).unwrap();
            let with_link = admitted_project_snapshot(&project).unwrap();
            feature.auto_repair_limit_unadmitted_sha256 = Some(with_link.unadmitted_sha256.clone());
            feature.auto_repair_limit_volatile_sha256 = Some(with_link.volatile_sha256.clone());
            fs::remove_file(project.join("link-to-source")).unwrap();
            symlink("setup.py", project.join("link-to-source")).unwrap();
            let changed_link = admitted_project_snapshot(&project).unwrap();
            assert!(reconcile_limit_recovery_edits(&feature, &changed_link).is_err());
        }
    }

    #[test]
    fn safe_pre_effect_cancellation_rearms_once_but_quarantine_and_limit_never_do() {
        let mut feature = feature_with_status("running");
        feature.repair_attempts = 1;
        feature.repair_pending = true;
        feature.checkpoint = "repair_1_prepared".into();
        feature.auto_ai_repair_limit = Some(5);
        feature.auto_repair_policy_revision = Some(4);
        feature.auto_repair_epoch = 7;
        set_auto_repair_lifecycle(&mut feature, "running", "fixture").unwrap();
        persist_automatic_cancellation_intent(&mut feature, true, "inactive", "quarantined")
            .unwrap();
        assert_eq!(feature.auto_repair_lifecycle, "held");
        assert_eq!(feature.status, "paused");
        let old_epoch = feature.auto_repair_epoch;
        assert!(enable_auto_repair_for_failed_feature(&mut feature, false, 100, 9).unwrap());
        assert_eq!(feature.auto_ai_repair_limit, Some(5));
        assert_eq!(feature.repair_attempts, 1);
        assert_eq!(feature.auto_repair_epoch, old_epoch + 1);
        assert_eq!(feature.auto_repair_policy_revision, Some(9));

        for lifecycle in ["quarantined", "limit_reached"] {
            let mut blocked = feature_with_status("failed");
            blocked.auto_repair_lifecycle = lifecycle.into();
            blocked.auto_ai_repair_limit = Some(3);
            blocked.escalation_count = 2;
            let epoch = blocked.auto_repair_epoch;
            assert!(enable_auto_repair_for_failed_feature(&mut blocked, true, 99, 10).is_err());
            assert_eq!(blocked.auto_repair_lifecycle, lifecycle);
            assert_eq!(blocked.auto_ai_repair_limit, Some(3));
            assert_eq!(blocked.escalation_count, 2);
            assert_eq!(blocked.auto_repair_epoch, epoch);
        }
    }

    #[test]
    fn auto_run_arms_the_next_selected_feature_with_its_own_immutable_budget() {
        let mut first = feature_with_status("succeeded");
        first.auto_ai_repair_limit = Some(2);
        first.escalation_count = 1;
        let mut second = feature_with_status("queued");
        second.id = Uuid::new_v4().to_string();
        second.checkpoint = "not_started".into();
        let second_id = second.id.clone();
        let mut state = Snapshot {
            auto_run: true,
            auto_ai_repair_enabled: true,
            auto_ai_repair_max_escalations: 9,
            auto_ai_repair_policy_revision: 41,
            queue: vec![first, second],
            ..Default::default()
        };
        assert!(arm_selected_queued_feature(&mut state, &second_id).unwrap());
        let second = &state.queue[1];
        assert_eq!(second.auto_ai_repair_limit, Some(9));
        assert_eq!(second.auto_repair_policy_revision, Some(41));
        assert_eq!(second.auto_repair_lifecycle, "running");
        assert_eq!(second.auto_repair_epoch, 1);
        assert_eq!(state.queue[0].auto_ai_repair_limit, Some(2));
        assert_eq!(state.queue[0].escalation_count, 1);

        let mut third = feature_with_status("queued");
        third.id = Uuid::new_v4().to_string();
        third.checkpoint = "not_started".into();
        let third_id = third.id.clone();
        let mut disabled = Snapshot {
            auto_run: true,
            auto_ai_repair_enabled: false,
            auto_ai_repair_max_escalations: 6,
            auto_ai_repair_policy_revision: 50,
            queue: vec![third],
            ..Default::default()
        };
        assert!(arm_selected_queued_feature(&mut disabled, &third_id).unwrap());
        assert_eq!(disabled.queue[0].auto_ai_repair_limit, Some(6));
        assert_eq!(disabled.queue[0].auto_repair_lifecycle, "inactive");
        disabled.auto_ai_repair_max_escalations = 99;
        disabled.auto_ai_repair_enabled = true;
        disabled.auto_ai_repair_policy_revision = 51;
        assert!(arm_selected_queued_feature(&mut disabled, &third_id).unwrap());
        assert_eq!(disabled.queue[0].auto_ai_repair_limit, Some(6));
        assert_eq!(disabled.queue[0].auto_repair_lifecycle, "running");
        assert_eq!(disabled.queue[0].auto_repair_policy_revision, Some(51));
    }

    #[tokio::test]
    async fn emergency_pause_prevents_validation_process_creation() {
        let (_directory, engine) = control_test_engine();
        let marker = engine.root.join("example/emergency-marker");
        let mut feature = engine.database.lock().unwrap().state.queue[0].clone();
        feature.validation = format!("touch {}", marker.display());
        let response = control(
            State(engine.clone()),
            authorized_headers(),
            Json(json!({"action":"emergency"})),
        )
        .await;
        assert_eq!(response.0, StatusCode::OK);
        let project = fs::canonicalize(engine.root.join("example")).unwrap();
        assert!(engine.validate_command(&feature, &project).await.is_err());
        assert!(!marker.exists());
    }

    #[test]
    fn generated_paths_remain_in_selected_project() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        for path in [
            "../outside",
            "/tmp/outside",
            ".git/config",
            "a/../../outside",
            "C:/outside",
            "a\\b",
        ] {
            assert!(checked_path(&root, path).is_err(), "{path}");
        }
        assert!(checked_path(&root, "src/main.py").is_ok());
    }

    // Digest conventions must match developer_chat.rs request_payload_sha256.
    fn chat_request_payload_sha256(
        project: &str,
        chat_id: &str,
        message: &str,
        model_target: &str,
    ) -> String {
        hash(
            &serde_json::to_vec(&json!({
                "project":project,
                "chat_id":chat_id,
                "message":message,
                "model_target":model_target,
                "attachments":Vec::<ChatAttachment>::new(),
            }))
            .unwrap(),
        )
    }

    // Digest conventions must match developer_chat.rs chat_response_provenance_sha256.
    fn chat_reply_provenance_sha256(
        project: &str,
        chat_id: &str,
        request_id: &str,
        model_target: &str,
        model: &str,
        content: &str,
    ) -> String {
        hash(
            &serde_json::to_vec(&json!({
                "project":project,
                "chat_id":chat_id,
                "request_id":request_id,
                "model_target":model_target,
                "model":model,
                "content":content,
            }))
            .unwrap(),
        )
    }

    // The DeveloperChat database field is private, so the completed repair handoff is
    // installed through a separate connection to developer.sqlite3 using the exact
    // request payload, content, and provenance bindings repair_handoff re-verifies.
    fn install_completed_chat_repair_handoff(engine: &Engine) -> (String, String, String) {
        let chat_id = Uuid::new_v4().to_string();
        let request_id = Uuid::new_v4().to_string();
        let project = "example";
        let model_target = "mac";
        let model = "fixture";
        let question = "Diagnose the failed validation";
        let diagnosis =
            "The fixture validation failed because the saved artifact is stale; regenerate it and revalidate.";
        let diagnosis_sha256 = hash(diagnosis.as_bytes());
        let provenance_sha256 = chat_reply_provenance_sha256(
            project,
            &chat_id,
            &request_id,
            model_target,
            model,
            diagnosis,
        );
        engine
            .chat
            .create_conversation(&chat_id, project, None)
            .unwrap();
        let user_message = json!({
            "sequence":1,
            "role":"user",
            "content":question,
            "attachments":[],
            "request_id":request_id,
            "model_target":model_target,
        });
        let assistant_message = json!({
            "sequence":2,
            "role":"assistant",
            "content":diagnosis,
            "attachments":[],
            "request_id":request_id,
            "model_target":model_target,
            "model":model,
            "content_sha256":diagnosis_sha256,
            "provenance_sha256":provenance_sha256,
        });
        let connection = Connection::open(engine.data.join("developer.sqlite3")).unwrap();
        let transaction = connection.unchecked_transaction().unwrap();
        transaction
            .execute(
                "INSERT INTO developer_chat_request(
                   id,project,chat_id,message,model_target,pending,payload_sha256,reserved_bytes,pre_history_compat)
                 VALUES(?1,?2,?3,?4,?5,0,?6,0,0)",
                rusqlite::params![
                    request_id,
                    project,
                    chat_id,
                    question,
                    model_target,
                    chat_request_payload_sha256(project, &chat_id, question, model_target),
                ],
            )
            .unwrap();
        for (sequence, message) in [
            (1u64, user_message.to_string()),
            (2u64, assistant_message.to_string()),
        ] {
            transaction
                .execute(
                    "INSERT INTO developer_chat_message(chat_id,sequence,message,byte_count)
                     VALUES(?1,?2,?3,?4)",
                    rusqlite::params![chat_id, sequence, message, message.len() as u64],
                )
                .unwrap();
        }
        transaction.commit().unwrap();
        (chat_id, request_id, diagnosis_sha256)
    }

    fn persisted_developer_state(engine: &Engine) -> Value {
        let connection = Connection::open(engine.data.join("developer.sqlite3")).unwrap();
        let encoded: String = connection
            .query_row("SELECT state FROM developer_state WHERE id=1", [], |row| {
                row.get(0)
            })
            .unwrap();
        serde_json::from_str(&encoded).unwrap()
    }

    // Proves the rejection is durably committed exactly once before the route answers
    // with its error, and that no model request can start from the rejected state.
    async fn assert_manual_escalation_preflight_rejection_is_durable(
        engine: &Arc<Engine>,
        mut request: RepairEscalationMutation,
        expected_error: &str,
        expected_lifecycle: &str,
        expected_reason: &str,
        expected_limit: u32,
        before: (u64, u32, u32, u32),
    ) {
        let (revision_before, count_before, escalation_reserved_before, review_reserved_before) =
            before;
        // RepairEscalationMutation is not Clone; rebuild the identical binding from
        // cloned fields so the original stays available for the refreshed retry.
        let first_request = RepairEscalationMutation {
            action: request.action.clone(),
            feature_id: request.feature_id.clone(),
            expected_revision: request.expected_revision,
            expected_checkpoint: request.expected_checkpoint.clone(),
            model_target: request.model_target.clone(),
            chat_id: request.chat_id.clone(),
            chat_request_id: request.chat_request_id.clone(),
            diagnosis_sha256: request.diagnosis_sha256.clone(),
            proposal_id: request.proposal_id.clone(),
        };
        let response = repair_escalation_control(
            State(engine.clone()),
            authorized_headers(),
            Json(first_request),
        )
        .await;
        assert_eq!(response.0, StatusCode::CONFLICT);
        assert_eq!(response.1["error"], expected_error);
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        assert!(!engine.escalation_running.load(Ordering::SeqCst));
        assert!(engine.escalation_cancellation.lock().unwrap().is_none());
        assert!(!engine.repair_loop_authorized.load(Ordering::SeqCst));
        drop(engine.inference_gate.try_acquire().unwrap());
        let persisted = persisted_developer_state(engine);
        assert_eq!(persisted["revision"].as_u64().unwrap(), revision_before + 1);
        let feature = &persisted["queue_v11"][0];
        assert_eq!(feature["auto_repair_lifecycle"], expected_lifecycle);
        assert_eq!(feature["auto_repair_reason"], expected_reason);
        assert_eq!(feature["auto_ai_repair_limit"], expected_limit);
        assert!(feature["auto_repair_step_started_at_ms"].is_null());
        assert_eq!(feature["escalation_count"], count_before);
        assert_eq!(
            feature["escalation_evidence_reserved"],
            escalation_reserved_before
        );
        assert_eq!(feature["review_evidence_reserved"], review_reserved_before);
        assert!(feature["escalation_proposal"].is_null());
        assert_eq!(feature["escalation_pending"], false);
        {
            let database = engine.database.lock().unwrap();
            let feature = &database.state.queue[0];
            assert_eq!(database.state.revision, revision_before + 1);
            assert_eq!(feature.auto_repair_lifecycle, expected_lifecycle);
            assert_eq!(feature.auto_repair_reason, expected_reason);
            assert_eq!(feature.auto_ai_repair_limit, Some(expected_limit));
            assert_eq!(feature.escalation_count, count_before);
            assert_eq!(
                feature.escalation_evidence_reserved,
                escalation_reserved_before
            );
            assert_eq!(feature.review_evidence_reserved, review_reserved_before);
            assert!(feature.escalation_proposal.is_none());
        }
        assert_eq!(
            engine.snapshot().unwrap()["queue"][0]["escalation_status"],
            "idle"
        );
        // Even with a fully refreshed binding the preflight must reject again without
        // ever starting a model request, and commit exactly one more revision.
        request.expected_revision = engine.snapshot().unwrap()["revision"].as_u64().unwrap();
        let response =
            repair_escalation_control(State(engine.clone()), authorized_headers(), Json(request))
                .await;
        assert_eq!(response.0, StatusCode::CONFLICT);
        assert_eq!(response.1["error"], expected_error);
        for _ in 0..8 {
            tokio::task::yield_now().await;
        }
        assert!(!engine.escalation_running.load(Ordering::SeqCst));
        assert!(engine.escalation_cancellation.lock().unwrap().is_none());
        drop(engine.inference_gate.try_acquire().unwrap());
        let persisted = persisted_developer_state(engine);
        assert_eq!(persisted["revision"].as_u64().unwrap(), revision_before + 2);
        assert_eq!(persisted["queue_v11"][0]["escalation_count"], count_before);
        assert!(persisted["queue_v11"][0]["escalation_proposal"].is_null());
    }

    fn prepare_escalation_request(
        revision: u64,
        chat_id: &str,
        request_id: &str,
        diagnosis_sha256: &str,
    ) -> RepairEscalationMutation {
        RepairEscalationMutation {
            action: "prepare".into(),
            feature_id: "a8e78ac7-c9a9-47f0-92dc-b35777880967".into(),
            expected_revision: revision,
            expected_checkpoint: Some("applied".into()),
            model_target: Some("mac".into()),
            chat_id: Some(chat_id.into()),
            chat_request_id: Some(request_id.into()),
            diagnosis_sha256: Some(diagnosis_sha256.into()),
            proposal_id: None,
        }
    }

    #[tokio::test]
    async fn prepare_escalation_route_durably_rejects_when_escalation_count_reaches_snapshot_cap() {
        let (_directory, engine) = control_test_engine();
        engine
            .change(|state| {
                state.auto_ai_repair_enabled = true;
                state.auto_ai_repair_max_escalations = 3;
                state.queue[0].escalation_count = 3;
                Ok(())
            })
            .unwrap();
        let (chat_id, request_id, diagnosis_sha256) =
            install_completed_chat_repair_handoff(&engine);
        let revision_before = engine.snapshot().unwrap()["revision"].as_u64().unwrap();
        assert_manual_escalation_preflight_rejection_is_durable(
            &engine,
            prepare_escalation_request(revision_before, &chat_id, &request_id, &diagnosis_sha256),
            "Repair escalation limit reached",
            "limit_reached",
            "3 of 3 AI escalations used. Automatic repair stopped. Correct the project without AI and revalidate, change the reviewer when applicable, or remove the feature.",
            3,
            (revision_before, 3, 0, 0),
        )
        .await;
    }

    #[tokio::test]
    async fn prepare_escalation_route_durably_rejects_when_escalation_evidence_capacity_is_exhausted(
    ) {
        let (_directory, engine) = control_test_engine();
        engine
            .change(|state| {
                state.auto_ai_repair_enabled = true;
                state.auto_ai_repair_max_escalations = 5;
                let feature = &mut state.queue[0];
                feature.escalation_count = 1;
                // Three more reserved entries would exceed the bounded 300-entry capacity.
                feature.escalation_evidence_reserved = (ESCALATION_HISTORY_LIMIT as u32) - 2;
                Ok(())
            })
            .unwrap();
        let (chat_id, request_id, diagnosis_sha256) =
            install_completed_chat_repair_handoff(&engine);
        let revision_before = engine.snapshot().unwrap()["revision"].as_u64().unwrap();
        assert_manual_escalation_preflight_rejection_is_durable(
            &engine,
            prepare_escalation_request(revision_before, &chat_id, &request_id, &diagnosis_sha256),
            "Repair escalation evidence capacity reached",
            "held",
            "Automatic repair stopped because its bounded escalation evidence capacity is unavailable. Inspect the feature evidence before resuming.",
            5,
            (revision_before, 1, (ESCALATION_HISTORY_LIMIT as u32) - 2, 0),
        )
        .await;
    }

    #[tokio::test]
    async fn prepare_escalation_route_durably_rejects_when_review_evidence_capacity_is_exhausted() {
        let (_directory, engine) = control_test_engine();
        engine
            .change(|state| {
                state.auto_ai_repair_enabled = true;
                state.auto_ai_repair_max_escalations = 5;
                let feature = &mut state.queue[0];
                // One more reserved entry would exceed the bounded 104-entry capacity.
                feature.review_evidence_reserved = REVIEW_HISTORY_LIMIT as u32;
                Ok(())
            })
            .unwrap();
        let (chat_id, request_id, diagnosis_sha256) =
            install_completed_chat_repair_handoff(&engine);
        let revision_before = engine.snapshot().unwrap()["revision"].as_u64().unwrap();
        assert_manual_escalation_preflight_rejection_is_durable(
            &engine,
            prepare_escalation_request(revision_before, &chat_id, &request_id, &diagnosis_sha256),
            "Independent review evidence capacity reached",
            "held",
            "Automatic repair stopped because its bounded independent-review evidence capacity is unavailable. Inspect the feature evidence before resuming.",
            5,
            (revision_before, 0, 0, REVIEW_HISTORY_LIMIT as u32),
        )
        .await;
    }
}
