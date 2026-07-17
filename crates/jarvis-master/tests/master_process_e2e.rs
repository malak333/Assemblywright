use jarvis_protocol::MAX_WIRE_FRAME_BYTES;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use tempfile::tempdir;

struct ChildGuard {
    child: Child,
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn windows_master_process_owns_state_and_completes_cross_process_fixture() {
    let directory = tempdir().expect("temporary master process directory");
    let binary = env!("CARGO_BIN_EXE_jarvis-master");

    let setup = run(binary, directory.path(), ["setup"]);
    assert_success(&setup, "setup");
    let setup_receipt: Value = serde_json::from_slice(&setup.stdout).expect("setup JSON receipt");
    assert_eq!(setup_receipt["status"], "setup_complete");
    assert_eq!(setup_receipt["protocol_version"], 1);
    assert_eq!(setup_receipt["schema_version"], 2);
    assert!(directory.path().join("master.sqlite3").is_file());
    assert!(directory.path().join("development.token").is_file());
    let development_token = std::fs::read_to_string(directory.path().join("development.token"))
        .expect("read generated development token");
    let development_token = development_token.trim();
    assert!(!development_token.is_empty());
    assert!(
        !String::from_utf8_lossy(&setup.stdout).contains(development_token),
        "setup receipt exposed the development bearer"
    );

    let endpoint = unused_loopback_addr();
    let mut server = spawn_server(binary, directory.path(), endpoint);
    let ready = read_ready(&mut server.child);
    assert_eq!(ready["status"], "ready");
    assert_eq!(ready["endpoint"], endpoint.to_string());
    assert_unauthorized_without_bearer(endpoint);
    assert_oversized_body_is_rejected(endpoint, development_token);

    let health = run(
        binary,
        directory.path(),
        ["health", "--endpoint", &endpoint.to_string()],
    );
    assert_success(&health, "initial health");
    let health_json: Value = serde_json::from_slice(&health.stdout).expect("health JSON");
    assert_eq!(health_json["status"], "ok");
    assert_eq!(health_json["state"]["terminal_steps"], 0);

    let second_endpoint = unused_loopback_addr();
    let second_owner = run(
        binary,
        directory.path(),
        ["serve", "--bind", &second_endpoint.to_string()],
    );
    assert!(
        !second_owner.status.success(),
        "second owner unexpectedly started"
    );
    assert!(
        String::from_utf8_lossy(&second_owner.stderr).contains("already owns"),
        "unexpected second-owner error: {}",
        String::from_utf8_lossy(&second_owner.stderr)
    );

    let fixture = run(
        binary,
        directory.path(),
        [
            "fixture-worker",
            "--endpoint",
            &endpoint.to_string(),
            "--prompt",
            "complete the child-process fixture",
        ],
    );
    assert_success(&fixture, "fixture worker");
    let fixture_json: Value = serde_json::from_slice(&fixture.stdout).expect("fixture JSON");
    assert_eq!(fixture_json["status"], "fixture_complete");
    assert_eq!(fixture_json["accepted_result"]["status"], "succeeded");

    let completed_health = run(
        binary,
        directory.path(),
        ["health", "--endpoint", &endpoint.to_string()],
    );
    assert_success(&completed_health, "completed health");
    let completed_json: Value =
        serde_json::from_slice(&completed_health.stdout).expect("completed health JSON");
    assert_eq!(completed_json["state"]["registered_devices"], 1);
    assert_eq!(completed_json["state"]["active_connections"], 1);
    assert_eq!(completed_json["state"]["terminal_steps"], 1);
    assert_eq!(completed_json["state"]["active_attempts"], 0);

    server.child.kill().expect("stop first master process");
    server.child.wait().expect("reap first master process");

    let restart_endpoint = unused_loopback_addr();
    let mut restarted = spawn_server(binary, directory.path(), restart_endpoint);
    let restarted_ready = read_ready(&mut restarted.child);
    assert_eq!(restarted_ready["status"], "ready");

    let restarted_health = run(
        binary,
        directory.path(),
        ["health", "--endpoint", &restart_endpoint.to_string()],
    );
    assert_success(&restarted_health, "restarted health");
    let restarted_json: Value =
        serde_json::from_slice(&restarted_health.stdout).expect("restarted health JSON");
    assert_eq!(
        restarted_json["startup_reconciliation"]["disconnected_connections"],
        1
    );
    assert_eq!(restarted_json["state"]["active_connections"], 0);
    assert_eq!(restarted_json["state"]["terminal_steps"], 1);
}

fn run<const N: usize>(binary: &str, data_dir: &Path, arguments: [&str; N]) -> Output {
    Command::new(binary)
        .arg("--data-dir")
        .arg(data_dir)
        .args(arguments)
        .output()
        .expect("run jarvis-master command")
}

fn spawn_server(binary: &str, data_dir: &Path, endpoint: SocketAddr) -> ChildGuard {
    let child = Command::new(binary)
        .arg("--data-dir")
        .arg(data_dir)
        .arg("serve")
        .arg("--bind")
        .arg(endpoint.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn jarvis-master serve");
    ChildGuard { child }
}

fn read_ready(child: &mut Child) -> Value {
    let stdout = child.stdout.take().expect("master stdout");
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).expect("read ready receipt");
    assert!(!line.is_empty(), "master exited without a ready receipt");
    serde_json::from_str(&line)
        .unwrap_or_else(|error| panic!("invalid ready receipt {line:?}: {error}"))
}

fn unused_loopback_addr() -> SocketAddr {
    TcpListener::bind("127.0.0.1:0")
        .expect("reserve loopback address")
        .local_addr()
        .expect("read loopback address")
}

fn assert_unauthorized_without_bearer(endpoint: SocketAddr) {
    let mut stream = TcpStream::connect(endpoint).expect("connect without bearer");
    write!(
        stream,
        "GET /health HTTP/1.1\r\nHost: {endpoint}\r\nConnection: close\r\n\r\n"
    )
    .expect("write unauthenticated health request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read unauthenticated health response");
    assert!(
        response.starts_with("HTTP/1.1 401 Unauthorized"),
        "unexpected unauthenticated response: {response}"
    );
}

fn assert_oversized_body_is_rejected(endpoint: SocketAddr, token: &str) {
    let body = vec![b'x'; MAX_WIRE_FRAME_BYTES + 1];
    let mut stream = TcpStream::connect(endpoint).expect("connect for oversized request");
    write!(
        stream,
        "POST /v1/development/steps HTTP/1.1\r\nHost: {endpoint}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )
    .expect("write oversized request headers");
    stream
        .write_all(&body)
        .expect("write oversized request body");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read oversized response");
    assert!(
        response.starts_with("HTTP/1.1 413 Payload Too Large"),
        "unexpected oversized response status"
    );
}

fn assert_success(output: &Output, operation: &str) {
    assert!(
        output.status.success(),
        "{operation} failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
