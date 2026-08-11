use assemblywright_protocol::{
    CancellationAcknowledgement, CancellationAcknowledgementStatus, CancellationId,
    CancellationInstruction, DistributedEventBatch, DistributedEventCursor, FixtureJobResult,
    JobEnvelope, JobResultEnvelope, JobResultStatus, LocalCodingJobRequest, MlxJobResult,
    ProtocolError, MAX_JOB_RESULT_BYTES, MLX_GENERATE_TEXT_OPERATION, PROTOCOL_VERSION,
};
use fs2::FileExt;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
#[cfg(unix)]
use rustix::process::{waitid, Pid, WaitId, WaitIdOptions};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::Stdio;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
#[cfg(unix)]
use tokio::io::{AsyncReadExt, AsyncWriteExt};
#[cfg(unix)]
use tokio::process::{Child, Command};
use tokio::sync::Notify;
use uuid::Uuid;

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

pub const AGENT_SCHEMA_VERSION: i64 = 1;

/// Native-agent admission for the default-off coding-dispatch kernel. This
/// validates the exact path-free binding only; it deliberately grants no
/// repository access, process execution, provider call, or mutation runtime.
pub fn validate_local_coding_dispatch(
    job: &JobEnvelope,
) -> Result<LocalCodingJobRequest, ProtocolError> {
    job.validate_local_coding()
}

#[derive(Debug, thiserror::Error)]
pub enum FixtureRuntimeError {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("fixture jobs are disabled")]
    Disabled,
    #[error("fixture job is already active")]
    AlreadyActive,
    #[error("fixture job is not active")]
    NotActive,
    #[error("fixture job was cancelled")]
    Cancelled,
    #[error("fixture runtime state is unavailable")]
    Unavailable,
    #[error("fixture result serialization failed")]
    Serialization,
}

const MAX_MLX_STDOUT_BYTES: usize = MAX_JOB_RESULT_BYTES - 1024;
const MAX_MLX_EXECUTABLE_BYTES: u64 = 16 * 1024 * 1024;
const MLX_TERM_GRACE: Duration = Duration::from_millis(150);
const MLX_KILL_GRACE: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MlxRuntimeError {
    #[error(transparent)]
    Protocol(#[from] ProtocolError),
    #[error("MLX jobs are disabled")]
    Disabled,
    #[error("MLX job is already active")]
    AlreadyActive,
    #[error("MLX job is not active")]
    NotActive,
    #[error("MLX job was cancelled")]
    Cancelled,
    #[error("MLX job timed out")]
    Timeout,
    #[error("MLX backend configuration is invalid")]
    InvalidConfiguration,
    #[error("MLX backend failed")]
    BackendFailed,
    #[error("MLX backend output is invalid or exceeded its bound")]
    InvalidOutput,
    #[error("MLX process group could not be terminated and reaped")]
    CleanupFailed,
    #[error("MLX runtime is closed because a previous process group could not be proven reaped")]
    CleanupUnproven,
    #[error("MLX runtime state is unavailable")]
    Unavailable,
}

#[derive(Debug, Clone)]
pub struct MlxRuntimeConfig {
    executable_path: PathBuf,
    model_path: PathBuf,
    model_id: String,
    executable_sha256: [u8; 32],
    #[cfg(unix)]
    executable_device: u64,
    #[cfg(unix)]
    executable_inode: u64,
    #[cfg(unix)]
    model_device: u64,
    #[cfg(unix)]
    model_inode: u64,
}

impl MlxRuntimeConfig {
    pub fn validate(
        executable_path: PathBuf,
        model_path: PathBuf,
        model_id: String,
    ) -> Result<Self, MlxRuntimeError> {
        if !executable_path.is_absolute()
            || !model_path.is_absolute()
            || executable_path.file_name() != Some(OsStr::new("mlx_lm.generate"))
            || model_id.is_empty()
            || model_id.len() > assemblywright_protocol::MAX_MODEL_NAME_BYTES
            || model_id.trim() != model_id
        {
            return Err(MlxRuntimeError::InvalidConfiguration);
        }
        let executable_metadata = fs::symlink_metadata(&executable_path)
            .map_err(|_| MlxRuntimeError::InvalidConfiguration)?;
        let model_metadata =
            fs::symlink_metadata(&model_path).map_err(|_| MlxRuntimeError::InvalidConfiguration)?;
        if executable_metadata.file_type().is_symlink()
            || !executable_metadata.is_file()
            || model_metadata.file_type().is_symlink()
            || !model_metadata.is_dir()
        {
            return Err(MlxRuntimeError::InvalidConfiguration);
        }
        #[cfg(unix)]
        if executable_metadata.permissions().mode() & 0o111 == 0 {
            return Err(MlxRuntimeError::InvalidConfiguration);
        }
        let executable_path =
            fs::canonicalize(executable_path).map_err(|_| MlxRuntimeError::InvalidConfiguration)?;
        let model_path =
            fs::canonicalize(model_path).map_err(|_| MlxRuntimeError::InvalidConfiguration)?;
        let executable_metadata = fs::symlink_metadata(&executable_path)
            .map_err(|_| MlxRuntimeError::InvalidConfiguration)?;
        let model_metadata =
            fs::symlink_metadata(&model_path).map_err(|_| MlxRuntimeError::InvalidConfiguration)?;
        if executable_metadata.file_type().is_symlink()
            || !executable_metadata.is_file()
            || executable_metadata.len() > MAX_MLX_EXECUTABLE_BYTES
            || model_metadata.file_type().is_symlink()
            || !model_metadata.is_dir()
        {
            return Err(MlxRuntimeError::InvalidConfiguration);
        }
        let executable_sha256 = Sha256::digest(
            fs::read(&executable_path).map_err(|_| MlxRuntimeError::InvalidConfiguration)?,
        )
        .into();
        Ok(Self {
            executable_path,
            model_path,
            model_id,
            executable_sha256,
            #[cfg(unix)]
            executable_device: executable_metadata.dev(),
            #[cfg(unix)]
            executable_inode: executable_metadata.ino(),
            #[cfg(unix)]
            model_device: model_metadata.dev(),
            #[cfg(unix)]
            model_inode: model_metadata.ino(),
        })
    }

    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    fn revalidate_paths(&self) -> Result<(), MlxRuntimeError> {
        let executable_metadata = fs::symlink_metadata(&self.executable_path)
            .map_err(|_| MlxRuntimeError::InvalidConfiguration)?;
        let model_metadata = fs::symlink_metadata(&self.model_path)
            .map_err(|_| MlxRuntimeError::InvalidConfiguration)?;
        let executable_sha256: [u8; 32] = Sha256::digest(
            fs::read(&self.executable_path).map_err(|_| MlxRuntimeError::InvalidConfiguration)?,
        )
        .into();
        if executable_metadata.file_type().is_symlink()
            || !executable_metadata.is_file()
            || executable_metadata.len() > MAX_MLX_EXECUTABLE_BYTES
            || model_metadata.file_type().is_symlink()
            || !model_metadata.is_dir()
            || fs::canonicalize(&self.executable_path)
                .map_err(|_| MlxRuntimeError::InvalidConfiguration)?
                != self.executable_path
            || fs::canonicalize(&self.model_path)
                .map_err(|_| MlxRuntimeError::InvalidConfiguration)?
                != self.model_path
            || executable_sha256 != self.executable_sha256
        {
            return Err(MlxRuntimeError::InvalidConfiguration);
        }
        #[cfg(unix)]
        if executable_metadata.dev() != self.executable_device
            || executable_metadata.ino() != self.executable_inode
            || executable_metadata.permissions().mode() & 0o111 == 0
            || model_metadata.dev() != self.model_device
            || model_metadata.ino() != self.model_inode
        {
            return Err(MlxRuntimeError::InvalidConfiguration);
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct MlxJobRuntime {
    config: Option<Arc<MlxRuntimeConfig>>,
    active: Arc<Mutex<Option<ActiveMlxJob>>>,
    /// Latched when a process group could not be proven reaped. Sticky for the
    /// life of the process: the agent is app-supervised and default-off, so a
    /// restart is the intended reset rather than a self-clearing timer.
    cleanup_unproven: Arc<AtomicBool>,
}

#[derive(Clone)]
struct ActiveMlxJob {
    job: JobEnvelope,
    cancelled: Arc<AtomicBool>,
    process_group: Arc<std::sync::atomic::AtomicI32>,
    wake: Arc<Notify>,
    finished: Arc<AtomicBool>,
    finished_notify: Arc<Notify>,
}

impl MlxJobRuntime {
    pub fn new(config: Option<MlxRuntimeConfig>) -> Self {
        Self {
            config: config.map(Arc::new),
            active: Arc::new(Mutex::new(None)),
            cleanup_unproven: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Report a job that already has a definite verdict, without letting a slow
    /// or unprovable cleanup relabel it.
    ///
    /// `terminate_backend` failing means "I could not prove the process group is
    /// gone". That is a fail-closed condition for the *runtime*, not this job's
    /// outcome: a cancelled job is cancelled even when reaping was slow, and the
    /// safety rule is that cancellation dominates completion. Masking the verdict
    /// with `CleanupFailed` used to turn a cancellation into an internal error
    /// under load, which lost the one fact the caller needed.
    ///
    /// So the verdict is preserved and the cleanup failure is latched instead.
    /// The latch closes the runtime to new work, so an unproven reap still fails
    /// closed rather than being swallowed.
    fn resolve_cleanup(
        &self,
        cleanup: Result<(), MlxRuntimeError>,
        verdict: MlxRuntimeError,
    ) -> MlxRuntimeError {
        if cleanup.is_err() {
            self.cleanup_unproven.store(true, Ordering::SeqCst);
        }
        verdict
    }

    /// True once a process group could not be proven reaped. The runtime refuses
    /// new work in that state, because it cannot promise that a previous
    /// backend is no longer running or emitting output.
    pub fn cleanup_unproven(&self) -> bool {
        self.cleanup_unproven.load(Ordering::SeqCst)
    }

    pub async fn execute(&self, job: JobEnvelope) -> Result<JobResultEnvelope, MlxRuntimeError> {
        if self.cleanup_unproven() {
            return Err(MlxRuntimeError::CleanupUnproven);
        }
        let config = self.config.clone().ok_or(MlxRuntimeError::Disabled)?;
        let request = job.validate_mlx_reasoning()?;
        if job.selected_model != config.model_id {
            return Err(MlxRuntimeError::Protocol(ProtocolError::InvalidMlxJob));
        }
        let result_sequence = job
            .sequence
            .checked_add(1)
            .ok_or(MlxRuntimeError::Unavailable)?;
        let active = ActiveMlxJob {
            job: job.clone(),
            cancelled: Arc::new(AtomicBool::new(false)),
            process_group: Arc::new(std::sync::atomic::AtomicI32::new(0)),
            wake: Arc::new(Notify::new()),
            finished: Arc::new(AtomicBool::new(false)),
            finished_notify: Arc::new(Notify::new()),
        };
        {
            let mut slot = self
                .active
                .lock()
                .map_err(|_| MlxRuntimeError::Unavailable)?;
            if slot.is_some() {
                return Err(MlxRuntimeError::AlreadyActive);
            }
            *slot = Some(active.clone());
        }

        let outcome = self.run_backend(config, &request, &active).await;
        match outcome {
            Ok(output) => {
                let result = match build_mlx_result(&job, result_sequence, output) {
                    Ok(result) => result,
                    Err(error) => {
                        self.clear_active(&active);
                        self.finish_active(&active);
                        return Err(error);
                    }
                };
                let mut slot = self
                    .active
                    .lock()
                    .map_err(|_| MlxRuntimeError::Unavailable)?;
                if active.cancelled.load(Ordering::SeqCst)
                    || slot
                        .as_ref()
                        .is_none_or(|current| current.job.attempt_id != job.attempt_id)
                {
                    drop(slot);
                    self.finish_active(&active);
                    return Err(MlxRuntimeError::Cancelled);
                }
                *slot = None;
                drop(slot);
                self.finish_active(&active);
                Ok(result)
            }
            Err(error) => {
                self.clear_active(&active);
                self.finish_active(&active);
                Err(error)
            }
        }
    }

    pub async fn cancel(
        &self,
        instruction: &CancellationInstruction,
    ) -> Result<CancellationAcknowledgement, MlxRuntimeError> {
        if self.config.is_none() {
            return Err(MlxRuntimeError::Disabled);
        }
        let active = {
            let slot = self
                .active
                .lock()
                .map_err(|_| MlxRuntimeError::Unavailable)?;
            let active = slot.as_ref().cloned().ok_or(MlxRuntimeError::NotActive)?;
            instruction.validate_for_job(&active.job)?;
            active.cancelled.store(true, Ordering::SeqCst);
            active
        };
        #[cfg(unix)]
        {
            let process_group = active.process_group.load(Ordering::SeqCst);
            if process_group > 0 {
                signal_process_group(process_group, libc::SIGTERM)?;
            }
        }
        active.wake.notify_waiters();
        let finished = active.finished_notify.notified();
        if !active.finished.load(Ordering::SeqCst) {
            tokio::time::timeout(
                MLX_TERM_GRACE + MLX_KILL_GRACE + Duration::from_millis(250),
                finished,
            )
            .await
            .map_err(|_| MlxRuntimeError::CleanupFailed)?;
        }
        let acknowledgement = CancellationAcknowledgement {
            protocol_version: PROTOCOL_VERSION,
            connection_epoch: instruction.connection_epoch,
            sequence: instruction
                .sequence
                .checked_add(1)
                .ok_or(MlxRuntimeError::Unavailable)?,
            task_id: instruction.task_id,
            step_id: instruction.step_id,
            attempt_id: instruction.attempt_id,
            lease_id: instruction.lease_id,
            cancellation_id: instruction.cancellation_id,
            status: CancellationAcknowledgementStatus::Cancelled,
        };
        acknowledgement.validate_for_instruction(instruction)?;
        Ok(acknowledgement)
    }

    pub async fn shutdown_active(&self) -> Result<(), MlxRuntimeError> {
        let active = {
            let slot = self
                .active
                .lock()
                .map_err(|_| MlxRuntimeError::Unavailable)?;
            let Some(active) = slot.as_ref().cloned() else {
                return Ok(());
            };
            active.cancelled.store(true, Ordering::SeqCst);
            active
        };
        #[cfg(unix)]
        {
            let process_group = active.process_group.load(Ordering::SeqCst);
            if process_group > 0 {
                signal_process_group(process_group, libc::SIGTERM)?;
            }
        }
        active.wake.notify_waiters();
        let finished = active.finished_notify.notified();
        if !active.finished.load(Ordering::SeqCst) {
            tokio::time::timeout(
                MLX_TERM_GRACE + MLX_KILL_GRACE + MLX_KILL_GRACE + Duration::from_millis(500),
                finished,
            )
            .await
            .map_err(|_| MlxRuntimeError::CleanupFailed)?;
        }
        Ok(())
    }

    fn finish_active(&self, active: &ActiveMlxJob) {
        active.finished.store(true, Ordering::SeqCst);
        active.finished_notify.notify_one();
    }

    fn clear_active(&self, active: &ActiveMlxJob) {
        if let Ok(mut slot) = self.active.lock() {
            if slot
                .as_ref()
                .is_some_and(|current| current.job.attempt_id == active.job.attempt_id)
            {
                *slot = None;
            }
        }
    }

    #[cfg(unix)]
    async fn run_backend(
        &self,
        config: Arc<MlxRuntimeConfig>,
        request: &assemblywright_protocol::MlxJobRequest,
        active: &ActiveMlxJob,
    ) -> Result<String, MlxRuntimeError> {
        config.revalidate_paths()?;
        let mut command = Command::new(&config.executable_path);
        command
            .arg("--model")
            .arg(&config.model_path)
            .arg("--prompt")
            .arg("-")
            .arg("--max-tokens")
            .arg(request.max_tokens.to_string())
            .arg("--temp")
            .arg(format!("{:.3}", request.temperature_milli as f64 / 1000.0))
            .arg("--seed")
            .arg("0")
            .arg("--verbose")
            .arg("false")
            .env_clear()
            .env("HF_HUB_OFFLINE", "1")
            .env("TRANSFORMERS_OFFLINE", "1")
            .env("HF_HUB_DISABLE_TELEMETRY", "1")
            .env("DO_NOT_TRACK", "1")
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env("TOKENIZERS_PARALLELISM", "false")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .process_group(0);
        let mut child = command
            .spawn()
            .map_err(|_| MlxRuntimeError::BackendFailed)?;
        let process_group = i32::try_from(child.id().ok_or(MlxRuntimeError::BackendFailed)?)
            .map_err(|_| MlxRuntimeError::BackendFailed)?;
        let leader_pid = Pid::from_raw(process_group).ok_or(MlxRuntimeError::BackendFailed)?;
        active.process_group.store(process_group, Ordering::SeqCst);
        if active.cancelled.load(Ordering::SeqCst) {
            let cleanup = terminate_process_group(&mut child, process_group).await;
            return Err(self.resolve_cleanup(cleanup, MlxRuntimeError::Cancelled));
        }
        let mut stdin = child.stdin.take().ok_or(MlxRuntimeError::BackendFailed)?;
        let stdout = child.stdout.take().ok_or(MlxRuntimeError::BackendFailed)?;
        let prompt = request.prompt.clone();
        let mut writer = tokio::spawn(async move {
            stdin.write_all(prompt.as_bytes()).await?;
            stdin.shutdown().await
        });
        let mut reader = tokio::spawn(async move {
            let mut output = Vec::new();
            stdout
                .take((MAX_MLX_STDOUT_BYTES + 1) as u64)
                .read_to_end(&mut output)
                .await?;
            Ok::<Vec<u8>, std::io::Error>(output)
        });
        let deadline = Duration::from_millis(
            active
                .job
                .deadline_after_ms
                .min(active.job.lease_duration_ms),
        );
        let timeout = tokio::time::sleep(deadline);
        tokio::pin!(timeout);
        let mut writer_done = false;
        let mut reader_done = false;
        let mut output: Option<Vec<u8>> = None;
        let status = loop {
            tokio::select! {
                leader = observe_leader_exit(leader_pid) => match leader {
                    Ok(()) => break reap_exited_process_group(&mut child, process_group).await?,
                    Err(_) => {
                        let cleanup = terminate_backend(
                            &mut child,
                            process_group,
                            &mut writer,
                            writer_done,
                            &mut reader,
                            reader_done,
                        ).await;
                        return Err(self.resolve_cleanup(cleanup, MlxRuntimeError::BackendFailed));
                    }
                },
                result = &mut writer, if !writer_done => {
                    writer_done = true;
                    if !matches!(result, Ok(Ok(()))) {
                        let cleanup = terminate_backend(
                            &mut child,
                            process_group,
                            &mut writer,
                            writer_done,
                            &mut reader,
                            reader_done,
                        ).await;
                        return Err(self.resolve_cleanup(cleanup, MlxRuntimeError::BackendFailed));
                    }
                }
                result = &mut reader, if !reader_done => {
                    reader_done = true;
                    let bytes = match result {
                        Ok(Ok(bytes)) => bytes,
                        _ => {
                            let cleanup = terminate_backend(
                                &mut child,
                                process_group,
                                &mut writer,
                                writer_done,
                                &mut reader,
                                reader_done,
                            ).await;
                            return Err(self.resolve_cleanup(cleanup, MlxRuntimeError::BackendFailed));
                        }
                    };
                    if bytes.len() > MAX_MLX_STDOUT_BYTES {
                        let cleanup = terminate_backend(
                            &mut child,
                            process_group,
                            &mut writer,
                            writer_done,
                            &mut reader,
                            reader_done,
                        ).await;
                        return Err(self.resolve_cleanup(cleanup, MlxRuntimeError::InvalidOutput));
                    }
                    output = Some(bytes);
                }
                _ = active.wake.notified() => {
                    let cleanup = terminate_backend(
                        &mut child,
                        process_group,
                        &mut writer,
                        writer_done,
                        &mut reader,
                        reader_done,
                    ).await;
                    return Err(self.resolve_cleanup(cleanup, MlxRuntimeError::Cancelled));
                }
                _ = &mut timeout => {
                    let cleanup = terminate_backend(
                        &mut child,
                        process_group,
                        &mut writer,
                        writer_done,
                        &mut reader,
                        reader_done,
                    ).await;
                    return Err(self.resolve_cleanup(cleanup, MlxRuntimeError::Timeout));
                }
            }
        };
        if active.cancelled.load(Ordering::SeqCst) {
            let cleanup = terminate_backend(
                &mut child,
                process_group,
                &mut writer,
                writer_done,
                &mut reader,
                reader_done,
            )
            .await;
            return Err(self.resolve_cleanup(cleanup, MlxRuntimeError::Cancelled));
        }
        if !status.success() {
            let cleanup = terminate_backend(
                &mut child,
                process_group,
                &mut writer,
                writer_done,
                &mut reader,
                reader_done,
            )
            .await;
            return Err(self.resolve_cleanup(cleanup, MlxRuntimeError::BackendFailed));
        }
        if !writer_done {
            match tokio::time::timeout(Duration::from_secs(1), &mut writer).await {
                Ok(Ok(Ok(()))) => writer_done = true,
                Err(_) => {
                    let cleanup = terminate_backend(
                        &mut child,
                        process_group,
                        &mut writer,
                        false,
                        &mut reader,
                        reader_done,
                    )
                    .await;
                    return Err(self.resolve_cleanup(cleanup, MlxRuntimeError::BackendFailed));
                }
                Ok(_) => {
                    let cleanup = terminate_backend(
                        &mut child,
                        process_group,
                        &mut writer,
                        true,
                        &mut reader,
                        reader_done,
                    )
                    .await;
                    return Err(self.resolve_cleanup(cleanup, MlxRuntimeError::BackendFailed));
                }
            }
        }
        let output = match output {
            Some(output) => output,
            None => match tokio::time::timeout(Duration::from_secs(1), &mut reader).await {
                Ok(Ok(Ok(output))) => output,
                Err(_) => {
                    let cleanup = terminate_backend(
                        &mut child,
                        process_group,
                        &mut writer,
                        writer_done,
                        &mut reader,
                        false,
                    )
                    .await;
                    return Err(self.resolve_cleanup(cleanup, MlxRuntimeError::BackendFailed));
                }
                Ok(_) => {
                    let cleanup = terminate_backend(
                        &mut child,
                        process_group,
                        &mut writer,
                        writer_done,
                        &mut reader,
                        true,
                    )
                    .await;
                    return Err(self.resolve_cleanup(cleanup, MlxRuntimeError::BackendFailed));
                }
            },
        };
        if output.len() > MAX_MLX_STDOUT_BYTES {
            return Err(MlxRuntimeError::InvalidOutput);
        }
        if process_group_exists(process_group)? {
            terminate_process_group(&mut child, process_group).await?;
            return Err(MlxRuntimeError::BackendFailed);
        }
        let output = String::from_utf8(output).map_err(|_| MlxRuntimeError::InvalidOutput)?;
        let output = output.trim().to_string();
        if output.is_empty() {
            return Err(MlxRuntimeError::InvalidOutput);
        }
        Ok(output)
    }

    #[cfg(not(unix))]
    async fn run_backend(
        &self,
        _config: Arc<MlxRuntimeConfig>,
        _request: &assemblywright_protocol::MlxJobRequest,
        _active: &ActiveMlxJob,
    ) -> Result<String, MlxRuntimeError> {
        Err(MlxRuntimeError::BackendFailed)
    }
}

fn build_mlx_result(
    job: &JobEnvelope,
    sequence: u64,
    output: String,
) -> Result<JobResultEnvelope, MlxRuntimeError> {
    let payload = serde_json::to_value(MlxJobResult {
        operation: MLX_GENERATE_TEXT_OPERATION.to_string(),
        output,
        model: job.selected_model.clone(),
    })
    .map_err(|_| MlxRuntimeError::InvalidOutput)?;
    let payload_bytes = serde_json::to_vec(&payload).map_err(|_| MlxRuntimeError::InvalidOutput)?;
    let result = JobResultEnvelope {
        protocol_version: PROTOCOL_VERSION,
        connection_epoch: job.connection_epoch,
        sequence,
        task_id: job.task_id,
        step_id: job.step_id,
        attempt_id: job.attempt_id,
        lease_id: job.lease_id,
        cancellation_id: job.cancellation_id,
        status: JobResultStatus::Completed,
        context_sha256: job.context_sha256,
        payload_sha256: Sha256::digest(&payload_bytes).into(),
        payload,
    };
    result.validate_mlx_reasoning_result(job)?;
    Ok(result)
}

#[cfg(unix)]
fn signal_process_group(process_group: i32, signal: i32) -> Result<(), MlxRuntimeError> {
    let result = unsafe { libc::kill(-process_group, signal) };
    let error = std::io::Error::last_os_error().raw_os_error();
    if result == 0 || error == Some(libc::ESRCH) || error == Some(libc::EPERM) {
        // macOS can report EPERM when only an owned zombie leader remains.
        // Post-reap probe-only confirmation still fails closed if the group
        // remains present or unsignalable.
        Ok(())
    } else {
        Err(MlxRuntimeError::CleanupFailed)
    }
}

#[cfg(unix)]
fn process_group_exists(process_group: i32) -> Result<bool, MlxRuntimeError> {
    let result = unsafe { libc::kill(-process_group, 0) };
    if result == 0 {
        Ok(true)
    } else if std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
        Ok(false)
    } else {
        Err(MlxRuntimeError::CleanupFailed)
    }
}

#[cfg(unix)]
async fn terminate_process_group(
    child: &mut Child,
    process_group: i32,
) -> Result<(), MlxRuntimeError> {
    signal_process_group(process_group, libc::SIGTERM)?;
    tokio::time::sleep(MLX_TERM_GRACE).await;
    signal_process_group(process_group, libc::SIGKILL)?;
    tokio::time::timeout(MLX_KILL_GRACE, child.wait())
        .await
        .map_err(|_| MlxRuntimeError::CleanupFailed)?
        .map_err(|_| MlxRuntimeError::CleanupFailed)?;
    confirm_process_group_absent(process_group).await
}

#[cfg(unix)]
async fn reap_exited_process_group(
    child: &mut Child,
    process_group: i32,
) -> Result<std::process::ExitStatus, MlxRuntimeError> {
    // The leader is observed with WNOWAIT, so its PID/PGID cannot be recycled
    // while the final group signals are issued.
    signal_process_group(process_group, libc::SIGTERM)?;
    tokio::time::sleep(Duration::from_millis(25)).await;
    signal_process_group(process_group, libc::SIGKILL)?;
    let status = tokio::time::timeout(MLX_KILL_GRACE, child.wait())
        .await
        .map_err(|_| MlxRuntimeError::CleanupFailed)?
        .map_err(|_| MlxRuntimeError::CleanupFailed)?;
    confirm_process_group_absent(process_group).await?;
    Ok(status)
}

#[cfg(unix)]
async fn confirm_process_group_absent(process_group: i32) -> Result<(), MlxRuntimeError> {
    let deadline = tokio::time::Instant::now() + MLX_KILL_GRACE;
    while process_group_exists(process_group)? {
        if tokio::time::Instant::now() >= deadline {
            return Err(MlxRuntimeError::CleanupFailed);
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Ok(())
}

#[cfg(unix)]
async fn observe_leader_exit(leader_pid: Pid) -> Result<(), MlxRuntimeError> {
    loop {
        if waitid(
            WaitId::Pid(leader_pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOHANG | WaitIdOptions::NOWAIT,
        )
        .map_err(|_| MlxRuntimeError::BackendFailed)?
        .is_some()
        {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
}

#[cfg(unix)]
async fn terminate_backend(
    child: &mut Child,
    process_group: i32,
    writer: &mut tokio::task::JoinHandle<std::io::Result<()>>,
    writer_done: bool,
    reader: &mut tokio::task::JoinHandle<std::io::Result<Vec<u8>>>,
    reader_done: bool,
) -> Result<(), MlxRuntimeError> {
    if !writer_done {
        writer.abort();
    }
    if !reader_done {
        reader.abort();
    }
    let termination = terminate_process_group(child, process_group).await;
    if !writer_done {
        let _ = writer.await;
    }
    if !reader_done {
        let _ = reader.await;
    }
    termination
}

#[derive(Clone)]
pub struct FixtureJobRuntime {
    enabled: bool,
    active: Arc<Mutex<HashMap<CancellationId, ActiveFixtureJob>>>,
}

#[derive(Clone)]
struct ActiveFixtureJob {
    job: JobEnvelope,
    cancelled: Arc<AtomicBool>,
    notify: Arc<Notify>,
}

impl FixtureJobRuntime {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            active: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn execute(
        &self,
        job: JobEnvelope,
    ) -> Result<JobResultEnvelope, FixtureRuntimeError> {
        if !self.enabled {
            return Err(FixtureRuntimeError::Disabled);
        }
        let request = job.validate_fixture_reasoning()?;
        let active = ActiveFixtureJob {
            job: job.clone(),
            cancelled: Arc::new(AtomicBool::new(false)),
            notify: Arc::new(Notify::new()),
        };
        {
            let mut jobs = self
                .active
                .lock()
                .map_err(|_| FixtureRuntimeError::Unavailable)?;
            if !jobs.is_empty() {
                return Err(FixtureRuntimeError::AlreadyActive);
            }
            jobs.insert(job.cancellation_id, active.clone());
        }

        if request.delay_ms > 0 {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(request.delay_ms)) => {}
                _ = active.notify.notified() => {}
            }
        }
        let cancelled = {
            let mut jobs = self
                .active
                .lock()
                .map_err(|_| FixtureRuntimeError::Unavailable)?;
            let cancelled = active.cancelled.load(Ordering::SeqCst);
            jobs.remove(&job.cancellation_id);
            cancelled
        };
        if cancelled {
            return Err(FixtureRuntimeError::Cancelled);
        }

        let payload = serde_json::to_value(FixtureJobResult::synthetic_echo(request.input))
            .map_err(|_| FixtureRuntimeError::Serialization)?;
        let payload_bytes =
            serde_json::to_vec(&payload).map_err(|_| FixtureRuntimeError::Serialization)?;
        let result = JobResultEnvelope {
            protocol_version: PROTOCOL_VERSION,
            connection_epoch: job.connection_epoch,
            sequence: job
                .sequence
                .checked_add(1)
                .ok_or(FixtureRuntimeError::Unavailable)?,
            task_id: job.task_id,
            step_id: job.step_id,
            attempt_id: job.attempt_id,
            lease_id: job.lease_id,
            cancellation_id: job.cancellation_id,
            status: JobResultStatus::Completed,
            context_sha256: job.context_sha256,
            payload_sha256: Sha256::digest(&payload_bytes).into(),
            payload,
        };
        result.validate_for_job(&job)?;
        Ok(result)
    }

    pub fn cancel(
        &self,
        instruction: &CancellationInstruction,
    ) -> Result<CancellationAcknowledgement, FixtureRuntimeError> {
        if !self.enabled {
            return Err(FixtureRuntimeError::Disabled);
        }
        let active = {
            let mut jobs = self
                .active
                .lock()
                .map_err(|_| FixtureRuntimeError::Unavailable)?;
            let active = jobs
                .get(&instruction.cancellation_id)
                .cloned()
                .ok_or(FixtureRuntimeError::NotActive)?;
            instruction.validate_for_job(&active.job)?;
            active.cancelled.store(true, Ordering::SeqCst);
            jobs.remove(&instruction.cancellation_id);
            active
        };
        active.notify.notify_waiters();
        let acknowledgement = CancellationAcknowledgement {
            protocol_version: PROTOCOL_VERSION,
            connection_epoch: instruction.connection_epoch,
            sequence: instruction
                .sequence
                .checked_add(1)
                .ok_or(FixtureRuntimeError::Unavailable)?,
            task_id: instruction.task_id,
            step_id: instruction.step_id,
            attempt_id: instruction.attempt_id,
            lease_id: instruction.lease_id,
            cancellation_id: instruction.cancellation_id,
            status: CancellationAcknowledgementStatus::Cancelled,
        };
        acknowledgement.validate_for_instruction(instruction)?;
        Ok(acknowledgement)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error(transparent)]
    Protocol(#[from] assemblywright_protocol::ProtocolError),
    #[error("agent storage error: {0}")]
    Storage(#[from] rusqlite::Error),
    #[error("agent filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("agent data directory must be an absolute owner-only directory")]
    UnsafeDataDirectory,
    #[error("another assemblywright-agent process already owns {lock_path}")]
    OwnerAlreadyActive { lock_path: PathBuf },
    #[error("event batch belongs to a different master stream")]
    EventStreamMismatch,
    #[error("event batch does not continue from the accepted durable cursor")]
    EventCursorGap,
    #[error("stored agent cursor is invalid")]
    InvalidStoredCursor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCursorSnapshot {
    pub cursor: Option<DistributedEventCursor>,
    pub updated_at_ms: Option<u64>,
}

pub struct AgentCursorStore {
    _owner_lock: File,
    connection: Connection,
}

impl Drop for AgentCursorStore {
    fn drop(&mut self) {
        // Release explicitly so an immediate same-process reopen does not
        // depend on platform-specific close/unlock ordering.
        let _ = FileExt::unlock(&self._owner_lock);
    }
}

impl AgentCursorStore {
    pub fn open(data_dir: impl AsRef<Path>) -> Result<Self, AgentError> {
        let data_dir = data_dir.as_ref();
        prepare_data_directory(data_dir)?;
        let lock_path = data_dir.join("agent.owner.lock");
        let owner_lock = open_owner_lock(&lock_path)?;
        owner_lock
            .try_lock_exclusive()
            .map_err(|_| AgentError::OwnerAlreadyActive {
                lock_path: lock_path.clone(),
            })?;
        let database_path = data_dir.join("agent.sqlite3");
        let connection = Connection::open(database_path)?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;\nPRAGMA busy_timeout = 5000;\nPRAGMA synchronous = FULL;",
        )?;
        migrate(&connection)?;
        Ok(Self {
            _owner_lock: owner_lock,
            connection,
        })
    }

    pub fn snapshot(&self) -> Result<AgentCursorSnapshot, AgentError> {
        let stored = self
            .connection
            .query_row(
                "SELECT stream_id, sequence, updated_at_ms FROM agent_event_cursor WHERE singleton = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<i64>>(2)?,
                    ))
                },
            )
            .optional()?
            .ok_or(AgentError::InvalidStoredCursor)?;
        snapshot_from_stored(stored)
    }

    pub fn accept_batch(
        &mut self,
        batch: &DistributedEventBatch,
        now_ms: u64,
    ) -> Result<AgentCursorSnapshot, AgentError> {
        batch.validate()?;
        if now_ms == 0 {
            return Err(AgentError::InvalidStoredCursor);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let stored = tx.query_row(
            "SELECT stream_id, sequence, updated_at_ms FROM agent_event_cursor WHERE singleton = 1",
            [],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                ))
            },
        )?;
        let current = snapshot_from_stored(stored)?;
        match current.cursor {
            Some(cursor) => {
                if cursor.stream_id != batch.stream_id {
                    return Err(AgentError::EventStreamMismatch);
                }
                if cursor.sequence != batch.after_sequence {
                    return Err(AgentError::EventCursorGap);
                }
            }
            None if batch.after_sequence != 0 => return Err(AgentError::EventCursorGap),
            None => {}
        }
        tx.execute(
            "UPDATE agent_event_cursor\n\
             SET stream_id = ?1, sequence = ?2, updated_at_ms = ?3\n\
             WHERE singleton = 1",
            params![
                batch.stream_id.to_string(),
                i64::try_from(batch.next_sequence).map_err(|_| AgentError::InvalidStoredCursor)?,
                i64::try_from(now_ms).map_err(|_| AgentError::InvalidStoredCursor)?,
            ],
        )?;
        tx.commit()?;
        Ok(AgentCursorSnapshot {
            cursor: Some(DistributedEventCursor {
                stream_id: batch.stream_id,
                sequence: batch.next_sequence,
            }),
            updated_at_ms: Some(now_ms),
        })
    }
}

fn migrate(connection: &Connection) -> Result<(), AgentError> {
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version == 0 {
        connection.execute_batch(
            "BEGIN IMMEDIATE;\n\
             CREATE TABLE agent_event_cursor (\n\
               singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),\n\
               stream_id TEXT,\n\
               sequence INTEGER NOT NULL CHECK (sequence >= 0),\n\
               updated_at_ms INTEGER,\n\
               CHECK ((stream_id IS NULL AND sequence = 0 AND updated_at_ms IS NULL) OR\n\
                      (stream_id IS NOT NULL AND updated_at_ms IS NOT NULL))\n\
             );\n\
             INSERT INTO agent_event_cursor (singleton, stream_id, sequence, updated_at_ms)\n\
               VALUES (1, NULL, 0, NULL);\n\
             PRAGMA user_version = 1;\n\
             COMMIT;",
        )?;
        return Ok(());
    }
    if version != AGENT_SCHEMA_VERSION {
        return Err(AgentError::InvalidStoredCursor);
    }
    Ok(())
}

fn snapshot_from_stored(
    (stream_id, sequence, updated_at_ms): (Option<String>, i64, Option<i64>),
) -> Result<AgentCursorSnapshot, AgentError> {
    let sequence = u64::try_from(sequence).map_err(|_| AgentError::InvalidStoredCursor)?;
    let updated_at_ms = updated_at_ms
        .map(|value| u64::try_from(value).map_err(|_| AgentError::InvalidStoredCursor))
        .transpose()?;
    let cursor = match stream_id {
        Some(stream_id) => {
            let stream_id =
                Uuid::parse_str(&stream_id).map_err(|_| AgentError::InvalidStoredCursor)?;
            if stream_id.is_nil() || updated_at_ms.is_none() {
                return Err(AgentError::InvalidStoredCursor);
            }
            Some(DistributedEventCursor {
                stream_id,
                sequence,
            })
        }
        None if sequence == 0 && updated_at_ms.is_none() => None,
        None => return Err(AgentError::InvalidStoredCursor),
    };
    Ok(AgentCursorSnapshot {
        cursor,
        updated_at_ms,
    })
}

fn prepare_data_directory(data_dir: &Path) -> Result<(), AgentError> {
    if !data_dir.is_absolute() {
        return Err(AgentError::UnsafeDataDirectory);
    }
    match fs::symlink_metadata(data_dir) {
        Ok(metadata) => validate_data_directory(&metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir(data_dir)?;
            #[cfg(unix)]
            fs::set_permissions(data_dir, fs::Permissions::from_mode(0o700))?;
            validate_data_directory(&fs::symlink_metadata(data_dir)?)
        }
        Err(error) => Err(error.into()),
    }
}

fn validate_data_directory(metadata: &fs::Metadata) -> Result<(), AgentError> {
    if !metadata.file_type().is_dir() {
        return Err(AgentError::UnsafeDataDirectory);
    }
    #[cfg(unix)]
    if metadata.uid() != unsafe { libc::geteuid() }
        || metadata.permissions().mode() & 0o7777 != 0o700
    {
        return Err(AgentError::UnsafeDataDirectory);
    }
    Ok(())
}

fn open_owner_lock(path: &Path) -> Result<File, AgentError> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        options.mode(0o600);
        options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    }
    let file = options.open(path)?;
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        let metadata = file.metadata()?;
        if !metadata.file_type().is_file()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.nlink() != 1
            || metadata.permissions().mode() & 0o7777 != 0o600
        {
            return Err(AgentError::UnsafeDataDirectory);
        }
    }
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use assemblywright_protocol::{
        AttemptId, CancellationInstruction, ContextHandlingPolicy, DistributedEvent,
        DistributedEventKind, LeaseId, Sensitivity, StepId, TaskId, CANCELLATION_ACK_DEADLINE_MS,
        FIXTURE_REASONING_CAPABILITY_ID, FIXTURE_REASONING_MODEL, MAX_FIXTURE_INPUT_BYTES,
        MLX_REASONING_CAPABILITY_ID, PROTOCOL_VERSION,
    };
    use serde_json::json;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    fn batch(stream_id: Uuid, after_sequence: u64, next_sequence: u64) -> DistributedEventBatch {
        let events = if next_sequence == after_sequence {
            Vec::new()
        } else {
            vec![DistributedEvent {
                protocol_version: PROTOCOL_VERSION,
                cursor: DistributedEventCursor {
                    stream_id,
                    sequence: next_sequence,
                },
                occurred_at_ms: 1_000,
                kind: DistributedEventKind::StepQueued,
                task_id: Some(TaskId::new(Uuid::new_v4())),
                step_id: Some(StepId::new(Uuid::new_v4())),
                device_id: None,
                connection_epoch: None,
            }]
        };
        DistributedEventBatch {
            protocol_version: PROTOCOL_VERSION,
            stream_id,
            after_sequence,
            next_sequence,
            events,
            has_more: false,
        }
    }

    fn fixture_job(delay_ms: u64, input: &str) -> JobEnvelope {
        let context = json!({
            "operation": "synthetic_echo",
            "input": input,
            "delay_ms": delay_ms
        });
        JobEnvelope {
            protocol_version: PROTOCOL_VERSION,
            connection_epoch: 7,
            sequence: 1,
            task_id: TaskId::new(Uuid::new_v4()),
            step_id: StepId::new(Uuid::new_v4()),
            attempt_id: AttemptId::new(Uuid::new_v4()),
            lease_id: LeaseId::new(Uuid::new_v4()),
            cancellation_id: CancellationId::new(Uuid::new_v4()),
            capability_id: FIXTURE_REASONING_CAPABILITY_ID.to_string(),
            selected_model: FIXTURE_REASONING_MODEL.to_string(),
            sensitivity: Sensitivity::Public,
            context_handling: ContextHandlingPolicy::EphemeralNoRetention,
            lease_duration_ms: 60_000,
            deadline_after_ms: 60_000,
            context_sha256: Sha256::digest(serde_json::to_vec(&context).unwrap()).into(),
            context,
        }
    }

    fn cancellation(job: &JobEnvelope) -> CancellationInstruction {
        CancellationInstruction {
            protocol_version: PROTOCOL_VERSION,
            connection_epoch: job.connection_epoch,
            sequence: job.sequence + 1,
            task_id: job.task_id,
            step_id: job.step_id,
            attempt_id: job.attempt_id,
            lease_id: job.lease_id,
            cancellation_id: job.cancellation_id,
            deadline_after_ms: CANCELLATION_ACK_DEADLINE_MS,
        }
    }

    fn mlx_job(prompt: &str) -> JobEnvelope {
        let context = json!({
            "operation": "generate_text",
            "prompt": prompt,
            "max_tokens": 32,
            "temperature_milli": 700
        });
        JobEnvelope {
            protocol_version: PROTOCOL_VERSION,
            connection_epoch: 8,
            sequence: 1,
            task_id: TaskId::new(Uuid::new_v4()),
            step_id: StepId::new(Uuid::new_v4()),
            attempt_id: AttemptId::new(Uuid::new_v4()),
            lease_id: LeaseId::new(Uuid::new_v4()),
            cancellation_id: CancellationId::new(Uuid::new_v4()),
            capability_id: MLX_REASONING_CAPABILITY_ID.to_string(),
            selected_model: "local-mlx-model".to_string(),
            sensitivity: Sensitivity::Public,
            context_handling: ContextHandlingPolicy::EphemeralNoRetention,
            lease_duration_ms: 60_000,
            deadline_after_ms: 60_000,
            context_sha256: Sha256::digest(serde_json::to_vec(&context).unwrap()).into(),
            context,
        }
    }

    #[cfg(unix)]
    fn mlx_runtime(script: &str) -> (tempfile::TempDir, MlxJobRuntime) {
        let directory = tempdir().expect("MLX runtime fixture");
        let executable = directory.path().join("mlx_lm.generate");
        let model = directory.path().join("model");
        fs::create_dir(&model).expect("create model directory");
        fs::write(&executable, script).expect("write exact MLX executable fixture");
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
            .expect("make MLX fixture executable");
        let config = MlxRuntimeConfig::validate(executable, model, "local-mlx-model".to_string())
            .expect("validate MLX fixture config");
        (directory, MlxJobRuntime::new(Some(config)))
    }

    #[test]
    fn agent_cursor_acceptance_is_durable_and_fail_closed() {
        let directory = tempdir().expect("temporary parent");
        let data_dir = directory.path().join("agent");
        let stream_id = Uuid::new_v4();
        let mut store = AgentCursorStore::open(&data_dir).expect("open agent store");
        assert_eq!(
            store.snapshot().expect("empty cursor"),
            AgentCursorSnapshot {
                cursor: None,
                updated_at_ms: None
            }
        );
        store
            .accept_batch(&batch(stream_id, 0, 1), 2_000)
            .expect("accept first event");
        assert!(matches!(
            store.accept_batch(&batch(stream_id, 0, 1), 2_001),
            Err(AgentError::EventCursorGap)
        ));
        drop(store);

        let mut reopened = AgentCursorStore::open(&data_dir).expect("reopen agent store");
        assert_eq!(
            reopened.snapshot().expect("durable cursor").cursor,
            Some(DistributedEventCursor {
                stream_id,
                sequence: 1
            })
        );
        assert!(matches!(
            reopened.accept_batch(&batch(Uuid::new_v4(), 1, 2), 3_000),
            Err(AgentError::EventStreamMismatch)
        ));
        reopened
            .accept_batch(&batch(stream_id, 1, 2), 3_001)
            .expect("accept exact successor");
    }

    #[tokio::test]
    async fn fixture_runtime_is_default_off_and_accepts_only_exact_bounded_contract() {
        let job = fixture_job(0, "fixture");
        assert!(matches!(
            FixtureJobRuntime::new(false).execute(job.clone()).await,
            Err(FixtureRuntimeError::Disabled)
        ));
        let result = FixtureJobRuntime::new(true)
            .execute(job.clone())
            .await
            .expect("execute fixed fixture");
        assert_eq!(result.payload["output"], "fixture");
        assert_eq!(result.payload["synthetic"], true);

        let mut wrong_model = job.clone();
        wrong_model.selected_model = "mlx-real-model".to_string();
        assert!(matches!(
            FixtureJobRuntime::new(true).execute(wrong_model).await,
            Err(FixtureRuntimeError::Protocol(_))
        ));
        let oversized = "x".repeat(MAX_FIXTURE_INPUT_BYTES + 1);
        assert!(matches!(
            FixtureJobRuntime::new(true)
                .execute(fixture_job(0, &oversized))
                .await,
            Err(FixtureRuntimeError::Protocol(_))
        ));
        let mut malformed = job;
        malformed.context =
            json!({"operation":"synthetic_echo","input":"x","delay_ms":0,"extra":true});
        malformed.context_sha256 =
            Sha256::digest(serde_json::to_vec(&malformed.context).unwrap()).into();
        assert!(matches!(
            FixtureJobRuntime::new(true).execute(malformed).await,
            Err(FixtureRuntimeError::Protocol(_))
        ));
    }

    #[tokio::test]
    async fn cancellation_suppresses_late_fixture_output() {
        let runtime = FixtureJobRuntime::new(true);
        let job = fixture_job(5_000, "must-not-escape");
        let execution = {
            let runtime = runtime.clone();
            let job = job.clone();
            tokio::spawn(async move { runtime.execute(job).await })
        };
        tokio::task::yield_now().await;
        let acknowledgement = runtime
            .cancel(&cancellation(&job))
            .expect("cancel active fixture");
        assert_eq!(
            acknowledgement.status,
            CancellationAcknowledgementStatus::Cancelled
        );
        assert!(matches!(
            execution.await.expect("join fixture"),
            Err(FixtureRuntimeError::Cancelled)
        ));
        assert!(matches!(
            runtime.cancel(&cancellation(&job)),
            Err(FixtureRuntimeError::NotActive)
        ));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn mlx_runtime_uses_exact_args_stdin_and_minimal_offline_environment() {
        let (_directory, runtime) = mlx_runtime(
            r#"#!/bin/sh
set -eu
[ "$1" = "--model" ] && [ -d "$2" ]
[ "$3" = "--prompt" ] && [ "$4" = "-" ]
[ "$5" = "--max-tokens" ] && [ "$6" = "32" ]
[ "$7" = "--temp" ] && [ "$8" = "0.700" ]
[ "$9" = "--seed" ] && [ "${10}" = "0" ]
[ "${11}" = "--verbose" ] && [ "${12}" = "false" ]
[ "${HF_HUB_OFFLINE:-}" = "1" ]
[ "${TRANSFORMERS_OFFLINE:-}" = "1" ]
[ "${HF_HUB_DISABLE_TELEMETRY:-}" = "1" ]
[ "${PYTHONDONTWRITEBYTECODE:-}" = "1" ]
[ -z "${HOME:-}" ]
prompt=$(cat)
printf 'generated:%s' "$prompt"
"#,
        );
        let job = mlx_job("private-to-local-process");
        let result = runtime.execute(job.clone()).await.expect("execute MLX");
        assert_eq!(
            result.payload,
            json!({
                "operation":"generate_text",
                "output":"generated:private-to-local-process",
                "model":"local-mlx-model"
            })
        );
        result
            .validate_mlx_reasoning_result(&job)
            .expect("validate exact MLX result");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn mlx_cancellation_terminates_process_group_and_suppresses_output() {
        let (_directory, runtime) = mlx_runtime(
            r#"#!/bin/sh
trap '' TERM
cat >/dev/null
exec sleep 30
"#,
        );
        let job = mlx_job("cancel");
        let execution = {
            let runtime = runtime.clone();
            let job = job.clone();
            tokio::spawn(async move { runtime.execute(job).await })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        let acknowledgement = runtime
            .cancel(&cancellation(&job))
            .await
            .expect("cancel and reap MLX process group");
        assert_eq!(
            acknowledgement.status,
            CancellationAcknowledgementStatus::Cancelled
        );
        let execution_outcome = execution.await.expect("join MLX execution");
        assert!(
            matches!(execution_outcome, Err(MlxRuntimeError::Cancelled)),
            "unexpected cancellation outcome: {execution_outcome:?}"
        );
        assert!(matches!(
            runtime.cancel(&cancellation(&job)).await,
            Err(MlxRuntimeError::NotActive)
        ));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn mlx_agent_shutdown_terminates_process_group_and_suppresses_output() {
        let (_directory, runtime) = mlx_runtime(
            r#"#!/bin/sh
trap '' TERM
cat >/dev/null
sleep 30
printf 'must-not-escape'
"#,
        );
        let job = mlx_job("agent-shutdown");
        let execution = {
            let runtime = runtime.clone();
            let job = job.clone();
            tokio::spawn(async move { runtime.execute(job).await })
        };
        tokio::time::sleep(Duration::from_millis(50)).await;
        runtime
            .shutdown_active()
            .await
            .expect("agent shutdown reaps MLX process group");
        let execution_outcome = execution.await.expect("join MLX shutdown execution");
        assert!(
            matches!(execution_outcome, Err(MlxRuntimeError::Cancelled)),
            "unexpected shutdown outcome: {execution_outcome:?}"
        );
    }

    // A slow or unprovable reap must not relabel a job that already has a
    // definite verdict. This used to fail only under CPU load, where the reap
    // proof exceeded its budget and a cancelled job surfaced as an internal
    // error (HTTP 500) instead of `job_cancelled` (409) — losing the one fact
    // the caller needed and contradicting "cancellation dominates completion".
    // Pin the precedence directly so it no longer depends on machine speed.
    #[test]
    fn an_unprovable_cleanup_never_overwrites_a_definite_verdict() {
        let runtime = MlxJobRuntime::new(None);
        assert!(!runtime.cleanup_unproven());

        for verdict in [
            MlxRuntimeError::Cancelled,
            MlxRuntimeError::Timeout,
            MlxRuntimeError::BackendFailed,
            MlxRuntimeError::InvalidOutput,
        ] {
            let reported =
                runtime.resolve_cleanup(Err(MlxRuntimeError::CleanupFailed), verdict.clone());
            assert_eq!(
                reported, verdict,
                "cleanup failure must not replace the {verdict:?} verdict"
            );
        }

        // Swallowing it would be the other failure mode, so the runtime latches
        // closed instead and refuses to admit new work.
        assert!(runtime.cleanup_unproven());
    }

    #[test]
    fn a_proven_cleanup_leaves_the_runtime_open() {
        let runtime = MlxJobRuntime::new(None);
        let reported = runtime.resolve_cleanup(Ok(()), MlxRuntimeError::Cancelled);
        assert_eq!(reported, MlxRuntimeError::Cancelled);
        assert!(!runtime.cleanup_unproven());
    }

    #[tokio::test]
    async fn a_latched_runtime_refuses_new_work_before_touching_the_backend() {
        let (_directory, runtime) = mlx_runtime(
            r#"#!/bin/sh
cat >/dev/null
printf 'must-not-run'
"#,
        );
        runtime.resolve_cleanup(
            Err(MlxRuntimeError::CleanupFailed),
            MlxRuntimeError::Cancelled,
        );
        assert!(runtime.cleanup_unproven());

        // Refusal happens before configuration or contract checks, because the
        // runtime cannot promise the previous backend stopped emitting output.
        assert!(matches!(
            runtime.execute(mlx_job("after-latch")).await,
            Err(MlxRuntimeError::CleanupUnproven)
        ));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn mlx_cancel_completion_notification_cannot_be_lost() {
        let (_directory, runtime) = mlx_runtime(
            r#"#!/bin/sh
cat >/dev/null
printf 'fast-result'
"#,
        );
        for _ in 0..32 {
            let job = mlx_job("race");
            let execution = {
                let runtime = runtime.clone();
                let job = job.clone();
                tokio::spawn(async move { runtime.execute(job).await })
            };
            tokio::task::yield_now().await;
            let cancellation = runtime.cancel(&cancellation(&job)).await;
            assert!(
                !matches!(cancellation, Err(MlxRuntimeError::CleanupFailed)),
                "completion notification was lost"
            );
            let _ = execution.await.expect("join fast MLX execution");
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn mlx_stdout_overflow_fails_closed_and_reaps_the_backend() {
        let (_directory, runtime) = mlx_runtime(
            r#"#!/bin/sh
cat >/dev/null
i=0
while [ "$i" -lt 800 ]; do
  printf '%01024d' 0
  i=$((i + 1))
done
"#,
        );
        assert!(matches!(
            runtime.execute(mlx_job("overflow")).await,
            Err(MlxRuntimeError::InvalidOutput)
        ));
        let valid_job = mlx_job("new-attempt-after-cleanup");
        assert!(
            !matches!(
                runtime.execute(valid_job).await,
                Err(MlxRuntimeError::AlreadyActive)
            ),
            "overflow left the one-attempt runtime occupied"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn mlx_runtime_rejects_executable_replacement_after_startup_validation() {
        let (directory, runtime) = mlx_runtime(
            r#"#!/bin/sh
cat >/dev/null
printf 'validated'
"#,
        );
        fs::write(
            directory.path().join("mlx_lm.generate"),
            "#!/bin/sh\ncat >/dev/null\nprintf 'replaced'\n",
        )
        .expect("replace validated MLX executable");
        assert!(matches!(
            runtime.execute(mlx_job("replacement")).await,
            Err(MlxRuntimeError::InvalidConfiguration)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn mlx_configuration_rejects_relative_missing_and_symlink_paths() {
        let directory = tempdir().expect("MLX config fixture");
        assert!(matches!(
            MlxRuntimeConfig::validate(
                PathBuf::from("relative"),
                directory.path().to_path_buf(),
                "model".to_string()
            ),
            Err(MlxRuntimeError::InvalidConfiguration)
        ));
        let executable = directory.path().join("missing");
        let model = directory.path().join("model");
        fs::create_dir(&model).expect("model");
        assert!(matches!(
            MlxRuntimeConfig::validate(executable, model, "model".to_string()),
            Err(MlxRuntimeError::InvalidConfiguration)
        ));
    }
}
