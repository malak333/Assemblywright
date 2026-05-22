use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use ed25519_dalek::{Signer, SigningKey};
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
    assert_array_contains(&contract["endpoints"], "path", "/permissions/grants");
    assert_array_contains(&contract["endpoints"], "path", "/permissions/policy-review");
    assert_array_contains(&contract["endpoints"], "path", "/model-routes");
    assert_array_contains(&contract["endpoints"], "path", "/activity/summary");
    assert_array_contains(&contract["endpoints"], "path", "/activity/events");
    assert_array_contains(&contract["endpoints"], "path", "/approvals/:id/approve");
    assert_array_contains(
        &contract["endpoints"],
        "path",
        "/plugins/installed/:id/execution",
    );
    assert_array_contains(
        &contract["endpoints"],
        "path",
        "/plugins/installed/:id/publisher/verify",
    );
    assert_array_contains(
        &contract["endpoints"],
        "path",
        "/plugins/installed/:id/publisher/signature/verify",
    );
    assert_array_contains(&contract["endpoints"], "path", "/plugins/installed/:id/run");
    assert_string_array_contains(&contract["safe_inspection_paths"], "/model-routes");
    assert_string_array_contains(&contract["safe_inspection_paths"], "/model-routes/:id");
    assert_string_array_contains(&contract["safe_inspection_paths"], "/scheduler/attention");
    assert_string_array_contains(&contract["safe_inspection_paths"], "/scheduler/jobs/:id");
    assert_string_array_contains(&contract["safe_inspection_paths"], "/permissions/grants");
    assert_string_array_contains(
        &contract["safe_inspection_paths"],
        "/permissions/policy-review",
    );
    assert_string_array_contains(&contract["safe_inspection_paths"], "/approvals/:id");
    assert_string_array_contains(&contract["safe_inspection_paths"], "/activity/summary");
    assert_string_array_contains(&contract["safe_inspection_paths"], "/activity/events");

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
    let route_id = command["route_evidence"]["id"]
        .as_str()
        .expect("command route id")
        .to_string();

    let routes = run_cli_json(["routes", "list", "--endpoint", endpoint.as_str()]);
    assert_array_contains(&routes, "id", &route_id);
    assert_array_contains(&routes, "task_id", &task_id);
    assert_eq!(routes[0]["context_for_model"], Value::Null);
    let scoped_routes = run_cli_json([
        "routes",
        "list",
        "--task-id",
        task_id.as_str(),
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_array_contains(&scoped_routes, "id", &route_id);
    let route = run_cli_json([
        "routes",
        "get",
        route_id.as_str(),
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(route["id"], route_id);
    assert_eq!(route["context_for_model"], Value::Null);

    let manifests = run_cli_json(["plugins", "list", "--endpoint", endpoint.as_str()]);
    assert_array_contains(&manifests, "id", "fake_echo");
    assert_array_contains(&manifests, "id", "fake_status");

    let activity = run_cli_json(["activity", "summary", "--endpoint", endpoint.as_str()]);
    assert_eq!(activity["repository_backed"], true);
    assert!(
        activity["task_count"].as_u64().unwrap_or_default() >= 1,
        "{activity}"
    );
    assert!(
        activity["audit_entry_count"].as_u64().unwrap_or_default() >= 1,
        "{activity}"
    );
    assert_array_contains(&activity["status_counts"], "status", "completed");
    assert_array_contains(&activity["recent_tasks"], "id", &task_id);
    assert_array_contains(
        &activity["recent_audit_entries"],
        "event_type",
        "plugin_completed",
    );
    let activity_events = run_cli_text([
        "activity",
        "watch",
        "--max-events",
        "2",
        "--interval-ms",
        "100",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert!(
        activity_events.matches("event: activity_summary").count() >= 2,
        "{activity_events}"
    );
    assert!(
        activity_events.contains("\"task_count\""),
        "{activity_events}"
    );

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
    let local_signing_key = SigningKey::from_bytes(&[7_u8; 32]);
    let local_trusted_public_key =
        BASE64_STANDARD.encode(local_signing_key.verifying_key().as_bytes());
    let local_manifest = signed_manifest(
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
                "permissions": ["read_workspace", "network"],
                "risk_tier": "low",
                "input_schema": {
                    "schema": {
                        "type": "object",
                        "properties": { "path": { "type": "string" } },
                        "required": ["path"],
                        "additionalProperties": false
                    }
                },
                "output_schema": { "schema": { "type": "object" } },
                "proactive": false,
                "memory_access": "none",
                "model_access": "none",
                "network_access": {
                    "mode": "declared_hosts",
                    "allowed_hosts": ["api.jarvis.local"]
                },
                "audit_fields": ["path"],
                "timeout": { "timeout_ms": 5000, "on_timeout": "cancel" },
                "cancellation": "cooperative"
            }]
        }),
        &local_signing_key,
    );
    fs::write(
        &plugin_manifest_path,
        serde_json::to_string(&local_manifest).expect("serialize signed local manifest"),
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
    assert_eq!(installed_plugin["execution_grant"], "metadata_only");
    assert_eq!(installed_plugin["manifest"]["source"], "local_development");
    assert_eq!(
        installed_plugin["provenance"]["integrity_status"],
        "not_verified"
    );
    assert_eq!(
        installed_plugin["provenance"]["capture_method"],
        "local_manifest_snapshot"
    );
    assert!(
        installed_plugin["provenance"]["manifest_sha256"]
            .as_str()
            .expect("manifest hash")
            .len()
            == 64
    );

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
    assert_eq!(installed_plugin_get["execution_grant"], "metadata_only");

    let initial_grants = run_cli_json(["permissions", "grants", "--endpoint", endpoint.as_str()]);
    assert_eq!(initial_grants["executable_installed_plugin_count"], 0);
    assert_eq!(initial_grants["unverified_installed_plugin_count"], 1);
    assert_eq!(initial_grants["side_effects_require_approval"], true);
    assert_array_contains(
        &initial_grants["installed_plugin_grants"],
        "plugin_id",
        "local_e2e_plugin",
    );
    assert_array_contains(
        &initial_grants["installed_plugin_grants"],
        "execution_grant",
        "metadata_only",
    );
    assert_array_contains(
        &initial_grants["installed_plugin_grants"],
        "integrity_status",
        "not_verified",
    );
    assert_array_contains(
        &initial_grants["installed_plugin_grants"],
        "capture_method",
        "local_manifest_snapshot",
    );

    let initial_policy_review =
        run_cli_json(["permissions", "review", "--endpoint", endpoint.as_str()]);
    assert_eq!(initial_policy_review["status"], "review_required");
    assert_eq!(
        initial_policy_review["unverified_installed_plugin_count"],
        1
    );
    assert_array_contains(
        &initial_policy_review["items"],
        "item_type",
        "installed_plugin_provenance",
    );
    assert_array_contains(
        &initial_policy_review["items"],
        "item_type",
        "publisher_identity",
    );
    assert_array_contains(
        &initial_policy_review["items"],
        "item_type",
        "network_plugin_action",
    );
    assert_array_contains(&initial_policy_review["items"], "severity", "medium");

    let premature_publisher_verification = run_cli_failure([
        "plugins",
        "verify-publisher",
        "local_e2e_plugin",
        "--trusted-origin",
        "Jarvis E2E",
        "--decided-by",
        "local_ipc_e2e",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert!(premature_publisher_verification.contains("requires local provenance"));

    let premature_signature_verification = run_cli_failure([
        "plugins",
        "verify-publisher-signature",
        "local_e2e_plugin",
        "--trusted-public-key",
        local_trusted_public_key.as_str(),
        "--decided-by",
        "local_ipc_e2e",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert!(premature_signature_verification.contains("requires local provenance"));

    let blocked_installed_run = run_cli_json([
        "plugins",
        "run-installed",
        "local_e2e_plugin",
        "inspect",
        "--input",
        r#"{"path":"README.md"}"#,
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(blocked_installed_run["status"], "blocked");
    assert_eq!(blocked_installed_run["execution_enabled"], false);
    assert_eq!(blocked_installed_run["execution_grant"], "metadata_only");
    assert_eq!(blocked_installed_run["manifest_valid"], true);
    assert_eq!(blocked_installed_run["action_declared"], true);
    assert_eq!(blocked_installed_run["input_valid"], true);
    assert_eq!(blocked_installed_run["contract_validated"], true);
    assert_eq!(blocked_installed_run["side_effect_executed"], false);
    assert_eq!(
        blocked_installed_run["audit_entry"]["event_type"],
        "installed_plugin_execution_blocked"
    );

    let contract_dry_run = run_cli_json([
        "plugins",
        "run-installed",
        "local_e2e_plugin",
        "inspect",
        "--input",
        r#"{"path":"README.md"}"#,
        "--dry-run",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(contract_dry_run["status"], "dry_run");
    assert_eq!(contract_dry_run["execution_grant"], "metadata_only");
    assert_eq!(contract_dry_run["input_valid"], true);
    assert_eq!(contract_dry_run["contract_validated"], true);
    assert_eq!(contract_dry_run["side_effect_executed"], false);
    assert_eq!(
        contract_dry_run["audit_entry"]["event_type"],
        "installed_plugin_contract_dry_run"
    );

    let verified_local_plugin = run_cli_json([
        "plugins",
        "verify-installed",
        "local_e2e_plugin",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(
        verified_local_plugin["provenance"]["integrity_status"],
        "matches_install_snapshot"
    );
    assert_eq!(
        verified_local_plugin["provenance"]["origin_claim_verified"],
        false
    );

    let wrong_trusted_public_key = BASE64_STANDARD.encode(
        SigningKey::from_bytes(&[8_u8; 32])
            .verifying_key()
            .as_bytes(),
    );
    let wrong_signature_verification = run_cli_failure([
        "plugins",
        "verify-publisher-signature",
        "local_e2e_plugin",
        "--trusted-public-key",
        wrong_trusted_public_key.as_str(),
        "--decided-by",
        "local_ipc_e2e",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert!(wrong_signature_verification.contains("does not match trusted_public_key"));

    let signature_verified = run_cli_json([
        "plugins",
        "verify-publisher-signature",
        "local_e2e_plugin",
        "--trusted-public-key",
        local_trusted_public_key.as_str(),
        "--decided-by",
        "local_ipc_e2e",
        "--reason",
        "trusted test signing key",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(
        signature_verified["provenance"]["origin_claim_verified"],
        true
    );

    let wrong_publisher_verification = run_cli_failure([
        "plugins",
        "verify-publisher",
        "local_e2e_plugin",
        "--trusted-origin",
        "Someone Else",
        "--decided-by",
        "local_ipc_e2e",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert!(wrong_publisher_verification.contains("trusted_origin must exactly match"));

    let publisher_verified = run_cli_json([
        "plugins",
        "verify-publisher",
        "local_e2e_plugin",
        "--trusted-origin",
        "Jarvis E2E",
        "--decided-by",
        "local_ipc_e2e",
        "--reason",
        "local test operator pinned the manifest author",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(
        publisher_verified["provenance"]["origin_claim_verified"],
        true
    );

    let publisher_policy_review =
        run_cli_json(["permissions", "review", "--endpoint", endpoint.as_str()]);
    assert_array_not_contains(
        &publisher_policy_review["items"],
        "item_type",
        "publisher_identity",
    );
    assert_array_contains(
        &publisher_policy_review["items"],
        "item_type",
        "network_plugin_action",
    );

    let subprocess_plugin_dir = temp_dir.path().join("local-subprocess-plugin");
    fs::create_dir(&subprocess_plugin_dir).expect("create subprocess plugin dir");
    let subprocess_plugin_dir = subprocess_plugin_dir
        .canonicalize()
        .expect("canonical subprocess plugin dir");
    write_executable_plugin_script(&subprocess_plugin_dir);
    let subprocess_manifest_path = subprocess_plugin_dir.join("jarvis-plugin.json");
    fs::write(
        &subprocess_manifest_path,
        json!({
            "manifest_schema_version": 1,
            "id": "local_subprocess_e2e",
            "name": "Local Subprocess E2E Plugin",
            "version": "0.1.0",
            "source": "local_subprocess",
            "author": "Jarvis E2E",
            "source_path": subprocess_plugin_dir.display().to_string(),
            "subprocess": {
                "command": "plugin-runner.py",
                "args": [],
                "stdin": "json",
                "stdout": "json"
            },
            "actions": [{
                "name": "inspect",
                "description": "Validate local subprocess execution.",
                "permissions": ["read_workspace"],
                "risk_tier": "low",
                "input_schema": {
                    "schema": {
                        "type": "object",
                        "properties": { "path": { "type": "string" } },
                        "required": ["path"],
                        "additionalProperties": false
                    }
                },
                "output_schema": {
                    "schema": {
                        "type": "object",
                        "properties": { "path": { "type": "string" } },
                        "required": ["path"],
                        "additionalProperties": false
                    }
                },
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
    .expect("write subprocess plugin manifest");

    let subprocess_installed = run_cli_json([
        "plugins",
        "install",
        subprocess_manifest_path.to_str().expect("manifest path"),
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(subprocess_installed["id"], "local_subprocess_e2e");
    assert_eq!(subprocess_installed["execution_enabled"], false);
    assert_eq!(subprocess_installed["execution_grant"], "metadata_only");
    assert_eq!(
        subprocess_installed["provenance"]["integrity_status"],
        "not_verified"
    );
    assert!(
        subprocess_installed["provenance"]["subprocess_command_sha256"]
            .as_str()
            .expect("subprocess hash")
            .len()
            == 64
    );

    let subprocess_enable_unverified = run_cli_failure([
        "plugins",
        "enable-installed",
        "local_subprocess_e2e",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert!(subprocess_enable_unverified.contains("requires local provenance verification"));

    let subprocess_verified = run_cli_json([
        "plugins",
        "verify-installed",
        "local_subprocess_e2e",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(
        subprocess_verified["provenance"]["integrity_status"],
        "matches_install_snapshot"
    );

    let subprocess_publisher_verified = run_cli_json([
        "plugins",
        "verify-publisher",
        "local_subprocess_e2e",
        "--trusted-origin",
        "Jarvis E2E",
        "--decided-by",
        "local_ipc_e2e",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(
        subprocess_publisher_verified["provenance"]["origin_claim_verified"],
        true
    );

    let subprocess_enabled = run_cli_json([
        "plugins",
        "enable-installed",
        "local_subprocess_e2e",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(subprocess_enabled["execution_enabled"], true);
    assert_eq!(subprocess_enabled["execution_grant"], "subprocess_stdio");

    let subprocess_run = run_cli_json([
        "plugins",
        "run-installed",
        "local_subprocess_e2e",
        "inspect",
        "--input",
        r#"{"path":"README.md"}"#,
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(subprocess_run["status"], "completed");
    assert_eq!(subprocess_run["execution_enabled"], true);
    assert_eq!(subprocess_run["execution_grant"], "subprocess_stdio");
    assert_eq!(
        subprocess_run["provenance"]["integrity_status"],
        "matches_install_snapshot"
    );
    assert_eq!(subprocess_run["output"]["path"], "README.md");
    assert_eq!(subprocess_run["side_effect_executed"], true);
    assert_eq!(
        subprocess_run["audit_entry"]["event_type"],
        "installed_plugin_subprocess_completed"
    );
    assert_eq!(
        subprocess_run["audit_entry"]["payload"]["sandbox_process_started"],
        true
    );

    let unsafe_manifest_path = subprocess_plugin_dir.join("unsafe-plugin.json");
    fs::write(
        &unsafe_manifest_path,
        json!({
            "manifest_schema_version": 1,
            "id": "unsafe_subprocess_e2e",
            "name": "Unsafe Subprocess E2E Plugin",
            "version": "0.1.0",
            "source": "local_subprocess",
            "author": "Jarvis E2E",
            "source_path": subprocess_plugin_dir.display().to_string(),
            "subprocess": {
                "command": "/bin/echo",
                "args": [],
                "stdin": "json",
                "stdout": "json"
            },
            "actions": [{
                "name": "inspect",
                "description": "Should be blocked because command escapes source_path.",
                "permissions": ["read_workspace"],
                "risk_tier": "low",
                "input_schema": { "schema": { "type": "object" } },
                "output_schema": { "schema": { "type": "object" } },
                "proactive": false,
                "memory_access": "none",
                "model_access": "none",
                "audit_fields": [],
                "timeout": { "timeout_ms": 5000, "on_timeout": "cancel" },
                "cancellation": "cooperative"
            }]
        })
        .to_string(),
    )
    .expect("write unsafe subprocess manifest");
    let unsafe_install = run_cli_failure([
        "plugins",
        "install",
        unsafe_manifest_path.to_str().expect("manifest path"),
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert!(unsafe_install.contains("subprocess command must live under source_path"));

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
    assert_array_contains(
        &all_audit,
        "event_type",
        "installed_plugin_execution_blocked",
    );
    assert_array_contains(
        &all_audit,
        "event_type",
        "installed_plugin_publisher_verified",
    );
    assert_array_contains(
        &all_audit,
        "event_type",
        "installed_plugin_publisher_signature_verified",
    );

    let approval_command = run_cli_json([
        "command",
        "plugin approval echo needs user approval",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(approval_command["accepted"], false);
    assert_eq!(approval_command["task"]["status"], "waiting_for_approval");
    assert_eq!(
        approval_command["plugin_results"][0]["status"],
        "approval_required"
    );
    assert_eq!(
        approval_command["plugin_results"][0]["metadata"]["approval_status"],
        "pending"
    );
    assert_array_contains(
        &approval_command["audit_entries"],
        "event_type",
        "approval_pending",
    );
    assert_array_contains(
        &approval_command["audit_entries"],
        "event_type",
        "plugin_approval_required",
    );
    let approval_task_id = approval_command["task"]["id"]
        .as_str()
        .expect("approval task id")
        .to_string();

    let pending_approvals = run_cli_json([
        "approvals",
        "list",
        "--status",
        "pending",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_array_contains(&pending_approvals, "task_id", &approval_task_id);
    let approval_id = pending_approvals[0]["id"]
        .as_str()
        .expect("approval id")
        .to_string();
    assert_eq!(pending_approvals[0]["status"], "pending");
    assert_eq!(pending_approvals[0]["action"], "fake_echo.approval_echo");
    assert_eq!(pending_approvals[0]["risk_tier"], "confirm");

    let approval_detail = run_cli_json([
        "approvals",
        "get",
        approval_id.as_str(),
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(approval_detail["id"], approval_id);
    assert_eq!(approval_detail["task_id"], approval_task_id);

    let approved = run_cli_json([
        "approvals",
        "approve",
        approval_id.as_str(),
        "--decided-by",
        "local_ipc_e2e",
        "--reason",
        "reviewed deterministic approval scaffold",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(approved["status"], "approved");
    assert_eq!(approved["decided_by"], "local_ipc_e2e");

    let approval_audit = run_cli_json([
        "tasks",
        "audit",
        "--task-id",
        approval_task_id.as_str(),
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_array_contains(&approval_audit, "event_type", "approval_granted");
    let approval_audit_encoded =
        serde_json::to_string(&approval_audit).expect("approval audit JSON");
    assert!(
        approval_audit_encoded.contains("\"side_effect_executed\":false"),
        "{approval_audit_encoded}"
    );

    let deny_command = run_cli_json([
        "command",
        "plugin approval echo deny this approval",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(deny_command["task"]["status"], "waiting_for_approval");
    let deny_task_id = deny_command["task"]["id"]
        .as_str()
        .expect("deny approval task id")
        .to_string();
    let deny_pending = run_cli_json([
        "approvals",
        "list",
        "--status",
        "pending",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_array_contains(&deny_pending, "task_id", &deny_task_id);
    let deny_approval_id = deny_pending
        .as_array()
        .expect("pending approvals array")
        .iter()
        .find(|approval| approval["task_id"].as_str() == Some(deny_task_id.as_str()))
        .and_then(|approval| approval["id"].as_str())
        .expect("deny approval id")
        .to_string();
    let denied = run_cli_json([
        "approvals",
        "deny",
        deny_approval_id.as_str(),
        "--decided-by",
        "local_ipc_e2e",
        "--reason",
        "not safe enough",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(denied["status"], "denied");

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

    let memory_classification =
        run_cli_json(["memory", "classification", "--endpoint", endpoint.as_str()]);
    assert_eq!(memory_classification["active_count"], 1);
    assert_eq!(memory_classification["deleted_count"], 0);
    assert_array_contains(
        &memory_classification["by_sensitivity"],
        "label",
        "workspace",
    );
    assert_array_contains(&memory_classification["by_category"], "label", "e2e");

    let scheduled = run_cli_json([
        "scheduler",
        "schedule",
        "local e2e job",
        "plugin status",
        "--once-at",
        "2999-01-01T00:00:00Z",
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

    let scheduler_policy_review =
        run_cli_json(["permissions", "review", "--endpoint", endpoint.as_str()]);
    assert_eq!(scheduler_policy_review["status"], "review_required");
    assert_array_contains(
        &scheduler_policy_review["items"],
        "item_type",
        "scheduled_scheduler_trigger",
    );
    assert_array_contains(&scheduler_policy_review["items"], "action", &scheduler_id);
    assert!(!serde_json::to_string(&scheduler_policy_review)
        .expect("scheduler policy review JSON")
        .contains("plugin status"));

    let due_once = run_cli_json([
        "scheduler",
        "schedule",
        "due once e2e job",
        "plugin status",
        "--once-at",
        "2020-01-01T00:00:00Z",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let due_once_id = due_once["id"].as_str().expect("due once id").to_string();
    let scheduler_attention =
        run_cli_json(["scheduler", "attention", "--endpoint", endpoint.as_str()]);
    assert_eq!(scheduler_attention["attention_required"], true);
    assert_eq!(scheduler_attention["due_count"], 1);
    assert_array_contains(
        &scheduler_attention["items"],
        "notification_kind",
        "due_now",
    );
    assert!(!serde_json::to_string(&scheduler_attention)
        .expect("scheduler attention JSON")
        .contains("plugin status"));
    let due_interval = run_cli_json([
        "scheduler",
        "schedule",
        "due interval e2e job",
        "plugin status",
        "--interval-seconds",
        "1",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let due_interval_id = due_interval["id"]
        .as_str()
        .expect("due interval id")
        .to_string();
    let recurring_policy_review =
        run_cli_json(["permissions", "review", "--endpoint", endpoint.as_str()]);
    assert_array_contains(
        &recurring_policy_review["items"],
        "item_type",
        "recurring_scheduler_trigger",
    );
    assert_array_contains(
        &recurring_policy_review["items"],
        "action",
        &due_interval_id,
    );
    assert!(!serde_json::to_string(&recurring_policy_review)
        .expect("recurring scheduler policy review JSON")
        .contains("plugin status"));
    std::thread::sleep(Duration::from_millis(1100));
    let run_due = run_cli_json([
        "scheduler",
        "run-due",
        "--limit",
        "8",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(run_due["emergency_paused"], false);
    assert!(run_due["executions"]
        .as_array()
        .expect("executions array")
        .iter()
        .any(|execution| execution["accepted"] == true));
    assert_array_contains_nested(&run_due["executions"], &["job", "id"], &due_once_id);
    assert_array_contains_nested(&run_due["executions"], &["job", "id"], &due_interval_id);
    assert_array_contains_nested(&run_due["executions"], &["job", "status"], "completed");
    assert_array_contains_nested(&run_due["executions"], &["job", "status"], "scheduled");

    let diagnostics = run_cli_json(["diagnostics", "export", "--endpoint", endpoint.as_str()]);
    assert_eq!(diagnostics["repository_backed"], true);
    assert_eq!(diagnostics["schema_version"], 8);
    assert_eq!(
        diagnostics["health"]["contract"]["name"],
        "jarvis.local-ipc"
    );
    assert_eq!(diagnostics["active_memory_item_count"], 1);
    assert!(
        diagnostics["model_route_record_count"]
            .as_u64()
            .unwrap_or(0)
            >= 1
    );
    assert_array_contains(&diagnostics["scheduler_jobs"], "id", &scheduler_id);
    let diagnostics_encoded = serde_json::to_string(&diagnostics).expect("diagnostics JSON");
    assert!(!diagnostics_encoded.contains("updated through jarvis-cli e2e"));
    assert!(!diagnostics_encoded.contains("plugin status"));
    assert!(!diagnostics_encoded.contains("cross-process e2e"));

    let fail_closed_job = run_cli_json([
        "scheduler",
        "schedule",
        "fail closed approval e2e job",
        "plugin approval echo scheduler should pause",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let fail_closed_id = fail_closed_job["id"]
        .as_str()
        .expect("fail closed scheduler id")
        .to_string();
    let fail_closed_cancelled_job = run_cli_json([
        "scheduler",
        "schedule",
        "cancelled by fail closed e2e job",
        "plugin status",
        "--once-at",
        "2999-01-01T00:00:00Z",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let fail_closed_cancelled_id = fail_closed_cancelled_job["id"]
        .as_str()
        .expect("fail closed cancelled scheduler id")
        .to_string();
    let fail_closed_run = run_cli_json([
        "scheduler",
        "run-due",
        "--limit",
        "1",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(fail_closed_run["emergency_paused"], true);
    assert_eq!(fail_closed_run["executions"][0]["accepted"], false);
    assert_eq!(
        fail_closed_run["executions"][0]["job"]["id"],
        fail_closed_id
    );
    assert_eq!(fail_closed_run["executions"][0]["job"]["status"], "failed");
    assert_array_contains(
        &fail_closed_run["executions"][0]["audit_entries"],
        "event_type",
        "scheduler_fail_closed_emergency_pause",
    );
    let fail_closed_pause_status = run_cli_json(["pause-status", "--endpoint", endpoint.as_str()]);
    assert_eq!(fail_closed_pause_status["paused"], true);
    let cancelled_by_fail_closed = run_cli_json([
        "scheduler",
        "get",
        fail_closed_cancelled_id.as_str(),
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(cancelled_by_fail_closed["status"], "cancelled");
    let fail_closed_audit = run_cli_json(["tasks", "audit", "--endpoint", endpoint.as_str()]);
    assert_array_contains(
        &fail_closed_audit,
        "event_type",
        "scheduler_fail_closed_emergency_pause",
    );
    assert_array_contains(
        &fail_closed_audit,
        "event_type",
        "emergency_pause_activated",
    );

    let resume_after_fail_closed = run_cli_json(["resume", "--endpoint", endpoint.as_str()]);
    assert_eq!(resume_after_fail_closed["paused"], false);

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
    assert_array_contains(&persisted_audit, "event_type", "approval_granted");

    let persisted_routes =
        run_cli_json(["routes", "list", "--endpoint", restarted_endpoint.as_str()]);
    assert_array_contains(&persisted_routes, "id", &route_id);
    assert_array_contains(&persisted_routes, "task_id", &task_id);
    let persisted_route = run_cli_json([
        "routes",
        "get",
        route_id.as_str(),
        "--endpoint",
        restarted_endpoint.as_str(),
    ]);
    assert_eq!(persisted_route["id"], route_id);
    assert_eq!(persisted_route["task_id"], task_id);
    assert_eq!(persisted_route["context_for_model"], Value::Null);
    let persisted_route_encoded =
        serde_json::to_string(&persisted_route).expect("persisted route JSON");
    assert!(!persisted_route_encoded.contains("cross-process e2e"));

    let persisted_approvals = run_cli_json([
        "approvals",
        "list",
        "--endpoint",
        restarted_endpoint.as_str(),
    ]);
    assert_array_contains(&persisted_approvals, "id", &approval_id);
    assert_array_contains(&persisted_approvals, "id", &deny_approval_id);

    let persisted_grants = run_cli_json([
        "permissions",
        "grants",
        "--endpoint",
        restarted_endpoint.as_str(),
    ]);
    assert_array_contains(&persisted_grants["approval_counts"], "status", "approved");
    assert_array_contains(&persisted_grants["approval_counts"], "status", "denied");
    assert_array_contains(
        &persisted_grants["installed_plugin_grants"],
        "plugin_id",
        "local_e2e_plugin",
    );
    assert_array_contains(
        &persisted_grants["installed_plugin_grants"],
        "integrity_status",
        "matches_install_snapshot",
    );
    assert_eq!(persisted_grants["executable_installed_plugin_count"], 1);
    assert_eq!(persisted_grants["unverified_installed_plugin_count"], 0);
    assert_eq!(persisted_grants["side_effects_require_approval"], true);

    let persisted_policy_review = run_cli_json([
        "permissions",
        "review",
        "--endpoint",
        restarted_endpoint.as_str(),
    ]);
    assert_eq!(
        persisted_policy_review["executable_installed_plugin_count"],
        1
    );
    assert_eq!(
        persisted_policy_review["unverified_installed_plugin_count"],
        0
    );
    assert_eq!(
        persisted_policy_review["side_effects_require_approval"],
        true
    );

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

    let restored_memory = run_cli_json([
        "memory",
        "restore",
        memory_id.as_str(),
        "--endpoint",
        restarted_endpoint.as_str(),
    ]);
    assert_eq!(restored_memory["id"], memory_id);
    assert!(restored_memory["deleted_at"].is_null());

    let active_memory_after_restore =
        run_cli_json(["memory", "list", "--endpoint", restarted_endpoint.as_str()]);
    assert_array_contains(&active_memory_after_restore, "id", &memory_id);

    restarted.stop();
    drop(temp_dir);
}

#[test]
fn serve_background_scheduler_runs_due_jobs_and_honors_pause() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let db_path = temp_dir.path().join("jarvis-background-e2e.sqlite");
    let mut server = JarvisServer::start_with_background(&db_path, 50, 1);
    let endpoint = server.endpoint();

    let first = run_cli_json([
        "scheduler",
        "schedule",
        "background first e2e job",
        "plugin status",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let first_id = first["id"].as_str().expect("first id").to_string();
    let second = run_cli_json([
        "scheduler",
        "schedule",
        "background second e2e job",
        "plugin status",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let second_id = second["id"].as_str().expect("second id").to_string();

    wait_for_json(Duration::from_secs(3), || {
        let jobs = run_cli_json(["scheduler", "list", "--endpoint", endpoint.as_str()]);
        array_has_job_status(&jobs, &first_id, "completed")
            && array_has_job_status(&jobs, &second_id, "completed")
    });

    let tasks = run_cli_json(["tasks", "list", "--endpoint", endpoint.as_str()]);
    assert!(
        tasks.as_array().expect("tasks").len() >= 2,
        "expected background scheduler tasks, got {tasks}"
    );
    let audit = run_cli_json(["tasks", "audit", "--endpoint", endpoint.as_str()]);
    assert_array_contains(&audit, "event_type", "scheduler_job_due");
    assert_array_contains(&audit, "event_type", "scheduler_job_completed");

    let pause = run_cli_json([
        "pause",
        "--reason",
        "background e2e pause",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(pause["paused"], true);
    let paused_job = run_cli_json([
        "scheduler",
        "schedule",
        "background paused e2e job",
        "plugin status",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let paused_id = paused_job["id"].as_str().expect("paused id").to_string();
    std::thread::sleep(Duration::from_millis(160));
    let paused_status = run_cli_json([
        "scheduler",
        "get",
        paused_id.as_str(),
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(paused_status["status"], "scheduled");

    server.stop();
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
        Self::start_inner(db_path, None)
    }

    fn start_with_background(db_path: &Path, interval_ms: u64, limit: usize) -> Self {
        Self::start_inner(db_path, Some((interval_ms, limit)))
    }

    fn start_inner(db_path: &Path, scheduler_background: Option<(u64, usize)>) -> Self {
        let bind = unused_loopback_addr();
        let endpoint = format!("http://{bind}");
        let temp_dir = tempfile::tempdir().expect("server temp dir");
        let bind_arg = bind.to_string();
        let db_path_arg = db_path
            .to_str()
            .expect("db path is valid UTF-8")
            .to_string();
        let mut args = vec![
            "serve".to_string(),
            "--bind".to_string(),
            bind_arg,
            "--db-path".to_string(),
            db_path_arg,
        ];
        if let Some((interval_ms, limit)) = scheduler_background {
            args.extend([
                "--scheduler-background".to_string(),
                "--scheduler-interval-ms".to_string(),
                interval_ms.to_string(),
                "--scheduler-limit".to_string(),
                limit.to_string(),
            ]);
        }
        let child = Command::new(BIN)
            .args(args)
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

fn wait_for_json(timeout: Duration, mut condition: impl FnMut() -> bool) {
    let started = std::time::Instant::now();
    while started.elapsed() < timeout {
        if condition() {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("condition did not become true within {:?}", timeout);
}

fn array_has_job_status(value: &Value, id: &str, status: &str) -> bool {
    value
        .as_array()
        .expect("expected array")
        .iter()
        .any(|item| {
            item.get("id").and_then(Value::as_str) == Some(id)
                && item.get("status").and_then(Value::as_str) == Some(status)
        })
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

fn run_cli_failure<const N: usize>(args: [&str; N]) -> String {
    let output = Command::new(BIN)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("run jarvis cli");

    assert!(
        !output.status.success(),
        "jarvis cli unexpectedly succeeded: {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
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

fn signed_manifest(mut manifest: Value, signing_key: &SigningKey) -> Value {
    let typed_manifest: jarvis_core::PluginManifest =
        serde_json::from_value(manifest.clone()).expect("decode unsigned plugin manifest");
    let payload = serde_json::to_vec(&typed_manifest).expect("serialize unsigned plugin manifest");
    let signature = signing_key.sign(&payload);
    manifest["publisher_signature"] = json!({
        "scheme": "ed25519-v1",
        "public_key": BASE64_STANDARD.encode(signing_key.verifying_key().as_bytes()),
        "signature": BASE64_STANDARD.encode(signature.to_bytes()),
    });
    manifest
}

#[cfg(unix)]
fn write_executable_plugin_script(dir: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let script = dir.join("plugin-runner.py");
    fs::write(
        &script,
        r#"#!/usr/bin/env python3
import json
import sys

request = json.load(sys.stdin)
json.dump({"path": request["input"]["path"]}, sys.stdout)
"#,
    )
    .expect("write plugin runner");
    let mut permissions = fs::metadata(&script)
        .expect("script metadata")
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&script, permissions).expect("chmod plugin runner");
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

fn assert_array_not_contains(value: &Value, field: &str, unexpected: &str) {
    let array = value.as_array().unwrap_or_else(|| {
        panic!("expected array, got {}", json!(value));
    });
    assert!(
        !array
            .iter()
            .any(|item| item.get(field).and_then(Value::as_str) == Some(unexpected)),
        "expected array not to contain object with {field}={unexpected}, got {value}"
    );
}

fn assert_array_contains_nested(value: &Value, path: &[&str], expected: &str) {
    let array = value.as_array().unwrap_or_else(|| {
        panic!("expected array, got {}", json!(value));
    });
    assert!(
        array.iter().any(|item| {
            let mut cursor = item;
            for segment in path {
                let Some(next) = cursor.get(segment) else {
                    return false;
                };
                cursor = next;
            }
            cursor.as_str() == Some(expected)
        }),
        "expected array to contain object with {}={expected}, got {value}",
        path.join(".")
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
