use assemblywright_protocol::{
    build_local_coding_patch_artifact, local_coding_admission_sha256, local_coding_paths_sha256,
    CancellationAcknowledgement, CancellationAcknowledgementStatus, CancellationInstruction,
    JobEnvelope, JobResultEnvelope, JobResultStatus, LocalCodingAgentCompletion,
    LocalCodingEditOperation, LocalCodingJobResult, LocalCodingResultArtifact,
    LocalCodingSnapshotChunk, LocalCodingSnapshotChunkRequest, ProtocolError,
    LOCAL_CODING_COMPLETED_STATUS, LOCAL_CODING_FIXTURE_TEST_STATUS,
    MAX_LOCAL_CODING_SNAPSHOT_BUNDLE_BYTES, PROTOCOL_VERSION,
};
use git2::{ObjectType, Oid, Repository, TreeWalkMode, TreeWalkResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::ffi::{CString, OsStr};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
#[cfg(test)]
use std::os::unix::process::CommandExt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
#[cfg(test)]
use std::thread;
use std::time::Duration;
use std::time::Instant;

const BUNDLE_MAGIC: &[u8] = b"AW-SNAPSHOT-BUNDLE-V1\n";
const BUNDLE_END_MAGIC: &[u8] = b"AW-SNAPSHOT-END-V1\n";
const MAX_OBJECTS: usize = 50_000;
const MAX_OBJECT_BYTES: u64 = 256 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_PATH_BYTES: usize = 1024;
const RETAINED_WORKSPACE_TTL_MS: u64 = 60 * 60 * 1000;
const MAX_RETENTION_RECORD_BYTES: usize = 32 * 1024;
const RETENTION_RECORD_VERSION: u16 = 1;
#[cfg(not(test))]
const PARENT_VERIFICATION_WAIT_TIMEOUT: Duration = Duration::from_secs(7);
#[cfg(test)]
const PARENT_VERIFICATION_WAIT_TIMEOUT: Duration = Duration::from_millis(200);

#[derive(Debug, thiserror::Error)]
pub enum LocalCodingSnapshotError {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("local coding snapshot materialization is disabled")]
    Disabled,
    #[error("another local coding snapshot is active")]
    AlreadyActive,
    #[error("no matching local coding snapshot is active")]
    NotActive,
    #[error("local coding snapshot filesystem operation failed")]
    Io(#[from] std::io::Error),
    #[error("local coding snapshot object operation failed")]
    Git(#[from] git2::Error),
    #[error("local coding snapshot bundle was rejected")]
    Rejected,
    #[error("local coding snapshot runtime lock was poisoned")]
    Unavailable,
    #[error("local coding snapshot verification cleanup timed out")]
    VerificationTimeout,
    #[error("local coding snapshot process-group effects remain possible")]
    EffectPossible,
}

#[derive(Debug)]
pub enum LocalCodingSnapshotAcceptance {
    Continue { next_offset: u64 },
    Complete(Box<LocalCodingAgentCompletion>),
}

#[derive(Debug)]
struct ActiveMaterialization {
    job: JobEnvelope,
    staging_directory: PathBuf,
    bundle_file: File,
    expected_offset: u64,
    total_bytes: Option<u64>,
    attempt_deadline: Instant,
    verifying: bool,
    effect_possible: bool,
    terminal_failure: bool,
    cancellation_requested: Arc<AtomicBool>,
}

#[derive(Debug)]
struct ContainedCodingEvidence {
    changed_paths_sha256: [u8; 32],
    workspace_tree_sha256: [u8; 32],
    artifact_bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileEvidence {
    mode: u32,
    size: u64,
    sha256: [u8; 32],
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RetainedWorkspaceRecord {
    record_version: u16,
    job: JobEnvelope,
    sealed_workspace_name: String,
    workspace_tree_sha256: [u8; 32],
    expires_at_ms: u64,
    binding_sha256: [u8; 32],
}

#[derive(Debug)]
pub struct LocalCodingSnapshotRuntime {
    enabled: bool,
    root: PathBuf,
    active: Mutex<Option<ActiveMaterialization>>,
    completed: Mutex<Option<JobEnvelope>>,
    state_changed: Condvar,
}

impl LocalCodingSnapshotRuntime {
    pub fn open(data_dir: &Path, enabled: bool) -> Result<Self, LocalCodingSnapshotError> {
        let root = data_dir.join("local-coding-snapshots");
        ensure_private_directory(&root)?;
        let completed = recover_retained_workspace(&root)?;
        Ok(Self {
            enabled,
            root,
            active: Mutex::new(None),
            completed: Mutex::new(completed),
            state_changed: Condvar::new(),
        })
    }

    pub fn admit(&self, job: JobEnvelope) -> Result<u64, LocalCodingSnapshotError> {
        if !self.enabled {
            return Err(LocalCodingSnapshotError::Disabled);
        }
        job.validate_local_coding()?;
        let mut active = self
            .active
            .lock()
            .map_err(|_| LocalCodingSnapshotError::Unavailable)?;
        if active.is_some() {
            return Err(LocalCodingSnapshotError::AlreadyActive);
        }
        if self
            .completed
            .lock()
            .map_err(|_| LocalCodingSnapshotError::Unavailable)?
            .is_some()
        {
            return Err(LocalCodingSnapshotError::AlreadyActive);
        }
        let staging_directory = self.root.join(job.attempt_id.0.to_string());
        fs::create_dir(&staging_directory)?;
        fs::set_permissions(&staging_directory, fs::Permissions::from_mode(0o700))?;
        let bundle_path = staging_directory.join("snapshot.bundle.partial");
        let mut options = OpenOptions::new();
        options
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
        let bundle_file = options.open(&bundle_path)?;
        let attempt_deadline = Instant::now()
            .checked_add(Duration::from_millis(
                job.lease_duration_ms.min(job.deadline_after_ms),
            ))
            .ok_or(LocalCodingSnapshotError::Rejected)?;
        *active = Some(ActiveMaterialization {
            job,
            staging_directory,
            bundle_file,
            expected_offset: 0,
            total_bytes: None,
            attempt_deadline,
            verifying: false,
            effect_possible: false,
            terminal_failure: false,
            cancellation_requested: Arc::new(AtomicBool::new(false)),
        });
        Ok(0)
    }

    pub fn accept_chunk(
        &self,
        chunk: LocalCodingSnapshotChunk,
    ) -> Result<LocalCodingSnapshotAcceptance, LocalCodingSnapshotError> {
        let mut guard = self
            .active
            .lock()
            .map_err(|_| LocalCodingSnapshotError::Unavailable)?;
        let active = guard.as_mut().ok_or(LocalCodingSnapshotError::NotActive)?;
        if active.effect_possible || active.terminal_failure {
            return Err(LocalCodingSnapshotError::EffectPossible);
        }
        if Instant::now() >= active.attempt_deadline {
            cleanup_attempt_state(&self.root, active)?;
            guard.take();
            return Err(LocalCodingSnapshotError::Rejected);
        }
        if active.verifying || active.cancellation_requested.load(Ordering::Acquire) {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        let content = (|| -> Result<Vec<u8>, LocalCodingSnapshotError> {
            let request = request_for_job(&active.job, active.expected_offset)?;
            chunk.validate_for_request(&request)?;
            if active.expected_offset != chunk.offset
                || active
                    .total_bytes
                    .is_some_and(|total| total != chunk.total_bytes)
            {
                return Err(LocalCodingSnapshotError::Rejected);
            }
            let content = chunk.decode_content()?;
            let metadata = active.bundle_file.metadata()?;
            if !metadata.is_file()
                || metadata.nlink() != 1
                || metadata.uid() != unsafe { libc::geteuid() }
                || metadata.len() != active.expected_offset
            {
                return Err(LocalCodingSnapshotError::Rejected);
            }
            Ok(content)
        })();
        let content = match content {
            Ok(content) => content,
            Err(error) => {
                cleanup_attempt_state(&self.root, active)?;
                guard.take();
                return Err(error);
            }
        };
        let active = guard.as_mut().ok_or(LocalCodingSnapshotError::NotActive)?;
        let write_result = (|| -> Result<u64, LocalCodingSnapshotError> {
            active.total_bytes.get_or_insert(chunk.total_bytes);
            active.bundle_file.write_all(&content)?;
            active.expected_offset = active
                .expected_offset
                .checked_add(content.len() as u64)
                .ok_or(LocalCodingSnapshotError::Rejected)?;
            if chunk.complete {
                active.bundle_file.sync_all()?;
                if active.total_bytes != Some(active.expected_offset) {
                    return Err(LocalCodingSnapshotError::Rejected);
                }
            }
            Ok(active.expected_offset)
        })();
        let next_offset = match write_result {
            Ok(next_offset) => next_offset,
            Err(error) => {
                cleanup_attempt_state(&self.root, active)?;
                guard.take();
                return Err(error);
            }
        };
        if !chunk.complete {
            return Ok(LocalCodingSnapshotAcceptance::Continue { next_offset });
        }
        let bundle_file = active.bundle_file.try_clone();
        let bundle_file = match bundle_file {
            Ok(bundle_file) => bundle_file,
            Err(error) => {
                cleanup_attempt_state(&self.root, active)?;
                guard.take();
                return Err(error.into());
            }
        };
        active.verifying = true;
        let verification = ActiveMaterialization {
            job: active.job.clone(),
            staging_directory: active.staging_directory.clone(),
            bundle_file,
            expected_offset: active.expected_offset,
            total_bytes: active.total_bytes,
            attempt_deadline: active.attempt_deadline,
            verifying: true,
            effect_possible: active.effect_possible,
            terminal_failure: active.terminal_failure,
            cancellation_requested: Arc::clone(&active.cancellation_requested),
        };
        drop(guard);
        let verification_result = materialize_verify_and_execute(&self.root, &verification);
        if matches!(
            &verification_result,
            Err(LocalCodingSnapshotError::EffectPossible)
        ) {
            let mut guard = self
                .active
                .lock()
                .map_err(|_| LocalCodingSnapshotError::Unavailable)?;
            let active = guard
                .as_mut()
                .filter(|active| {
                    active.job.attempt_id == verification.job.attempt_id && active.verifying
                })
                .ok_or(LocalCodingSnapshotError::EffectPossible)?;
            active.verifying = false;
            active.effect_possible = true;
            active.terminal_failure = true;
            self.state_changed.notify_all();
            return Err(LocalCodingSnapshotError::EffectPossible);
        }
        let mut guard = self
            .active
            .lock()
            .map_err(|_| LocalCodingSnapshotError::Unavailable)?;
        let active = guard.as_ref().ok_or(LocalCodingSnapshotError::NotActive)?;
        if active.job.attempt_id != verification.job.attempt_id || !active.verifying {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        if active.cancellation_requested.load(Ordering::Acquire) {
            cleanup_attempt_state(&self.root, &verification)?;
            guard.take();
            self.state_changed.notify_all();
            return Err(LocalCodingSnapshotError::Rejected);
        }
        let evidence = match verification_result {
            Ok(evidence) => evidence,
            Err(error) => {
                if let Err(cleanup_error) = cleanup_attempt_state(&self.root, &verification) {
                    if let Some(active) = guard.as_mut() {
                        active.verifying = false;
                        active.terminal_failure = true;
                    }
                    self.state_changed.notify_all();
                    return Err(cleanup_error);
                }
                guard.take();
                self.state_changed.notify_all();
                return Err(error);
            }
        };
        let workspace_expires_at_ms = current_time_ms()?
            .checked_add(RETAINED_WORKSPACE_TTL_MS)
            .ok_or(LocalCodingSnapshotError::Rejected)?;
        let result = match build_result(&verification.job, &evidence, workspace_expires_at_ms) {
            Ok(result) => result,
            Err(error) => {
                guard.take();
                self.state_changed.notify_all();
                return Err(error);
            }
        };
        let retention_result = (|| -> Result<(), LocalCodingSnapshotError> {
            let sealed_workspace_name = sealed_workspace_name(&verification.job);
            let retained = self.root.join(&sealed_workspace_name);
            fs::rename(
                self.root
                    .join(format!("{}.materialized", verification.job.attempt_id.0)),
                &retained,
            )?;
            write_retention_record(
                &self.root,
                RetainedWorkspaceRecord::new(
                    verification.job.clone(),
                    sealed_workspace_name,
                    evidence.workspace_tree_sha256,
                    workspace_expires_at_ms,
                )?,
            )?;
            cleanup_tree_if_exists(&verification.staging_directory)
        })();
        if let Err(error) = retention_result {
            if let Some(active) = guard.as_mut() {
                active.verifying = false;
                active.terminal_failure = true;
            }
            self.state_changed.notify_all();
            return Err(error);
        }
        *self
            .completed
            .lock()
            .map_err(|_| LocalCodingSnapshotError::Unavailable)? = Some(verification.job);
        guard.take();
        self.state_changed.notify_all();
        Ok(LocalCodingSnapshotAcceptance::Complete(Box::new(result)))
    }

    pub fn cancel(
        &self,
        instruction: &CancellationInstruction,
    ) -> Result<CancellationAcknowledgement, LocalCodingSnapshotError> {
        let mut guard = self
            .active
            .lock()
            .map_err(|_| LocalCodingSnapshotError::Unavailable)?;
        if let Some(active) = guard.as_mut() {
            instruction.validate_for_job(&active.job)?;
            if active.effect_possible {
                return Err(LocalCodingSnapshotError::EffectPossible);
            }
            if active.terminal_failure {
                cleanup_attempt_state(&self.root, active)?;
                guard.take();
                return cancellation_acknowledgement(instruction);
            }
            if active.verifying {
                let attempt_id = active.job.attempt_id;
                active.cancellation_requested.store(true, Ordering::Release);
                let wait_deadline = Instant::now() + PARENT_VERIFICATION_WAIT_TIMEOUT;
                while guard
                    .as_ref()
                    .is_some_and(|active| active.job.attempt_id == attempt_id && active.verifying)
                {
                    let remaining = wait_deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        return Err(LocalCodingSnapshotError::VerificationTimeout);
                    }
                    let (next, timeout) = self
                        .state_changed
                        .wait_timeout(guard, remaining)
                        .map_err(|_| LocalCodingSnapshotError::Unavailable)?;
                    guard = next;
                    if timeout.timed_out()
                        && guard.as_ref().is_some_and(|active| {
                            active.job.attempt_id == attempt_id && active.verifying
                        })
                    {
                        return Err(LocalCodingSnapshotError::VerificationTimeout);
                    }
                }
                if guard.as_ref().is_some_and(|active| {
                    active.job.attempt_id == attempt_id
                        && (active.effect_possible || active.terminal_failure)
                }) {
                    return Err(LocalCodingSnapshotError::EffectPossible);
                }
                if let Some(active) = guard
                    .as_ref()
                    .filter(|active| active.job.attempt_id == attempt_id)
                {
                    cleanup_attempt_state(&self.root, active)?;
                    guard.take();
                }
                return cancellation_acknowledgement(instruction);
            }
            cleanup_attempt_state(&self.root, active)?;
            guard.take();
            return cancellation_acknowledgement(instruction);
        }
        drop(guard);
        let mut completed = self
            .completed
            .lock()
            .map_err(|_| LocalCodingSnapshotError::Unavailable)?;
        let job = completed
            .as_ref()
            .ok_or(LocalCodingSnapshotError::NotActive)?;
        instruction.validate_for_job(job)?;
        cleanup_retained_workspace(&self.root, job)?;
        completed.take();
        cancellation_acknowledgement(instruction)
    }

    pub fn shutdown(&self) -> Result<(), LocalCodingSnapshotError> {
        let mut guard = self
            .active
            .lock()
            .map_err(|_| LocalCodingSnapshotError::Unavailable)?;
        if guard.as_ref().is_some_and(|active| active.effect_possible) {
            return Err(LocalCodingSnapshotError::EffectPossible);
        }
        if guard.as_ref().is_some_and(|active| active.verifying) {
            if let Some(active) = guard.as_ref() {
                active.cancellation_requested.store(true, Ordering::Release);
            }
            let wait_deadline = Instant::now() + PARENT_VERIFICATION_WAIT_TIMEOUT;
            while guard.as_ref().is_some_and(|active| active.verifying) {
                let remaining = wait_deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(LocalCodingSnapshotError::VerificationTimeout);
                }
                let (next, timeout) = self
                    .state_changed
                    .wait_timeout(guard, remaining)
                    .map_err(|_| LocalCodingSnapshotError::Unavailable)?;
                guard = next;
                if timeout.timed_out() && guard.as_ref().is_some_and(|active| active.verifying) {
                    return Err(LocalCodingSnapshotError::VerificationTimeout);
                }
            }
            if guard.as_ref().is_some_and(|active| active.effect_possible) {
                return Err(LocalCodingSnapshotError::EffectPossible);
            }
        }
        if let Some(active) = guard.as_ref() {
            cleanup_attempt_state(&self.root, active)?;
            guard.take();
        }
        Ok(())
    }
}

fn current_time_ms() -> Result<u64, LocalCodingSnapshotError> {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| LocalCodingSnapshotError::Rejected)?
        .as_millis()
        .try_into()
        .map_err(|_| LocalCodingSnapshotError::Rejected)
}

fn cancellation_acknowledgement(
    instruction: &CancellationInstruction,
) -> Result<CancellationAcknowledgement, LocalCodingSnapshotError> {
    Ok(CancellationAcknowledgement {
        protocol_version: PROTOCOL_VERSION,
        connection_epoch: instruction.connection_epoch,
        sequence: instruction
            .sequence
            .checked_add(1)
            .ok_or(LocalCodingSnapshotError::Rejected)?,
        task_id: instruction.task_id,
        step_id: instruction.step_id,
        attempt_id: instruction.attempt_id,
        lease_id: instruction.lease_id,
        cancellation_id: instruction.cancellation_id,
        status: CancellationAcknowledgementStatus::Cancelled,
    })
}

fn request_for_job(
    job: &JobEnvelope,
    offset: u64,
) -> Result<LocalCodingSnapshotChunkRequest, LocalCodingSnapshotError> {
    let context = job.validate_local_coding()?;
    Ok(LocalCodingSnapshotChunkRequest {
        protocol_version: job.protocol_version,
        connection_epoch: job.connection_epoch,
        task_id: job.task_id,
        step_id: job.step_id,
        attempt_id: job.attempt_id,
        lease_id: job.lease_id,
        cancellation_id: job.cancellation_id,
        snapshot_id: context.snapshot_id,
        snapshot_sha256: context.snapshot_sha256,
        offset,
    })
}

fn build_result(
    job: &JobEnvelope,
    evidence: &ContainedCodingEvidence,
    workspace_expires_at_ms: u64,
) -> Result<LocalCodingAgentCompletion, LocalCodingSnapshotError> {
    let context = job.validate_local_coding()?;
    let artifact =
        LocalCodingResultArtifact::from_bytes(uuid::Uuid::new_v4(), &evidence.artifact_bytes)?;
    let payload = serde_json::to_value(LocalCodingJobResult {
        status: LOCAL_CODING_COMPLETED_STATUS.to_string(),
        work_packet_sha256: context.work_packet_sha256,
        admission_sha256: local_coding_admission_sha256(job),
        snapshot_sha256: context.snapshot_sha256,
        allowed_paths_sha256: context.work_packet.allowed_paths_sha256()?,
        changed_paths_sha256: evidence.changed_paths_sha256,
        patch_sha256: artifact.artifact_sha256,
        artifact_id: artifact.artifact_id,
        artifact_sha256: artifact.artifact_sha256,
        artifact_size_bytes: artifact.artifact_size_bytes,
        changed_file_count: u16::try_from(context.work_packet.allowed_paths.len())
            .map_err(|_| LocalCodingSnapshotError::Rejected)?,
        test_status: LOCAL_CODING_FIXTURE_TEST_STATUS.to_string(),
        mutation_performed: true,
        workspace_retained: true,
        workspace_expires_at_ms,
        ambiguous: false,
    })
    .map_err(|_| LocalCodingSnapshotError::Rejected)?;
    let payload_sha256 = json_sha256(&payload)?;
    let result = JobResultEnvelope {
        protocol_version: job.protocol_version,
        connection_epoch: job.connection_epoch,
        sequence: job
            .sequence
            .checked_add(1)
            .ok_or(LocalCodingSnapshotError::Rejected)?,
        task_id: job.task_id,
        step_id: job.step_id,
        attempt_id: job.attempt_id,
        lease_id: job.lease_id,
        cancellation_id: job.cancellation_id,
        status: JobResultStatus::Completed,
        context_sha256: job.context_sha256,
        payload_sha256,
        payload,
    };
    let completion = LocalCodingAgentCompletion { result, artifact };
    completion.validate_for_job(job)?;
    Ok(completion)
}

fn materialize_verify_and_execute(
    root: &Path,
    active: &ActiveMaterialization,
) -> Result<ContainedCodingEvidence, LocalCodingSnapshotError> {
    reject_if_stopped(&active.cancellation_requested, active.attempt_deadline)?;
    let context = active.job.validate_local_coding()?;
    let materialized = root.join(format!("{}.materialized", active.job.attempt_id.0));
    fs::create_dir(&materialized)?;
    fs::set_permissions(&materialized, fs::Permissions::from_mode(0o700))?;
    let mut bundle_file = active.bundle_file.try_clone()?;
    bundle_file.seek(SeekFrom::Start(0))?;
    parse_bundle(
        &mut bundle_file,
        &materialized,
        context.snapshot_sha256,
        &active.cancellation_requested,
        active.attempt_deadline,
    )?;
    run_deterministic_edit_packet(
        &materialized,
        &context.work_packet,
        &active.cancellation_requested,
        active.attempt_deadline,
    )
}

fn run_deterministic_edit_packet(
    workspace: &Path,
    packet: &assemblywright_protocol::FeatureConveyorCodingWorkPacketMetadata,
    cancellation_requested: &AtomicBool,
    attempt_deadline: Instant,
) -> Result<ContainedCodingEvidence, LocalCodingSnapshotError> {
    reject_if_stopped(cancellation_requested, attempt_deadline)?;
    packet.validate()?;
    let before = collect_file_evidence(workspace, cancellation_requested, attempt_deadline)?;
    let workspace_handle = open_private_directory(workspace)?;
    for operation in &packet.operations {
        reject_if_stopped(cancellation_requested, attempt_deadline)?;
        match operation {
            LocalCodingEditOperation::Write(arguments) => {
                let (parent_handles, leaf) =
                    open_verified_parent_chain(&workspace_handle, &arguments.path)?;
                let parent = parent_handles
                    .last()
                    .ok_or(LocalCodingSnapshotError::Rejected)?;
                match arguments.expected_before_sha256 {
                    Some(expected) => {
                        let evidence = before
                            .get(&arguments.path)
                            .ok_or(LocalCodingSnapshotError::Rejected)?;
                        if evidence.sha256 != expected {
                            return Err(LocalCodingSnapshotError::Rejected);
                        }
                        let file = open_file_at(parent, &leaf, libc::O_RDONLY, 0)?;
                        let metadata = file.metadata()?;
                        if !metadata.is_file()
                            || metadata.nlink() != 1
                            || metadata.uid() != unsafe { libc::geteuid() }
                            || metadata.len() != evidence.size
                            || hash_open_file(
                                &file,
                                MAX_FILE_BYTES,
                                cancellation_requested,
                                attempt_deadline,
                            )? != expected
                        {
                            return Err(LocalCodingSnapshotError::Rejected);
                        }
                        verify_named_file_identity(parent, &leaf, &metadata)?;
                        atomic_write_at(
                            parent,
                            &leaf,
                            &decode_lower_hex_bytes(&arguments.replacement_hex)?,
                            if arguments.executable { 0o755 } else { 0o644 },
                            Some(&metadata),
                        )?;
                    }
                    None => {
                        if before.contains_key(&arguments.path) {
                            return Err(LocalCodingSnapshotError::Rejected);
                        }
                        ensure_leaf_absent(parent, &leaf)?;
                        atomic_write_at(
                            parent,
                            &leaf,
                            &decode_lower_hex_bytes(&arguments.replacement_hex)?,
                            if arguments.executable { 0o755 } else { 0o644 },
                            None,
                        )?;
                    }
                }
            }
            LocalCodingEditOperation::Delete(arguments) => {
                let evidence = before
                    .get(&arguments.path)
                    .ok_or(LocalCodingSnapshotError::Rejected)?;
                if evidence.sha256 != arguments.expected_before_sha256 {
                    return Err(LocalCodingSnapshotError::Rejected);
                }
                let (parent_handles, leaf) =
                    open_verified_parent_chain(&workspace_handle, &arguments.path)?;
                let parent = parent_handles
                    .last()
                    .ok_or(LocalCodingSnapshotError::Rejected)?;
                let file = open_file_at(parent, &leaf, libc::O_RDONLY, 0)?;
                let metadata = file.metadata()?;
                if !metadata.is_file()
                    || metadata.nlink() != 1
                    || metadata.uid() != unsafe { libc::geteuid() }
                    || hash_open_file(
                        &file,
                        MAX_FILE_BYTES,
                        cancellation_requested,
                        attempt_deadline,
                    )? != arguments.expected_before_sha256
                {
                    return Err(LocalCodingSnapshotError::Rejected);
                }
                verify_named_file_identity(parent, &leaf, &metadata)?;
                atomic_delete_at(
                    parent,
                    &leaf,
                    &metadata,
                    arguments.expected_before_sha256,
                    cancellation_requested,
                    attempt_deadline,
                )?;
            }
        }
    }
    let after = collect_file_evidence(workspace, cancellation_requested, attempt_deadline)?;
    let changed = changed_paths(&before, &after);
    if changed != packet.allowed_paths {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    Ok(ContainedCodingEvidence {
        changed_paths_sha256: local_coding_paths_sha256(&changed),
        workspace_tree_sha256: workspace_tree_sha256(&after),
        artifact_bytes: build_local_coding_patch_artifact(packet)?,
    })
}

fn open_private_directory(path: &Path) -> Result<File, LocalCodingSnapshotError> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let directory = options.open(path)?;
    validate_private_directory(&directory)?;
    Ok(directory)
}

fn validate_private_directory(directory: &File) -> Result<(), LocalCodingSnapshotError> {
    let metadata = directory.metadata()?;
    if !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    Ok(())
}

fn open_verified_parent_chain(
    workspace: &File,
    relative: &str,
) -> Result<(Vec<File>, CString), LocalCodingSnapshotError> {
    let path = Path::new(relative);
    let mut components = path.components().collect::<Vec<_>>();
    let leaf = match components.pop() {
        Some(Component::Normal(leaf)) => {
            CString::new(leaf.as_bytes()).map_err(|_| LocalCodingSnapshotError::Rejected)?
        }
        _ => return Err(LocalCodingSnapshotError::Rejected),
    };
    let mut handles = vec![workspace.try_clone()?];
    for component in components {
        let Component::Normal(name) = component else {
            return Err(LocalCodingSnapshotError::Rejected);
        };
        let name = CString::new(name.as_bytes()).map_err(|_| LocalCodingSnapshotError::Rejected)?;
        let parent = handles.last().ok_or(LocalCodingSnapshotError::Rejected)?;
        let descriptor = unsafe {
            libc::openat(
                parent.as_raw_fd(),
                name.as_ptr(),
                libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            )
        };
        if descriptor < 0 {
            return Err(LocalCodingSnapshotError::Io(std::io::Error::last_os_error()));
        }
        let handle = unsafe { File::from_raw_fd(descriptor) };
        validate_private_directory(&handle)?;
        handles.push(handle);
    }
    Ok((handles, leaf))
}

fn open_file_at(
    parent: &File,
    leaf: &CString,
    flags: i32,
    mode: libc::mode_t,
) -> Result<File, LocalCodingSnapshotError> {
    let descriptor = unsafe {
        libc::openat(
            parent.as_raw_fd(),
            leaf.as_ptr(),
            flags | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            libc::c_uint::from(mode),
        )
    };
    if descriptor < 0 {
        return Err(LocalCodingSnapshotError::Io(std::io::Error::last_os_error()));
    }
    Ok(unsafe { File::from_raw_fd(descriptor) })
}

fn fchmod_file(file: &File, mode: libc::mode_t) -> Result<(), LocalCodingSnapshotError> {
    if unsafe { libc::fchmod(file.as_raw_fd(), mode) } != 0 {
        return Err(LocalCodingSnapshotError::Io(std::io::Error::last_os_error()));
    }
    Ok(())
}

fn ensure_leaf_absent(parent: &File, leaf: &CString) -> Result<(), LocalCodingSnapshotError> {
    let mut current = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe {
        libc::fstatat(
            parent.as_raw_fd(),
            leaf.as_ptr(),
            current.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } == 0
    {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    if std::io::Error::last_os_error().raw_os_error() != Some(libc::ENOENT) {
        return Err(LocalCodingSnapshotError::Io(std::io::Error::last_os_error()));
    }
    Ok(())
}

fn atomic_write_at(
    parent: &File,
    leaf: &CString,
    replacement: &[u8],
    mode: libc::mode_t,
    expected_before: Option<&fs::Metadata>,
) -> Result<(), LocalCodingSnapshotError> {
    let temp_name = CString::new(format!(
        ".assemblywright-edit-{}-{}",
        unsafe { libc::getpid() },
        current_time_ms()?
    ))
    .map_err(|_| LocalCodingSnapshotError::Rejected)?;
    let mut temp = open_file_at(
        parent,
        &temp_name,
        libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL,
        0o600,
    )?;
    let result = (|| -> Result<(), LocalCodingSnapshotError> {
        temp.write_all(replacement)?;
        fchmod_file(&temp, mode)?;
        temp.sync_all()?;
        atomic_install_at(parent, &temp_name, leaf, expected_before)?;
        if unsafe { libc::fsync(parent.as_raw_fd()) } != 0 {
            return Err(LocalCodingSnapshotError::Io(std::io::Error::last_os_error()));
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = unsafe { libc::unlinkat(parent.as_raw_fd(), temp_name.as_ptr(), 0) };
    }
    result
}

#[cfg(target_os = "macos")]
fn atomic_delete_at(
    parent: &File,
    leaf: &CString,
    expected_identity: &fs::Metadata,
    expected_sha256: [u8; 32],
    cancellation_requested: &AtomicBool,
    attempt_deadline: Instant,
) -> Result<(), LocalCodingSnapshotError> {
    let capture_name = CString::new(format!(
        ".assemblywright-delete-{}-{}",
        unsafe { libc::getpid() },
        current_time_ms()?
    ))
    .map_err(|_| LocalCodingSnapshotError::Rejected)?;
    let capture = open_file_at(
        parent,
        &capture_name,
        libc::O_RDWR | libc::O_CREAT | libc::O_EXCL,
        0o600,
    )?;
    let placeholder = capture.metadata()?;
    if unsafe {
        libc::renameatx_np(
            parent.as_raw_fd(),
            capture_name.as_ptr(),
            parent.as_raw_fd(),
            leaf.as_ptr(),
            libc::RENAME_SWAP,
        )
    } != 0
    {
        let error = LocalCodingSnapshotError::Io(std::io::Error::last_os_error());
        let _ = unlink_file_at(parent, &capture_name);
        return Err(error);
    }
    let verification = (|| -> Result<(bool, fs::Metadata), LocalCodingSnapshotError> {
        let displaced = open_file_at(parent, &capture_name, libc::O_RDONLY, 0)?;
        let displaced_metadata = displaced.metadata()?;
        let matches = displaced_metadata.dev() == expected_identity.dev()
            && displaced_metadata.ino() == expected_identity.ino()
            && displaced_metadata.uid() == expected_identity.uid()
            && displaced_metadata.is_file()
            && displaced_metadata.nlink() == 1
            && hash_open_file(
                &displaced,
                MAX_FILE_BYTES,
                cancellation_requested,
                attempt_deadline,
            )? == expected_sha256;
        Ok((matches, displaced_metadata))
    })();
    let (matches, displaced_metadata) = match verification {
        Ok(verified) => verified,
        Err(error) => {
            rollback_delete_swap(parent, leaf, &capture_name, &placeholder)?;
            return Err(error);
        }
    };
    if !matches {
        rollback_delete_swap(parent, leaf, &capture_name, &placeholder)?;
        return Err(LocalCodingSnapshotError::Rejected);
    }
    verify_named_file_identity(parent, leaf, &placeholder)
        .map_err(|_| LocalCodingSnapshotError::EffectPossible)?;
    unlink_file_at(parent, leaf)?;
    verify_named_file_identity(parent, &capture_name, &displaced_metadata)
        .map_err(|_| LocalCodingSnapshotError::EffectPossible)?;
    unlink_file_at(parent, &capture_name)?;
    if unsafe { libc::fsync(parent.as_raw_fd()) } != 0 {
        return Err(LocalCodingSnapshotError::Io(std::io::Error::last_os_error()));
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn rollback_delete_swap(
    parent: &File,
    leaf: &CString,
    capture_name: &CString,
    placeholder: &fs::Metadata,
) -> Result<(), LocalCodingSnapshotError> {
    verify_named_file_identity(parent, leaf, placeholder)
        .map_err(|_| LocalCodingSnapshotError::EffectPossible)?;
    if unsafe {
        libc::renameatx_np(
            parent.as_raw_fd(),
            capture_name.as_ptr(),
            parent.as_raw_fd(),
            leaf.as_ptr(),
            libc::RENAME_SWAP,
        )
    } != 0
    {
        return Err(LocalCodingSnapshotError::EffectPossible);
    }
    unlink_file_at(parent, capture_name)?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn atomic_delete_at(
    _parent: &File,
    _leaf: &CString,
    _expected_identity: &fs::Metadata,
    _expected_sha256: [u8; 32],
    _cancellation_requested: &AtomicBool,
    _attempt_deadline: Instant,
) -> Result<(), LocalCodingSnapshotError> {
    Err(LocalCodingSnapshotError::Rejected)
}

#[cfg(target_os = "macos")]
fn atomic_install_at(
    parent: &File,
    temp_name: &CString,
    leaf: &CString,
    expected_before: Option<&fs::Metadata>,
) -> Result<(), LocalCodingSnapshotError> {
    let flags = if expected_before.is_some() {
        libc::RENAME_SWAP
    } else {
        libc::RENAME_EXCL
    };
    if unsafe {
        libc::renameatx_np(
            parent.as_raw_fd(),
            temp_name.as_ptr(),
            parent.as_raw_fd(),
            leaf.as_ptr(),
            flags,
        )
    } != 0
    {
        return Err(LocalCodingSnapshotError::Io(std::io::Error::last_os_error()));
    }
    if let Some(expected) = expected_before {
        if let Err(error) = verify_named_file_identity(parent, temp_name, expected) {
            let rollback = unsafe {
                libc::renameatx_np(
                    parent.as_raw_fd(),
                    temp_name.as_ptr(),
                    parent.as_raw_fd(),
                    leaf.as_ptr(),
                    libc::RENAME_SWAP,
                )
            };
            if rollback != 0 {
                return Err(LocalCodingSnapshotError::EffectPossible);
            }
            return Err(error);
        }
        unlink_file_at(parent, temp_name)?;
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn atomic_install_at(
    _parent: &File,
    _temp_name: &CString,
    _leaf: &CString,
    _expected_before: Option<&fs::Metadata>,
) -> Result<(), LocalCodingSnapshotError> {
    Err(LocalCodingSnapshotError::Rejected)
}

fn verify_named_file_identity(
    parent: &File,
    leaf: &CString,
    opened: &fs::Metadata,
) -> Result<(), LocalCodingSnapshotError> {
    let mut current = std::mem::MaybeUninit::<libc::stat>::uninit();
    if unsafe {
        libc::fstatat(
            parent.as_raw_fd(),
            leaf.as_ptr(),
            current.as_mut_ptr(),
            libc::AT_SYMLINK_NOFOLLOW,
        )
    } != 0
    {
        return Err(LocalCodingSnapshotError::Io(std::io::Error::last_os_error()));
    }
    let current = unsafe { current.assume_init() };
    if u64::try_from(current.st_dev).ok() != Some(opened.dev())
        || current.st_ino != opened.ino()
        || current.st_uid != opened.uid()
        || (current.st_mode & libc::S_IFMT) != libc::S_IFREG
    {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    Ok(())
}

fn unlink_file_at(parent: &File, leaf: &CString) -> Result<(), LocalCodingSnapshotError> {
    if unsafe { libc::unlinkat(parent.as_raw_fd(), leaf.as_ptr(), 0) } != 0 {
        return Err(LocalCodingSnapshotError::Io(std::io::Error::last_os_error()));
    }
    Ok(())
}

fn decode_lower_hex_bytes(value: &str) -> Result<Vec<u8>, LocalCodingSnapshotError> {
    if value.len() % 2 != 0
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|_| LocalCodingSnapshotError::Rejected)
        })
        .collect()
}

fn hash_open_file(
    file: &File,
    maximum: u64,
    cancellation_requested: &AtomicBool,
    attempt_deadline: Instant,
) -> Result<[u8; 32], LocalCodingSnapshotError> {
    reject_if_stopped(cancellation_requested, attempt_deadline)?;
    let before = file.metadata()?;
    if !before.is_file() || before.nlink() != 1 || before.len() > maximum {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    let mut file = file.try_clone()?;
    file.seek(SeekFrom::Start(0))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut total = 0_u64;
    loop {
        reject_if_stopped(cancellation_requested, attempt_deadline)?;
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or(LocalCodingSnapshotError::Rejected)?;
        if total > maximum {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        digest.update(&buffer[..read]);
    }
    let after = file.metadata()?;
    if total != before.len()
        || before.dev() != after.dev()
        || before.ino() != after.ino()
        || before.len() != after.len()
    {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    Ok(digest.finalize().into())
}

#[cfg(test)]
fn signal_process_group(process_group: i32, signal: i32) -> Result<(), LocalCodingSnapshotError> {
    let result = unsafe { libc::kill(-process_group, signal) };
    let error = std::io::Error::last_os_error().raw_os_error();
    if result == 0 || error == Some(libc::ESRCH) {
        Ok(())
    } else {
        Err(LocalCodingSnapshotError::EffectPossible)
    }
}

#[cfg(test)]
fn process_group_exists(process_group: i32) -> Result<bool, LocalCodingSnapshotError> {
    let result = unsafe { libc::kill(-process_group, 0) };
    if result == 0 {
        Ok(true)
    } else if std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
        Ok(false)
    } else {
        Err(LocalCodingSnapshotError::EffectPossible)
    }
}

#[cfg(test)]
fn terminate_and_reap_process_group(
    child: &mut std::process::Child,
) -> Result<(), LocalCodingSnapshotError> {
    const TERM_GRACE: Duration = Duration::from_millis(150);
    const KILL_GRACE: Duration = Duration::from_secs(1);
    let process_group =
        i32::try_from(child.id()).map_err(|_| LocalCodingSnapshotError::Rejected)?;
    signal_process_group(process_group, libc::SIGTERM)?;
    let term_deadline = Instant::now() + TERM_GRACE;
    while Instant::now() < term_deadline {
        if child.try_wait()?.is_some() {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    signal_process_group(process_group, libc::SIGKILL)?;
    let kill_deadline = Instant::now() + KILL_GRACE;
    while child.try_wait()?.is_none() {
        if Instant::now() >= kill_deadline {
            return Err(LocalCodingSnapshotError::EffectPossible);
        }
        thread::sleep(Duration::from_millis(5));
    }
    while process_group_exists(process_group)? {
        if Instant::now() >= kill_deadline {
            return Err(LocalCodingSnapshotError::EffectPossible);
        }
        thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}

fn collect_file_evidence(
    workspace: &Path,
    cancellation_requested: &AtomicBool,
    attempt_deadline: Instant,
) -> Result<HashMap<String, FileEvidence>, LocalCodingSnapshotError> {
    collect_file_evidence_with_limits(
        workspace,
        cancellation_requested,
        attempt_deadline,
        MAX_OBJECTS,
        MAX_OBJECT_BYTES,
    )
}

fn collect_file_evidence_with_limits(
    workspace: &Path,
    cancellation_requested: &AtomicBool,
    attempt_deadline: Instant,
    max_entries: usize,
    max_bytes: u64,
) -> Result<HashMap<String, FileEvidence>, LocalCodingSnapshotError> {
    struct EvidenceBudget {
        entries: usize,
        bytes: u64,
        max_entries: usize,
        max_bytes: u64,
    }

    fn visit(
        workspace: &Path,
        directory: &Path,
        evidence: &mut HashMap<String, FileEvidence>,
        budget: &mut EvidenceBudget,
        cancellation_requested: &AtomicBool,
        attempt_deadline: Instant,
    ) -> Result<(), LocalCodingSnapshotError> {
        for entry in fs::read_dir(directory)? {
            reject_if_stopped(cancellation_requested, attempt_deadline)?;
            let entry = entry?;
            budget.entries = budget
                .entries
                .checked_add(1)
                .ok_or(LocalCodingSnapshotError::Rejected)?;
            if budget.entries > budget.max_entries {
                return Err(LocalCodingSnapshotError::Rejected);
            }
            let path = entry.path();
            let relative = path
                .strip_prefix(workspace)
                .map_err(|_| LocalCodingSnapshotError::Rejected)?;
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() || metadata.uid() != unsafe { libc::geteuid() } {
                return Err(LocalCodingSnapshotError::Rejected);
            }
            if metadata.is_dir() {
                visit(
                    workspace,
                    &path,
                    evidence,
                    budget,
                    cancellation_requested,
                    attempt_deadline,
                )?;
            } else if metadata.is_file() {
                let mut options = OpenOptions::new();
                options
                    .read(true)
                    .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
                let file = options.open(&path)?;
                let opened = file.metadata()?;
                let per_file_limit = if relative.components().next()
                    == Some(Component::Normal(OsStr::new(".git")))
                {
                    MAX_OBJECT_BYTES
                } else {
                    MAX_FILE_BYTES
                };
                if !opened.is_file()
                    || opened.nlink() != 1
                    || opened.uid() != unsafe { libc::geteuid() }
                    || opened.len() > per_file_limit
                    || metadata.dev() != opened.dev()
                    || metadata.ino() != opened.ino()
                {
                    return Err(LocalCodingSnapshotError::Rejected);
                }
                budget.bytes = budget
                    .bytes
                    .checked_add(opened.len())
                    .ok_or(LocalCodingSnapshotError::Rejected)?;
                if budget.bytes > budget.max_bytes {
                    return Err(LocalCodingSnapshotError::Rejected);
                }
                let normalized = relative
                    .to_str()
                    .ok_or(LocalCodingSnapshotError::Rejected)?
                    .replace('\\', "/");
                if evidence.contains_key(&normalized) {
                    return Err(LocalCodingSnapshotError::Rejected);
                }
                evidence.insert(
                    normalized,
                    FileEvidence {
                        mode: opened.permissions().mode() & 0o777,
                        size: opened.len(),
                        sha256: hash_open_file(
                            &file,
                            per_file_limit,
                            cancellation_requested,
                            attempt_deadline,
                        )?,
                    },
                );
            } else {
                return Err(LocalCodingSnapshotError::Rejected);
            }
        }
        Ok(())
    }

    let mut evidence = HashMap::new();
    let mut budget = EvidenceBudget {
        entries: 0,
        bytes: 0,
        max_entries,
        max_bytes,
    };
    visit(
        workspace,
        workspace,
        &mut evidence,
        &mut budget,
        cancellation_requested,
        attempt_deadline,
    )?;
    Ok(evidence)
}

fn changed_paths(
    before: &HashMap<String, FileEvidence>,
    after: &HashMap<String, FileEvidence>,
) -> Vec<String> {
    let mut paths = before
        .keys()
        .chain(after.keys())
        .filter_map(|path| (before.get(path) != after.get(path)).then_some(path.clone()))
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    paths.sort_unstable();
    paths
}

fn cleanup_attempt_state(
    root: &Path,
    active: &ActiveMaterialization,
) -> Result<(), LocalCodingSnapshotError> {
    let materialized = root.join(format!("{}.materialized", active.job.attempt_id.0));
    cleanup_tree_if_exists(&materialized)?;
    cleanup_tree_if_exists(&active.staging_directory)?;
    cleanup_retained_workspace(root, &active.job)
}

fn parse_bundle(
    file: &mut File,
    destination: &Path,
    expected_snapshot_sha256: [u8; 32],
    cancellation_requested: &AtomicBool,
    attempt_deadline: Instant,
) -> Result<(), LocalCodingSnapshotError> {
    parse_bundle_with_materialized_limit(
        file,
        destination,
        expected_snapshot_sha256,
        cancellation_requested,
        attempt_deadline,
        MAX_OBJECT_BYTES,
    )
}

fn parse_bundle_with_materialized_limit(
    file: &mut File,
    destination: &Path,
    expected_snapshot_sha256: [u8; 32],
    cancellation_requested: &AtomicBool,
    attempt_deadline: Instant,
    max_materialized_bytes: u64,
) -> Result<(), LocalCodingSnapshotError> {
    reject_if_stopped(cancellation_requested, attempt_deadline)?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.len() == 0
        || metadata.len() > MAX_LOCAL_CODING_SNAPSHOT_BUNDLE_BYTES
    {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    require_exact(file, BUNDLE_MAGIC)?;
    let commit_text = read_exact_vec(file, 40)?;
    if !commit_text
        .iter()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
    {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    let commit = Oid::from_str(
        std::str::from_utf8(&commit_text).map_err(|_| LocalCodingSnapshotError::Rejected)?,
    )?;
    let repository = Repository::init(destination)?;
    let odb = repository.odb()?;
    let mut object_count = 0_usize;
    let mut object_bytes = 0_u64;
    let mut object_ids = HashSet::new();
    loop {
        reject_if_stopped(cancellation_requested, attempt_deadline)?;
        let marker = read_u8(file)?;
        if marker == 0 {
            break;
        }
        if marker != 1 {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        let kind = match read_u8(file)? {
            1 => ObjectType::Commit,
            2 => ObjectType::Tree,
            3 => ObjectType::Blob,
            _ => return Err(LocalCodingSnapshotError::Rejected),
        };
        let oid = Oid::from_bytes(&read_exact_vec(file, 20)?)?;
        let length = read_u64(file)?;
        if length > MAX_OBJECT_BYTES
            || (kind == ObjectType::Blob && length > MAX_FILE_BYTES)
            || object_count >= MAX_OBJECTS
        {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        object_bytes = object_bytes
            .checked_add(length)
            .ok_or(LocalCodingSnapshotError::Rejected)?;
        if object_bytes > MAX_OBJECT_BYTES || !object_ids.insert(oid) {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        let data = read_exact_vec(
            file,
            usize::try_from(length).map_err(|_| LocalCodingSnapshotError::Rejected)?,
        )?;
        reject_if_stopped(cancellation_requested, attempt_deadline)?;
        if odb.write(kind, &data)? != oid {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        object_count += 1;
    }
    if !object_ids.contains(&commit) {
        return Err(LocalCodingSnapshotError::Rejected);
    }

    let mut digest = Sha256::new();
    digest.update(b"assemblywright.repository-snapshot.v1\0");
    digest.update(commit.as_bytes());
    let mut seen_paths = HashSet::new();
    let mut manifest = HashMap::new();
    let mut file_count = 0_usize;
    let mut materialized_bytes = 0_u64;
    loop {
        reject_if_stopped(cancellation_requested, attempt_deadline)?;
        let marker = read_u8(file)?;
        if marker == 0 {
            break;
        }
        if marker != 1 || file_count >= MAX_OBJECTS {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        let path_length = usize::from(read_u16(file)?);
        if path_length == 0 || path_length > MAX_PATH_BYTES {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        let path_bytes = read_exact_vec(file, path_length)?;
        let relative = validate_relative_path(&path_bytes, &mut seen_paths)?;
        let mode = read_u32(file)?;
        if !matches!(mode, 0o100644 | 0o100755) {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        let oid = Oid::from_bytes(&read_exact_vec(file, 20)?)?;
        let expected_size = read_u64(file)?;
        if expected_size > MAX_FILE_BYTES || !object_ids.contains(&oid) {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        materialized_bytes = materialized_bytes
            .checked_add(expected_size)
            .ok_or(LocalCodingSnapshotError::Rejected)?;
        if materialized_bytes > max_materialized_bytes {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        let blob = odb.read(oid)?;
        if blob.kind() != ObjectType::Blob || blob.data().len() as u64 != expected_size {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        write_materialized_file(destination, &relative, blob.data(), mode)?;
        reject_if_stopped(cancellation_requested, attempt_deadline)?;
        let normalized = relative
            .to_str()
            .ok_or(LocalCodingSnapshotError::Rejected)?
            .replace('\\', "/");
        manifest.insert(normalized.clone(), (mode, oid, expected_size));
        digest.update((normalized.len() as u64).to_be_bytes());
        digest.update(normalized.as_bytes());
        digest.update(mode.to_be_bytes());
        digest.update(oid.as_bytes());
        digest.update(expected_size.to_be_bytes());
        digest.update(blob.data());
        file_count += 1;
    }
    require_exact(file, BUNDLE_END_MAGIC)?;
    let footer = read_exact_vec(file, 32)?;
    let computed: [u8; 32] = digest.finalize().into();
    if footer.as_slice() != expected_snapshot_sha256 || computed != expected_snapshot_sha256 {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    let mut trailing = [0_u8; 1];
    if file.read(&mut trailing)? != 0 {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    validate_exact_object_graph(&repository, commit, &object_ids, &manifest)?;
    reject_if_stopped(cancellation_requested, attempt_deadline)?;
    write_repository_metadata(&repository, commit)?;
    let reopened = Repository::open(destination)?;
    if reopened.head()?.target() != Some(commit) || !reopened.is_shallow() {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    Ok(())
}

fn reject_if_stopped(
    cancellation_requested: &AtomicBool,
    attempt_deadline: Instant,
) -> Result<(), LocalCodingSnapshotError> {
    if cancellation_requested.load(Ordering::Acquire) || Instant::now() >= attempt_deadline {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    Ok(())
}

fn validate_exact_object_graph(
    repository: &Repository,
    commit: Oid,
    object_ids: &HashSet<Oid>,
    manifest: &HashMap<String, (u32, Oid, u64)>,
) -> Result<(), LocalCodingSnapshotError> {
    let commit_object = repository.find_commit(commit)?;
    let tree = commit_object.tree()?;
    let mut reachable = HashSet::from([commit, tree.id()]);
    let mut reached_paths = HashSet::new();
    let mut rejected = false;
    tree.walk(TreeWalkMode::PreOrder, |root, entry| {
        if rejected {
            return TreeWalkResult::Abort;
        }
        let Some(name) = entry.name() else {
            rejected = true;
            return TreeWalkResult::Abort;
        };
        if root.len() + name.len() > MAX_PATH_BYTES {
            rejected = true;
            return TreeWalkResult::Abort;
        }
        reachable.insert(entry.id());
        match entry.kind() {
            Some(ObjectType::Tree) => TreeWalkResult::Ok,
            Some(ObjectType::Blob) => {
                let path = format!("{root}{name}");
                let mode = entry.filemode() as u32;
                let expected = manifest.get(&path);
                let blob = repository.find_blob(entry.id());
                match (expected, blob) {
                    (Some((expected_mode, expected_oid, expected_size)), Ok(blob))
                        if *expected_mode == mode
                            && *expected_oid == entry.id()
                            && *expected_size == blob.size() as u64
                            && reached_paths.insert(path) =>
                    {
                        TreeWalkResult::Ok
                    }
                    _ => {
                        rejected = true;
                        TreeWalkResult::Abort
                    }
                }
            }
            _ => {
                rejected = true;
                TreeWalkResult::Abort
            }
        }
    })?;
    if rejected
        || reachable != *object_ids
        || reached_paths.len() != manifest.len()
        || !manifest.keys().all(|path| reached_paths.contains(path))
    {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    Ok(())
}

fn write_repository_metadata(
    repository: &Repository,
    commit: Oid,
) -> Result<(), LocalCodingSnapshotError> {
    let git_dir = repository.path();
    fs::write(git_dir.join("HEAD"), b"ref: refs/heads/snapshot\n")?;
    fs::write(git_dir.join("shallow"), format!("{commit}\n"))?;
    let refs = git_dir.join("refs").join("heads");
    fs::create_dir_all(&refs)?;
    fs::write(refs.join("snapshot"), format!("{commit}\n"))?;
    let mut config = repository.config()?;
    config.set_bool("core.bare", false)?;
    config.set_bool("core.filemode", false)?;
    config.set_bool("core.autocrlf", false)?;
    config.set_bool("core.symlinks", false)?;
    config.set_str("core.hooksPath", ".git/hooks-disabled")?;
    fs::create_dir_all(git_dir.join("hooks-disabled"))?;
    let commit = repository.find_commit(commit)?;
    let tree = commit.tree()?;
    let mut index = repository.index()?;
    index.read_tree(&tree)?;
    index.write()?;
    Ok(())
}

fn write_materialized_file(
    root: &Path,
    relative: &Path,
    data: &[u8],
    mode: u32,
) -> Result<(), LocalCodingSnapshotError> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        create_private_directories(root, parent)?;
    }
    let mut options = OpenOptions::new();
    options
        .write(true)
        .create_new(true)
        .mode(if mode == 0o100755 { 0o700 } else { 0o600 })
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let mut file = options.open(&path)?;
    file.write_all(data)?;
    file.sync_all()?;
    Ok(())
}

fn create_private_directories(root: &Path, target: &Path) -> Result<(), LocalCodingSnapshotError> {
    let relative = target
        .strip_prefix(root)
        .map_err(|_| LocalCodingSnapshotError::Rejected)?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        let Component::Normal(name) = component else {
            return Err(LocalCodingSnapshotError::Rejected);
        };
        current.push(name);
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || !metadata.is_dir() {
                    return Err(LocalCodingSnapshotError::Rejected);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current)?;
                fs::set_permissions(&current, fs::Permissions::from_mode(0o700))?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

fn validate_relative_path(
    bytes: &[u8],
    seen: &mut HashSet<String>,
) -> Result<PathBuf, LocalCodingSnapshotError> {
    let text = std::str::from_utf8(bytes).map_err(|_| LocalCodingSnapshotError::Rejected)?;
    if text.is_empty()
        || text.len() > MAX_PATH_BYTES
        || text.contains('\\')
        || text.split('/').any(|component| component.is_empty())
    {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    let path = PathBuf::from(text);
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    for component in path.components() {
        let Component::Normal(name) = component else {
            return Err(LocalCodingSnapshotError::Rejected);
        };
        let name = name.to_str().ok_or(LocalCodingSnapshotError::Rejected)?;
        if name.is_empty()
            || name == "."
            || name == ".."
            || name.contains(['/', '\\', ':'])
            || name.ends_with([' ', '.'])
            || name.eq_ignore_ascii_case(".git")
            || is_windows_reserved_name(name)
        {
            return Err(LocalCodingSnapshotError::Rejected);
        }
    }
    if !seen.insert(text.to_ascii_lowercase()) {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    Ok(path)
}

fn is_windows_reserved_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (stem.len() == 4
            && (stem.starts_with("COM") || stem.starts_with("LPT"))
            && matches!(stem.as_bytes()[3], b'1'..=b'9'))
}

fn ensure_private_directory(path: &Path) -> Result<(), LocalCodingSnapshotError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink()
                || !metadata.is_dir()
                || metadata.uid() != unsafe { libc::geteuid() }
                || metadata.permissions().mode() & 0o077 != 0
            {
                return Err(LocalCodingSnapshotError::Rejected);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(path)?;
            fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
        }
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

impl RetainedWorkspaceRecord {
    fn new(
        job: JobEnvelope,
        sealed_workspace_name: String,
        workspace_tree_sha256: [u8; 32],
        expires_at_ms: u64,
    ) -> Result<Self, LocalCodingSnapshotError> {
        let mut record = Self {
            record_version: RETENTION_RECORD_VERSION,
            job,
            sealed_workspace_name,
            workspace_tree_sha256,
            expires_at_ms,
            binding_sha256: [0; 32],
        };
        record.binding_sha256 = record.expected_binding()?;
        Ok(record)
    }

    fn expected_binding(&self) -> Result<[u8; 32], LocalCodingSnapshotError> {
        let mut digest = Sha256::new();
        digest.update(b"assemblywright-retained-workspace-v1\0");
        digest.update(RETENTION_RECORD_VERSION.to_be_bytes());
        digest
            .update(serde_json::to_vec(&self.job).map_err(|_| LocalCodingSnapshotError::Rejected)?);
        digest.update(self.sealed_workspace_name.as_bytes());
        digest.update(self.workspace_tree_sha256);
        digest.update(self.expires_at_ms.to_be_bytes());
        Ok(digest.finalize().into())
    }

    fn validate(&self) -> Result<(), LocalCodingSnapshotError> {
        self.job.validate_local_coding()?;
        if self.record_version != RETENTION_RECORD_VERSION
            || self.sealed_workspace_name != sealed_workspace_name(&self.job)
            || self.workspace_tree_sha256 == [0; 32]
            || self.binding_sha256 != self.expected_binding()?
        {
            return Err(LocalCodingSnapshotError::Rejected);
        }
        Ok(())
    }
}

fn sealed_workspace_name(job: &JobEnvelope) -> String {
    format!("{}.sealed", job.attempt_id.0)
}

fn retention_record_name(job: &JobEnvelope) -> String {
    format!("{}.retention.json", job.attempt_id.0)
}

fn workspace_tree_sha256(evidence: &HashMap<String, FileEvidence>) -> [u8; 32] {
    let mut paths = evidence.keys().collect::<Vec<_>>();
    paths.sort_unstable();
    let mut digest = Sha256::new();
    digest.update(b"assemblywright-workspace-tree-v1\0");
    for path in paths {
        let item = &evidence[path];
        digest.update((path.len() as u64).to_be_bytes());
        digest.update(path.as_bytes());
        digest.update(item.mode.to_be_bytes());
        digest.update(item.size.to_be_bytes());
        digest.update(item.sha256);
    }
    digest.finalize().into()
}

fn write_retention_record(
    root: &Path,
    record: RetainedWorkspaceRecord,
) -> Result<(), LocalCodingSnapshotError> {
    record.validate()?;
    let encoded = serde_json::to_vec(&record).map_err(|_| LocalCodingSnapshotError::Rejected)?;
    if encoded.is_empty() || encoded.len() > MAX_RETENTION_RECORD_BYTES {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    let mut options = OpenOptions::new();
    options
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let mut file = options.open(root.join(retention_record_name(&record.job)))?;
    file.write_all(&encoded)?;
    fchmod_file(&file, 0o600)?;
    file.sync_all()?;
    Ok(())
}

fn read_retention_record(path: &Path) -> Result<RetainedWorkspaceRecord, LocalCodingSnapshotError> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let mut file = options.open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.len() == 0
        || metadata.len() > MAX_RETENTION_RECORD_BYTES as u64
    {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    let mut encoded = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut encoded)?;
    let record: RetainedWorkspaceRecord =
        serde_json::from_slice(&encoded).map_err(|_| LocalCodingSnapshotError::Rejected)?;
    record.validate()?;
    Ok(record)
}

fn recover_retained_workspace(
    root: &Path,
) -> Result<Option<JobEnvelope>, LocalCodingSnapshotError> {
    let mut sealed = Vec::new();
    let mut records = Vec::new();
    let mut stale = Vec::new();
    for entry in fs::read_dir(root)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(LocalCodingSnapshotError::Rejected)?;
        if name.ends_with(".sealed") {
            sealed.push(path);
        } else if name.ends_with(".retention.json") {
            records.push(path);
        } else {
            stale.push(path);
        }
    }
    for path in stale {
        cleanup_tree(&path)?;
    }
    if sealed.is_empty() && records.is_empty() {
        return Ok(None);
    }
    if sealed.len() != 1 || records.len() != 1 {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    let record = read_retention_record(&records[0])?;
    if sealed[0].file_name().and_then(|name| name.to_str())
        != Some(record.sealed_workspace_name.as_str())
        || records[0].file_name().and_then(|name| name.to_str())
            != Some(retention_record_name(&record.job).as_str())
    {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    let workspace = open_private_directory(&sealed[0])?;
    drop(workspace);
    let evidence = collect_file_evidence(
        &sealed[0],
        &AtomicBool::new(false),
        Instant::now() + Duration::from_secs(60),
    )?;
    if workspace_tree_sha256(&evidence) != record.workspace_tree_sha256 {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    if record.expires_at_ms <= current_time_ms()? {
        cleanup_tree(&sealed[0])?;
        fs::remove_file(&records[0])?;
        return Ok(None);
    }
    Ok(Some(record.job))
}

fn cleanup_retained_workspace(
    root: &Path,
    job: &JobEnvelope,
) -> Result<(), LocalCodingSnapshotError> {
    cleanup_tree_if_exists(&root.join(sealed_workspace_name(job)))?;
    let record = root.join(retention_record_name(job));
    match fs::symlink_metadata(&record) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(LocalCodingSnapshotError::Rejected)
        }
        Ok(_) => {
            let root_handle = open_private_directory(root)?;
            let name = CString::new(retention_record_name(job))
                .map_err(|_| LocalCodingSnapshotError::Rejected)?;
            unlink_file_at(&root_handle, &name)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn cleanup_tree_if_exists(path: &Path) -> Result<(), LocalCodingSnapshotError> {
    match fs::symlink_metadata(path) {
        Ok(_) => cleanup_tree(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn cleanup_tree(path: &Path) -> Result<(), LocalCodingSnapshotError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        fs::remove_file(path)?;
        return Ok(());
    }
    // On Unix, std's remove_dir_all implementation traverses relative to held
    // directory descriptors and never follows symlinks. If this named entry is
    // replaced after the metadata check, it either removes only that link or
    // fails; it cannot recursively traverse the replacement target.
    fs::remove_dir_all(path)?;
    Ok(())
}

fn require_exact(reader: &mut File, expected: &[u8]) -> Result<(), LocalCodingSnapshotError> {
    if read_exact_vec(reader, expected.len())? != expected {
        return Err(LocalCodingSnapshotError::Rejected);
    }
    Ok(())
}

fn read_exact_vec(reader: &mut File, length: usize) -> Result<Vec<u8>, LocalCodingSnapshotError> {
    let mut bytes = vec![0; length];
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn read_u8(reader: &mut File) -> Result<u8, LocalCodingSnapshotError> {
    Ok(read_exact_vec(reader, 1)?[0])
}

fn read_u16(reader: &mut File) -> Result<u16, LocalCodingSnapshotError> {
    Ok(u16::from_be_bytes(
        read_exact_vec(reader, 2)?
            .try_into()
            .map_err(|_| LocalCodingSnapshotError::Rejected)?,
    ))
}

fn read_u32(reader: &mut File) -> Result<u32, LocalCodingSnapshotError> {
    Ok(u32::from_be_bytes(
        read_exact_vec(reader, 4)?
            .try_into()
            .map_err(|_| LocalCodingSnapshotError::Rejected)?,
    ))
}

fn read_u64(reader: &mut File) -> Result<u64, LocalCodingSnapshotError> {
    Ok(u64::from_be_bytes(
        read_exact_vec(reader, 8)?
            .try_into()
            .map_err(|_| LocalCodingSnapshotError::Rejected)?,
    ))
}

fn json_sha256(value: &serde_json::Value) -> Result<[u8; 32], LocalCodingSnapshotError> {
    let encoded = serde_json::to_vec(value).map_err(|_| LocalCodingSnapshotError::Rejected)?;
    Ok(Sha256::digest(encoded).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use assemblywright_protocol::{
        AttemptId, CancellationId, ContextHandlingPolicy, DeviceId,
        FeatureConveyorCodingWorkPacketMetadata, LeaseId, LocalCodingJobRequest, Sensitivity,
        StepId, TaskId, LOCAL_CODING_CAPABILITY_ID, LOCAL_CODING_MODEL,
    };
    use git2::{IndexAddOption, Signature};
    use serde_json::to_value;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;
    use tempfile::tempdir;
    use uuid::Uuid;

    fn job() -> JobEnvelope {
        job_with_snapshot([3; 32])
    }

    fn job_with_snapshot(snapshot_sha256: [u8; 32]) -> JobEnvelope {
        let work_packet = FeatureConveyorCodingWorkPacketMetadata::fixture(
            Uuid::new_v4(),
            Sha256::digest(b"bounded materialization\n").into(),
        );
        let work_packet_sha256 = work_packet.canonical_sha256().unwrap();
        let context = to_value(LocalCodingJobRequest {
            feature_id: Uuid::new_v4(),
            specification_revision: 1,
            lifecycle_revision: 2,
            feature_lease_id: Uuid::new_v4(),
            snapshot_id: Uuid::new_v4(),
            snapshot_sha256,
            work_packet_sha256,
            work_packet,
            device_id: DeviceId::new(Uuid::new_v4()),
            device_registry_revision: 1,
            queue_revision: 1,
            emergency_pause_revision: 1,
        })
        .unwrap();
        JobEnvelope {
            protocol_version: PROTOCOL_VERSION,
            connection_epoch: 1,
            sequence: 1,
            task_id: TaskId::new(Uuid::new_v4()),
            step_id: StepId::new(Uuid::new_v4()),
            attempt_id: AttemptId::new(Uuid::new_v4()),
            lease_id: LeaseId::new(Uuid::new_v4()),
            cancellation_id: CancellationId::new(Uuid::new_v4()),
            capability_id: LOCAL_CODING_CAPABILITY_ID.to_string(),
            selected_model: LOCAL_CODING_MODEL.to_string(),
            sensitivity: Sensitivity::Workspace,
            context_handling: ContextHandlingPolicy::SealedUntilResolvedOrExpired,
            lease_duration_ms: 60_000,
            deadline_after_ms: 60_000,
            context_sha256: json_sha256(&context).unwrap(),
            context,
        }
    }

    fn fixture_bundle() -> (Vec<u8>, [u8; 32]) {
        fixture_bundle_with_extra_object(false)
    }

    fn fixture_bundle_with_extra_object(include_extra_object: bool) -> (Vec<u8>, [u8; 32]) {
        fixture_bundle_with_paths(&["README.md"], include_extra_object)
    }

    fn fixture_bundle_with_paths(
        manifest_paths: &[&str],
        include_extra_object: bool,
    ) -> (Vec<u8>, [u8; 32]) {
        let source = tempdir().unwrap();
        let repository = Repository::init(source.path()).unwrap();
        let content = b"bounded materialization\n";
        fs::write(source.path().join("README.md"), content).unwrap();
        let mut index = repository.index().unwrap();
        index
            .add_all(["README.md"].iter(), IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repository.find_tree(tree_oid).unwrap();
        let blob_oid = tree.get_name("README.md").unwrap().id();
        let signature = Signature::now("Assemblywright Test", "test@example.invalid").unwrap();
        let commit_oid = repository
            .commit(
                Some("HEAD"),
                &signature,
                &signature,
                "bounded fixture",
                &tree,
                &[],
            )
            .unwrap();
        drop(tree);
        drop(index);
        let odb = repository.odb().unwrap();
        let mut bundle = Vec::new();
        bundle.extend_from_slice(BUNDLE_MAGIC);
        bundle.extend_from_slice(commit_oid.to_string().as_bytes());
        for (kind, oid) in [
            (ObjectType::Commit, commit_oid),
            (ObjectType::Tree, tree_oid),
            (ObjectType::Blob, blob_oid),
        ] {
            let object = odb.read(oid).unwrap();
            assert_eq!(object.kind(), kind);
            bundle.extend_from_slice(&[
                1,
                match kind {
                    ObjectType::Commit => 1,
                    ObjectType::Tree => 2,
                    ObjectType::Blob => 3,
                    _ => unreachable!(),
                },
            ]);
            bundle.extend_from_slice(oid.as_bytes());
            bundle.extend_from_slice(&(object.data().len() as u64).to_be_bytes());
            bundle.extend_from_slice(object.data());
        }
        if include_extra_object {
            let content = b"unreferenced secret object\n";
            let oid = odb.write(ObjectType::Blob, content).unwrap();
            bundle.extend_from_slice(&[1, 3]);
            bundle.extend_from_slice(oid.as_bytes());
            bundle.extend_from_slice(&(content.len() as u64).to_be_bytes());
            bundle.extend_from_slice(content);
        }
        bundle.push(0);
        let mut digest = Sha256::new();
        digest.update(b"assemblywright.repository-snapshot.v1\0");
        digest.update(commit_oid.as_bytes());
        for path in manifest_paths {
            bundle.push(1);
            bundle.extend_from_slice(&(path.len() as u16).to_be_bytes());
            bundle.extend_from_slice(path.as_bytes());
            bundle.extend_from_slice(&0o100644_u32.to_be_bytes());
            bundle.extend_from_slice(blob_oid.as_bytes());
            bundle.extend_from_slice(&(content.len() as u64).to_be_bytes());
            digest.update((path.len() as u64).to_be_bytes());
            digest.update(path.as_bytes());
            digest.update(0o100644_u32.to_be_bytes());
            digest.update(blob_oid.as_bytes());
            digest.update((content.len() as u64).to_be_bytes());
            digest.update(content);
        }
        bundle.push(0);
        let snapshot_sha256: [u8; 32] = digest.finalize().into();
        bundle.extend_from_slice(BUNDLE_END_MAGIC);
        bundle.extend_from_slice(&snapshot_sha256);
        (bundle, snapshot_sha256)
    }

    fn chunk(
        job: &JobEnvelope,
        offset: u64,
        total_bytes: u64,
        content: &[u8],
    ) -> LocalCodingSnapshotChunk {
        let request = request_for_job(job, offset).unwrap();
        let mut content_hex = String::with_capacity(content.len() * 2);
        for byte in content {
            use std::fmt::Write as _;
            write!(&mut content_hex, "{byte:02x}").unwrap();
        }
        LocalCodingSnapshotChunk {
            protocol_version: request.protocol_version,
            connection_epoch: request.connection_epoch,
            task_id: request.task_id,
            step_id: request.step_id,
            attempt_id: request.attempt_id,
            lease_id: request.lease_id,
            cancellation_id: request.cancellation_id,
            snapshot_id: request.snapshot_id,
            snapshot_sha256: request.snapshot_sha256,
            offset,
            total_bytes,
            content_sha256: Sha256::digest(content).into(),
            content_hex,
            complete: offset + content.len() as u64 == total_bytes,
        }
    }

    fn cancellation(job: &JobEnvelope) -> CancellationInstruction {
        CancellationInstruction {
            protocol_version: job.protocol_version,
            connection_epoch: job.connection_epoch,
            sequence: job.sequence + 2,
            task_id: job.task_id,
            step_id: job.step_id,
            attempt_id: job.attempt_id,
            lease_id: job.lease_id,
            cancellation_id: job.cancellation_id,
            deadline_after_ms: 1_000,
        }
    }

    #[test]
    fn disabled_duplicate_and_startup_cleanup_fail_closed() {
        let directory = tempdir().unwrap();
        let stale = directory.path().join("local-coding-snapshots");
        fs::create_dir(&stale).unwrap();
        fs::set_permissions(&stale, fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(stale.join("stale"), b"stale").unwrap();
        let disabled = LocalCodingSnapshotRuntime::open(directory.path(), false).unwrap();
        assert_eq!(fs::read_dir(&stale).unwrap().count(), 0);
        assert!(matches!(
            disabled.admit(job()),
            Err(LocalCodingSnapshotError::Disabled)
        ));

        let runtime = LocalCodingSnapshotRuntime::open(directory.path(), true).unwrap();
        runtime.admit(job()).unwrap();
        assert!(matches!(
            runtime.admit(job()),
            Err(LocalCodingSnapshotError::AlreadyActive)
        ));
        runtime.shutdown().unwrap();
        assert_eq!(fs::read_dir(&stale).unwrap().count(), 0);
    }

    #[test]
    fn exact_chunks_materialize_verify_receipt_and_leave_no_repository_state() {
        let directory = tempdir().unwrap();
        let runtime = LocalCodingSnapshotRuntime::open(directory.path(), true).unwrap();
        let (bundle, snapshot_sha256) = fixture_bundle();
        let job = job_with_snapshot(snapshot_sha256);
        runtime.admit(job.clone()).unwrap();
        let mut offset = 0_usize;
        let mut completed = None;
        while offset < bundle.len() {
            let end = (offset + 23).min(bundle.len());
            match runtime
                .accept_chunk(chunk(
                    &job,
                    offset as u64,
                    bundle.len() as u64,
                    &bundle[offset..end],
                ))
                .unwrap()
            {
                LocalCodingSnapshotAcceptance::Continue { next_offset } => {
                    assert_eq!(next_offset, end as u64)
                }
                LocalCodingSnapshotAcceptance::Complete(result) => completed = Some(*result),
            }
            offset = end;
        }
        let result = completed.expect("final chunk returns one receipt");
        result.validate_for_job(&job).unwrap();
        let acknowledgement = runtime.cancel(&cancellation(&job)).unwrap();
        assert_eq!(
            acknowledgement.status,
            CancellationAcknowledgementStatus::Cancelled
        );
        let root = directory.path().join("local-coding-snapshots");
        assert_eq!(fs::read_dir(root).unwrap().count(), 0);
    }

    fn complete_fixture(
        directory: &Path,
    ) -> (LocalCodingSnapshotRuntime, JobEnvelope, PathBuf, PathBuf) {
        let runtime = LocalCodingSnapshotRuntime::open(directory, true).unwrap();
        let (bundle, snapshot_sha256) = fixture_bundle();
        let job = job_with_snapshot(snapshot_sha256);
        runtime.admit(job.clone()).unwrap();
        assert!(matches!(
            runtime
                .accept_chunk(chunk(&job, 0, bundle.len() as u64, &bundle))
                .unwrap(),
            LocalCodingSnapshotAcceptance::Complete(_)
        ));
        let sealed = runtime.root.join(sealed_workspace_name(&job));
        let record = runtime.root.join(retention_record_name(&job));
        (runtime, job, sealed, record)
    }

    #[test]
    fn retained_completion_recovers_exact_cancel_after_restart_and_blocks_admission() {
        let directory = tempdir().unwrap();
        let (runtime, job, sealed, record) = complete_fixture(directory.path());
        drop(runtime);
        let restarted = LocalCodingSnapshotRuntime::open(directory.path(), true).unwrap();
        assert!(matches!(
            restarted.admit(job.clone()),
            Err(LocalCodingSnapshotError::AlreadyActive)
        ));
        restarted.cancel(&cancellation(&job)).unwrap();
        assert!(!sealed.exists());
        assert!(!record.exists());
    }

    #[test]
    fn retained_completion_tamper_or_orphan_fails_closed_and_exact_expiry_cleans_pair() {
        let tampered = tempdir().unwrap();
        let (runtime, _job, sealed, record) = complete_fixture(tampered.path());
        drop(runtime);
        fs::write(sealed.join("README.md"), b"tampered").unwrap();
        assert!(matches!(
            LocalCodingSnapshotRuntime::open(tampered.path(), true),
            Err(LocalCodingSnapshotError::Rejected)
        ));
        assert!(record.exists());

        let orphaned = tempdir().unwrap();
        let (runtime, _job, _sealed, record) = complete_fixture(orphaned.path());
        drop(runtime);
        fs::remove_file(record).unwrap();
        assert!(matches!(
            LocalCodingSnapshotRuntime::open(orphaned.path(), true),
            Err(LocalCodingSnapshotError::Rejected)
        ));

        let expired = tempdir().unwrap();
        let (runtime, _job, sealed, record_path) = complete_fixture(expired.path());
        drop(runtime);
        let mut record = read_retention_record(&record_path).unwrap();
        record.expires_at_ms = current_time_ms().unwrap();
        record.binding_sha256 = record.expected_binding().unwrap();
        fs::remove_file(&record_path).unwrap();
        write_retention_record(&expired.path().join("local-coding-snapshots"), record).unwrap();
        let restarted = LocalCodingSnapshotRuntime::open(expired.path(), true).unwrap();
        assert!(restarted.completed.lock().unwrap().is_none());
        assert!(!sealed.exists());
        assert!(!record_path.exists());
    }

    #[test]
    fn held_parent_descriptor_prevents_same_uid_symlink_substitution_escape() {
        let directory = tempdir().unwrap();
        let workspace = directory.path().join("workspace");
        let parent = workspace.join("nested");
        let moved_parent = workspace.join("nested-held");
        let outside = directory.path().join("outside");
        fs::create_dir_all(&parent).unwrap();
        fs::create_dir(&outside).unwrap();
        fs::set_permissions(&workspace, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(&parent, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(&outside, fs::Permissions::from_mode(0o700)).unwrap();
        let workspace_handle = open_private_directory(&workspace).unwrap();
        let (parents, leaf) =
            open_verified_parent_chain(&workspace_handle, "nested/new.rs").unwrap();
        fs::rename(&parent, &moved_parent).unwrap();
        std::os::unix::fs::symlink(&outside, &parent).unwrap();
        atomic_write_at(parents.last().unwrap(), &leaf, b"safe", 0o644, None).unwrap();
        assert_eq!(fs::read(moved_parent.join("new.rs")).unwrap(), b"safe");
        assert!(!outside.join("new.rs").exists());

        fs::write(moved_parent.join("replace.rs"), b"before").unwrap();
        fs::write(outside.join("replace.rs"), b"outside").unwrap();
        let (parents, leaf) =
            open_verified_parent_chain(&workspace_handle, "nested-held/replace.rs").unwrap();
        let opened = open_file_at(parents.last().unwrap(), &leaf, libc::O_RDONLY, 0).unwrap();
        let expected = opened.metadata().unwrap();
        let wrong_expected = fs::metadata(outside.join("replace.rs")).unwrap();
        assert!(matches!(
            atomic_write_at(
                parents.last().unwrap(),
                &leaf,
                b"rejected",
                0o644,
                Some(&wrong_expected),
            ),
            Err(LocalCodingSnapshotError::Rejected)
        ));
        assert_eq!(
            fs::read(moved_parent.join("replace.rs")).unwrap(),
            b"before"
        );
        fs::rename(&moved_parent, workspace.join("nested-held-again")).unwrap();
        std::os::unix::fs::symlink(&outside, &moved_parent).unwrap();
        atomic_write_at(
            parents.last().unwrap(),
            &leaf,
            b"replacement",
            0o644,
            Some(&expected),
        )
        .unwrap();
        assert_eq!(
            fs::read(workspace.join("nested-held-again/replace.rs")).unwrap(),
            b"replacement"
        );
        assert_eq!(fs::read(outside.join("replace.rs")).unwrap(), b"outside");
    }

    #[test]
    fn atomic_delete_rolls_back_same_uid_leaf_substitution_without_deleting_replacement() {
        let directory = tempdir().unwrap();
        let workspace = directory.path().join("workspace");
        fs::create_dir(&workspace).unwrap();
        fs::set_permissions(&workspace, fs::Permissions::from_mode(0o700)).unwrap();
        let victim = workspace.join("victim.rs");
        let original = b"approved original";
        let replacement = b"same uid replacement";
        fs::write(&victim, original).unwrap();
        let workspace_handle = open_private_directory(&workspace).unwrap();
        let (parents, leaf) = open_verified_parent_chain(&workspace_handle, "victim.rs").unwrap();
        let opened = open_file_at(parents.last().unwrap(), &leaf, libc::O_RDONLY, 0).unwrap();
        let expected_identity = opened.metadata().unwrap();
        let expected_sha256 = Sha256::digest(original).into();
        let moved_original = workspace.join("original-moved.rs");
        fs::rename(&victim, &moved_original).unwrap();
        fs::write(&victim, replacement).unwrap();

        assert!(matches!(
            atomic_delete_at(
                parents.last().unwrap(),
                &leaf,
                &expected_identity,
                expected_sha256,
                &AtomicBool::new(false),
                Instant::now() + Duration::from_secs(5),
            ),
            Err(LocalCodingSnapshotError::Rejected)
        ));
        assert_eq!(fs::read(&victim).unwrap(), replacement);
        assert_eq!(fs::read(&moved_original).unwrap(), original);
        assert!(fs::read_dir(&workspace).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".assemblywright-delete-")
        }));
    }

    #[test]
    fn corrupt_or_out_of_order_chunks_fail_and_immediately_clean_partial_state() {
        let directory = tempdir().unwrap();
        let runtime = LocalCodingSnapshotRuntime::open(directory.path(), true).unwrap();
        let (bundle, snapshot_sha256) = fixture_bundle();
        let job = job_with_snapshot(snapshot_sha256);
        runtime.admit(job.clone()).unwrap();
        let out_of_order = chunk(&job, 1, bundle.len() as u64, &bundle[..16]);
        assert!(matches!(
            runtime.accept_chunk(out_of_order),
            Err(LocalCodingSnapshotError::Protocol(_)) | Err(LocalCodingSnapshotError::Rejected)
        ));
        assert!(runtime.active.lock().unwrap().is_none());
        assert_eq!(
            fs::read_dir(directory.path().join("local-coding-snapshots"))
                .unwrap()
                .count(),
            0
        );
    }

    #[test]
    fn local_attempt_deadline_dominates_chunk_acceptance_and_cleans_state() {
        let directory = tempdir().unwrap();
        let runtime = LocalCodingSnapshotRuntime::open(directory.path(), true).unwrap();
        let (bundle, snapshot_sha256) = fixture_bundle();
        let mut job = job_with_snapshot(snapshot_sha256);
        job.lease_duration_ms = 1;
        job.deadline_after_ms = 1;
        runtime.admit(job.clone()).unwrap();
        thread::sleep(Duration::from_millis(5));
        assert!(matches!(
            runtime.accept_chunk(chunk(&job, 0, bundle.len() as u64, &bundle)),
            Err(LocalCodingSnapshotError::Rejected)
        ));
        assert!(runtime.active.lock().unwrap().is_none());
        assert_eq!(fs::read_dir(&runtime.root).unwrap().count(), 0);
    }

    #[test]
    fn manifest_path_policy_rejects_ambiguous_or_cross_platform_unsafe_names() {
        let mut seen = HashSet::new();
        assert_eq!(
            validate_relative_path(b"Sources/Worker.swift", &mut seen).unwrap(),
            PathBuf::from("Sources/Worker.swift")
        );

        for invalid in [
            b"".as_slice(),
            b"/absolute",
            b"trailing/",
            b"double//separator",
            b"./relative",
            b"parent/../escape",
            b".git/config",
            b"metadata/.GIT/index",
            b"back\\slash",
            b"drive:name",
            b"trailing-dot.",
            b"trailing-space ",
            b"CON",
            b"aux.txt",
            b"COM1.log",
            b"lpt9",
        ] {
            assert!(
                matches!(
                    validate_relative_path(invalid, &mut HashSet::new()),
                    Err(LocalCodingSnapshotError::Rejected)
                ),
                "unsafe manifest path was accepted: {:?}",
                String::from_utf8_lossy(invalid)
            );
        }
        assert!(matches!(
            validate_relative_path(&[0xff], &mut HashSet::new()),
            Err(LocalCodingSnapshotError::Rejected)
        ));
        assert!(matches!(
            validate_relative_path(
                vec![b'a'; MAX_PATH_BYTES + 1].as_slice(),
                &mut HashSet::new()
            ),
            Err(LocalCodingSnapshotError::Rejected)
        ));
        let maximum = (0..5)
            .map(|_| "a".repeat(204))
            .collect::<Vec<_>>()
            .join("/");
        assert_eq!(maximum.len(), MAX_PATH_BYTES);
        assert_eq!(
            validate_relative_path(maximum.as_bytes(), &mut HashSet::new()).unwrap(),
            PathBuf::from(maximum)
        );

        let mut collisions = HashSet::new();
        validate_relative_path(b"Sources/File.swift", &mut collisions).unwrap();
        assert!(matches!(
            validate_relative_path(b"sources/file.swift", &mut collisions),
            Err(LocalCodingSnapshotError::Rejected)
        ));
    }

    #[test]
    fn evidence_walk_fails_closed_on_cancellation_entry_and_aggregate_bounds() {
        let directory = tempdir().unwrap();
        fs::write(directory.path().join("README.md"), b"1234").unwrap();
        fs::create_dir(directory.path().join("nested")).unwrap();
        fs::write(directory.path().join("nested/file.txt"), b"5678").unwrap();
        let deadline = Instant::now() + Duration::from_secs(60);

        assert!(matches!(
            collect_file_evidence_with_limits(
                directory.path(),
                &AtomicBool::new(true),
                deadline,
                MAX_OBJECTS,
                MAX_OBJECT_BYTES,
            ),
            Err(LocalCodingSnapshotError::Rejected)
        ));

        let large = tempdir().unwrap();
        let large_file = File::create(large.path().join("large.bin")).unwrap();
        large_file.set_len(MAX_FILE_BYTES).unwrap();
        let cancellation_requested = Arc::new(AtomicBool::new(false));
        let canceller_flag = Arc::clone(&cancellation_requested);
        let canceller = thread::spawn(move || {
            thread::sleep(Duration::from_millis(1));
            canceller_flag.store(true, Ordering::Release);
        });
        assert!(matches!(
            collect_file_evidence(
                large.path(),
                &cancellation_requested,
                Instant::now() + Duration::from_secs(60),
            ),
            Err(LocalCodingSnapshotError::Rejected)
        ));
        canceller.join().unwrap();
        assert!(matches!(
            collect_file_evidence_with_limits(
                directory.path(),
                &AtomicBool::new(false),
                deadline,
                1,
                MAX_OBJECT_BYTES,
            ),
            Err(LocalCodingSnapshotError::Rejected)
        ));
        assert!(matches!(
            collect_file_evidence_with_limits(
                directory.path(),
                &AtomicBool::new(false),
                deadline,
                MAX_OBJECTS,
                7,
            ),
            Err(LocalCodingSnapshotError::Rejected)
        ));
    }

    #[test]
    fn process_group_termination_escalates_and_removes_descendants_with_bounded_waits() {
        let mut child = std::process::Command::new("/bin/sh")
            .args(["-c", "trap '' TERM; sleep 30"])
            .process_group(0)
            .spawn()
            .unwrap();
        let process_group = i32::try_from(child.id()).unwrap();
        terminate_and_reap_process_group(&mut child).unwrap();
        assert!(child.try_wait().unwrap().is_some());
        assert!(!process_group_exists(process_group).unwrap());
    }

    #[test]
    fn unreferenced_object_is_rejected_even_when_manifest_digest_is_valid() {
        let directory = tempdir().unwrap();
        let runtime = LocalCodingSnapshotRuntime::open(directory.path(), true).unwrap();
        let (bundle, snapshot_sha256) = fixture_bundle_with_extra_object(true);
        let job = job_with_snapshot(snapshot_sha256);
        runtime.admit(job.clone()).unwrap();
        assert!(matches!(
            runtime.accept_chunk(chunk(&job, 0, bundle.len() as u64, &bundle)),
            Err(LocalCodingSnapshotError::Rejected)
        ));
        assert_eq!(fs::read_dir(&runtime.root).unwrap().count(), 0);
    }

    #[test]
    fn duplicate_blob_paths_cannot_amplify_materialized_output_past_aggregate_limit() {
        let directory = tempdir().unwrap();
        let (bundle, snapshot_sha256) = fixture_bundle_with_paths(&["README.md", "COPY.md"], false);
        let bundle_path = directory.path().join("snapshot.bundle");
        fs::write(&bundle_path, bundle).unwrap();
        let mut bundle_file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(bundle_path)
            .unwrap();
        let workspace = directory.path().join("workspace");
        let one_blob_limit = b"bounded materialization\n".len() as u64;

        assert!(matches!(
            parse_bundle_with_materialized_limit(
                &mut bundle_file,
                &workspace,
                snapshot_sha256,
                &AtomicBool::new(false),
                Instant::now() + Duration::from_secs(60),
                one_blob_limit,
            ),
            Err(LocalCodingSnapshotError::Rejected)
        ));
        assert!(workspace.join("README.md").is_file());
        assert!(!workspace.join("COPY.md").exists());
    }

    #[test]
    fn cancellation_during_verification_waits_until_cleanup_is_complete() {
        let directory = tempdir().unwrap();
        let runtime = Arc::new(LocalCodingSnapshotRuntime::open(directory.path(), true).unwrap());
        let job = job();
        runtime.admit(job.clone()).unwrap();
        let staging = {
            let mut active = runtime.active.lock().unwrap();
            let active = active.as_mut().unwrap();
            active.verifying = true;
            active.staging_directory.clone()
        };
        let instruction = cancellation(&job);
        let (sender, receiver) = mpsc::channel();
        let cancelling_runtime = Arc::clone(&runtime);
        let cancellation_thread = thread::spawn(move || {
            sender
                .send(cancelling_runtime.cancel(&instruction))
                .unwrap();
        });
        let mut requested = false;
        for _ in 0..100 {
            let was_requested = runtime
                .active
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .cancellation_requested
                .load(Ordering::Acquire);
            if was_requested {
                requested = true;
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        assert!(requested);
        assert!(receiver.recv_timeout(Duration::from_millis(25)).is_err());
        cleanup_tree(&staging).unwrap();
        runtime.active.lock().unwrap().take();
        runtime.state_changed.notify_all();
        assert_eq!(
            receiver
                .recv_timeout(Duration::from_secs(1))
                .unwrap()
                .unwrap()
                .status,
            CancellationAcknowledgementStatus::Cancelled
        );
        cancellation_thread.join().unwrap();
        assert!(!staging.exists());
    }

    #[test]
    fn unproven_process_group_latches_state_and_never_acknowledges_cancellation() {
        let directory = tempdir().unwrap();
        let runtime = Arc::new(LocalCodingSnapshotRuntime::open(directory.path(), true).unwrap());
        let job = job();
        runtime.admit(job.clone()).unwrap();
        let staging = {
            let mut active = runtime.active.lock().unwrap();
            let active = active.as_mut().unwrap();
            active.verifying = true;
            active.staging_directory.clone()
        };
        let instruction = cancellation(&job);
        let (sender, receiver) = mpsc::channel();
        let cancelling_runtime = Arc::clone(&runtime);
        let cancellation_thread = thread::spawn(move || {
            sender
                .send(cancelling_runtime.cancel(&instruction))
                .unwrap();
        });
        for _ in 0..100 {
            if runtime
                .active
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .cancellation_requested
                .load(Ordering::Acquire)
            {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        {
            let mut active = runtime.active.lock().unwrap();
            let active = active.as_mut().unwrap();
            assert!(active.cancellation_requested.load(Ordering::Acquire));
            active.verifying = false;
            active.effect_possible = true;
            active.terminal_failure = true;
            runtime.state_changed.notify_all();
        }

        assert!(matches!(
            receiver.recv_timeout(Duration::from_secs(1)).unwrap(),
            Err(LocalCodingSnapshotError::EffectPossible)
        ));
        cancellation_thread.join().unwrap();
        assert!(staging.exists());
        assert!(runtime.active.lock().unwrap().is_some());
        assert!(matches!(
            runtime.cancel(&cancellation(&job)),
            Err(LocalCodingSnapshotError::EffectPossible)
        ));
        assert!(matches!(
            runtime.shutdown(),
            Err(LocalCodingSnapshotError::EffectPossible)
        ));
        assert!(staging.exists());
        assert!(runtime.active.lock().unwrap().is_some());

        {
            let mut active = runtime.active.lock().unwrap();
            let active = active.as_mut().unwrap();
            active.effect_possible = false;
            active.terminal_failure = false;
        }
        runtime.shutdown().unwrap();
        assert!(!staging.exists());
    }

    #[test]
    fn cleanup_does_not_follow_an_interior_path_replacement_symlink() {
        use std::os::unix::fs::symlink;

        let directory = tempdir().unwrap();
        let normal = directory.path().join("normal");
        fs::create_dir(&normal).unwrap();
        fs::write(normal.join("owned.txt"), b"owned").unwrap();
        cleanup_tree(&normal).unwrap();
        assert!(!normal.exists());

        let victim = directory.path().join("victim");
        fs::create_dir(&victim).unwrap();
        fs::write(victim.join("survives.txt"), b"must survive").unwrap();
        let attempt = directory.path().join("attempt");
        let interior = attempt.join("interior");
        fs::create_dir_all(&interior).unwrap();
        fs::write(interior.join("owned.txt"), b"owned").unwrap();
        let replaced = directory.path().join("replaced-interior");
        fs::rename(&interior, &replaced).unwrap();
        symlink(&victim, &interior).unwrap();

        cleanup_tree(&attempt).unwrap();
        assert!(!attempt.exists());
        assert_eq!(
            fs::read(victim.join("survives.txt")).unwrap(),
            b"must survive"
        );
        assert_eq!(fs::read(replaced.join("owned.txt")).unwrap(), b"owned");
        cleanup_tree(&replaced).unwrap();
    }

    #[test]
    fn cancellation_verification_wait_timeout_returns_no_ack_and_retains_recovery_state() {
        let directory = tempdir().unwrap();
        let runtime = LocalCodingSnapshotRuntime::open(directory.path(), true).unwrap();
        let job = job();
        runtime.admit(job.clone()).unwrap();
        {
            let mut active = runtime.active.lock().unwrap();
            active.as_mut().unwrap().verifying = true;
        }

        let started = Instant::now();
        assert!(matches!(
            runtime.cancel(&cancellation(&job)),
            Err(LocalCodingSnapshotError::VerificationTimeout)
        ));
        assert!(started.elapsed() >= Duration::from_millis(150));
        {
            let active = runtime.active.lock().unwrap();
            let active = active
                .as_ref()
                .expect("timed-out state remains recoverable");
            assert!(active.verifying);
            assert!(active.cancellation_requested.load(Ordering::Acquire));
        }

        runtime.active.lock().unwrap().as_mut().unwrap().verifying = false;
        runtime.state_changed.notify_all();
        assert_eq!(
            runtime.cancel(&cancellation(&job)).unwrap().status,
            CancellationAcknowledgementStatus::Cancelled
        );
    }

    #[test]
    fn shutdown_verification_wait_timeout_fails_closed_and_retains_recovery_state() {
        let directory = tempdir().unwrap();
        let runtime = LocalCodingSnapshotRuntime::open(directory.path(), true).unwrap();
        runtime.admit(job()).unwrap();
        runtime.active.lock().unwrap().as_mut().unwrap().verifying = true;

        assert!(matches!(
            runtime.shutdown(),
            Err(LocalCodingSnapshotError::VerificationTimeout)
        ));
        assert!(runtime.active.lock().unwrap().is_some());

        runtime.active.lock().unwrap().as_mut().unwrap().verifying = false;
        runtime.state_changed.notify_all();
        runtime.shutdown().unwrap();
        assert!(runtime.active.lock().unwrap().is_none());
    }

    #[test]
    fn cancellation_during_verification_never_acknowledges_failed_cleanup() {
        let directory = tempdir().unwrap();
        let runtime = Arc::new(LocalCodingSnapshotRuntime::open(directory.path(), true).unwrap());
        let job = job();
        runtime.admit(job.clone()).unwrap();
        let staging = {
            let mut active = runtime.active.lock().unwrap();
            let active = active.as_mut().unwrap();
            active.verifying = true;
            active.staging_directory.clone()
        };
        fs::set_permissions(&staging, fs::Permissions::from_mode(0o000)).unwrap();
        let instruction = cancellation(&job);
        let (sender, receiver) = mpsc::channel();
        let cancelling_runtime = Arc::clone(&runtime);
        let cancellation_thread = thread::spawn(move || {
            sender
                .send(cancelling_runtime.cancel(&instruction))
                .unwrap();
        });
        for _ in 0..100 {
            if runtime
                .active
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .cancellation_requested
                .load(Ordering::Acquire)
            {
                break;
            }
            thread::sleep(Duration::from_millis(1));
        }
        {
            let mut active = runtime.active.lock().unwrap();
            active.as_mut().unwrap().verifying = false;
            runtime.state_changed.notify_all();
        }
        let failure = receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(matches!(failure, Err(LocalCodingSnapshotError::Io(_))));
        cancellation_thread.join().unwrap();
        assert!(staging.exists());
        assert!(runtime.active.lock().unwrap().is_some());

        fs::set_permissions(&staging, fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            runtime.cancel(&cancellation(&job)).unwrap().status,
            CancellationAcknowledgementStatus::Cancelled
        );
        assert!(!staging.exists());
    }

    #[test]
    fn terminal_failure_and_shutdown_preserve_state_until_cleanup_succeeds() {
        let directory = tempdir().unwrap();
        let runtime = LocalCodingSnapshotRuntime::open(directory.path(), true).unwrap();
        let job = job();
        runtime.admit(job.clone()).unwrap();
        let staging = runtime
            .active
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .staging_directory
            .clone();
        fs::set_permissions(&staging, fs::Permissions::from_mode(0o000)).unwrap();
        let invalid_bundle = [0_u8];
        assert!(matches!(
            runtime.accept_chunk(chunk(&job, 0, 1, &invalid_bundle)),
            Err(LocalCodingSnapshotError::Io(_))
        ));
        assert!(runtime.active.lock().unwrap().is_some());
        assert!(staging.exists());
        assert!(matches!(
            runtime.shutdown(),
            Err(LocalCodingSnapshotError::Io(_))
        ));
        assert!(runtime.active.lock().unwrap().is_some());

        fs::set_permissions(&staging, fs::Permissions::from_mode(0o700)).unwrap();
        runtime.shutdown().unwrap();
        assert!(runtime.active.lock().unwrap().is_none());
        assert!(!staging.exists());
    }
}
