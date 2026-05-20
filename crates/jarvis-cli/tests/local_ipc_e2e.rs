use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::Duration;

use serde_json::{json, Value};
use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_jarvis");

#[test]
fn serve_exposes_local_ipc_contract_and_persists_state() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let db_path = temp_dir.path().join("jarvis-e2e.sqlite");

    let mut server = JarvisServer::start(&db_path);
    let endpoint = server.endpoint();

    let health = run_cli_text(["health", "--endpoint", endpoint.as_str()]);
    assert!(health.contains("jarvis-core: ok"), "{health}");
    assert!(health.contains("runtime: routed-fake-local-model+first-party-plugins"));
    assert!(health.contains("paused: false"));
    assert!(health.contains("contract: v1"), "{health}");

    let contract = run_cli_json(["contract", "--endpoint", endpoint.as_str()]);
    assert_eq!(contract["contract"]["name"], "jarvis.local-ipc");
    assert_eq!(contract["contract"]["version"], 1);
    assert_array_contains(&contract["endpoints"], "path", "/diagnostics/export");
    assert_string_array_contains(&contract["safe_inspection_paths"], "/scheduler/jobs/:id");

    let command = run_cli_json([
        "command",
        "plugin echo cross-process e2e",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(command["accepted"], true);
    assert_eq!(command["task"]["status"], "completed");
    assert_eq!(command["route"]["model"], "fake-local-model");
    assert_eq!(command["plugin_results"][0]["status"], "completed");
    assert_eq!(
        command["plugin_results"][0]["output"]["message"],
        "cross-process e2e"
    );
    assert_array_contains(&command["audit_entries"], "event_type", "plugin_completed");
    let task_id = command["task"]["id"]
        .as_str()
        .expect("command task id")
        .to_string();

    let manifests = run_cli_json(["plugins", "list", "--endpoint", endpoint.as_str()]);
    assert_array_contains(&manifests, "id", "fake_echo");
    assert_array_contains(&manifests, "id", "fake_status");

    let fake_echo_manifest = run_cli_json([
        "plugins",
        "get",
        "fake_echo",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(fake_echo_manifest["id"], "fake_echo");
    assert_eq!(fake_echo_manifest["source"], "first_party");

    let plugin_dir = temp_dir.path().join("local-plugin");
    fs::create_dir(&plugin_dir).expect("create local plugin dir");
    let plugin_dir = plugin_dir.canonicalize().expect("canonical plugin dir");
    let plugin_manifest_path = plugin_dir.join("jarvis-plugin.json");
    fs::write(
        &plugin_manifest_path,
        json!({
            "manifest_schema_version": 1,
            "id": "local_e2e_plugin",
            "name": "Local E2E Plugin",
            "version": "0.1.0",
            "source": "local_development",
            "author": "Jarvis E2E",
            "source_path": plugin_dir.display().to_string(),
            "actions": [{
                "name": "inspect",
                "description": "Validate local install metadata.",
                "permissions": ["read_workspace"],
                "risk_tier": "low",
                "input_schema": { "schema": { "type": "object" } },
                "output_schema": { "schema": { "type": "object" } },
                "proactive": false,
                "memory_access": "none",
                "model_access": "none",
                "audit_fields": ["path"],
                "timeout": { "timeout_ms": 5000, "on_timeout": "cancel" },
                "cancellation": "cooperative"
            }]
        })
        .to_string(),
    )
    .expect("write local plugin manifest");

    let installed_plugin = run_cli_json([
        "plugins",
        "install",
        plugin_manifest_path.to_str().expect("manifest path"),
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(installed_plugin["id"], "local_e2e_plugin");
    assert_eq!(installed_plugin["execution_enabled"], false);
    assert_eq!(installed_plugin["manifest"]["source"], "local_development");

    let installed_plugins = run_cli_json(["plugins", "installed", "--endpoint", endpoint.as_str()]);
    assert_array_contains(&installed_plugins, "id", "local_e2e_plugin");

    let installed_plugin_get = run_cli_json([
        "plugins",
        "installed-get",
        "local_e2e_plugin",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(installed_plugin_get["id"], "local_e2e_plugin");
    assert_eq!(installed_plugin_get["execution_enabled"], false);

    let tasks = run_cli_json(["tasks", "list", "--endpoint", endpoint.as_str()]);
    assert_array_contains(&tasks, "id", &task_id);

    let task_audit = run_cli_json([
        "tasks",
        "audit",
        "--task-id",
        task_id.as_str(),
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_array_contains(&task_audit, "event_type", "task_completed");
    assert_array_contains(&task_audit, "event_type", "plugin_completed");

    let all_audit = run_cli_json(["tasks", "audit", "--endpoint", endpoint.as_str()]);
    assert_array_contains(&all_audit, "event_type", "plugin_completed");

    let memory = run_cli_json([
        "memory",
        "create",
        "e2e",
        "ipc-contract",
        "persisted through jarvis-cli serve",
        "--provenance",
        "crates/jarvis-cli/tests/local_ipc_e2e.rs",
        "--sensitivity",
        "workspace",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(memory["category"], "e2e");
    assert_eq!(memory["key"], "ipc-contract");
    assert_eq!(memory["sensitivity"], "workspace");
    let memory_id = memory["id"].as_str().expect("memory id").to_string();

    let fetched_memory = run_cli_json([
        "memory",
        "get",
        memory_id.as_str(),
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(fetched_memory["id"], memory_id);
    assert_eq!(
        fetched_memory["value"],
        "persisted through jarvis-cli serve"
    );

    let updated_memory = run_cli_json([
        "memory",
        "update",
        memory_id.as_str(),
        "updated through jarvis-cli e2e",
        "--provenance",
        "local_ipc_e2e update",
        "--sensitivity",
        "workspace",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(updated_memory["value"], "updated through jarvis-cli e2e");

    let reviewed_memory = run_cli_json([
        "memory",
        "review",
        memory_id.as_str(),
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert!(reviewed_memory["reviewed_at"].is_string());

    let memory_list = run_cli_json(["memory", "list", "--endpoint", endpoint.as_str()]);
    assert_array_contains(&memory_list, "id", &memory_id);

    let scheduled = run_cli_json([
        "scheduler",
        "schedule",
        "local e2e job",
        "plugin status",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(scheduled["name"], "local e2e job");
    assert_eq!(scheduled["status"], "scheduled");
    let scheduler_id = scheduled["id"].as_str().expect("scheduler id").to_string();

    let scheduler_list = run_cli_json(["scheduler", "list", "--endpoint", endpoint.as_str()]);
    assert_array_contains(&scheduler_list, "id", &scheduler_id);

    let scheduler_item = run_cli_json([
        "scheduler",
        "get",
        scheduler_id.as_str(),
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(scheduler_item["id"], scheduler_id);
    assert_eq!(scheduler_item["command"], "plugin status");

    let diagnostics = run_cli_json(["diagnostics", "export", "--endpoint", endpoint.as_str()]);
    assert_eq!(diagnostics["repository_backed"], true);
    assert_eq!(diagnostics["schema_version"], 3);
    assert_eq!(
        diagnostics["health"]["contract"]["name"],
        "jarvis.local-ipc"
    );
    assert_eq!(diagnostics["active_memory_item_count"], 1);
    assert_array_contains(&diagnostics["scheduler_jobs"], "id", &scheduler_id);
    let diagnostics_encoded = serde_json::to_string(&diagnostics).expect("diagnostics JSON");
    assert!(!diagnostics_encoded.contains("updated through jarvis-cli e2e"));
    assert!(!diagnostics_encoded.contains("plugin status"));
    assert!(!diagnostics_encoded.contains("cross-process e2e"));

    let pause = run_cli_json([
        "pause",
        "--reason",
        "local ipc e2e",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(pause["paused"], true);
    assert_eq!(pause["reason"], "local ipc e2e");

    let blocked = run_cli_json([
        "command",
        "plugin echo blocked by pause",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(blocked["accepted"], false);
    assert_eq!(blocked["task"]["status"], "blocked");

    let resume = run_cli_json(["resume", "--endpoint", endpoint.as_str()]);
    assert_eq!(resume["paused"], false);

    let cancelled = run_cli_json([
        "scheduler",
        "cancel",
        scheduler_id.as_str(),
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(cancelled["status"], "cancelled");

    let pause_status = run_cli_json(["pause-status", "--endpoint", endpoint.as_str()]);
    assert_eq!(pause_status["paused"], false);

    server.stop();

    let mut restarted = JarvisServer::start(&db_path);
    let restarted_endpoint = restarted.endpoint();

    let restarted_health = run_cli_text(["health", "--endpoint", restarted_endpoint.as_str()]);
    assert!(
        restarted_health.contains("jarvis-core: ok"),
        "{restarted_health}"
    );
    assert!(
        restarted_health.contains("paused: false"),
        "{restarted_health}"
    );

    let persisted_tasks =
        run_cli_json(["tasks", "list", "--endpoint", restarted_endpoint.as_str()]);
    assert_array_contains(&persisted_tasks, "id", &task_id);

    let persisted_task = run_cli_json([
        "tasks",
        "get",
        task_id.as_str(),
        "--endpoint",
        restarted_endpoint.as_str(),
    ]);
    assert_eq!(persisted_task["id"], task_id);
    assert_eq!(persisted_task["status"], "completed");

    let persisted_audit =
        run_cli_json(["tasks", "audit", "--endpoint", restarted_endpoint.as_str()]);
    assert_array_contains(&persisted_audit, "event_type", "plugin_completed");

    let persisted_memory =
        run_cli_json(["memory", "list", "--endpoint", restarted_endpoint.as_str()]);
    assert_array_contains(&persisted_memory, "id", &memory_id);
    assert_array_contains(&persisted_memory, "key", "ipc-contract");

    let persisted_scheduler = run_cli_json([
        "scheduler",
        "list",
        "--endpoint",
        restarted_endpoint.as_str(),
    ]);
    assert_array_contains(&persisted_scheduler, "id", &scheduler_id);

    let persisted_installed_plugins = run_cli_json([
        "plugins",
        "installed",
        "--endpoint",
        restarted_endpoint.as_str(),
    ]);
    assert_array_contains(&persisted_installed_plugins, "id", "local_e2e_plugin");

    let persisted_diagnostics = run_cli_json([
        "diagnostics",
        "export",
        "--endpoint",
        restarted_endpoint.as_str(),
    ]);
    assert_eq!(persisted_diagnostics["repository_backed"], true);
    assert_array_contains(
        &persisted_diagnostics["scheduler_jobs"],
        "id",
        &scheduler_id,
    );

    let persisted_pause = run_cli_json(["pause-status", "--endpoint", restarted_endpoint.as_str()]);
    assert_eq!(persisted_pause["paused"], false);

    let deleted_memory = run_cli_json([
        "memory",
        "delete",
        memory_id.as_str(),
        "--endpoint",
        restarted_endpoint.as_str(),
    ]);
    assert!(deleted_memory["deleted_at"].is_string());

    let deleted_memory_list = run_cli_json([
        "memory",
        "list",
        "--include-deleted",
        "--endpoint",
        restarted_endpoint.as_str(),
    ]);
    assert_array_contains(&deleted_memory_list, "id", &memory_id);

    let active_memory_after_delete =
        run_cli_json(["memory", "list", "--endpoint", restarted_endpoint.as_str()]);
    assert_array_lacks(&active_memory_after_delete, "id", &memory_id);

    restarted.stop();
    drop(temp_dir);
}

#[test]
#[ignore = "opt-in release proof; spawns jarvis smoke and duplicates broader CLI coverage"]
fn cli_smoke_command_is_release_gate_compatible() {
    let output = run_cli(["smoke"]);
    let stdout = String::from_utf8(output.stdout).expect("stdout is UTF-8");
    assert!(
        stdout.contains("jarvis smoke: ok"),
        "unexpected smoke stdout: {stdout}"
    );
}

struct JarvisServer {
    child: Option<Child>,
    endpoint: String,
    _temp_dir: TempDir,
}

impl JarvisServer {
    fn start(db_path: &Path) -> Self {
        let bind = unused_loopback_addr();
        let endpoint = format!("http://{bind}");
        let temp_dir = tempfile::tempdir().expect("server temp dir");
        let child = Command::new(BIN)
            .args([
                "serve",
                "--bind",
                &bind.to_string(),
                "--db-path",
                db_path.to_str().expect("db path is valid UTF-8"),
            ])
            .current_dir(temp_dir.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start jarvis serve");

        let mut server = Self {
            child: Some(child),
            endpoint,
            _temp_dir: temp_dir,
        };
        server.wait_until_healthy();
        server
    }

    fn endpoint(&self) -> String {
        self.endpoint.clone()
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            if child.try_wait().expect("check server status").is_none() {
                child.kill().expect("kill jarvis serve");
            }
            child.wait().expect("wait for jarvis serve");
        }
    }

    fn wait_until_healthy(&mut self) {
        let mut last_error = None;
        for _ in 0..80 {
            if let Some(status) = self
                .child
                .as_mut()
                .expect("server child")
                .try_wait()
                .expect("check server status")
            {
                panic!("jarvis serve exited before health check: {status}");
            }

            match request(&self.endpoint, "GET", "/health", None) {
                Ok(response) => {
                    let health: Value = serde_json::from_str(&response).expect("health JSON");
                    assert_eq!(health["status"], "ok");
                    return;
                }
                Err(error) => {
                    last_error = Some(error);
                    std::thread::sleep(Duration::from_millis(25));
                }
            }
        }

        panic!(
            "jarvis serve did not become healthy: {}",
            last_error.unwrap_or_else(|| "no request attempted".to_string())
        );
    }
}

impl Drop for JarvisServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn unused_loopback_addr() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let addr = listener.local_addr().expect("read local addr");
    drop(listener);
    addr
}

fn run_cli_json<const N: usize>(args: [&str; N]) -> Value {
    let output = run_cli(args);
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "stdout was not JSON: {error}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn run_cli_text<const N: usize>(args: [&str; N]) -> String {
    let output = run_cli(args);
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

fn run_cli<const N: usize>(args: [&str; N]) -> Output {
    let output = Command::new(BIN)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("run jarvis cli");

    assert!(
        output.status.success(),
        "jarvis cli failed: {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn assert_array_contains(value: &Value, field: &str, expected: &str) {
    let array = value.as_array().unwrap_or_else(|| {
        panic!("expected array, got {}", json!(value));
    });
    assert!(
        array
            .iter()
            .any(|item| item.get(field).and_then(Value::as_str) == Some(expected)),
        "expected array to contain object with {field}={expected}, got {value}"
    );
}

fn assert_string_array_contains(value: &Value, expected: &str) {
    let array = value.as_array().unwrap_or_else(|| {
        panic!("expected array, got {}", json!(value));
    });
    assert!(
        array.iter().any(|item| item.as_str() == Some(expected)),
        "expected array to contain {expected}, got {value}"
    );
}

fn assert_array_lacks(value: &Value, field: &str, expected: &str) {
    let array = value.as_array().unwrap_or_else(|| {
        panic!("expected array, got {}", json!(value));
    });
    assert!(
        !array
            .iter()
            .any(|item| item.get(field).and_then(Value::as_str) == Some(expected)),
        "expected array not to contain object with {field}={expected}, got {value}"
    );
}

fn request(endpoint: &str, method: &str, path: &str, body: Option<&str>) -> Result<String, String> {
    let target = endpoint
        .strip_prefix("http://")
        .ok_or_else(|| format!("only http:// endpoints are supported: {endpoint}"))?;
    let host_port = target.trim_end_matches('/');
    let address = host_port
        .to_socket_addrs()
        .map_err(|error| error.to_string())?
        .next()
        .ok_or_else(|| format!("could not resolve endpoint: {endpoint}"))?;
    let mut stream = TcpStream::connect(address).map_err(|error| error.to_string())?;
    let body = body.unwrap_or("");
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host_port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );

    stream
        .write_all(request.as_bytes())
        .map_err(|error| error.to_string())?;
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .map_err(|error| error.to_string())?;

    let Some((headers, response_body)) = response.split_once("\r\n\r\n") else {
        return Ok(response);
    };

    if !headers.starts_with("HTTP/1.1 2") {
        return Err(format!("{headers}\n\n{response_body}"));
    }

    Ok(response_body.to_string())
}
