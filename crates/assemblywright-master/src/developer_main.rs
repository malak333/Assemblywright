//! Owner-selected supervised developer runner. This is not production execution evidence.
mod developer_chat;
mod developer_planning;
mod developer_review;

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
    process::Stdio,
    sync::{
        atomic::{AtomicBool, AtomicU8, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::process::Command;
use uuid::Uuid;

use developer_chat::{
    ChatAttachment, ChatModelConfig, ChatRepairHandoff, DeveloperChat, InferenceGate,
    InferenceLease,
};
use developer_planning::{
    approved_metadata, begin_provider, bind_pending_packet, combined_plan, expected_response,
    finish_unavailable, invalidate_pending, new_session, provider_prompt, record_request,
    request_digest, ApprovedPlanMetadata, PlanningContextFile, PlanningPacket,
    PlanningProviderOutput, PlanningSession, MAX_PLANNING_SESSIONS, PLANNING_OUTPUT_SCHEMA,
    PLANNING_SCHEMA_FILENAME,
};
use developer_review::{
    hex_digest, DeveloperReviewCallError, DeveloperReviewDecisionKind, DeveloperReviewFile,
    DeveloperReviewFinding, DeveloperReviewOutput, DeveloperReviewPacket, DeveloperReviewer,
    MODEL_ID as REVIEW_MODEL_ID, PROVIDER_ID as REVIEW_PROVIDER_ID,
};

const REPAIR_LIMIT: u32 = 3;
const ESCALATION_LIMIT: u32 = 20;
const ESCALATION_HISTORY_LIMIT: usize = 80;

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
struct RepairableValidationFailure(String);

#[derive(Debug, thiserror::Error)]
#[error("{0}")]
struct RepairableReviewRejection(String);

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

#[derive(Clone, Serialize, Deserialize)]
struct RepairEscalationProposal {
    proposal_id: String,
    attempt: u32,
    feature_id: String,
    feature_checkpoint: String,
    binding_revision: u64,
    model_target: String,
    model: String,
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
}

#[derive(Clone, Serialize, Deserialize)]
struct RepairEscalationEvidence {
    proposal_id: String,
    attempt: u32,
    model_target: String,
    model: String,
    chat_request_id: String,
    diagnosis_sha256: String,
    outcome: String,
    proposal_sha256: Option<String>,
    summary: String,
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
    #[serde(default = "default_model_target")]
    model_target: String,
    #[serde(default = "legacy_review_status")]
    review_status: String,
    #[serde(default)]
    review_attempts: u32,
    #[serde(default)]
    review_summary: String,
    #[serde(default)]
    review_pending: Option<ReviewPendingEvidence>,
    #[serde(default)]
    review_history: Vec<ReviewAttemptEvidence>,
    #[serde(default)]
    planning: Option<ApprovedPlanMetadata>,
    #[serde(default)]
    cumulative_evidence_version: u8,
}
#[derive(Clone, Default, Serialize, Deserialize)]
struct Snapshot {
    revision: u64,
    auto_run: bool,
    emergency_paused: bool,
    #[serde(
        rename = "queue_v8",
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
    planning_sessions: Vec<PlanningSession>,
}
struct Database {
    connection: Connection,
    state: Snapshot,
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

struct Engine {
    database: Mutex<Database>,
    running: AtomicBool,
    cancellation: AtomicU8,
    planning_cancellation: Mutex<Option<Arc<AtomicU8>>>,
    planning_running: AtomicBool,
    planning_completion_recovery: Mutex<Option<PlanningCompletionRecovery>>,
    escalation_cancellation: Mutex<Option<Arc<AtomicU8>>>,
    escalation_running: AtomicBool,
    repair_loop_authorized: AtomicBool,
    shutdown: AtomicBool,
    root: PathBuf,
    data: PathBuf,
    token: String,
    model_targets: Vec<ModelTarget>,
    inference_gate: Arc<InferenceGate>,
    chat: Arc<DeveloperChat>,
    reviewer: DeveloperReviewer,
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
        // An emergency cancellation invalidates a provider reply even when the
        // emergency state write itself fails. Recovery may only publish an explicit
        // interrupted result after the owner durably clears the volatile latch.
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
        // Inspect the durable binding before taking the recovery lock. Status snapshots
        // hold the database lock while reading recovery state, so nesting these locks in
        // the opposite order would permit a completion/status deadlock.
        let retryable =
            persisted.is_err() && self.planning_packet_is_pending(packet).unwrap_or(true);
        // Re-sample after the failed transition. Emergency may race the first sample
        // above; a reply rejected by the live latch must never remain cached as Output.
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

    fn change<T>(&self, f: impl FnOnce(&mut Snapshot) -> Result<T>) -> Result<T> {
        self.change_with_commit(f, || {})
    }

    fn change_with_commit<T>(
        &self,
        f: impl FnOnce(&mut Snapshot) -> Result<T>,
        after_commit: impl FnOnce(),
    ) -> Result<T> {
        let mut db = self
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?;
        let mut next = db.state.clone();
        let result = f(&mut next)?;
        next.revision += 1;
        let data = serde_json::to_string(&next)?;
        db.connection.execute("INSERT INTO developer_state(id,state) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET state=excluded.state", [data])?;
        db.state = next;
        after_commit();
        Ok(result)
    }

    fn emergency_paused(&self, state: &Snapshot) -> bool {
        state.emergency_paused || self.cancellation.load(Ordering::SeqCst) == 2
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
        let queue: Vec<Value> = db
            .state
            .queue
            .iter()
            .filter(|f| f.status != "removed")
            .map(|f| json!({
                "id":f.id,"project":f.project,"instruction":f.instruction,"validation":f.validation,
                "status":f.status,"checkpoint":f.checkpoint,"message":f.message,
                "repair_attempts":f.repair_attempts,"model_target":f.model_target,
                "escalation_count":f.escalation_count,
                "escalation_status":f.escalation_proposal.as_ref().map(|proposal| proposal.status.as_str()).unwrap_or("idle"),
                "escalation_active":f.escalation_proposal.as_ref().is_some_and(|proposal| proposal.status == "preparing"),
                "review_status":f.review_status,"review_model":REVIEW_MODEL_ID,
                "review_summary":f.review_summary,"review_attempts":f.review_attempts,
                "planning_status":if f.planning.is_some() { "approved" } else { "legacy_unplanned" },
                "planning":f.planning,
                "changed_files":f.edits.as_ref().map(|e| e.iter().map(|e| &e.path).collect::<Vec<_>>()).unwrap_or_default()
            }))
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
            "stage":session.stage,"revision":session.revision,"running":session.running
        })).collect();
        let repair_active = running && db.state.queue.iter().any(|f| f.repair_pending);
        Ok(
            json!({"mode":"supervised_developer","host":std::env::var("COMPUTERNAME").unwrap_or_else(|_|"local".into()),
            "workspace_root":self.root,"revision":db.state.revision,"auto_run":db.state.auto_run,
            "emergency_paused":self.emergency_paused(&db.state),"running":running,"queue":queue,
            "planning_required":true,"planning_provider":REVIEW_PROVIDER_ID,"planning_model":REVIEW_MODEL_ID,
            "planning_running":planning_running,"planning_sessions":planning_sessions,
            "planning_recovery_pending":self.planning_completion_recovery.lock().is_ok_and(|recovery| recovery.is_some()),
            "repair_limit":REPAIR_LIMIT,"repair_active":repair_active,"model_targets":model_targets,
            "escalation_running":self.escalation_running.load(Ordering::SeqCst),
            "chat_model_selection":true,
            "chat_running":self.chat.is_running(),
            "review_provider":REVIEW_PROVIDER_ID,"review_model":REVIEW_MODEL_ID,"review_required":true}),
        )
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
        let db = self
            .database
            .lock()
            .map_err(|_| anyhow!("state lock failed"))?;
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
        let feature = db
            .state
            .queue
            .iter()
            .find(|feature| feature.status != "succeeded" && feature.status != "removed");
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
            let feature = feature.context("No unfinished feature to start")?;
            if feature.id != expected_feature_id
                || feature.model_target != expected_model_target
                || feature.status != expected_status
                || feature.checkpoint != expected_checkpoint
            {
                bail!("The first queued feature state changed; refresh status before starting");
            }
        }
        if let Some(feature) = feature {
            if escalation_apply_is_quarantined(feature) {
                bail!("Prepare and explicitly approve a new repair proposal after the interrupted application");
            }
            if feature.status == "failed"
                && feature.repair_attempts > 0
                && feature.edits.is_none()
                && !feature.repair_pending
            {
                bail!("Use Repair and retry for another bounded model correction");
            }
        }
        if self.running.load(Ordering::SeqCst) {
            return Ok(());
        }
        let inference_lease = feature
            .map(|_| {
                self.inference_gate
                    .try_acquire()
                    .context("Selected local model feature cannot start")
            })
            .transpose()?;
        if self
            .running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Ok(());
        }
        self.cancellation.store(0, Ordering::SeqCst);
        self.repair_loop_authorized.store(false, Ordering::SeqCst);
        drop(db);
        self.launch(inference_lease);
        Ok(())
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
        if self.shutdown.load(Ordering::SeqCst) {
            bail!("Developer runner is shutting down");
        }
        if self.running.load(Ordering::SeqCst) {
            bail!("Stop the active run before repairing a feature");
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
        let feature = &mut next.queue[first];
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
            if self.emergency_paused(&database.state) {
                bail!("Clear Emergency Pause before preparing a repair proposal");
            }
            database
                .state
                .queue
                .iter()
                .find(|feature| feature.id == request.feature_id)
                .context("Feature not found")?
                .project
                .clone()
        };
        let handoff = self.chat.repair_handoff(&project, chat_request_id)?;
        validate_repair_handoff(&handoff, &project, chat_request_id, diagnosis_sha256)?;

        if self.shutdown.load(Ordering::SeqCst) {
            bail!("Developer runner is shutting down");
        }
        if self.running.load(Ordering::SeqCst)
            || self.planning_running.load(Ordering::SeqCst)
            || self.chat.is_running()
        {
            bail!("Stop active developer work before preparing a repair proposal");
        }
        let cancellation = self.reserve_escalation_call()?;
        let inference_lease = match self.inference_gate.try_acquire() {
            Ok(lease) => Some(lease),
            Err(error) => {
                self.release_escalation_call(&cancellation);
                return Err(error.context("Selected local model repair proposal cannot start"));
            }
        };
        let proposal_id = Uuid::new_v4().to_string();
        let prepared = self.change(|state| {
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
            let feature = &mut state.queue[first];
            if feature.status != "failed" || feature.checkpoint != expected_checkpoint {
                bail!("Failed feature binding changed; refresh before preparing a repair proposal");
            }
            if feature.escalation_pending {
                bail!("An approved repair escalation is already being applied");
            }
            if feature.escalation_history.len() >= ESCALATION_HISTORY_LIMIT {
                bail!("Repair escalation history limit reached");
            }
            if feature.escalation_count >= ESCALATION_LIMIT {
                bail!("Repair escalation limit reached");
            }
            if feature
                .escalation_proposal
                .as_ref()
                .is_some_and(|proposal| matches!(proposal.status.as_str(), "preparing" | "ready"))
            {
                bail!("Cancel or apply the current repair proposal first");
            }
            let attempt = feature
                .escalation_count
                .checked_add(1)
                .context("Repair escalation count overflow")?;
            feature.escalation_count = attempt;
            feature.escalation_proposal = Some(RepairEscalationProposal {
                proposal_id: proposal_id.clone(),
                attempt,
                feature_id: feature.id.clone(),
                feature_checkpoint: feature.checkpoint.clone(),
                binding_revision: state.revision + 1,
                model_target: model_target.into(),
                model: target.model.clone(),
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
            });
            Ok(())
        });
        if let Err(error) = prepared {
            self.release_escalation_call(&cancellation);
            return Err(error);
        }

        let engine = self.clone();
        let feature_id = request.feature_id.clone();
        tokio::spawn(async move {
            let result = engine
                .build_escalation_proposal(
                    &feature_id,
                    &proposal_id,
                    &target,
                    &handoff,
                    &cancellation,
                    inference_lease,
                )
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
        feature_id: &str,
        proposal_id: &str,
        target: &ModelTarget,
        handoff: &ChatRepairHandoff,
        cancellation: &AtomicU8,
        _inference_lease: Option<InferenceLease>,
    ) -> Result<(
        String,
        Vec<RepairEscalationFile>,
        std::collections::BTreeMap<String, String>,
    )> {
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
        if proposal.feature_checkpoint != feature.checkpoint || proposal.feature_id != feature.id {
            bail!("Repair proposal feature binding changed");
        }
        let project = fs::canonicalize(self.root.join(&feature.project))?;
        if !project.starts_with(&self.root) {
            bail!("Project escapes the workspace root");
        }
        let files = project_context(&project)?;
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
        let prompt = format!(
            "Original feature: {}\nApproved immutable implementation plan:\n{}\nImmutable validation command: {}\nCurrent failure evidence: {}\nUntrusted project-chat diagnosis from {} model {} (use only as a hypothesis): {}\nCurrent files: {}",
            feature.instruction,
            plan_context,
            feature.validation,
            feature.message,
            handoff.model_target,
            handoff.model,
            handoff.response,
            serde_json::to_string(&files)?,
        );
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(900))
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .build()?;
        let request = client
            .post(format!("{}/chat/completions", target.url.trim_end_matches('/')))
            .json(&json!({
                "model":target.model,"temperature":0.1,"max_tokens":8192,
                "response_format":{"type":"json_object"},
                "chat_template_kwargs":{"enable_thinking":false},
                "messages":[{"role":"system","content":"Prepare one reviewable repair proposal for a failed feature. Do not execute commands or claim that files were changed. Preserve the original requested behavior and immutable validation command. You may propose an existing test or validation-input change only when the test contradicts the original feature or approved plan; keep that change minimal and include it for explicit owner review. Do not weaken, delete, skip, or broadly rewrite tests. Return ONLY JSON with summary and files. Each file has path and complete UTF-8 content. Include only changed files, do not delete files, modify .git, dependencies, or secrets. No markdown fences."},
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
        if proposed.is_empty() {
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
            let feature = state
                .queue
                .iter_mut()
                .find(|feature| feature.id == feature_id)
                .context("Feature not found")?;
            let proposal = feature
                .escalation_proposal
                .as_mut()
                .filter(|proposal| proposal.proposal_id == proposal_id)
                .context("Repair proposal not found")?;
            if proposal.status != "preparing" {
                return Ok(());
            }
            proposal.binding_revision = state.revision + 1;
            let outcome = if cancellation.load(Ordering::SeqCst) != 0 {
                proposal.status = "cancelled".into();
                proposal.summary = "Repair proposal preparation was cancelled".into();
                proposal.error = None;
                "cancelled"
            } else {
                match result {
                    Ok((summary, files, protected_inputs)) => {
                        proposal.status = "ready".into();
                        proposal.summary = summary;
                        proposal.files = files;
                        proposal.protected_inputs = protected_inputs;
                        proposal.error = None;
                        "ready"
                    }
                    Err(error) => {
                        proposal.status = "unavailable".into();
                        proposal.summary =
                            "The selected local AI could not prepare a repair proposal".into();
                        proposal.error = Some(error.to_string().chars().take(1000).collect());
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
                chat_request_id: proposal.chat_request_id.clone(),
                diagnosis_sha256: proposal.diagnosis_sha256.clone(),
                outcome: outcome.into(),
                proposal_sha256,
                summary: proposal.summary.clone(),
            });
            Ok(())
        })
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
        if self.running.load(Ordering::SeqCst)
            || self.planning_running.load(Ordering::SeqCst)
            || self.chat.is_running()
            || self.escalation_running.load(Ordering::SeqCst)
        {
            bail!("Stop active developer work before applying a repair proposal");
        }
        let (project, chat_request_id) = {
            let database = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            if self.emergency_paused(&database.state) {
                bail!("Clear Emergency Pause before applying a repair proposal");
            }
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
            (feature.project.clone(), proposal.chat_request_id.clone())
        };
        let handoff = self.chat.repair_handoff(&project, &chat_request_id)?;

        self.running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| anyhow!("Another developer run started"))?;
        let inference_lease = match self.inference_gate.try_acquire() {
            Ok(lease) => lease,
            Err(error) => {
                self.running.store(false, Ordering::SeqCst);
                return Err(error.context("Repair proposal application cannot start"));
            }
        };
        self.repair_loop_authorized.store(false, Ordering::SeqCst);
        let applied = self.change(|state| {
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
                chat_request_id: proposal.chat_request_id.clone(),
                diagnosis_sha256: proposal.diagnosis_sha256.clone(),
                outcome: "approved_to_apply".into(),
                proposal_sha256: Some(approved_proposal_sha256),
                summary:
                    "Owner approved these exact repair proposal bytes for one application attempt"
                        .into(),
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
        self.change(|state| {
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
            feature.escalation_history.push(RepairEscalationEvidence {
                proposal_id: proposal.proposal_id.clone(),
                attempt: proposal.attempt,
                model_target: proposal.model_target.clone(),
                model: proposal.model.clone(),
                chat_request_id: proposal.chat_request_id.clone(),
                diagnosis_sha256: proposal.diagnosis_sha256.clone(),
                outcome: "cancelled".into(),
                proposal_sha256: None,
                summary: proposal.summary.clone(),
            });
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
        if self.running.load(Ordering::SeqCst) {
            bail!("Stop the active run before removing a feature");
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
            || self.planning_running.load(Ordering::SeqCst)
            || self.escalation_running.load(Ordering::SeqCst)
        {
            bail!("Stop active feature and chat work before shutting down");
        }
        self.shutdown.store(true, Ordering::SeqCst);
        Ok(())
    }
    async fn run_queue(&self, mut inference_lease: Option<InferenceLease>) -> Result<()> {
        loop {
            let feature = self.change(|s| {
                Ok(s.queue
                    .iter()
                    .find(|f| f.status != "succeeded" && f.status != "removed")
                    .cloned())
            })?;
            let Some(feature) = feature else {
                break;
            };
            if self.cancelled() {
                self.change(|state| {
                    let current = state
                        .queue
                        .iter_mut()
                        .find(|candidate| candidate.id == feature.id)
                        .context("feature missing")?;
                    if current.escalation_pending
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
            let active_inference_lease = match inference_lease.take() {
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
            };
            let result = self.run_feature(&feature).await;
            drop(active_inference_lease);
            if self.cancelled() {
                self.change(|state| {
                    let current = state
                        .queue
                        .iter_mut()
                        .find(|candidate| candidate.id == feature.id)
                        .context("feature missing")?;
                    if current.escalation_pending
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
                    } else {
                        current.status = "paused".into();
                        current.message = "Stopped. Resume continues from the saved checkpoint; applied files are retained.".into();
                    }
                    Ok(())
                })?;
                break;
            }
            if let Err(error) = result {
                let escalation_attempt = feature.escalation_pending;
                let repairable_validation = feature.repair_pending
                    && error
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
                    current.message =
                        if repairable_validation && current.repair_attempts >= REPAIR_LIMIT {
                            format!("Repair stopped after {REPAIR_LIMIT} attempts.\n{latest_error}")
                        } else {
                            latest_error.clone()
                        }
                        .chars()
                        .take(4000)
                        .collect();
                    current.repair_pending = false;
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
                                    "Repair proposal application did not complete: {latest_error}. Inspect the workspace and prepare a new proposal; Resume will not replay remaining edits."
                                ),
                            )?;
                        } else {
                            finish_escalation_application(
                                current,
                                next_revision,
                                "failed",
                                &latest_error,
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
                        Ok(false)
                    }
                })?;
                if reserved_next {
                    continue;
                }
                break;
            }
            self.complete_reviewed_feature(&feature.id)?;
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

    fn complete_reviewed_feature(&self, feature_id: &str) -> Result<()> {
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
            current.status = "failed".into();
            current.review_status = "interrupted".into();
            current.review_summary = "Generated files changed after ChatGPT Codex approval. Resume revalidates the current bytes and starts a fresh review.".into();
            current.checkpoint = "review_binding_changed".into();
            current.message = current.review_summary.clone();
            next.revision = completion_revision;
            let data = serde_json::to_string(&next)?;
            db.connection.execute("INSERT INTO developer_state(id,state) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET state=excluded.state", [data])?;
            db.state = next;
            return Err(error);
        }
        current.status = "succeeded".into();
        current.checkpoint = format!("review_{}_approved", current.review_attempts);
        current.message = "Changes applied, validation passed, and the fixed ChatGPT Codex reviewer approved the exact generated files.".into();
        current.repair_pending = false;
        if current.escalation_pending {
            finish_escalation_application(
                current,
                completion_revision,
                "succeeded",
                "Repair proposal was applied once, validation passed, and ChatGPT Codex approved the exact files",
            )?;
        }
        next.revision = completion_revision;
        let data = serde_json::to_string(&next)?;
        db.connection.execute("INSERT INTO developer_state(id,state) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET state=excluded.state", [data])?;
        db.state = next;
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
                && repair_protected_inputs(&project, &validation_paths)? != protected_repair_inputs
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
                if !seen.insert(path.to_lowercase()) {
                    bail!("Duplicate output file");
                }
                if feature.repair_pending && is_protected_repair_input(path, &validation_paths) {
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
        };
        if !checkpoint_has_applied_edits(&feature.checkpoint) {
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
                edits.clone()
            };
            for edit in &application_edits {
                if self.cancelled() {
                    bail!("Stopped");
                }
                apply_edit(&project, edit)?;
                if feature.escalation_pending {
                    self.change(|state| {
                        let current = state
                            .queue
                            .iter_mut()
                            .find(|candidate| candidate.id == feature.id)
                            .context("feature missing")?;
                        current.edits = Some(merge_review_edits(
                            current.edits.as_deref().unwrap_or_default(),
                            std::slice::from_ref(edit),
                        )?);
                        let proposal = current
                            .escalation_proposal
                            .as_mut()
                            .filter(|proposal| {
                                proposal.status == "approved" || proposal.status == "applying"
                            })
                            .context("Approved repair proposal evidence is missing")?;
                        if !proposal.files.iter().any(|file| file.path == edit.path) {
                            bail!("Applied file is outside the approved repair proposal");
                        }
                        if !proposal.applied_paths.iter().any(|path| path == &edit.path) {
                            proposal.applied_paths.push(edit.path.clone());
                        }
                        proposal.status = "applying".into();
                        proposal.binding_revision = state.revision + 1;
                        current.checkpoint = format!("escalation_{}_applying", proposal.attempt);
                        current.message = format!(
                            "Applied {} of {} explicitly approved repair files",
                            proposal.applied_paths.len(),
                            proposal.files.len()
                        );
                        Ok(())
                    })?;
                }
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
            let current = state
                .queue
                .iter_mut()
                .find(|candidate| candidate.id == feature.id)
                .context("feature missing")?;
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
                    let current = state
                        .queue
                        .iter_mut()
                        .find(|candidate| candidate.id == feature.id)
                        .context("feature missing")?;
                    interrupt_pending_review(current, summary)?;
                    Ok(())
                })?;
                bail!(summary)
            }
        };
        decision.validate_exact(&rebound)?;
        let decision_sha256 = decision.sha256()?;
        let approved = decision.decision == DeveloperReviewDecisionKind::Approved;
        let summary = review_summary(&decision);
        let decision_recorded = self.change(|state| {
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
        let log_path = self.data.join(format!("{}.log", feature.id));
        let log = fs::File::create(&log_path)?;
        #[cfg(windows)]
        let mut command = {
            use std::os::windows::process::CommandExt;
            let mut c = Command::new("cmd.exe");
            c.args(["/d", "/s", "/c"]);
            c.as_std_mut()
                .raw_arg(format!("\"{}\"", feature.validation));
            c
        };
        #[cfg(not(windows))]
        let mut command = {
            let mut c = Command::new("/bin/sh");
            c.args(["-c", &feature.validation]);
            c
        };
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.as_std_mut().process_group(0);
        }
        let mut child = command
            .current_dir(project)
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log)
            .kill_on_drop(true)
            .spawn()?;
        let pid = child.id().context("Validation process has no ID")?;
        let started = std::time::Instant::now();
        loop {
            if self.cancelled()
                || started.elapsed() > Duration::from_secs(900)
                || fs::metadata(&log_path)?.len() > 2 * 1024 * 1024
            {
                terminate_tree(pid, &mut child).await?;
                bail!("Validation stopped or exceeded its time/output limit");
            }
            if let Some(status) = child.try_wait()? {
                if status.success() {
                    let mut evidence = Sha256::new();
                    evidence.update(b"assemblywright.developer-validation.v1\0");
                    evidence.update(feature.id.as_bytes());
                    evidence.update(feature.validation.as_bytes());
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
    feature.review_status = "pending".into();
    feature.review_summary = "Required ChatGPT Codex review has not started".into();
    feature.review_pending = None;
    feature.status = "queued".into();
    feature.checkpoint = format!("repair_{attempt}_reserved");
    feature.message =
        format!("Repair attempt {attempt} of {REPAIR_LIMIT} reserved by owner control");
    feature.edits = None;
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

fn review_checkpoint_requires_revalidation(checkpoint: &str) -> bool {
    checkpoint.starts_with("review_") || checkpoint == "review_binding_changed"
}

fn checkpoint_has_applied_edits(checkpoint: &str) -> bool {
    checkpoint == "applied"
        || checkpoint == "validated"
        || checkpoint.ends_with("_applied")
        || checkpoint.ends_with("_validated")
        || review_checkpoint_requires_revalidation(checkpoint)
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

fn developer_review_packet(
    feature: &Feature,
    project: &Path,
    current_edits: &[Edit],
    validation_evidence_sha256: &str,
) -> Result<DeveloperReviewPacket> {
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
        model_id: REVIEW_MODEL_ID.into(),
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
    if rebound.sha256()? != approved.packet_sha256 {
        bail!("Generated files changed after ChatGPT Codex approval; validation and review must run again");
    }
    Ok(())
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
fn finish_escalation_application(
    feature: &mut Feature,
    binding_revision: u64,
    outcome: &str,
    summary: &str,
) -> Result<()> {
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
                && evidence.outcome == "approved_to_apply"
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
        chat_request_id: proposal.chat_request_id.clone(),
        diagnosis_sha256: proposal.diagnosis_sha256.clone(),
        outcome: outcome.into(),
        proposal_sha256: Some(approved_proposal_sha256),
        summary: summary.chars().take(1000).collect(),
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
    Ok(())
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
fn validate_sha256(value: &str, label: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("Invalid {label}");
    }
    Ok(())
}
fn validate_repair_handoff(
    handoff: &ChatRepairHandoff,
    project: &str,
    request_id: &str,
    diagnosis_sha256: &str,
) -> Result<()> {
    if handoff.project != project
        || handoff.request_id != request_id
        || handoff.response_sha256 != diagnosis_sha256
        || hash(handoff.response.as_bytes()) != handoff.response_sha256
    {
        bail!("Selected chat diagnosis binding changed; refresh project chat");
    }
    Ok(())
}
fn repair_escalation_proposal_sha256(proposal: &RepairEscalationProposal) -> Result<String> {
    let mut canonical = proposal.clone();
    canonical.binding_revision = 0;
    canonical.status.clear();
    canonical.error = None;
    canonical.applied_paths.clear();
    canonical.apply_request_id = None;
    Ok(hash(&serde_json::to_vec(&canonical)?))
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
    let path = checked_path(root, &edit.path)?;
    let current = if path.exists() {
        Some(hash(&fs::read(&path)?))
    } else {
        None
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
    fs::create_dir_all(path.parent().context("No parent directory")?)?;
    fs::write(&path, edit.content.as_bytes())?;
    fs::OpenOptions::new().write(true).open(&path)?.sync_all()?;
    Ok(())
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
fn repair_protected_inputs(
    root: &Path,
    validation_paths: &[String],
) -> Result<std::collections::HashMap<String, String>> {
    fn intersects_validation_path(relative: &str, validation_paths: &[String]) -> bool {
        let normalized = relative.replace('\\', "/").to_lowercase();
        validation_paths.iter().any(|protected| {
            normalized == *protected
                || normalized.starts_with(&format!("{protected}/"))
                || protected.starts_with(&format!("{normalized}/"))
        })
    }
    fn visit(
        root: &Path,
        dir: &Path,
        validation_paths: &[String],
        out: &mut std::collections::HashMap<String, String>,
        bytes_seen: &mut usize,
        files_seen: &mut usize,
        depth: usize,
    ) -> Result<()> {
        if depth > 12 {
            bail!("Protected repair input tree exceeds depth limit");
        }
        let mut entries: Vec<_> = fs::read_dir(dir)?.collect::<std::io::Result<_>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let name = entry.file_name().to_string_lossy().into_owned();
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                continue;
            }
            let relative = entry
                .path()
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");
            if kind.is_dir() {
                let normally_skipped = name.starts_with('.')
                    || ["target", "node_modules", "__pycache__", "venv", "dist"]
                        .contains(&name.as_str());
                if normally_skipped && !intersects_validation_path(&relative, validation_paths) {
                    continue;
                }
                visit(
                    root,
                    &entry.path(),
                    validation_paths,
                    out,
                    bytes_seen,
                    files_seen,
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
            *files_seen += 1;
            if *files_seen > 2000 {
                bail!("Protected repair input count exceeds 2000 files");
            }
            let content = fs::read(entry.path())?;
            *bytes_seen = bytes_seen
                .checked_add(content.len())
                .context("Protected repair input size overflow")?;
            if *bytes_seen > 64 * 1024 * 1024 {
                bail!("Protected repair inputs exceed 64 MiB");
            }
            out.insert(relative.to_lowercase(), hash(&content));
        }
        Ok(())
    }
    let mut out = std::collections::HashMap::new();
    visit(root, root, validation_paths, &mut out, &mut 0, &mut 0, 0)?;
    Ok(out)
}
fn project_context(root: &Path) -> Result<Vec<Value>> {
    fn visit(
        root: &Path,
        dir: &Path,
        out: &mut Vec<Value>,
        budget: &mut usize,
        depth: usize,
    ) -> Result<()> {
        if depth > 6 {
            return Ok(());
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
            {
                continue;
            }
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                visit(root, &entry.path(), out, budget, depth + 1)?;
            } else if kind.is_file() && entry.metadata()?.len() <= 16000 {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    *budget += content.len();
                    out.push(json!({"path":entry.path().strip_prefix(root)?.to_string_lossy().replace('\\',"/"),"content":content}));
                }
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    visit(root, root, &mut out, &mut 0, 0)?;
    Ok(out)
}
async fn terminate_tree(pid: u32, child: &mut tokio::process::Child) -> Result<()> {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill.exe")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .output()
            .await?;
    }
    #[cfg(unix)]
    unsafe {
        libc::kill(-(pid as i32), libc::SIGKILL);
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
    Ok(())
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
    api(engine.snapshot())
}

#[derive(Deserialize)]
struct ChatQuery {
    project: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RepairEscalationMutation {
    action: String,
    feature_id: String,
    expected_revision: u64,
    expected_checkpoint: Option<String>,
    model_target: Option<String>,
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
    api(engine.chat.snapshot(&query.project))
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
        let project = request["project"]
            .as_str()
            .context("Missing chat project")?;
        let message = request["message"]
            .as_str()
            .context("Missing chat message")?;
        let id = request["id"].as_str().context("Missing chat request ID")?;
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
        engine.chat.cancel(id)
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
        let digest = request_digest(&raw)?;
        let request: PlanningMutation = serde_json::from_value(raw)?;
        developer_planning::validate_identifier(&request.feature_id)?;
        developer_planning::validate_identifier(&request.request_id)?;

        {
            let database = self
                .database
                .lock()
                .map_err(|_| anyhow!("state lock failed"))?;
            if self.emergency_paused(&database.state) {
                bail!("Clear Emergency Pause before brainstorming");
            }
        }

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
        if let Some(existing) = existing {
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
            if self.shutdown.load(Ordering::SeqCst) {
                bail!("Developer runner is shutting down");
            }
            if self.running.load(Ordering::SeqCst) || self.chat.is_running() {
                bail!("Stop active developer work before brainstorming");
            }
            if self.escalation_running.load(Ordering::SeqCst) {
                bail!("Wait for the repair proposal to finish before brainstorming");
            }
            Some(self.reserve_planning_call()?)
        } else {
            if request.action == "approve_and_enqueue"
                && (self.running.load(Ordering::SeqCst)
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
            self.change(|state| {
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
                    )?);
                }
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
                            model_target: session.model_target.clone(),
                            review_status: "pending".into(),
                            review_attempts: 0,
                            review_summary: "Required ChatGPT Codex review has not started".into(),
                            review_pending: None,
                            review_history: Vec::new(),
                            planning: Some(metadata),
                            cumulative_evidence_version: 1,
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
                let result = match provider_prompt(&packet) {
                    Ok(prompt) => engine
                        .reviewer
                        .call_tool_free(
                            &prompt,
                            PLANNING_SCHEMA_FILENAME,
                            PLANNING_OUTPUT_SCHEMA,
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
        match request["action"].as_str().context("Missing action")? {
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
                engine.change(|s| {
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
                engine.repair_loop_authorized.store(false, Ordering::SeqCst);
                engine
                    .cancellation
                    .fetch_max(if emergency { 2 } else { 1 }, Ordering::SeqCst);
                engine.cancel_planning_call(emergency);
                engine.cancel_escalation_call(emergency);
                if emergency {
                    engine.chat.cancel_for_emergency();
                }
                engine.change(|s| {
                    if emergency {
                        s.emergency_paused = true;
                    }
                    for session in &mut s.planning_sessions {
                        invalidate_pending(session, if emergency {
                            "Emergency Pause interrupted the planning request; retry after clearing the pause."
                        } else {
                            "Stop interrupted the planning request; retry when ready."
                        })?;
                    }
                    Ok(())
                })?;
                if emergency {
                    if let Ok(mut recovery) = engine.planning_completion_recovery.lock() {
                        *recovery = None;
                    }
                }
            }
            "shutdown" => {
                engine.begin_shutdown()?;
            }
            "clear_emergency" => {
                if engine.running.load(Ordering::SeqCst)
                    || engine.planning_running.load(Ordering::SeqCst)
                    || engine.chat.is_running()
                    || engine.escalation_running.load(Ordering::SeqCst)
                {
                    bail!("Wait for active developer work to stop");
                }
                engine.change_with_commit(
                    |s| {
                        s.emergency_paused = false;
                        Ok(())
                    },
                    || engine.cancellation.store(0, Ordering::SeqCst),
                )?;
            }
            "auto_run" => {
                let enabled = request["enabled"]
                    .as_bool()
                    .context("Missing enabled value")?;
                engine.change(|s| {
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
    connection.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; CREATE TABLE IF NOT EXISTS developer_state(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL); CREATE TABLE IF NOT EXISTS developer_state_v1_backup(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL); CREATE TABLE IF NOT EXISTS developer_state_v2_backup(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL); CREATE TABLE IF NOT EXISTS developer_state_v3_backup(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL); CREATE TABLE IF NOT EXISTS developer_state_v4_backup(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL); CREATE TABLE IF NOT EXISTS developer_state_v5_backup(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL); CREATE TABLE IF NOT EXISTS developer_state_v6_backup(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL); CREATE TABLE IF NOT EXISTS developer_state_v7_backup(id INTEGER PRIMARY KEY CHECK(id=1),state TEXT NOT NULL);")?;
    let (mut state, loaded_queue_version): (Snapshot, u8) =
        match connection.query_row("SELECT state FROM developer_state WHERE id=1", [], |r| {
            r.get::<_, String>(0)
        }) {
            Ok(data) => {
                let value: Value = serde_json::from_str(&data)?;
                let loaded_queue_version = if value.get("queue_v8").is_some() {
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
                (serde_json::from_value(value)?, loaded_queue_version)
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => (
                Snapshot {
                    auto_run: true,
                    ..Default::default()
                },
                8,
            ),
            Err(e) => return Err(e.into()),
        };
    let root = fs::canonicalize(&args.workspace_root)?;
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
    for feature in &mut state.queue {
        if !matches!(feature.model_target.as_str(), "mac" | "windows") {
            bail!("Persisted feature has an unknown model target");
        }
        if feature
            .escalation_proposal
            .as_ref()
            .is_some_and(|proposal| proposal.status == "preparing")
        {
            let proposal = feature.escalation_proposal.as_mut().unwrap();
            proposal.status = "interrupted".into();
            proposal.summary = "Runner restarted before the selected local AI finished the repair proposal; prepare a new proposal".into();
            proposal.error = None;
            proposal.binding_revision = recovery_revision;
            feature.escalation_history.push(RepairEscalationEvidence {
                proposal_id: proposal.proposal_id.clone(),
                attempt: proposal.attempt,
                model_target: proposal.model_target.clone(),
                model: proposal.model.clone(),
                chat_request_id: proposal.chat_request_id.clone(),
                diagnosis_sha256: proposal.diagnosis_sha256.clone(),
                outcome: "interrupted".into(),
                proposal_sha256: None,
                summary: proposal.summary.clone(),
            });
        }
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
        } else if feature.status == "running" {
            feature.status = "paused".into();
            if feature.review_pending.is_some() {
                interrupt_pending_review(
                    feature,
                    "Runner restarted during ChatGPT Codex review. Resume revalidates the generated files and starts a fresh review attempt.",
                )?;
            }
            feature.message =
                "Runner restarted. Review the workspace, then Resume from the saved checkpoint."
                    .into();
        }
        if feature.review_status == "legacy_unreviewed"
            && !matches!(feature.status.as_str(), "succeeded" | "removed")
        {
            feature.review_status = "pending".into();
            feature.review_summary = "Required ChatGPT Codex review has not started".into();
        }
    }
    for session in &mut state.planning_sessions {
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
    let inference_gate = InferenceGate::new();
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
    )?;
    let engine = Arc::new(Engine {
        database: Mutex::new(Database { connection, state }),
        running: AtomicBool::new(false),
        cancellation: AtomicU8::new(0),
        planning_cancellation: Mutex::new(None),
        planning_running: AtomicBool::new(false),
        planning_completion_recovery: Mutex::new(None),
        escalation_cancellation: Mutex::new(None),
        escalation_running: AtomicBool::new(false),
        repair_loop_authorized: AtomicBool::new(false),
        shutdown: AtomicBool::new(false),
        root,
        data: args.data_dir,
        token,
        model_targets,
        inference_gate,
        chat,
        reviewer,
    });
    engine.change(|_| Ok(()))?;
    let app = Router::new()
        .route("/status", get(status))
        .route("/control", post(control))
        .route(
            "/chat",
            get(chat_status)
                .post(chat_start)
                .layer(DefaultBodyLimit::max(9 * 1024 * 1024)),
        )
        .route("/chat/projects", get(chat_projects))
        .route("/chat/cancel", post(chat_cancel))
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
        let inference_gate = InferenceGate::new();
        let model_targets = vec![ModelTarget {
            id: "mac",
            name: "Mac local AI",
            url: "http://127.0.0.1:1/v1".into(),
            model: "fixture".into(),
        }];
        let chat = DeveloperChat::open(
            &database_path,
            fs::canonicalize(&workspace).unwrap(),
            vec![ChatModelConfig {
                target: "mac".into(),
                url: "http://127.0.0.1:1/v1".into(),
                model: "fixture".into(),
            }],
            inference_gate.clone(),
        )
        .unwrap();
        let reviewer = DeveloperReviewer::new(codex_executable, codex_home, &data).unwrap();
        let engine = Arc::new(Engine {
            database: Mutex::new(Database {
                connection,
                state: Snapshot {
                    auto_run: true,
                    queue: vec![feature_with_status("failed")],
                    ..Default::default()
                },
            }),
            running: AtomicBool::new(false),
            cancellation: AtomicU8::new(0),
            planning_cancellation: Mutex::new(None),
            planning_running: AtomicBool::new(false),
            planning_completion_recovery: Mutex::new(None),
            escalation_cancellation: Mutex::new(None),
            escalation_running: AtomicBool::new(false),
            repair_loop_authorized: AtomicBool::new(false),
            shutdown: AtomicBool::new(false),
            root: fs::canonicalize(workspace).unwrap(),
            data,
            token: "test-token".into(),
            model_targets,
            inference_gate,
            chat,
            reviewer,
        });
        engine.change(|_| Ok(())).unwrap();
        (dir, engine)
    }

    fn authorized_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", "Bearer test-token".parse().unwrap());
        headers
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
            model_target: "mac".into(),
            review_status: "pending".into(),
            review_attempts: 0,
            review_summary: String::new(),
            review_pending: None,
            review_history: Vec::new(),
            planning: None,
            cumulative_evidence_version: 1,
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
        });
        let proposal = feature.escalation_proposal.as_ref().unwrap();
        feature.escalation_history.push(RepairEscalationEvidence {
            proposal_id: "a6ea29b7-8800-4506-986f-448005687215".into(),
            attempt: 0,
            model_target: "mac".into(),
            model: "older-model".into(),
            chat_request_id: "418ab506-8295-4b4c-90c7-30f8dd7dcedd".into(),
            diagnosis_sha256: "0".repeat(64),
            outcome: "approved_to_apply".into(),
            proposal_sha256: Some("0".repeat(64)),
            summary: "older approval".into(),
        });
        feature.escalation_history.push(RepairEscalationEvidence {
            proposal_id: proposal.proposal_id.clone(),
            attempt: proposal.attempt,
            model_target: proposal.model_target.clone(),
            model: proposal.model.clone(),
            chat_request_id: proposal.chat_request_id.clone(),
            diagnosis_sha256: proposal.diagnosis_sha256.clone(),
            outcome: "approved_to_apply".into(),
            proposal_sha256: Some(repair_escalation_proposal_sha256(proposal).unwrap()),
            summary: "approved".into(),
        });
        feature
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
    fn queue_v8_reads_legacy_state_defaults_and_fails_closed_for_old_parsers() {
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
        assert!(current_value.get("queue_v8").is_some());
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
        };
        assert!(configured_model_targets(&duplicate).is_err());
        let ipv4 = reqwest::Url::parse("http://127.0.0.1:8080/v1").unwrap();
        let ipv6_alias = reqwest::Url::parse("http://[::1]:8080/other").unwrap();
        let distinct_port = reqwest::Url::parse("http://127.0.0.1:8081/v1").unwrap();
        assert!(same_loopback_listener(&ipv4, &ipv6_alias));
        assert!(!same_loopback_listener(&ipv4, &distinct_port));
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
}
