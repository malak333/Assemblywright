#![cfg(target_os = "macos")]

use assemblywright_protocol::{
    AttemptId, CancellationId, CancellationInstruction, ContextHandlingPolicy, DistributedEvent,
    DistributedEventBatch, DistributedEventCursor, DistributedEventKind,
    FeatureConveyorCodingWorkPacketMetadata, JobEnvelope, LeaseId, LocalCodingJobRequest,
    LocalCodingSnapshotChunk, Sensitivity, StepId, TaskId, CANCELLATION_ACK_DEADLINE_MS,
    FIXTURE_REASONING_CAPABILITY_ID, FIXTURE_REASONING_MODEL, LOCAL_CODING_CAPABILITY_ID,
    LOCAL_CODING_MODEL, MLX_REASONING_CAPABILITY_ID, PROTOCOL_VERSION,
};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use git2::{IndexAddOption, ObjectType, Repository, Signature};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

const TOKEN: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn supervised_agent_uds_requires_identity_and_bearer_and_persists_exact_cursor() {
    let temporary = tempfile::tempdir().expect("agent relay fixture");
    let runtime_dir = temporary.path().join("run");
    fs::create_dir(&runtime_dir).expect("create runtime directory");
    fs::set_permissions(&runtime_dir, fs::Permissions::from_mode(0o700))
        .expect("secure runtime directory");
    let socket_path = runtime_dir.join("agent.sock");
    let data_dir = temporary.path().join("data");
    let mut child = start_agent(&data_dir, &socket_path, false);

    let pid = child.id().to_string();
    let process = Command::new("ps")
        .args(["-ww", "-o", "command=", "-p", &pid])
        .output()
        .expect("inspect agent argv");
    assert!(process.status.success());
    let command_line = String::from_utf8_lossy(&process.stdout);
    assert!(!command_line.contains(TOKEN), "{command_line}");
    assert!(
        !command_line.contains(socket_path.to_string_lossy().as_ref()),
        "{command_line}"
    );

    let unauthorized = send(&socket_path, request("GET", "/health", None, None));
    assert_eq!(unauthorized["status"], 401);
    let bearer = Some(format!("Bearer {TOKEN}"));
    let health = send(
        &socket_path,
        request("GET", "/health", bearer.clone(), None),
    );
    assert_eq!(health["status"], 200);
    let health_body = response_body(&health);
    assert_eq!(health_body["mode"], "developer_event_relay");
    assert_eq!(
        health_body["boundary"],
        "metadata_only_no_authoritative_state"
    );
    assert!(health_body["cursor"]["cursor"].is_null());

    let stream_id = Uuid::new_v4();
    let batch = batch(stream_id, 0, 1);
    let accepted = send(
        &socket_path,
        request(
            "POST",
            "/v1/events/accept",
            bearer.clone(),
            Some(serde_json::to_value(&batch).expect("batch JSON")),
        ),
    );
    assert_eq!(accepted["status"], 200);
    assert_eq!(response_body(&accepted)["cursor"]["cursor"]["sequence"], 1);

    let replay = send(
        &socket_path,
        request(
            "POST",
            "/v1/events/accept",
            bearer.clone(),
            Some(serde_json::to_value(&batch).expect("batch JSON")),
        ),
    );
    assert_eq!(replay["status"], 409);
    assert_eq!(response_body(&replay)["error"], "event_cursor_rejected");

    let health = send(&socket_path, request("GET", "/health", bearer, None));
    assert_eq!(response_body(&health)["cursor"]["cursor"]["sequence"], 1);

    child.kill().expect("stop first agent process");
    child.wait().expect("reap first agent process");
    let restart_socket_path = runtime_dir.join("agent-restart.sock");
    let restarted = start_agent(&data_dir, &restart_socket_path, false);
    let restarted_health = send(
        &restart_socket_path,
        request("GET", "/health", Some(format!("Bearer {TOKEN}")), None),
    );
    assert_eq!(restarted_health["status"], 200);
    assert_eq!(
        response_body(&restarted_health)["cursor"]["cursor"]["stream_id"],
        stream_id.to_string()
    );
    assert_eq!(
        response_body(&restarted_health)["cursor"]["cursor"]["sequence"],
        1
    );
    ChildGuard(restarted);
}

#[test]
fn authenticated_uds_fixture_success_and_cancellation_suppress_late_output() {
    let temporary = tempfile::tempdir().expect("agent fixture job");
    let runtime_dir = temporary.path().join("run");
    fs::create_dir(&runtime_dir).expect("create runtime");
    fs::set_permissions(&runtime_dir, fs::Permissions::from_mode(0o700)).expect("secure runtime");
    let socket_path = runtime_dir.join("agent.sock");
    let child = start_agent(&temporary.path().join("data"), &socket_path, true);
    let bearer = Some(format!("Bearer {TOKEN}"));
    let health = send(
        &socket_path,
        request("GET", "/health", bearer.clone(), None),
    );
    let health_body = response_body(&health);
    assert_eq!(health_body["fixture_jobs_enabled"], true);
    assert_eq!(
        health_body["boundary"],
        "metadata_cursor_plus_in_memory_public_fixture_jobs_no_retention"
    );

    let completed = send(
        &socket_path,
        request(
            "POST",
            "/v1/jobs/execute",
            bearer.clone(),
            Some(serde_json::to_value(fixture_job(0, "bounded")).unwrap()),
        ),
    );
    assert_eq!(completed["status"], 200);
    assert_eq!(response_body(&completed)["payload"]["output"], "bounded");

    let delayed = fixture_job(5_000, "must-not-escape");
    let delayed_for_thread = delayed.clone();
    let socket_for_thread = socket_path.clone();
    let bearer_for_thread = bearer.clone();
    let execution = thread::spawn(move || {
        send(
            &socket_for_thread,
            request(
                "POST",
                "/v1/jobs/execute",
                bearer_for_thread,
                Some(serde_json::to_value(delayed_for_thread).unwrap()),
            ),
        )
    });
    thread::sleep(Duration::from_millis(100));
    let cancelled = send(
        &socket_path,
        request(
            "POST",
            "/v1/jobs/cancel",
            bearer,
            Some(serde_json::to_value(cancellation(&delayed)).unwrap()),
        ),
    );
    assert_eq!(cancelled["status"], 200);
    assert_eq!(response_body(&cancelled)["status"], "cancelled");
    let late = execution.join().expect("join delayed fixture");
    assert_eq!(late["status"], 409);
    assert_eq!(response_body(&late)["error"], "job_cancelled");
    ChildGuard(child);
}

#[test]
fn authenticated_uds_mlx_success_and_cancellation_are_separate_and_bounded() {
    let temporary = tempfile::tempdir().expect("agent MLX job");
    let runtime_dir = temporary.path().join("run");
    fs::create_dir(&runtime_dir).expect("create runtime");
    fs::set_permissions(&runtime_dir, fs::Permissions::from_mode(0o700)).expect("secure runtime");
    let socket_path = runtime_dir.join("agent.sock");
    let executable = temporary.path().join("mlx_lm.generate");
    let model_path = temporary.path().join("model");
    fs::create_dir(&model_path).expect("create model");
    fs::write(
        &executable,
        r#"#!/bin/sh
set -eu
prompt=$(cat)
if [ "$prompt" = "delay" ]; then
  trap '' TERM
  sleep 30
fi
printf 'mlx:%s' "$prompt"
"#,
    )
    .expect("write MLX executable");
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
        .expect("make MLX executable");
    let child = start_mlx_agent(
        &temporary.path().join("data"),
        &socket_path,
        &executable,
        &model_path,
    );
    let bearer = Some(format!("Bearer {TOKEN}"));
    let health = send(
        &socket_path,
        request("GET", "/health", bearer.clone(), None),
    );
    let health_body = response_body(&health);
    assert_eq!(health_body["fixture_jobs_enabled"], false);
    assert_eq!(health_body["mlx_jobs_enabled"], true);
    assert_eq!(
        health_body["boundary"],
        "metadata_cursor_plus_bounded_public_mlx_jobs_no_retention"
    );

    let completed_job = mlx_job("bounded");
    let completed = send(
        &socket_path,
        request(
            "POST",
            "/v1/mlx/jobs/execute",
            bearer.clone(),
            Some(serde_json::to_value(&completed_job).unwrap()),
        ),
    );
    assert_eq!(completed["status"], 200);
    assert_eq!(
        response_body(&completed)["payload"]["output"],
        "mlx:bounded"
    );

    let delayed = mlx_job("delay");
    let delayed_for_thread = delayed.clone();
    let socket_for_thread = socket_path.clone();
    let bearer_for_thread = bearer.clone();
    let execution = thread::spawn(move || {
        send(
            &socket_for_thread,
            request(
                "POST",
                "/v1/mlx/jobs/execute",
                bearer_for_thread,
                Some(serde_json::to_value(delayed_for_thread).unwrap()),
            ),
        )
    });
    thread::sleep(Duration::from_millis(100));
    let cancelled = send(
        &socket_path,
        request(
            "POST",
            "/v1/mlx/jobs/cancel",
            bearer,
            Some(serde_json::to_value(cancellation(&delayed)).unwrap()),
        ),
    );
    assert_eq!(cancelled["status"], 200);
    assert_eq!(response_body(&cancelled)["status"], "cancelled");
    let late = execution.join().expect("join delayed MLX");
    assert_eq!(late["status"], 409);
    assert_eq!(response_body(&late)["error"], "job_cancelled");
    ChildGuard(child);
}

#[test]
fn authenticated_uds_local_coding_snapshot_admission_cancellation_and_restart_cleanup() {
    let temporary = tempfile::tempdir().expect("agent local coding snapshot");
    let runtime_dir = temporary.path().join("run");
    fs::create_dir(&runtime_dir).expect("create runtime");
    fs::set_permissions(&runtime_dir, fs::Permissions::from_mode(0o700)).expect("secure runtime");
    let data_dir = temporary.path().join("data");
    let socket_path = runtime_dir.join("local-coding.sock");
    let mut child = start_local_coding_agent(&data_dir, &socket_path);
    let bearer = Some(format!("Bearer {TOKEN}"));
    let health = send(
        &socket_path,
        request("GET", "/health", bearer.clone(), None),
    );
    let health_body = response_body(&health);
    assert_eq!(health_body["local_coding_snapshots_enabled"], true);
    assert_eq!(
        health_body["boundary"],
        "metadata_cursor_plus_ephemeral_snapshot_materialization_no_execution"
    );

    let (bundle, snapshot_sha256) = local_coding_bundle();
    let job = local_coding_job(snapshot_sha256);
    let admitted = send(
        &socket_path,
        request(
            "POST",
            "/v1/local-coding/snapshots/admit",
            bearer.clone(),
            Some(serde_json::to_value(&job).unwrap()),
        ),
    );
    assert_eq!(admitted["status"], 200);
    assert_eq!(response_body(&admitted)["status"], "snapshot_admitted");
    let snapshot_id = serde_json::from_value::<LocalCodingJobRequest>(job.context.clone())
        .unwrap()
        .snapshot_id;
    let chunk = LocalCodingSnapshotChunk {
        protocol_version: job.protocol_version,
        connection_epoch: job.connection_epoch,
        task_id: job.task_id,
        step_id: job.step_id,
        attempt_id: job.attempt_id,
        lease_id: job.lease_id,
        cancellation_id: job.cancellation_id,
        snapshot_id,
        snapshot_sha256,
        offset: 0,
        total_bytes: bundle.len() as u64,
        content_sha256: Sha256::digest(&bundle).into(),
        content_hex: bundle.iter().map(|byte| format!("{byte:02x}")).collect(),
        complete: true,
    };
    let materialized = send(
        &socket_path,
        request(
            "POST",
            "/v1/local-coding/snapshots/accept",
            bearer.clone(),
            Some(serde_json::to_value(chunk).unwrap()),
        ),
    );
    assert_eq!(materialized["status"], 200);
    assert_eq!(
        response_body(&materialized)["payload"]["status"],
        "snapshot_materialized"
    );
    let cancelled = send(
        &socket_path,
        request(
            "POST",
            "/v1/local-coding/snapshots/cancel",
            bearer.clone(),
            Some(serde_json::to_value(cancellation(&job)).unwrap()),
        ),
    );
    assert_eq!(cancelled["status"], 200);
    assert_eq!(response_body(&cancelled)["status"], "cancelled");

    let second_job = local_coding_job([3; 32]);
    let admitted = send(
        &socket_path,
        request(
            "POST",
            "/v1/local-coding/snapshots/admit",
            bearer,
            Some(serde_json::to_value(&second_job).unwrap()),
        ),
    );
    assert_eq!(admitted["status"], 200);
    child.kill().expect("stop with partial snapshot");
    child.wait().expect("reap partial snapshot agent");

    let restart_socket = runtime_dir.join("local-coding-restart.sock");
    let restarted = start_local_coding_agent(&data_dir, &restart_socket);
    assert_eq!(
        fs::read_dir(data_dir.join("local-coding-snapshots"))
            .unwrap()
            .count(),
        0
    );
    ChildGuard(restarted);
}

#[test]
fn agent_sigterm_reaps_active_mlx_process_group_before_exit() {
    let temporary = tempfile::tempdir().expect("agent MLX shutdown");
    let runtime_dir = temporary.path().join("run");
    fs::create_dir(&runtime_dir).expect("create runtime");
    fs::set_permissions(&runtime_dir, fs::Permissions::from_mode(0o700)).expect("secure runtime");
    let socket_path = runtime_dir.join("agent.sock");
    let executable = temporary.path().join("mlx_lm.generate");
    let model_path = temporary.path().join("model");
    let backend_pid_path = temporary.path().join("backend.pid");
    fs::create_dir(&model_path).expect("create model");
    let backend_pid_text = backend_pid_path.to_string_lossy();
    assert!(!backend_pid_text.contains('"'));
    fs::write(
        &executable,
        format!(
            "#!/bin/sh\ntrap '' TERM\necho $$ > \"{backend_pid_text}\"\ncat >/dev/null\nsleep 30\nprintf 'must-not-escape'\n"
        ),
    )
    .expect("write MLX shutdown executable");
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700))
        .expect("make MLX executable");
    let mut child = start_mlx_agent(
        &temporary.path().join("data"),
        &socket_path,
        &executable,
        &model_path,
    );
    let socket_for_thread = socket_path.clone();
    let execution = thread::spawn(move || {
        std::panic::catch_unwind(|| {
            send(
                &socket_for_thread,
                request(
                    "POST",
                    "/v1/mlx/jobs/execute",
                    Some(format!("Bearer {TOKEN}")),
                    Some(serde_json::to_value(mlx_job("shutdown")).unwrap()),
                ),
            )
        })
    });
    let marker_deadline = Instant::now() + Duration::from_secs(5);
    while !backend_pid_path.exists() && Instant::now() < marker_deadline {
        thread::sleep(Duration::from_millis(20));
    }
    let backend_pid: i32 = fs::read_to_string(&backend_pid_path)
        .expect("backend process marker")
        .trim()
        .parse()
        .expect("backend process ID");
    assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGTERM) }, 0);
    let exit_deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll agent exit") {
            break status;
        }
        assert!(
            Instant::now() < exit_deadline,
            "agent did not finish bounded MLX shutdown"
        );
        thread::sleep(Duration::from_millis(20));
    };
    assert!(status.success(), "agent shutdown failed: {status}");
    let backend_exists = unsafe { libc::kill(-backend_pid, 0) } == 0;
    let backend_error = std::io::Error::last_os_error();
    assert!(
        !backend_exists && backend_error.raw_os_error() == Some(libc::ESRCH),
        "MLX process group survived agent SIGTERM: {backend_error}"
    );
    let _ = execution.join();
}

#[test]
fn agent_rejects_parent_mismatch_before_creating_durable_state() {
    let temporary = tempfile::tempdir().expect("parent mismatch fixture");
    let data_dir = temporary.path().join("data");
    let startup = json!({
        "version": 1,
        "supervised_parent_pid": u32::MAX,
        "socket_path": temporary.path().join("agent.sock"),
        "peer_code_requirement": "cdhash H\"0123456789abcdef0123456789abcdef01234567\"",
        "peer_identity_profile": "adhoc_exact",
        "bearer_token": TOKEN
    })
    .to_string();
    let mut child = Command::new(env!("CARGO_BIN_EXE_assemblywright-agent"))
        .args(["--data-dir", data_dir.to_str().expect("data path"), "serve"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mismatched agent");
    child
        .stdin
        .take()
        .expect("startup stdin")
        .write_all(startup.as_bytes())
        .expect("write startup document");
    let output = child.wait_with_output().expect("wait for parent rejection");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("direct parent"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!data_dir.exists());
}

#[test]
fn agent_rejects_non_exact_peer_requirement_before_creating_durable_state() {
    let temporary = tempfile::tempdir().expect("peer requirement fixture");
    let data_dir = temporary.path().join("data");
    let startup = json!({
        "version": 1,
        "supervised_parent_pid": std::process::id(),
        "socket_path": temporary.path().join("agent.sock"),
        "peer_code_requirement": "true",
        "peer_identity_profile": "adhoc_exact",
        "bearer_token": TOKEN
    })
    .to_string();
    let mut child = Command::new(env!("CARGO_BIN_EXE_assemblywright-agent"))
        .args(["--data-dir", data_dir.to_str().expect("data path"), "serve"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn invalid-policy agent");
    child
        .stdin
        .take()
        .expect("startup stdin")
        .write_all(startup.as_bytes())
        .expect("write startup document");
    let output = child.wait_with_output().expect("wait for policy rejection");
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("identity profile"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!data_dir.exists());
}

#[test]
fn agent_rejects_partial_or_disabled_mlx_startup_authority_before_storage() {
    let temporary = tempfile::tempdir().expect("partial MLX startup fixture");
    let data_dir = temporary.path().join("data");
    for startup in [
        json!({
            "version": 2,
            "supervised_parent_pid": std::process::id(),
            "socket_path": temporary.path().join("partial.sock"),
            "peer_code_requirement": current_process_designated_requirement(),
            "peer_identity_profile": "adhoc_exact",
            "bearer_token": TOKEN,
            "mlx_jobs_enabled": true,
            "mlx_executable_path": temporary.path().join("mlx_lm.generate"),
            "mlx_model_path": temporary.path().join("model")
        }),
        json!({
            "version": 2,
            "supervised_parent_pid": std::process::id(),
            "socket_path": temporary.path().join("disabled.sock"),
            "peer_code_requirement": current_process_designated_requirement(),
            "peer_identity_profile": "adhoc_exact",
            "bearer_token": TOKEN,
            "mlx_jobs_enabled": false,
            "mlx_executable_path": temporary.path().join("mlx_lm.generate"),
            "mlx_model_path": temporary.path().join("model"),
            "mlx_model_id": "local-mlx-model"
        }),
    ] {
        let mut child = Command::new(env!("CARGO_BIN_EXE_assemblywright-agent"))
            .args(["--data-dir", data_dir.to_str().expect("data path"), "serve"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn invalid MLX agent");
        child
            .stdin
            .take()
            .expect("startup stdin")
            .write_all(startup.to_string().as_bytes())
            .expect("write invalid MLX startup document");
        let output = child.wait_with_output().expect("wait for MLX rejection");
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("required iff"),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!data_dir.exists());
    }
}

fn batch(stream_id: Uuid, after_sequence: u64, next_sequence: u64) -> DistributedEventBatch {
    DistributedEventBatch {
        protocol_version: PROTOCOL_VERSION,
        stream_id,
        after_sequence,
        next_sequence,
        events: vec![DistributedEvent {
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
        }],
        has_more: false,
    }
}

fn fixture_job(delay_ms: u64, input: &str) -> JobEnvelope {
    let context = json!({"operation":"synthetic_echo","input":input,"delay_ms":delay_ms});
    JobEnvelope {
        protocol_version: PROTOCOL_VERSION,
        connection_epoch: 17,
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

fn mlx_job(prompt: &str) -> JobEnvelope {
    let context = json!({
        "operation":"generate_text",
        "prompt":prompt,
        "max_tokens":32,
        "temperature_milli":700
    });
    JobEnvelope {
        protocol_version: PROTOCOL_VERSION,
        connection_epoch: 18,
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

fn local_coding_job(snapshot_sha256: [u8; 32]) -> JobEnvelope {
    let context = serde_json::to_value(LocalCodingJobRequest {
        feature_id: Uuid::new_v4(),
        specification_revision: 1,
        lifecycle_revision: 2,
        feature_lease_id: Uuid::new_v4(),
        snapshot_id: Uuid::new_v4(),
        snapshot_sha256,
        work_packet_sha256: [4; 32],
        work_packet: FeatureConveyorCodingWorkPacketMetadata {
            packet_id: Uuid::new_v4(),
            ordinal: 1,
            acceptance_criteria_count: 1,
        },
        device_id: assemblywright_protocol::DeviceId::new(Uuid::new_v4()),
        device_registry_revision: 1,
        queue_revision: 1,
        emergency_pause_revision: 1,
    })
    .unwrap();
    JobEnvelope {
        protocol_version: PROTOCOL_VERSION,
        connection_epoch: 19,
        sequence: 1,
        task_id: TaskId::new(Uuid::new_v4()),
        step_id: StepId::new(Uuid::new_v4()),
        attempt_id: AttemptId::new(Uuid::new_v4()),
        lease_id: LeaseId::new(Uuid::new_v4()),
        cancellation_id: CancellationId::new(Uuid::new_v4()),
        capability_id: LOCAL_CODING_CAPABILITY_ID.to_string(),
        selected_model: LOCAL_CODING_MODEL.to_string(),
        sensitivity: Sensitivity::Workspace,
        context_handling: ContextHandlingPolicy::EphemeralNoRetention,
        lease_duration_ms: 60_000,
        deadline_after_ms: 60_000,
        context_sha256: Sha256::digest(serde_json::to_vec(&context).unwrap()).into(),
        context,
    }
}

fn local_coding_bundle() -> (Vec<u8>, [u8; 32]) {
    const MAGIC: &[u8] = b"AW-SNAPSHOT-BUNDLE-V1\n";
    const END_MAGIC: &[u8] = b"AW-SNAPSHOT-END-V1\n";
    let source = tempfile::tempdir().unwrap();
    let repository = Repository::init(source.path()).unwrap();
    let content = b"native process materialization\n";
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
            "native process fixture",
            &tree,
            &[],
        )
        .unwrap();
    let odb = repository.odb().unwrap();
    let mut bundle = Vec::new();
    bundle.extend_from_slice(MAGIC);
    bundle.extend_from_slice(commit_oid.to_string().as_bytes());
    for (kind, oid) in [
        (ObjectType::Commit, commit_oid),
        (ObjectType::Tree, tree_oid),
        (ObjectType::Blob, blob_oid),
    ] {
        let object = odb.read(oid).unwrap();
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
    bundle.push(0);
    bundle.push(1);
    bundle.extend_from_slice(&("README.md".len() as u16).to_be_bytes());
    bundle.extend_from_slice(b"README.md");
    bundle.extend_from_slice(&0o100644_u32.to_be_bytes());
    bundle.extend_from_slice(blob_oid.as_bytes());
    bundle.extend_from_slice(&(content.len() as u64).to_be_bytes());
    bundle.push(0);
    let mut digest = Sha256::new();
    digest.update(b"assemblywright.repository-snapshot.v1\0");
    digest.update(commit_oid.as_bytes());
    digest.update(("README.md".len() as u64).to_be_bytes());
    digest.update(b"README.md");
    digest.update(0o100644_u32.to_be_bytes());
    digest.update(blob_oid.as_bytes());
    digest.update((content.len() as u64).to_be_bytes());
    digest.update(content);
    let snapshot_sha256: [u8; 32] = digest.finalize().into();
    bundle.extend_from_slice(END_MAGIC);
    bundle.extend_from_slice(&snapshot_sha256);
    (bundle, snapshot_sha256)
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

fn request(method: &str, path: &str, authorization: Option<String>, body: Option<Value>) -> Value {
    json!({
        "version": 1,
        "method": method,
        "path": path,
        "authorization": authorization,
        "accept": "application/json",
        "content_type": "application/json",
        "body_base64": body
            .map(|body| BASE64_STANDARD.encode(body.to_string()))
            .unwrap_or_default()
    })
}

fn send(socket_path: &Path, request: Value) -> Value {
    let mut stream = UnixStream::connect(socket_path).expect("connect agent relay");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .expect("set read timeout");
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .expect("set write timeout");
    let frame = serde_json::to_vec(&request).expect("encode UDS request");
    stream
        .write_all(&(frame.len() as u32).to_be_bytes())
        .expect("write frame prefix");
    stream.write_all(&frame).expect("write frame");
    stream
        .shutdown(std::net::Shutdown::Write)
        .expect("half-close request");
    let mut prefix = [0_u8; 4];
    stream.read_exact(&mut prefix).expect("read frame prefix");
    let length = u32::from_be_bytes(prefix) as usize;
    assert!(length > 0 && length <= 12 * 1024 * 1024);
    let mut response = vec![0_u8; length];
    stream.read_exact(&mut response).expect("read response");
    serde_json::from_slice(&response).expect("decode response")
}

fn response_body(response: &Value) -> Value {
    let encoded = response["body_base64"]
        .as_str()
        .expect("response body base64");
    let body = BASE64_STANDARD.decode(encoded).expect("decode body");
    serde_json::from_slice(&body).expect("decode body JSON")
}

fn start_agent(data_dir: &Path, socket_path: &Path, fixture_jobs_enabled: bool) -> Child {
    let startup = json!({
        "version": 1,
        "supervised_parent_pid": std::process::id(),
        "socket_path": socket_path,
        "peer_code_requirement": current_process_designated_requirement(),
        "peer_identity_profile": "adhoc_exact",
        "bearer_token": TOKEN,
        "fixture_jobs_enabled": fixture_jobs_enabled
    })
    .to_string();
    let mut child = Command::new(env!("CARGO_BIN_EXE_assemblywright-agent"))
        .args(["--data-dir", data_dir.to_str().expect("data path"), "serve"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn supervised agent");
    child
        .stdin
        .take()
        .expect("startup stdin")
        .write_all(startup.as_bytes())
        .expect("write startup document");
    wait_for_socket(&mut child, socket_path);
    child
}

fn start_mlx_agent(
    data_dir: &Path,
    socket_path: &Path,
    executable: &Path,
    model_path: &Path,
) -> Child {
    let startup = json!({
        "version": 2,
        "supervised_parent_pid": std::process::id(),
        "socket_path": socket_path,
        "peer_code_requirement": current_process_designated_requirement(),
        "peer_identity_profile": "adhoc_exact",
        "bearer_token": TOKEN,
        "fixture_jobs_enabled": false,
        "mlx_jobs_enabled": true,
        "mlx_executable_path": executable,
        "mlx_model_path": model_path,
        "mlx_model_id": "local-mlx-model"
    })
    .to_string();
    let mut child = Command::new(env!("CARGO_BIN_EXE_assemblywright-agent"))
        .args(["--data-dir", data_dir.to_str().expect("data path"), "serve"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn supervised MLX agent");
    child
        .stdin
        .take()
        .expect("startup stdin")
        .write_all(startup.as_bytes())
        .expect("write MLX startup document");
    wait_for_socket(&mut child, socket_path);
    child
}

fn start_local_coding_agent(data_dir: &Path, socket_path: &Path) -> Child {
    let startup = json!({
        "version": 2,
        "supervised_parent_pid": std::process::id(),
        "socket_path": socket_path,
        "peer_code_requirement": current_process_designated_requirement(),
        "peer_identity_profile": "adhoc_exact",
        "bearer_token": TOKEN,
        "fixture_jobs_enabled": false,
        "mlx_jobs_enabled": false,
        "local_coding_snapshots_enabled": true
    })
    .to_string();
    let mut child = Command::new(env!("CARGO_BIN_EXE_assemblywright-agent"))
        .args(["--data-dir", data_dir.to_str().expect("data path"), "serve"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn supervised local coding snapshot agent");
    child
        .stdin
        .take()
        .expect("startup stdin")
        .write_all(startup.as_bytes())
        .expect("write local coding startup document");
    wait_for_socket(&mut child, socket_path);
    child
}

fn wait_for_socket(child: &mut Child, socket_path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut observed_mode = None;
    loop {
        if let Ok(metadata) = fs::symlink_metadata(socket_path) {
            let mode = metadata.permissions().mode() & 0o777;
            if mode == 0o600 {
                return;
            }
            observed_mode = Some(mode);
        }
        if let Some(status) = child.try_wait().expect("inspect agent process") {
            let mut stderr = String::new();
            child
                .stderr
                .take()
                .expect("agent stderr")
                .read_to_string(&mut stderr)
                .expect("read agent stderr");
            panic!("agent exited early ({status}): {stderr}");
        }
        assert!(
            Instant::now() < deadline,
            "agent socket startup timed out; final observed mode was {}",
            observed_mode
                .map(|mode| format!("{mode:04o}"))
                .unwrap_or_else(|| "missing".to_string())
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn current_process_designated_requirement() -> String {
    let executable = std::env::current_exe().expect("current test executable");
    let output = Command::new("codesign")
        .args([
            "-d",
            "-r-",
            executable.to_str().expect("test executable path"),
        ])
        .output()
        .expect("inspect test code requirement");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    combined
        .split_once("designated =>")
        .and_then(|(_, requirement)| requirement.lines().next())
        .map(str::trim)
        .filter(|requirement| !requirement.is_empty())
        .expect("designated requirement")
        .to_string()
}
