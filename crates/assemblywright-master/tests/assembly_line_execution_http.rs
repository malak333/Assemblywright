use assemblywright_protocol::{
    AssemblyLineEmergencyPauseRequest, AssemblyLineOwnerProjection, AssemblyLineStartRequest,
    AssemblyLineStopRequest, FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
};
use serde::Serialize;
use serde_json::Value;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use tempfile::tempdir;
use uuid::Uuid;

struct ChildGuard(Child);

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn execution_routes_fail_closed_without_authenticated_host_effect_runtime() {
    let directory = tempdir().unwrap();
    let binary = env!("CARGO_BIN_EXE_assemblywright-master");
    assert!(Command::new(binary)
        .arg("--data-dir")
        .arg(directory.path())
        .arg("setup")
        .status()
        .unwrap()
        .success());
    let endpoint = unused_loopback_addr();
    let mut server = spawn_server(binary, directory.path(), endpoint);
    read_ready(&mut server.0);
    let token = std::fs::read_to_string(directory.path().join("development.token")).unwrap();
    let token = token.trim();
    let before: AssemblyLineOwnerProjection = serde_json::from_value(response_json(&get_request(
        endpoint,
        "/v1/assembly-line",
        Some(token),
    )))
    .unwrap();

    let mut start = AssemblyLineStartRequest {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        request_id: Uuid::new_v4(),
        expected_state_revision: before.assembly_line.state_revision,
        expected_queue_revision: before.assembly_line.queue_revision,
        expected_emergency_pause_revision: before.emergency_pause_revision,
        queue_count: 1,
        windows_executor_id: Uuid::new_v4(),
        windows_executor_revision: 1,
        mac_executor_id: Uuid::new_v4(),
        mac_executor_revision: 1,
        auto_run: before.assembly_line.auto_run,
        owner_start_approval_sha256: [0; 32],
    };
    start.owner_start_approval_sha256 = start.canonical_owner_start_approval_sha256().unwrap();
    assert_not_found(post_json(
        endpoint,
        "/v1/assembly-line/start",
        Some(token),
        &start,
    ));
    assert!(post_json(endpoint, "/v1/assembly-line/start", None, &start)
        .starts_with("HTTP/1.1 401 Unauthorized"));

    let session_id = Uuid::new_v4();
    let child_epoch_id = Uuid::new_v4();
    assert_not_found(post_json(
        endpoint,
        "/v1/assembly-line/stop",
        Some(token),
        &AssemblyLineStopRequest {
            schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
            request_id: Uuid::new_v4(),
            session_id,
            expected_state_revision: before.assembly_line.state_revision,
            expected_child_epoch_id: child_epoch_id,
        },
    ));
    assert_not_found(post_json(
        endpoint,
        "/v1/assembly-line/emergency-pause",
        Some(token),
        &AssemblyLineEmergencyPauseRequest {
            schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
            request_id: Uuid::new_v4(),
            session_id,
            expected_child_epoch_id: child_epoch_id,
            expected_state_revision: before.assembly_line.state_revision,
            expected_emergency_pause_revision: before.emergency_pause_revision,
        },
    ));

    let after: AssemblyLineOwnerProjection = serde_json::from_value(response_json(&get_request(
        endpoint,
        "/v1/assembly-line",
        Some(token),
    )))
    .unwrap();
    assert_eq!(after.assembly_line, before.assembly_line);
    assert_eq!(
        after.emergency_pause_revision,
        before.emergency_pause_revision
    );
    assert!(!after.emergency_paused);
}

fn assert_not_found(response: String) {
    assert!(response.starts_with("HTTP/1.1 404 Not Found"), "{response}");
    assert_eq!(
        response_json(&response),
        serde_json::json!({"error":"not_found"})
    );
}

fn spawn_server(binary: &str, data_dir: &Path, endpoint: SocketAddr) -> ChildGuard {
    ChildGuard(
        Command::new(binary)
            .arg("--data-dir")
            .arg(data_dir)
            .arg("serve")
            .arg("--bind")
            .arg(endpoint.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    )
}

fn read_ready(child: &mut Child) {
    let mut line = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(&line).unwrap()["status"],
        "ready"
    );
}

fn unused_loopback_addr() -> SocketAddr {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
}

fn get_request(endpoint: SocketAddr, path: &str, token: Option<&str>) -> String {
    request(endpoint, "GET", path, token, "")
}

fn post_json<T: Serialize>(
    endpoint: SocketAddr,
    path: &str,
    token: Option<&str>,
    body: &T,
) -> String {
    request(
        endpoint,
        "POST",
        path,
        token,
        &serde_json::to_string(body).unwrap(),
    )
}

fn request(
    endpoint: SocketAddr,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: &str,
) -> String {
    let mut stream = TcpStream::connect(endpoint).unwrap();
    let authorization = token
        .map(|token| format!("Authorization: Bearer {token}\r\n"))
        .unwrap_or_default();
    write!(
        stream,
        "{method} {path} HTTP/1.1\r\nHost: {endpoint}\r\n{authorization}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}

fn response_json(response: &str) -> Value {
    serde_json::from_str(response.split_once("\r\n\r\n").unwrap().1).unwrap()
}
