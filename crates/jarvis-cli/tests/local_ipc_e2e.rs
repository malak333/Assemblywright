use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use ed25519_dalek::{Signer, SigningKey};
use jarvis_core::SqliteRepository;
use p256::ecdsa::{Signature as P256Signature, SigningKey as P256SigningKey};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tempfile::TempDir;

fn jarvis_cli_bin() -> PathBuf {
    static STABLE_BIN: OnceLock<PathBuf> = OnceLock::new();
    STABLE_BIN
        .get_or_init(|| {
            let source = resolve_jarvis_cli_bin();
            let destination = std::env::current_exe()
                .expect("current E2E executable path")
                .parent()
                .expect("current E2E executable directory")
                .join(format!("jarvis-e2e-bin-{}", std::process::id()));
            fs::copy(&source, &destination).unwrap_or_else(|error| {
                panic!(
                    "copy stable Jarvis CLI E2E binary from {} to {}: {error}",
                    source.display(),
                    destination.display()
                )
            });
            destination
        })
        .clone()
}

fn resolve_jarvis_cli_bin() -> PathBuf {
    let cargo_bin = PathBuf::from(env!("CARGO_BIN_EXE_jarvis"));
    if cargo_bin.is_file() {
        return cargo_bin;
    }

    let workspace_debug_bin = workspace_root().join("target/debug/jarvis");
    if workspace_debug_bin.is_file() {
        return workspace_debug_bin;
    }

    let current_exe_debug_bin = std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.parent()
                .and_then(|deps| deps.parent())
                .map(Path::to_path_buf)
        })
        .map(|debug_dir| debug_dir.join("jarvis"));
    if let Some(path) = current_exe_debug_bin {
        if path.is_file() {
            return path;
        }
    }

    cargo_bin
}

#[test]
fn release_readiness_cli_falls_back_without_running_server() {
    let endpoint = format!("http://{}", unused_loopback_addr());

    let release_readiness = run_cli_json(["release", "readiness", "--endpoint", endpoint.as_str()]);
    let format_release_readiness: Value = serde_json::from_str(&run_cli_text([
        "release",
        "readiness",
        "--format",
        "json",
        "--endpoint",
        endpoint.as_str(),
    ]))
    .expect("release readiness --format json output");
    let readable_readiness =
        run_cli_text(["release", "readiness", "--endpoint", endpoint.as_str()]);
    let readable_full_runbook = run_cli_text([
        "release",
        "readiness",
        "--endpoint",
        endpoint.as_str(),
        "--all-commands",
    ]);

    assert_eq!(release_readiness["production_ready"], false);
    assert_eq!(format_release_readiness["production_ready"], false);
    assert_eq!(release_readiness["evidence_mode_enabled"], false);
    assert_eq!(format_release_readiness["evidence_mode_enabled"], false);
    assert_eq!(
        format_release_readiness["pending_feature_count"],
        release_readiness["pending_feature_count"]
    );
    assert_array_contains(
        &release_readiness["implemented_features"],
        "key",
        "installed_plugin_execution",
    );
    let activity_events_feature = release_readiness["implemented_features"]
        .as_array()
        .expect("implemented features")
        .iter()
        .find(|feature| feature["key"] == "activity_events")
        .expect("activity events feature");
    assert!(
        activity_events_feature["proof"]
            .as_str()
            .expect("activity events proof")
            .contains("model-output chunk metadata frames"),
        "{activity_events_feature}"
    );
    assert!(
        activity_events_feature["boundary"]
            .as_str()
            .expect("activity events boundary")
            .contains("content_redacted:true"),
        "{activity_events_feature}"
    );
    let installed_plugin_feature = release_readiness["implemented_features"]
        .as_array()
        .expect("implemented features")
        .iter()
        .find(|feature| feature["key"] == "installed_plugin_execution")
        .expect("installed plugin execution feature");
    assert!(
        installed_plugin_feature["proof"]
            .as_str()
            .expect("installed plugin proof")
            .contains("os_sandbox_enforced:false"),
        "{installed_plugin_feature}"
    );
    assert!(
        installed_plugin_feature["boundary"]
            .as_str()
            .expect("installed plugin boundary")
            .contains("os_sandbox_enforced:false"),
        "{installed_plugin_feature}"
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
        "release_ci_gate",
    );
    assert_array_contains(
        &release_readiness["implemented_features"],
        "key",
        "release_evidence_status",
    );
    assert_array_contains(
        &release_readiness["implemented_features"],
        "key",
        "release_evidence_bundle",
    );
    let evidence_status_feature = readiness_feature(&release_readiness, "release_evidence_status");
    let evidence_status_proof = evidence_status_feature["proof"]
        .as_str()
        .expect("evidence status proof");
    assert!(evidence_status_proof.contains("/release/evidence-status"));
    assert!(evidence_status_proof.contains("repository-backed command-result evidence"));
    assert!(evidence_status_proof.contains("host-egress policy"));
    assert!(evidence_status_proof.contains("archive-URI validation"));
    assert!(evidence_status_proof.contains("child-report semantic revalidation"));
    assert!(evidence_status_feature["boundary"]
        .as_str()
        .expect("evidence status boundary")
        .contains("file/report inventory"));
    let evidence_bundle_feature = readiness_feature(&release_readiness, "release_evidence_bundle");
    let evidence_bundle_proof = evidence_bundle_feature["proof"]
        .as_str()
        .expect("evidence bundle proof");
    assert!(evidence_bundle_proof.contains("SHA-256-bound evidence manifest"));
    assert!(evidence_bundle_proof.contains("host-egress fields"));
    assert!(evidence_bundle_proof.contains("durable reports archive URI"));
    assert!(evidence_bundle_proof.contains("child reports are revalidated"));
    assert!(evidence_bundle_feature["boundary"]
        .as_str()
        .expect("evidence bundle boundary")
        .contains("owner-recorded external"));
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
    assert!(readable_readiness.contains("Jarvis release readiness:"));
    assert!(readable_readiness.contains("Production ready: false"));
    assert!(readable_readiness.contains("External evidence mode: false"));
    assert!(readable_readiness.contains("Pending features:"));
    assert!(readable_readiness.contains("live_voice_loop"));
    assert!(readable_readiness.contains("Top manual gates:"));
    assert!(readable_readiness.contains("Next verification commands:"));
    assert!(readable_readiness.contains("Showing 4 of"));
    assert!(readable_readiness.contains("--all-commands for the full readable runbook"));
    assert!(readable_readiness.contains("Raw JSON: rerun with --json"));
    assert!(readable_full_runbook.contains("Recommended verification commands:"));
    assert!(readable_full_runbook.contains("./scripts/release-ci-workflow-smoke.sh"));
    assert!(!readable_full_runbook.contains("./scripts/packaged-app-release-smoke.sh"));
    assert!(readable_full_runbook.contains("./scripts/package-distribution.sh --check"));
    assert!(
        readable_full_runbook.contains("./scripts/package-distribution.sh --unsigned-launch-check")
    );
    assert!(
        readable_full_runbook.contains("cargo run -p jarvis-cli -- release live-device-runbook")
    );
    assert!(readable_full_runbook
        .contains("cargo run -p jarvis-cli -- release signed-distribution-runbook"));
    assert!(
        readable_full_runbook.contains("cargo run -p jarvis-cli -- release plugin-trust-runbook")
    );
    assert!(readable_full_runbook.contains(
        "./scripts/release-evidence-bundle.sh --write-template target/release-evidence-bundle.env"
    ));
    assert!(readable_full_runbook.contains(
        "set -a && source target/release-plugin-trust-qa.env && set +a && ./scripts/release-plugin-trust-qa.sh --assert-complete"
    ));
    assert!(readable_full_runbook.contains(
        "set -a && source target/release-evidence-bundle.env && set +a && ./scripts/release-evidence-bundle.sh --bundle"
    ));
    assert!(readable_full_runbook.contains("JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true"));
    assert!(!readable_full_runbook.contains("Showing 4 of"));
    assert_string_array_order(
        &release_readiness["recommended_verification_commands"],
        "./scripts/release-ci-workflow-smoke.sh",
        "./scripts/release-operator-qa-smoke.sh",
    );
    assert_string_array_order(
        &release_readiness["recommended_verification_commands"],
        "./scripts/release-operator-qa-smoke.sh",
        "./scripts/package-distribution.sh --check",
    );
    assert_string_array_order(
        &release_readiness["recommended_verification_commands"],
        "./scripts/package-distribution.sh --check",
        "./scripts/package-distribution.sh --unsigned-launch-check",
    );
    assert_string_array_order(
        &release_readiness["recommended_verification_commands"],
        "./scripts/package-distribution.sh --unsigned-launch-check",
        "cargo run -p jarvis-cli -- release signed-distribution-runbook",
    );
    assert_string_array_order(
        &release_readiness["recommended_verification_commands"],
        "cargo run -p jarvis-cli -- release signed-distribution-runbook",
        "JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' JARVIS_NOTARYTOOL_PROFILE='...' ./scripts/package-distribution.sh",
    );
    assert_string_array_order(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' JARVIS_NOTARYTOOL_PROFILE='...' ./scripts/package-distribution.sh",
        "JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' JARVIS_NOTARYTOOL_APPLE_ID='apple-id@example.com' JARVIS_NOTARYTOOL_TEAM_ID='TEAMID1234' JARVIS_NOTARYTOOL_PASSWORD='app-specific-password' ./scripts/package-distribution.sh",
    );
    assert_string_array_order(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' JARVIS_NOTARYTOOL_APPLE_ID='apple-id@example.com' JARVIS_NOTARYTOOL_TEAM_ID='TEAMID1234' JARVIS_NOTARYTOOL_PASSWORD='app-specific-password' ./scripts/package-distribution.sh",
        "./scripts/release-external-handoff.sh --write target/release-external-handoff",
    );
    assert_string_array_order(
        &release_readiness["recommended_verification_commands"],
        "./scripts/release-external-handoff.sh --write target/release-external-handoff",
        "cargo run -p jarvis-cli -- release live-device-runbook",
    );
    assert_string_array_order(
        &release_readiness["recommended_verification_commands"],
        "cargo run -p jarvis-cli -- release live-device-runbook",
        "./scripts/release-live-device-qa.sh --check",
    );
    assert_string_array_order(
        &release_readiness["recommended_verification_commands"],
        "./scripts/release-live-device-qa.sh --check",
        "./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env",
    );
    assert_string_array_order(
        &release_readiness["recommended_verification_commands"],
        "./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env",
        "cargo run -p jarvis-cli -- release plugin-trust-runbook",
    );
    assert_string_array_order(
        &release_readiness["recommended_verification_commands"],
        "cargo run -p jarvis-cli -- release plugin-trust-runbook",
        "./scripts/release-plugin-trust-qa.sh --check",
    );
    assert_string_array_order(
        &release_readiness["recommended_verification_commands"],
        "./scripts/release-plugin-trust-qa.sh --check",
        "./scripts/release-plugin-trust-qa.sh --write-template target/release-plugin-trust-qa.env",
    );
    assert_string_array_order(
        &release_readiness["recommended_verification_commands"],
        "set -a && source target/release-plugin-trust-qa.env && set +a && ./scripts/release-plugin-trust-qa.sh --assert-complete",
        "set -a && source target/release-evidence-bundle.env && set +a && ./scripts/release-evidence-bundle.sh --bundle",
    );
    assert_string_array_substring_order(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED=true",
        "./scripts/release-evidence-doctor.sh --assert-complete",
    );
    assert_string_array_order(
        &release_readiness["recommended_verification_commands"],
        "./scripts/release-evidence-doctor.sh --assert-complete",
        "Start or restart the core with JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external",
    );
    assert_string_array_order(
        &release_readiness["recommended_verification_commands"],
        "Start or restart the core with JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external",
        "cargo run -p jarvis-cli -- release readiness",
    );
    assert!(
        serde_json::from_str::<Value>(&readable_readiness).is_err(),
        "default release readiness output should be operator-readable text"
    );
}

#[test]
fn release_help_surfaces_current_evidence_boundaries() {
    let release_help = run_cli_text(["release", "--help"]);
    let readiness_help = run_cli_text(["release", "readiness", "--help"]);
    let evidence_status_help = run_cli_text(["release", "evidence-status", "--help"]);

    assert!(release_help.contains("Read-only release operator commands."));
    assert!(release_help.contains("fall back to conservative local metadata"));
    assert!(readiness_help.contains("JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external"));
    assert!(readiness_help.contains("production_ready field stays false"));

    assert!(evidence_status_help.contains("Print release evidence file/report status."));
    assert!(evidence_status_help.contains("semantic report validation"));
    assert!(evidence_status_help.contains("owner-asserted plugin-trust review source"));
    assert!(evidence_status_help.contains("host-egress evidence fields"));
    assert!(evidence_status_help.contains("final-bundle archive URI validation"));
    assert!(evidence_status_help.contains("final-bundle local signature-validation status"));
    assert!(evidence_status_help.contains("Default output is operator-readable"));
    assert!(evidence_status_help.contains("use --json for the exact structured payload"));
    assert!(!evidence_status_help.contains("status as JSON."));
}

#[test]
fn health_cli_reports_server_unavailable_with_operator_guidance() {
    let endpoint = format!("http://{}", unused_loopback_addr());

    let output = run_cli_failure(["health", "--endpoint", endpoint.as_str()]);

    assert_server_required_guidance(&output);
}

#[test]
fn server_required_cli_inspection_reports_unavailable_with_operator_guidance() {
    let endpoint = format!("http://{}", unused_loopback_addr());

    for args in [
        ["diagnostics", "export", "--endpoint", endpoint.as_str()],
        ["plugins", "installed", "--endpoint", endpoint.as_str()],
        ["permissions", "grants", "--endpoint", endpoint.as_str()],
        ["permissions", "review", "--endpoint", endpoint.as_str()],
    ] {
        let output = run_cli_failure(args);
        assert_server_required_guidance(&output);
    }
}

#[test]
fn strict_ipc_cli_commands_report_unavailable_with_operator_guidance() {
    let endpoint = format!("http://{}", unused_loopback_addr());
    let commands: Vec<Vec<&str>> = vec![
        vec!["command", "status check", "--endpoint", endpoint.as_str()],
        vec!["pause-status", "--endpoint", endpoint.as_str()],
        vec![
            "pause",
            "--reason",
            "manual test",
            "--endpoint",
            endpoint.as_str(),
        ],
        vec!["resume", "--endpoint", endpoint.as_str()],
        vec!["scheduler", "list", "--endpoint", endpoint.as_str()],
        vec!["scheduler", "attention", "--endpoint", endpoint.as_str()],
        vec!["tasks", "list", "--endpoint", endpoint.as_str()],
        vec!["activity", "summary", "--endpoint", endpoint.as_str()],
        vec!["routes", "list", "--endpoint", endpoint.as_str()],
        vec!["memory", "classification", "--endpoint", endpoint.as_str()],
        vec!["approvals", "list", "--endpoint", endpoint.as_str()],
    ];

    for args in commands {
        let output = run_cli_failure_args(&args);
        assert_server_required_guidance(&output);
    }
}

fn assert_server_required_guidance(output: &str) {
    assert!(output.contains("jarvis-core is unavailable"), "{output}");
    assert!(
        output.contains("requires a running repository-backed core"),
        "{output}"
    );
    assert!(
        output.contains("cargo run -p jarvis-cli -- serve"),
        "{output}"
    );
    assert!(
        output.contains("cargo run -p jarvis-cli -- smoke"),
        "{output}"
    );
    assert!(output.contains("jarvis release readiness"), "{output}");
    assert!(output.contains("jarvis plugins list"), "{output}");
    assert!(output.contains("jarvis tools list"), "{output}");
}

#[test]
fn model_tools_cli_falls_back_without_running_server() {
    let endpoint = format!("http://{}", unused_loopback_addr());

    let tools = run_cli_json(["tools", "list", "--endpoint", endpoint.as_str()]);
    let tools_model_alias = run_cli_json(["tools", "model", "--endpoint", endpoint.as_str()]);
    let tools_catalog_alias = run_cli_json(["tools", "catalog", "--endpoint", endpoint.as_str()]);

    assert_eq!(tools["source"], "registered_first_party_plugins");
    assert_eq!(tools_model_alias["source"], tools["source"]);
    assert_eq!(tools_catalog_alias["source"], tools["source"]);
    assert_eq!(tools_model_alias["tools"], tools["tools"]);
    assert_eq!(tools_catalog_alias["tools"], tools["tools"]);
    assert_array_contains(&tools["tools"], "plugin_id", "fake_echo");
    assert_array_contains(&tools["tools"], "plugin_id", "fake_status");
    let encoded_tools = serde_json::to_string(&tools["tools"]).expect("tools JSON");
    assert!(!encoded_tools.contains("source_path"));
    assert!(!encoded_tools.contains("subprocess"));
    assert!(!encoded_tools.contains("provenance"));
}

#[test]
fn contract_and_first_party_plugins_fall_back_without_running_server() {
    let endpoint = format!("http://{}", unused_loopback_addr());

    let contract = run_cli_json(["contract", "--endpoint", endpoint.as_str()]);
    let explicit_json_contract =
        run_cli_json(["contract", "--json", "--endpoint", endpoint.as_str()]);
    assert_eq!(explicit_json_contract, contract);
    assert_eq!(contract["contract"]["name"], "jarvis.local-ipc");
    assert_eq!(contract["contract"]["version"], 1);
    assert_array_contains(&contract["endpoints"], "path", "/tools/model");
    assert_array_contains(&contract["features"], "key", "model_tool_catalog_grounding");

    let manifests = run_cli_json(["plugins", "list", "--json", "--endpoint", endpoint.as_str()]);
    let manifests_available_alias = run_cli_json([
        "plugins",
        "available",
        "--json",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_array_contains(&manifests_available_alias, "id", "fake_echo");
    assert_array_contains(&manifests_available_alias, "id", "fake_status");
    assert_array_contains(&manifests, "id", "fake_echo");
    assert_array_contains(&manifests, "id", "fake_status");

    let status_manifest = run_cli_json([
        "plugins",
        "get",
        "fake_status",
        "--json",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(status_manifest["id"], "fake_status");
    assert_array_contains(&status_manifest["actions"], "name", "status");
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
        &evidence_readiness["pending_features"],
        "key",
        "live_voice_loop",
    );
    assert!(!evidence_readiness["implemented_features"]
        .as_array()
        .expect("implemented features")
        .iter()
        .any(|feature| feature["key"] == "live_voice_loop"));
    assert!(evidence_readiness["blocking_manual_gates"]
        .as_array()
        .expect("blocking gates")
        .iter()
        .any(|gate| gate
            .as_str()
            .expect("gate string")
            .contains("live microphone")));
}

#[test]
#[cfg(unix)]
fn release_readiness_cli_computes_production_ready_only_from_external_complete_evidence_status() {
    let temp_dir = tempfile::tempdir().expect("temp complete release evidence");
    let fixture = write_complete_release_evidence_fixture(temp_dir.path());
    let fallback_endpoint = format!("http://{}", unused_loopback_addr());
    let evidence_env = fixture.env_refs();
    let fallback_evidence_status = run_cli_json_with_env(
        [
            "release",
            "evidence-status",
            "--endpoint",
            fallback_endpoint.as_str(),
        ],
        &evidence_env,
    );
    assert_eq!(fallback_evidence_status["complete"], false);
    let fallback_live_item =
        release_evidence_item(&fallback_evidence_status, "live_device_qa_report");
    assert_eq!(
        fallback_live_item["status"], "invalid",
        "{fallback_live_item}"
    );
    assert!(
        fallback_live_item["detail"]
            .as_str()
            .expect("fallback live detail")
            .contains("requires repository-backed IPC evidence-status"),
        "{fallback_live_item}"
    );

    let db_path = temp_dir.path().join("jarvis-complete-evidence.sqlite");
    let mut server = JarvisServer::start_with_env(&db_path, &evidence_env);
    let endpoint = server.endpoint();
    let command = run_cli_json([
        "command",
        "status check",
        "--json",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let task_id = command["task"]["id"].as_str().expect("task id");
    bind_complete_release_evidence_fixture_to_task(&fixture, task_id);

    let evidence_status = run_cli_json([
        "release",
        "evidence-status",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let format_evidence_status_output = run_cli([
        "release",
        "evidence-status",
        "--format",
        "json",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let format_evidence_status: Value =
        serde_json::from_slice(&format_evidence_status_output.stdout)
            .expect("server-backed evidence-status --format json output");
    assert_eq!(evidence_status["complete"], true);
    assert_eq!(format_evidence_status["complete"], true);
    assert_eq!(evidence_status["missing_count"], 0);
    assert_eq!(evidence_status["invalid_count"], 0);
    assert_all_evidence_items_present(&evidence_status);

    let conservative_readiness =
        run_cli_json(["release", "readiness", "--endpoint", endpoint.as_str()]);
    assert_eq!(conservative_readiness["production_ready"], false);
    assert_eq!(conservative_readiness["evidence_mode_enabled"], false);
    let cli_only_external_env_readiness = run_cli_json_with_env(
        ["release", "readiness", "--endpoint", endpoint.as_str()],
        &[("JARVIS_RELEASE_READINESS_EVIDENCE_MODE", "external")],
    );
    assert_eq!(cli_only_external_env_readiness["production_ready"], false);
    assert_eq!(
        cli_only_external_env_readiness["evidence_mode_enabled"],
        false
    );
    server.stop();

    let mut external_env = evidence_env.clone();
    external_env.push(("JARVIS_RELEASE_READINESS_EVIDENCE_MODE", "external"));
    let mut external_server = JarvisServer::start_with_env(&db_path, &external_env);
    let external_endpoint = external_server.endpoint();

    let external_readiness = run_cli_json_with_env(
        [
            "release",
            "readiness",
            "--endpoint",
            external_endpoint.as_str(),
        ],
        &[],
    );
    let format_external_readiness_output = run_cli_with_env(
        [
            "release",
            "readiness",
            "--format",
            "json",
            "--endpoint",
            external_endpoint.as_str(),
        ],
        &[("JARVIS_RELEASE_READINESS_EVIDENCE_MODE", "external")],
    );
    let format_external_readiness: Value =
        serde_json::from_slice(&format_external_readiness_output.stdout)
            .expect("server-backed readiness --format json output");
    assert_eq!(external_readiness["production_ready"], true);
    assert_eq!(format_external_readiness["production_ready"], true);
    assert_eq!(external_readiness["evidence_mode_enabled"], true);
    assert_eq!(format_external_readiness["evidence_mode_enabled"], true);
    assert_eq!(external_readiness["pending_feature_count"], 0);
    assert_eq!(format_external_readiness["pending_feature_count"], 0);
    assert!(external_readiness["pending_features"]
        .as_array()
        .expect("pending features")
        .is_empty());
    assert!(external_readiness["blocking_manual_gates"]
        .as_array()
        .expect("blocking gates")
        .is_empty());
    assert!(external_readiness["proof_boundary"]
        .as_str()
        .expect("external proof boundary")
        .contains("does not perform signing"));
    assert_array_contains(
        &external_readiness["implemented_features"],
        "key",
        "live_voice_loop",
    );
    let live_voice_feature = readiness_feature(&external_readiness, "live_voice_loop");
    assert_eq!(live_voice_feature["status"], "implemented");
    assert!(live_voice_feature["boundary"]
        .as_str()
        .expect("live voice external boundary")
        .contains("Owner-recorded live-device QA evidence"));
    let evidence_bundle_feature = readiness_feature(&external_readiness, "release_evidence_bundle");
    assert_eq!(evidence_bundle_feature["status"], "implemented");
    assert!(evidence_bundle_feature["proof"]
        .as_str()
        .expect("evidence bundle proof")
        .contains("SHA-256-bound evidence manifest"));
    assert!(evidence_bundle_feature["proof"]
        .as_str()
        .expect("evidence bundle proof")
        .contains("child reports are revalidated"));
    assert!(evidence_bundle_feature["boundary"]
        .as_str()
        .expect("evidence bundle boundary")
        .contains("owner-recorded external signing"));
    let signed_bundle = release_evidence_item(&evidence_status, "signed_app_bundle");
    assert_eq!(signed_bundle["manual_gate"], true);
    assert!(signed_bundle["detail"]
        .as_str()
        .expect("signed app detail")
        .contains("not validated by evidence-status"));
    let live_report = release_evidence_item(&evidence_status, "live_device_qa_report");
    assert_eq!(live_report["manual_gate"], true);
    assert!(live_report["detail"]
        .as_str()
        .expect("live report detail")
        .contains("owner-recorded"));
    external_server.stop();
}

#[test]
fn release_readiness_external_mode_with_live_voice_evidence_but_incomplete_release_evidence_stays_not_production_ready(
) {
    let temp_dir = tempfile::tempdir().expect("temp live-only release evidence");
    let db_path = temp_dir.path().join("jarvis-live-only-evidence.sqlite");
    let live_report_path = temp_dir.path().join("release-live-device-qa-report.json");
    let report_path = live_report_path
        .to_str()
        .expect("live report path is UTF-8")
        .to_string();
    let env = [
        ("JARVIS_QA_REPORT_PATH", report_path.as_str()),
        ("JARVIS_RELEASE_READINESS_EVIDENCE_MODE", "external"),
    ];
    let mut server = JarvisServer::start_with_env(&db_path, &env);
    let endpoint = server.endpoint();

    let command = run_cli_json([
        "command",
        "status check",
        "--json",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let task_id = command["task"]["id"].as_str().expect("task id");
    let mut live_report = valid_live_device_qa_report();
    live_report["voice_command_observation"]["command_result_evidence_id"] =
        json!(format!("task:{task_id}"));
    write_live_device_qa_report(&live_report_path, live_report);

    let evidence_status = run_cli_json([
        "release",
        "evidence-status",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(evidence_status["complete"], false);
    assert_release_evidence_item_status(
        &evidence_status,
        "live_device_qa_report",
        "present",
        "valid live-device report should be accepted through repository-backed IPC",
    );
    assert!(
        evidence_status["missing_count"]
            .as_i64()
            .expect("missing count")
            > 0
    );

    let readiness = run_cli_json(["release", "readiness", "--endpoint", endpoint.as_str()]);
    assert_eq!(readiness["production_ready"], false);
    assert!(readiness["pending_features"]
        .as_array()
        .expect("pending features")
        .is_empty());
    assert_array_contains(&readiness["implemented_features"], "key", "live_voice_loop");
    assert!(readiness["blocking_manual_gates"]
        .as_array()
        .expect("blocking gates")
        .iter()
        .any(|gate| gate.as_str().expect("gate").contains("Developer ID")));
    assert!(readiness["blocking_manual_gates"]
        .as_array()
        .expect("blocking gates")
        .iter()
        .any(|gate| gate
            .as_str()
            .expect("gate")
            .contains("final release evidence bundle")));

    server.stop();
}

#[test]
#[cfg(unix)]
fn release_live_device_qa_script_generated_report_clears_evidence_status() {
    let temp_dir = tempfile::tempdir().expect("temp live-device script evidence");
    let app_path = write_live_device_qa_app_fixture(temp_dir.path());
    let live_report_path = temp_dir.path().join("release-live-device-qa-report.json");
    let db_path = temp_dir
        .path()
        .join("jarvis-live-device-script-evidence.sqlite");
    let app_path_arg = app_path.to_str().expect("app path UTF-8").to_string();
    let report_path_arg = live_report_path
        .to_str()
        .expect("live report path is UTF-8")
        .to_string();
    let expected_version = env!("CARGO_PKG_VERSION");
    let server_env = [
        ("JARVIS_QA_INSTALLED_APP_PATH", app_path_arg.as_str()),
        ("JARVIS_QA_REPORT_PATH", report_path_arg.as_str()),
        (
            "JARVIS_QA_EXPECTED_BUNDLE_ID",
            "com.nobiletechnology.jarvis.selftest",
        ),
        ("JARVIS_QA_EXPECTED_VERSION", expected_version),
        ("JARVIS_RELEASE_READINESS_EVIDENCE_MODE", "external"),
    ];
    let mut server = JarvisServer::start_with_env(&db_path, &server_env);
    let endpoint = server.endpoint();

    let command = run_cli_json([
        "command",
        "status check",
        "--json",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(command["accepted"], true, "{command}");
    let task_id = command["task"]["id"].as_str().expect("task id");
    let task_evidence_id = format!("task:{task_id}");

    let script_output = run_repo_script_with_env(
        "scripts/release-live-device-qa.sh",
        &["--assert-complete"],
        &[
            ("JARVIS_QA_INSTALLED_APP_PATH", app_path_arg.as_str()),
            ("JARVIS_QA_REPORT_PATH", report_path_arg.as_str()),
            (
                "JARVIS_QA_EXPECTED_BUNDLE_ID",
                "com.nobiletechnology.jarvis.selftest",
            ),
            ("JARVIS_QA_EXPECTED_VERSION", expected_version),
            ("JARVIS_QA_CLEAN_PROFILE_VALIDATED", "true"),
            ("JARVIS_QA_FINDER_LAUNCH_VALIDATED", "true"),
            ("JARVIS_QA_MICROPHONE_VALIDATED", "true"),
            ("JARVIS_QA_SPEECH_PERMISSION_VALIDATED", "true"),
            ("JARVIS_QA_TRANSCRIPT_HANDOFF_VALIDATED", "true"),
            ("JARVIS_QA_AUDIO_OUTPUT_VALIDATED", "true"),
            ("JARVIS_QA_NOTIFICATION_VALIDATED", "true"),
            ("JARVIS_QA_RESTART_VALIDATED", "true"),
            ("JARVIS_QA_MANUAL_RELEASE_QA_VALIDATED", "true"),
            ("JARVIS_QA_OWNER_NAME", "Release Operator"),
            ("JARVIS_QA_DEVICE_LABEL", "script E2E Mac fixture"),
            ("JARVIS_QA_PROFILE_LABEL", "script E2E clean profile"),
            ("JARVIS_QA_VOICE_CHECK_STARTED_AT", "2026-05-22T16:00:00Z"),
            (
                "JARVIS_QA_VOICE_CHECK_COMPLETED_AT",
                "2026-05-22T16:05:00Z",
            ),
            (
                "JARVIS_QA_CLEAN_PROFILE_EVIDENCE_NOTE",
                "Clean profile install observed in the controlled release QA lane.",
            ),
            (
                "JARVIS_QA_FINDER_LAUNCH_EVIDENCE_NOTE",
                "Finder launch observed in the controlled release QA lane.",
            ),
            (
                "JARVIS_QA_MICROPHONE_EVIDENCE_NOTE",
                "Observed microphone permission prompt in the controlled release QA lane.",
            ),
            (
                "JARVIS_QA_SPEECH_PERMISSION_EVIDENCE_NOTE",
                "Observed Speech permission prompt in the controlled release QA lane.",
            ),
            (
                "JARVIS_QA_TRANSCRIPT_HANDOFF_EVIDENCE_NOTE",
                "Observed transcript handoff reach the command path in the controlled release QA lane.",
            ),
            (
                "JARVIS_QA_AUDIO_OUTPUT_EVIDENCE_NOTE",
                "Observed speech output playback in the controlled release QA lane.",
            ),
            (
                "JARVIS_QA_NOTIFICATION_EVIDENCE_NOTE",
                "Visible scheduler notification observed in the controlled release QA lane.",
            ),
            (
                "JARVIS_QA_NOTIFICATION_OBSERVED_AT",
                "2026-05-22T16:04:00Z",
            ),
            ("JARVIS_QA_NOTIFICATION_KIND", "due_now"),
            (
                "JARVIS_QA_NOTIFICATION_TITLE",
                "Scheduler job ready: release verification",
            ),
            (
                "JARVIS_QA_NOTIFICATION_BODY",
                "A scheduled Jarvis job is due now.",
            ),
            (
                "JARVIS_QA_NOTIFICATION_THREAD_IDENTIFIER",
                "jarvis.scheduler",
            ),
            (
                "JARVIS_QA_RESTART_EVIDENCE_NOTE",
                "Restart recovery observed in the controlled release QA lane.",
            ),
            (
                "JARVIS_QA_MANUAL_RELEASE_QA_EVIDENCE_NOTE",
                "Manual release QA surfaces observed in the controlled release QA lane.",
            ),
            ("JARVIS_QA_VOICE_TEST_PHRASE", "Jarvis status check."),
            ("JARVIS_QA_OBSERVED_TRANSCRIPT", "Jarvis status check."),
            ("JARVIS_QA_EXPECTED_COMMAND_TEXT", "status check"),
            ("JARVIS_QA_OBSERVED_COMMAND_TEXT", "status check"),
            (
                "JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID",
                task_evidence_id.as_str(),
            ),
            ("JARVIS_QA_AUDIO_OUTPUT_DEVICE_LABEL", "Built-in speakers"),
        ],
    );
    let script_stdout = String::from_utf8_lossy(&script_output.stdout);
    assert!(
        script_stdout.contains("Jarvis live-device QA assertion: complete"),
        "{script_stdout}"
    );
    assert!(
        script_stdout.contains(report_path_arg.as_str()),
        "{script_stdout}"
    );

    let report = read_json_file(&live_report_path);
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["evidence_type"], "owner_recorded_live_device_qa");
    assert_eq!(report["self_test_fixture"], false);
    assert_eq!(report["installed_app_path"], app_path_arg);
    assert_eq!(
        report["app_bundle"]["bundle_identifier"],
        "com.nobiletechnology.jarvis.selftest"
    );
    assert_eq!(
        report["voice_command_observation"]["command_result_evidence_id"],
        task_evidence_id
    );
    assert_eq!(
        report["notification_observation"]["thread_identifier"],
        "jarvis.scheduler"
    );
    assert_eq!(
        report["voice_command_observation"]["audio_output_device_label"],
        "Built-in speakers"
    );

    let evidence_status = run_cli_json([
        "release",
        "evidence-status",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(evidence_status["complete"], false, "{evidence_status}");
    assert_release_evidence_item_status(
        &evidence_status,
        "live_device_qa_report",
        "present",
        "script-generated live-device report should resolve against repository evidence",
    );

    let readiness = run_cli_json(["release", "readiness", "--endpoint", endpoint.as_str()]);
    assert_eq!(readiness["production_ready"], false);
    assert!(readiness["pending_features"]
        .as_array()
        .expect("pending features")
        .is_empty());
    assert_array_contains(&readiness["implemented_features"], "key", "live_voice_loop");
    let live_voice_feature = readiness_feature(&readiness, "live_voice_loop");
    assert_eq!(live_voice_feature["status"], "implemented");
    assert!(live_voice_feature["boundary"]
        .as_str()
        .expect("live voice boundary")
        .contains("Owner-recorded live-device QA evidence"));

    server.stop();
}

#[test]
#[cfg(unix)]
fn release_readiness_rejects_invalid_live_voice_evidence_even_when_other_evidence_is_complete() {
    let temp_dir = tempfile::tempdir().expect("temp complete release evidence");
    let fixture = write_complete_release_evidence_fixture(temp_dir.path());
    let endpoint = format!("http://{}", unused_loopback_addr());

    let mut live_report = valid_live_device_qa_report();
    live_report["validation_flags"]["transcript_handoff"] = json!(false);
    write_json_report(Path::new(&fixture.live_report_path), live_report);

    let evidence_env = fixture.env_refs();
    let evidence_status = run_cli_json_with_env(
        [
            "release",
            "evidence-status",
            "--endpoint",
            endpoint.as_str(),
        ],
        &evidence_env,
    );
    let live_item = release_evidence_item(&evidence_status, "live_device_qa_report");
    assert_eq!(live_item["status"], "invalid", "{live_item}");
    assert!(live_item["detail"]
        .as_str()
        .expect("live detail")
        .contains("validation_flags.transcript_handoff"));

    let external_readiness = run_cli_json_with_env(
        ["release", "readiness", "--endpoint", endpoint.as_str()],
        &fixture.env_refs_with_external_mode(),
    );
    assert_eq!(external_readiness["production_ready"], false);
    assert_array_contains(
        &external_readiness["pending_features"],
        "key",
        "live_voice_loop",
    );
    assert!(!external_readiness["implemented_features"]
        .as_array()
        .expect("implemented features")
        .iter()
        .any(|feature| feature["key"] == "live_voice_loop"));
    assert!(external_readiness["blocking_manual_gates"]
        .as_array()
        .expect("blocking gates")
        .iter()
        .any(|gate| gate
            .as_str()
            .expect("gate string")
            .contains("live microphone")));
}

#[test]
#[cfg(unix)]
fn release_evidence_doctor_assert_complete_matches_cli_evidence_status_fixture() {
    let temp_dir = tempfile::tempdir().expect("temp complete release evidence");
    let fixture = write_complete_release_evidence_fixture(temp_dir.path());
    let evidence_env = fixture.env_refs();
    let db_path = temp_dir.path().join("jarvis-doctor-parity.sqlite");
    let mut server = JarvisServer::start_with_env(&db_path, &evidence_env);
    let endpoint = server.endpoint();
    let command = run_cli_json([
        "command",
        "status check",
        "--json",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let task_id = command["task"]["id"].as_str().expect("task id");
    bind_complete_release_evidence_fixture_to_task(&fixture, task_id);

    let evidence_status = run_cli_json([
        "release",
        "evidence-status",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(evidence_status["complete"], true);
    assert_eq!(evidence_status["missing_count"], 0);
    assert_eq!(evidence_status["invalid_count"], 0);
    assert_all_evidence_items_present(&evidence_status);

    let mut doctor_env = evidence_env;
    doctor_env.push(("JARVIS_EVIDENCE_STATUS_ENDPOINT", endpoint.as_str()));
    let doctor_output = run_repo_script_with_env(
        "scripts/release-evidence-doctor.sh",
        &["--assert-complete"],
        &doctor_env,
    );
    let doctor_text = String::from_utf8(doctor_output.stdout).expect("doctor stdout is utf8");
    assert!(
        doctor_text.contains("Jarvis release evidence inventory: complete"),
        "{doctor_text}"
    );
    assert!(
        doctor_text.contains("Missing evidence items: 0"),
        "{doctor_text}"
    );
    assert!(
        doctor_text.contains("host-level egress enforcement"),
        "{doctor_text}"
    );
    server.stop();
}

#[test]
#[cfg(unix)]
fn release_evidence_doctor_assert_complete_rejects_unresolved_command_evidence() {
    let temp_dir = tempfile::tempdir().expect("temp complete release evidence");
    let fixture = write_complete_release_evidence_fixture(temp_dir.path());
    let evidence_env = fixture.env_refs();
    let db_path = temp_dir
        .path()
        .join("jarvis-doctor-unresolved-command-evidence.sqlite");
    let mut server = JarvisServer::start_with_env(&db_path, &evidence_env);
    let endpoint = server.endpoint();

    let evidence_status = run_cli_json([
        "release",
        "evidence-status",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let live_item = release_evidence_item(&evidence_status, "live_device_qa_report");
    assert_eq!(live_item["status"], "invalid", "{live_item}");
    assert!(live_item["detail"]
        .as_str()
        .expect("live detail")
        .contains("does not resolve to repository evidence"));

    let mut doctor_env = fixture.env_refs();
    doctor_env.push(("JARVIS_EVIDENCE_STATUS_ENDPOINT", endpoint.as_str()));
    let doctor_output = run_repo_script_failure_with_env(
        "scripts/release-evidence-doctor.sh",
        &["--assert-complete"],
        &doctor_env,
    );
    assert!(
        doctor_output.contains(
            "release evidence doctor --assert-complete requires jarvis release evidence-status --json"
        ),
        "{doctor_output}"
    );
    assert!(
        doctor_output.contains("live_device_qa_report=invalid"),
        "{doctor_output}"
    );
    server.stop();
}

#[test]
#[cfg(unix)]
fn release_evidence_doctor_rejects_final_bundle_with_invalid_child_report() {
    let temp_dir = tempfile::tempdir().expect("temp complete release evidence");
    let fixture = write_complete_release_evidence_fixture(temp_dir.path());

    let mut live_report: Value = serde_json::from_str(
        &fs::read_to_string(&fixture.live_report_path).expect("read live report"),
    )
    .expect("decode live report");
    live_report["validation_flags"]["notification"] = json!(false);
    write_json_report(Path::new(&fixture.live_report_path), live_report);

    let mut bundle_report: Value = serde_json::from_str(
        &fs::read_to_string(&fixture.bundle_path).expect("read evidence bundle"),
    )
    .expect("decode evidence bundle");
    bundle_report["reports"]["live_device_qa_sha256"] =
        json!(file_sha256(Path::new(&fixture.live_report_path)));
    write_json_report(Path::new(&fixture.bundle_path), bundle_report);

    let doctor_output = run_repo_script_with_env(
        "scripts/release-evidence-doctor.sh",
        &["--check"],
        &fixture.env_refs(),
    );
    let doctor_text = String::from_utf8(doctor_output.stdout).expect("doctor stdout is utf8");
    assert!(
        doctor_text.contains("release evidence bundle references invalid live-device QA report"),
        "{doctor_text}"
    );
    assert!(
        doctor_text
            .contains("live-device QA report missing true flag: validation_flags.notification"),
        "{doctor_text}"
    );
}

#[test]
fn release_readiness_rejects_semantically_invalid_live_voice_evidence() {
    let endpoint = format!("http://{}", unused_loopback_addr());

    fn wrong_bundle_id(report: &mut Value) {
        report["app_bundle"]["bundle_identifier"] = json!("com.example.StaleJarvis");
    }
    fn wrong_version(report: &mut Value) {
        report["app_bundle"]["short_version"] = json!("9.9.9");
    }
    fn mismatched_microphone_privacy_prompt(report: &mut Value) {
        report["app_bundle"]["microphone_usage_description"] = json!("Jarvis microphone fixture");
    }
    fn mismatched_speech_privacy_prompt(report: &mut Value) {
        report["app_bundle"]["speech_recognition_usage_description"] =
            json!("Jarvis speech fixture");
    }
    fn bad_started_timestamp(report: &mut Value) {
        report["owner_recorded_live_voice_evidence"]["voice_check_started_at"] =
            json!("not-a-timestamp");
    }
    fn reversed_timestamps(report: &mut Value) {
        report["owner_recorded_live_voice_evidence"]["voice_check_started_at"] =
            json!("2026-05-22T16:05:00Z");
        report["owner_recorded_live_voice_evidence"]["voice_check_completed_at"] =
            json!("2026-05-22T16:00:00Z");
    }
    fn self_test_fixture(report: &mut Value) {
        report["self_test_fixture"] = json!(true);
    }
    fn wrong_installed_app_path(report: &mut Value) {
        report["installed_app_path"] = json!("/tmp/Jarvis.app");
    }
    fn wrong_bundled_core_path(report: &mut Value) {
        report["bundled_core"]["executable_path"] = json!("/tmp/jarvis-cli");
    }
    fn wrong_bundled_core_version(report: &mut Value) {
        report["bundled_core"]["version"] = json!("jarvis 9.9.9");
    }
    fn malformed_bundled_core_digest(report: &mut Value) {
        report["bundled_core"]["sha256"] = json!("not-a-digest");
    }
    fn false_live_validation_flag(report: &mut Value) {
        report["validation_flags"]["clean_profile"] = json!(false);
    }
    fn false_voice_loop_flag(report: &mut Value) {
        report["voice_loop"]["same_command_path"] = json!(false);
    }
    fn mismatched_observed_transcript(report: &mut Value) {
        report["voice_command_observation"]["observed_transcript"] = json!("Jarvis stats check.");
    }
    fn malformed_command_result_evidence_id(report: &mut Value) {
        report["voice_command_observation"]["command_result_evidence_id"] = json!("looked good");
    }
    fn blank_owner_evidence_note(report: &mut Value) {
        report["owner_recorded_live_voice_evidence"]["audio_output_evidence_note"] = json!("   ");
    }
    fn blank_non_voice_owner_evidence_note(report: &mut Value) {
        report["owner_recorded_non_voice_evidence"]["clean_profile_evidence_note"] = json!("   ");
    }
    fn malformed_notification_timestamp(report: &mut Value) {
        report["owner_recorded_non_voice_evidence"]["notification_observed_at"] =
            json!("2026-05-22T16:04:00-04:00");
    }
    fn mismatched_notification_observation_timestamp(report: &mut Value) {
        report["notification_observation"]["observed_at"] = json!("2026-05-22T16:03:00Z");
    }
    fn wrong_notification_thread_identifier(report: &mut Value) {
        report["notification_observation"]["thread_identifier"] = json!("jarvis.other");
    }
    fn invalid_notification_kind(report: &mut Value) {
        report["notification_observation"]["kind"] = json!("visible");
    }
    fn blank_audio_output_device_label(report: &mut Value) {
        report["voice_command_observation"]["audio_output_device_label"] = json!("   ");
    }
    fn blank_proof_boundary(report: &mut Value) {
        report["proof_boundary"] = json!("   ");
    }

    for (name, mutate, detail_fragment) in [
        (
            "wrong bundle id",
            wrong_bundle_id as fn(&mut Value),
            "app_bundle.bundle_identifier",
        ),
        (
            "wrong version",
            wrong_version as fn(&mut Value),
            "app_bundle.short_version",
        ),
        (
            "mismatched microphone privacy prompt",
            mismatched_microphone_privacy_prompt as fn(&mut Value),
            "app_bundle.microphone_usage_description",
        ),
        (
            "mismatched speech privacy prompt",
            mismatched_speech_privacy_prompt as fn(&mut Value),
            "app_bundle.speech_recognition_usage_description",
        ),
        (
            "bad started timestamp",
            bad_started_timestamp as fn(&mut Value),
            "voice_check_started_at",
        ),
        (
            "reversed timestamps",
            reversed_timestamps as fn(&mut Value),
            "voice_check_completed_at",
        ),
        (
            "self-test fixture",
            self_test_fixture as fn(&mut Value),
            "self-test fixture",
        ),
        (
            "wrong installed app path",
            wrong_installed_app_path as fn(&mut Value),
            "installed_app_path",
        ),
        (
            "wrong bundled core path",
            wrong_bundled_core_path as fn(&mut Value),
            "bundled_core.executable_path",
        ),
        (
            "wrong bundled core version",
            wrong_bundled_core_version as fn(&mut Value),
            "bundled_core.version",
        ),
        (
            "malformed bundled core digest",
            malformed_bundled_core_digest as fn(&mut Value),
            "bundled_core.sha256",
        ),
        (
            "false live validation flag",
            false_live_validation_flag as fn(&mut Value),
            "validation_flags.clean_profile",
        ),
        (
            "false voice loop flag",
            false_voice_loop_flag as fn(&mut Value),
            "voice_loop.same_command_path",
        ),
        (
            "mismatched observed transcript",
            mismatched_observed_transcript as fn(&mut Value),
            "observed_transcript",
        ),
        (
            "malformed command result evidence id",
            malformed_command_result_evidence_id as fn(&mut Value),
            "command_result_evidence_id",
        ),
        (
            "blank owner evidence note",
            blank_owner_evidence_note as fn(&mut Value),
            "audio_output_evidence_note",
        ),
        (
            "blank non-voice owner evidence note",
            blank_non_voice_owner_evidence_note as fn(&mut Value),
            "owner_recorded_non_voice_evidence.clean_profile_evidence_note",
        ),
        (
            "malformed notification timestamp",
            malformed_notification_timestamp as fn(&mut Value),
            "notification_observed_at",
        ),
        (
            "mismatched notification observation timestamp",
            mismatched_notification_observation_timestamp as fn(&mut Value),
            "notification_observation.observed_at",
        ),
        (
            "wrong notification thread identifier",
            wrong_notification_thread_identifier as fn(&mut Value),
            "notification_observation.thread_identifier",
        ),
        (
            "invalid notification kind",
            invalid_notification_kind as fn(&mut Value),
            "notification_observation.kind",
        ),
        (
            "blank audio output device label",
            blank_audio_output_device_label as fn(&mut Value),
            "audio_output_device_label",
        ),
        (
            "blank proof boundary",
            blank_proof_boundary as fn(&mut Value),
            "proof_boundary",
        ),
    ] {
        let temp_dir = tempfile::tempdir().expect("temp live QA report");
        let live_report_path = temp_dir
            .path()
            .join(format!("release-live-device-qa-report-{name}.json"));
        let mut report = valid_live_device_qa_report();
        mutate(&mut report);
        write_live_device_qa_report(&live_report_path, report);
        let report_path = live_report_path
            .to_str()
            .expect("live report path is UTF-8")
            .to_string();

        let evidence_status = run_cli_json_with_env(
            [
                "release",
                "evidence-status",
                "--endpoint",
                endpoint.as_str(),
            ],
            &[("JARVIS_QA_REPORT_PATH", report_path.as_str())],
        );
        let live_item = evidence_status["items"]
            .as_array()
            .expect("evidence items")
            .iter()
            .find(|item| item["key"] == "live_device_qa_report")
            .unwrap_or_else(|| panic!("missing live-device QA item for {name}"));
        assert_eq!(live_item["status"], "invalid", "{name}: {live_item}");
        assert!(
            live_item["detail"]
                .as_str()
                .expect("detail string")
                .contains(detail_fragment),
            "{name}: {live_item}"
        );

        let readiness = run_cli_json_with_env(
            ["release", "readiness", "--endpoint", endpoint.as_str()],
            &[
                ("JARVIS_QA_REPORT_PATH", report_path.as_str()),
                ("JARVIS_RELEASE_READINESS_EVIDENCE_MODE", "external"),
            ],
        );
        assert_array_contains(&readiness["pending_features"], "key", "live_voice_loop");
        assert!(!readiness["implemented_features"]
            .as_array()
            .expect("implemented features")
            .iter()
            .any(|feature| feature["key"] == "live_voice_loop"));
        assert!(readiness["blocking_manual_gates"]
            .as_array()
            .expect("blocking gates")
            .iter()
            .any(|gate| gate
                .as_str()
                .expect("gate string")
                .contains("live microphone")));
    }
}

#[test]
fn release_readiness_rejects_missing_live_voice_evidence_fields() {
    let endpoint = format!("http://{}", unused_loopback_addr());

    for missing_path in [
        "owner_recorded_live_voice_evidence.microphone_evidence_note",
        "owner_recorded_live_voice_evidence.speech_permission_evidence_note",
        "owner_recorded_live_voice_evidence.transcript_handoff_evidence_note",
        "owner_recorded_live_voice_evidence.audio_output_evidence_note",
        "voice_command_observation.command_result_evidence_id",
        "voice_command_observation.audio_output_device_label",
        "notification_observation.title",
        "notification_observation.body",
        "notification_observation.thread_identifier",
        "notification_observation.observed_at",
    ] {
        let temp_dir = tempfile::tempdir().expect("temp missing live QA report");
        let live_report_path = temp_dir
            .path()
            .join(format!("release-live-device-qa-report-{missing_path}.json"));
        let mut report = valid_live_device_qa_report();
        remove_json_field(&mut report, missing_path);
        write_live_device_qa_report(&live_report_path, report);
        let report_path = live_report_path
            .to_str()
            .expect("live report path is UTF-8")
            .to_string();

        let evidence_status = run_cli_json_with_env(
            [
                "release",
                "evidence-status",
                "--endpoint",
                endpoint.as_str(),
            ],
            &[("JARVIS_QA_REPORT_PATH", report_path.as_str())],
        );
        let live_item = release_evidence_item(&evidence_status, "live_device_qa_report");
        assert_eq!(
            live_item["status"], "invalid",
            "{missing_path}: {live_item}"
        );
        assert!(
            live_item["detail"]
                .as_str()
                .expect("detail string")
                .contains(missing_path),
            "{missing_path}: {live_item}"
        );

        let readiness = run_cli_json_with_env(
            ["release", "readiness", "--endpoint", endpoint.as_str()],
            &[
                ("JARVIS_QA_REPORT_PATH", report_path.as_str()),
                ("JARVIS_RELEASE_READINESS_EVIDENCE_MODE", "external"),
            ],
        );
        assert_eq!(readiness["production_ready"], false);
        assert_array_contains(&readiness["pending_features"], "key", "live_voice_loop");
        assert!(
            !readiness["implemented_features"]
                .as_array()
                .expect("implemented features")
                .iter()
                .any(|feature| feature["key"] == "live_voice_loop"),
            "{missing_path}: {readiness}"
        );
    }
}

#[test]
fn release_evidence_status_resolves_live_voice_command_result_against_repository() {
    let temp_dir = tempfile::tempdir().expect("temp live QA repository evidence");
    let db_path = temp_dir.path().join("jarvis-e2e.sqlite");
    let live_report_path = temp_dir.path().join("release-live-device-qa-report.json");
    let report_path = live_report_path
        .to_str()
        .expect("live report path is UTF-8")
        .to_string();
    let mut server =
        JarvisServer::start_with_env(&db_path, &[("JARVIS_QA_REPORT_PATH", report_path.as_str())]);
    let endpoint = server.endpoint();

    let command = run_cli_json([
        "command",
        "status check",
        "--json",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(command["accepted"], true, "{command}");
    let task_id = command["task"]["id"].as_str().expect("task id");
    let task_audit_id = command["audit_entries"]
        .as_array()
        .expect("audit entries")
        .iter()
        .find(|entry| entry["task_id"].as_str() == Some(task_id))
        .and_then(|entry| entry["id"].as_str())
        .expect("task audit id")
        .to_string();

    let mut task_report = valid_live_device_qa_report();
    task_report["voice_command_observation"]["command_result_evidence_id"] =
        json!(format!("task:{task_id}"));
    write_live_device_qa_report(&live_report_path, task_report);
    let task_evidence_status = run_cli_json([
        "release",
        "evidence-status",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_release_evidence_item_status(
        &task_evidence_status,
        "live_device_qa_report",
        "present",
        "task evidence should resolve against the served repository",
    );

    let mut audit_report = valid_live_device_qa_report();
    audit_report["voice_command_observation"]["command_result_evidence_id"] =
        json!(format!("audit:{task_audit_id}"));
    write_live_device_qa_report(&live_report_path, audit_report);
    let audit_evidence_status = run_cli_json([
        "release",
        "evidence-status",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_release_evidence_item_status(
        &audit_evidence_status,
        "live_device_qa_report",
        "present",
        "task audit evidence should resolve against the served repository",
    );

    let unrelated_command = run_cli_json([
        "command",
        "different command",
        "--json",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(unrelated_command["accepted"], true, "{unrelated_command}");
    let unrelated_task_id = unrelated_command["task"]["id"].as_str().expect("task id");
    let unrelated_task_audit_id = unrelated_command["audit_entries"]
        .as_array()
        .expect("unrelated audit entries")
        .iter()
        .find(|entry| entry["task_id"].as_str() == Some(unrelated_task_id))
        .and_then(|entry| entry["id"].as_str())
        .expect("unrelated task audit id")
        .to_string();
    let mut wrong_task_report = valid_live_device_qa_report();
    wrong_task_report["voice_command_observation"]["command_result_evidence_id"] =
        json!(format!("task:{unrelated_task_id}"));
    write_live_device_qa_report(&live_report_path, wrong_task_report);
    let wrong_task_evidence_status = run_cli_json([
        "release",
        "evidence-status",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_release_evidence_item_status(
        &wrong_task_evidence_status,
        "live_device_qa_report",
        "invalid",
        "wrong task evidence must not clear live-device QA",
    );
    let wrong_task_item =
        release_evidence_item(&wrong_task_evidence_status, "live_device_qa_report");
    assert!(
        wrong_task_item["detail"]
            .as_str()
            .expect("live detail")
            .contains("does not match observed_command_text"),
        "{wrong_task_item}"
    );
    let wrong_task_readiness = run_cli_json_with_env(
        ["release", "readiness", "--endpoint", endpoint.as_str()],
        &[("JARVIS_RELEASE_READINESS_EVIDENCE_MODE", "external")],
    );
    assert_array_contains(
        &wrong_task_readiness["pending_features"],
        "key",
        "live_voice_loop",
    );

    let mut wrong_audit_report = valid_live_device_qa_report();
    wrong_audit_report["voice_command_observation"]["command_result_evidence_id"] =
        json!(format!("audit:{unrelated_task_audit_id}"));
    write_live_device_qa_report(&live_report_path, wrong_audit_report);
    let wrong_audit_evidence_status = run_cli_json([
        "release",
        "evidence-status",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_release_evidence_item_status(
        &wrong_audit_evidence_status,
        "live_device_qa_report",
        "invalid",
        "wrong audit evidence must not clear live-device QA",
    );
    let wrong_audit_item =
        release_evidence_item(&wrong_audit_evidence_status, "live_device_qa_report");
    assert!(
        wrong_audit_item["detail"]
            .as_str()
            .expect("live detail")
            .contains("does not match observed_command_text"),
        "{wrong_audit_item}"
    );
    let wrong_audit_readiness = run_cli_json_with_env(
        ["release", "readiness", "--endpoint", endpoint.as_str()],
        &[("JARVIS_RELEASE_READINESS_EVIDENCE_MODE", "external")],
    );
    assert_array_contains(
        &wrong_audit_readiness["pending_features"],
        "key",
        "live_voice_loop",
    );

    let mut missing_report = valid_live_device_qa_report();
    missing_report["voice_command_observation"]["command_result_evidence_id"] =
        json!("task:00000000-0000-4000-8000-000000009999");
    write_live_device_qa_report(&live_report_path, missing_report);
    let missing_evidence_status = run_cli_json([
        "release",
        "evidence-status",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_release_evidence_item_status(
        &missing_evidence_status,
        "live_device_qa_report",
        "invalid",
        "missing task evidence must not clear live-device QA",
    );
    let live_item = release_evidence_item(&missing_evidence_status, "live_device_qa_report");
    assert!(
        live_item["detail"]
            .as_str()
            .expect("live detail")
            .contains("does not resolve to repository evidence"),
        "{live_item}"
    );

    let readiness = run_cli_json_with_env(
        ["release", "readiness", "--endpoint", endpoint.as_str()],
        &[("JARVIS_RELEASE_READINESS_EVIDENCE_MODE", "external")],
    );
    assert_array_contains(&readiness["pending_features"], "key", "live_voice_loop");
    assert!(!readiness["implemented_features"]
        .as_array()
        .expect("implemented features")
        .iter()
        .any(|feature| feature["key"] == "live_voice_loop"));

    server.stop();
}

#[test]
fn release_evidence_status_rejects_semantically_invalid_plugin_and_bundle_evidence() {
    let endpoint = format!("http://{}", unused_loopback_addr());

    let temp_dir = tempfile::tempdir().expect("temp release evidence reports");
    let plugin_report_path = temp_dir.path().join("release-plugin-trust-qa-report.json");
    let bundle_path = temp_dir.path().join("release-evidence-bundle.json");

    let mut plugin_report = valid_plugin_trust_qa_report();
    plugin_report["owner_recorded_plugin_trust_evidence"]["review_started_at"] =
        json!("2026-05-22T16:20:00Z");
    plugin_report["owner_recorded_plugin_trust_evidence"]["review_completed_at"] =
        json!("2026-05-22T16:10:00Z");
    write_json_report(&plugin_report_path, plugin_report);

    let mut bundle_report = valid_release_evidence_bundle();
    bundle_report["validation_flags"]["local_signature_validation"] = json!(false);
    write_json_report(&bundle_path, bundle_report);

    let plugin_path = plugin_report_path.to_str().expect("plugin report utf8");
    let bundle_path = bundle_path.to_str().expect("bundle report utf8");
    let evidence_status = run_cli_json_with_env(
        [
            "release",
            "evidence-status",
            "--endpoint",
            endpoint.as_str(),
        ],
        &[
            ("JARVIS_EVIDENCE_PLUGIN_QA_REPORT", plugin_path),
            ("JARVIS_EVIDENCE_OUTPUT_PATH", bundle_path),
        ],
    );
    let items = evidence_status["items"].as_array().expect("evidence items");
    let plugin_item = items
        .iter()
        .find(|item| item["key"] == "plugin_trust_qa_report")
        .expect("plugin trust item");
    assert_eq!(plugin_item["status"], "invalid", "{plugin_item}");
    assert!(
        plugin_item["detail"]
            .as_str()
            .expect("plugin detail")
            .contains("review_completed_at"),
        "{plugin_item}"
    );
    let bundle_item = items
        .iter()
        .find(|item| item["key"] == "release_evidence_bundle")
        .expect("release evidence bundle item");
    assert_eq!(bundle_item["status"], "invalid", "{bundle_item}");
    assert!(
        bundle_item["detail"]
            .as_str()
            .expect("bundle detail")
            .contains("local_signature_validation"),
        "{bundle_item}"
    );
}

#[test]
fn release_evidence_status_rejects_invalid_plugin_artifact_bindings() {
    let endpoint = format!("http://{}", unused_loopback_addr());
    let temp_dir = tempfile::tempdir().expect("temp release plugin report");
    let plugin_report_path = temp_dir.path().join("release-plugin-trust-qa-report.json");

    for artifact in plugin_trust_artifact_keys() {
        for (field, invalid_value) in [
            ("uri", json!("   ")),
            ("uri", json!("jarvis/plugin-trust/artifact.json")),
            (
                "uri",
                json!("file:///tmp/jarvis/plugin-trust/artifact.json"),
            ),
            ("sha256", json!("not-a-sha")),
        ] {
            let mut plugin_report = valid_plugin_trust_qa_report();
            plugin_report["evidence_artifacts"][artifact][field] = invalid_value;
            write_json_report(&plugin_report_path, plugin_report);
            let plugin_path = plugin_report_path.to_str().expect("plugin report utf8");
            let evidence_status = run_cli_json_with_env(
                [
                    "release",
                    "evidence-status",
                    "--endpoint",
                    endpoint.as_str(),
                ],
                &[("JARVIS_EVIDENCE_PLUGIN_QA_REPORT", plugin_path)],
            );
            let plugin_item = release_evidence_item(&evidence_status, "plugin_trust_qa_report");
            let detail_fragment = format!("evidence_artifacts.{artifact}.{field}");
            assert_eq!(
                plugin_item["status"], "invalid",
                "{detail_fragment}: {plugin_item}"
            );
            assert!(
                plugin_item["detail"]
                    .as_str()
                    .expect("plugin detail")
                    .contains(&detail_fragment),
                "{detail_fragment}: {plugin_item}"
            );
            if field == "uri" {
                let detail = plugin_item["detail"].as_str().expect("plugin detail");
                assert!(
                    detail.contains("durable release evidence archive")
                        || detail.contains("URI with a scheme")
                        || detail.contains("missing required fields"),
                    "{detail_fragment}: {plugin_item}"
                );
            }
        }
    }
}

#[test]
fn release_evidence_status_rejects_invalid_final_bundle_owner_evidence() {
    let endpoint = format!("http://{}", unused_loopback_addr());
    let temp_dir = tempfile::tempdir().expect("temp release evidence bundle");
    let bundle_path = temp_dir.path().join("release-evidence-bundle.json");

    fn blank_archive_uri(report: &mut Value) {
        report["owner_recorded_release_evidence"]["reports_archive_uri"] = json!("   ");
    }
    fn placeholder_archive_uri(report: &mut Value) {
        report["owner_recorded_release_evidence"]["reports_archive_uri"] =
            json!("file://self-test/release-evidence");
    }
    fn archive_location_without_uri_scheme(report: &mut Value) {
        report["owner_recorded_release_evidence"]["reports_archive_uri"] =
            json!("release-evidence/archive");
    }
    fn malformed_completed_timestamp(report: &mut Value) {
        report["owner_recorded_release_evidence"]["completed_at"] =
            json!("2026-05-22T16:45:00-04:00");
    }
    fn completed_after_generated(report: &mut Value) {
        report["owner_recorded_release_evidence"]["completed_at"] = json!("2026-05-22T17:01:00Z");
    }

    for (name, mutate, detail_fragment) in [
        (
            "blank archive uri",
            blank_archive_uri as fn(&mut Value),
            "owner_recorded_release_evidence.reports_archive_uri",
        ),
        (
            "placeholder archive uri",
            placeholder_archive_uri as fn(&mut Value),
            "durable release evidence archive",
        ),
        (
            "archive location without URI scheme",
            archive_location_without_uri_scheme as fn(&mut Value),
            "URI with a scheme",
        ),
        (
            "malformed completed timestamp",
            malformed_completed_timestamp as fn(&mut Value),
            "completed_at",
        ),
        (
            "completed after generated",
            completed_after_generated as fn(&mut Value),
            "completed_at",
        ),
    ] {
        let mut bundle_report = valid_release_evidence_bundle();
        mutate(&mut bundle_report);
        write_json_report(&bundle_path, bundle_report);
        let bundle_path_str = bundle_path.to_str().expect("bundle path utf8");
        let evidence_status = run_cli_json_with_env(
            [
                "release",
                "evidence-status",
                "--endpoint",
                endpoint.as_str(),
            ],
            &[("JARVIS_EVIDENCE_OUTPUT_PATH", bundle_path_str)],
        );
        let bundle_item = release_evidence_item(&evidence_status, "release_evidence_bundle");
        assert_eq!(bundle_item["status"], "invalid", "{name}: {bundle_item}");
        assert!(
            bundle_item["detail"]
                .as_str()
                .expect("bundle detail")
                .contains(detail_fragment),
            "{name}: {bundle_item}"
        );
    }
}

#[test]
#[cfg(unix)]
fn release_evidence_bundle_assertion_rejects_temporary_archive_uri() {
    let temp_dir = tempfile::tempdir().expect("temp release evidence bundle assertion");
    let fixture = write_complete_release_evidence_fixture(temp_dir.path());
    let output_path = temp_dir
        .path()
        .join("generated-release-evidence-bundle.json");
    let output_path_str = output_path.to_str().expect("bundle output path utf8");
    let mut env = fixture.env_refs();
    env.extend([
        ("JARVIS_EVIDENCE_OUTPUT_PATH", output_path_str),
        ("JARVIS_EVIDENCE_SELF_TEST_MODE", "true"),
        ("JARVIS_EVIDENCE_VALIDATE_LOCAL_SIGNATURES", "false"),
        ("JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_VALIDATED", "true"),
        ("JARVIS_EVIDENCE_NOTARIZATION_VALIDATED", "true"),
        ("JARVIS_EVIDENCE_CLEAN_PROFILE_VALIDATED", "true"),
        ("JARVIS_EVIDENCE_LIVE_DEVICE_QA_VALIDATED", "true"),
        ("JARVIS_EVIDENCE_PLUGIN_TRUST_QA_VALIDATED", "true"),
        ("JARVIS_EVIDENCE_REPORTS_ARCHIVED", "true"),
        ("JARVIS_EVIDENCE_OWNER_NAME", "Jarvis Release E2E"),
        ("JARVIS_EVIDENCE_COMPLETED_AT", "2026-05-22T16:45:00Z"),
        (
            "JARVIS_EVIDENCE_SIGNED_DISTRIBUTION_NOTE",
            "Signed distribution provenance reviewed in the controlled release lane.",
        ),
        (
            "JARVIS_EVIDENCE_NOTARIZATION_NOTE",
            "Notarization evidence reviewed in the controlled release lane.",
        ),
        (
            "JARVIS_EVIDENCE_CLEAN_PROFILE_NOTE",
            "Clean profile evidence reviewed in the controlled release lane.",
        ),
        (
            "JARVIS_EVIDENCE_LIVE_DEVICE_QA_NOTE",
            "Live-device QA evidence reviewed in the controlled release lane.",
        ),
        (
            "JARVIS_EVIDENCE_PLUGIN_TRUST_QA_NOTE",
            "Plugin-trust QA evidence reviewed in the controlled release lane.",
        ),
        (
            "JARVIS_EVIDENCE_REPORTS_ARCHIVE_NOTE",
            "Release evidence reports archived in the controlled release lane.",
        ),
        (
            "JARVIS_EVIDENCE_REPORTS_ARCHIVE_URI",
            "file:///tmp/jarvis/release-evidence",
        ),
    ]);

    let output =
        run_repo_script_failure_with_env("scripts/release-evidence-bundle.sh", &["--bundle"], &env);
    assert!(
        output.contains("JARVIS_EVIDENCE_REPORTS_ARCHIVE_URI"),
        "{output}"
    );
    assert!(
        output.contains("durable release evidence archive"),
        "{output}"
    );
    assert!(
        !output_path.exists(),
        "temporary archive URI failure must not write {}",
        output_path.display()
    );
}

#[test]
fn release_evidence_status_rejects_false_plugin_and_bundle_validation_flags() {
    let endpoint = format!("http://{}", unused_loopback_addr());

    let temp_dir = tempfile::tempdir().expect("temp release evidence reports");
    let plugin_report_path = temp_dir.path().join("release-plugin-trust-qa-report.json");
    let bundle_path = temp_dir.path().join("release-evidence-bundle.json");

    for (field, detail_fragment) in [
        ("marketplace_review", "validation_flags.marketplace_review"),
        ("malware_scan", "validation_flags.malware_scan"),
        ("os_sandbox", "validation_flags.os_sandbox"),
        ("egress_enforcement", "validation_flags.egress_enforcement"),
        (
            "signed_publisher_policy",
            "validation_flags.signed_publisher_policy",
        ),
        (
            "manual_trust_review",
            "validation_flags.manual_trust_review",
        ),
    ] {
        let mut plugin_report = valid_plugin_trust_qa_report();
        plugin_report["validation_flags"][field] = json!(false);
        write_json_report(&plugin_report_path, plugin_report);
        let plugin_path = plugin_report_path.to_str().expect("plugin report utf8");
        let evidence_status = run_cli_json_with_env(
            [
                "release",
                "evidence-status",
                "--endpoint",
                endpoint.as_str(),
            ],
            &[("JARVIS_EVIDENCE_PLUGIN_QA_REPORT", plugin_path)],
        );
        let plugin_item = release_evidence_item(&evidence_status, "plugin_trust_qa_report");
        assert_eq!(plugin_item["status"], "invalid", "{field}: {plugin_item}");
        assert!(
            plugin_item["detail"]
                .as_str()
                .expect("plugin detail")
                .contains(detail_fragment),
            "{field}: {plugin_item}"
        );
    }

    for (field, detail_fragment) in [
        (
            "signed_distribution",
            "validation_flags.signed_distribution",
        ),
        ("notarization", "validation_flags.notarization"),
        ("clean_profile", "validation_flags.clean_profile"),
        ("live_device_qa", "validation_flags.live_device_qa"),
        ("plugin_trust_qa", "validation_flags.plugin_trust_qa"),
        ("reports_archived", "validation_flags.reports_archived"),
        (
            "local_signature_validation",
            "validation_flags.local_signature_validation",
        ),
    ] {
        let mut bundle_report = valid_release_evidence_bundle();
        bundle_report["validation_flags"][field] = json!(false);
        write_json_report(&bundle_path, bundle_report);
        let bundle_path = bundle_path.to_str().expect("bundle report utf8");
        let evidence_status = run_cli_json_with_env(
            [
                "release",
                "evidence-status",
                "--endpoint",
                endpoint.as_str(),
            ],
            &[("JARVIS_EVIDENCE_OUTPUT_PATH", bundle_path)],
        );
        let bundle_item = release_evidence_item(&evidence_status, "release_evidence_bundle");
        assert_eq!(bundle_item["status"], "invalid", "{field}: {bundle_item}");
        assert!(
            bundle_item["detail"]
                .as_str()
                .expect("bundle detail")
                .contains(detail_fragment),
            "{field}: {bundle_item}"
        );
    }
}

#[test]
fn release_evidence_status_rejects_bundle_report_wrong_schema_identity() {
    let endpoint = format!("http://{}", unused_loopback_addr());

    let temp_dir = tempfile::tempdir().expect("temp release evidence reports");
    let bundle_path = temp_dir.path().join("release-evidence-bundle.json");

    let mut bundle_report = valid_release_evidence_bundle();
    bundle_report["evidence_type"] = json!("self_test_fixture");
    write_json_report(&bundle_path, bundle_report);

    let bundle_path = bundle_path.to_str().expect("bundle report utf8");
    let evidence_status = run_cli_json_with_env(
        [
            "release",
            "evidence-status",
            "--endpoint",
            endpoint.as_str(),
        ],
        &[("JARVIS_EVIDENCE_OUTPUT_PATH", bundle_path)],
    );
    let items = evidence_status["items"].as_array().expect("evidence items");
    let bundle_item = items
        .iter()
        .find(|item| item["key"] == "release_evidence_bundle")
        .expect("release evidence bundle item");
    assert_eq!(bundle_item["status"], "invalid", "{bundle_item}");
    assert!(
        bundle_item["detail"]
            .as_str()
            .expect("bundle detail")
            .contains("evidence_type"),
        "{bundle_item}"
    );
}

#[test]
fn release_evidence_status_rejects_plugin_report_wrong_schema_identity() {
    let endpoint = format!("http://{}", unused_loopback_addr());

    let temp_dir = tempfile::tempdir().expect("temp release evidence reports");
    let plugin_report_path = temp_dir.path().join("release-plugin-trust-qa-report.json");

    let mut plugin_report = valid_plugin_trust_qa_report();
    plugin_report["evidence_type"] = json!("self_test_fixture");
    write_json_report(&plugin_report_path, plugin_report);

    let plugin_path = plugin_report_path.to_str().expect("plugin report utf8");
    let evidence_status = run_cli_json_with_env(
        [
            "release",
            "evidence-status",
            "--endpoint",
            endpoint.as_str(),
        ],
        &[("JARVIS_EVIDENCE_PLUGIN_QA_REPORT", plugin_path)],
    );
    let items = evidence_status["items"].as_array().expect("evidence items");
    let plugin_item = items
        .iter()
        .find(|item| item["key"] == "plugin_trust_qa_report")
        .expect("plugin trust item");
    assert_eq!(plugin_item["status"], "invalid", "{plugin_item}");
    assert!(
        plugin_item["detail"]
            .as_str()
            .expect("plugin detail")
            .contains("evidence_type"),
        "{plugin_item}"
    );
}

#[test]
fn release_evidence_status_rejects_plugin_report_self_test_fixture_identity() {
    let endpoint = format!("http://{}", unused_loopback_addr());

    let temp_dir = tempfile::tempdir().expect("temp release evidence reports");
    let plugin_report_path = temp_dir.path().join("release-plugin-trust-qa-report.json");

    let mut plugin_report = valid_plugin_trust_qa_report();
    plugin_report["self_test_fixture"] = json!(true);
    write_json_report(&plugin_report_path, plugin_report);

    let plugin_path = plugin_report_path.to_str().expect("plugin report utf8");
    let evidence_status = run_cli_json_with_env(
        [
            "release",
            "evidence-status",
            "--endpoint",
            endpoint.as_str(),
        ],
        &[("JARVIS_EVIDENCE_PLUGIN_QA_REPORT", plugin_path)],
    );
    let items = evidence_status["items"].as_array().expect("evidence items");
    let plugin_item = items
        .iter()
        .find(|item| item["key"] == "plugin_trust_qa_report")
        .expect("plugin trust item");
    assert_eq!(plugin_item["status"], "invalid", "{plugin_item}");
    assert!(
        plugin_item["detail"]
            .as_str()
            .expect("plugin detail")
            .contains("self_test_fixture"),
        "{plugin_item}"
    );
}

#[test]
fn release_evidence_status_rejects_plugin_report_non_owner_review_source() {
    let endpoint = format!("http://{}", unused_loopback_addr());

    let temp_dir = tempfile::tempdir().expect("temp release evidence reports");
    let plugin_report_path = temp_dir.path().join("release-plugin-trust-qa-report.json");

    let mut plugin_report = valid_plugin_trust_qa_report();
    plugin_report["review_source"] = json!("imported-ci-report");
    write_json_report(&plugin_report_path, plugin_report);

    let plugin_path = plugin_report_path.to_str().expect("plugin report utf8");
    let evidence_status = run_cli_json_with_env(
        [
            "release",
            "evidence-status",
            "--endpoint",
            endpoint.as_str(),
        ],
        &[("JARVIS_EVIDENCE_PLUGIN_QA_REPORT", plugin_path)],
    );
    let plugin_item = release_evidence_item(&evidence_status, "plugin_trust_qa_report");
    assert_eq!(plugin_item["status"], "invalid", "{plugin_item}");
    let detail = plugin_item["detail"].as_str().expect("plugin detail");
    assert!(detail.contains("review_source"), "{plugin_item}");
    assert!(
        detail.contains("owner-asserted-manual-review"),
        "{plugin_item}"
    );
}

#[test]
fn release_evidence_status_rejects_plugin_report_wrong_version() {
    let endpoint = format!("http://{}", unused_loopback_addr());

    let temp_dir = tempfile::tempdir().expect("temp release evidence reports");
    let plugin_report_path = temp_dir.path().join("release-plugin-trust-qa-report.json");

    let mut plugin_report = valid_plugin_trust_qa_report();
    plugin_report["version"] = json!("9.9.9");
    write_json_report(&plugin_report_path, plugin_report);

    let plugin_path = plugin_report_path.to_str().expect("plugin report utf8");
    let evidence_status = run_cli_json_with_env(
        [
            "release",
            "evidence-status",
            "--endpoint",
            endpoint.as_str(),
        ],
        &[("JARVIS_EVIDENCE_PLUGIN_QA_REPORT", plugin_path)],
    );
    let plugin_item = release_evidence_item(&evidence_status, "plugin_trust_qa_report");
    assert_eq!(plugin_item["status"], "invalid", "{plugin_item}");
    assert!(
        plugin_item["detail"]
            .as_str()
            .expect("plugin detail")
            .contains("version"),
        "{plugin_item}"
    );
}

#[test]
#[cfg(unix)]
fn release_evidence_status_accepts_semantically_valid_plugin_and_bundle_evidence() {
    let temp_dir = tempfile::tempdir().expect("temp release evidence reports");
    let fixture = write_complete_release_evidence_fixture(temp_dir.path());
    let evidence_env = fixture.env_refs();
    let db_path = temp_dir.path().join("jarvis-valid-evidence.sqlite");
    let mut server = JarvisServer::start_with_env(&db_path, &evidence_env);
    let endpoint = server.endpoint();
    let command = run_cli_json([
        "command",
        "status check",
        "--json",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let task_id = command["task"]["id"].as_str().expect("task id");
    bind_complete_release_evidence_fixture_to_task(&fixture, task_id);

    let evidence_status = run_cli_json([
        "release",
        "evidence-status",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let items = evidence_status["items"].as_array().expect("evidence items");
    assert!(items
        .iter()
        .any(|item| item["key"] == "plugin_trust_qa_report"
            && item["status"] == "present"
            && item["detail"]
                .as_str()
                .expect("plugin detail")
                .contains("egress validation timestamps")));
    assert!(items
        .iter()
        .any(|item| item["key"] == "release_evidence_bundle"
            && item["status"] == "present"
            && item["detail"]
                .as_str()
                .expect("bundle detail")
                .contains("SHA-256")));
    server.stop();
}

#[test]
#[cfg(unix)]
fn release_evidence_status_rejects_stale_signed_provenance_artifact_digests() {
    let temp_dir = tempfile::tempdir().expect("temp release evidence reports");
    let fixture = write_complete_release_evidence_fixture(temp_dir.path());
    let endpoint = format!("http://{}", unused_loopback_addr());

    let mut stale_report = valid_signed_distribution_provenance_report(
        temp_dir
            .path()
            .join("dist/Jarvis.app")
            .to_str()
            .expect("app path utf8"),
        temp_dir
            .path()
            .join("dist/Jarvis-0.1.4.zip")
            .to_str()
            .expect("zip path utf8"),
        temp_dir
            .path()
            .join("dist/Jarvis-0.1.4.pkg")
            .to_str()
            .expect("pkg path utf8"),
        "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        &file_sha256(&temp_dir.path().join("dist/Jarvis-0.1.4.pkg")),
    );
    stale_report["artifacts"]["zip_sha256"] =
        json!("fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210");
    write_json_report(Path::new(&fixture.signed_provenance_path), stale_report);

    let evidence_env = fixture.env_refs();
    let evidence_status = run_cli_json_with_env(
        [
            "release",
            "evidence-status",
            "--endpoint",
            endpoint.as_str(),
        ],
        &evidence_env,
    );
    let signed_provenance_item = evidence_status["items"]
        .as_array()
        .expect("evidence items")
        .iter()
        .find(|item| item["key"] == "signed_distribution_provenance_report")
        .expect("signed provenance item");
    assert_eq!(
        signed_provenance_item["status"], "invalid",
        "{signed_provenance_item}"
    );
    assert!(
        signed_provenance_item["detail"]
            .as_str()
            .expect("signed provenance detail")
            .contains("artifacts.zip_sha256 does not match current app zip artifact"),
        "{signed_provenance_item}"
    );
}

#[test]
#[cfg(unix)]
fn release_evidence_status_rejects_rejected_signed_provenance_notary_status() {
    let temp_dir = tempfile::tempdir().expect("temp release evidence reports");
    let fixture = write_complete_release_evidence_fixture(temp_dir.path());
    let endpoint = format!("http://{}", unused_loopback_addr());

    let mut rejected_report = valid_signed_distribution_provenance_report(
        temp_dir
            .path()
            .join("dist/Jarvis.app")
            .to_str()
            .expect("app path utf8"),
        temp_dir
            .path()
            .join("dist/Jarvis-0.1.4.zip")
            .to_str()
            .expect("zip path utf8"),
        temp_dir
            .path()
            .join("dist/Jarvis-0.1.4.pkg")
            .to_str()
            .expect("pkg path utf8"),
        &file_sha256(&temp_dir.path().join("dist/Jarvis-0.1.4.zip")),
        &file_sha256(&temp_dir.path().join("dist/Jarvis-0.1.4.pkg")),
    );
    rejected_report["notarization"]["app_zip_status"] = json!("Rejected");
    write_json_report(Path::new(&fixture.signed_provenance_path), rejected_report);

    let evidence_status = run_cli_json_with_env(
        [
            "release",
            "evidence-status",
            "--endpoint",
            endpoint.as_str(),
        ],
        &fixture.env_refs(),
    );
    let signed_provenance_item = evidence_status["items"]
        .as_array()
        .expect("evidence items")
        .iter()
        .find(|item| item["key"] == "signed_distribution_provenance_report")
        .expect("signed provenance item");
    assert_eq!(
        signed_provenance_item["status"], "invalid",
        "{signed_provenance_item}"
    );
    assert!(
        signed_provenance_item["detail"]
            .as_str()
            .expect("signed provenance detail")
            .contains("notarization.app_zip_status"),
        "{signed_provenance_item}"
    );
}

#[test]
#[cfg(unix)]
fn release_evidence_status_rejects_stale_signed_provenance_core_version() {
    let temp_dir = tempfile::tempdir().expect("temp release evidence reports");
    let fixture = write_complete_release_evidence_fixture(temp_dir.path());
    let endpoint = format!("http://{}", unused_loopback_addr());

    let mut stale_report = valid_signed_distribution_provenance_report(
        temp_dir
            .path()
            .join("dist/Jarvis.app")
            .to_str()
            .expect("app path utf8"),
        temp_dir
            .path()
            .join("dist/Jarvis-0.1.4.zip")
            .to_str()
            .expect("zip path utf8"),
        temp_dir
            .path()
            .join("dist/Jarvis-0.1.4.pkg")
            .to_str()
            .expect("pkg path utf8"),
        &file_sha256(&temp_dir.path().join("dist/Jarvis-0.1.4.zip")),
        &file_sha256(&temp_dir.path().join("dist/Jarvis-0.1.4.pkg")),
    );
    stale_report["artifacts"]["bundled_core_version"] = json!("jarvis 9.9.9");
    write_json_report(Path::new(&fixture.signed_provenance_path), stale_report);

    let evidence_status = run_cli_json_with_env(
        [
            "release",
            "evidence-status",
            "--endpoint",
            endpoint.as_str(),
        ],
        &fixture.env_refs(),
    );
    let signed_provenance_item = evidence_status["items"]
        .as_array()
        .expect("evidence items")
        .iter()
        .find(|item| item["key"] == "signed_distribution_provenance_report")
        .expect("signed provenance item");
    assert_eq!(
        signed_provenance_item["status"], "invalid",
        "{signed_provenance_item}"
    );
    assert!(
        signed_provenance_item["detail"]
            .as_str()
            .expect("signed provenance detail")
            .contains("artifacts.bundled_core_version"),
        "{signed_provenance_item}"
    );
}

#[test]
#[cfg(unix)]
fn release_evidence_status_rejects_stale_signed_provenance_core_digest() {
    let temp_dir = tempfile::tempdir().expect("temp release evidence reports");
    let fixture = write_complete_release_evidence_fixture(temp_dir.path());
    let endpoint = format!("http://{}", unused_loopback_addr());

    let mut stale_report = valid_signed_distribution_provenance_report(
        temp_dir
            .path()
            .join("dist/Jarvis.app")
            .to_str()
            .expect("app path utf8"),
        temp_dir
            .path()
            .join("dist/Jarvis-0.1.4.zip")
            .to_str()
            .expect("zip path utf8"),
        temp_dir
            .path()
            .join("dist/Jarvis-0.1.4.pkg")
            .to_str()
            .expect("pkg path utf8"),
        &file_sha256(&temp_dir.path().join("dist/Jarvis-0.1.4.zip")),
        &file_sha256(&temp_dir.path().join("dist/Jarvis-0.1.4.pkg")),
    );
    stale_report["artifacts"]["bundled_core_sha256"] =
        json!("fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210");
    write_json_report(Path::new(&fixture.signed_provenance_path), stale_report);

    let evidence_status = run_cli_json_with_env(
        [
            "release",
            "evidence-status",
            "--endpoint",
            endpoint.as_str(),
        ],
        &fixture.env_refs(),
    );
    let signed_provenance_item = evidence_status["items"]
        .as_array()
        .expect("evidence items")
        .iter()
        .find(|item| item["key"] == "signed_distribution_provenance_report")
        .expect("signed provenance item");
    assert_eq!(
        signed_provenance_item["status"], "invalid",
        "{signed_provenance_item}"
    );
    assert!(
        signed_provenance_item["detail"]
            .as_str()
            .expect("signed provenance detail")
            .contains(
                "artifacts.bundled_core_sha256 does not match current bundled core executable"
            ),
        "{signed_provenance_item}"
    );
}

#[test]
#[cfg(unix)]
fn release_evidence_status_rejects_stale_final_bundle_report_digests() {
    let temp_dir = tempfile::tempdir().expect("temp release evidence reports");
    let fixture = write_complete_release_evidence_fixture(temp_dir.path());
    let endpoint = format!("http://{}", unused_loopback_addr());

    let mut stale_bundle: Value = serde_json::from_str(
        &fs::read_to_string(&fixture.bundle_path).expect("read evidence bundle"),
    )
    .expect("decode evidence bundle");
    stale_bundle["reports"]["plugin_trust_qa_sha256"] =
        json!("fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210");
    write_json_report(Path::new(&fixture.bundle_path), stale_bundle);

    let evidence_status = run_cli_json_with_env(
        [
            "release",
            "evidence-status",
            "--endpoint",
            endpoint.as_str(),
        ],
        &fixture.env_refs(),
    );
    let bundle_item = evidence_status["items"]
        .as_array()
        .expect("evidence items")
        .iter()
        .find(|item| item["key"] == "release_evidence_bundle")
        .expect("release evidence bundle item");
    assert_eq!(bundle_item["status"], "invalid", "{bundle_item}");
    assert!(
        bundle_item["detail"]
            .as_str()
            .expect("bundle detail")
            .contains(
                "reports.plugin_trust_qa_sha256 does not match current plugin-trust QA report"
            ),
        "{bundle_item}"
    );
}

#[test]
#[cfg(unix)]
fn release_evidence_status_rejects_live_device_core_digest_mismatch() {
    let temp_dir = tempfile::tempdir().expect("temp release evidence reports");
    let fixture = write_complete_release_evidence_fixture(temp_dir.path());
    let db_path = temp_dir.path().join("jarvis-live-core-mismatch.sqlite");
    let mut server = JarvisServer::start_with_env(&db_path, &fixture.env_refs());
    let endpoint = server.endpoint();
    let command = run_cli_json([
        "command",
        "status check",
        "--json",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let task_id = command["task"]["id"].as_str().expect("task id");
    bind_complete_release_evidence_fixture_to_task(&fixture, task_id);

    let mut live_report: Value = serde_json::from_str(
        &fs::read_to_string(&fixture.live_report_path).expect("read live report"),
    )
    .expect("decode live report");
    live_report["bundled_core"]["sha256"] =
        json!("fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210");
    write_json_report(Path::new(&fixture.live_report_path), live_report);

    let mut bundle_report: Value = serde_json::from_str(
        &fs::read_to_string(&fixture.bundle_path).expect("read evidence bundle"),
    )
    .expect("decode evidence bundle");
    bundle_report["reports"]["live_device_qa_sha256"] =
        json!(file_sha256(Path::new(&fixture.live_report_path)));
    write_json_report(Path::new(&fixture.bundle_path), bundle_report);

    let evidence_status = run_cli_json_with_env(
        [
            "release",
            "evidence-status",
            "--endpoint",
            endpoint.as_str(),
        ],
        &fixture.env_refs(),
    );
    let live_item = release_evidence_item(&evidence_status, "live_device_qa_report");
    assert_eq!(live_item["status"], "present", "{live_item}");
    let bundle_item = release_evidence_item(&evidence_status, "release_evidence_bundle");
    assert_eq!(bundle_item["status"], "invalid", "{bundle_item}");
    assert!(
        bundle_item["detail"]
            .as_str()
            .expect("bundle detail")
            .contains("bundled_core.sha256"),
        "{bundle_item}"
    );
    server.stop();
}

#[test]
#[cfg(unix)]
fn release_evidence_status_rejects_final_bundle_completed_before_child_reports() {
    let temp_dir = tempfile::tempdir().expect("temp release evidence reports");
    let fixture = write_complete_release_evidence_fixture(temp_dir.path());
    let db_path = temp_dir.path().join("jarvis-bundle-timestamp.sqlite");
    let mut server = JarvisServer::start_with_env(&db_path, &fixture.env_refs());
    let endpoint = server.endpoint();
    let command = run_cli_json([
        "command",
        "status check",
        "--json",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let task_id = command["task"]["id"].as_str().expect("task id");
    bind_complete_release_evidence_fixture_to_task(&fixture, task_id);

    let mut bundle_report: Value = serde_json::from_str(
        &fs::read_to_string(&fixture.bundle_path).expect("read evidence bundle"),
    )
    .expect("decode evidence bundle");
    bundle_report["owner_recorded_release_evidence"]["completed_at"] =
        json!("2026-05-22T16:05:30Z");
    write_json_report(Path::new(&fixture.bundle_path), bundle_report);

    let evidence_status = run_cli_json_with_env(
        [
            "release",
            "evidence-status",
            "--endpoint",
            endpoint.as_str(),
        ],
        &fixture.env_refs(),
    );
    let bundle_item = release_evidence_item(&evidence_status, "release_evidence_bundle");
    assert_eq!(bundle_item["status"], "invalid", "{bundle_item}");
    let detail = bundle_item["detail"].as_str().expect("bundle detail");
    assert!(
        detail.contains("generated after owner_recorded_release_evidence.completed_at"),
        "{bundle_item}"
    );
    server.stop();
}

#[test]
#[cfg(unix)]
fn release_evidence_status_rejects_final_bundle_with_invalid_child_report() {
    let temp_dir = tempfile::tempdir().expect("temp release evidence reports");
    let fixture = write_complete_release_evidence_fixture(temp_dir.path());
    let endpoint = format!("http://{}", unused_loopback_addr());

    let mut live_report: Value = serde_json::from_str(
        &fs::read_to_string(&fixture.live_report_path).expect("read live report"),
    )
    .expect("decode live report");
    live_report["validation_flags"]["notification"] = json!(false);
    write_json_report(Path::new(&fixture.live_report_path), live_report);

    let mut bundle_report: Value = serde_json::from_str(
        &fs::read_to_string(&fixture.bundle_path).expect("read evidence bundle"),
    )
    .expect("decode evidence bundle");
    bundle_report["reports"]["live_device_qa_sha256"] =
        json!(file_sha256(Path::new(&fixture.live_report_path)));
    write_json_report(Path::new(&fixture.bundle_path), bundle_report);

    let evidence_status = run_cli_json_with_env(
        [
            "release",
            "evidence-status",
            "--endpoint",
            endpoint.as_str(),
        ],
        &fixture.env_refs(),
    );
    let live_item = release_evidence_item(&evidence_status, "live_device_qa_report");
    assert_eq!(live_item["status"], "invalid", "{live_item}");
    let bundle_item = release_evidence_item(&evidence_status, "release_evidence_bundle");
    assert_eq!(bundle_item["status"], "invalid", "{bundle_item}");
    let detail = bundle_item["detail"].as_str().expect("bundle detail");
    assert!(
        detail.contains("live-device QA report referenced by release evidence bundle is invalid"),
        "{bundle_item}"
    );
    assert!(
        detail.contains("validation_flags.notification"),
        "{bundle_item}"
    );
}

#[test]
#[cfg(unix)]
fn release_evidence_status_rejects_future_dated_release_evidence_reports() {
    let endpoint = format!("http://{}", unused_loopback_addr());

    for item_key in [
        "signed_distribution_provenance_report",
        "live_device_qa_report",
        "plugin_trust_qa_report",
        "release_evidence_bundle",
    ] {
        let temp_dir = tempfile::tempdir().expect("temp release evidence reports");
        let fixture = write_complete_release_evidence_fixture(temp_dir.path());
        let report_path = match item_key {
            "signed_distribution_provenance_report" => fixture.signed_provenance_path.as_str(),
            "live_device_qa_report" => fixture.live_report_path.as_str(),
            "plugin_trust_qa_report" => fixture.plugin_report_path.as_str(),
            "release_evidence_bundle" => fixture.bundle_path.as_str(),
            _ => unreachable!("unknown release evidence item"),
        };
        let mut report: Value = serde_json::from_str(
            &fs::read_to_string(report_path).expect("read release evidence report"),
        )
        .expect("decode release evidence report");
        report["generated_at"] = json!("2999-01-01T00:00:00Z");
        write_json_report(Path::new(report_path), report);

        let evidence_status = run_cli_json_with_env(
            [
                "release",
                "evidence-status",
                "--endpoint",
                endpoint.as_str(),
            ],
            &fixture.env_refs(),
        );
        let evidence_item = evidence_status["items"]
            .as_array()
            .expect("evidence items")
            .iter()
            .find(|item| item["key"] == item_key)
            .unwrap_or_else(|| panic!("missing evidence item: {item_key}"));
        assert_eq!(evidence_item["status"], "invalid", "{evidence_item}");
        let detail = evidence_item["detail"].as_str().expect("evidence detail");
        assert!(detail.contains("generated_at"), "{evidence_item}");
        assert!(detail.contains("current time"), "{evidence_item}");

        if item_key == "live_device_qa_report" {
            let readable_status = run_cli_text_with_env(
                [
                    "release",
                    "evidence-status",
                    "--endpoint",
                    endpoint.as_str(),
                ],
                &fixture.env_refs(),
            );
            assert!(readable_status.contains("live_device_qa_report"));
            assert!(readable_status.contains(&format!("path: {}", fixture.live_report_path)));
            assert!(readable_status.contains("detail: JSON report generated_at"));
            assert!(readable_status.contains("current time"));
        }
    }
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
    let format_evidence_status: Value = serde_json::from_str(&run_cli_text([
        "release",
        "evidence-status",
        "--format",
        "json",
        "--endpoint",
        endpoint.as_str(),
    ]))
    .expect("release evidence-status --format json output");
    let readable_status = run_cli_text([
        "release",
        "evidence-status",
        "--endpoint",
        endpoint.as_str(),
    ]);

    assert_eq!(evidence_status["complete"], false);
    assert_eq!(format_evidence_status["complete"], false);
    assert_eq!(
        format_evidence_status["missing_count"],
        evidence_status["missing_count"]
    );
    assert!(evidence_status["items"]
        .as_array()
        .expect("evidence items")
        .iter()
        .any(|item| item["key"] == "signed_app_bundle" && item["label"] == "App bundle path"));
    assert!(evidence_status["items"]
        .as_array()
        .expect("evidence items")
        .iter()
        .any(|item| item["key"] == "signed_app_zip" && item["label"] == "App zip path"));
    assert!(evidence_status["items"]
        .as_array()
        .expect("evidence items")
        .iter()
        .any(|item| item["key"] == "signed_installer_package"
            && item["label"] == "Installer package path"));
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
    assert!(evidence_status["proof_boundary"]
        .as_str()
        .expect("evidence proof boundary")
        .contains("owner-asserted review-source semantics"));
    assert!(evidence_status["proof_boundary"]
        .as_str()
        .expect("evidence proof boundary")
        .contains("archive-URI"));
    assert!(readable_status.contains("Jarvis release evidence status:"));
    assert!(readable_status.contains("Complete: false"));
    assert!(readable_status.contains("Missing evidence:"));
    assert!(readable_status.contains("Invalid evidence:"));
    assert!(readable_status.contains("signed_app_bundle"));
    assert!(readable_status.contains("path: target/distribution/Jarvis.app"));
    assert!(readable_status.contains("detail: expected evidence path is missing"));
    assert!(readable_status.contains("release_evidence_bundle"));
    assert!(readable_status.contains("Raw JSON: rerun with --json"));
    assert!(
        serde_json::from_str::<Value>(&readable_status).is_err(),
        "default evidence-status output should be operator-readable text"
    );
}

#[test]
fn release_evidence_status_accepts_bundle_evidence_report_aliases() {
    let temp_dir = tempfile::tempdir().expect("temp evidence report");
    let live_report_path = temp_dir.path().join("custom-live-report.json");
    write_valid_live_device_qa_report(&live_report_path);
    let report_path = live_report_path
        .to_str()
        .expect("live report path is UTF-8")
        .to_string();
    let db_path = temp_dir.path().join("jarvis-alias-evidence.sqlite");
    let env = [("JARVIS_EVIDENCE_LIVE_QA_REPORT", report_path.as_str())];
    let mut server = JarvisServer::start_with_env(&db_path, &env);
    let endpoint = server.endpoint();
    let command = run_cli_json([
        "command",
        "status check",
        "--json",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let task_id = command["task"]["id"].as_str().expect("task id");
    let mut live_report: Value =
        serde_json::from_str(&fs::read_to_string(&live_report_path).expect("read live report"))
            .expect("decode live report");
    live_report["voice_command_observation"]["command_result_evidence_id"] =
        json!(format!("task:{task_id}"));
    write_json_report(&live_report_path, live_report);

    let evidence_status = run_cli_json([
        "release",
        "evidence-status",
        "--endpoint",
        endpoint.as_str(),
    ]);

    assert!(evidence_status["items"]
        .as_array()
        .expect("evidence items")
        .iter()
        .any(|item| item["key"] == "live_device_qa_report"
            && item["path"] == report_path
            && item["status"] == "present"));
    server.stop();
}

#[test]
fn release_evidence_status_prefers_bundle_evidence_report_env_over_legacy_aliases() {
    let temp_dir = tempfile::tempdir().expect("temp evidence report aliases");
    let live_report_path = temp_dir.path().join("bundle-live-report.json");
    let legacy_live_report_path = temp_dir.path().join("legacy-live-report.json");
    let plugin_report_path = temp_dir.path().join("bundle-plugin-report.json");
    let legacy_plugin_report_path = temp_dir.path().join("legacy-plugin-report.json");
    write_valid_live_device_qa_report(&live_report_path);
    write_valid_live_device_qa_report(&legacy_live_report_path);
    write_json_report(&plugin_report_path, valid_plugin_trust_qa_report());
    write_json_report(&legacy_plugin_report_path, valid_plugin_trust_qa_report());
    let live_report = live_report_path
        .to_str()
        .expect("live report path is UTF-8")
        .to_string();
    let legacy_live_report = legacy_live_report_path
        .to_str()
        .expect("legacy live report path is UTF-8")
        .to_string();
    let plugin_report = plugin_report_path
        .to_str()
        .expect("plugin report path is UTF-8")
        .to_string();
    let legacy_plugin_report = legacy_plugin_report_path
        .to_str()
        .expect("legacy plugin report path is UTF-8")
        .to_string();
    let db_path = temp_dir.path().join("jarvis-precedence-evidence.sqlite");
    let env = [
        ("JARVIS_EVIDENCE_LIVE_QA_REPORT", live_report.as_str()),
        ("JARVIS_QA_REPORT_PATH", legacy_live_report.as_str()),
        ("JARVIS_EVIDENCE_PLUGIN_QA_REPORT", plugin_report.as_str()),
        (
            "JARVIS_PLUGIN_QA_REPORT_PATH",
            legacy_plugin_report.as_str(),
        ),
    ];
    let mut server = JarvisServer::start_with_env(&db_path, &env);
    let endpoint = server.endpoint();
    let command = run_cli_json([
        "command",
        "status check",
        "--json",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let task_id = command["task"]["id"].as_str().expect("task id");
    let mut live_report_json: Value =
        serde_json::from_str(&fs::read_to_string(&live_report_path).expect("read live report"))
            .expect("decode live report");
    live_report_json["voice_command_observation"]["command_result_evidence_id"] =
        json!(format!("task:{task_id}"));
    write_json_report(&live_report_path, live_report_json);

    let evidence_status = run_cli_json([
        "release",
        "evidence-status",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let live_item = release_evidence_item(&evidence_status, "live_device_qa_report");
    let plugin_item = release_evidence_item(&evidence_status, "plugin_trust_qa_report");
    assert_eq!(live_item["path"], live_report, "{live_item}");
    assert_eq!(plugin_item["path"], plugin_report, "{plugin_item}");
    assert_ne!(live_item["path"], legacy_live_report, "{live_item}");
    assert_ne!(plugin_item["path"], legacy_plugin_report, "{plugin_item}");
    server.stop();
}

#[test]
#[cfg(unix)]
fn release_evidence_status_marks_present_artifacts_as_presence_only() {
    let temp_dir = tempfile::tempdir().expect("temp evidence artifacts");
    let dist_dir = write_placeholder_distribution(temp_dir.path());
    let endpoint = format!("http://{}", unused_loopback_addr());
    let dist_dir_env = dist_dir.to_str().expect("dist dir utf8");

    let evidence_status = run_cli_json_with_env(
        [
            "release",
            "evidence-status",
            "--endpoint",
            endpoint.as_str(),
        ],
        &[("JARVIS_EVIDENCE_DIST_DIR", dist_dir_env)],
    );
    let items = evidence_status["items"].as_array().expect("evidence items");

    let app_bundle_item = items
        .iter()
        .find(|item| item["key"] == "signed_app_bundle")
        .expect("missing app bundle evidence item");
    assert_eq!(app_bundle_item["status"], "present", "{app_bundle_item}");
    let app_bundle_detail = app_bundle_item["detail"].as_str().expect("detail string");
    assert!(
        app_bundle_detail.contains("Info.plist bundle identifier"),
        "{app_bundle_detail}"
    );
    assert!(
        app_bundle_detail.contains("privacy prompt copy"),
        "{app_bundle_detail}"
    );
    assert!(
        app_bundle_detail.contains("not validated by evidence-status"),
        "{app_bundle_detail}"
    );

    let bundled_core_item = items
        .iter()
        .find(|item| item["key"] == "bundled_core_executable")
        .expect("missing bundled core evidence item");
    assert_eq!(
        bundled_core_item["status"], "present",
        "{bundled_core_item}"
    );
    let bundled_core_detail = bundled_core_item["detail"].as_str().expect("detail string");
    assert!(
        bundled_core_detail.contains("version marker matches expected release version"),
        "{bundled_core_detail}"
    );
    assert!(
        bundled_core_detail.contains("not validated by evidence-status"),
        "{bundled_core_detail}"
    );

    for key in [
        "app_executable",
        "signed_app_zip",
        "signed_installer_package",
    ] {
        let item = items
            .iter()
            .find(|item| item["key"] == key)
            .unwrap_or_else(|| panic!("missing evidence item {key}"));
        assert_eq!(item["status"], "present", "{item}");
        let detail = item["detail"].as_str().expect("detail string");
        assert!(detail.contains("presence only"), "{detail}");
        assert!(
            detail.contains("not validated by evidence-status"),
            "{detail}"
        );
    }

    let readable_status = run_cli_text_with_env(
        [
            "release",
            "evidence-status",
            "--endpoint",
            endpoint.as_str(),
        ],
        &[("JARVIS_EVIDENCE_DIST_DIR", dist_dir_env)],
    );
    assert!(readable_status.contains("signed_app_bundle: present"));
    assert!(readable_status.contains(&format!("path: {dist_dir_env}/Jarvis.app")));
    assert!(readable_status.contains("  detail: "));
    assert!(readable_status.contains("Info.plist bundle identifier"));
    assert!(readable_status.contains("bundled_core_executable: present"));
    assert!(readable_status.contains(&format!(
        "path: {dist_dir_env}/Jarvis.app/Contents/Resources/bin/jarvis-cli"
    )));
    assert!(readable_status.contains("version marker matches expected release version"));
    assert!(readable_status.contains("app_executable: present; presence-only caveat"));
    assert!(readable_status.contains("signed_app_zip: present; presence-only caveat"));
    assert!(readable_status.contains(&format!("path: {dist_dir_env}/Jarvis-0.1.4.zip")));
    assert!(readable_status.contains("signed_installer_package: present; presence-only caveat"));
    assert!(readable_status.contains("presence only"));

    assert!(evidence_status["proof_boundary"]
        .as_str()
        .expect("proof boundary")
        .contains("does not sign"));
    assert!(evidence_status["proof_boundary"]
        .as_str()
        .expect("proof boundary")
        .contains("owner-asserted review-source semantics"));
}

#[test]
#[cfg(unix)]
fn release_evidence_status_server_marks_present_artifacts_as_presence_only() {
    let temp_dir = tempfile::tempdir().expect("temp evidence artifacts");
    let dist_dir = write_placeholder_distribution(temp_dir.path());
    let db_path = temp_dir.path().join("jarvis-evidence-status.sqlite");
    let dist_dir_value = dist_dir.to_str().expect("dist dir utf8").to_string();
    let mut server = JarvisServer::start_with_env(
        &db_path,
        &[("JARVIS_EVIDENCE_DIST_DIR", dist_dir_value.as_str())],
    );

    let evidence_status = run_cli_json([
        "release",
        "evidence-status",
        "--endpoint",
        server.endpoint().as_str(),
    ]);
    let items = evidence_status["items"].as_array().expect("evidence items");

    let app_bundle_item = items
        .iter()
        .find(|item| item["key"] == "signed_app_bundle")
        .expect("missing app bundle evidence item");
    assert_eq!(app_bundle_item["status"], "present", "{app_bundle_item}");
    let app_bundle_detail = app_bundle_item["detail"].as_str().expect("detail string");
    assert!(
        app_bundle_detail.contains("Info.plist bundle identifier"),
        "{app_bundle_detail}"
    );
    assert!(
        app_bundle_detail.contains("privacy prompt copy"),
        "{app_bundle_detail}"
    );
    assert!(
        app_bundle_detail.contains("not validated by evidence-status"),
        "{app_bundle_detail}"
    );

    let bundled_core_item = items
        .iter()
        .find(|item| item["key"] == "bundled_core_executable")
        .expect("missing bundled core evidence item");
    assert_eq!(
        bundled_core_item["status"], "present",
        "{bundled_core_item}"
    );
    let bundled_core_detail = bundled_core_item["detail"].as_str().expect("detail string");
    assert!(
        bundled_core_detail.contains("version marker matches expected release version"),
        "{bundled_core_detail}"
    );
    assert!(
        bundled_core_detail.contains("not validated by evidence-status"),
        "{bundled_core_detail}"
    );

    for key in [
        "app_executable",
        "signed_app_zip",
        "signed_installer_package",
    ] {
        let item = items
            .iter()
            .find(|item| item["key"] == key)
            .unwrap_or_else(|| panic!("missing evidence item {key}"));
        assert_eq!(item["status"], "present", "{item}");
        let detail = item["detail"].as_str().expect("detail string");
        assert!(detail.contains("presence only"), "{detail}");
        assert!(
            detail.contains("not validated by evidence-status"),
            "{detail}"
        );
    }

    assert!(evidence_status["proof_boundary"]
        .as_str()
        .expect("proof boundary")
        .contains("does not sign"));

    let readable_status = run_cli_text([
        "release",
        "evidence-status",
        "--endpoint",
        server.endpoint().as_str(),
    ]);
    assert!(readable_status.contains("signed_app_bundle: present"));
    assert!(readable_status.contains("app_executable: present; presence-only caveat"));
    assert!(readable_status.contains("signed_app_zip: present; presence-only caveat"));
    assert!(readable_status.contains("signed_installer_package: present; presence-only caveat"));
    assert!(readable_status.contains("  detail: "));
    assert!(readable_status.contains("not validated by evidence-status"));
    server.stop();
}

#[test]
#[cfg(unix)]
fn release_evidence_status_rejects_stale_app_bundle_metadata() {
    let temp_dir = tempfile::tempdir().expect("temp evidence artifacts");
    let dist_dir = write_placeholder_distribution(temp_dir.path());
    fs::write(
        dist_dir.join("Jarvis.app/Contents/Info.plist"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key>
  <string>com.example.StaleJarvis</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.4</string>
  <key>CFBundleVersion</key>
  <string>0.1.4</string>
  <key>NSMicrophoneUsageDescription</key>
  <string>Jarvis uses microphone input only when you explicitly start local voice capture.</string>
  <key>NSSpeechRecognitionUsageDescription</key>
  <string>Jarvis uses speech recognition only to turn your spoken command into a local assistant request.</string>
</dict>
</plist>
"#,
    )
    .expect("write stale Info.plist");

    let endpoint = format!("http://{}", unused_loopback_addr());
    let dist_dir_env = dist_dir.to_str().expect("dist dir utf8");
    let evidence_status = run_cli_json_with_env(
        [
            "release",
            "evidence-status",
            "--endpoint",
            endpoint.as_str(),
        ],
        &[("JARVIS_EVIDENCE_DIST_DIR", dist_dir_env)],
    );
    assert_eq!(evidence_status["complete"], false);
    assert_eq!(evidence_status["invalid_count"], 1);
    let app_bundle = evidence_status["items"]
        .as_array()
        .expect("evidence items")
        .iter()
        .find(|item| item["key"] == "signed_app_bundle")
        .expect("app bundle item");
    assert_eq!(app_bundle["status"], "invalid");
    assert!(app_bundle["detail"]
        .as_str()
        .expect("detail")
        .contains("CFBundleIdentifier mismatch"));

    fs::write(
        dist_dir.join("Jarvis.app/Contents/Info.plist"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key>
  <string>com.nobiletechnology.jarvis</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.4</string>
  <key>CFBundleVersion</key>
  <string>0.1.4</string>
  <key>NSMicrophoneUsageDescription</key>
  <string>Jarvis microphone fixture</string>
  <key>NSSpeechRecognitionUsageDescription</key>
  <string>Jarvis uses speech recognition only to turn your spoken command into a local assistant request.</string>
</dict>
</plist>
"#,
    )
    .expect("write stale privacy Info.plist");

    let evidence_status = run_cli_json_with_env(
        [
            "release",
            "evidence-status",
            "--endpoint",
            endpoint.as_str(),
        ],
        &[("JARVIS_EVIDENCE_DIST_DIR", dist_dir_env)],
    );
    assert_eq!(evidence_status["complete"], false);
    assert_eq!(evidence_status["invalid_count"], 1);
    let app_bundle = evidence_status["items"]
        .as_array()
        .expect("evidence items")
        .iter()
        .find(|item| item["key"] == "signed_app_bundle")
        .expect("app bundle item");
    assert_eq!(app_bundle["status"], "invalid");
    assert!(app_bundle["detail"]
        .as_str()
        .expect("detail")
        .contains("NSMicrophoneUsageDescription mismatch"));

    fs::write(
        dist_dir.join("Jarvis.app/Contents/Info.plist"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key>
  <string>com.nobiletechnology.jarvis</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.4</string>
  <key>CFBundleVersion</key>
  <string>0.1.4</string>
  <key>NSMicrophoneUsageDescription</key>
  <string>Jarvis uses microphone input only when you explicitly start local voice capture.</string>
  <key>NSSpeechRecognitionUsageDescription</key>
  <string>Jarvis speech fixture</string>
</dict>
</plist>
"#,
    )
    .expect("write stale Speech privacy Info.plist");

    let evidence_status = run_cli_json_with_env(
        [
            "release",
            "evidence-status",
            "--endpoint",
            endpoint.as_str(),
        ],
        &[("JARVIS_EVIDENCE_DIST_DIR", dist_dir_env)],
    );
    assert_eq!(evidence_status["complete"], false);
    assert_eq!(evidence_status["invalid_count"], 1);
    let app_bundle = evidence_status["items"]
        .as_array()
        .expect("evidence items")
        .iter()
        .find(|item| item["key"] == "signed_app_bundle")
        .expect("app bundle item");
    assert_eq!(app_bundle["status"], "invalid");
    assert!(app_bundle["detail"]
        .as_str()
        .expect("detail")
        .contains("NSSpeechRecognitionUsageDescription mismatch"));
}

#[test]
#[cfg(unix)]
fn release_evidence_status_rejects_stale_bundled_core_version_marker() {
    let temp_dir = tempfile::tempdir().expect("temp evidence artifacts");
    let dist_dir = write_placeholder_distribution(temp_dir.path());
    fs::write(
        dist_dir.join("Jarvis.app/Contents/Resources/bin/jarvis-cli.version"),
        "jarvis 0.0.0\n",
    )
    .expect("write stale bundled core version marker");
    let endpoint = format!("http://{}", unused_loopback_addr());
    let dist_dir = dist_dir.to_str().expect("dist dir utf8");

    let evidence_status = run_cli_json_with_env(
        [
            "release",
            "evidence-status",
            "--endpoint",
            endpoint.as_str(),
        ],
        &[("JARVIS_EVIDENCE_DIST_DIR", dist_dir)],
    );
    let items = evidence_status["items"].as_array().expect("evidence items");
    let bundled_core_item = items
        .iter()
        .find(|item| item["key"] == "bundled_core_executable")
        .expect("missing bundled core evidence item");
    assert_eq!(
        bundled_core_item["status"], "invalid",
        "{bundled_core_item}"
    );
    let bundled_core_detail = bundled_core_item["detail"].as_str().expect("detail");
    assert!(bundled_core_detail.contains("version marker mismatch"));
    assert!(bundled_core_detail.contains("package-distribution.sh --unsigned-launch-check"));
}

#[test]
fn release_help_documents_operator_boundaries() {
    let version = run_cli_text(["--version"]);
    assert!(version.contains("jarvis "));
    assert!(version.contains(env!("CARGO_PKG_VERSION")));

    let release_help = run_cli_text(["release", "--help"]);
    assert!(release_help.contains("Read-only"));
    assert!(release_help.contains("IPC"));
    assert!(release_help.contains("conservative local"));

    let readiness_help = run_cli_text(["release", "readiness", "--help"]);
    assert!(readiness_help.contains("JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external"));
    assert!(readiness_help.contains("production_ready"));
    assert!(readiness_help.contains("notarization"));
    assert!(readiness_help.contains("Falls back to local read-only readiness metadata"));

    let evidence_help = run_cli_text(["release", "evidence-status", "--help"]);
    assert!(evidence_help.contains("file/report inventory plus semantic report validation"));
    assert!(evidence_help.contains("owner-asserted plugin-trust review source"));
    assert!(evidence_help.contains("host-egress evidence fields"));
    assert!(evidence_help.contains("Default output is operator-readable"));
    assert!(evidence_help.contains("per-item paths/details"));
    assert!(evidence_help.contains("same-line presence-only caveats"));
    assert!(evidence_help.contains("use --json for the exact structured payload"));
    assert!(evidence_help.contains("does not prove Developer ID signing"));
    assert!(evidence_help.contains("live-device QA"));
    assert!(evidence_help.contains("marketplace review"));
    assert!(evidence_help.contains("Falls back to local read-only evidence inspection"));

    let live_runbook_help = run_cli_text(["release", "live-device-runbook", "--help"]);
    assert!(live_runbook_help.contains("live-device QA runbook"));
    assert!(live_runbook_help.contains("live_voice_loop"));
    assert!(live_runbook_help.contains("does not perform live microphone"));
    assert!(live_runbook_help
        .contains("Falls back to local read-only readiness and evidence inspection"));

    let signed_distribution_help =
        run_cli_text(["release", "signed-distribution-runbook", "--help"]);
    assert!(signed_distribution_help.contains("signed distribution runbook"));
    assert!(signed_distribution_help.contains("Developer ID signing"));
    assert!(signed_distribution_help.contains("does not perform signing"));
    assert!(signed_distribution_help
        .contains("Falls back to local read-only readiness and evidence inspection"));

    let plugin_trust_help = run_cli_text(["release", "plugin-trust-runbook", "--help"]);
    assert!(plugin_trust_help.contains("plugin-trust QA runbook"));
    assert!(plugin_trust_help.contains("marketplace review"));
    assert!(plugin_trust_help.contains("does not perform marketplace review"));
    assert!(plugin_trust_help
        .contains("Falls back to local read-only readiness and evidence inspection"));

    let evidence_bundle_help = run_cli_text(["release", "evidence-bundle-runbook", "--help"]);
    assert!(evidence_bundle_help.contains("final evidence-bundle runbook"));
    assert!(evidence_bundle_help.contains("final release evidence bundle"));
    assert!(evidence_bundle_help.contains("does not generate the bundle"));
    assert!(evidence_bundle_help
        .contains("Falls back to local read-only readiness and evidence inspection"));
}

#[test]
fn release_live_device_runbook_summarizes_next_operator_steps() {
    let endpoint = format!("http://{}", unused_loopback_addr());
    let readable_runbook = run_cli_text([
        "release",
        "live-device-runbook",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let json_runbook = run_cli_json([
        "release",
        "live-device-runbook",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let format_json_runbook: Value = serde_json::from_str(&run_cli_text([
        "release",
        "live-device-runbook",
        "--format",
        "json",
        "--endpoint",
        endpoint.as_str(),
    ]))
    .expect("live-device runbook --format json output");

    assert!(readable_runbook.contains("Jarvis live-device QA runbook:"));
    assert!(readable_runbook.contains("live_voice_loop: pending_manual_validation"));
    assert!(readable_runbook.contains("live_device_qa_report:"));
    assert!(readable_runbook.contains("./scripts/release-live-device-qa.sh --check"));
    assert!(readable_runbook.contains(
        "./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env"
    ));
    assert!(readable_runbook.contains(
        "set -a && source target/release-live-device-qa.env && set +a && ./scripts/release-live-device-qa.sh --assert-complete"
    ));
    assert!(readable_runbook.contains("Evidence detail: expected JSON report is missing"));
    assert!(readable_runbook.contains("Run on the release machine:"));
    assert!(readable_runbook.contains(
        "Set JARVIS_RELEASE_CORE_ENDPOINT='<release-core-endpoint>' in target/release-live-device-qa.env before collecting command evidence"
    ));
    assert!(readable_runbook.contains(
        "cargo run -p jarvis-cli -- command \"status check\" --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\" --json"
    ));
    assert!(readable_runbook.contains("JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID='task:<uuid>'"));
    assert!(readable_runbook.contains(
        "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release evidence-status --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\""
    ));
    assert!(readable_runbook.contains(
        "Start or restart the core with JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external"
    ));
    assert!(readable_runbook.contains(
        "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release readiness --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\""
    ));
    assert!(readable_runbook.contains("Verify microphone and Speech permission prompts"));
    assert!(readable_runbook.contains("Boundary: runbook and local evidence inspection only"));
    assert!(readable_runbook.contains("Raw JSON: rerun with --json"));

    assert_eq!(
        json_runbook["generated_from"],
        "release readiness plus evidence-status"
    );
    assert_eq!(
        format_json_runbook["generated_from"],
        "release readiness plus evidence-status"
    );
    assert_eq!(json_runbook["production_ready"], false);
    assert_eq!(format_json_runbook["production_ready"], false);
    assert_eq!(json_runbook["live_voice_feature"]["key"], "live_voice_loop");
    assert_eq!(
        format_json_runbook["live_voice_feature"]["key"],
        "live_voice_loop"
    );
    assert_eq!(
        json_runbook["live_voice_feature"]["status"],
        "pending_manual_validation"
    );
    assert_eq!(
        json_runbook["live_device_evidence"]["key"],
        "live_device_qa_report"
    );
    assert_eq!(json_runbook["live_device_evidence"]["status"], "missing");
    assert_eq!(json_runbook["live_device_evidence"]["kind"], "json_report");
    assert_eq!(
        json_runbook["live_device_evidence"]["path"],
        "target/release-live-device-qa-report.json"
    );
    assert_eq!(json_runbook["live_device_evidence"]["manual_gate"], true);
    assert_eq!(
        json_runbook["live_device_evidence"]["required_for_production"],
        true
    );
    assert_string_array_exact(
        &json_runbook["commands"],
        &[
            "./scripts/release-live-device-qa.sh --check",
            "./scripts/release-live-device-qa.sh --write-template target/release-live-device-qa.env",
            "Set JARVIS_RELEASE_CORE_ENDPOINT='<release-core-endpoint>' in target/release-live-device-qa.env before collecting command evidence",
            "cargo run -p jarvis-cli -- command \"status check\" --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\" --json",
            "Record the returned task ID as JARVIS_QA_COMMAND_RESULT_EVIDENCE_ID='task:<uuid>' or a task-associated audit ID as 'audit:<uuid>' in target/release-live-device-qa.env",
            "set -a && source target/release-live-device-qa.env && set +a && ./scripts/release-live-device-qa.sh --assert-complete",
            "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release evidence-status --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\"",
            "Start or restart the core with JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external",
            "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release readiness --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\"",
        ],
    );
    assert_string_array_contains_substring(
        &json_runbook["manual_checks"],
        "signed, notarized package into /Applications",
    );
    assert_string_array_contains_substring(
        &json_runbook["manual_checks"],
        "Finder or LaunchServices",
    );
    assert_string_array_contains_substring(
        &json_runbook["manual_checks"],
        "microphone and Speech permission prompts",
    );
    assert_string_array_contains_substring(
        &json_runbook["manual_checks"],
        "observed transcript reaches the command path",
    );
    assert_string_array_contains_substring(
        &json_runbook["manual_checks"],
        "live speech output, structured scheduler notification kind/title/body/thread evidence",
    );
    assert_string_array_contains_substring(
        &json_runbook["manual_checks"],
        "release-live-device-qa-report.json",
    );
    assert!(json_runbook["proof_boundary"]
        .as_str()
        .expect("proof boundary")
        .contains("does not perform live-device validation"));
}

#[test]
fn release_signed_distribution_runbook_summarizes_next_operator_steps() {
    let endpoint = format!("http://{}", unused_loopback_addr());
    let readable_runbook = run_cli_text([
        "release",
        "signed-distribution-runbook",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let json_runbook = run_cli_json([
        "release",
        "signed-distribution-runbook",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let format_json_runbook: Value = serde_json::from_str(&run_cli_text([
        "release",
        "signed-distribution-runbook",
        "--format",
        "json",
        "--endpoint",
        endpoint.as_str(),
    ]))
    .expect("signed-distribution runbook --format json output");

    assert!(readable_runbook.contains("Jarvis signed distribution runbook:"));
    assert!(readable_runbook.contains("signed_app_bundle:"));
    assert!(readable_runbook.contains("signed_app_zip:"));
    assert!(readable_runbook.contains("signed_installer_package:"));
    assert!(readable_runbook.contains("signed_distribution_provenance_report:"));
    assert!(readable_runbook.contains("./scripts/package-distribution.sh --check"));
    assert!(readable_runbook.contains("./scripts/package-distribution.sh --unsigned-launch-check"));
    assert!(readable_runbook.contains("JARVIS_DEVELOPER_ID_APPLICATION="));
    assert!(readable_runbook.contains("JARVIS_NOTARYTOOL_APPLE_ID="));
    assert!(readable_runbook.contains(
        "Set JARVIS_RELEASE_CORE_ENDPOINT='<release-core-endpoint>' before external evidence checks"
    ));
    assert!(readable_runbook.contains(
        "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release evidence-status --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\""
    ));
    assert!(readable_runbook.contains("./scripts/release-evidence-doctor.sh --check"));
    assert!(readable_runbook.contains("Boundary: runbook and local evidence inspection only"));
    assert!(readable_runbook.contains("Raw JSON: rerun with --json"));

    assert_eq!(
        json_runbook["generated_from"],
        "release readiness plus evidence-status"
    );
    assert_eq!(
        format_json_runbook["generated_from"],
        "release readiness plus evidence-status"
    );
    assert_eq!(json_runbook["production_ready"], false);
    assert_eq!(format_json_runbook["production_ready"], false);
    assert_eq!(
        format_json_runbook, json_runbook,
        "signed-distribution --json and --format json must stay equivalent"
    );
    let distribution_evidence = json_runbook["distribution_evidence"]
        .as_array()
        .expect("distribution evidence");
    let distribution_keys = distribution_evidence
        .iter()
        .map(|item| {
            item.get("key")
                .and_then(Value::as_str)
                .expect("distribution evidence key")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        distribution_keys,
        [
            "signed_app_bundle",
            "app_executable",
            "bundled_core_executable",
            "signed_app_zip",
            "signed_installer_package",
            "signed_distribution_provenance_report",
        ],
        "signed-distribution runbook must preserve the full evidence handoff"
    );
    assert!(distribution_evidence
        .iter()
        .any(|item| item.get("key").and_then(Value::as_str) == Some("signed_app_zip")));
    assert!(distribution_evidence.iter().any(|item| {
        item.get("key").and_then(Value::as_str) == Some("signed_distribution_provenance_report")
            && item.get("status").and_then(Value::as_str) == Some("missing")
            && item.get("path").and_then(Value::as_str)
                == Some("target/distribution/Jarvis-0.1.4-signed-provenance.json")
    }));
    assert!(format_json_runbook["distribution_evidence"]
        .as_array()
        .expect("format distribution evidence")
        .iter()
        .any(|item| item.get("key").and_then(Value::as_str) == Some("signed_app_zip")));
    assert_string_array_exact(
        &json_runbook["commands"],
        &[
            "./scripts/package-distribution.sh --check",
            "./scripts/package-distribution.sh --unsigned-launch-check",
            "JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' JARVIS_NOTARYTOOL_PROFILE='...' ./scripts/package-distribution.sh",
            "JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' JARVIS_NOTARYTOOL_APPLE_ID='apple-id@example.com' JARVIS_NOTARYTOOL_TEAM_ID='TEAMID1234' JARVIS_NOTARYTOOL_PASSWORD='app-specific-password' ./scripts/package-distribution.sh",
            "Set JARVIS_RELEASE_CORE_ENDPOINT='<release-core-endpoint>' before external evidence checks",
            "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release evidence-status --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\"",
            "./scripts/release-evidence-doctor.sh --check",
            "cargo run -p jarvis-cli -- release live-device-runbook",
        ],
    );
    assert_string_array_exact(
        &json_runbook["manual_checks"],
        &[
            "Configure Developer ID Application and Installer identities plus either a notarytool keychain profile or Apple ID/team/app-specific password credentials on the release Mac.",
            "Run the full package-distribution lane and preserve the signed zip, signed installer package, signed provenance report, and notarytool logs referenced by that report.",
            "Confirm the signed installer package metadata still targets the Jarvis package identifier, release version, and /Applications install location.",
            "Confirm the signed app zip and installer package are notarized and stapled before clean-profile installation.",
            "Rerun evidence-status and evidence-doctor so missing or invalid signed artifact paths are visible before final bundling.",
            "Continue with live-device QA, plugin-trust QA, final evidence bundle generation, and external evidence-mode readiness.",
        ],
    );
    assert_string_array_contains_substring(
        &json_runbook["commands"],
        "cargo run -p jarvis-cli -- release live-device-runbook",
    );
    assert!(json_runbook["proof_boundary"]
        .as_str()
        .expect("proof boundary")
        .contains("does not perform signing"));
}

#[test]
fn release_plugin_trust_runbook_summarizes_next_operator_steps() {
    let endpoint = format!("http://{}", unused_loopback_addr());
    let readable_runbook = run_cli_text([
        "release",
        "plugin-trust-runbook",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let json_runbook = run_cli_json([
        "release",
        "plugin-trust-runbook",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let format_json_runbook: Value = serde_json::from_str(&run_cli_text([
        "release",
        "plugin-trust-runbook",
        "--format",
        "json",
        "--endpoint",
        endpoint.as_str(),
    ]))
    .expect("plugin-trust runbook --format json output");

    assert!(readable_runbook.contains("Jarvis plugin-trust QA runbook:"));
    assert!(readable_runbook.contains("plugin_trust_qa_report:"));
    assert!(readable_runbook.contains("./scripts/release-plugin-trust-qa.sh --check"));
    assert!(readable_runbook.contains(
        "./scripts/release-plugin-trust-qa.sh --write-template target/release-plugin-trust-qa.env"
    ));
    assert!(readable_runbook.contains(
        "set -a && source target/release-plugin-trust-qa.env && set +a && ./scripts/release-plugin-trust-qa.sh --assert-complete"
    ));
    assert!(readable_runbook.contains("Evidence detail: expected JSON report is missing"));
    assert!(readable_runbook.contains("Run on the release machine:"));
    assert!(readable_runbook.contains(
        "Set JARVIS_RELEASE_CORE_ENDPOINT='<release-core-endpoint>' before external evidence checks"
    ));
    assert!(readable_runbook.contains(
        "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release evidence-status --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\""
    ));
    assert!(readable_runbook.contains("./scripts/release-evidence-bundle.sh --check"));
    assert!(readable_runbook.contains(
        "./scripts/release-evidence-bundle.sh --write-template target/release-evidence-bundle.env"
    ));
    assert!(readable_runbook.contains(
        "set -a && source target/release-evidence-bundle.env && set +a && ./scripts/release-evidence-bundle.sh --bundle"
    ));
    assert!(readable_runbook.contains("./scripts/release-evidence-doctor.sh --assert-complete"));
    assert!(readable_runbook.contains("Validate host-level egress enforcement"));
    assert!(readable_runbook.contains("archived artifact URIs and SHA-256 digests"));
    assert!(readable_runbook.contains("final release evidence bundle"));
    assert!(readable_runbook.contains("Boundary: runbook and local evidence inspection only"));
    assert!(readable_runbook.contains("host-level egress enforcement"));
    assert!(readable_runbook.contains("Raw JSON: rerun with --json"));

    assert_eq!(
        json_runbook["generated_from"],
        "release readiness plus evidence-status"
    );
    assert_eq!(
        format_json_runbook["generated_from"],
        "release readiness plus evidence-status"
    );
    assert_eq!(json_runbook["production_ready"], false);
    assert_eq!(format_json_runbook["production_ready"], false);
    assert_eq!(
        json_runbook["plugin_trust_evidence"]["key"],
        "plugin_trust_qa_report"
    );
    assert_eq!(
        format_json_runbook["plugin_trust_evidence"]["key"],
        "plugin_trust_qa_report"
    );
    assert_eq!(json_runbook["plugin_trust_evidence"]["status"], "missing");
    assert_eq!(json_runbook["plugin_trust_evidence"]["kind"], "json_report");
    assert_eq!(
        json_runbook["plugin_trust_evidence"]["path"],
        "target/release-plugin-trust-qa-report.json"
    );
    assert_eq!(json_runbook["plugin_trust_evidence"]["manual_gate"], true);
    assert_eq!(
        json_runbook["plugin_trust_evidence"]["required_for_production"],
        true
    );
    assert_string_array_exact(
        &json_runbook["commands"],
        &[
            "./scripts/release-plugin-trust-qa.sh --check",
            "./scripts/release-plugin-trust-qa.sh --write-template target/release-plugin-trust-qa.env",
            "set -a && source target/release-plugin-trust-qa.env && set +a && ./scripts/release-plugin-trust-qa.sh --assert-complete",
            "Set JARVIS_RELEASE_CORE_ENDPOINT='<release-core-endpoint>' before external evidence checks",
            "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release evidence-status --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\"",
            "./scripts/release-evidence-doctor.sh --check",
            "./scripts/release-evidence-bundle.sh --check",
            "./scripts/release-evidence-bundle.sh --write-template target/release-evidence-bundle.env",
            "set -a && source target/release-evidence-bundle.env && set +a && ./scripts/release-evidence-bundle.sh --bundle",
            "./scripts/release-evidence-doctor.sh --assert-complete",
        ],
    );
    assert_string_array_contains_substring(
        &json_runbook["manual_checks"],
        "marketplace review workflow",
    );
    assert_string_array_contains_substring(&json_runbook["manual_checks"], "malware scan");
    assert_string_array_contains_substring(
        &json_runbook["manual_checks"],
        "signed publisher policy",
    );
    assert_string_array_contains_substring(&json_runbook["manual_checks"], "macOS sandbox");
    assert_string_array_contains_substring(
        &json_runbook["manual_checks"],
        "host-level egress enforcement with deny and declared-host allow fixtures",
    );
    assert_string_array_contains_substring(
        &json_runbook["manual_checks"],
        "archived artifact URIs and SHA-256 digests",
    );
    assert_string_array_contains_substring(
        &json_runbook["manual_checks"],
        "release-plugin-trust-qa-report.json",
    );
    assert_string_array_contains_substring(
        &json_runbook["manual_checks"],
        "signed distribution, live-device QA, and plugin-trust QA evidence all exist",
    );
    assert!(json_runbook["proof_boundary"]
        .as_str()
        .expect("proof boundary")
        .contains("host-level egress enforcement"));
}

#[test]
fn release_evidence_bundle_runbook_summarizes_next_operator_steps() {
    let endpoint = format!("http://{}", unused_loopback_addr());
    let readable_runbook = run_cli_text([
        "release",
        "evidence-bundle-runbook",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let json_runbook = run_cli_json([
        "release",
        "evidence-bundle-runbook",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let format_json_runbook: Value = serde_json::from_str(&run_cli_text([
        "release",
        "evidence-bundle-runbook",
        "--format",
        "json",
        "--endpoint",
        endpoint.as_str(),
    ]))
    .expect("evidence-bundle runbook --format json output");

    assert!(readable_runbook.contains("Jarvis final evidence-bundle runbook:"));
    for key in [
        "signed_distribution_provenance_report",
        "live_device_qa_report",
        "plugin_trust_qa_report",
        "release_evidence_bundle",
    ] {
        assert!(readable_runbook.contains(key), "{readable_runbook}");
    }
    assert!(readable_runbook.contains("./scripts/release-evidence-bundle.sh --check"));
    assert!(readable_runbook.contains(
        "./scripts/release-evidence-bundle.sh --write-template target/release-evidence-bundle.env"
    ));
    assert!(readable_runbook.contains(
        "set -a && source target/release-evidence-bundle.env && set +a && ./scripts/release-evidence-bundle.sh --bundle"
    ));
    assert!(readable_runbook.contains("./scripts/release-evidence-doctor.sh --assert-complete"));
    assert!(readable_runbook.contains("durable reports archive URI"));
    assert!(readable_runbook.contains(
        "Start or restart the core with JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external"
    ));
    assert!(readable_runbook.contains("Boundary: runbook and local evidence inspection only"));
    assert!(readable_runbook.contains("no final bundle was generated"));
    assert!(readable_runbook.contains("Raw JSON: rerun with --json"));

    assert_eq!(
        json_runbook["generated_from"],
        "release readiness plus evidence-status"
    );
    assert_eq!(
        format_json_runbook["generated_from"],
        "release readiness plus evidence-status"
    );
    assert_eq!(json_runbook["production_ready"], false);
    assert_eq!(format_json_runbook["production_ready"], false);
    assert_eq!(
        format_json_runbook, json_runbook,
        "evidence-bundle --json and --format json must stay equivalent"
    );
    let child_keys = json_runbook["child_evidence"]
        .as_array()
        .expect("child evidence")
        .iter()
        .map(|item| item["key"].as_str().expect("child evidence key"))
        .collect::<Vec<_>>();
    assert_eq!(
        child_keys,
        [
            "signed_distribution_provenance_report",
            "live_device_qa_report",
            "plugin_trust_qa_report",
        ],
        "final bundle runbook must preserve child evidence order"
    );
    assert_eq!(
        json_runbook["final_bundle_evidence"]["key"],
        "release_evidence_bundle"
    );
    assert_eq!(json_runbook["final_bundle_evidence"]["status"], "missing");
    assert_eq!(
        json_runbook["final_bundle_evidence"]["path"],
        "target/release-evidence-bundle.json"
    );
    assert_string_array_exact(
        &json_runbook["commands"],
        &[
            "./scripts/release-evidence-bundle.sh --check",
            "./scripts/release-evidence-bundle.sh --write-template target/release-evidence-bundle.env",
            "set -a && source target/release-evidence-bundle.env && set +a && ./scripts/release-evidence-bundle.sh --bundle",
            "./scripts/release-evidence-doctor.sh --check",
            "./scripts/release-evidence-doctor.sh --assert-complete",
            "Set JARVIS_RELEASE_CORE_ENDPOINT='<release-core-endpoint>' before external evidence checks",
            "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release evidence-status --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\"",
            "Start or restart the core with JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external",
            "JARVIS_RELEASE_READINESS_EVIDENCE_MODE=external cargo run -p jarvis-cli -- release readiness --endpoint \"${JARVIS_RELEASE_CORE_ENDPOINT:?set JARVIS_RELEASE_CORE_ENDPOINT}\"",
        ],
    );
    assert_string_array_contains_substring(
        &json_runbook["manual_checks"],
        "signed-distribution, live-device QA, and plugin-trust QA reports exist",
    );
    assert_string_array_contains_substring(
        &json_runbook["manual_checks"],
        "durable reports archive URI",
    );
    assert_string_array_contains_substring(
        &json_runbook["manual_checks"],
        "release-evidence-doctor --assert-complete",
    );
    assert!(json_runbook["proof_boundary"]
        .as_str()
        .expect("proof boundary")
        .contains("does not generate the final bundle"));
}

#[test]
fn release_runbook_ipc_endpoints_emit_normalized_core_json() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let db_path = temp_dir.path().join("jarvis-runbook-e2e.sqlite");
    let mut server = JarvisServer::start(&db_path);
    let endpoint = server.endpoint();

    let expected = [
        (
            "/release/signed-distribution-runbook",
            "signed_distribution",
            vec![
                "signed_app_bundle",
                "app_executable",
                "bundled_core_executable",
                "signed_app_zip",
                "signed_installer_package",
                "signed_distribution_provenance_report",
            ],
        ),
        (
            "/release/live-device-runbook",
            "live_device",
            vec!["live_device_qa_report"],
        ),
        (
            "/release/plugin-trust-runbook",
            "plugin_trust",
            vec!["plugin_trust_qa_report"],
        ),
        (
            "/release/evidence-bundle-runbook",
            "evidence_bundle",
            vec![
                "signed_distribution_provenance_report",
                "live_device_qa_report",
                "plugin_trust_qa_report",
                "release_evidence_bundle",
            ],
        ),
    ];

    for (path, runbook, evidence_keys) in expected {
        let response = http_get_json(&endpoint, path);
        assert_eq!(
            response["generated_from"],
            "release readiness plus evidence-status"
        );
        assert_eq!(response["runbook"], runbook);
        assert_eq!(response["production_ready"], false);
        let actual_keys = response["evidence_items"]
            .as_array()
            .expect("runbook evidence items")
            .iter()
            .map(|item| item["key"].as_str().expect("evidence key"))
            .collect::<Vec<_>>();
        assert_eq!(actual_keys, evidence_keys, "{path}: {response}");
        assert!(
            !response["commands"]
                .as_array()
                .expect("commands")
                .is_empty(),
            "{path}: {response}"
        );
        assert!(response["proof_boundary"]
            .as_str()
            .expect("proof boundary")
            .contains("Runbook and local evidence inspection only"));
    }

    server.stop();
}

#[cfg(unix)]
#[test]
fn release_plugin_trust_qa_assertion_rejects_temporary_artifact_uri() {
    let temp_dir = tempfile::tempdir().expect("temp plugin trust QA");
    let report_path = temp_dir.path().join("release-plugin-trust-qa-report.json");
    let report_path_arg = report_path
        .to_str()
        .expect("report path is valid UTF-8")
        .to_string();
    let output = run_repo_script_failure_with_env(
        "scripts/release-plugin-trust-qa.sh",
        &["--assert-complete"],
        &[
            ("JARVIS_PLUGIN_QA_REPORT_PATH", report_path_arg.as_str()),
            (
                "JARVIS_PLUGIN_QA_REVIEW_SOURCE",
                "owner-asserted-manual-review",
            ),
            ("JARVIS_PLUGIN_QA_SELF_TEST_FIXTURE", "false"),
            ("JARVIS_PLUGIN_QA_MARKETPLACE_REVIEW_VALIDATED", "true"),
            ("JARVIS_PLUGIN_QA_MALWARE_SCAN_VALIDATED", "true"),
            ("JARVIS_PLUGIN_QA_OS_SANDBOX_VALIDATED", "true"),
            ("JARVIS_PLUGIN_QA_EGRESS_ENFORCEMENT_VALIDATED", "true"),
            ("JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_POLICY_VALIDATED", "true"),
            ("JARVIS_PLUGIN_QA_MANUAL_TRUST_REVIEW_VALIDATED", "true"),
            ("JARVIS_PLUGIN_QA_OWNER_NAME", "Release Operator"),
            ("JARVIS_PLUGIN_QA_REVIEW_STARTED_AT", "2026-05-22T16:10:00Z"),
            (
                "JARVIS_PLUGIN_QA_REVIEW_COMPLETED_AT",
                "2026-05-22T16:20:00Z",
            ),
            (
                "JARVIS_PLUGIN_QA_MARKETPLACE_EVIDENCE_NOTE",
                "Marketplace review evidence archived.",
            ),
            (
                "JARVIS_PLUGIN_QA_MARKETPLACE_ARTIFACT_URI",
                "file:///tmp/jarvis/plugin-trust/marketplace-review.json",
            ),
            (
                "JARVIS_PLUGIN_QA_MARKETPLACE_ARTIFACT_SHA256",
                "1111111111111111111111111111111111111111111111111111111111111111",
            ),
            (
                "JARVIS_PLUGIN_QA_MALWARE_SCAN_EVIDENCE_NOTE",
                "Malware scan evidence archived.",
            ),
            (
                "JARVIS_PLUGIN_QA_MALWARE_SCAN_ARTIFACT_URI",
                "archive://jarvis/plugin-trust/malware-scan.json",
            ),
            (
                "JARVIS_PLUGIN_QA_MALWARE_SCAN_ARTIFACT_SHA256",
                "2222222222222222222222222222222222222222222222222222222222222222",
            ),
            (
                "JARVIS_PLUGIN_QA_OS_SANDBOX_EVIDENCE_NOTE",
                "OS sandbox validation evidence archived.",
            ),
            (
                "JARVIS_PLUGIN_QA_OS_SANDBOX_ARTIFACT_URI",
                "archive://jarvis/plugin-trust/os-sandbox.json",
            ),
            (
                "JARVIS_PLUGIN_QA_OS_SANDBOX_ARTIFACT_SHA256",
                "3333333333333333333333333333333333333333333333333333333333333333",
            ),
            (
                "JARVIS_PLUGIN_QA_EGRESS_EVIDENCE_NOTE",
                "Host-level egress validation evidence archived.",
            ),
            (
                "JARVIS_PLUGIN_QA_EGRESS_ARTIFACT_URI",
                "archive://jarvis/plugin-trust/egress.json",
            ),
            (
                "JARVIS_PLUGIN_QA_EGRESS_ARTIFACT_SHA256",
                "4444444444444444444444444444444444444444444444444444444444444444",
            ),
            (
                "JARVIS_PLUGIN_QA_EGRESS_POLICY_LABEL",
                "Host egress policy/profile reviewed.",
            ),
            (
                "JARVIS_PLUGIN_QA_EGRESS_VALIDATION_COMPLETED_AT",
                "2026-05-22T16:18:00Z",
            ),
            (
                "JARVIS_PLUGIN_QA_EGRESS_DENY_FIXTURE_EVIDENCE_NOTE",
                "Undeclared-host deny evidence archived.",
            ),
            (
                "JARVIS_PLUGIN_QA_EGRESS_ALLOW_FIXTURE_EVIDENCE_NOTE",
                "Declared-host allow evidence archived.",
            ),
            (
                "JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_EVIDENCE_NOTE",
                "Signed publisher policy evidence archived.",
            ),
            (
                "JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_ARTIFACT_URI",
                "archive://jarvis/plugin-trust/signed-publisher.json",
            ),
            (
                "JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_ARTIFACT_SHA256",
                "5555555555555555555555555555555555555555555555555555555555555555",
            ),
            (
                "JARVIS_PLUGIN_QA_MANUAL_REVIEW_EVIDENCE_NOTE",
                "Manual plugin trust review evidence archived.",
            ),
            (
                "JARVIS_PLUGIN_QA_MANUAL_REVIEW_ARTIFACT_URI",
                "archive://jarvis/plugin-trust/manual-review.json",
            ),
            (
                "JARVIS_PLUGIN_QA_MANUAL_REVIEW_ARTIFACT_SHA256",
                "6666666666666666666666666666666666666666666666666666666666666666",
            ),
        ],
    );

    assert!(
        output.contains("JARVIS_PLUGIN_QA_MARKETPLACE_ARTIFACT_URI"),
        "{output}"
    );
    assert!(
        output.contains("durable release evidence archive"),
        "{output}"
    );
}

#[cfg(unix)]
#[test]
fn release_plugin_trust_qa_assertion_report_is_accepted_by_evidence_status() {
    let temp_dir = tempfile::tempdir().expect("temp complete release evidence");
    let fixture = write_complete_release_evidence_fixture(temp_dir.path());
    let report_path = Path::new(&fixture.plugin_report_path);
    let report_path_arg = fixture.plugin_report_path.clone();

    let script_output = run_repo_script_with_env(
        "scripts/release-plugin-trust-qa.sh",
        &["--assert-complete"],
        &[
            ("JARVIS_PLUGIN_QA_REPORT_PATH", report_path_arg.as_str()),
            (
                "JARVIS_PLUGIN_QA_REVIEW_SOURCE",
                "owner-asserted-manual-review",
            ),
            ("JARVIS_PLUGIN_QA_SELF_TEST_FIXTURE", "false"),
            ("JARVIS_PLUGIN_QA_MARKETPLACE_REVIEW_VALIDATED", "true"),
            ("JARVIS_PLUGIN_QA_MALWARE_SCAN_VALIDATED", "true"),
            ("JARVIS_PLUGIN_QA_OS_SANDBOX_VALIDATED", "true"),
            ("JARVIS_PLUGIN_QA_EGRESS_ENFORCEMENT_VALIDATED", "true"),
            ("JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_POLICY_VALIDATED", "true"),
            ("JARVIS_PLUGIN_QA_MANUAL_TRUST_REVIEW_VALIDATED", "true"),
            ("JARVIS_PLUGIN_QA_OWNER_NAME", "Release Operator"),
            ("JARVIS_PLUGIN_QA_REVIEW_STARTED_AT", "2026-05-22T16:10:00Z"),
            (
                "JARVIS_PLUGIN_QA_REVIEW_COMPLETED_AT",
                "2026-05-22T16:20:00Z",
            ),
            (
                "JARVIS_PLUGIN_QA_MARKETPLACE_EVIDENCE_NOTE",
                "Marketplace review evidence archived.",
            ),
            (
                "JARVIS_PLUGIN_QA_MARKETPLACE_ARTIFACT_URI",
                "archive://jarvis/plugin-trust/marketplace-review.json",
            ),
            (
                "JARVIS_PLUGIN_QA_MARKETPLACE_ARTIFACT_SHA256",
                "1111111111111111111111111111111111111111111111111111111111111111",
            ),
            (
                "JARVIS_PLUGIN_QA_MALWARE_SCAN_EVIDENCE_NOTE",
                "Malware scan evidence archived.",
            ),
            (
                "JARVIS_PLUGIN_QA_MALWARE_SCAN_ARTIFACT_URI",
                "archive://jarvis/plugin-trust/malware-scan.json",
            ),
            (
                "JARVIS_PLUGIN_QA_MALWARE_SCAN_ARTIFACT_SHA256",
                "2222222222222222222222222222222222222222222222222222222222222222",
            ),
            (
                "JARVIS_PLUGIN_QA_OS_SANDBOX_EVIDENCE_NOTE",
                "OS sandbox validation evidence archived.",
            ),
            (
                "JARVIS_PLUGIN_QA_OS_SANDBOX_ARTIFACT_URI",
                "archive://jarvis/plugin-trust/os-sandbox.json",
            ),
            (
                "JARVIS_PLUGIN_QA_OS_SANDBOX_ARTIFACT_SHA256",
                "3333333333333333333333333333333333333333333333333333333333333333",
            ),
            (
                "JARVIS_PLUGIN_QA_EGRESS_EVIDENCE_NOTE",
                "Host-level egress validation evidence archived.",
            ),
            (
                "JARVIS_PLUGIN_QA_EGRESS_ARTIFACT_URI",
                "archive://jarvis/plugin-trust/egress.json",
            ),
            (
                "JARVIS_PLUGIN_QA_EGRESS_ARTIFACT_SHA256",
                "4444444444444444444444444444444444444444444444444444444444444444",
            ),
            (
                "JARVIS_PLUGIN_QA_EGRESS_POLICY_LABEL",
                "Host egress policy/profile reviewed.",
            ),
            (
                "JARVIS_PLUGIN_QA_EGRESS_VALIDATION_COMPLETED_AT",
                "2026-05-22T16:18:00Z",
            ),
            (
                "JARVIS_PLUGIN_QA_EGRESS_DENY_FIXTURE_EVIDENCE_NOTE",
                "Undeclared-host deny evidence archived.",
            ),
            (
                "JARVIS_PLUGIN_QA_EGRESS_ALLOW_FIXTURE_EVIDENCE_NOTE",
                "Declared-host allow evidence archived.",
            ),
            (
                "JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_EVIDENCE_NOTE",
                "Signed publisher policy evidence archived.",
            ),
            (
                "JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_ARTIFACT_URI",
                "archive://jarvis/plugin-trust/signed-publisher.json",
            ),
            (
                "JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_ARTIFACT_SHA256",
                "5555555555555555555555555555555555555555555555555555555555555555",
            ),
            (
                "JARVIS_PLUGIN_QA_MANUAL_REVIEW_EVIDENCE_NOTE",
                "Manual plugin trust review evidence archived.",
            ),
            (
                "JARVIS_PLUGIN_QA_MANUAL_REVIEW_ARTIFACT_URI",
                "archive://jarvis/plugin-trust/manual-review.json",
            ),
            (
                "JARVIS_PLUGIN_QA_MANUAL_REVIEW_ARTIFACT_SHA256",
                "6666666666666666666666666666666666666666666666666666666666666666",
            ),
        ],
    );
    let script_stdout = String::from_utf8_lossy(&script_output.stdout);
    assert!(
        script_stdout.contains("Jarvis plugin trust QA assertion: complete"),
        "{script_stdout}"
    );
    assert!(
        script_stdout.contains(report_path_arg.as_str()),
        "{script_stdout}"
    );

    assert!(report_path.exists(), "plugin trust QA report was written");
    let report = read_json_file(report_path);
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["evidence_type"], "owner_recorded_plugin_trust_qa");
    assert_eq!(report["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(report["self_test_fixture"], false);
    assert_eq!(report["review_source"], "owner-asserted-manual-review");
    assert_eq!(
        report["evidence_artifacts"]["marketplace_review"]["uri"],
        "archive://jarvis/plugin-trust/marketplace-review.json"
    );
    assert_eq!(
        report["evidence_artifacts"]["egress_enforcement"]["sha256"],
        "4444444444444444444444444444444444444444444444444444444444444444"
    );

    let plugin_generated_at = report["generated_at"]
        .as_str()
        .expect("plugin report generated_at")
        .to_string();
    let bundle_path = Path::new(&fixture.bundle_path);
    let mut bundle_report = read_json_file(bundle_path);
    bundle_report["reports"]["plugin_trust_qa_sha256"] = json!(file_sha256(report_path));
    bundle_report["owner_recorded_release_evidence"]["completed_at"] =
        json!(plugin_generated_at.as_str());
    bundle_report["generated_at"] = json!(plugin_generated_at);
    write_json_report(bundle_path, bundle_report);

    let evidence_env = fixture.env_refs();
    let db_path = temp_dir.path().join("jarvis-plugin-script-evidence.sqlite");
    let mut server = JarvisServer::start_with_env(&db_path, &evidence_env);
    let endpoint = server.endpoint();
    let command = run_cli_json([
        "command",
        "status check",
        "--json",
        "--endpoint",
        endpoint.as_str(),
    ]);
    let task_id = command["task"]["id"].as_str().expect("task id");
    bind_complete_release_evidence_fixture_to_task(&fixture, task_id);

    let evidence_status = run_cli_json([
        "release",
        "evidence-status",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(evidence_status["complete"], true, "{evidence_status}");
    assert_eq!(evidence_status["missing_count"], 0, "{evidence_status}");
    assert_eq!(evidence_status["invalid_count"], 0, "{evidence_status}");
    assert_all_evidence_items_present(&evidence_status);
    let plugin_trust_item = release_evidence_item(&evidence_status, "plugin_trust_qa_report");
    assert_eq!(plugin_trust_item["status"], "present");
    assert!(plugin_trust_item["detail"]
        .as_str()
        .expect("plugin trust detail")
        .contains("egress validation timestamps"));
    let bundle_item = release_evidence_item(&evidence_status, "release_evidence_bundle");
    assert_eq!(bundle_item["status"], "present");
    assert!(bundle_item["detail"]
        .as_str()
        .expect("bundle detail")
        .contains("SHA-256"));
    server.stop();
}

#[cfg(unix)]
#[test]
fn release_external_handoff_snapshots_match_live_runbook_commands() {
    let temp_dir = tempfile::tempdir().expect("temp external handoff");
    let handoff_dir = temp_dir.path().join("handoff");
    let handoff_dir_arg = handoff_dir
        .to_str()
        .expect("handoff path is valid UTF-8")
        .to_string();
    let endpoint = format!("http://{}", unused_loopback_addr());
    let evidence_root = temp_dir.path().join("isolated-evidence");
    let evidence_dist = evidence_root.join("distribution");
    let evidence_live = evidence_root.join("live-device.json");
    let evidence_plugin = evidence_root.join("plugin-trust.json");
    let evidence_bundle = evidence_root.join("bundle.json");
    let evidence_dist_arg = evidence_dist.to_string_lossy().to_string();
    let evidence_live_arg = evidence_live.to_string_lossy().to_string();
    let evidence_plugin_arg = evidence_plugin.to_string_lossy().to_string();
    let evidence_bundle_arg = evidence_bundle.to_string_lossy().to_string();
    let evidence_env = [
        ("JARVIS_EVIDENCE_DIST_DIR", evidence_dist_arg.as_str()),
        ("JARVIS_EVIDENCE_LIVE_QA_REPORT", evidence_live_arg.as_str()),
        (
            "JARVIS_EVIDENCE_PLUGIN_QA_REPORT",
            evidence_plugin_arg.as_str(),
        ),
        ("JARVIS_EVIDENCE_OUTPUT_PATH", evidence_bundle_arg.as_str()),
    ];
    let mut external_evidence_env = evidence_env.to_vec();
    external_evidence_env.extend([
        ("JARVIS_RELEASE_HANDOFF_ENDPOINT", endpoint.as_str()),
        ("JARVIS_RELEASE_READINESS_EVIDENCE_MODE", "external"),
    ]);

    run_repo_script_with_env(
        "scripts/release-external-handoff.sh",
        &["--write", handoff_dir_arg.as_str()],
        &external_evidence_env,
    );

    let signed_direct = run_cli_json_with_env(
        [
            "release",
            "signed-distribution-runbook",
            "--endpoint",
            endpoint.as_str(),
        ],
        &evidence_env,
    );
    let live_direct = run_cli_json_with_env(
        [
            "release",
            "live-device-runbook",
            "--endpoint",
            endpoint.as_str(),
        ],
        &evidence_env,
    );
    let plugin_direct = run_cli_json_with_env(
        [
            "release",
            "plugin-trust-runbook",
            "--endpoint",
            endpoint.as_str(),
        ],
        &evidence_env,
    );
    let bundle_direct = run_cli_json_with_env(
        [
            "release",
            "evidence-bundle-runbook",
            "--endpoint",
            endpoint.as_str(),
        ],
        &evidence_env,
    );
    let readiness_direct = run_cli_json_with_env(
        ["release", "readiness", "--endpoint", endpoint.as_str()],
        &external_evidence_env,
    );
    let evidence_status_direct = run_cli_json_with_env(
        [
            "release",
            "evidence-status",
            "--endpoint",
            endpoint.as_str(),
        ],
        &external_evidence_env,
    );

    let signed_snapshot = read_json_file(handoff_dir.join("signed-distribution-runbook.json"));
    let live_snapshot = read_json_file(handoff_dir.join("live-device-runbook.json"));
    let plugin_snapshot = read_json_file(handoff_dir.join("plugin-trust-runbook.json"));
    let bundle_snapshot = read_json_file(handoff_dir.join("evidence-bundle-runbook.json"));
    let readiness_snapshot = read_json_file(handoff_dir.join("release-readiness.json"));
    let evidence_status_snapshot = read_json_file(handoff_dir.join("release-evidence-status.json"));
    let manifest = read_json_file(handoff_dir.join("release-handoff-manifest.json"));
    let evidence_bundle_template =
        fs::read_to_string(handoff_dir.join("release-evidence-bundle.env"))
            .expect("read release evidence bundle template");

    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(
        manifest["evidence_type"],
        "release_external_handoff_manifest"
    );
    assert_eq!(manifest["release_version"], env!("CARGO_PKG_VERSION"));
    assert!(
        manifest["proof_boundary"]
            .as_str()
            .expect("manifest proof boundary")
            .contains("does not prove signing"),
        "{manifest}"
    );
    let manifest_files = manifest["files"].as_array().expect("manifest files");
    for required in [
        "release-live-device-qa.env",
        "release-plugin-trust-qa.env",
        "release-evidence-bundle.env",
        "release-readiness.json",
        "release-evidence-status.json",
        "signed-distribution-runbook.json",
        "live-device-runbook.json",
        "plugin-trust-runbook.json",
        "evidence-bundle-runbook.json",
        "release-evidence-checklist.md",
        "README.md",
    ] {
        let entry = manifest_files
            .iter()
            .find(|entry| entry["path"] == required)
            .unwrap_or_else(|| panic!("manifest missing {required}: {manifest}"));
        let handoff_file = handoff_dir.join(required);
        let expected_sha = file_sha256(&handoff_file);
        assert_eq!(
            entry["sha256"], expected_sha,
            "digest mismatch for {required}"
        );
        assert_eq!(
            entry["bytes"].as_u64().expect("manifest byte count"),
            fs::metadata(&handoff_file)
                .expect("handoff file metadata")
                .len(),
            "byte count mismatch for {required}"
        );
    }

    assert_eq!(
        signed_snapshot["commands"], signed_direct["commands"],
        "signed-distribution handoff snapshot commands must match the live runbook"
    );
    assert_eq!(
        signed_snapshot["distribution_evidence"], signed_direct["distribution_evidence"],
        "signed-distribution handoff snapshot must preserve evidence rows"
    );
    assert_eq!(
        signed_snapshot["proof_boundary"], signed_direct["proof_boundary"],
        "signed-distribution handoff snapshot must preserve the proof boundary"
    );

    assert_eq!(
        live_snapshot["commands"], live_direct["commands"],
        "live-device handoff snapshot commands must match the live runbook"
    );
    assert_eq!(
        live_snapshot["live_device_evidence"], live_direct["live_device_evidence"],
        "live-device handoff snapshot must preserve evidence status"
    );
    assert_eq!(
        live_snapshot["live_voice_feature"], live_direct["live_voice_feature"],
        "live-device handoff snapshot must preserve pending voice feature state"
    );
    assert_eq!(
        live_snapshot["proof_boundary"], live_direct["proof_boundary"],
        "live-device handoff snapshot must preserve the proof boundary"
    );

    assert_eq!(
        plugin_snapshot["commands"], plugin_direct["commands"],
        "plugin-trust handoff snapshot commands must match the live runbook"
    );
    assert_eq!(
        plugin_snapshot["plugin_trust_evidence"], plugin_direct["plugin_trust_evidence"],
        "plugin-trust handoff snapshot must preserve evidence status"
    );
    assert_eq!(
        plugin_snapshot["proof_boundary"], plugin_direct["proof_boundary"],
        "plugin-trust handoff snapshot must preserve the proof boundary"
    );

    assert_eq!(
        bundle_snapshot["commands"], bundle_direct["commands"],
        "evidence-bundle handoff snapshot commands must match the live runbook"
    );
    assert_eq!(
        bundle_snapshot["child_evidence"], bundle_direct["child_evidence"],
        "evidence-bundle handoff snapshot must preserve child evidence status"
    );
    assert_eq!(
        bundle_snapshot["final_bundle_evidence"], bundle_direct["final_bundle_evidence"],
        "evidence-bundle handoff snapshot must preserve final bundle evidence status"
    );
    assert_eq!(
        bundle_snapshot["proof_boundary"], bundle_direct["proof_boundary"],
        "evidence-bundle handoff snapshot must preserve the proof boundary"
    );

    assert_eq!(
        readiness_snapshot["production_ready"], readiness_direct["production_ready"],
        "release handoff readiness snapshot must match live external-mode readiness"
    );
    assert_eq!(
        readiness_snapshot["evidence_mode_enabled"], readiness_direct["evidence_mode_enabled"],
        "release handoff readiness snapshot must preserve external evidence mode"
    );
    assert_eq!(
        readiness_snapshot["pending_feature_count"], readiness_direct["pending_feature_count"],
        "release handoff readiness snapshot must preserve pending feature count"
    );
    assert_eq!(
        readiness_snapshot["blocking_manual_gates"], readiness_direct["blocking_manual_gates"],
        "release handoff readiness snapshot must preserve manual release blockers"
    );
    assert_eq!(
        readiness_snapshot["readiness_scope"], readiness_direct["readiness_scope"],
        "release handoff readiness snapshot must be generated in external evidence mode"
    );
    assert!(readiness_snapshot["readiness_scope"]
        .as_str()
        .expect("readiness scope")
        .contains("external release evidence status"));
    assert_eq!(
        evidence_status_snapshot["complete"], evidence_status_direct["complete"],
        "release handoff evidence-status snapshot must match live completion state"
    );
    assert_eq!(
        evidence_status_snapshot["missing"], evidence_status_direct["missing"],
        "release handoff evidence-status snapshot must preserve missing evidence"
    );
    assert_eq!(
        evidence_status_snapshot["invalid"], evidence_status_direct["invalid"],
        "release handoff evidence-status snapshot must preserve invalid evidence"
    );
    assert_eq!(
        evidence_status_snapshot["items"], evidence_status_direct["items"],
        "release handoff evidence-status snapshot must preserve evidence item rows"
    );
    assert!(
        evidence_bundle_template.contains("JARVIS_EVIDENCE_OVERWRITE_OUTPUT=false"),
        "final bundle template must default overwrite protection off"
    );
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

    let contract = run_cli_json(["contract", "--json", "--endpoint", endpoint.as_str()]);
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
    assert_array_contains(&contract["features"], "key", "release_ci_gate");
    assert_array_contains(&contract["features"], "key", "release_evidence_bundle");
    let contract_release_evidence_status = contract["features"]
        .as_array()
        .expect("contract features")
        .iter()
        .find(|feature| feature["key"] == "release_evidence_status")
        .expect("release evidence status feature");
    let contract_release_evidence_status_proof = contract_release_evidence_status["proof"]
        .as_str()
        .expect("release evidence status proof");
    assert!(
        contract_release_evidence_status_proof
            .contains("repository-backed command-result evidence"),
        "{contract_release_evidence_status}"
    );
    assert!(
        contract_release_evidence_status_proof.contains("host-egress policy"),
        "{contract_release_evidence_status}"
    );
    assert!(
        contract_release_evidence_status_proof.contains("archive-URI validation"),
        "{contract_release_evidence_status}"
    );
    assert!(
        contract_release_evidence_status_proof.contains("child-report semantic revalidation"),
        "{contract_release_evidence_status}"
    );
    let contract_release_evidence_bundle = contract["features"]
        .as_array()
        .expect("contract features")
        .iter()
        .find(|feature| feature["key"] == "release_evidence_bundle")
        .expect("release evidence bundle feature");
    let contract_release_evidence_bundle_proof = contract_release_evidence_bundle["proof"]
        .as_str()
        .expect("release evidence bundle proof");
    assert!(
        contract_release_evidence_bundle_proof.contains("review source"),
        "{contract_release_evidence_bundle}"
    );
    assert!(
        contract_release_evidence_bundle_proof.contains("host-egress fields"),
        "{contract_release_evidence_bundle}"
    );
    assert!(
        contract_release_evidence_bundle_proof.contains("durable reports archive URI"),
        "{contract_release_evidence_bundle}"
    );
    assert!(
        contract_release_evidence_bundle_proof.contains("child reports are revalidated"),
        "{contract_release_evidence_bundle}"
    );
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
    assert!(contract["endpoints"]
        .as_array()
        .expect("contract endpoints")
        .iter()
        .any(|endpoint| endpoint["path"] == "/activity/summary" && endpoint["redacted"] == true));
    assert!(contract["endpoints"]
        .as_array()
        .expect("contract endpoints")
        .iter()
        .any(|endpoint| endpoint["path"] == "/activity/events" && endpoint["redacted"] == true));
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
    let served_activity_events_feature = release_readiness["implemented_features"]
        .as_array()
        .expect("implemented features")
        .iter()
        .find(|feature| feature["key"] == "activity_events")
        .expect("activity events feature");
    assert!(
        served_activity_events_feature["proof"]
            .as_str()
            .expect("activity events proof")
            .contains("model-output chunk metadata frames"),
        "{served_activity_events_feature}"
    );
    assert!(
        served_activity_events_feature["boundary"]
            .as_str()
            .expect("activity events boundary")
            .contains("content_redacted:true"),
        "{served_activity_events_feature}"
    );
    let served_installed_plugin_feature = release_readiness["implemented_features"]
        .as_array()
        .expect("implemented features")
        .iter()
        .find(|feature| feature["key"] == "installed_plugin_execution")
        .expect("installed plugin execution feature");
    assert!(
        served_installed_plugin_feature["boundary"]
            .as_str()
            .expect("installed plugin boundary")
            .contains("os_sandbox_enforced:false"),
        "{served_installed_plugin_feature}"
    );
    assert_array_contains(
        &release_readiness["implemented_features"],
        "key",
        "release_ci_gate",
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
        "./scripts/release-ci-workflow-smoke.sh",
    );
    assert_string_array_contains(
        &release_readiness["recommended_verification_commands"],
        "./scripts/release-operator-qa-smoke.sh",
    );
    assert_string_array_missing(
        &release_readiness["recommended_verification_commands"],
        "./scripts/packaged-app-release-smoke.sh",
    );
    assert_string_array_contains(
        &release_readiness["recommended_verification_commands"],
        "./scripts/package-distribution.sh --unsigned-launch-check",
    );
    assert_string_array_contains(
        &release_readiness["recommended_verification_commands"],
        "cargo run -p jarvis-cli -- release signed-distribution-runbook",
    );
    assert_string_array_contains(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_DEVELOPER_ID_APPLICATION='Developer ID Application: ...' JARVIS_DEVELOPER_ID_INSTALLER='Developer ID Installer: ...' JARVIS_NOTARYTOOL_APPLE_ID='apple-id@example.com' JARVIS_NOTARYTOOL_TEAM_ID='TEAMID1234' JARVIS_NOTARYTOOL_PASSWORD='app-specific-password' ./scripts/package-distribution.sh",
    );
    assert_string_array_contains(
        &release_readiness["recommended_verification_commands"],
        "./scripts/release-external-handoff.sh --write target/release-external-handoff",
    );
    assert_string_array_contains(
        &release_readiness["recommended_verification_commands"],
        "cargo run -p jarvis-cli -- release live-device-runbook",
    );
    assert_string_array_contains(
        &release_readiness["recommended_verification_commands"],
        "cargo run -p jarvis-cli -- release plugin-trust-runbook",
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
    assert_string_array_contains_substring(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_QA_NOTIFICATION_KIND=",
    );
    assert_string_array_contains_substring(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_QA_NOTIFICATION_TITLE=",
    );
    assert_string_array_contains_substring(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_QA_NOTIFICATION_BODY=",
    );
    assert_string_array_contains_substring(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_QA_NOTIFICATION_THREAD_IDENTIFIER=",
    );
    assert_string_array_contains(
        &release_readiness["recommended_verification_commands"],
        "./scripts/release-plugin-trust-qa.sh --check",
    );
    assert_string_array_contains(
        &release_readiness["recommended_verification_commands"],
        "./scripts/release-plugin-trust-qa.sh --write-template target/release-plugin-trust-qa.env",
    );
    assert_string_array_contains(
        &release_readiness["recommended_verification_commands"],
        "set -a && source target/release-plugin-trust-qa.env && set +a && ./scripts/release-plugin-trust-qa.sh --assert-complete",
    );
    assert_string_array_contains_substring(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_PLUGIN_QA_OWNER_NAME=",
    );
    assert_string_array_contains_substring(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_PLUGIN_QA_EGRESS_EVIDENCE_NOTE=",
    );
    assert_string_array_contains_substring(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_PLUGIN_QA_EGRESS_DENY_FIXTURE_EVIDENCE_NOTE=",
    );
    assert_string_array_contains_substring(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_PLUGIN_QA_EGRESS_ALLOW_FIXTURE_EVIDENCE_NOTE=",
    );
    assert_string_array_contains_substring(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_PLUGIN_QA_MARKETPLACE_ARTIFACT_URI=",
    );
    assert_string_array_contains_substring(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_PLUGIN_QA_MARKETPLACE_ARTIFACT_SHA256=",
    );
    assert_string_array_contains_substring(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_PLUGIN_QA_MALWARE_SCAN_ARTIFACT_URI=",
    );
    assert_string_array_contains_substring(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_PLUGIN_QA_MALWARE_SCAN_ARTIFACT_SHA256=",
    );
    assert_string_array_contains_substring(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_PLUGIN_QA_OS_SANDBOX_ARTIFACT_URI=",
    );
    assert_string_array_contains_substring(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_PLUGIN_QA_OS_SANDBOX_ARTIFACT_SHA256=",
    );
    assert_string_array_contains_substring(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_PLUGIN_QA_EGRESS_ARTIFACT_URI=",
    );
    assert_string_array_contains_substring(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_PLUGIN_QA_EGRESS_ARTIFACT_SHA256=",
    );
    assert_string_array_contains_substring(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_ARTIFACT_URI=",
    );
    assert_string_array_contains_substring(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_PLUGIN_QA_SIGNED_PUBLISHER_ARTIFACT_SHA256=",
    );
    assert_string_array_contains_substring(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_PLUGIN_QA_MANUAL_REVIEW_ARTIFACT_URI=",
    );
    assert_string_array_contains_substring(
        &release_readiness["recommended_verification_commands"],
        "JARVIS_PLUGIN_QA_MANUAL_REVIEW_ARTIFACT_SHA256=",
    );
    assert_string_array_contains(
        &release_readiness["recommended_verification_commands"],
        "./scripts/release-evidence-bundle.sh --check",
    );
    assert_string_array_contains(
        &release_readiness["recommended_verification_commands"],
        "./scripts/release-evidence-bundle.sh --write-template target/release-evidence-bundle.env",
    );
    assert_string_array_contains(
        &release_readiness["recommended_verification_commands"],
        "set -a && source target/release-evidence-bundle.env && set +a && ./scripts/release-evidence-bundle.sh --bundle",
    );
    assert_string_array_contains(
        &release_readiness["recommended_verification_commands"],
        "./scripts/release-evidence-doctor.sh --check",
    );
    assert_string_array_contains(
        &release_readiness["recommended_verification_commands"],
        "./scripts/release-evidence-doctor.sh --assert-complete",
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
    assert!(evidence_status["proof_boundary"]
        .as_str()
        .expect("evidence proof boundary")
        .contains("owner-asserted review-source semantics"));
    assert!(evidence_status["proof_boundary"]
        .as_str()
        .expect("evidence proof boundary")
        .contains("archive-URI"));

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

    let readable_routes = run_cli_text(["routes", "list", "--endpoint", endpoint.as_str()]);
    assert!(readable_routes.contains("Jarvis model routes:"));
    assert!(readable_routes.contains(route_id.as_str()));
    assert!(readable_routes.contains("Raw JSON: rerun with --json"));
    assert!(
        serde_json::from_str::<Value>(&readable_routes).is_err(),
        "default routes list output should be operator-readable text"
    );
    let readable_route = run_cli_text([
        "routes",
        "get",
        route_id.as_str(),
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert!(readable_route.contains("Jarvis model route:"));
    assert!(readable_route.contains("Model context: redacted"));
    assert!(
        serde_json::from_str::<Value>(&readable_route).is_err(),
        "default route get output should be operator-readable text"
    );
    let route_json = run_cli_json([
        "routes",
        "get",
        route_id.as_str(),
        "--json",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(route_json["id"], route_id);

    let manifests = run_cli_json(["plugins", "list", "--json", "--endpoint", endpoint.as_str()]);
    let manifests_available_alias = run_cli_json([
        "plugins",
        "available",
        "--json",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_array_contains(&manifests_available_alias, "id", "fake_echo");
    assert_array_contains(&manifests_available_alias, "id", "fake_status");
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

    let readable_tools = run_cli_text(["tools", "list", "--endpoint", endpoint.as_str()]);
    assert!(readable_tools.contains("Registered first-party model tools:"));
    assert!(readable_tools.contains("fake_echo.echo"));
    assert!(readable_tools.contains("fake_status.status"));
    assert!(readable_tools.contains("Raw JSON: rerun with --json"));

    let readable_plugins = run_cli_text(["plugins", "list", "--endpoint", endpoint.as_str()]);
    assert!(readable_plugins.contains("Registered first-party plugins:"));
    assert!(readable_plugins.contains("fake_echo"));
    assert!(readable_plugins.contains("fake_status"));
    assert!(readable_plugins.contains("Actions:"));
    assert!(readable_plugins.contains("jarvis tools list"));
    assert!(readable_plugins.contains("Raw JSON: rerun with --json"));

    let readable_ask = run_cli_text(["ask", "plugin status", "--endpoint", endpoint.as_str()]);
    assert!(readable_ask.contains("Jarvis command: completed"));
    assert!(readable_ask.contains("Accepted: true"));
    assert!(readable_ask.contains("Route: local / fake-local-model"));
    assert!(readable_ask.contains("Tools:"));
    assert!(readable_ask.contains("fake_status.status: completed"));
    assert!(readable_ask.contains("Raw JSON: rerun with --json"));
    assert!(
        serde_json::from_str::<Value>(&readable_ask).is_err(),
        "default command output should be operator-readable text"
    );

    let ask_json = run_cli_json([
        "ask",
        "plugin status",
        "--json",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(ask_json["accepted"], true, "{ask_json}");
    assert_eq!(ask_json["task"]["status"], "completed", "{ask_json}");
    assert_eq!(
        ask_json["plugin_results"][0]["metadata"]["plugin_id"],
        "fake_status"
    );
    assert_eq!(
        ask_json["plugin_results"][0]["metadata"]["action"],
        "status"
    );

    let readable_tasks = run_cli_text(["tasks", "list", "--endpoint", endpoint.as_str()]);
    assert!(readable_tasks.contains("Jarvis tasks:"));
    assert!(readable_tasks.contains(task_id.as_str()));
    assert!(readable_tasks.contains("Raw JSON: rerun with --json"));
    assert!(
        serde_json::from_str::<Value>(&readable_tasks).is_err(),
        "default tasks list output should be operator-readable text"
    );
    let readable_task = run_cli_text([
        "tasks",
        "get",
        task_id.as_str(),
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert!(readable_task.contains("Jarvis task:"));
    assert!(readable_task.contains("Input: omitted from human output"));
    assert!(!readable_task.contains("plugin echo cross-process e2e"));
    assert!(
        serde_json::from_str::<Value>(&readable_task).is_err(),
        "default tasks get output should be operator-readable text"
    );
    let readable_audit = run_cli_text(["tasks", "audit", "--endpoint", endpoint.as_str()]);
    assert!(readable_audit.contains("Jarvis audit entries:"));
    assert!(readable_audit.contains("plugin_completed"));
    assert!(
        serde_json::from_str::<Value>(&readable_audit).is_err(),
        "default tasks audit output should be operator-readable text"
    );
    let task_json = run_cli_json([
        "tasks",
        "get",
        task_id.as_str(),
        "--json",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(task_json["id"], task_id);

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
    assert!(
        activity["recent_tasks"]
            .as_array()
            .expect("activity recent tasks")
            .iter()
            .all(|task| task.get("user_input").is_none()),
        "{activity}"
    );
    assert_array_contains(
        &activity["recent_audit_entries"],
        "event_type",
        "plugin_completed",
    );
    let readable_activity = run_cli_text(["activity", "summary", "--endpoint", endpoint.as_str()]);
    assert!(readable_activity.contains("Jarvis activity summary:"));
    assert!(readable_activity.contains("Task statuses:"));
    assert!(readable_activity.contains("Recent tasks:"));
    assert!(readable_activity.contains("Recent audit:"));
    assert!(readable_activity.contains("Raw JSON: rerun with --json"));
    assert!(!readable_activity.contains("plugin echo cross-process e2e"));
    assert!(
        serde_json::from_str::<Value>(&readable_activity).is_err(),
        "default activity summary output should be operator-readable text"
    );
    let activity_json = run_cli_json([
        "activity",
        "summary",
        "--json",
        "--endpoint",
        endpoint.as_str(),
    ]);
    assert_eq!(activity_json["repository_backed"], true);
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
        activity_events.contains("event: activity_progress"),
        "{activity_events}"
    );
    assert!(
        activity_events.contains("\"kind\":\"model_step\""),
        "{activity_events}"
    );
    assert!(
        activity_events.contains("\"kind\":\"model_output\""),
        "{activity_events}"
    );
    assert!(
        activity_events.contains("\"content_redacted\":true"),
        "{activity_events}"
    );
    assert!(
        activity_events.contains("\"byte_count\""),
        "{activity_events}"
    );
    assert!(
        activity_events.contains("\"stage\":\"completed\""),
        "{activity_events}"
    );
    assert!(
        activity_events.contains("\"model\":\"fake-local-model\""),
        "{activity_events}"
    );
    assert!(
        activity_events.contains("\"task_count\""),
        "{activity_events}"
    );
    assert!(
        !activity_events.contains("\"user_input\""),
        "{activity_events}"
    );

    let fake_echo_manifest = run_cli_json([
        "plugins",
        "get",
        "fake_echo",
        "--json",
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
    let installed_plugins_encoded =
        serde_json::to_string(&installed_plugins).expect("installed plugins JSON");
    assert!(!installed_plugins_encoded.contains(plugin_dir.to_str().expect("plugin dir")));
    assert!(!installed_plugins_encoded.contains("\"source_path\":"));
    assert!(!installed_plugins_encoded.contains("\"manifest_path\":"));
    assert!(!installed_plugins_encoded.contains("\"manifest_sha256\":"));
    assert!(!installed_plugins_encoded.contains("\"source_tree_sha256\":"));
    assert!(!installed_plugins_encoded.contains("\"subprocess_command_path\":"));
    assert!(!installed_plugins_encoded.contains("\"subprocess_command_sha256\":"));
    assert_eq!(installed_plugins[0]["local_paths_redacted"], true);
    assert_eq!(installed_plugins[0]["provenance_hashes_redacted"], true);

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
    let installed_plugin_get_encoded =
        serde_json::to_string(&installed_plugin_get).expect("installed plugin JSON");
    assert!(!installed_plugin_get_encoded.contains(plugin_dir.to_str().expect("plugin dir")));
    assert!(!installed_plugin_get_encoded.contains("\"source_path\":"));
    assert!(!installed_plugin_get_encoded.contains("\"manifest_path\":"));
    assert!(!installed_plugin_get_encoded.contains("\"manifest_sha256\":"));
    assert!(!installed_plugin_get_encoded.contains("\"source_tree_sha256\":"));
    assert!(!installed_plugin_get_encoded.contains("\"subprocess_command_path\":"));
    assert!(!installed_plugin_get_encoded.contains("\"subprocess_command_sha256\":"));

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
    assert_eq!(subprocess_run["local_paths_redacted"], true);
    assert_eq!(subprocess_run["provenance_hashes_redacted"], true);
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
        false
    );
    assert_eq!(
        subprocess_run["audit_entry"]["payload"]["os_sandbox_enforced"],
        false
    );
    assert!(
        subprocess_run["audit_entry"]["payload"]["os_sandbox_boundary"]
            .as_str()
            .expect("sandbox boundary")
            .contains("does not enforce an OS sandbox")
    );
    assert_eq!(
        subprocess_run["audit_entry"]["payload"]["subprocess_started"],
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
    assert_redacts_installed_plugin_provenance(
        &subprocess_run_encoded,
        subprocess_plugin_dir
            .to_str()
            .expect("subprocess plugin dir"),
    );
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
    assert_eq!(changed_subprocess_run["local_paths_redacted"], true);
    assert_eq!(changed_subprocess_run["provenance_hashes_redacted"], true);
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
        false
    );
    assert_eq!(
        noisy_stdout_run["audit_entry"]["payload"]["os_sandbox_enforced"],
        false
    );
    assert_eq!(
        noisy_stdout_run["audit_entry"]["payload"]["subprocess_started"],
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
        false
    );
    assert_eq!(
        noisy_stderr_run["audit_entry"]["payload"]["os_sandbox_enforced"],
        false
    );
    assert_eq!(
        noisy_stderr_run["audit_entry"]["payload"]["subprocess_started"],
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
        network_subprocess_default_enable
            .contains("subprocess_stdio grant requires at least one non-network action"),
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
    assert_redacts_installed_plugin_provenance(
        &all_audit_encoded,
        subprocess_plugin_dir
            .to_str()
            .expect("subprocess plugin dir"),
    );
    assert!(!all_audit_encoded.contains("raw stderr secret"));
    assert!(!all_audit_encoded.contains("ignored"));

    let activity_summary = run_cli_json(["activity", "summary", "--endpoint", endpoint.as_str()]);
    let activity_summary_encoded =
        serde_json::to_string(&activity_summary).expect("activity summary JSON");
    assert_redacts_installed_plugin_provenance(
        &activity_summary_encoded,
        subprocess_plugin_dir
            .to_str()
            .expect("subprocess plugin dir"),
    );

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

    let missing_index = run_cli_json(["memory", "index-status", "--endpoint", endpoint.as_str()]);
    assert_eq!(missing_index["state"], "missing");
    assert_eq!(missing_index["missing_entry_count"], 1);
    let rebuilt_index = run_cli_json(["memory", "index-rebuild", "--endpoint", endpoint.as_str()]);
    assert_eq!(rebuilt_index["state"], "current");
    assert_eq!(rebuilt_index["current_entry_count"], 1);
    let rebuilt_encoded = serde_json::to_string(&rebuilt_index).unwrap();
    assert!(!rebuilt_encoded.contains("persisted through jarvis-cli serve"));
    assert!(!rebuilt_encoded.contains(&memory_id));
    assert!(!rebuilt_encoded.contains("content_sha256"));
    let index_audit = run_cli_json(["tasks", "audit", "--json", "--endpoint", endpoint.as_str()]);
    assert_array_contains(&index_audit, "event_type", "memory_index_rebuilt");
    let index_audit_encoded = serde_json::to_string(&index_audit).unwrap();
    assert!(!index_audit_encoded.contains("persisted through jarvis-cli serve"));
    assert!(!index_audit_encoded.contains("memory-index.json"));
    let index_path = db_path.with_file_name("jarvis-e2e.sqlite.memory-index.json");
    fs::write(&index_path, b"corrupt-index-artifact").expect("corrupt memory index fixture");
    let corrupt_index = run_cli_json(["memory", "index-status", "--endpoint", endpoint.as_str()]);
    assert_eq!(corrupt_index["state"], "corrupt");
    assert_eq!(
        run_cli_json(["memory", "index-rebuild", "--endpoint", endpoint.as_str(),])["state"],
        "current"
    );

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
    let stale_index = run_cli_json(["memory", "index-status", "--endpoint", endpoint.as_str()]);
    assert_eq!(stale_index["state"], "stale");
    assert_eq!(stale_index["stale_entry_count"], 1);

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

    let retention_plan =
        run_cli_json(["memory", "retention-plan", "--endpoint", endpoint.as_str()]);
    assert_eq!(retention_plan["status"], "operator_review_required");
    assert_eq!(retention_plan["automation_enabled"], false);
    assert_eq!(retention_plan["value_redaction_required"], true);
    assert_array_contains(
        &retention_plan["candidates"],
        "status",
        "deleted_sensitive_retained",
    );
    assert_array_contains(
        &retention_plan["candidates"],
        "recommended_action",
        "operator_purge_or_restore",
    );
    assert_array_contains(
        &retention_plan["candidates"],
        "memory_id",
        sensitive_memory_id.as_str(),
    );
    let retention_plan_encoded =
        serde_json::to_string(&retention_plan).expect("retention plan JSON");
    assert!(!retention_plan_encoded.contains("do not expose deleted sensitive memory"));

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
    assert_eq!(diagnostics["schema_version"], 11);
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
    let persisted_index = run_cli_json([
        "memory",
        "index-status",
        "--endpoint",
        restarted_endpoint.as_str(),
    ]);
    assert_eq!(persisted_index["state"], "stale");

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
    let deleted_projection = run_cli_json([
        "memory",
        "index-status",
        "--endpoint",
        restarted_endpoint.as_str(),
    ]);
    assert_eq!(deleted_projection["deleted_projection_count"], 1);
    let rebuilt_after_delete = run_cli_json([
        "memory",
        "index-rebuild",
        "--endpoint",
        restarted_endpoint.as_str(),
    ]);
    assert_eq!(rebuilt_after_delete["state"], "current");
    assert_eq!(rebuilt_after_delete["indexed_entry_count"], 0);

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
    let restored_index = run_cli_json([
        "memory",
        "index-status",
        "--endpoint",
        restarted_endpoint.as_str(),
    ]);
    assert_eq!(restored_index["state"], "stale");
    assert_eq!(restored_index["missing_entry_count"], 1);

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
    let transport_chunks = command["audit_entries"]
        .as_array()
        .expect("audit array")
        .iter()
        .filter(|entry| entry["event_type"] == "model_output_chunk")
        .collect::<Vec<_>>();
    assert!(!transport_chunks.is_empty());
    assert!(transport_chunks.iter().all(|entry| {
        entry["payload"]["provider_native"] == true && entry["payload"]["content_redacted"] == true
    }));
    let encoded = serde_json::to_string(&command).expect("command JSON");
    assert!(!encoded.contains("JARVIS_OLLAMA_BASE_URL"));
    assert!(!encoded.contains("partial-envelope-must-not-surface"));

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

#[test]
fn serve_recovers_from_hallucinated_tool_then_executes_registered_tool() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let db_path = temp_dir
        .path()
        .join("jarvis-provider-invalid-then-valid-tool-e2e.sqlite");
    let (ollama_base_url, server_thread) = start_ollama_invalid_then_valid_tool_server();
    let mut server = JarvisServer::start_with_env(
        &db_path,
        &[
            ("JARVIS_LOCAL_MODEL_PROVIDER", "ollama"),
            (
                "JARVIS_LOCAL_MODEL",
                "provider-invalid-then-valid-tool-test",
            ),
            ("JARVIS_OLLAMA_BASE_URL", ollama_base_url.as_str()),
        ],
    );
    let endpoint = server.endpoint();

    let command = run_cli_json([
        "command",
        "recover from a bad status tool and then use the registered one",
        "--endpoint",
        endpoint.as_str(),
    ]);

    assert_eq!(command["accepted"], true, "{command}");
    assert_eq!(command["task"]["status"], "completed", "{command}");
    assert_eq!(command["message"], "provider completed after valid status");
    assert_eq!(command["steps"][0]["tool_results"][0]["status"], "rejected");
    assert!(command["steps"][0]["tool_results"][0]["output"]["error"]
        .as_str()
        .expect("rejection error")
        .contains("plugin status is not registered"));
    assert_eq!(
        command["steps"][1]["tool_results"][0]["plugin_id"],
        "fake_status"
    );
    assert_eq!(
        command["steps"][1]["tool_results"][0]["status"],
        "completed"
    );
    assert_array_contains(
        &command["audit_entries"],
        "event_type",
        "tool_request_rejected",
    );
    assert_array_contains(
        &command["audit_entries"],
        "event_type",
        "tool_execution_result",
    );
    let rejection_count = command["audit_entries"]
        .as_array()
        .expect("audit entries")
        .iter()
        .filter(|entry| entry["event_type"] == "tool_request_rejected")
        .count();
    let execution_count = command["audit_entries"]
        .as_array()
        .expect("audit entries")
        .iter()
        .filter(|entry| entry["event_type"] == "tool_execution_result")
        .count();
    assert_eq!(rejection_count, 1);
    assert_eq!(execution_count, 1);

    server.stop();
    server_thread
        .join()
        .expect("ollama invalid-then-valid-tool stub thread");
    drop(temp_dir);
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

    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let db_path = temp_dir
        .path()
        .join("jarvis-provider-invalid-tool-readable-e2e.sqlite");
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
    let readable = run_cli_text([
        "command",
        "ask provider to inspect status with a bad tool",
        "--endpoint",
        endpoint.as_str(),
    ]);

    assert!(readable.contains("Jarvis command: completed"), "{readable}");
    assert!(readable.contains("Tools:"), "{readable}");
    assert!(
        readable.contains("Registered first-party model tools are: fake_echo.approval_echo, fake_echo.echo, fake_status.status"),
        "{readable}"
    );
    assert!(
        readable.contains("Latest audit: task_completed - command completed"),
        "{readable}"
    );
    assert!(
        readable.contains("Raw JSON: rerun with --json"),
        "{readable}"
    );

    server.stop();
    server_thread
        .join()
        .expect("ollama invalid-tool readable stub thread");
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
fn serve_reports_codex_cloud_provider_health_and_executes_selected_model() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let db_path = temp_dir.path().join("jarvis-codex-cloud-e2e.sqlite");
    let (chatgpt_base_url, server_thread) = start_chatgpt_native_tool_server();
    let mut server = JarvisServer::start_with_env(
        &db_path,
        &[
            ("JARVIS_LOCAL_MODEL_ENABLED", "false"),
            ("JARVIS_CHATGPT_ENABLED", "true"),
            ("JARVIS_CHATGPT_AUTH", "api_key"),
            ("JARVIS_CHATGPT_MODEL", "gpt-codex-e2e"),
            ("JARVIS_OPENAI_API_KEY", "test-openai-token"),
            ("JARVIS_OPENAI_BASE_URL", chatgpt_base_url.as_str()),
            ("JARVIS_CHATGPT_REQUIRES_APPROVAL", "true"),
        ],
    );
    let endpoint = server.endpoint();

    let health = http_get_json(endpoint.as_str(), "/health");
    assert_eq!(
        health["command_runtime"],
        "routed-codex-cloud-model+first-party-plugins"
    );
    assert_eq!(health["chatgpt_enabled"], true);
    assert_eq!(health["chatgpt_auth_mode"], "api_key");
    assert_eq!(health["chatgpt_model"], "gpt-codex-e2e");
    assert_eq!(health["chatgpt_requires_approval"], true);

    let command = run_cli_json([
        "command",
        "ask codex provider for status",
        "--sensitivity",
        "workspace",
        "--endpoint",
        endpoint.as_str(),
    ]);

    assert_eq!(command["accepted"], true, "{command}");
    assert_eq!(command["task"]["status"], "completed", "{command}");
    assert_eq!(command["route"]["provider"], "chat_gpt");
    assert_eq!(command["route"]["model"], "gpt-codex-e2e");
    assert_eq!(
        command["steps"][0]["tool_results"][0]["plugin_id"],
        "fake_status"
    );
    assert_eq!(command["message"], "native tool result observed");
    let encoded = serde_json::to_string(&command).expect("command JSON");
    assert!(!encoded.contains("test-openai-token"));
    assert!(!encoded.contains("JARVIS_OPENAI_API_KEY"));

    server.stop();
    server_thread
        .join()
        .expect("codex cloud provider stub thread");
    drop(temp_dir);
}

#[cfg(unix)]
#[test]
fn serve_executes_codex_account_through_constrained_cli_subprocess() {
    let temp_dir = tempfile::tempdir().expect("create temp dir");
    let db_path = temp_dir.path().join("jarvis-codex-account-e2e.sqlite");
    let codex_executable = temp_dir.path().join("codex-account-stub");
    fs::write(
        &codex_executable,
        r#"#!/bin/sh
out=""
saw_approval_policy=false
saw_ignore_config=false
saw_ignore_rules=false
saw_shell_disabled=false
saw_exec_disabled=false
saw_code_host_disabled=false
saw_apps_disabled=false
saw_browser_disabled=false
saw_computer_disabled=false
saw_strict_config=false
saw_web_search_disabled=false
while [ "$#" -gt 0 ]; do
  case "$1" in
    --ask-for-approval) exit 41 ;;
    --ignore-user-config) saw_ignore_config=true ;;
    --ignore-rules) saw_ignore_rules=true ;;
    --strict-config) saw_strict_config=true ;;
    --disable)
      shift
      case "$1" in
        shell_tool) saw_shell_disabled=true ;;
        unified_exec) saw_exec_disabled=true ;;
        code_mode_host) saw_code_host_disabled=true ;;
        apps) saw_apps_disabled=true ;;
        browser_use) saw_browser_disabled=true ;;
        computer_use) saw_computer_disabled=true ;;
      esac
      ;;
    -c)
      shift
      case "$1" in
        approval_policy=*never*) saw_approval_policy=true ;;
        web_search=*disabled*) saw_web_search_disabled=true ;;
      esac
      ;;
    --output-last-message)
      shift
      out="$1"
      ;;
  esac
  shift
done
[ "$saw_approval_policy" = true ] || exit 42
[ "$saw_ignore_config" = true ] || exit 43
[ "$saw_ignore_rules" = true ] || exit 44
[ "$saw_shell_disabled" = true ] || exit 49
[ "$saw_exec_disabled" = true ] || exit 50
[ "$saw_code_host_disabled" = true ] || exit 51
[ "$saw_apps_disabled" = true ] || exit 52
[ "$saw_browser_disabled" = true ] || exit 53
[ "$saw_computer_disabled" = true ] || exit 54
[ "$saw_strict_config" = true ] || exit 55
[ "$saw_web_search_disabled" = true ] || exit 56
[ -n "$out" ] || exit 45
[ -z "${JARVIS_OPENAI_API_KEY+x}" ] || exit 46
[ -z "${JARVIS_SECRET_LEAK_TEST+x}" ] || exit 47
prompt=$(cat)
case "$prompt" in *"Redacted task context:"*) ;; *) exit 48 ;; esac
printf 'codex account e2e ok' > "$out"
printf '{"type":"done"}\n'
"#,
    )
    .expect("write Codex account stub");
    make_executable(&codex_executable);
    let executable = codex_executable
        .to_str()
        .expect("Codex executable path is UTF-8");

    let mut server = JarvisServer::start_with_env(
        &db_path,
        &[
            ("JARVIS_LOCAL_MODEL_ENABLED", "false"),
            ("JARVIS_CHATGPT_ENABLED", "true"),
            ("JARVIS_CHATGPT_AUTH", "codex_account"),
            ("JARVIS_CHATGPT_MODEL", "gpt-codex-account-e2e"),
            ("JARVIS_CODEX_EXECUTABLE", executable),
            ("JARVIS_CHATGPT_TIMEOUT_MS", "10000"),
            ("JARVIS_CHATGPT_REQUIRES_APPROVAL", "true"),
            ("JARVIS_OPENAI_API_KEY", "must-not-reach-codex-child"),
            ("JARVIS_SECRET_LEAK_TEST", "must-not-reach-codex-child"),
        ],
    );
    let endpoint = server.endpoint();

    let health = http_get_json(endpoint.as_str(), "/health");
    assert_eq!(health["chatgpt_enabled"], true);
    assert_eq!(health["chatgpt_auth_mode"], "codex_account");
    assert_eq!(health["chatgpt_model"], "gpt-codex-account-e2e");

    let command = run_cli_json([
        "command",
        "answer through the logged-in Codex account",
        "--sensitivity",
        "workspace",
        "--endpoint",
        endpoint.as_str(),
    ]);

    assert_eq!(command["accepted"], true, "{command}");
    assert_eq!(command["task"]["status"], "completed", "{command}");
    assert_eq!(command["route"]["provider"], "chat_gpt");
    assert_eq!(command["route"]["model"], "gpt-codex-account-e2e");
    assert_eq!(command["message"], "codex account e2e ok");
    let encoded = serde_json::to_string(&command).expect("command JSON");
    assert!(!encoded.contains("must-not-reach-codex-child"));
    assert!(!encoded.contains(executable));

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

#[test]
fn trusted_wake_key_prepare_keeps_secrets_off_argv_and_bounds_stdin() {
    let help = run_cli_text(["system-wake", "key-prepare", "--help"]);
    assert!(help.contains("--document-stdin"));
    assert!(help.contains("maximum 8192 bytes"));
    assert!(help.contains("short-lived one-time grant_token secret"));
    assert!(help.contains("trusted device-only journal code"));
    assert!(help.contains("raw prepare response is not install input"));
    assert!(help.contains("Never place the prepare document, proof, or returned token in argv"));
    assert!(!help.contains("--proof-payload-b64"));
    assert!(!help.contains("--proof-signature-der-b64"));
    assert!(!help.contains("--grant-token"));
    assert!(!help.contains("--confirmation"));
    assert!(!help.contains("--new-public-key-x963-b64"));

    let missing_flag = run_cli_failure(["system-wake", "key-prepare"]);
    assert!(missing_flag.contains("--document-stdin"));

    let mut child = Command::new(jarvis_cli_bin())
        .args(["system-wake", "key-prepare", "--document-stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&vec![b'x'; 8_193])
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("at most 8192 bytes"));
}

#[test]
fn trusted_wake_cross_process_is_disabled_signed_idempotent_and_redacted() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("wake.sqlite");
    let signing_key = P256SigningKey::from_bytes((&[13_u8; 32]).into()).unwrap();
    let public_key = signing_key.verifying_key().to_encoded_point(false);
    let bootstrap = json!({
        "rule_id": "4a617276-6973-4000-8000-000000000010",
        "public_key_x963_b64": BASE64_STANDARD.encode(public_key.as_bytes()),
        "command": "trusted wake cross process local check",
        "allow_rotation": false,
    });
    let mut server = JarvisServer::start_with_trusted_wake_bootstrap(&db_path, &bootstrap);
    let endpoint = server.endpoint();
    let status: Value =
        serde_json::from_str(&request(&endpoint, "GET", "/system-wake/status", None).unwrap())
            .unwrap();
    assert_eq!(status["rule"]["enabled"], false);
    let generation = status["rule"]["generation"].as_u64().unwrap();
    let original_fingerprint = status["rule"]["key_fingerprint"].clone();
    let bypass_key = P256SigningKey::from_bytes((&[14_u8; 32]).into()).unwrap();
    let bypass = json!({
        "rule_id": "4a617276-6973-4000-8000-000000000010",
        "public_key_x963_b64": BASE64_STANDARD.encode(
            bypass_key.verifying_key().to_encoded_point(false).as_bytes()
        ),
        "command": "legacy bootstrap must not rotate",
        "allow_rotation": true,
    });
    let bypass_bind = unused_loopback_addr();
    let mut bypass_child = Command::new(jarvis_cli_bin())
        .args([
            "serve",
            "--bind",
            &bypass_bind.to_string(),
            "--db-path",
            db_path.to_str().unwrap(),
            "--trusted-wake-bootstrap-stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    bypass_child
        .stdin
        .take()
        .unwrap()
        .write_all(bypass.to_string().as_bytes())
        .unwrap();
    let bypass_output = bypass_child.wait_with_output().unwrap();
    assert!(!bypass_output.status.success());
    let unchanged: Value =
        serde_json::from_str(&request(&endpoint, "GET", "/system-wake/status", None).unwrap())
            .unwrap();
    assert_eq!(unchanged["rule"]["generation"], generation);
    assert_eq!(unchanged["rule"]["key_fingerprint"], original_fingerprint);
    let enable = json!({ "enabled": true, "expected_generation": generation });
    request(
        &endpoint,
        "POST",
        "/system-wake/rule",
        Some(&enable.to_string()),
    )
    .unwrap();
    let payload = json!({
        "schema_version": 1,
        "rule_id": "4a617276-6973-4000-8000-000000000010",
        "rule_generation": generation,
        "session_id": status["session_id"],
        "challenge": status["challenge"],
        "counter": 1,
        "occurred_at": chrono::Utc::now(),
        "nonce": "00000000-0000-4000-8000-000000000333",
    });
    let bytes = serde_json::to_vec(&payload).unwrap();
    let signature: P256Signature = signing_key.sign(&bytes);
    let envelope = json!({
        "payload_b64": BASE64_STANDARD.encode(&bytes),
        "signature_der_b64": BASE64_STANDARD.encode(signature.to_der().as_bytes()),
    });
    let first: Value = serde_json::from_str(
        &request(
            &endpoint,
            "POST",
            "/system-wake/events",
            Some(&envelope.to_string()),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(first["event"]["state"], "completed");
    assert_eq!(first["idempotent_retry"], false);
    let retry: Value = serde_json::from_str(
        &request(
            &endpoint,
            "POST",
            "/system-wake/events",
            Some(&envelope.to_string()),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(retry["event"]["id"], first["event"]["id"]);
    assert_eq!(retry["idempotent_retry"], true);
    assert!(retry["execution"].is_null());
    let tasks: Value =
        serde_json::from_str(&request(&endpoint, "GET", "/tasks", None).unwrap()).unwrap();
    assert_eq!(tasks.as_array().unwrap().len(), 1);
    let audit = request(&endpoint, "GET", "/audit", None).unwrap();
    assert!(!audit.contains("signature_der_b64"));
    assert!(!audit.contains("payload_b64"));
    assert!(!audit.contains("trusted wake cross process local check"));
    server.stop();
}

#[test]
fn trusted_wake_lost_key_recovery_is_one_shot_disabled_and_redacted_across_processes() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("wake-recovery.sqlite");
    let old_key = P256SigningKey::from_bytes((&[31_u8; 32]).into()).unwrap();
    let old_public = old_key.verifying_key().to_encoded_point(false);
    let bootstrap = json!({
        "rule_id": "4a617276-6973-4000-8000-000000000010",
        "public_key_x963_b64": BASE64_STANDARD.encode(old_public.as_bytes()),
        "command": "trusted wake recovery cross process local check",
        "allow_rotation": false,
    });
    let mut original = JarvisServer::start_with_trusted_wake_bootstrap(&db_path, &bootstrap);
    let status: Value = serde_json::from_str(
        &request(&original.endpoint(), "GET", "/system-wake/status", None).unwrap(),
    )
    .unwrap();
    let new_key = P256SigningKey::from_bytes((&[32_u8; 32]).into()).unwrap();
    let new_public = new_key.verifying_key().to_encoded_point(false);
    let new_public_b64 = BASE64_STANDARD.encode(new_public.as_bytes());
    let prepare = json!({
        "operation": "recover",
        "expected_generation": status["rule"]["generation"],
        "expected_fingerprint": status["rule"]["key_fingerprint"],
        "new_public_key_x963_b64": new_public_b64,
        "confirmation": "RECOVER LOST TRUSTED WAKE KEY AND BLOCK PENDING WORK",
        "proof": null,
    });
    let prepared: Value = serde_json::from_str(
        &request(
            &original.endpoint(),
            "POST",
            "/system-wake/key-control/prepare",
            Some(&prepare.to_string()),
        )
        .unwrap(),
    )
    .unwrap();
    let token = prepared["grant_token"].as_str().unwrap().to_string();
    let target_generation = prepared["pending"]["target_generation"].as_u64().unwrap();
    assert_eq!(prepared["blocked_accepted_count"], 0);
    let prepared_status: Value = serde_json::from_str(
        &request(&original.endpoint(), "GET", "/system-wake/status", None).unwrap(),
    )
    .unwrap();
    assert_eq!(prepared_status["rule"]["enabled"], false);
    assert_eq!(prepared_status["rule"]["generation"], target_generation);
    original.stop();

    let install = json!({
        "rule_id": "4a617276-6973-4000-8000-000000000010",
        "target_generation": target_generation,
        "new_public_key_x963_b64": new_public_b64,
        "grant_token": token,
    });
    let mut restarted = JarvisServer::start_with_trusted_wake_key_control(&db_path, &install);
    let installed: Value = serde_json::from_str(
        &request(&restarted.endpoint(), "GET", "/system-wake/status", None).unwrap(),
    )
    .unwrap();
    assert_eq!(installed["rule"]["generation"], target_generation);
    assert_eq!(installed["rule"]["enabled"], false);
    assert!(installed["pending_key_control"].is_null());
    let expected_fingerprint = Sha256::digest(new_public.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(installed["rule"]["key_fingerprint"], expected_fingerprint);
    let audit = request(&restarted.endpoint(), "GET", "/audit", None).unwrap();
    assert!(!audit.contains(prepared["grant_token"].as_str().unwrap()));
    assert!(!audit.contains(&new_public_b64));
    assert!(audit.contains("trusted_wake_key_control_installed"));
    restarted.stop();
}

#[test]
fn trusted_wake_signed_rotation_rejects_wrong_key_and_token_replay_across_processes() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("wake-rotation.sqlite");
    let old_key = P256SigningKey::from_bytes((&[51_u8; 32]).into()).unwrap();
    let old_public = old_key.verifying_key().to_encoded_point(false);
    let bootstrap = json!({
        "rule_id": "4a617276-6973-4000-8000-000000000010",
        "public_key_x963_b64": BASE64_STANDARD.encode(old_public.as_bytes()),
        "command": "trusted wake signed rotation local check",
        "allow_rotation": false,
    });
    let mut original = JarvisServer::start_with_trusted_wake_bootstrap(&db_path, &bootstrap);
    let status: Value = serde_json::from_str(
        &request(&original.endpoint(), "GET", "/system-wake/status", None).unwrap(),
    )
    .unwrap();
    let new_key = P256SigningKey::from_bytes((&[52_u8; 32]).into()).unwrap();
    let new_public = new_key.verifying_key().to_encoded_point(false);
    let new_public_b64 = BASE64_STANDARD.encode(new_public.as_bytes());
    let new_fingerprint = Sha256::digest(new_public.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let proof_payload = json!({
        "domain": "jarvis.trusted-wake.key-control.v1",
        "schema_version": 1,
        "operation": "rotate",
        "rule_id": "4a617276-6973-4000-8000-000000000010",
        "expected_generation": status["rule"]["generation"],
        "expected_fingerprint": status["rule"]["key_fingerprint"],
        "new_fingerprint": new_fingerprint,
        "session_id": status["session_id"],
        "challenge": status["challenge"],
        "confirmation": "ROTATE TRUSTED WAKE KEY",
        "occurred_at": chrono::Utc::now(),
        "nonce": "00000000-0000-4000-8000-000000000551",
    });
    let proof_bytes = serde_json::to_vec(&proof_payload).unwrap();
    let proof_signature: P256Signature = old_key.sign(&proof_bytes);
    let prepare = json!({
        "operation": "rotate",
        "expected_generation": status["rule"]["generation"],
        "expected_fingerprint": status["rule"]["key_fingerprint"],
        "new_public_key_x963_b64": new_public_b64,
        "confirmation": "ROTATE TRUSTED WAKE KEY",
        "proof": {
            "payload_b64": BASE64_STANDARD.encode(&proof_bytes),
            "signature_der_b64": BASE64_STANDARD.encode(proof_signature.to_der().as_bytes())
        }
    });
    let prepared: Value = serde_json::from_str(
        &request(
            &original.endpoint(),
            "POST",
            "/system-wake/key-control/prepare",
            Some(&prepare.to_string()),
        )
        .unwrap(),
    )
    .unwrap();
    let token = prepared["grant_token"].as_str().unwrap().to_string();
    let target_generation = prepared["pending"]["target_generation"].as_u64().unwrap();
    original.stop();

    let wrong_key = P256SigningKey::from_bytes((&[53_u8; 32]).into()).unwrap();
    let wrong_install = json!({
        "rule_id": "4a617276-6973-4000-8000-000000000010",
        "target_generation": target_generation,
        "new_public_key_x963_b64": BASE64_STANDARD.encode(
            wrong_key.verifying_key().to_encoded_point(false).as_bytes()
        ),
        "grant_token": token,
    });
    assert_key_control_start_fails(&db_path, &wrong_install);

    let install = json!({
        "rule_id": "4a617276-6973-4000-8000-000000000010",
        "target_generation": target_generation,
        "new_public_key_x963_b64": new_public_b64,
        "grant_token": prepared["grant_token"],
    });
    let mut installed_server =
        JarvisServer::start_with_trusted_wake_key_control(&db_path, &install);
    let installed: Value = serde_json::from_str(
        &request(
            &installed_server.endpoint(),
            "GET",
            "/system-wake/status",
            None,
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(installed["rule"]["enabled"], false);
    assert_eq!(installed["rule"]["key_fingerprint"], new_fingerprint);
    installed_server.stop();
    assert_key_control_start_fails(&db_path, &install);

    let mut active = JarvisServer::start(&db_path);
    let active_status: Value = serde_json::from_str(
        &request(&active.endpoint(), "GET", "/system-wake/status", None).unwrap(),
    )
    .unwrap();
    request(
        &active.endpoint(),
        "POST",
        "/system-wake/rule",
        Some(&json!({ "enabled": true, "expected_generation": target_generation }).to_string()),
    )
    .unwrap();
    let wake_payload = json!({
        "schema_version": 1,
        "rule_id": "4a617276-6973-4000-8000-000000000010",
        "rule_generation": target_generation,
        "session_id": active_status["session_id"],
        "challenge": active_status["challenge"],
        "counter": 1,
        "occurred_at": chrono::Utc::now(),
        "nonce": "00000000-0000-4000-8000-000000000552",
    });
    let wake_bytes = serde_json::to_vec(&wake_payload).unwrap();
    let old_signature: P256Signature = old_key.sign(&wake_bytes);
    let old_envelope = json!({
        "payload_b64": BASE64_STANDARD.encode(&wake_bytes),
        "signature_der_b64": BASE64_STANDARD.encode(old_signature.to_der().as_bytes()),
    });
    assert!(request(
        &active.endpoint(),
        "POST",
        "/system-wake/events",
        Some(&old_envelope.to_string()),
    )
    .is_err());
    let new_signature: P256Signature = new_key.sign(&wake_bytes);
    let new_envelope = json!({
        "payload_b64": BASE64_STANDARD.encode(&wake_bytes),
        "signature_der_b64": BASE64_STANDARD.encode(new_signature.to_der().as_bytes()),
    });
    let accepted: Value = serde_json::from_str(
        &request(
            &active.endpoint(),
            "POST",
            "/system-wake/events",
            Some(&new_envelope.to_string()),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(accepted["event"]["state"], "completed");
    let audit = request(&active.endpoint(), "GET", "/audit", None).unwrap();
    assert!(!audit.contains(&token));
    assert!(!audit.contains(&new_public_b64));
    assert!(!audit.contains(&BASE64_STANDARD.encode(&proof_bytes)));
    assert!(!audit.contains(&BASE64_STANDARD.encode(proof_signature.to_der().as_bytes())));
    active.stop();
}

fn assert_key_control_start_fails(db_path: &Path, document: &Value) {
    let bind = unused_loopback_addr();
    let mut child = Command::new(jarvis_cli_bin())
        .args([
            "serve",
            "--bind",
            &bind.to_string(),
            "--db-path",
            db_path.to_str().unwrap(),
            "--trusted-wake-key-control-stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(document.to_string().as_bytes())
        .unwrap();
    assert!(!child.wait_with_output().unwrap().status.success());
}

struct JarvisServer {
    child: Option<Child>,
    endpoint: String,
    _temp_dir: TempDir,
}

impl JarvisServer {
    fn start_with_trusted_wake_bootstrap(db_path: &Path, bootstrap: &Value) -> Self {
        let _startup_guard = jarvis_server_startup_lock().lock().expect("startup lock");
        let bind = unused_loopback_addr();
        let endpoint = format!("http://{bind}");
        let temp_dir = tempfile::tempdir().expect("server temp dir");
        let mut child = Command::new(jarvis_cli_bin())
            .args([
                "serve",
                "--bind",
                &bind.to_string(),
                "--db-path",
                db_path.to_str().expect("db path"),
                "--trusted-wake-bootstrap-stdin",
            ])
            .current_dir(temp_dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start bootstrapped jarvis serve");
        child
            .stdin
            .take()
            .expect("bootstrap stdin")
            .write_all(serde_json::to_string(bootstrap).unwrap().as_bytes())
            .expect("write bootstrap");
        let mut server = Self {
            child: Some(child),
            endpoint,
            _temp_dir: temp_dir,
        };
        server.wait_until_healthy();
        server
    }

    fn start_with_trusted_wake_key_control(db_path: &Path, document: &Value) -> Self {
        let _startup_guard = jarvis_server_startup_lock().lock().expect("startup lock");
        let bind = unused_loopback_addr();
        let endpoint = format!("http://{bind}");
        let temp_dir = tempfile::tempdir().expect("server temp dir");
        let mut child = Command::new(jarvis_cli_bin())
            .args([
                "serve",
                "--bind",
                &bind.to_string(),
                "--db-path",
                db_path.to_str().expect("db path"),
                "--trusted-wake-key-control-stdin",
            ])
            .current_dir(temp_dir.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("start key-control jarvis serve");
        child
            .stdin
            .take()
            .expect("key-control stdin")
            .write_all(serde_json::to_string(document).unwrap().as_bytes())
            .expect("write key-control document");
        let mut server = Self {
            child: Some(child),
            endpoint,
            _temp_dir: temp_dir,
        };
        server.wait_until_healthy();
        server
    }
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
        let mut command = Command::new(jarvis_cli_bin());
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
            "private_transport_sentinel": "partial-envelope-must-not-surface",
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
            format!(
                "{}\n{}\n",
                json!({ "response": envelope, "done": false }),
                json!({ "response": "", "done": true })
            ),
            format!(
                "{}\n",
                json!({ "response": "provider saw tool result", "done": true })
            ),
        ];

        for response in responses {
            let (mut stream, _) = listener.accept().expect("ollama request");
            let mut buffer = [0_u8; 8192];
            let read = stream.read(&mut buffer).expect("read request");
            let request = String::from_utf8_lossy(&buffer[..read]);
            assert!(request.contains("POST /api/generate"), "{request}");
            assert!(request.contains("\"stream\":true"), "{request}");
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
                "HTTP/1.1 200 OK\r\ncontent-type: application/x-ndjson\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
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
            format!(
                "{}\n{}\n",
                json!({ "response": envelope, "done": false }),
                json!({ "response": "", "done": true })
            ),
            format!(
                "{}\n",
                json!({ "response": "provider recovered after tool rejection", "done": true })
            ),
        ];

        for (index, response) in responses.into_iter().enumerate() {
            let (mut stream, _) = listener.accept().expect("ollama request");
            let mut buffer = [0_u8; 8192];
            let read = stream.read(&mut buffer).expect("read request");
            let request = String::from_utf8_lossy(&buffer[..read]);
            assert!(request.contains("POST /api/generate"), "{request}");
            assert!(request.contains("\"stream\":true"), "{request}");
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
                "HTTP/1.1 200 OK\r\ncontent-type: application/x-ndjson\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                response.len(),
                response
            );
            stream.write_all(http.as_bytes()).expect("write response");
        }
    });

    (format!("http://{address}"), handle)
}

fn start_ollama_invalid_then_valid_tool_server() -> (String, thread::JoinHandle<()>) {
    let listener =
        TcpListener::bind("127.0.0.1:0").expect("bind ollama invalid-then-valid-tool stub");
    let address = listener
        .local_addr()
        .expect("ollama invalid-then-valid-tool stub address");
    let handle = thread::spawn(move || {
        let invalid_envelope = json!({
            "message": "provider guessed an invalid status plugin",
            "complete": false,
            "tool_requests": [
                {
                    "plugin_id": "status",
                    "action": "status",
                    "input": {}
                }
            ]
        })
        .to_string();
        let valid_envelope = json!({
            "message": "provider retried with registered status",
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
            format!(
                "{}\n{}\n",
                json!({ "response": invalid_envelope, "done": false }),
                json!({ "response": "", "done": true })
            ),
            format!(
                "{}\n{}\n",
                json!({ "response": valid_envelope, "done": false }),
                json!({ "response": "", "done": true })
            ),
            format!(
                "{}\n",
                json!({ "response": "provider completed after valid status", "done": true })
            ),
        ];

        for (index, response) in responses.into_iter().enumerate() {
            let (mut stream, _) = listener
                .accept()
                .expect("ollama invalid-then-valid-tool request");
            let mut buffer = [0_u8; 8192];
            let read = stream.read(&mut buffer).expect("read request");
            let request = String::from_utf8_lossy(&buffer[..read]);
            assert!(request.contains("POST /api/generate"), "{request}");
            assert!(request.contains("\"stream\":true"), "{request}");
            assert!(
                request.contains("Registered first-party tools are exactly this JSON allowlist")
            );
            assert!(request.contains("\\\"plugin_id\\\":\\\"fake_status\\\""));
            assert!(request.contains("\\\"action\\\":\\\"status\\\""));
            match index {
                1 => {
                    assert!(request.contains("rejected"), "{request}");
                    assert!(
                        request.contains("plugin status is not registered"),
                        "{request}"
                    );
                }
                2 => {
                    assert!(
                        request.contains("\\\"status\\\":\\\"completed\\\""),
                        "{request}"
                    );
                    assert!(request.contains("\\\"plugin_count\\\""), "{request}");
                }
                _ => {}
            }
            let http = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/x-ndjson\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
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
        let response = format!(
            "{}\n",
            json!({
                "response": "I can check that.\n{\"tool_requests\":[{\"plugin_id\":\"fake_status\",\"action\":\"status\",\"input\":{}}]}",
                "done": true
            })
        );

        let (mut stream, _) = listener.accept().expect("ollama request");
        let mut buffer = [0_u8; 8192];
        let read = stream.read(&mut buffer).expect("read request");
        let request = String::from_utf8_lossy(&buffer[..read]);
        assert!(request.contains("POST /api/generate"), "{request}");
        assert!(request.contains("\"stream\":true"), "{request}");
        assert!(request.contains("one strict JSON object with no surrounding prose"));
        let http = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/x-ndjson\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
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
    match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => {
            let addr = listener.local_addr().expect("read local addr");
            drop(listener);
            addr
        }
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            "127.0.0.1:9".parse().expect("discard endpoint")
        }
        Err(error) => panic!("bind ephemeral port: {error}"),
    }
}

fn run_cli_json<const N: usize>(args: [&str; N]) -> Value {
    let output = run_cli_with_env(args, &[("JARVIS_CLI_JSON", "1")]);
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "stdout was not JSON: {error}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn http_get_json(endpoint: &str, path: &str) -> Value {
    let host = endpoint
        .strip_prefix("http://")
        .unwrap_or_else(|| panic!("expected http endpoint: {endpoint}"));
    let mut stream = TcpStream::connect(host).unwrap_or_else(|error| {
        panic!("connect to {endpoint}: {error}");
    });
    stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .expect("set http read timeout");
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )
    .expect("write http request");

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read http response");
    let (head, body) = response
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("invalid HTTP response: {response}"));
    assert!(
        head.starts_with("HTTP/1.1 200") || head.starts_with("HTTP/1.0 200"),
        "unexpected HTTP response for {path}: {head}\n{body}"
    );
    serde_json::from_str(body).unwrap_or_else(|error| {
        panic!("HTTP body was not JSON: {error}\npath: {path}\nbody:\n{body}");
    })
}

fn read_json_file(path: impl AsRef<Path>) -> Value {
    let path = path.as_ref();
    let data = fs::read(path).unwrap_or_else(|error| {
        panic!("failed to read JSON file {}: {error}", path.display());
    });
    serde_json::from_slice(&data).unwrap_or_else(|error| {
        panic!("file was not JSON: {error}\npath: {}", path.display());
    })
}

fn run_cli_json_with_env<const N: usize>(args: [&str; N], env: &[(&str, &str)]) -> Value {
    let mut merged_env = Vec::with_capacity(env.len() + 1);
    merged_env.push(("JARVIS_CLI_JSON", "1"));
    merged_env.extend_from_slice(env);
    let output = run_cli_with_env(args, &merged_env);
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "stdout was not JSON: {error}\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

fn run_cli_failure<const N: usize>(args: [&str; N]) -> String {
    run_cli_failure_args(&args)
}

fn run_cli_failure_args(args: &[&str]) -> String {
    let output = Command::new(jarvis_cli_bin())
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

fn run_cli_text_with_env<const N: usize>(args: [&str; N], env: &[(&str, &str)]) -> String {
    let output = run_cli_with_env(args, env);
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

fn run_cli<const N: usize>(args: [&str; N]) -> Output {
    run_cli_with_env(args, &[])
}

fn run_cli_with_env<const N: usize>(args: [&str; N], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(jarvis_cli_bin());
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

#[cfg(unix)]
fn run_repo_script_with_env(script: &str, args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(workspace_root().join(script));
    command
        .args(args)
        .stdin(Stdio::null())
        .current_dir(workspace_root());
    for (key, value) in env {
        command.env(key, value);
    }
    let output = command.output().expect("run repository script");

    assert!(
        output.status.success(),
        "repository script failed: {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

#[cfg(unix)]
fn run_repo_script_failure_with_env(script: &str, args: &[&str], env: &[(&str, &str)]) -> String {
    let mut command = Command::new(workspace_root().join(script));
    command
        .args(args)
        .stdin(Stdio::null())
        .current_dir(workspace_root());
    for (key, value) in env {
        command.env(key, value);
    }
    let output = command.output().expect("run repository script");

    assert!(
        !output.status.success(),
        "repository script unexpectedly succeeded: {:?}\nstdout:\n{}\nstderr:\n{}",
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

#[cfg(unix)]
fn workspace_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
        .to_path_buf()
}

fn valid_live_device_qa_report() -> Value {
    json!({
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
        "bundled_core": {
            "executable_path": "/Applications/Jarvis.app/Contents/Resources/bin/jarvis-cli",
            "version": "jarvis 0.1.4",
            "sha256": "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
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
        "owner_recorded_non_voice_evidence": {
            "clean_profile_evidence_note": "Clean profile install observed.",
            "finder_launch_evidence_note": "Finder launch observed.",
            "notification_evidence_note": "Scheduler notification observed.",
            "notification_observed_at": "2026-05-22T16:04:00Z",
            "restart_evidence_note": "Restart recovery observed.",
            "manual_release_qa_evidence_note": "Manual release QA surfaces observed."
        },
        "notification_observation": {
            "kind": "due_now",
            "title": "Scheduler job ready: Release reminder",
            "body": "A scheduled Jarvis job is due now.",
            "thread_identifier": "jarvis.scheduler",
            "observed_at": "2026-05-22T16:04:00Z"
        },
        "voice_command_observation": {
            "test_phrase": "Jarvis status check.",
            "observed_transcript": "Jarvis status check.",
            "expected_command_text": "status check",
            "observed_command_text": "status check",
            "command_result_evidence_id": "task:00000000-0000-4000-8000-000000000002",
            "audio_output_device_label": "Built-in speakers"
        },
        "proof_boundary": "Owner-recorded live device QA fixture for CLI E2E."
    })
}

fn valid_plugin_trust_qa_report() -> Value {
    json!({
        "schema_version": 1,
        "evidence_type": "owner_recorded_plugin_trust_qa",
        "version": "0.1.4",
        "self_test_fixture": false,
        "generated_at": "2026-05-22T16:21:00Z",
        "review_source": "owner-asserted-manual-review",
        "validation_flags": {
            "marketplace_review": true,
            "malware_scan": true,
            "os_sandbox": true,
            "egress_enforcement": true,
            "signed_publisher_policy": true,
            "manual_trust_review": true
        },
        "owner_recorded_plugin_trust_evidence": {
            "owner_name": "Release Operator",
            "review_started_at": "2026-05-22T16:10:00Z",
            "review_completed_at": "2026-05-22T16:20:00Z",
            "marketplace_evidence_note": "Marketplace review evidence archived.",
            "malware_scan_evidence_note": "Malware scan evidence archived.",
            "os_sandbox_evidence_note": "OS sandbox validation evidence archived.",
            "egress_evidence_note": "Host-level egress validation evidence archived.",
            "egress_policy_label": "Host egress policy/profile reviewed.",
            "egress_validation_completed_at": "2026-05-22T16:18:00Z",
            "egress_deny_fixture_evidence_note": "Undeclared-host deny evidence archived.",
            "egress_allow_fixture_evidence_note": "Declared-host allow evidence archived.",
            "signed_publisher_evidence_note": "Signed publisher policy evidence archived.",
            "manual_review_evidence_note": "Manual plugin trust review evidence archived."
        },
        "evidence_artifacts": {
            "marketplace_review": {
                "uri": "archive://jarvis/plugin-trust/marketplace-review.json",
                "sha256": "1111111111111111111111111111111111111111111111111111111111111111"
            },
            "malware_scan": {
                "uri": "archive://jarvis/plugin-trust/malware-scan.json",
                "sha256": "2222222222222222222222222222222222222222222222222222222222222222"
            },
            "os_sandbox": {
                "uri": "archive://jarvis/plugin-trust/os-sandbox.json",
                "sha256": "3333333333333333333333333333333333333333333333333333333333333333"
            },
            "egress_enforcement": {
                "uri": "archive://jarvis/plugin-trust/egress.json",
                "sha256": "4444444444444444444444444444444444444444444444444444444444444444"
            },
            "signed_publisher_policy": {
                "uri": "archive://jarvis/plugin-trust/signed-publisher.json",
                "sha256": "5555555555555555555555555555555555555555555555555555555555555555"
            },
            "manual_trust_review": {
                "uri": "archive://jarvis/plugin-trust/manual-review.json",
                "sha256": "6666666666666666666666666666666666666666666666666666666666666666"
            }
        },
        "proof_boundary": "Owner-recorded plugin trust fixture for CLI E2E."
    })
}

fn plugin_trust_artifact_keys() -> [&'static str; 6] {
    [
        "marketplace_review",
        "malware_scan",
        "os_sandbox",
        "egress_enforcement",
        "signed_publisher_policy",
        "manual_trust_review",
    ]
}

fn valid_release_evidence_bundle() -> Value {
    let digest = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    json!({
        "schema_version": 1,
        "evidence_type": "release_evidence_bundle",
        "generated_at": "2026-05-22T17:00:00Z",
        "version": "0.1.4",
        "artifacts": {
            "app_path": "target/distribution/Jarvis.app",
            "zip_path": "target/distribution/Jarvis-0.1.4.zip",
            "pkg_path": "target/distribution/Jarvis-0.1.4.pkg",
            "zip_sha256": digest,
            "pkg_sha256": digest
        },
        "reports": {
            "signed_distribution_provenance_report": "target/distribution/Jarvis-0.1.4-signed-provenance.json",
            "live_device_qa_report": "target/release-live-device-qa-report.json",
            "plugin_trust_qa_report": "target/release-plugin-trust-qa-report.json",
            "signed_distribution_provenance_sha256": digest,
            "live_device_qa_sha256": digest,
            "plugin_trust_qa_sha256": digest
        },
        "validation_flags": {
            "signed_distribution": true,
            "notarization": true,
            "clean_profile": true,
            "live_device_qa": true,
            "plugin_trust_qa": true,
            "reports_archived": true,
            "local_signature_validation": true
        },
        "owner_recorded_release_evidence": {
            "owner_name": "Release Operator",
            "completed_at": "2026-05-22T16:45:00Z",
            "signed_distribution_note": "Signed distribution provenance reviewed.",
            "notarization_note": "Notarization evidence reviewed.",
            "clean_profile_note": "Clean profile evidence reviewed.",
            "live_device_qa_note": "Live-device QA evidence reviewed.",
            "plugin_trust_qa_note": "Plugin-trust QA evidence reviewed.",
            "reports_archive_note": "Release evidence reports archived.",
            "reports_archive_uri": "file://release-evidence/archive"
        },
        "proof_boundary": "Release evidence bundle fixture for CLI E2E."
    })
}

#[cfg(unix)]
fn valid_release_evidence_bundle_for_paths(
    app_path: &Path,
    zip_path: &Path,
    pkg_path: &Path,
    signed_provenance_path: &Path,
    live_report_path: &Path,
    plugin_report_path: &Path,
) -> Value {
    let zip_sha256 = file_sha256(zip_path);
    let pkg_sha256 = file_sha256(pkg_path);
    let signed_provenance_sha256 = file_sha256(signed_provenance_path);
    let live_device_qa_sha256 = file_sha256(live_report_path);
    let plugin_trust_qa_sha256 = file_sha256(plugin_report_path);
    json!({
        "schema_version": 1,
        "evidence_type": "release_evidence_bundle",
        "generated_at": "2026-05-22T17:00:00Z",
        "version": "0.1.4",
        "artifacts": {
            "app_path": app_path.to_str().expect("app path utf8"),
            "zip_path": zip_path.to_str().expect("zip path utf8"),
            "pkg_path": pkg_path.to_str().expect("pkg path utf8"),
            "zip_sha256": zip_sha256,
            "pkg_sha256": pkg_sha256
        },
        "reports": {
            "signed_distribution_provenance_report": signed_provenance_path.to_str().expect("signed provenance path utf8"),
            "live_device_qa_report": live_report_path.to_str().expect("live report path utf8"),
            "plugin_trust_qa_report": plugin_report_path.to_str().expect("plugin report path utf8"),
            "signed_distribution_provenance_sha256": signed_provenance_sha256,
            "live_device_qa_sha256": live_device_qa_sha256,
            "plugin_trust_qa_sha256": plugin_trust_qa_sha256
        },
        "validation_flags": {
            "signed_distribution": true,
            "notarization": true,
            "clean_profile": true,
            "live_device_qa": true,
            "plugin_trust_qa": true,
            "reports_archived": true,
            "local_signature_validation": true
        },
        "owner_recorded_release_evidence": {
            "owner_name": "Release Operator",
            "completed_at": "2026-05-22T16:45:00Z",
            "signed_distribution_note": "Signed distribution provenance reviewed.",
            "notarization_note": "Notarization evidence reviewed.",
            "clean_profile_note": "Clean profile evidence reviewed.",
            "live_device_qa_note": "Live-device QA evidence reviewed.",
            "plugin_trust_qa_note": "Plugin-trust QA evidence reviewed.",
            "reports_archive_note": "Release evidence reports archived.",
            "reports_archive_uri": "file://release-evidence/archive"
        },
        "proof_boundary": "Release evidence bundle fixture for CLI E2E."
    })
}

fn valid_signed_distribution_provenance_report(
    app_path: &str,
    zip_path: &str,
    pkg_path: &str,
    zip_sha256: &str,
    pkg_sha256: &str,
) -> Value {
    let bundled_core_path = Path::new(app_path).join("Contents/Resources/bin/jarvis-cli");
    let bundled_core_sha256 = file_sha256(&bundled_core_path);
    let notary_log_dir = Path::new(zip_path)
        .parent()
        .expect("zip parent")
        .join("notary-logs");
    let app_zip_notary_log = notary_log_dir.join("app.log");
    let installer_pkg_notary_log = notary_log_dir.join("pkg.log");
    let app_zip_notary_log_sha256 = file_sha256(&app_zip_notary_log);
    let installer_pkg_notary_log_sha256 = file_sha256(&installer_pkg_notary_log);
    json!({
        "schema_version": 1,
        "evidence_type": "signed_distribution_provenance",
        "generated_at": "2026-05-22T16:40:00Z",
        "version": "0.1.4",
        "bundle_identifier": "com.nobiletechnology.jarvis",
        "artifacts": {
            "app_path": app_path,
            "zip_path": zip_path,
            "pkg_path": pkg_path,
            "zip_sha256": zip_sha256,
            "pkg_sha256": pkg_sha256,
            "bundled_core_path": bundled_core_path.to_str().expect("bundled core path utf8"),
            "bundled_core_sha256": bundled_core_sha256,
            "bundled_core_version": "jarvis 0.1.4"
        },
        "signing": {
            "developer_id_application_identity": "Developer ID Application: Jarvis QA Fixture",
            "developer_id_installer_identity": "Developer ID Installer: Jarvis QA Fixture",
            "app_bundle_codesign": "Authority=Developer ID Application: Jarvis QA Fixture",
            "app_executable_codesign": "Authority=Developer ID Application: Jarvis QA Fixture",
            "bundled_core_codesign": "Authority=Developer ID Application: Jarvis QA Fixture",
            "installer_pkg_signature": "Developer ID Installer: Jarvis QA Fixture"
        },
        "notarization": {
            "app_zip_submission_id": "00000000-0000-4000-8000-000000000001",
            "installer_pkg_submission_id": "00000000-0000-4000-8000-000000000002",
            "app_zip_status": "Accepted",
            "installer_pkg_status": "Accepted",
            "app_zip_notary_log": app_zip_notary_log.to_str().expect("app notary log path utf8"),
            "installer_pkg_notary_log": installer_pkg_notary_log.to_str().expect("pkg notary log path utf8"),
            "app_zip_notary_log_sha256": app_zip_notary_log_sha256,
            "installer_pkg_notary_log_sha256": installer_pkg_notary_log_sha256
        },
        "stapling": {
            "app_bundle_validation": "The validate action worked!",
            "installer_pkg_validation": "The validate action worked!"
        },
        "gatekeeper": {
            "app_bundle_assessment": "accepted",
            "installer_pkg_assessment": "accepted"
        },
        "validation_flags": {
            "developer_id_application_signed": true,
            "developer_id_installer_signed": true,
            "app_zip_notarized": true,
            "installer_pkg_notarized": true,
            "app_stapled": true,
            "installer_pkg_stapled": true,
            "gatekeeper_assessed": true,
            "artifact_digests_recorded": true
        },
        "proof_boundary": "Signed distribution provenance fixture for CLI E2E."
    })
}

fn write_valid_live_device_qa_report(path: &Path) {
    write_live_device_qa_report(path, valid_live_device_qa_report());
}

fn write_live_device_qa_report(path: &Path, report: Value) {
    write_json_report(path, report);
}

fn write_json_report(path: &Path, report: Value) {
    fs::write(
        path,
        serde_json::to_string_pretty(&report).expect("serialize JSON report"),
    )
    .expect("write JSON report");
}

fn remove_json_field(value: &mut Value, path: &str) {
    let (parent_path, field) = path
        .rsplit_once('.')
        .unwrap_or_else(|| panic!("missing nested JSON path: {path}"));
    let pointer = format!("/{}", parent_path.replace('.', "/"));
    let parent = value
        .pointer_mut(&pointer)
        .unwrap_or_else(|| panic!("missing JSON parent {parent_path}"));
    parent
        .as_object_mut()
        .unwrap_or_else(|| panic!("JSON parent is not an object: {parent_path}"))
        .remove(field)
        .unwrap_or_else(|| panic!("missing JSON field: {path}"));
}

#[cfg(unix)]
struct CompleteReleaseEvidenceFixture {
    dist_dir: String,
    signed_provenance_path: String,
    live_report_path: String,
    plugin_report_path: String,
    bundle_path: String,
}

#[cfg(unix)]
impl CompleteReleaseEvidenceFixture {
    fn env_refs(&self) -> Vec<(&str, &str)> {
        vec![
            ("JARVIS_EVIDENCE_DIST_DIR", self.dist_dir.as_str()),
            (
                "JARVIS_EVIDENCE_SIGNED_PROVENANCE_REPORT",
                self.signed_provenance_path.as_str(),
            ),
            (
                "JARVIS_EVIDENCE_LIVE_QA_REPORT",
                self.live_report_path.as_str(),
            ),
            (
                "JARVIS_EVIDENCE_PLUGIN_QA_REPORT",
                self.plugin_report_path.as_str(),
            ),
            ("JARVIS_EVIDENCE_OUTPUT_PATH", self.bundle_path.as_str()),
        ]
    }

    fn env_refs_with_external_mode(&self) -> Vec<(&str, &str)> {
        let mut env = self.env_refs();
        env.push(("JARVIS_RELEASE_READINESS_EVIDENCE_MODE", "external"));
        env
    }
}

#[cfg(unix)]
fn write_complete_release_evidence_fixture(root: &Path) -> CompleteReleaseEvidenceFixture {
    let dist_dir = write_placeholder_distribution(root);
    let live_report_path = root.join("release-live-device-qa-report.json");
    let plugin_report_path = root.join("release-plugin-trust-qa-report.json");
    let signed_provenance_path = dist_dir.join("Jarvis-0.1.4-signed-provenance.json");
    let bundle_path = root.join("release-evidence-bundle.json");

    let bundled_core_sha256 =
        file_sha256(&dist_dir.join("Jarvis.app/Contents/Resources/bin/jarvis-cli"));
    let mut live_report = valid_live_device_qa_report();
    live_report["bundled_core"]["sha256"] = json!(bundled_core_sha256);
    write_json_report(&live_report_path, live_report);
    write_json_report(&plugin_report_path, valid_plugin_trust_qa_report());
    let zip_path = dist_dir.join("Jarvis-0.1.4.zip");
    let pkg_path = dist_dir.join("Jarvis-0.1.4.pkg");
    let notary_log_dir = dist_dir.join("notary-logs");
    fs::create_dir_all(&notary_log_dir).expect("create notary log dir");
    fs::write(
        notary_log_dir.join("app.log"),
        "id: 00000000-0000-4000-8000-000000000001\nstatus: Accepted\n",
    )
    .expect("write app notary log");
    fs::write(
        notary_log_dir.join("pkg.log"),
        "id: 00000000-0000-4000-8000-000000000002\nstatus: Accepted\n",
    )
    .expect("write package notary log");
    let zip_sha256 = file_sha256(&zip_path);
    let pkg_sha256 = file_sha256(&pkg_path);
    write_json_report(
        &signed_provenance_path,
        valid_signed_distribution_provenance_report(
            dist_dir.join("Jarvis.app").to_str().expect("app path utf8"),
            zip_path.to_str().expect("zip path utf8"),
            pkg_path.to_str().expect("pkg path utf8"),
            &zip_sha256,
            &pkg_sha256,
        ),
    );
    write_json_report(
        &bundle_path,
        valid_release_evidence_bundle_for_paths(
            &dist_dir.join("Jarvis.app"),
            &zip_path,
            &pkg_path,
            &signed_provenance_path,
            &live_report_path,
            &plugin_report_path,
        ),
    );

    CompleteReleaseEvidenceFixture {
        dist_dir: dist_dir.to_str().expect("dist dir utf8").to_string(),
        signed_provenance_path: signed_provenance_path
            .to_str()
            .expect("signed provenance path utf8")
            .to_string(),
        live_report_path: live_report_path
            .to_str()
            .expect("live report path utf8")
            .to_string(),
        plugin_report_path: plugin_report_path
            .to_str()
            .expect("plugin report path utf8")
            .to_string(),
        bundle_path: bundle_path.to_str().expect("bundle path utf8").to_string(),
    }
}

#[cfg(unix)]
fn bind_complete_release_evidence_fixture_to_task(
    fixture: &CompleteReleaseEvidenceFixture,
    task_id: &str,
) {
    let live_report_path = Path::new(&fixture.live_report_path);
    let mut live_report: Value =
        serde_json::from_str(&fs::read_to_string(live_report_path).expect("read live report"))
            .expect("decode live report");
    live_report["voice_command_observation"]["command_result_evidence_id"] =
        json!(format!("task:{task_id}"));
    write_json_report(live_report_path, live_report);

    let bundle_path = Path::new(&fixture.bundle_path);
    let mut bundle_report: Value =
        serde_json::from_str(&fs::read_to_string(bundle_path).expect("read evidence bundle"))
            .expect("decode evidence bundle");
    bundle_report["reports"]["live_device_qa_sha256"] = json!(file_sha256(live_report_path));
    write_json_report(bundle_path, bundle_report);
}

fn file_sha256(path: &Path) -> String {
    let contents = fs::read(path).expect("read digest fixture");
    format!("{:x}", Sha256::digest(&contents))
}

fn assert_all_evidence_items_present(evidence_status: &Value) {
    for key in [
        "signed_app_bundle",
        "app_executable",
        "bundled_core_executable",
        "signed_app_zip",
        "signed_installer_package",
        "signed_distribution_provenance_report",
        "live_device_qa_report",
        "plugin_trust_qa_report",
        "release_evidence_bundle",
    ] {
        assert!(evidence_status["items"]
            .as_array()
            .expect("evidence items")
            .iter()
            .any(|item| item["key"] == key && item["status"] == "present"));
    }
}

fn release_evidence_item<'a>(evidence_status: &'a Value, key: &str) -> &'a Value {
    evidence_status["items"]
        .as_array()
        .expect("evidence items")
        .iter()
        .find(|item| item["key"] == key)
        .unwrap_or_else(|| panic!("missing evidence item {key}"))
}

fn readiness_feature<'a>(readiness: &'a Value, key: &str) -> &'a Value {
    readiness["implemented_features"]
        .as_array()
        .expect("implemented readiness features")
        .iter()
        .find(|feature| feature["key"] == key)
        .unwrap_or_else(|| panic!("missing readiness feature {key}"))
}

fn assert_release_evidence_item_status(
    evidence_status: &Value,
    key: &str,
    expected_status: &str,
    context: &str,
) {
    let item = release_evidence_item(evidence_status, key);
    assert_eq!(
        item["status"], expected_status,
        "{context}: expected {key} to be {expected_status}: {item}"
    );
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path).expect("file metadata").permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions).expect("chmod executable");
}

#[cfg(unix)]
fn write_live_device_qa_app_fixture(root: &Path) -> PathBuf {
    let app_path = root.join("Jarvis.app");
    let macos_dir = app_path.join("Contents/MacOS");
    let contents_dir = app_path.join("Contents");
    let resources_dir = app_path.join("Contents/Resources/bin");
    fs::create_dir_all(&macos_dir).expect("create live QA app executable dir");
    fs::create_dir_all(&resources_dir).expect("create live QA bundled core dir");
    fs::write(
        contents_dir.join("Info.plist"),
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>JarvisMacApp</string>
  <key>CFBundleIdentifier</key>
  <string>com.nobiletechnology.jarvis.selftest</string>
  <key>CFBundleShortVersionString</key>
  <string>{version}</string>
  <key>CFBundleVersion</key>
  <string>{version}</string>
  <key>NSMicrophoneUsageDescription</key>
  <string>Jarvis uses microphone input only when you explicitly start local voice capture.</string>
  <key>NSSpeechRecognitionUsageDescription</key>
  <string>Jarvis uses speech recognition only to turn your spoken command into a local assistant request.</string>
</dict>
</plist>
"#,
            version = env!("CARGO_PKG_VERSION")
        ),
    )
    .expect("write live QA Info.plist");
    let app_executable = macos_dir.join("JarvisMacApp");
    let bundled_core = resources_dir.join("jarvis-cli");
    fs::write(&app_executable, "#!/bin/sh\n").expect("write live QA app executable");
    write_fixture_bundled_core_executable(&bundled_core);
    make_executable(&app_executable);
    make_executable(&bundled_core);
    app_path
}

#[cfg(unix)]
fn write_placeholder_distribution(root: &Path) -> std::path::PathBuf {
    let dist_dir = root.join("dist");
    let app_path = dist_dir.join("Jarvis.app");
    let macos_dir = app_path.join("Contents/MacOS");
    let contents_dir = app_path.join("Contents");
    let resources_dir = app_path.join("Contents/Resources/bin");
    fs::create_dir_all(&macos_dir).expect("create app executable dir");
    fs::create_dir_all(&resources_dir).expect("create bundled core dir");
    fs::write(
        contents_dir.join("Info.plist"),
        r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key>
  <string>com.nobiletechnology.jarvis</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.4</string>
  <key>CFBundleVersion</key>
  <string>0.1.4</string>
  <key>NSMicrophoneUsageDescription</key>
  <string>Jarvis uses microphone input only when you explicitly start local voice capture.</string>
  <key>NSSpeechRecognitionUsageDescription</key>
  <string>Jarvis uses speech recognition only to turn your spoken command into a local assistant request.</string>
</dict>
</plist>
"#,
    )
    .expect("write Info.plist");
    let app_executable = macos_dir.join("JarvisMacApp");
    let bundled_core = resources_dir.join("jarvis-cli");
    fs::write(&app_executable, "#!/bin/sh\n").expect("write app executable");
    write_fixture_bundled_core_executable(&bundled_core);
    fs::write(resources_dir.join("jarvis-cli.version"), "jarvis 0.1.4\n")
        .expect("write bundled core version marker");
    make_executable(&app_executable);
    make_executable(&bundled_core);
    let zip_output = Command::new("zip")
        .args(["-qr", "Jarvis-0.1.4.zip", "Jarvis.app"])
        .current_dir(&dist_dir)
        .output()
        .expect("create app zip fixture");
    assert!(
        zip_output.status.success(),
        "create app zip fixture failed: {:?}\nstdout:\n{}\nstderr:\n{}",
        zip_output.status.code(),
        String::from_utf8_lossy(&zip_output.stdout),
        String::from_utf8_lossy(&zip_output.stderr)
    );
    fs::write(dist_dir.join("Jarvis-0.1.4.pkg"), "pkg placeholder").expect("write pkg placeholder");
    dist_dir
}

#[cfg(unix)]
fn write_fixture_bundled_core_executable(path: &Path) {
    let debug_cli = workspace_root().join("target/debug/jarvis");
    if debug_cli.is_file() {
        fs::copy(&debug_cli, path).expect("copy current CLI into fixture bundle");
    } else {
        fs::write(path, "#!/bin/sh\nprintf 'jarvis 0.1.4\\n'\n")
            .expect("write bundled core fallback");
    }
}

fn signed_manifest(mut manifest: Value, signing_key: &SigningKey) -> Value {
    let mut typed_manifest: jarvis_core::PluginManifest =
        serde_json::from_value(manifest.clone()).expect("decode unsigned plugin manifest");
    typed_manifest.source_path = None;
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

fn assert_redacts_installed_plugin_provenance(encoded: &str, plugin_dir: &str) {
    assert!(!encoded.contains(plugin_dir), "{encoded}");
    for field in [
        "\"source_path\":",
        "\"manifest_path\":",
        "\"manifest_sha256\":",
        "\"source_tree_sha256\":",
        "\"subprocess_command_path\":",
        "\"subprocess_command_sha256\":",
    ] {
        assert!(!encoded.contains(field), "{field} leaked in {encoded}");
    }
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

fn assert_string_array_missing(value: &Value, unexpected: &str) {
    let array = value.as_array().unwrap_or_else(|| {
        panic!("expected array, got {}", json!(value));
    });
    assert!(
        array.iter().all(|item| item.as_str() != Some(unexpected)),
        "expected array to omit {unexpected}, got {value}"
    );
}

fn assert_string_array_exact(value: &Value, expected: &[&str]) {
    let array = value.as_array().unwrap_or_else(|| {
        panic!("expected array, got {}", json!(value));
    });
    let actual = array
        .iter()
        .map(|item| {
            item.as_str()
                .unwrap_or_else(|| panic!("expected string array item, got {item}"))
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, expected, "unexpected string array: {value}");
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

fn assert_string_array_order(value: &Value, first: &str, second: &str) {
    assert_string_array_substring_order(value, first, second);
}

fn assert_string_array_substring_order(value: &Value, first: &str, second: &str) {
    let array = value.as_array().unwrap_or_else(|| {
        panic!("expected array, got {}", json!(value));
    });
    let first_index = array
        .iter()
        .filter_map(Value::as_str)
        .position(|item| item.contains(first))
        .unwrap_or_else(|| panic!("expected array to contain item with {first}, got {value}"));
    let second_index = array
        .iter()
        .filter_map(Value::as_str)
        .position(|item| item.contains(second))
        .unwrap_or_else(|| panic!("expected array to contain item with {second}, got {value}"));
    assert!(
        first_index < second_index,
        "expected {first} before {second}, got {value}"
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
