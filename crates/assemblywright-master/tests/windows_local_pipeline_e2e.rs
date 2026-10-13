use fs2::FileExt;
use git2::{Repository, Signature};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn local_pipeline_completes_with_real_validator_and_separate_review() {
    let fixture = Fixture::new();
    {
        let source_repo = Repository::open(&fixture.repo).unwrap();
        source_repo
            .remote(
                "origin",
                "https://snapshot-user:snapshot-secret@example.invalid/source.git",
            )
            .unwrap();
        source_repo
            .config()
            .unwrap()
            .set_str("assemblywright.snapshot-secret", "must-not-copy")
            .unwrap();
    }
    let server = FakeModel::start(vec![
        edits("src.py", "GOOD = 'implemented'\n"),
        review(true, &[]),
    ]);
    let mut job = fixture.job(server.endpoint(), 2, 30);
    job["model"] = json!("implementer-model");
    job["reviewer_model"] = json!("reviewer-model");
    job["reasoning_effort"] = json!("max");
    job["reviewer_reasoning_effort"] = json!("ultra");
    let output = fixture.run(&job);
    let requests = server.join();

    assert_success(&output);
    let implementation: Value = serde_json::from_str(&requests[0]).unwrap();
    let review_request: Value = serde_json::from_str(&requests[1]).unwrap();
    assert_eq!(implementation["model"], "implementer-model");
    assert_eq!(implementation["reasoning_effort"], "max");
    assert_eq!(implementation["reasoning_budget_tokens"], 12_288);
    assert_eq!(implementation["reasoning_format"], "deepseek");
    assert_eq!(implementation["max_tokens"], 32_768);
    assert_eq!(implementation["response_format"]["type"], "json_object");
    assert_eq!(
        implementation["chat_template_kwargs"]["enable_thinking"],
        true
    );
    assert_eq!(review_request["model"], "reviewer-model");
    assert_eq!(review_request["reasoning_effort"], "ultra");
    assert_eq!(review_request["reasoning_budget_tokens"], 16_384);
    assert_eq!(review_request["reasoning_format"], "deepseek");
    assert_eq!(review_request["max_tokens"], 32_768);
    assert_eq!(review_request["response_format"]["type"], "json_object");
    assert!(requests[1].contains("Findings are only blocking defects"));
    assert!(requests[1].contains("positive observations"));
    assert!(requests[1].contains(r#"\"approve\":true,\"findings\":[]"#));
    let report = fixture.report();
    assert_eq!(report["status"], "completed");
    assert_eq!(report["stage"], "completed");
    let candidate = PathBuf::from(report["candidate"].as_str().unwrap());
    assert_eq!(
        fs::read_to_string(candidate.join("src.py")).unwrap(),
        "GOOD = 'implemented'\n"
    );
    assert!(
        !candidate.join(".git").join("config").exists()
            || !fs::read_to_string(candidate.join(".git").join("config"))
                .unwrap()
                .contains("remote \"origin\"")
    );
    let source_repo = Repository::open(&fixture.repo).unwrap();
    let source_head = source_repo.head().unwrap().peel_to_commit().unwrap().id();
    let candidate_repo = Repository::open(&candidate).unwrap();
    let candidate_head = candidate_repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(candidate_head.parent_id(0).unwrap(), source_head);
    assert!(candidate_repo
        .graph_descendant_of(candidate_head.id(), source_head)
        .unwrap());
    assert_eq!(candidate_repo.remotes().unwrap().len(), 0);
    let candidate_config = fs::read_to_string(candidate.join(".git").join("config")).unwrap();
    assert!(!candidate_config.contains("snapshot-user"));
    assert!(!candidate_config.contains("snapshot-secret"));
    assert!(candidate_repo.statuses(None).unwrap().is_empty());
    assert_eq!(
        source_repo.head().unwrap().peel_to_commit().unwrap().id(),
        source_head
    );
    assert!(source_repo.statuses(None).unwrap().is_empty());
    assert_eq!(
        source_repo.find_remote("origin").unwrap().url(),
        Some("https://snapshot-user:snapshot-secret@example.invalid/source.git")
    );
    assert_eq!(
        source_repo
            .config()
            .unwrap()
            .get_string("assemblywright.snapshot-secret")
            .unwrap(),
        "must-not-copy"
    );
    assert_eq!(
        fs::read_to_string(fixture.repo.join("src.py")).unwrap(),
        "ORIGINAL = True\n"
    );
    let run = fixture
        .state
        .join("runs")
        .join(report["run_id"].as_str().unwrap());
    let roles: Value =
        serde_json::from_slice(&fs::read(run.join("artifacts/model-roles.json")).unwrap()).unwrap();
    assert_eq!(roles["implementation"]["reasoning_budget_tokens"], 12_288);
    assert_eq!(roles["review"]["reasoning_budget_tokens"], 16_384);
    assert_eq!(roles["generation_policy"]["connect_timeout_seconds"], 10);
    assert_eq!(roles["generation_policy"]["total_timeout_seconds"], 900);
    assert_eq!(roles["generation_policy"]["max_tokens"], 32_768);
    assert_eq!(
        roles["generation_policy"]["response_format"]["type"],
        "json_object"
    );
    assert_eq!(
        roles["generation_policy"]["cancellation_poll_milliseconds"],
        150
    );
    assert!(run.join("submitted-job.json").is_file());
    assert_eq!(
        fs::read_to_string(run.join("source-head.txt"))
            .unwrap()
            .trim()
            .len(),
        40
    );
}

#[cfg(feature = "local-chatgpt-test-fixture")]
#[test]
fn chatgpt_planner_and_reviewer_are_frozen_while_local_model_implements() {
    let fixture = Fixture::new();
    let server = FakeModel::start(vec![edits("src.py", "GOOD = 'cloud-reviewed'\n")]);
    let mut job = fixture.job(server.endpoint(), 1, 30);
    enable_chatgpt_roles(&mut job);
    let provider = chatgpt_fixture(
        "Keep the change within src.py and satisfy the exact specification.",
        &review(true, &[]),
        false,
        None,
    );
    let output = fixture.run_with_chatgpt(&job, &provider);
    let requests = server.join();
    assert_success(&output);
    assert_eq!(requests.len(), 1, "only local implementation uses /v1/chat");
    assert!(requests[0].contains("Untrusted planning guidance"));
    assert!(requests[0].contains("Keep the change within src.py"));
    let report = fixture.report();
    let run = fixture
        .state
        .join("runs")
        .join(report["run_id"].as_str().unwrap());
    let roles: Value =
        serde_json::from_slice(&fs::read(run.join("artifacts/model-roles.json")).unwrap()).unwrap();
    assert_eq!(roles["orchestrator"]["provider"], "chatgpt");
    assert_eq!(roles["orchestrator"]["model"], "gpt-plan");
    assert_eq!(roles["review"]["provider"], "chatgpt");
    assert_eq!(roles["review"]["model"], "gpt-review");
    assert!(roles["review"].get("reasoning_budget_tokens").is_none());
    assert!(run.join("artifacts/orchestrator-request.json").exists());
    assert!(run.join("artifacts/orchestrator-response.json").exists());
    assert!(run.join("artifacts/attempt-1-review-request.json").exists());
    assert!(run
        .join("artifacts/attempt-1-review-response.json")
        .exists());
}

#[cfg(feature = "local-chatgpt-test-fixture")]
#[test]
fn chatgpt_rejection_retries_local_implementation_without_provider_fallback() {
    let fixture = Fixture::new();
    let server = FakeModel::start(vec![
        edits("src.py", "GOOD = 'first'\n"),
        edits("src.py", "GOOD = 'second'\n"),
    ]);
    let mut job = fixture.job(server.endpoint(), 2, 30);
    enable_chatgpt_roles(&mut job);
    let provider = chatgpt_fixture(
        "Implement only the requested file.",
        &review(false, &["candidate does not meet the specification"]),
        false,
        None,
    );
    let output = fixture.run_with_chatgpt(&job, &provider);
    let requests = server.join();
    assert!(!output.status.success());
    assert_eq!(requests.len(), 2);
    assert!(requests[1].contains("Independent review rejected"));
    let report = fixture.report();
    assert_eq!(report["stage"], "attempts_exhausted");
}

#[cfg(feature = "local-chatgpt-test-fixture")]
#[test]
fn malformed_chatgpt_output_fails_planning_without_local_fallback() {
    let fixture = Fixture::new();
    let server = FakeModel::start(Vec::new());
    let mut job = fixture.job(server.endpoint(), 1, 30);
    enable_chatgpt_roles(&mut job);
    let provider = chatgpt_fixture("unused", &review(true, &[]), true, None);
    let output = fixture.run_with_chatgpt(&job, &provider);
    server.join();
    assert!(!output.status.success());
    let report = fixture.report();
    assert_eq!(report["stage"], "planning");
    assert!(report["message"]
        .as_str()
        .unwrap()
        .contains("without fallback"));
}

#[cfg(feature = "local-chatgpt-test-fixture")]
#[test]
fn unsupported_chatgpt_model_effort_fails_before_local_implementation() {
    let fixture = Fixture::new();
    let server = FakeModel::start(Vec::new());
    let mut job = fixture.job(server.endpoint(), 1, 30);
    enable_chatgpt_roles(&mut job);
    job["orchestrator_reasoning_effort"] = json!("ultra");
    let provider = chatgpt_fixture("unused", &review(true, &[]), false, None);
    let output = fixture.run_with_chatgpt(&job, &provider);
    server.join();
    assert!(!output.status.success());
    let report = fixture.report();
    assert_eq!(report["stage"], "chatgpt_catalog");
    assert!(report["message"]
        .as_str()
        .unwrap()
        .contains("without fallback"));
}

#[cfg(feature = "local-chatgpt-test-fixture")]
#[test]
fn malformed_chatgpt_review_fails_without_local_reviewer_fallback() {
    let fixture = Fixture::new();
    let server = FakeModel::start(vec![edits("src.py", "GOOD = 'candidate'\n")]);
    let mut job = fixture.job(server.endpoint(), 2, 30);
    enable_chatgpt_roles(&mut job);
    let mut provider = chatgpt_fixture("Implement exactly.", &review(true, &[]), false, None);
    provider["malformed_stage"] = json!("review");
    let output = fixture.run_with_chatgpt(&job, &provider);
    let requests = server.join();
    assert!(!output.status.success());
    assert_eq!(
        requests.len(),
        1,
        "malformed cloud review must not fall back"
    );
    let report = fixture.report();
    assert_eq!(report["stage"], "review");
    assert!(report["message"]
        .as_str()
        .unwrap()
        .contains("without fallback"));
}

#[cfg(feature = "local-chatgpt-test-fixture")]
#[test]
fn cancellation_terminates_contained_chatgpt_planning_process() {
    let fixture = Fixture::new();
    let server = FakeModel::start(Vec::new());
    let mut job = fixture.job(server.endpoint(), 1, 30);
    enable_chatgpt_roles(&mut job);
    let provider = chatgpt_fixture("unused", &review(true, &[]), false, Some("planning"));
    let mut child = fixture.spawn_with_chatgpt(&job, &provider);
    wait_for_stage(&fixture, "planning");
    assert!(Command::new(binary())
        .args(["cancel", "--state-dir"])
        .arg(&fixture.state)
        .output()
        .unwrap()
        .status
        .success());
    assert!(!wait_for_child(&mut child).success());
    server.join();
    let report = fixture.report();
    assert_eq!(report["status"], "cancelled");
    assert_eq!(report["stage"], "cancelled");
}

#[cfg(feature = "local-chatgpt-test-fixture")]
#[test]
fn emergency_pause_terminates_contained_chatgpt_review_process() {
    let fixture = Fixture::new();
    let server = FakeModel::start(vec![edits("src.py", "GOOD = 'waiting-review'\n")]);
    let mut job = fixture.job(server.endpoint(), 1, 30);
    enable_chatgpt_roles(&mut job);
    let provider = chatgpt_fixture(
        "Implement exactly.",
        &review(true, &[]),
        false,
        Some("review"),
    );
    let mut child = fixture.spawn_with_chatgpt(&job, &provider);
    wait_for_stage(&fixture, "review");
    assert!(Command::new(binary())
        .args(["pause", "--state-dir"])
        .arg(&fixture.state)
        .output()
        .unwrap()
        .status
        .success());
    assert!(!wait_for_child(&mut child).success());
    server.join();
    let report = fixture.report();
    assert_eq!(report["status"], "cancelled");
    assert_eq!(report["stage"], "paused");
}

#[test]
fn none_effort_uses_deepseek_extraction_without_enabling_thinking_for_both_roles() {
    let fixture = Fixture::new();
    let server = FakeModel::start(vec![
        edits("src.py", "GOOD = 'no thinking'\n"),
        review(true, &[]),
    ]);

    let output = fixture.run(&fixture.job(server.endpoint(), 1, 30));
    let requests = server.join();

    assert_success(&output);
    assert_eq!(requests.len(), 2);
    for request in &requests {
        let request: Value = serde_json::from_str(request).unwrap();
        assert_eq!(request["reasoning_effort"], "none");
        assert_eq!(request["reasoning_budget_tokens"], 0);
        assert_eq!(request["reasoning_format"], "deepseek");
        assert_eq!(request["chat_template_kwargs"]["enable_thinking"], false);
    }

    let report = fixture.report();
    let run = fixture
        .state
        .join("runs")
        .join(report["run_id"].as_str().unwrap());
    let roles: Value =
        serde_json::from_slice(&fs::read(run.join("artifacts/model-roles.json")).unwrap()).unwrap();
    for role in ["implementation", "review"] {
        assert_eq!(roles[role]["reasoning_effort"], "none");
        assert_eq!(roles[role]["reasoning_budget_tokens"], 0);
        assert_eq!(roles[role]["reasoning_format"], "deepseek");
    }
}

#[test]
fn confinement_compares_candidate_with_exact_source_head_not_historical_root() {
    let fixture = Fixture::new();
    fs::write(
        fixture.repo.join("styles.css"),
        "body { background: historical; }\n",
    )
    .unwrap();
    fixture.commit_all("historical non-allowlisted style change");
    let source_repo = Repository::open(&fixture.repo).unwrap();
    let source_head = source_repo.head().unwrap().peel_to_commit().unwrap().id();
    let styles_blob = source_repo
        .find_blob(
            source_repo
                .head()
                .unwrap()
                .peel_to_tree()
                .unwrap()
                .get_path(Path::new("styles.css"))
                .unwrap()
                .id(),
        )
        .unwrap()
        .content()
        .to_vec();

    let server = FakeModel::start(vec![
        edits("src.py", "GOOD = 'exact source base'\n"),
        review(true, &[]),
    ]);
    let output = fixture.run(&fixture.job(server.endpoint(), 1, 30));
    server.join();

    assert_success(&output);
    let report = fixture.report();
    assert_eq!(report["status"], "completed");
    let candidate = PathBuf::from(report["candidate"].as_str().unwrap());
    let candidate_repo = Repository::open(candidate).unwrap();
    let candidate_head = candidate_repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(candidate_head.parent_id(0).unwrap(), source_head);
    assert_eq!(
        candidate_repo
            .find_blob(
                candidate_head
                    .tree()
                    .unwrap()
                    .get_path(Path::new("styles.css"))
                    .unwrap()
                    .id(),
            )
            .unwrap()
            .content(),
        styles_blob
    );
}

#[test]
fn confinement_still_rejects_an_actual_non_allowlisted_model_edit() {
    let fixture = Fixture::new();
    fs::write(fixture.repo.join("styles.css"), "body { color: black; }\n").unwrap();
    fixture.commit_all("add non-allowlisted style");
    let server = FakeModel::start(vec![edits("styles.css", "body { color: unauthorized; }\n")]);

    let output = fixture.run(&fixture.job(server.endpoint(), 1, 30));
    server.join();

    assert!(!output.status.success());
    let report = fixture.report();
    assert_eq!(report["status"], "failed");
    assert_eq!(report["stage"], "attempts_exhausted");
    assert!(report["message"]
        .as_str()
        .unwrap()
        .contains("path is outside allowed_files: styles.css"));
    assert_eq!(
        fs::read_to_string(fixture.repo.join("styles.css")).unwrap(),
        "body { color: black; }\n"
    );
}

#[test]
fn cancel_and_pause_during_snapshot_stop_candidate_writes_before_inference() {
    for action in ["cancel", "pause"] {
        let fixture = Fixture::new();
        fixture.add_snapshot_files(1_500, 2 * 1024);
        let probe = ConnectionProbe::start();
        let job = fixture.job(probe.endpoint(), 1, 30);
        let mut child = fixture.spawn(&job);
        wait_for_stage(&fixture, "snapshot");

        let output = Command::new(binary())
            .arg(action)
            .arg("--state-dir")
            .arg(&fixture.state)
            .output()
            .unwrap();
        assert_success(&output);
        let candidate = PathBuf::from(fixture.report()["candidate"].as_str().unwrap());
        let shape_after_control_returned = tree_shape(&candidate);
        thread::sleep(Duration::from_millis(150));
        assert_eq!(tree_shape(&candidate), shape_after_control_returned);

        let status = wait_for_child(&mut child);
        assert!(!status.success());
        let report = fixture.report();
        assert_eq!(report["status"], "cancelled");
        assert_eq!(
            report["stage"],
            if action == "cancel" {
                "cancelled"
            } else {
                "paused"
            }
        );
        assert_eq!(probe.finish(), 0, "snapshot stop must precede inference");
    }
}

#[test]
fn oversized_snapshot_object_fails_before_decompression_or_inference() {
    let fixture = Fixture::new();
    fixture.add_sparse_file_and_commit("oversized.bin", 64 * 1024 * 1024 + 1);
    let probe = ConnectionProbe::start();
    let output = fixture.run(&fixture.job(probe.endpoint(), 1, 30));

    assert!(!output.status.success());
    let report = fixture.report();
    assert_eq!(report["status"], "failed");
    assert_eq!(report["stage"], "snapshot");
    assert!(report["message"]
        .as_str()
        .unwrap()
        .contains("snapshot object is 67108865 bytes; limit is 67108864 bytes"));
    assert_eq!(probe.finish(), 0, "object bound must precede inference");
}

#[test]
fn repair_prompt_prioritizes_failure_and_skips_exact_noop_validation() {
    let fixture = Fixture::new();
    let failing = "BROKEN = True\n";
    let server = FakeModel::start(vec![
        edits("src.py", failing),
        edits("src.py", failing),
        edits("src.py", "GOOD = 'repaired'\n"),
        review(true, &[]),
    ]);
    let output = fixture.run(&fixture.job(server.endpoint(), 3, 30));
    let requests = server.join();

    assert_success(&output);
    assert_eq!(requests.len(), 4);
    assert!(requests[1].contains("REPAIR THIS FAILURE FIRST"));
    assert!(requests[1].contains("Validation failed"));
    assert!(requests[1].contains("Repair the failure stated at the start"));
    assert!(requests[2].contains("Validation failed"));
    assert!(requests[2].contains("made no byte-level changes"));
    assert!(requests[2].contains("Do not repeat unchanged failing code"));
    let report = fixture.report();
    let run = fixture
        .state
        .join("runs")
        .join(report["run_id"].as_str().unwrap());
    assert!(run.join("artifacts/attempt-1-gate.json").exists());
    assert!(!run.join("artifacts/attempt-2-gate.json").exists());
    assert!(run.join("artifacts/attempt-3-gate.json").exists());
}

#[test]
fn oversized_content_length_and_chunked_catalog_and_model_bodies_fail_bounded() {
    let catalog_fixture = Fixture::new();
    let catalog_server = OversizedHttpServer::start(OversizedTarget::CatalogContentLength);
    let catalog_output =
        catalog_fixture.run(&catalog_fixture.job(catalog_server.endpoint(), 1, 30));
    let catalog_requests = catalog_server.join();
    assert!(!catalog_output.status.success());
    assert_eq!(catalog_requests.len(), 1);
    assert!(catalog_requests[0].starts_with("GET /v1/models "));
    let catalog_report = catalog_fixture.report();
    assert_eq!(catalog_report["stage"], "model_catalog");
    assert!(catalog_report["message"]
        .as_str()
        .unwrap()
        .contains("local model catalog exceeds 1048576 bytes"));

    let chunked_catalog_fixture = Fixture::new();
    let chunked_catalog_server = OversizedHttpServer::start(OversizedTarget::CatalogChunked);
    let chunked_catalog_output = chunked_catalog_fixture.run(&chunked_catalog_fixture.job(
        chunked_catalog_server.endpoint(),
        1,
        30,
    ));
    let chunked_catalog_requests = chunked_catalog_server.join();
    assert!(!chunked_catalog_output.status.success());
    assert_eq!(chunked_catalog_requests.len(), 1);
    assert!(chunked_catalog_requests[0].starts_with("GET /v1/models "));
    assert!(chunked_catalog_fixture.report()["message"]
        .as_str()
        .unwrap()
        .contains("local model catalog exceeds 1048576 bytes"));

    let model_fixture = Fixture::new();
    let model_server = OversizedHttpServer::start(OversizedTarget::ModelChunked);
    let model_output = model_fixture.run(&model_fixture.job(model_server.endpoint(), 1, 30));
    let model_requests = model_server.join();
    assert!(!model_output.status.success());
    assert_eq!(model_requests.len(), 2);
    assert!(model_requests[1].starts_with("POST /v1/chat/completions "));
    let model_report = model_fixture.report();
    assert_eq!(model_report["stage"], "attempts_exhausted");
    assert!(model_report["message"]
        .as_str()
        .unwrap()
        .contains("local model response exceeds 4194304 bytes"));
}

#[test]
fn permissions_are_revision_bound_and_blocked_by_pause_and_active_runner() {
    let fixture = Fixture::new();
    let initial = fixture.permissions_get();
    assert_eq!(initial["mode"], "full");
    assert_eq!(initial["revision"], 0);

    let changed = fixture.permissions_set("ask", 0);
    assert_success(&changed);
    let current: Value = serde_json::from_slice(&changed.stdout).unwrap();
    assert_eq!(current["mode"], "ask");
    assert_eq!(current["revision"], 1);
    let stale = fixture.permissions_set("auto", 0);
    assert!(!stale.status.success());
    assert!(String::from_utf8_lossy(&stale.stderr).contains("revision conflict"));

    assert!(Command::new(binary())
        .args(["pause", "--state-dir"])
        .arg(&fixture.state)
        .output()
        .unwrap()
        .status
        .success());
    let paused = fixture.permissions_set("full", 1);
    assert!(!paused.status.success());
    assert!(String::from_utf8_lossy(&paused.stderr).contains("Emergency Pause"));
    assert!(Command::new(binary())
        .args(["resume", "--state-dir"])
        .arg(&fixture.state)
        .output()
        .unwrap()
        .status
        .success());

    let runner_lock = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(fixture.state.join("runner.lock"))
        .unwrap();
    runner_lock.try_lock_exclusive().unwrap();
    let active = fixture.permissions_set("full", 1);
    assert!(!active.status.success());
    assert!(String::from_utf8_lossy(&active.stderr).contains("pipeline is active"));
    FileExt::unlock(&runner_lock).unwrap();

    fs::create_dir_all(fixture.state.join("github")).unwrap();
    let publication_lock = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(fixture.state.join("github/publication.lock"))
        .unwrap();
    publication_lock.try_lock_exclusive().unwrap();
    let publishing = fixture.permissions_set("full", 1);
    assert!(!publishing.status.success());
    assert!(String::from_utf8_lossy(&publishing.stderr).contains("publication is active"));
    FileExt::unlock(&publication_lock).unwrap();

    let audit: Value =
        serde_json::from_slice(&fs::read(fixture.state.join("permissions-audit.json")).unwrap())
            .unwrap();
    assert_eq!(audit.as_array().unwrap().len(), 1);
}

#[test]
fn invalid_model_role_settings_fail_before_inference() {
    let fixture = Fixture::new();
    let mut job = fixture.job("http://127.0.0.1:9/v1".into(), 1, 30);
    job["reviewer_model"] = json!("");
    let output = fixture.run(&job);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("reviewer_model"));

    job["reviewer_model"] = Value::Null;
    job["reasoning_effort"] = json!("unbounded");
    let output = fixture.run(&job);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("reasoning effort"));
}

#[test]
fn advertised_model_reasoning_efforts_are_enforced() {
    let fixture = Fixture::new();
    let server = FakeModel::start_with_catalog(
        Vec::new(),
        json!({"data":[{"id":"fixture-model","reasoning_efforts":["none","low"]}]}),
    );
    let mut job = fixture.job(server.endpoint(), 1, 30);
    job["reasoning_effort"] = json!("max");
    let output = fixture.run(&job);
    server.join();
    assert!(!output.status.success());
    let report = fixture.report();
    assert_eq!(report["stage"], "model_catalog");
    assert!(report["message"]
        .as_str()
        .unwrap()
        .contains("does not advertise reasoning effort max"));
}

#[test]
fn malformed_legacy_permission_audit_cannot_split_authority_from_audit() {
    let fixture = Fixture::new();
    fixture.permissions_get();
    fs::write(
        fixture.state.join("permissions-audit.json"),
        b"malformed legacy diagnostic mirror",
    )
    .unwrap();
    assert_success(&fixture.permissions_set("auto", 0));
    let permissions = fixture.permissions_get();
    assert_eq!(permissions["mode"], "auto");
    assert_eq!(permissions["revision"], 1);
    assert_eq!(permissions["audit"].as_array().unwrap().len(), 1);
    assert_eq!(permissions["audit"][0]["mode"], "auto");
}

#[test]
fn ask_mode_requires_exact_approvals_for_edits_and_validation() {
    let fixture = Fixture::new();
    assert_success(&fixture.permissions_set("ask", 0));
    let server = FakeModel::start(vec![
        edits("src.py", "GOOD = 'approved'\n"),
        review(true, &[]),
    ]);
    let mut child = fixture.spawn(&fixture.job(server.endpoint(), 1, 30));

    let edits_approval = wait_for_approval(&fixture, "apply_model_edits");
    assert_eq!(edits_approval["digest"].as_str().unwrap().len(), 64);
    fixture.decide(&edits_approval, true);
    let validation_approval = wait_for_approval(&fixture, "run_validation");
    assert_ne!(validation_approval["id"], edits_approval["id"]);
    assert_eq!(validation_approval["digest"].as_str().unwrap().len(), 64);
    fixture.decide(&validation_approval, true);

    assert!(child.wait().unwrap().success());
    server.join();
    assert_eq!(fixture.report()["status"], "completed");
    assert!(!fixture.state.join("pending-approval.json").exists());
}

#[test]
fn auto_mode_owner_rejection_fails_before_validator_execution() {
    let fixture = Fixture::new();
    assert_success(&fixture.permissions_set("auto", 0));
    let server = FakeModel::start(vec![edits("src.py", "GOOD = 'rejected'\n")]);
    let mut child = fixture.spawn(&fixture.job(server.endpoint(), 1, 30));
    let pending = wait_for_approval(&fixture, "run_validation");
    fixture.decide(&pending, false);
    assert!(!child.wait().unwrap().success());
    server.join();
    let report = fixture.report();
    assert_eq!(report["status"], "failed");
    assert_eq!(report["stage"], "approval_rejected");
    assert!(!fixture.state.join("pending-approval.json").exists());
}

#[test]
fn cancellation_wins_while_owner_approval_is_pending() {
    let fixture = Fixture::new();
    assert_success(&fixture.permissions_set("ask", 0));
    let server = FakeModel::start(vec![edits("src.py", "GOOD = 'cancelled'\n")]);
    let mut child = fixture.spawn(&fixture.job(server.endpoint(), 1, 30));
    let pending = wait_for_approval(&fixture, "apply_model_edits");
    assert!(Command::new(binary())
        .args(["cancel", "--state-dir"])
        .arg(&fixture.state)
        .output()
        .unwrap()
        .status
        .success());
    let stale = approval_decision(&fixture, &pending, true);
    assert!(!stale.status.success());
    assert!(!child.wait().unwrap().success());
    server.join();
    assert_eq!(fixture.report()["status"], "cancelled");
    assert!(!fixture.state.join("pending-approval.json").exists());
}

#[test]
fn corrupted_pending_approval_cannot_authorize_a_different_effect() {
    let fixture = Fixture::new();
    assert_success(&fixture.permissions_set("ask", 0));
    let server = FakeModel::start(vec![edits("src.py", "GOOD = 'corrupt'\n")]);
    let mut child = fixture.spawn(&fixture.job(server.endpoint(), 1, 30));
    let mut pending = wait_for_approval(&fixture, "apply_model_edits");
    pending.as_object_mut().unwrap().remove("status");
    pending["action"] = json!("run_validation");
    pending["digest"] = json!("0".repeat(64));
    pending["detail"] = json!("Run an unrelated command");
    pending["requested_at_ms"] = json!(1);
    fs::write(
        fixture.state.join("pending-approval.json"),
        serde_json::to_vec_pretty(&pending).unwrap(),
    )
    .unwrap();
    let decide = approval_decision(&fixture, &pending, true);
    assert!(!decide.status.success());
    let _ = child.wait().unwrap();
    server.join();
    let report = fixture.report();
    assert_eq!(report["status"], "failed");
    let candidate = PathBuf::from(report["candidate"].as_str().unwrap());
    assert_eq!(
        fs::read_to_string(candidate.join("src.py")).unwrap(),
        "ORIGINAL = True\n"
    );
}

#[test]
fn cancellation_committed_after_approval_wins_before_edit_effect() {
    let fixture = Fixture::new();
    assert_success(&fixture.permissions_set("ask", 0));
    let server = FakeModel::start(vec![edits("src.py", "GOOD = 'too late'\n")]);
    let mut child = fixture.spawn(&fixture.job(server.endpoint(), 1, 30));
    let mut pending = wait_for_approval(&fixture, "apply_model_edits");
    pending.as_object_mut().unwrap().remove("status");
    let control = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(fixture.state.join("control.lock"))
        .unwrap();
    control.lock_exclusive().unwrap();
    pending["approved"] = json!(true);
    pending["decided_at_ms"] = json!(pending["requested_at_ms"].as_u64().unwrap() + 1);
    fs::write(
        fixture.state.join("pending-approval.json"),
        serde_json::to_vec_pretty(&pending).unwrap(),
    )
    .unwrap();
    fs::write(fixture.state.join("cancel.flag"), b"test cancellation\n").unwrap();
    FileExt::unlock(&control).unwrap();

    assert!(!child.wait().unwrap().success());
    server.join();
    let report = fixture.report();
    assert_eq!(report["status"], "cancelled");
    let candidate = PathBuf::from(report["candidate"].as_str().unwrap());
    assert_eq!(
        fs::read_to_string(candidate.join("src.py")).unwrap(),
        "ORIGINAL = True\n"
    );
}

#[test]
fn cancellation_committed_after_approval_wins_before_validator_permission() {
    let fixture = Fixture::new();
    assert_success(&fixture.permissions_set("auto", 0));
    let server = FakeModel::start(vec![edits("src.py", "GOOD = 'candidate'\n")]);
    let mut job = fixture.job(server.endpoint(), 1, 30);
    job["validation"]["args"] = json!([
        "--exact",
        "validator_side_effect_fixture",
        "--ignored",
        "--nocapture"
    ]);
    let mut child = fixture.spawn(&job);
    let mut pending = wait_for_approval(&fixture, "run_validation");
    pending.as_object_mut().unwrap().remove("status");
    let control = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(fixture.state.join("control.lock"))
        .unwrap();
    control.lock_exclusive().unwrap();
    pending["approved"] = json!(true);
    pending["decided_at_ms"] = json!(pending["requested_at_ms"].as_u64().unwrap() + 1);
    fs::write(
        fixture.state.join("pending-approval.json"),
        serde_json::to_vec_pretty(&pending).unwrap(),
    )
    .unwrap();
    fs::write(fixture.state.join("cancel.flag"), b"test cancellation\n").unwrap();
    FileExt::unlock(&control).unwrap();

    assert!(!child.wait().unwrap().success());
    server.join();
    let report = fixture.report();
    assert_eq!(report["status"], "cancelled");
    let candidate = PathBuf::from(report["candidate"].as_str().unwrap());
    assert!(!candidate.join("validator-ran.txt").exists());
}

#[test]
fn validation_failure_is_returned_to_model_and_repaired() {
    let fixture = Fixture::new();
    let server = FakeModel::start(vec![
        edits("src.py", "BAD = True\n"),
        edits("src.py", "GOOD = 'repaired'\n"),
        review(true, &[]),
    ]);
    let output = fixture.run(&fixture.job(server.endpoint(), 3, 30));
    let requests = server.join();

    assert_success(&output);
    assert!(requests[1].contains("Validation failed"));
    assert!(requests[1].contains("expected GOOD marker"));
    assert_eq!(fixture.report()["attempt"], 2);
}

#[test]
fn review_rejection_requires_a_new_implementation_and_fresh_review() {
    let fixture = Fixture::new();
    let server = FakeModel::start(vec![
        edits("src.py", "GOOD = 'first'\n"),
        review(false, &["implementation is incomplete"]),
        edits("src.py", "GOOD = 'second'\n"),
        review(true, &[]),
    ]);
    let output = fixture.run(&fixture.job(server.endpoint(), 3, 30));
    let requests = server.join();

    assert_success(&output);
    assert!(requests[2].contains("Independent review rejected"));
    assert_eq!(fixture.report()["attempt"], 2);
}

#[test]
fn traversal_edit_is_rejected_then_repaired_without_touching_outside_file() {
    let fixture = Fixture::new();
    let outside = fixture.root.path().join("escape.py");
    let server = FakeModel::start(vec![
        edits("../escape.py", "owned\n"),
        edits("src.py", "GOOD = 'safe'\n"),
        review(true, &[]),
    ]);
    let output = fixture.run(&fixture.job(server.endpoint(), 2, 30));
    server.join();

    assert_success(&output);
    assert!(!outside.exists());
    assert_eq!(fixture.report()["attempt"], 2);
}

#[test]
fn endpoint_unavailable_and_validator_timeout_end_failed_not_completed() {
    let unavailable = Fixture::new();
    let socket = TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap();
    let output = unavailable.run(&unavailable.job(format!("http://{socket}/v1"), 1, 30));
    assert!(!output.status.success());
    assert_eq!(unavailable.report()["status"], "failed");

    let timeout = Fixture::new();
    let server = FakeModel::start(vec![edits("src.py", "GOOD = 'timeout'\n")]);
    let mut job = timeout.job(server.endpoint(), 1, 1);
    job["validation"]["args"] = json!([
        "--exact",
        "validator_timeout_fixture",
        "--ignored",
        "--nocapture"
    ]);
    let output = timeout.run(&job);
    server.join();
    assert!(!output.status.success());
    let report = timeout.report();
    assert_eq!(report["status"], "failed");
    let run = timeout
        .state
        .join("runs")
        .join(report["run_id"].as_str().unwrap());
    let gate: Value =
        serde_json::from_slice(&fs::read(run.join("artifacts/attempt-1-gate.json")).unwrap())
            .unwrap();
    assert_eq!(gate["timed_out"], true);
}

#[test]
fn validator_cannot_mutate_the_frozen_candidate() {
    let fixture = Fixture::new();
    let server = FakeModel::start(vec![edits("src.py", "GOOD = 'frozen'\n")]);
    let mut job = fixture.job(server.endpoint(), 1, 30);
    job["validation"]["args"] = json!([
        "--exact",
        "validator_mutation_fixture",
        "--ignored",
        "--nocapture"
    ]);
    let output = fixture.run(&job);
    server.join();
    assert!(!output.status.success());
    let report = fixture.report();
    assert_eq!(report["status"], "failed");
    assert!(report["message"]
        .as_str()
        .unwrap()
        .contains("mutated frozen tracked files"));
}

#[test]
fn validator_cannot_leave_nonignored_untracked_output() {
    let fixture = Fixture::new();
    let server = FakeModel::start(vec![edits("src.py", "GOOD = 'frozen'\n")]);
    let mut job = fixture.job(server.endpoint(), 1, 30);
    job["validation"]["args"] = json!([
        "--exact",
        "validator_untracked_fixture",
        "--ignored",
        "--nocapture"
    ]);
    let output = fixture.run(&job);
    server.join();
    assert!(!output.status.success());
    let report = fixture.report();
    assert_eq!(report["status"], "failed");
    assert!(report["message"]
        .as_str()
        .unwrap()
        .contains("nonignored untracked"));
}

#[test]
fn cancellation_arriving_with_review_response_wins_over_completion() {
    let fixture = Fixture::new();
    let server = FakeModel::start_with_hook(
        vec![edits("src.py", "GOOD = 'reviewed'\n"), review(true, &[])],
        Some((1, fixture.state.join("cancel.flag"))),
    );
    let output = fixture.run(&fixture.job(server.endpoint(), 1, 30));
    server.join();
    assert!(!output.status.success());
    let report = fixture.report();
    assert_eq!(report["status"], "cancelled");
    assert_ne!(report["stage"], "completed");
    assert!(fixture.state.join("cancel.flag").exists());
}

#[test]
fn validator_descendant_is_terminated_and_rejects_the_gate() {
    let fixture = Fixture::new();
    let server = FakeModel::start(vec![edits("src.py", "GOOD = 'tree'\n")]);
    let mut job = fixture.job(server.endpoint(), 1, 30);
    job["validation"]["args"] = json!([
        "--exact",
        "validator_descendant_fixture",
        "--ignored",
        "--nocapture"
    ]);
    let output = fixture.run(&job);
    server.join();
    assert!(!output.status.success());
    let report = fixture.report();
    let run = fixture
        .state
        .join("runs")
        .join(report["run_id"].as_str().unwrap());
    let gate: Value =
        serde_json::from_slice(&fs::read(run.join("artifacts/attempt-1-gate.json")).unwrap())
            .unwrap();
    assert_eq!(gate["success"], false);
    assert!(gate["stderr"]
        .as_str()
        .unwrap()
        .contains("descendant process"));
    let pid: u32 = fs::read_to_string(run.join("candidate/target/grandchild.pid"))
        .unwrap()
        .parse()
        .unwrap();
    assert_process_exits(pid);
}

#[test]
fn runner_crash_kills_validator_tree_and_status_reconciles_interrupted() {
    let fixture = Fixture::new();
    let server = FakeModel::start(vec![edits("src.py", "GOOD = 'crash'\n")]);
    let mut job = fixture.job(server.endpoint(), 1, 30);
    job["validation"]["args"] = json!([
        "--exact",
        "validator_tree_wait_fixture",
        "--ignored",
        "--nocapture"
    ]);
    fs::write(&fixture.job_path, serde_json::to_vec_pretty(&job).unwrap()).unwrap();
    let mut runner = Command::new(binary())
        .args(["run", "--job"])
        .arg(&fixture.job_path)
        .arg("--state-dir")
        .arg(&fixture.state)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    let pids_path = wait_for_candidate_file(&fixture, "target/tree-pids.txt");
    let pids = fs::read_to_string(&pids_path)
        .unwrap()
        .lines()
        .map(|line| line.parse::<u32>().unwrap())
        .collect::<Vec<_>>();
    runner.kill().unwrap();
    runner.wait().unwrap();
    for pid in pids {
        assert_process_exits(pid);
    }
    let status = Command::new(binary())
        .args(["status", "--state-dir"])
        .arg(&fixture.state)
        .output()
        .unwrap();
    server.join();
    assert!(status.status.success());
    assert_eq!(fixture.report()["status"], "interrupted");
}

#[test]
fn owner_cancel_stops_an_active_validator_and_never_reports_completed() {
    let fixture = Fixture::new();
    let server = FakeModel::start(vec![edits("src.py", "GOOD = 'cancel'\n")]);
    let mut job = fixture.job(server.endpoint(), 1, 30);
    job["validation"]["args"] = json!([
        "--exact",
        "validator_timeout_fixture",
        "--ignored",
        "--nocapture"
    ]);
    fs::write(&fixture.job_path, serde_json::to_vec_pretty(&job).unwrap()).unwrap();
    let mut child = Command::new(binary())
        .args(["run", "--job"])
        .arg(&fixture.job_path)
        .arg("--state-dir")
        .arg(&fixture.state)
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        if fixture.state.join("latest.json").exists() && fixture.report()["stage"] == "validation" {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "validation did not start"
        );
        thread::sleep(Duration::from_millis(50));
    }
    let cancel = Command::new(binary())
        .args(["cancel", "--state-dir"])
        .arg(&fixture.state)
        .output()
        .unwrap();
    assert!(cancel.status.success());
    assert!(!child.wait().unwrap().success());
    server.join();
    let report = fixture.report();
    assert_eq!(report["status"], "cancelled");
    assert_ne!(report["stage"], "completed");
}

#[test]
fn concurrent_run_rejection_does_not_rewrite_the_live_owner_report() {
    let fixture = Fixture::new();
    let server = FakeModel::start(vec![edits("src.py", "GOOD = 'active'\n")]);
    let mut job = fixture.job(server.endpoint(), 1, 30);
    job["validation"]["args"] = json!([
        "--exact",
        "validator_timeout_fixture",
        "--ignored",
        "--nocapture"
    ]);
    fs::write(&fixture.job_path, serde_json::to_vec_pretty(&job).unwrap()).unwrap();
    let mut owner = Command::new(binary())
        .args(["run", "--job"])
        .arg(&fixture.job_path)
        .arg("--state-dir")
        .arg(&fixture.state)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    wait_for_stage(&fixture, "validation");
    let live_run_id = fixture.report()["run_id"].as_str().unwrap().to_string();

    let rejected = fixture.run(&job);
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("another local pipeline"));
    let report = fixture.report();
    assert_eq!(report["run_id"], live_run_id);
    assert_eq!(report["status"], "running");
    assert_eq!(report["stage"], "validation");

    let observed = Command::new(binary())
        .args(["status", "--state-dir"])
        .arg(&fixture.state)
        .output()
        .unwrap();
    assert!(observed.status.success());
    assert_eq!(fixture.report()["status"], "running");
    Command::new(binary())
        .args(["cancel", "--state-dir"])
        .arg(&fixture.state)
        .output()
        .unwrap();
    assert!(!owner.wait().unwrap().success());
    server.join();
}

#[test]
fn state_lock_and_local_emergency_pause_fail_closed() {
    let fixture = Fixture::new();
    fs::create_dir_all(&fixture.state).unwrap();
    let lock = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(fixture.state.join("runner.lock"))
        .unwrap();
    lock.try_lock_exclusive().unwrap();
    let output = fixture.run(&json!({}));
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("another local pipeline"));
    FileExt::unlock(&lock).unwrap();

    assert!(Command::new(binary())
        .args(["pause", "--state-dir"])
        .arg(&fixture.state)
        .output()
        .unwrap()
        .status
        .success());
    let output = fixture.run(&json!({}));
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Emergency Pause"));
    assert!(Command::new(binary())
        .args(["resume", "--state-dir"])
        .arg(&fixture.state)
        .output()
        .unwrap()
        .status
        .success());
}

#[test]
#[ignore]
fn validator_success_fixture() {
    let content = fs::read_to_string("src.py").expect("candidate src.py");
    if !content.contains("GOOD") {
        eprintln!("expected GOOD marker");
        std::process::exit(7);
    }
}

#[test]
#[ignore]
fn validator_timeout_fixture() {
    thread::sleep(Duration::from_secs(10));
}

#[test]
#[ignore]
fn validator_mutation_fixture() {
    fs::write("src.py", "GOOD = 'validator changed this'\n").unwrap();
}

#[test]
#[ignore]
fn validator_untracked_fixture() {
    fs::write(
        "rogue-output.txt",
        "validator output outside ignored paths\n",
    )
    .unwrap();
}

#[test]
#[ignore]
fn validator_side_effect_fixture() {
    fs::write("validator-ran.txt", "validator started\n").unwrap();
}

#[test]
#[ignore]
#[allow(clippy::zombie_processes)] // Deliberately leave the child for Job Object descendant cleanup.
fn validator_descendant_fixture() {
    fs::create_dir_all("target").unwrap();
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "validator_grandchild_fixture",
            "--ignored",
            "--nocapture",
        ])
        .spawn()
        .unwrap();
    fs::write("target/grandchild.pid", child.id().to_string()).unwrap();
}

#[test]
#[ignore]
#[allow(clippy::zombie_processes)] // Deliberately keep the tree alive until the runner is killed.
fn validator_tree_wait_fixture() {
    fs::create_dir_all("target").unwrap();
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "validator_grandchild_fixture",
            "--ignored",
            "--nocapture",
        ])
        .spawn()
        .unwrap();
    fs::write(
        "target/tree-pids.txt",
        format!("{}\n{}\n", std::process::id(), child.id()),
    )
    .unwrap();
    thread::sleep(Duration::from_secs(30));
}

#[test]
#[ignore]
fn validator_grandchild_fixture() {
    thread::sleep(Duration::from_secs(30));
}

struct Fixture {
    root: TempDir,
    repo: PathBuf,
    state: PathBuf,
    job_path: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let repo = root.path().join("source");
        fs::create_dir_all(&repo).unwrap();
        fs::write(repo.join("src.py"), "ORIGINAL = True\n").unwrap();
        fs::write(repo.join(".gitignore"), "target/\n").unwrap();
        let git = Repository::init(&repo).unwrap();
        let mut index = git.index().unwrap();
        index.add_path(Path::new("src.py")).unwrap();
        index.add_path(Path::new(".gitignore")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = git.find_tree(tree_id).unwrap();
        let signature = Signature::now("Test", "test@example.invalid").unwrap();
        git.commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
            .unwrap();
        drop(tree);
        drop(git);
        let state = root.path().join("state");
        let job_path = root.path().join("job.json");
        Self {
            root,
            repo,
            state,
            job_path,
        }
    }

    fn job(&self, endpoint: String, attempts: u32, timeout: u64) -> Value {
        json!({
            "repository": self.repo,
            "specification": "Replace src.py with a working implementation containing GOOD.",
            "allowed_files": ["src.py"],
            "validation": {
                "program": std::env::current_exe().unwrap(),
                "args": ["--exact", "validator_success_fixture", "--ignored", "--nocapture"],
                "timeout_seconds": timeout
            },
            "endpoint": endpoint,
            "model": "fixture-model",
            "max_attempts": attempts
        })
    }

    fn add_snapshot_files(&self, count: usize, size: usize) {
        let content = vec![b'x'; size];
        for index in 0..count {
            fs::write(self.repo.join(format!("snapshot-{index:05}.dat")), &content).unwrap();
        }
        self.commit_all("snapshot load");
    }

    fn add_sparse_file_and_commit(&self, relative: &str, size: u64) {
        File::create(self.repo.join(relative))
            .unwrap()
            .set_len(size)
            .unwrap();
        self.commit_all("oversized snapshot object");
    }

    fn commit_all(&self, message: &str) {
        let repo = Repository::open(&self.repo).unwrap();
        let mut index = repo.index().unwrap();
        index
            .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
            .unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let parent = repo.head().unwrap().peel_to_commit().unwrap();
        let signature = Signature::now("Test", "test@example.invalid").unwrap();
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &[&parent],
        )
        .unwrap();
    }

    fn run(&self, job: &Value) -> Output {
        fs::write(&self.job_path, serde_json::to_vec_pretty(job).unwrap()).unwrap();
        Command::new(binary())
            .args(["run", "--job"])
            .arg(&self.job_path)
            .arg("--state-dir")
            .arg(&self.state)
            .output()
            .unwrap()
    }

    #[cfg(feature = "local-chatgpt-test-fixture")]
    fn run_with_chatgpt(&self, job: &Value, chatgpt_fixture: &Value) -> Output {
        let fixture_path = self.root.path().join("chatgpt-fixture.json");
        fs::write(
            &fixture_path,
            serde_json::to_vec_pretty(chatgpt_fixture).unwrap(),
        )
        .unwrap();
        fs::write(&self.job_path, serde_json::to_vec_pretty(job).unwrap()).unwrap();
        Command::new(binary())
            .args(["run", "--job"])
            .arg(&self.job_path)
            .arg("--state-dir")
            .arg(&self.state)
            .env("ASSEMBLYWRIGHT_CHATGPT_TEST_FIXTURE", fixture_path)
            .output()
            .unwrap()
    }

    fn spawn(&self, job: &Value) -> std::process::Child {
        fs::write(&self.job_path, serde_json::to_vec_pretty(job).unwrap()).unwrap();
        Command::new(binary())
            .args(["run", "--job"])
            .arg(&self.job_path)
            .arg("--state-dir")
            .arg(&self.state)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap()
    }

    #[cfg(feature = "local-chatgpt-test-fixture")]
    fn spawn_with_chatgpt(&self, job: &Value, chatgpt_fixture: &Value) -> std::process::Child {
        let fixture_path = self.root.path().join("chatgpt-fixture.json");
        fs::write(
            &fixture_path,
            serde_json::to_vec_pretty(chatgpt_fixture).unwrap(),
        )
        .unwrap();
        fs::write(&self.job_path, serde_json::to_vec_pretty(job).unwrap()).unwrap();
        Command::new(binary())
            .args(["run", "--job"])
            .arg(&self.job_path)
            .arg("--state-dir")
            .arg(&self.state)
            .env("ASSEMBLYWRIGHT_CHATGPT_TEST_FIXTURE", fixture_path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap()
    }

    fn permissions_get(&self) -> Value {
        let output = Command::new(binary())
            .args(["permissions", "get", "--state-dir"])
            .arg(&self.state)
            .output()
            .unwrap();
        assert_success(&output);
        serde_json::from_slice(&output.stdout).unwrap()
    }

    fn permissions_set(&self, mode: &str, expected_revision: u64) -> Output {
        Command::new(binary())
            .args(["permissions", "set", "--state-dir"])
            .arg(&self.state)
            .args([
                "--mode",
                mode,
                "--expected-revision",
                &expected_revision.to_string(),
            ])
            .output()
            .unwrap()
    }

    fn decide(&self, pending: &Value, approve: bool) {
        let output = approval_decision(self, pending, approve);
        assert_success(&output);
    }

    fn report(&self) -> Value {
        serde_json::from_slice(&fs::read(self.state.join("latest.json")).unwrap()).unwrap()
    }
}

fn approval_decision(fixture: &Fixture, pending: &Value, approve: bool) -> Output {
    let revision = pending["revision"].as_u64().unwrap().to_string();
    let requested_at_ms = pending["requested_at_ms"].as_u64().unwrap().to_string();
    Command::new(binary())
        .args(["approval", "decide", "--state-dir"])
        .arg(&fixture.state)
        .args([
            "--id",
            pending["id"].as_str().unwrap(),
            "--token",
            pending["token"].as_str().unwrap(),
            "--revision",
            &revision,
            "--run-id",
            pending["run_id"].as_str().unwrap(),
            "--action",
            pending["action"].as_str().unwrap(),
            "--digest",
            pending["digest"].as_str().unwrap(),
            "--requested-at-ms",
            &requested_at_ms,
            "--approve",
            if approve { "true" } else { "false" },
        ])
        .output()
        .unwrap()
}

fn wait_for_approval(fixture: &Fixture, action: &str) -> Value {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let output = Command::new(binary())
            .args(["approval", "show", "--state-dir"])
            .arg(&fixture.state)
            .output()
            .unwrap();
        if output.status.success() {
            let value: Value = serde_json::from_slice(&output.stdout).unwrap();
            if value["status"] == "pending"
                && value["action"] == action
                && value["approved"].is_null()
            {
                return value;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "approval did not reach {action}"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_assemblywright-local")
}

fn edits(path: &str, content: &str) -> String {
    json!({"edits":[{"path":path,"content":content}]}).to_string()
}
fn review(approve: bool, findings: &[&str]) -> String {
    json!({"approve":approve,"findings":findings}).to_string()
}

#[cfg(feature = "local-chatgpt-test-fixture")]
fn chatgpt_fixture(
    plan_text: &str,
    review_text: &str,
    malformed_chat: bool,
    delay_stage: Option<&str>,
) -> Value {
    json!({
        "catalog": {
            "status": "ok",
            "account_type": "chatgpt",
            "models": [
                {"id":"gpt-plan","name":"GPT Plan","reasoning_efforts":["high"],"default_reasoning_effort":"high"},
                {"id":"gpt-review","name":"GPT Review","reasoning_efforts":["xhigh"],"default_reasoning_effort":"xhigh"}
            ]
        },
        "plan_text": plan_text,
        "review_text": review_text,
        "chat_text": "ready",
        "malformed_chat": malformed_chat,
        "malformed_stage": null,
        "delay_stage": delay_stage,
        "delay_ms": if delay_stage.is_some() { 10_000 } else { 0 }
    })
}

#[cfg(feature = "local-chatgpt-test-fixture")]
fn enable_chatgpt_roles(job: &mut Value) {
    job["orchestrator_provider"] = json!("chatgpt");
    job["orchestrator_model"] = json!("gpt-plan");
    job["orchestrator_reasoning_effort"] = json!("high");
    job["reviewer_provider"] = json!("chatgpt");
    job["reviewer_model"] = json!("gpt-review");
    job["reviewer_reasoning_effort"] = json!("xhigh");
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

struct FakeModel {
    address: SocketAddr,
    thread: Option<thread::JoinHandle<Vec<String>>>,
}

impl FakeModel {
    fn start(responses: Vec<String>) -> Self {
        Self::start_internal(responses, None, None)
    }

    fn start_with_hook(responses: Vec<String>, hook: Option<(usize, PathBuf)>) -> Self {
        Self::start_internal(responses, hook, None)
    }

    fn start_with_catalog(responses: Vec<String>, catalog: Value) -> Self {
        Self::start_internal(responses, None, Some(catalog))
    }

    fn start_internal(
        responses: Vec<String>,
        hook: Option<(usize, PathBuf)>,
        catalog: Option<Value>,
    ) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let responses = Arc::new(Mutex::new(VecDeque::from(responses)));
        let thread = thread::spawn(move || {
            let mut requests = Vec::new();
            let mut index = 0;
            let mut catalog_served = false;
            loop {
                if catalog_served && responses.lock().unwrap().is_empty() {
                    break;
                }
                let (mut stream, _) = listener.accept().unwrap();
                let (request_line, body) = read_request(&mut stream);
                if request_line.starts_with("GET /v1/models ") {
                    let body = catalog
                        .clone()
                        .unwrap_or_else(|| json!({"object":"list","data":[]}))
                        .to_string();
                    write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
                    catalog_served = true;
                    continue;
                }
                let response = responses
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("unexpected model request");
                requests.push(body);
                let body = json!({"choices":[{"message":{"role":"assistant","content":response}}]})
                    .to_string();
                if let Some((hook_index, path)) = &hook {
                    if *hook_index == index {
                        fs::create_dir_all(path.parent().unwrap()).unwrap();
                        fs::write(path, b"test cancellation\n").unwrap();
                    }
                }
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
                index += 1;
            }
            requests
        });
        Self {
            address,
            thread: Some(thread),
        }
    }

    fn endpoint(&self) -> String {
        format!("http://{}/v1", self.address)
    }
    fn join(mut self) -> Vec<String> {
        self.thread.take().unwrap().join().unwrap()
    }
}

struct ConnectionProbe {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    connections: Arc<AtomicUsize>,
    thread: Option<thread::JoinHandle<()>>,
}

impl ConnectionProbe {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let connections = Arc::new(AtomicUsize::new(0));
        let thread_stop = stop.clone();
        let thread_connections = connections.clone();
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        thread_connections.fetch_add(1, Ordering::SeqCst);
                        let (_request_line, _body) = read_request(&mut stream);
                        let body = json!({"object":"list","data":[]}).to_string();
                        let _ = write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("connection probe failed: {error}"),
                }
            }
        });
        Self {
            address,
            stop,
            connections,
            thread: Some(thread),
        }
    }

    fn endpoint(&self) -> String {
        format!("http://{}/v1", self.address)
    }

    fn finish(mut self) -> usize {
        self.stop.store(true, Ordering::SeqCst);
        self.thread.take().unwrap().join().unwrap();
        self.connections.load(Ordering::SeqCst)
    }
}

enum OversizedTarget {
    CatalogContentLength,
    CatalogChunked,
    ModelChunked,
}

struct OversizedHttpServer {
    address: SocketAddr,
    thread: Option<thread::JoinHandle<Vec<String>>>,
}

impl OversizedHttpServer {
    fn start(target: OversizedTarget) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let thread = thread::spawn(move || {
            let mut requests = Vec::new();
            let expected = match target {
                OversizedTarget::CatalogContentLength | OversizedTarget::CatalogChunked => 1,
                OversizedTarget::ModelChunked => 2,
            };
            for index in 0..expected {
                let (mut stream, _) = listener.accept().unwrap();
                let (request_line, _body) = read_request(&mut stream);
                requests.push(request_line);
                match (&target, index) {
                    (OversizedTarget::CatalogContentLength, 0) => {
                        write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 1048577\r\nConnection: close\r\n\r\n"
                        )
                        .unwrap();
                    }
                    (OversizedTarget::CatalogChunked, 0) => {
                        write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
                        )
                        .unwrap();
                        write_chunk(&mut stream, &vec![b'x'; 1024 * 1024]);
                        write_chunk(&mut stream, b"x");
                        let _ = stream.write_all(b"0\r\n\r\n");
                    }
                    (OversizedTarget::ModelChunked, 0) => {
                        let body = json!({"object":"list","data":[]}).to_string();
                        write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        )
                        .unwrap();
                    }
                    (OversizedTarget::ModelChunked, 1) => {
                        write!(
                            stream,
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n"
                        )
                        .unwrap();
                        let chunk = vec![b'x'; 1024 * 1024];
                        for _ in 0..4 {
                            write_chunk(&mut stream, &chunk);
                        }
                        write_chunk(&mut stream, b"x");
                        let _ = stream.write_all(b"0\r\n\r\n");
                    }
                    _ => unreachable!(),
                }
            }
            requests
        });
        Self {
            address,
            thread: Some(thread),
        }
    }

    fn endpoint(&self) -> String {
        format!("http://{}/v1", self.address)
    }

    fn join(mut self) -> Vec<String> {
        self.thread.take().unwrap().join().unwrap()
    }
}

fn write_chunk(stream: &mut TcpStream, bytes: &[u8]) {
    if write!(stream, "{:X}\r\n", bytes.len()).is_ok() {
        let _ = stream.write_all(bytes);
        let _ = stream.write_all(b"\r\n");
    }
}

fn wait_for_child(child: &mut std::process::Child) -> std::process::ExitStatus {
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            return status;
        }
        if std::time::Instant::now() >= deadline {
            child.kill().unwrap();
            panic!("local runner did not stop within 15 seconds");
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn tree_shape(root: &Path) -> (usize, u64) {
    if !root.exists() {
        return (0, 0);
    }
    let mut files = 0;
    let mut bytes = 0_u64;
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let metadata = entry.metadata().unwrap();
            if metadata.is_dir() {
                directories.push(entry.path());
            } else {
                files += 1;
                bytes = bytes.checked_add(metadata.len()).unwrap();
            }
        }
    }
    (files, bytes)
}

fn wait_for_candidate_file(fixture: &Fixture, relative: &str) -> PathBuf {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        if fixture.state.join("latest.json").exists() {
            let report = fixture.report();
            if let Some(candidate) = report["candidate"].as_str() {
                let path = PathBuf::from(candidate).join(relative);
                if path.exists() {
                    return path;
                }
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "candidate file did not appear: {relative}"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

fn wait_for_stage(fixture: &Fixture, expected: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        if fixture.state.join("latest.json").exists() && fixture.report()["stage"] == expected {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "pipeline did not reach {expected}"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

fn assert_process_exits(pid: u32) {
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        let output = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&output.stdout);
        if !text.contains(&format!("\"{pid}\"")) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "process {pid} survived Job Object termination"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

fn read_request(stream: &mut TcpStream) -> (String, String) {
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 4096];
    let mut expected = None;
    loop {
        let count = stream.read(&mut buffer).unwrap();
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..count]);
        if expected.is_none() {
            if let Some(split) = bytes.windows(4).position(|v| v == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..split]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(str::trim)
                            .and_then(|v| v.parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                expected = Some(split + 4 + length);
            }
        }
        if expected.is_some_and(|size| bytes.len() >= size) {
            break;
        }
    }
    let split = bytes.windows(4).position(|v| v == b"\r\n\r\n").unwrap();
    let request_line = String::from_utf8_lossy(&bytes[..split])
        .lines()
        .next()
        .unwrap_or_default()
        .to_owned();
    (
        request_line,
        String::from_utf8(bytes[split + 4..].to_vec()).unwrap(),
    )
}
