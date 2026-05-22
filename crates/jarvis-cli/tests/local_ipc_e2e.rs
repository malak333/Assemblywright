use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use ed25519_dalek::{Signer, SigningKey};
use jarvis_core::SqliteRepository;
use serde_json::{json, Value};
use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_jarvis");

#[test]
fn release_readiness_cli_falls_back_without_running_server() {
    let endpoint = format!("http://{}", unused_loopback_addr());

    let release_readiness = run_cli_json(["release", "readiness", "--endpoint", endpoint.as_str()]);

    assert_eq!(release_readiness["production_ready"], false);
    assert_array_contains(
        &release_readiness["implemented_features"],
        "key",
        "installed_plugin_execution",
    );
    assert_array_contains(
        &release_readiness["implemented_features"],
        "key",
        "operator_release_qa_smoke",
    );
    assert_array_contains(
        &release_readiness["implemented_features"],
        "key",
        "unsigned_distribution_launch",
    );
    assert_array_contains(
        &release_readiness["implemented_features"],
        "key",
        "release_evidence_bundle",
    );
    assert_array_contains(
        &release_readiness["pending_features"],
        "key",
        "live_voice_loop",
    );
    assert_string_array_contains(
        &release_readiness["blocking_manual_gates"],
        "Developer ID Application and Installer signing credentials configured and used for a full signed package run",
    );
    assert_string_array_contains(
        &release_readiness["blocking_manual_gates"],
        "final release evidence bundle generated and archived after signed distribution, live-device QA, and plugin-trust QA reports exist",
    );
    assert!(release_readiness["proof_boundary"]
        .as_str()
        .expect("release readiness proof boundary")
        .contains("does not perform signing"));
}

#[test]
fn model_tools_cli_falls_back_without_running_server() {
    let endpoint = format!("http://{}", unused_loopback_addr());

    let tools = run_cli_json(["tools", "list", "--endpoint", endpoint.as_str()]);

    assert_eq!(tools["source"], "registered_first_party_plugins");
    assert_array_contains(&tools["tools"], "plugin_id", "fake_echo");
    assert_array_contains(&tools["tools"], "plugin_id", "fake_status");
    let encoded_tools = serde_json::to_string(&tools["tools"]).expect("tools JSON");
    assert!(!encoded_tools.contains("source_path"));
    assert!(!encoded_tools.contains("subprocess"));
    assert!(!encoded_tools.contains("provenance"));
}

#[test]
fn release_readiness_cli_uses_explicit_live_voice_evidence() {
    let temp_dir = tempfile::tempdir().expect("temp live QA report");
    let live_report_path = temp_dir.path().join("release-live-device-qa-report.json");
    write_valid_live_device_qa_report(&live_report_path);
    let endpoint = format!("http://{}", unused_loopback_addr());
    let report_path = live_report_path
        .to_str()
        .expect("live report path is UTF-8")
        .to_string();

    let conservative_readiness = run_cli_json_with_env(
        ["release", "readiness", "--endpoint", endpoint.as_str()],
        &[("JARVIS_QA_REPORT_PATH", report_path.as_str())],
    );
    assert_array_contains(
        &conservative_readiness["pending_features"],
        "key",
        "live_voice_loop",
    );

    let evidence_readiness = run_cli_json_with_env(
        ["release", "readiness", "--endpoint", endpoint.as_str()],
        &[
            ("JARVIS_QA_REPORT_PATH", report_path.as_str()),
            ("JARVIS_RELEASE_READINESS_EVIDENCE_MODE", "external"),
        ],
    );
    assert_eq!(evidence_readiness["production_ready"], false);
    assert_array_contains(
        &evidence_readiness["implemented_features"],
        "key",
        "live_voice_loop",
    );
    assert!(!evidence_readiness["pending_features"]
        .as_array()
        .expect("pending features")
        .iter()
        .any(|feature| feature["key"] == "live_voice_loop"));
    assert!(!evidence_readiness["blocking_manual_gates"]
        .as_array()
        .expect("blocking gates")
        .iter()
        .any(|gate| gate
            .as_str()
            .expect("gate string")
            .contains("live microphone")));
}

#[test]
fn release_evidence_status_cli_falls_back_without_running_server() {
    let endpoint = format!("http://{}", unused_loopback_addr());

    let evidence_status = run_cli_json([
        "release",
        "evidence-status",
        "--endpoint",
        endpoint.as_str(),
    ]);

    assert_eq!(evidence_status["complete"], false);
    assert!(evidence_status["items"]
        .as_array()
        .expect("evidence items")
        .iter()
        .any(|item| item["key"] == "release_evidence_bundle"
            && item["kind"] == "json_report"
            && item["manual_gate"] == true));
    assert!(evidence_status["proof_boundary"]
        .as_str()
        .expect("evidence proof boundary")
        .contains("does not sign"));
}

#[test]
fn serve_exposes_local_ipc_contract_and_persists_state() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let db_path = temp_dir.path().join("jarvis-e2e.sqlite");

    unsafe {
        std::env::set_var("JARVIS_SECRET_LEAK_TEST", "server inherited secret");
    }
    let mut server = JarvisServer::start(&db_path);
    unsafe {
        std::env::remove_var("JARVIS_SECRET_LEAK_TEST");
    }
    let endpoint = server.endpoint();

    let health = run_cli_text(["health", "--endpoint", endpoint.as_str()]);
    assert!(health.contains("jarvis-core: ok"), "{health}");
    assert!(health.contains("runtime: routed-fake-local-model+first-party-plugins"));
    assert!(health.contains("paused: false"));
    assert!(health.contains("contract: v1"), "{health}");

    let contract = run_cli_json(["contract", "--endpoint", endpoint.as_str()]);
    assert_eq!(contract["contract"]["name"], "jarvis.local-ipc");
    assert_eq!(contract["contract"]["version"], 1);
    assert_eq!(contract["compatibility"]["minimum_supported_version"], 1);
    assert_eq!(contract["compatibility"]["current_version"], 1);
    assert_eq!(contract["compatibility"]["additive_changes_allowed"], true);
    assert_string_array_contains(
        &contract["compatibility"]["client_requirements"],
        "Clients must ignore unknown JSON fields.",
    );
    assert_eq!(
        contract["compatibility"]["deprecated_endpoints"]
            .as_array()
            .expect("deprecated endpoints array")
            .len(),
        0
    );
    assert_array_contains(
        &contract["features"],
        "key",
        "scheduler_trigger_policy_review",
    );
    assert_array_contains(
        &contract["features"],
        "key",
        "scheduler_stale_running_recovery",
    );
    let stale_recovery = contract["features"]
        .as_array()
        .expect("features array")
        .iter()
        .find(|feature| feature["key"] == "scheduler_stale_running_recovery")
        .expect("scheduler stale recovery feature");
    assert!(
        stale_recovery["proof"]
            .as_str()
            .expect("stale recovery proof")
            .contains("opt-in startup recovery"),
        "{stale_recovery}"
    );
    assert!(
        stale_recovery["boundary"]
            .as_str()
            .expect("stale recovery boundary")
            .contains("no default background recovery"),
        "{stale_recovery}"
    );
    assert_array_contains(&contract["features"], "key", "installed_plugin_execution");
    assert_array_contains(&contract["features"], "key", "release_evidence_bundle");
    assert_array_contains(&contract["features"], "key", "live_voice_loop");
    assert_array_contains(&contract["features"], "status", "pending_manual_validation");
    assert_array_contains(&contract["endpoints"], "path", "/diagnostics/export");
    assert_array_contains(&contract["endpoints"], "path", "/release/readiness");
    assert_array_contains(&contract["endpoints"], "path", "/permissions/grants");
    assert_array_contains(&contract["endpoints"], "path", "/permissions/policy-review");
    assert_array_contains(&contract["endpoints"], "path", "/scheduler/recover-stale");
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
    assert_string_array_contains(&contract["safe_inspection_paths"], "/release/readiness");
    assert_string_array_contains(
        &contract["safe_inspection_paths"],
        "/release/evidence-status",
    );
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
    assert_string_array_contains(&contract["safe_inspection_paths"], "/memory/classification");
    assert_string_array_lacks(&contract["safe_inspection_paths"], "/memory");
    assert_string_array_lacks(&contract["safe_inspection_paths"], "/memory/:id");

    let release_readiness = run_cli_json(["release", "readiness", "--endpoint", endpoint.as_str()]);
    assert_eq!(release_readiness["production_ready"], false);
    assert_array_contains(
        &release_readiness["implemented_features"],
        "key",
        "installed_plugin_execution",
    );
    assert_array_contains(
        &release_readiness["implemented_features"],
        "key",
        "release_evidence_bundle",
    );
    assert_array_contains(
        &release_readiness["pending_features"],
        "key",
        "live_voice_loop",
    );
    assert_string_array_contains(
        &release_readiness["blocking_manual_gates"],
        "Developer ID Application and Installer signing credentials configured and used for a full signed package run",
    );
    assert_string_array_contains(
        &release_readiness["blocking_manual_gates"],
        "final release evidence bundle generated and archived after signed distribution, live-device QA, and plugin-trust QA reports exist",
    );
    assert_string_array_contains(
        &release_readiness["recommended_verification_commands"],
        "./scripts/release-local.sh",
    );
    assert_string_array_contains(
        &release_readiness["recommended_verification_commands"],
        "./scripts/release-operator-qa-smoke.sh",
    );
    assert_string_array_contains(
        &release_readiness["recommended_verification_commands"],
        "./scripts/release-live-device-qa.sh --check",
    );
    assert_string_array_contains_substring(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED=true",
    );
    assert_string_array_contains_substring(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_QA_OWNER_NAME=",
    );
    assert_string_array_contains_substring(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE=",
    );
    assert_string_array_contains(
        &release_readiness["recommended_verification_commands"],
        "./scripts/release-plugin-trust-qa.sh --check",
    );
    assert_string_array_contains_substring(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_PLUGIN_QA_OWNER_NAME=",
    );
    assert_string_array_contains_substring(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_PLUGIN_QA_EGRESS_EVIDENCE_NOTE=",
    );
    assert_string_array_contains(
        &release_readiness["recommended_verification_commands"],
        "./scripts/release-evidence-bundle.sh --check",
    );
    assert_string_array_contains(
        &release_readiness["recommended_verification_commands"],
        "./scripts/release-evidence-doctor.sh --check",
    );
    assert!(release_readiness["proof_boundary"]
        .as_str()
        .expect("release readiness proof boundary")
        .contains("does not perform signing"));

    let evidence_status = run_cli_json([
        "release",
        "evidence-status",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(evidence_status["complete"], false);
    assert!(evidence_status["items"]
        .as_array()
        .expect("evidence items")
        .iter()
        .any(|item| item["key"] == "live_device_qa_report"
            && item["kind"] == "json_report"
            && item["manual_gate"] == true));
    assert!(evidence_status["proof_boundary"]
        .as_str()
        .expect("evidence proof boundary")
        .contains("does not sign"));

    let command = run_cli_json([
        "command",
        "plugin echo cross-process e2e",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(command["accepted"], true, "{command}");
    assert_eq!(command["task"]["status"], "completed", "{command}");
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

    let tools = run_cli_json(["tools", "list", "--endpoint", endpoint.as_str()]);
    assert_eq!(tools["source"], "registered_first_party_plugins");
    assert_array_contains(&tools["tools"], "plugin_id", "fake_echo");
    assert_array_contains(&tools["tools"], "plugin_id", "fake_status");
    assert!(tools["proof_boundary"]
        .as_str()
        .expect("proof boundary")
        .contains("installed plugins"));
    let tools_encoded = serde_json::to_string(&tools["tools"]).expect("tools JSON");
    assert!(!tools_encoded.contains("source_path"));
    assert!(!tools_encoded.contains("subprocess"));
    assert!(!tools_encoded.contains("provenance"));

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
    fs::write(subprocess_plugin_dir.join("fixture-resource.txt"), "v1")
        .expect("write subprocess fixture resource");
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
                        "properties": {
                            "path": { "type": "string" },
                            "secret_seen": { "type": "boolean" },
                            "plugin_id": { "type": "string" },
                            "plugin_action": { "type": "string" }
                        },
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
    assert!(
        subprocess_installed["provenance"]["source_tree_sha256"]
            .as_str()
            .expect("source tree hash")
            .len()
            == 64
    );
    assert_eq!(
        subprocess_installed["provenance"]["source_tree_file_count"],
        3
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
    assert_eq!(subprocess_run["status"], "completed", "{subprocess_run}");
    assert_eq!(subprocess_run["execution_enabled"], true);
    assert_eq!(subprocess_run["execution_grant"], "subprocess_stdio");
    assert_eq!(
        subprocess_run["provenance"]["integrity_status"],
        "matches_install_snapshot"
    );
    assert_eq!(
        subprocess_run["provenance"]["source_tree_sha256"],
        subprocess_installed["provenance"]["source_tree_sha256"]
    );
    assert_eq!(subprocess_run["output"]["path"], "README.md");
    assert_eq!(subprocess_run["output"]["secret_seen"], false);
    assert_eq!(
        subprocess_run["output"]["plugin_id"],
        "local_subprocess_e2e"
    );
    assert_eq!(subprocess_run["output"]["plugin_action"], "inspect");
    assert_eq!(subprocess_run["side_effect_executed"], true);
    assert_eq!(
        subprocess_run["audit_entry"]["event_type"],
        "installed_plugin_subprocess_completed"
    );
    assert_eq!(
        subprocess_run["audit_entry"]["payload"]["sandbox_process_started"],
        true
    );
    assert_eq!(subprocess_run["progress_events"][0]["sequence"], 1);
    assert_eq!(subprocess_run["progress_events"][0]["stage"], "prepare");
    assert_eq!(
        subprocess_run["progress_events"][0]["message"],
        "validated request"
    );
    assert_eq!(subprocess_run["progress_events"][1]["sequence"], 2);
    assert_eq!(subprocess_run["progress_events"][1]["stage"], "complete");
    assert_eq!(
        subprocess_run["audit_entry"]["payload"]["progress_event_count"],
        2
    );
    let subprocess_run_encoded =
        serde_json::to_string(&subprocess_run).expect("subprocess run JSON");
    assert!(!subprocess_run_encoded.contains("raw stderr secret"));
    assert!(!subprocess_run_encoded.contains("ignored"));

    fs::write(subprocess_plugin_dir.join("fixture-resource.txt"), "v2")
        .expect("mutate subprocess fixture resource");
    let changed_subprocess_run = run_cli_json([
        "plugins",
        "run-installed",
        "local_subprocess_e2e",
        "inspect",
        "--input",
        r#"{"path":"README.md"}"#,
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(changed_subprocess_run["status"], "blocked");
    assert_eq!(
        changed_subprocess_run["provenance"]["integrity_status"],
        "changed_since_install"
    );
    assert_eq!(changed_subprocess_run["side_effect_executed"], false);

    let noisy_stdout_plugin_dir = temp_dir.path().join("noisy-stdout-subprocess-plugin");
    fs::create_dir(&noisy_stdout_plugin_dir).expect("create noisy stdout plugin dir");
    let noisy_stdout_plugin_dir = noisy_stdout_plugin_dir
        .canonicalize()
        .expect("canonical noisy stdout plugin dir");
    write_noisy_plugin_script(&noisy_stdout_plugin_dir, "stdout", 1_048_577);
    let noisy_stdout_manifest_path = noisy_stdout_plugin_dir.join("jarvis-plugin.json");
    fs::write(
        &noisy_stdout_manifest_path,
        local_subprocess_manifest_json(
            "noisy_stdout_subprocess_e2e",
            "Noisy Stdout Subprocess E2E Plugin",
            &noisy_stdout_plugin_dir,
        )
        .to_string(),
    )
    .expect("write noisy stdout plugin manifest");
    run_cli_json([
        "plugins",
        "install",
        noisy_stdout_manifest_path
            .to_str()
            .expect("noisy stdout manifest path"),
        "--endpoint",
        endpoint.as_str(),
    ]);
    run_cli_json([
        "plugins",
        "verify-installed",
        "noisy_stdout_subprocess_e2e",
        "--endpoint",
        endpoint.as_str(),
    ]);
    run_cli_json([
        "plugins",
        "enable-installed",
        "noisy_stdout_subprocess_e2e",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let noisy_stdout_run = run_cli_json([
        "plugins",
        "run-installed",
        "noisy_stdout_subprocess_e2e",
        "inspect",
        "--input",
        r#"{"path":"README.md"}"#,
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(noisy_stdout_run["status"], "failed");
    assert!(
        noisy_stdout_run["reason"]
            .as_str()
            .expect("noisy stdout reason")
            .contains("stdout exceeded 1048576 byte limit"),
        "{noisy_stdout_run}"
    );
    assert_eq!(noisy_stdout_run["side_effect_executed"], true);
    assert_eq!(
        noisy_stdout_run["audit_entry"]["event_type"],
        "installed_plugin_subprocess_failed"
    );
    assert_eq!(
        noisy_stdout_run["audit_entry"]["payload"]["sandbox_process_started"],
        true
    );

    let noisy_stderr_plugin_dir = temp_dir.path().join("noisy-stderr-subprocess-plugin");
    fs::create_dir(&noisy_stderr_plugin_dir).expect("create noisy stderr plugin dir");
    let noisy_stderr_plugin_dir = noisy_stderr_plugin_dir
        .canonicalize()
        .expect("canonical noisy stderr plugin dir");
    write_noisy_plugin_script(&noisy_stderr_plugin_dir, "stderr", 262_145);
    let noisy_stderr_manifest_path = noisy_stderr_plugin_dir.join("jarvis-plugin.json");
    fs::write(
        &noisy_stderr_manifest_path,
        local_subprocess_manifest_json(
            "noisy_stderr_subprocess_e2e",
            "Noisy Stderr Subprocess E2E Plugin",
            &noisy_stderr_plugin_dir,
        )
        .to_string(),
    )
    .expect("write noisy stderr plugin manifest");
    run_cli_json([
        "plugins",
        "install",
        noisy_stderr_manifest_path
            .to_str()
            .expect("noisy stderr manifest path"),
        "--endpoint",
        endpoint.as_str(),
    ]);
    run_cli_json([
        "plugins",
        "verify-installed",
        "noisy_stderr_subprocess_e2e",
        "--endpoint",
        endpoint.as_str(),
    ]);
    run_cli_json([
        "plugins",
        "enable-installed",
        "noisy_stderr_subprocess_e2e",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let noisy_stderr_run = run_cli_json([
        "plugins",
        "run-installed",
        "noisy_stderr_subprocess_e2e",
        "inspect",
        "--input",
        r#"{"path":"README.md"}"#,
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(noisy_stderr_run["status"], "failed");
    assert!(
        noisy_stderr_run["reason"]
            .as_str()
            .expect("noisy stderr reason")
            .contains("stderr exceeded 262144 byte limit"),
        "{noisy_stderr_run}"
    );
    assert_eq!(noisy_stderr_run["side_effect_executed"], true);
    assert_eq!(
        noisy_stderr_run["audit_entry"]["event_type"],
        "installed_plugin_subprocess_failed"
    );
    assert_eq!(
        noisy_stderr_run["audit_entry"]["payload"]["sandbox_process_started"],
        true
    );

    let network_subprocess_plugin_dir = temp_dir.path().join("network-subprocess-plugin");
    fs::create_dir(&network_subprocess_plugin_dir).expect("create network subprocess plugin dir");
    let network_subprocess_plugin_dir = network_subprocess_plugin_dir
        .canonicalize()
        .expect("canonical network subprocess plugin dir");
    write_executable_plugin_script(&network_subprocess_plugin_dir);
    let network_subprocess_manifest_path = network_subprocess_plugin_dir.join("jarvis-plugin.json");
    fs::write(
        &network_subprocess_manifest_path,
        json!({
            "manifest_schema_version": 1,
            "id": "network_subprocess_e2e",
            "name": "Network Subprocess E2E Plugin",
            "version": "0.1.0",
            "source": "local_subprocess",
            "author": "Jarvis E2E",
            "source_path": network_subprocess_plugin_dir.display().to_string(),
            "subprocess": {
                "command": "plugin-runner.py",
                "args": [],
                "stdin": "json",
                "stdout": "json"
            },
            "actions": [{
                "name": "inspect",
                "description": "Validate explicit network execution grant.",
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
                "output_schema": {
                    "schema": {
                        "type": "object",
                        "properties": {
                            "path": { "type": "string" },
                            "secret_seen": { "type": "boolean" },
                            "plugin_id": { "type": "string" },
                            "plugin_action": { "type": "string" }
                        },
                        "required": ["path"],
                        "additionalProperties": false
                    }
                },
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
        })
        .to_string(),
    )
    .expect("write network subprocess plugin manifest");

    let network_subprocess_installed = run_cli_json([
        "plugins",
        "install",
        network_subprocess_manifest_path
            .to_str()
            .expect("network manifest path"),
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(network_subprocess_installed["id"], "network_subprocess_e2e");
    assert_eq!(network_subprocess_installed["execution_enabled"], false);
    assert_eq!(
        network_subprocess_installed["execution_grant"],
        "metadata_only"
    );

    let network_subprocess_verified = run_cli_json([
        "plugins",
        "verify-installed",
        "network_subprocess_e2e",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(
        network_subprocess_verified["provenance"]["integrity_status"],
        "matches_install_snapshot"
    );

    let network_subprocess_default_enable = run_cli_failure([
        "plugins",
        "enable-installed",
        "network_subprocess_e2e",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert!(
        network_subprocess_default_enable.contains("requires subprocess_stdio_network grant"),
        "{network_subprocess_default_enable}"
    );

    let network_subprocess_enabled = run_cli_json([
        "plugins",
        "enable-installed",
        "network_subprocess_e2e",
        "--grant",
        "subprocess_stdio_network",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(network_subprocess_enabled["execution_enabled"], true);
    assert_eq!(
        network_subprocess_enabled["execution_grant"],
        "subprocess_stdio_network"
    );

    let network_subprocess_run = run_cli_json([
        "plugins",
        "run-installed",
        "network_subprocess_e2e",
        "inspect",
        "--input",
        r#"{"path":"README.md"}"#,
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(network_subprocess_run["status"], "completed");
    assert_eq!(
        network_subprocess_run["execution_grant"],
        "subprocess_stdio_network"
    );
    assert_eq!(network_subprocess_run["output"]["path"], "README.md");
    assert_eq!(network_subprocess_run["output"]["secret_seen"], false);
    assert_eq!(
        network_subprocess_run["output"]["plugin_id"],
        "network_subprocess_e2e"
    );
    assert_eq!(network_subprocess_run["output"]["plugin_action"], "inspect");
    assert_eq!(
        network_subprocess_run["audit_entry"]["payload"]["action_requires_network_grant"],
        true
    );
    assert_eq!(network_subprocess_run["side_effect_executed"], true);

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
    assert_array_contains(&all_audit, "event_type", "installed_plugin_progress");
    let all_audit_encoded = serde_json::to_string(&all_audit).expect("all audit JSON");
    assert!(!all_audit_encoded.contains("raw stderr secret"));
    assert!(!all_audit_encoded.contains("ignored"));

    let progress_activity_events = run_cli_text([
        "activity",
        "watch",
        "--max-events",
        "1",
        "--interval-ms",
        "100",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert!(
        progress_activity_events.contains("event: activity_progress"),
        "{progress_activity_events}"
    );
    assert!(
        progress_activity_events.contains("\"stage\":\"complete\""),
        "{progress_activity_events}"
    );
    assert!(
        !progress_activity_events.contains("raw stderr secret"),
        "{progress_activity_events}"
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

    let executed_approval = run_cli_json([
        "approvals",
        "execute",
        approval_id.as_str(),
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(executed_approval["accepted"], true);
    assert_eq!(executed_approval["task"]["status"], "completed");
    assert_eq!(
        executed_approval["audit_entry"]["event_type"],
        "approval_executed"
    );
    assert_eq!(
        executed_approval["plugin_results"][0]["status"],
        "completed"
    );
    assert_eq!(
        executed_approval["plugin_results"][0]["output"]["message"],
        "needs user approval"
    );
    assert_array_contains(
        &executed_approval["audit_entries"],
        "event_type",
        "plugin_completed_after_approval",
    );
    let executed_approval_audit = run_cli_json([
        "tasks",
        "audit",
        "--task-id",
        approval_task_id.as_str(),
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_array_contains(&executed_approval_audit, "event_type", "approval_executed");
    let executed_audit_encoded =
        serde_json::to_string(&executed_approval_audit).expect("executed approval audit JSON");
    assert!(
        executed_audit_encoded.contains("\"side_effect_executed\":true"),
        "{executed_audit_encoded}"
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

    let sensitive_memory = run_cli_json([
        "memory",
        "create",
        "retention",
        "deleted-secret",
        "do not expose deleted sensitive memory",
        "--provenance",
        "local_ipc_e2e retention review",
        "--sensitivity",
        "private",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let sensitive_memory_id = sensitive_memory["id"]
        .as_str()
        .expect("sensitive memory id")
        .to_string();
    let deleted_sensitive_memory = run_cli_json([
        "memory",
        "delete",
        sensitive_memory_id.as_str(),
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert!(deleted_sensitive_memory["deleted_at"].is_string());
    let retention_policy_review =
        run_cli_json(["permissions", "review", "--endpoint", endpoint.as_str()]);
    assert_array_contains(
        &retention_policy_review["items"],
        "item_type",
        "memory_retention_review",
    );
    assert_array_contains(
        &retention_policy_review["items"],
        "action",
        "retention/deleted-secret",
    );
    let retention_policy_review_encoded =
        serde_json::to_string(&retention_policy_review).expect("retention policy review JSON");
    assert!(!retention_policy_review_encoded.contains("do not expose deleted sensitive memory"));

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
    assert!(run_due["executions"]
        .as_array()
        .expect("executions array")
        .iter()
        .any(|execution| execution["audit_entries"]
            .as_array()
            .expect("audit entries")
            .iter()
            .any(
                |entry| entry["event_type"] == "scheduler_proactive_policy_checked"
                    && entry["payload"]["command_redacted"] == true
                    && entry["payload"]["policy_review_item_type"] == "scheduled_scheduler_trigger"
            )));
    assert!(run_due["executions"]
        .as_array()
        .expect("executions array")
        .iter()
        .any(|execution| execution["audit_entries"]
            .as_array()
            .expect("audit entries")
            .iter()
            .any(
                |entry| entry["event_type"] == "scheduler_proactive_policy_checked"
                    && entry["payload"]["command_redacted"] == true
                    && entry["payload"]["policy_review_item_type"] == "recurring_scheduler_trigger"
            )));
    let run_due_audit_entries = run_due["executions"]
        .as_array()
        .expect("executions array")
        .iter()
        .flat_map(|execution| {
            execution["audit_entries"]
                .as_array()
                .expect("audit entries")
                .iter()
        })
        .collect::<Vec<_>>();
    assert!(!serde_json::to_string(&run_due_audit_entries)
        .expect("run due audit JSON")
        .contains("\"plugin status\""));
    let proactive_task_audit = run_cli_json(["tasks", "audit", "--endpoint", endpoint.as_str()]);
    assert!(proactive_task_audit
        .as_array()
        .expect("task audit entries")
        .iter()
        .any(|entry| entry["event_type"] == "plugin_completed"
            && entry["payload"]["proactive"] == true));

    let diagnostics = run_cli_json(["diagnostics", "export", "--endpoint", endpoint.as_str()]);
    assert_eq!(diagnostics["repository_backed"], true);
    assert_eq!(diagnostics["schema_version"], 9);
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
        "fail closed non proactive e2e job",
        "plugin echo scheduler should not run",
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
    assert!(fail_closed_run["executions"][0]["message"]
        .as_str()
        .expect("fail closed message")
        .contains("fake_echo.echo cannot run proactively"));
    let fail_closed_audit = run_cli_json(["tasks", "audit", "--endpoint", endpoint.as_str()]);
    assert!(fail_closed_audit
        .as_array()
        .expect("fail closed audit entries")
        .iter()
        .any(|entry| entry["event_type"] == "plugin_execution_blocked"
            && entry["payload"]["proactive"] == true
            && entry["payload"]["side_effect_executed"] == false
            && entry["payload"]["error"]
                .as_str()
                .expect("blocked error")
                .contains("fake_echo.echo cannot run proactively")));
    assert!(!serde_json::to_string(&fail_closed_audit)
        .expect("fail closed audit JSON")
        .contains("scheduler should not run"));
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

    let stale_running = run_cli_json([
        "scheduler",
        "schedule",
        "stale running e2e job",
        "plugin status",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let stale_running_id = stale_running["id"]
        .as_str()
        .expect("stale running id")
        .to_string();

    let pause_status = run_cli_json(["pause-status", "--endpoint", endpoint.as_str()]);
    assert_eq!(pause_status["paused"], false);

    server.stop();
    {
        let repository = SqliteRepository::open(&db_path).expect("open repository");
        let job = repository
            .list_scheduler_jobs()
            .expect("scheduler jobs")
            .into_iter()
            .find(|job| job.id.to_string() == stale_running_id)
            .expect("stale running job");
        repository
            .mark_scheduler_job_running(job.id)
            .expect("mark stale running");
    }
    std::thread::sleep(Duration::from_millis(1100));

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

    let recovered_stale = run_cli_json([
        "scheduler",
        "recover-stale",
        "--older-than-seconds",
        "1",
        "--limit",
        "4",
        "--endpoint",
        restarted_endpoint.as_str(),
    ]);
    assert_eq!(
        recovered_stale["recovered"][0]["job"]["id"],
        stale_running_id
    );
    assert_eq!(recovered_stale["recovered"][0]["job"]["status"], "failed");
    assert_eq!(
        recovered_stale["recovered"][0]["audit_entry"]["event_type"],
        "scheduler_stale_running_recovered"
    );
    assert_eq!(
        recovered_stale["recovered"][0]["audit_entry"]["payload"]["command_redacted"],
        true
    );
    assert!(!serde_json::to_string(&recovered_stale)
        .expect("stale recovery JSON")
        .contains("\"plugin status\""));

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
    assert_array_contains(
        &persisted_audit,
        "event_type",
        "scheduler_stale_running_recovered",
    );

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
        "plugin_id",
        "noisy_stdout_subprocess_e2e",
    );
    assert_array_contains(
        &persisted_grants["installed_plugin_grants"],
        "plugin_id",
        "noisy_stderr_subprocess_e2e",
    );
    assert_array_contains(
        &persisted_grants["installed_plugin_grants"],
        "plugin_id",
        "network_subprocess_e2e",
    );
    assert_array_contains(
        &persisted_grants["installed_plugin_grants"],
        "execution_grant",
        "subprocess_stdio_network",
    );
    assert_array_contains(
        &persisted_grants["installed_plugin_grants"],
        "integrity_status",
        "matches_install_snapshot",
    );
    assert_array_contains(
        &persisted_grants["installed_plugin_grants"],
        "integrity_status",
        "changed_since_install",
    );
    assert_eq!(persisted_grants["executable_installed_plugin_count"], 4);
    assert_eq!(persisted_grants["unverified_installed_plugin_count"], 1);
    assert_eq!(persisted_grants["side_effects_require_approval"], true);

    let persisted_policy_review = run_cli_json([
        "permissions",
        "review",
        "--endpoint",
        restarted_endpoint.as_str(),
    ]);
    assert_eq!(
        persisted_policy_review["executable_installed_plugin_count"],
        4
    );
    assert_eq!(
        persisted_policy_review["unverified_installed_plugin_count"],
        1
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
    assert_array_contains(
        &persisted_installed_plugins,
        "id",
        "noisy_stdout_subprocess_e2e",
    );
    assert_array_contains(
        &persisted_installed_plugins,
        "id",
        "noisy_stderr_subprocess_e2e",
    );
    assert_array_contains(&persisted_installed_plugins, "id", "network_subprocess_e2e");

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
fn serve_can_recover_stale_scheduler_jobs_on_startup() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let db_path = temp_dir
        .path()
        .join("jarvis-scheduler-startup-recovery.sqlite");

    let mut server = JarvisServer::start(&db_path);
    let endpoint = server.endpoint();
    let stale_running = run_cli_json([
        "scheduler",
        "schedule",
        "stale startup recovery job",
        "plugin status",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let stale_running_id = stale_running["id"]
        .as_str()
        .expect("stale running id")
        .to_string();
    server.stop();

    {
        let repository = SqliteRepository::open(&db_path).expect("open repository");
        let job = repository
            .list_scheduler_jobs()
            .expect("scheduler jobs")
            .into_iter()
            .find(|job| job.id.to_string() == stale_running_id)
            .expect("stale running job");
        repository
            .mark_scheduler_job_running(job.id)
            .expect("mark stale running");
    }
    thread::sleep(Duration::from_millis(1100));

    let mut restarted = JarvisServer::start_with_startup_recovery(&db_path, 1, 4);
    let restarted_endpoint = restarted.endpoint();
    let health = run_cli_text(["health", "--endpoint", restarted_endpoint.as_str()]);
    assert!(health.contains("jarvis-core: ok"), "{health}");

    let jobs = run_cli_json([
        "scheduler",
        "list",
        "--endpoint",
        restarted_endpoint.as_str(),
    ]);
    let recovered_job = jobs
        .as_array()
        .expect("jobs array")
        .iter()
        .find(|job| job["id"] == stale_running_id)
        .expect("recovered job");
    assert_eq!(recovered_job["status"], "failed");

    let audit = run_cli_json(["tasks", "audit", "--endpoint", restarted_endpoint.as_str()]);
    let recovery_entry = audit
        .as_array()
        .expect("audit array")
        .iter()
        .find(|entry| entry["event_type"] == "scheduler_stale_running_recovered")
        .expect("startup recovery audit");
    assert_eq!(recovery_entry["payload"]["automatic_recovery"], true);
    assert_eq!(recovery_entry["payload"]["command_redacted"], true);
    assert!(!serde_json::to_string(&audit)
        .expect("audit JSON")
        .contains("plugin status"));

    restarted.stop();
}

#[test]
fn serve_executes_ollama_provider_tool_request_envelope() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let db_path = temp_dir.path().join("jarvis-provider-tool-e2e.sqlite");
    let (ollama_base_url, server_thread) = start_ollama_envelope_server();
    let mut server = JarvisServer::start_with_env(
        &db_path,
        &[
            ("JARVIS_LOCAL_MODEL_PROVIDER", "ollama"),
            ("JARVIS_LOCAL_MODEL", "provider-envelope-test"),
            ("JARVIS_OLLAMA_BASE_URL", ollama_base_url.as_str()),
        ],
    );
    let endpoint = server.endpoint();

    let command = run_cli_json([
        "command",
        "ask provider to inspect status",
        "--endpoint",
        endpoint.as_str(),
    ]);

    assert_eq!(command["accepted"], true, "{command}");
    assert_eq!(command["task"]["status"], "completed", "{command}");
    assert_eq!(command["route"]["model"], "provider-envelope-test");
    assert_eq!(
        command["steps"][0]["tool_results"][0]["plugin_id"],
        "fake_status"
    );
    assert_eq!(
        command["steps"][0]["tool_results"][0]["status"],
        "completed"
    );
    assert_eq!(command["message"], "provider saw tool result");
    assert_array_contains(
        &command["audit_entries"],
        "event_type",
        "tool_plan_received",
    );
    assert_array_contains(&command["audit_entries"], "event_type", "tool_policy_check");
    assert_array_contains(
        &command["audit_entries"],
        "event_type",
        "tool_execution_result",
    );
    let encoded = serde_json::to_string(&command).expect("command JSON");
    assert!(!encoded.contains("JARVIS_OLLAMA_BASE_URL"));

    server.stop();
    server_thread.join().expect("ollama stub thread");
    drop(temp_dir);
}

#[test]
fn serve_rejects_ollama_hallucinated_tool_with_registered_tool_guidance() {
    assert_ollama_hallucinated_tool_is_rejected("status", "plugin status is not registered");
}

#[test]
fn serve_rejects_ollama_chrome_extension_hallucination_with_registered_tool_guidance() {
    assert_ollama_hallucinated_tool_is_rejected(
        "chrome_extension",
        "plugin chrome_extension is not registered",
    );
}

fn assert_ollama_hallucinated_tool_is_rejected(plugin_id: &str, expected_error: &str) {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let db_path = temp_dir
        .path()
        .join("jarvis-provider-invalid-tool-e2e.sqlite");
    let (ollama_base_url, server_thread) = start_ollama_invalid_tool_server(plugin_id);
    let mut server = JarvisServer::start_with_env(
        &db_path,
        &[
            ("JARVIS_LOCAL_MODEL_PROVIDER", "ollama"),
            ("JARVIS_LOCAL_MODEL", "provider-invalid-tool-test"),
            ("JARVIS_OLLAMA_BASE_URL", ollama_base_url.as_str()),
        ],
    );
    let endpoint = server.endpoint();

    let command = run_cli_json([
        "command",
        "ask provider to inspect status with a bad tool",
        "--endpoint",
        endpoint.as_str(),
    ]);

    assert_eq!(command["accepted"], true, "{command}");
    assert_eq!(command["task"]["status"], "completed", "{command}");
    assert!(command["plugin_results"]
        .as_array()
        .expect("plugin results")
        .is_empty());
    assert_eq!(
        command["message"],
        "provider recovered after tool rejection"
    );
    assert_eq!(command["steps"][0]["tool_results"][0]["status"], "rejected");
    assert!(command["steps"][0]["tool_results"][0]["output"]["error"]
        .as_str()
        .expect("rejection error")
        .contains(expected_error));
    assert!(command["steps"][0]["tool_results"][0]["output"]["guidance"]
        .as_str()
        .expect("rejection guidance")
        .contains("Registered first-party model tools are: fake_echo.approval_echo, fake_echo.echo, fake_status.status"));
    assert_array_contains(
        &command["audit_entries"],
        "event_type",
        "tool_request_rejected",
    );
    let rejection = command["audit_entries"]
        .as_array()
        .expect("audit entries")
        .iter()
        .find(|entry| entry["event_type"] == "tool_request_rejected")
        .expect("rejection audit");
    assert_eq!(rejection["payload"]["plugin_id"], plugin_id);
    assert!(rejection["payload"]["registered_tools"]
        .as_array()
        .expect("registered tools")
        .iter()
        .any(|tool| tool == "fake_status.status"));
    let encoded = serde_json::to_string(&command).expect("command JSON");
    assert!(!encoded.contains("JARVIS_OLLAMA_BASE_URL"));

    server.stop();
    server_thread
        .join()
        .expect("ollama invalid-tool stub thread");
    drop(temp_dir);
}

#[test]
fn serve_rejects_ollama_mixed_prose_tool_json_as_malformed_model_output() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let db_path = temp_dir
        .path()
        .join("jarvis-provider-mixed-tool-json-e2e.sqlite");
    let (ollama_base_url, server_thread) = start_ollama_mixed_tool_json_server();
    let mut server = JarvisServer::start_with_env(
        &db_path,
        &[
            ("JARVIS_LOCAL_MODEL_PROVIDER", "ollama"),
            ("JARVIS_LOCAL_MODEL", "provider-mixed-tool-json-test"),
            ("JARVIS_OLLAMA_BASE_URL", ollama_base_url.as_str()),
        ],
    );
    let endpoint = server.endpoint();

    let command = run_cli_json([
        "command",
        "check status without leaking tool JSON",
        "--endpoint",
        endpoint.as_str(),
    ]);

    assert_eq!(command["accepted"], false, "{command}");
    assert_eq!(command["task"]["status"], "failed", "{command}");
    assert!(command["plugin_results"]
        .as_array()
        .expect("plugin results")
        .is_empty());
    let message = command["message"].as_str().expect("message");
    assert!(message.contains("Model execution failed during step 0"));
    assert_array_contains(&command["audit_entries"], "event_type", "model_step_failed");
    let failed = command["audit_entries"]
        .as_array()
        .expect("audit entries")
        .iter()
        .find(|entry| entry["event_type"] == "model_step_failed")
        .expect("model failure audit");
    assert!(failed["payload"]["error"]
        .as_str()
        .expect("model error")
        .contains("mixed prose and tool_requests"));
    let encoded = serde_json::to_string(&command).expect("command JSON");
    assert!(!encoded.contains("fake_status"));
    assert!(!encoded.contains("JARVIS_OLLAMA_BASE_URL"));

    server.stop();
    server_thread
        .join()
        .expect("ollama mixed-tool-json stub thread");
    drop(temp_dir);
}

#[test]
fn serve_executes_chatgpt_native_tool_call() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let db_path = temp_dir
        .path()
        .join("jarvis-chatgpt-native-tool-e2e.sqlite");
    let (chatgpt_base_url, server_thread) = start_chatgpt_native_tool_server();
    let mut server = JarvisServer::start_with_env(
        &db_path,
        &[
            ("JARVIS_LOCAL_MODEL_ENABLED", "false"),
            ("JARVIS_CHATGPT_ENABLED", "true"),
            ("JARVIS_CHATGPT_MODEL", "gpt-native-tool-test"),
            ("JARVIS_OPENAI_API_KEY", "test-openai-token"),
            ("JARVIS_OPENAI_BASE_URL", chatgpt_base_url.as_str()),
        ],
    );
    let endpoint = server.endpoint();

    let command = run_cli_json([
        "command",
        "ask chatgpt provider for status",
        "--sensitivity",
        "workspace",
        "--endpoint",
        endpoint.as_str(),
    ]);

    assert_eq!(command["accepted"], true, "{command}");
    assert_eq!(command["task"]["status"], "completed", "{command}");
    assert_eq!(command["route"]["provider"], "chat_gpt");
    assert_eq!(
        command["steps"][0]["tool_results"][0]["plugin_id"],
        "fake_status"
    );
    assert_eq!(
        command["steps"][0]["tool_results"][0]["status"],
        "completed"
    );
    assert_eq!(command["message"], "native tool result observed");
    assert_array_contains(
        &command["audit_entries"],
        "event_type",
        "tool_plan_received",
    );
    assert_array_contains(
        &command["audit_entries"],
        "event_type",
        "tool_execution_result",
    );
    let encoded = serde_json::to_string(&command).expect("command JSON");
    assert!(!encoded.contains("test-openai-token"));
    assert!(!encoded.contains("JARVIS_OPENAI_API_KEY"));

    server.stop();
    server_thread.join().expect("chatgpt stub thread");
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
        Self::start_inner(db_path, None, None)
    }

    fn start_with_background(db_path: &Path, interval_ms: u64, limit: usize) -> Self {
        Self::start_inner(db_path, Some((interval_ms, limit)), None)
    }

    fn start_with_startup_recovery(db_path: &Path, older_than_seconds: u64, limit: usize) -> Self {
        Self::start_inner(db_path, None, Some((older_than_seconds, limit)))
    }

    fn start_with_env(db_path: &Path, env: &[(&str, &str)]) -> Self {
        Self::start_inner_with_env(db_path, None, None, env)
    }

    fn start_inner(
        db_path: &Path,
        scheduler_background: Option<(u64, usize)>,
        scheduler_startup_recovery: Option<(u64, usize)>,
    ) -> Self {
        Self::start_inner_with_env(
            db_path,
            scheduler_background,
            scheduler_startup_recovery,
            &[],
        )
    }

    fn start_inner_with_env(
        db_path: &Path,
        scheduler_background: Option<(u64, usize)>,
        scheduler_startup_recovery: Option<(u64, usize)>,
        env: &[(&str, &str)],
    ) -> Self {
        let _startup_guard = jarvis_server_startup_lock()
            .lock()
            .expect("lock jarvis server startup");
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
        if let Some((older_than_seconds, limit)) = scheduler_startup_recovery {
            args.extend([
                "--scheduler-recover-stale-on-startup".to_string(),
                "--scheduler-stale-older-than-seconds".to_string(),
                older_than_seconds.to_string(),
                "--scheduler-stale-recovery-limit".to_string(),
                limit.to_string(),
            ]);
        }
        let mut command = Command::new(BIN);
        command
            .args(args)
            .current_dir(temp_dir.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        for (key, value) in env {
            command.env(key, value);
        }
        let child = command.spawn().expect("start jarvis serve");

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

fn jarvis_server_startup_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

impl Drop for JarvisServer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn start_ollama_envelope_server() -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ollama stub");
    let address = listener.local_addr().expect("ollama stub address");
    let handle = thread::spawn(move || {
        let envelope = json!({
            "message": "provider requested status",
            "complete": false,
            "tool_requests": [
                {
                    "plugin_id": "fake_status",
                    "action": "status",
                    "input": {}
                }
            ]
        })
        .to_string();
        let responses = [
            json!({ "response": envelope, "done": false }).to_string(),
            json!({ "response": "provider saw tool result", "done": true }).to_string(),
        ];

        for response in responses {
            let (mut stream, _) = listener.accept().expect("ollama request");
            let mut buffer = [0_u8; 8192];
            let read = stream.read(&mut buffer).expect("read request");
            let request = String::from_utf8_lossy(&buffer[..read]);
            assert!(request.contains("POST /api/generate"), "{request}");
            assert!(
                request.contains("Registered first-party tools are exactly this JSON allowlist")
            );
            assert!(
                request.contains("\\\"plugin_id\\\":\\\"fake_echo\\\""),
                "{request}"
            );
            assert!(
                request.contains("\\\"action\\\":\\\"approval_echo\\\""),
                "{request}"
            );
            assert!(request.contains("\\\"action\\\":\\\"echo\\\""), "{request}");
            assert!(
                request.contains("\\\"plugin_id\\\":\\\"fake_status\\\""),
                "{request}"
            );
            assert!(
                request.contains("\\\"action\\\":\\\"status\\\""),
                "{request}"
            );
            assert!(request.contains("action names, command aliases, endpoints, and capability names are invalid plugin ids"));
            assert!(!request.contains("chrome_extension"), "{request}");
            let http = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                response.len(),
                response
            );
            stream.write_all(http.as_bytes()).expect("write response");
        }
    });

    (format!("http://{address}"), handle)
}

fn start_ollama_invalid_tool_server(plugin_id: &str) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ollama invalid-tool stub");
    let address = listener
        .local_addr()
        .expect("ollama invalid-tool stub address");
    let plugin_id = plugin_id.to_string();
    let handle = thread::spawn(move || {
        let envelope = json!({
            "message": "provider guessed a status tool",
            "complete": false,
            "tool_requests": [
                {
                    "plugin_id": plugin_id.as_str(),
                    "action": "status",
                    "input": {}
                }
            ]
        })
        .to_string();
        let responses = [
            json!({ "response": envelope, "done": false }).to_string(),
            json!({ "response": "provider recovered after tool rejection", "done": true })
                .to_string(),
        ];

        for (index, response) in responses.into_iter().enumerate() {
            let (mut stream, _) = listener.accept().expect("ollama request");
            let mut buffer = [0_u8; 8192];
            let read = stream.read(&mut buffer).expect("read request");
            let request = String::from_utf8_lossy(&buffer[..read]);
            assert!(request.contains("POST /api/generate"), "{request}");
            assert!(
                request.contains("Registered first-party tools are exactly this JSON allowlist")
            );
            assert!(request.contains("\\\"plugin_id\\\":\\\"fake_status\\\""));
            assert!(request.contains("\\\"action\\\":\\\"status\\\""));
            assert!(request.contains("Never invent plugin_id or action values"));
            assert!(request.contains("action names, command aliases, endpoints, and capability names are invalid plugin ids"));
            if index == 1 {
                assert!(request.contains("rejected"), "{request}");
                assert!(
                    request.contains(&format!("plugin {plugin_id} is not registered")),
                    "{request}"
                );
            }
            let http = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                response.len(),
                response
            );
            stream.write_all(http.as_bytes()).expect("write response");
        }
    });

    (format!("http://{address}"), handle)
}

fn start_ollama_mixed_tool_json_server() -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ollama mixed-tool-json stub");
    let address = listener
        .local_addr()
        .expect("ollama mixed-tool-json stub address");
    let handle = thread::spawn(move || {
        let response = json!({
            "response": "I can check that.\n{\"tool_requests\":[{\"plugin_id\":\"fake_status\",\"action\":\"status\",\"input\":{}}]}",
            "done": true
        })
        .to_string();

        let (mut stream, _) = listener.accept().expect("ollama request");
        let mut buffer = [0_u8; 8192];
        let read = stream.read(&mut buffer).expect("read request");
        let request = String::from_utf8_lossy(&buffer[..read]);
        assert!(request.contains("POST /api/generate"), "{request}");
        assert!(request.contains("one strict JSON object with no surrounding prose"));
        let http = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            response.len(),
            response
        );
        stream.write_all(http.as_bytes()).expect("write response");
    });

    (format!("http://{address}"), handle)
}

fn start_chatgpt_native_tool_server() -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind chatgpt stub");
    let address = listener.local_addr().expect("chatgpt stub address");
    let handle = thread::spawn(move || {
        let responses = [
            json!({
                "choices": [
                    {
                        "message": {
                            "content": null,
                            "tool_calls": [
                                {
                                    "id": "call_1",
                                    "type": "function",
                                    "function": {
                                        "name": "fake_status__status",
                                        "arguments": "{}"
                                    }
                                }
                            ]
                        }
                    }
                ]
            })
            .to_string(),
            json!({
                "choices": [
                    { "message": { "content": "native tool result observed" } }
                ]
            })
            .to_string(),
        ];

        for (index, response) in responses.into_iter().enumerate() {
            let (mut stream, _) = listener.accept().expect("chatgpt request");
            let mut buffer = [0_u8; 8192];
            let read = stream.read(&mut buffer).expect("read request");
            let request = String::from_utf8_lossy(&buffer[..read]);
            let request_lower = request.to_ascii_lowercase();
            assert!(request.contains("POST /chat/completions"), "{request}");
            assert!(
                request_lower.contains("authorization: bearer test-openai-token"),
                "{request}"
            );
            if index == 0 {
                assert!(request.contains("\"tools\""), "{request}");
                assert!(request.contains("fake_echo__approval_echo"), "{request}");
                assert!(request.contains("fake_echo__echo"), "{request}");
                assert!(request.contains("fake_status__status"), "{request}");
                assert!(request.contains("\"tool_choice\":\"auto\""), "{request}");
                assert!(!request.contains("chrome_extension"), "{request}");
            } else {
                assert!(request.contains("fake_status"), "{request}");
            }
            let http = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                response.len(),
                response
            );
            stream.write_all(http.as_bytes()).expect("write response");
        }
    });

    (format!("http://{address}"), handle)
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

fn run_cli_json_with_env<const N: usize>(args: [&str; N], env: &[(&str, &str)]) -> Value {
    let output = run_cli_with_env(args, env);
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
    run_cli_with_env(args, &[])
}

fn run_cli_with_env<const N: usize>(args: [&str; N], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(BIN);
    command.args(args).stdin(Stdio::null());
    for (key, value) in env {
        command.env(key, value);
    }
    let output = command.output().expect("run jarvis cli");

    assert!(
        output.status.success(),
        "jarvis cli failed: {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn write_valid_live_device_qa_report(path: &Path) {
    let report = json!({
        "schema_version": 1,
        "evidence_type": "owner_recorded_live_device_qa",
        "self_test_fixture": false,
        "generated_at": "2026-05-22T16:06:00Z",
        "installed_app_path": "/Applications/Jarvis.app",
        "app_bundle": {
            "bundle_identifier": "com.nobiletechnology.jarvis",
            "short_version": "0.1.4",
            "build_version": "0.1.4",
            "microphone_usage_description": "Jarvis uses microphone input only when you explicitly start local voice capture.",
            "speech_recognition_usage_description": "Jarvis uses speech recognition only to turn your spoken command into a local assistant request."
        },
        "validation_flags": {
            "clean_profile": true,
            "finder_launch": true,
            "microphone": true,
            "speech_permission": true,
            "transcript_handoff": true,
            "audio_output": true,
            "notification": true,
            "restart": true,
            "manual_release_qa": true
        },
        "voice_loop": {
            "microphone_permission_prompt": true,
            "speech_permission_prompt": true,
            "spoken_transcript_handoff": true,
            "same_command_path": true,
            "speech_output_playback": true
        },
        "owner_recorded_live_voice_evidence": {
            "owner_name": "Release Operator",
            "device_label": "Clean-profile release Mac",
            "profile_label": "Clean macOS QA profile",
            "voice_check_started_at": "2026-05-22T16:00:00Z",
            "voice_check_completed_at": "2026-05-22T16:05:00Z",
            "microphone_evidence_note": "Microphone prompt and capture observed.",
            "speech_permission_evidence_note": "Speech prompt and recognition observed.",
            "transcript_handoff_evidence_note": "Spoken transcript reached the command path.",
            "audio_output_evidence_note": "Speech output playback observed."
        },
        "voice_command_observation": {
            "test_phrase": "Jarvis status check.",
            "observed_transcript": "Jarvis status check.",
            "observed_command_text": "status check",
            "command_result_evidence_id": "task:release-voice-fixture",
            "audio_output_device_label": "Built-in speakers"
        },
        "proof_boundary": "Owner-recorded live device QA fixture for CLI E2E."
    });
    fs::write(
        path,
        serde_json::to_string_pretty(&report).expect("serialize live QA report"),
    )
    .expect("write live QA report");
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
import os
import sys

request = json.load(sys.stdin)
print('{"jarvis_progress":true,"stage":"prepare","message":"validated request"}', file=sys.stderr)
print('raw stderr secret should stay redacted', file=sys.stderr)
print('{"jarvis_progress":true,"stage":"complete","message":"writing validated output","payload":{"ignored":"not exposed"}}', file=sys.stderr)
json.dump({
    "path": request["input"]["path"],
    "secret_seen": "JARVIS_SECRET_LEAK_TEST" in os.environ,
    "plugin_id": os.environ.get("JARVIS_PLUGIN_ID"),
    "plugin_action": os.environ.get("JARVIS_PLUGIN_ACTION")
}, sys.stdout)
"#,
    )
    .expect("write plugin runner");
    let mut permissions = fs::metadata(&script)
        .expect("script metadata")
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&script, permissions).expect("chmod plugin runner");
}

#[cfg(unix)]
fn write_noisy_plugin_script(dir: &Path, stream: &str, byte_count: usize) {
    use std::os::unix::fs::PermissionsExt;

    let script = dir.join("plugin-runner.py");
    let output_statement = match stream {
        "stdout" => format!("sys.stdout.write('x' * {byte_count})"),
        "stderr" => format!(
            "sys.stderr.write('x' * {byte_count})\njson.dump({{\"path\": request[\"input\"][\"path\"]}}, sys.stdout)"
        ),
        other => panic!("unsupported noisy stream: {other}"),
    };
    fs::write(
        &script,
        format!(
            r#"#!/usr/bin/env python3
import json
import sys

request = json.load(sys.stdin)
{output_statement}
sys.stdout.flush()
sys.stderr.flush()
"#
        ),
    )
    .expect("write noisy plugin runner");
    let mut permissions = fs::metadata(&script)
        .expect("noisy script metadata")
        .permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&script, permissions).expect("chmod noisy plugin runner");
}

fn local_subprocess_manifest_json(id: &str, name: &str, source_path: &Path) -> Value {
    json!({
        "manifest_schema_version": 1,
        "id": id,
        "name": name,
        "version": "0.1.0",
        "source": "local_subprocess",
        "author": "Jarvis E2E",
        "source_path": source_path.display().to_string(),
        "subprocess": {
            "command": "plugin-runner.py",
            "args": [],
            "stdin": "json",
            "stdout": "json"
        },
        "actions": [{
            "name": "inspect",
            "description": "Validate bounded subprocess output failure behavior.",
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
                    "properties": {
                        "path": { "type": "string" }
                    },
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

fn assert_string_array_contains_substring(value: &Value, expected: &str) {
    let array = value.as_array().unwrap_or_else(|| {
        panic!("expected array, got {}", json!(value));
    });
    assert!(
        array
            .iter()
            .filter_map(Value::as_str)
            .any(|item| item.contains(expected)),
        "expected array to contain item with {expected}, got {value}"
    );
}

fn assert_string_array_lacks(value: &Value, unexpected: &str) {
    let array = value.as_array().unwrap_or_else(|| {
        panic!("expected array, got {}", json!(value));
    });
    assert!(
        !array.iter().any(|item| item.as_str() == Some(unexpected)),
        "expected array not to contain {unexpected}, got {value}"
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
