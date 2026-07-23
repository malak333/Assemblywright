#![cfg(target_os = "macos")]

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use jarvis_protocol::{
    DistributedEvent, DistributedEventBatch, DistributedEventCursor, DistributedEventKind, StepId,
    TaskId, PROTOCOL_VERSION,
};
use serde_json::{json, Value};
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
    let mut child = start_agent(&data_dir, &socket_path);

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
    let restarted = start_agent(&data_dir, &restart_socket_path);
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
fn agent_rejects_parent_mismatch_before_creating_durable_state() {
    let temporary = tempfile::tempdir().expect("parent mismatch fixture");
    let data_dir = temporary.path().join("data");
    let startup = json!({
        "version": 1,
        "supervised_parent_pid": u32::MAX,
        "socket_path": temporary.path().join("agent.sock"),
        "peer_code_requirement": "true",
        "peer_identity_profile": "adhoc_exact",
        "bearer_token": TOKEN
    })
    .to_string();
    let mut child = Command::new(env!("CARGO_BIN_EXE_jarvis-agent"))
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

fn start_agent(data_dir: &Path, socket_path: &Path) -> Child {
    let startup = json!({
        "version": 1,
        "supervised_parent_pid": std::process::id(),
        "socket_path": socket_path,
        "peer_code_requirement": current_process_designated_requirement(),
        "peer_identity_profile": "adhoc_exact",
        "bearer_token": TOKEN
    })
    .to_string();
    let mut child = Command::new(env!("CARGO_BIN_EXE_jarvis-agent"))
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

fn wait_for_socket(child: &mut Child, socket_path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Ok(metadata) = fs::symlink_metadata(socket_path) {
            assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
            return;
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
        assert!(Instant::now() < deadline, "agent socket startup timed out");
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
