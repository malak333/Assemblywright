#![cfg_attr(not(windows), allow(dead_code))]

use rusqlite::Connection;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};

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
#[cfg(windows)]
fn windows_repository_onboarding_requires_three_confirmations_and_records_exact_grants() {
    exercise_windows_repository_onboarding();
}

fn exercise_windows_repository_onboarding() {
    let binary = env!("CARGO_BIN_EXE_assemblywright-master");
    let fixture = tempfile::tempdir().expect("repository-onboarding E2E fixture");
    let data_dir = fixture.path().join("master-data");
    let repository = fixture.path().join("ordinary-main-repository");
    std::fs::create_dir(&data_dir).expect("create master data directory");
    create_ordinary_main_repository(&repository);

    assert_success(
        &run_master(binary, &data_dir, &["setup"]),
        "initialize master data",
    );
    let endpoint = unused_loopback_addr();
    let mut server = spawn_server(binary, &data_dir, endpoint);
    let ready = read_ready(&mut server.child);
    assert_eq!(ready["status"], "ready");

    let script = workspace_root().join("scripts/windows-repository-onboarding.ps1");
    let self_test = run_onboarding(&script, "SelfTest", &data_dir, endpoint, None, None, &[]);
    assert_success(&self_test, "run repository-onboarding helper self-test");
    let (_, self_test_receipt) = decode_last_json_line(&self_test);
    assert_eq!(
        self_test_receipt["status"],
        "repository_onboarding_self_test_passed"
    );
    for decision in [
        "expired_absent_approval_negative",
        "expired_complete_resume",
        "expired_partial_resume",
        "expired_receipt_replay",
        "exact_grant_drift_negative",
        "revoked_grant_negative",
    ] {
        assert_eq!(self_test_receipt[decision], "verified", "{decision}");
    }
    let plan_output = run_onboarding(
        &script,
        "Plan",
        &data_dir,
        endpoint,
        Some(&repository),
        None,
        &[],
    );
    assert_success(&plan_output, "plan repository onboarding");
    let (_, plan) = decode_last_json_line(&plan_output);
    assert_eq!(plan["status"], "repository_onboarding_planned");
    let plan_id = plan["plan_id"]
        .as_str()
        .expect("planned canonical plan ID")
        .to_owned();
    let repository_id = plan["repository_id"]
        .as_str()
        .expect("planned canonical repository ID")
        .to_owned();
    let private_plan_path = data_dir
        .join("repository-onboarding-plans")
        .join(format!("{plan_id}.plan.json"));
    let generated_private_plan: Value = serde_json::from_str(
        &std::fs::read_to_string(&private_plan_path).expect("read private onboarding plan"),
    )
    .expect("decode private onboarding plan");
    let private_plan = rewrite_plan_as_expired(&private_plan_path, &generated_private_plan);
    let owner_token =
        std::fs::read_to_string(data_dir.join("development.token")).expect("read owner token");
    assert_output_excludes_secret(&plan_output, &owner_token);

    let missing_confirmation = run_onboarding(
        &script,
        "Approve",
        &data_dir,
        endpoint,
        None,
        Some(&plan_id),
        &["-ConfirmRegistration", "-ConfirmCloudDisclosure"],
    );
    assert!(
        !missing_confirmation.status.success(),
        "onboarding accepted fewer than all three owner confirmations"
    );
    assert!(
        String::from_utf8_lossy(&missing_confirmation.stderr).contains(
            "Approve requires separate -ConfirmRegistration, -ConfirmCloudDisclosure, and -ConfirmAutonomousPublication switches."
        ),
        "unexpected missing-confirmation rejection: {}",
        String::from_utf8_lossy(&missing_confirmation.stderr)
    );
    assert_output_excludes_secret(&missing_confirmation, &owner_token);

    let pre_approval_check = run_onboarding(
        &script,
        "Check",
        &data_dir,
        endpoint,
        None,
        Some(&plan_id),
        &[],
    );
    assert_success(&pre_approval_check, "check rejected approval");
    let (_, pre_approval) = decode_last_json_line(&pre_approval_check);
    assert_eq!(pre_approval["grant_state"], "absent");
    assert_eq!(pre_approval["authoring_receipt_present"], false);
    assert_eq!(
        pre_approval["approval_plan_sha256"],
        private_plan["approval_plan_sha256"]
    );
    assert_output_excludes_secret(&pre_approval_check, &owner_token);

    let expired_fresh_approval = run_onboarding(
        &script,
        "Approve",
        &data_dir,
        endpoint,
        None,
        Some(&plan_id),
        &[
            "-ConfirmRegistration",
            "-ConfirmCloudDisclosure",
            "-ConfirmAutonomousPublication",
        ],
    );
    assert!(
        !expired_fresh_approval.status.success(),
        "expired onboarding plan created fresh grants"
    );
    assert!(
        String::from_utf8_lossy(&expired_fresh_approval.stderr).contains(
            "The expired repository-onboarding plan may only resume existing exact revision-1 grants or replay its exact stored receipt and grants."
        ),
        "unexpected expired-plan rejection: {}",
        String::from_utf8_lossy(&expired_fresh_approval.stderr)
    );
    assert_output_excludes_secret(&expired_fresh_approval, &owner_token);
    assert_grants_absent(endpoint, &owner_token, &repository_id);
    assert_onboarding_audit_absent(&data_dir);

    std::fs::write(
        repository.join("README.md"),
        "# Drifted onboarding fixture\n",
    )
    .expect("drift disposable repository after planning");
    let drifted_approval = run_onboarding(
        &script,
        "Approve",
        &data_dir,
        endpoint,
        None,
        Some(&plan_id),
        &[
            "-ConfirmRegistration",
            "-ConfirmCloudDisclosure",
            "-ConfirmAutonomousPublication",
        ],
    );
    assert!(
        !drifted_approval.status.success(),
        "onboarding accepted repository drift after planning"
    );
    assert!(
        String::from_utf8_lossy(&drifted_approval.stderr).contains(
            "The repository was not an exact clean standard main checkout with normal tracked-index state."
        ),
        "unexpected repository-drift rejection: {}",
        String::from_utf8_lossy(&drifted_approval.stderr)
    );
    assert_output_excludes_secret(&drifted_approval, &owner_token);
    assert_grants_absent(endpoint, &owner_token, &repository_id);
    assert_onboarding_audit_absent(&data_dir);

    std::fs::write(
        repository.join("README.md"),
        "# Disposable onboarding fixture\n",
    )
    .expect("restore disposable repository fixture");
    let restored_status = run_git(
        &repository,
        &["status", "--porcelain=v1", "--untracked-files=all"],
    );
    assert_success(&restored_status, "verify restored Git fixture");
    assert!(
        restored_status.stdout.is_empty(),
        "restored Git fixture remained dirty: {}",
        String::from_utf8_lossy(&restored_status.stdout)
    );

    record_exact_grant(endpoint, &owner_token, &private_plan, "registration");
    let partial_check = run_onboarding(
        &script,
        "Check",
        &data_dir,
        endpoint,
        None,
        Some(&plan_id),
        &[],
    );
    assert_success(&partial_check, "check expired partial onboarding");
    let (_, partial) = decode_last_json_line(&partial_check);
    assert_eq!(partial["grant_state"], "exact_partial_revision_1");
    assert_eq!(partial["authoring_receipt_present"], false);
    assert_output_excludes_secret(&partial_check, &owner_token);

    let approval = run_onboarding(
        &script,
        "Approve",
        &data_dir,
        endpoint,
        None,
        Some(&plan_id),
        &[
            "-ConfirmRegistration",
            "-ConfirmCloudDisclosure",
            "-ConfirmAutonomousPublication",
        ],
    );
    assert_success(&approval, "approve repository onboarding");
    let (receipt_line, receipt) = decode_last_json_line(&approval);
    assert_eq!(receipt["status"], "repository_onboarding_ready");
    assert_eq!(receipt["repository_id"], repository_id);
    assert_eq!(receipt["registration_grant_revision"], 1);
    assert_eq!(receipt["cloud_disclosure_grant_revision"], 1);
    assert_eq!(receipt["autonomous_publication_grant_revision"], 1);
    assert_eq!(receipt["head_commit"], plan["head_commit"]);
    assert_eq!(receipt["scope_sha256"], plan["scope_sha256"]);
    assert_eq!(
        receipt["approval_plan_sha256"],
        private_plan["approval_plan_sha256"]
    );
    assert_path_free_canonical_receipt(&receipt_line, &receipt, &repository);
    assert_output_excludes_secret(&approval, &owner_token);

    let replay = run_onboarding(
        &script,
        "Approve",
        &data_dir,
        endpoint,
        None,
        Some(&plan_id),
        &[
            "-ConfirmRegistration",
            "-ConfirmCloudDisclosure",
            "-ConfirmAutonomousPublication",
        ],
    );
    assert_success(&replay, "replay exact completed onboarding");
    let (replayed_receipt_line, replayed_receipt) = decode_last_json_line(&replay);
    assert_eq!(replayed_receipt_line, receipt_line);
    assert_eq!(replayed_receipt, receipt);
    assert_output_excludes_secret(&replay, &owner_token);

    let grant_set = authenticated_get(
        endpoint,
        &owner_token,
        &format!("/v1/feature-conveyor/repositories/{repository_id}/grants"),
    );
    assert_eq!(grant_set["repository_id"], repository_id);
    assert_eq!(grant_set["emergency_paused"], false);
    for kind in ["registration", "cloud_disclosure", "autonomous_publication"] {
        assert_eq!(grant_set[kind]["revision"], 1, "{kind} revision drifted");
        assert_eq!(grant_set[kind]["revoked"], false, "{kind} was revoked");
        assert_eq!(grant_set[kind]["active"], true, "{kind} was inactive");
        assert!(grant_set[kind]["expires_at_ms"].is_null());
        assert_eq!(
            byte_array_hex(&grant_set[kind]["scope_sha256"]),
            private_plan[format!("{kind}_scope_sha256")]
                .as_str()
                .expect("private plan grant scope")
        );
        assert_eq!(
            byte_array_hex(&grant_set[kind]["owner_approval_sha256"]),
            private_plan[format!("{kind}_owner_approval_sha256")]
                .as_str()
                .expect("private plan grant approval")
        );
    }

    let check_output = run_onboarding(
        &script,
        "Check",
        &data_dir,
        endpoint,
        None,
        Some(&plan_id),
        &[],
    );
    assert_success(&check_output, "check completed onboarding");
    let (_, check) = decode_last_json_line(&check_output);
    assert_eq!(check["status"], "repository_onboarding_check_passed");
    assert_eq!(check["repository_id"], repository_id);
    assert_eq!(check["grant_state"], "exact_revision_1");
    assert_eq!(check["authoring_receipt_present"], true);
    assert_output_excludes_secret(&check_output, &owner_token);
    assert_exact_redacted_onboarding_audit(&data_dir, &repository_id, &repository);
    exercise_expired_complete_without_receipt(
        &script,
        &data_dir,
        endpoint,
        &owner_token,
        fixture.path(),
    );
}

fn exercise_expired_complete_without_receipt(
    script: &Path,
    data_dir: &Path,
    endpoint: SocketAddr,
    owner_token: &str,
    fixture_root: &Path,
) {
    let repository = fixture_root.join("expired-complete-repository");
    create_ordinary_main_repository(&repository);
    let plan_output = run_onboarding(
        script,
        "Plan",
        data_dir,
        endpoint,
        Some(&repository),
        None,
        &[],
    );
    assert_success(&plan_output, "plan complete-without-receipt recovery");
    let (_, summary) = decode_last_json_line(&plan_output);
    let plan_id = summary["plan_id"].as_str().expect("second plan ID");
    let plan_path = data_dir
        .join("repository-onboarding-plans")
        .join(format!("{plan_id}.plan.json"));
    let generated_plan: Value = serde_json::from_str(
        &std::fs::read_to_string(&plan_path).expect("read second private plan"),
    )
    .expect("decode second private plan");
    let expired_plan = rewrite_plan_as_expired(&plan_path, &generated_plan);
    for kind in ["registration", "cloud_disclosure", "autonomous_publication"] {
        record_exact_grant(endpoint, owner_token, &expired_plan, kind);
    }

    let check = run_onboarding(
        script,
        "Check",
        data_dir,
        endpoint,
        None,
        Some(plan_id),
        &[],
    );
    assert_success(&check, "check expired complete grants without receipt");
    let (_, checked) = decode_last_json_line(&check);
    assert_eq!(checked["grant_state"], "exact_revision_1");
    assert_eq!(checked["authoring_receipt_present"], false);

    let confirmations = [
        "-ConfirmRegistration",
        "-ConfirmCloudDisclosure",
        "-ConfirmAutonomousPublication",
    ];
    let approval = run_onboarding(
        script,
        "Approve",
        data_dir,
        endpoint,
        None,
        Some(plan_id),
        &confirmations,
    );
    assert_success(&approval, "resume expired complete grants through receipt");
    let (receipt_line, receipt) = decode_last_json_line(&approval);
    assert_eq!(receipt["status"], "repository_onboarding_ready");
    assert_eq!(
        receipt["approval_plan_sha256"],
        expired_plan["approval_plan_sha256"]
    );
    assert_path_free_canonical_receipt(&receipt_line, &receipt, &repository);
    assert_exact_redacted_onboarding_audit(
        data_dir,
        expired_plan["repository_id"]
            .as_str()
            .expect("second repository ID"),
        &repository,
    );
    assert_onboarding_audit_count(data_dir, 8);

    let replay = run_onboarding(
        script,
        "Approve",
        data_dir,
        endpoint,
        None,
        Some(plan_id),
        &confirmations,
    );
    assert_success(&replay, "replay recovered expired complete onboarding");
    let (replay_line, replay_receipt) = decode_last_json_line(&replay);
    assert_eq!(replay_line, receipt_line);
    assert_eq!(replay_receipt, receipt);
    assert_onboarding_audit_count(data_dir, 8);
}

fn rewrite_plan_as_expired(path: &Path, source: &Value) -> Value {
    const CREATED_AT_MS: u64 = 1;
    const EXPIRES_AT_MS: u64 = CREATED_AT_MS + 24 * 60 * 60 * 1000;

    let approval_document = format!(
        concat!(
            "{{\"schema_version\":1,\"plan_id\":{},\"repository_id\":{},",
            "\"repository_path\":{},\"base_branch\":{},\"head_commit\":{},",
            "\"created_at_ms\":{},\"expires_at_ms\":{},",
            "\"scope_sha256\":{}}}"
        ),
        json_string(source, "plan_id"),
        json_string(source, "repository_id"),
        json_string(source, "repository_path"),
        json_string(source, "base_branch"),
        json_string(source, "head_commit"),
        CREATED_AT_MS,
        EXPIRES_AT_MS,
        json_string(source, "scope_sha256"),
    );
    let approval_plan_sha256 = sha256_hex(approval_document.as_bytes());
    let binding = |purpose: &str, kind: &str| {
        sha256_hex(
            format!(
                "assemblywright.repository-onboarding.{purpose}.v1\0{approval_plan_sha256}\0{kind}"
            )
            .as_bytes(),
        )
    };
    let plan_line = format!(
        concat!(
            "{{\"schema_version\":1,\"status\":{},\"plan_id\":{},\"repository_id\":{},",
            "\"repository_path\":{},\"base_branch\":{},\"head_commit\":{},",
            "\"created_at_ms\":{},\"expires_at_ms\":{},",
            "\"scope_sha256\":{},\"approval_plan_sha256\":\"{}\",",
            "\"registration_scope_sha256\":{},\"registration_owner_approval_sha256\":\"{}\",",
            "\"cloud_disclosure_scope_sha256\":\"{}\",\"cloud_disclosure_owner_approval_sha256\":\"{}\",",
            "\"autonomous_publication_scope_sha256\":\"{}\",\"autonomous_publication_owner_approval_sha256\":\"{}\"}}"
        ),
        json_string(source, "status"),
        json_string(source, "plan_id"),
        json_string(source, "repository_id"),
        json_string(source, "repository_path"),
        json_string(source, "base_branch"),
        json_string(source, "head_commit"),
        CREATED_AT_MS,
        EXPIRES_AT_MS,
        json_string(source, "scope_sha256"),
        approval_plan_sha256,
        json_string(source, "scope_sha256"),
        binding("owner-approval", "registration"),
        binding("scope", "cloud_disclosure"),
        binding("owner-approval", "cloud_disclosure"),
        binding("scope", "autonomous_publication"),
        binding("owner-approval", "autonomous_publication"),
    );
    std::fs::write(path, &plan_line).expect("rewrite disposable plan as canonically expired");
    serde_json::from_str(&plan_line).expect("decode expired private onboarding plan")
}

fn json_string(value: &Value, key: &str) -> String {
    serde_json::to_string(value[key].as_str().unwrap_or_else(|| panic!("plan {key}")))
        .expect("encode plan string")
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn record_exact_grant(endpoint: SocketAddr, token: &str, plan: &Value, kind: &str) {
    let scope_key = format!("{kind}_scope_sha256");
    let approval_key = format!("{kind}_owner_approval_sha256");
    let receipt = authenticated_post(
        endpoint,
        token,
        "/v1/feature-conveyor/repository-grants",
        &serde_json::json!({
            "schema_version": 1,
            "expected_current_revision": 0,
            "expected_emergency_pause_revision": 0,
            "grant": {
                "repository_id": plan["repository_id"],
                "kind": kind,
                "revision": 1,
                "scope_sha256": hex_bytes(plan[&scope_key].as_str().expect("grant scope")),
                "owner_approval_sha256": hex_bytes(plan[&approval_key].as_str().expect("grant owner approval")),
                "expires_at_ms": null,
                "revoked": false
            }
        }),
    );
    assert_eq!(receipt["status"], "recorded");
    assert_eq!(receipt["kind"], kind);
    assert_eq!(receipt["revision"], 1);
}

fn hex_bytes(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            u8::from_str_radix(std::str::from_utf8(pair).expect("digest hex pair"), 16)
                .expect("digest byte")
        })
        .collect()
}

fn assert_grants_absent(endpoint: SocketAddr, token: &str, repository_id: &str) {
    let grant_set = authenticated_get(
        endpoint,
        token,
        &format!("/v1/feature-conveyor/repositories/{repository_id}/grants"),
    );
    assert_eq!(grant_set["repository_id"], repository_id);
    assert_eq!(grant_set["emergency_paused"], false);
    assert_eq!(grant_set["emergency_pause_revision"], 0);
    assert!(grant_set["registration"].is_null());
    assert!(grant_set["cloud_disclosure"].is_null());
    assert!(grant_set["autonomous_publication"].is_null());
}

fn assert_onboarding_audit_absent(data_dir: &Path) {
    let connection = Connection::open(data_dir.join("master.sqlite3"))
        .expect("open onboarding authority database");
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM feature_conveyor_audit
             WHERE event_kind IN (
               'repository_grant_revision_recorded',
               'repository_identity_preflight_eligible'
             )",
            [],
            |row| row.get(0),
        )
        .expect("count onboarding audit rows");
    assert_eq!(count, 0, "rejected onboarding emitted authority audit");
}

fn assert_onboarding_audit_count(data_dir: &Path, expected: i64) {
    let connection = Connection::open(data_dir.join("master.sqlite3"))
        .expect("open onboarding authority database");
    let count: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM feature_conveyor_audit
             WHERE event_kind IN (
               'repository_grant_revision_recorded',
               'repository_identity_preflight_eligible'
             )",
            [],
            |row| row.get(0),
        )
        .expect("count onboarding audit rows");
    assert_eq!(count, expected, "onboarding audit count drifted");
}

fn assert_exact_redacted_onboarding_audit(data_dir: &Path, repository_id: &str, repository: &Path) {
    let connection = Connection::open(data_dir.join("master.sqlite3"))
        .expect("open onboarding authority database");
    let mut statement = connection
        .prepare(
            "SELECT event_kind, redacted_metadata_json FROM feature_conveyor_audit
             WHERE event_kind IN (
               'repository_grant_revision_recorded',
               'repository_identity_preflight_eligible'
             )
             ORDER BY audit_id DESC
             LIMIT 4",
        )
        .expect("prepare onboarding audit query");
    let mut rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .expect("query onboarding audit")
        .collect::<Result<Vec<_>, _>>()
        .expect("read onboarding audit");
    rows.reverse();
    assert_eq!(rows.len(), 4, "onboarding or replay changed audit count");

    let expected_grant_kinds = ["registration", "cloud_disclosure", "autonomous_publication"];
    for ((event_kind, metadata), grant_kind) in rows[..3].iter().zip(expected_grant_kinds) {
        assert_eq!(event_kind, "repository_grant_revision_recorded");
        assert_eq!(
            serde_json::from_str::<Value>(metadata).expect("decode grant audit metadata"),
            serde_json::json!({
                "grant_kind": grant_kind,
                "revision": 1,
                "revoked": false,
                "scope_digest_present": true,
                "owner_approval_digest_present": true,
                "side_effect_executed": false
            })
        );
        assert_redacted_audit(metadata, repository_id, repository);
    }

    assert_eq!(rows[3].0, "repository_identity_preflight_eligible");
    assert_eq!(
        serde_json::from_str::<Value>(&rows[3].1).expect("decode preflight audit metadata"),
        serde_json::json!({
            "grant_kind": "registration",
            "grant_revision": 1,
            "emergency_pause_revision": 0,
            "scope_digest_matched": true,
            "point_in_time": true,
            "identity_only": true,
            "side_effect_executed": false
        })
    );
    assert_redacted_audit(&rows[3].1, repository_id, repository);
}

fn assert_redacted_audit(metadata: &str, repository_id: &str, repository: &Path) {
    assert!(!metadata.contains(repository_id));
    assert!(!metadata.contains(&repository.to_string_lossy().to_string()));
    assert!(!metadata.contains("repository_path"));
    assert!(!metadata.contains("head_commit"));
}

fn assert_output_excludes_secret(output: &Output, secret: &str) {
    let secret = secret.trim();
    assert!(!secret.is_empty(), "owner token fixture was empty");
    assert!(!String::from_utf8_lossy(&output.stdout).contains(secret));
    assert!(!String::from_utf8_lossy(&output.stderr).contains(secret));
}

fn byte_array_hex(value: &Value) -> String {
    value
        .as_array()
        .expect("digest byte array")
        .iter()
        .map(|byte| {
            format!(
                "{:02x}",
                byte.as_u64()
                    .filter(|byte| *byte <= u8::MAX.into())
                    .expect("digest byte")
            )
        })
        .collect()
}

fn create_ordinary_main_repository(repository: &Path) {
    std::fs::create_dir(repository).expect("create repository directory");
    assert_success(
        &Command::new("git")
            .args(["init", "-b", "main"])
            .arg(repository)
            .output()
            .expect("initialize Git repository"),
        "initialize ordinary main repository",
    );
    for arguments in [
        &["config", "user.name", "Assemblywright E2E"][..],
        &["config", "user.email", "assemblywright-e2e@example.invalid"][..],
        &["config", "core.autocrlf", "false"][..],
        &["config", "commit.gpgsign", "false"][..],
    ] {
        assert_success(&run_git(repository, arguments), "configure Git fixture");
    }
    std::fs::write(
        repository.join("README.md"),
        "# Disposable onboarding fixture\n",
    )
    .expect("write tracked fixture");
    assert_success(&run_git(repository, &["add", "README.md"]), "stage fixture");
    assert_success(
        &run_git(repository, &["commit", "-m", "Initialize fixture"]),
        "commit fixture",
    );
}

fn run_git(repository: &Path, arguments: &[&str]) -> Output {
    Command::new("git")
        .arg("-c")
        .arg("core.hooksPath=NUL")
        .arg("-C")
        .arg(repository)
        .args(arguments)
        .output()
        .expect("run fixture Git command")
}

fn run_master(binary: &str, data_dir: &Path, arguments: &[&str]) -> Output {
    Command::new(binary)
        .arg("--data-dir")
        .arg(data_dir)
        .args(arguments)
        .output()
        .expect("run assemblywright-master")
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
        .expect("spawn owner-loopback master");
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

fn run_onboarding(
    script: &Path,
    action: &str,
    data_dir: &Path,
    endpoint: SocketAddr,
    repository: Option<&Path>,
    plan_id: Option<&str>,
    confirmations: &[&str],
) -> Output {
    let mut command = Command::new("powershell.exe");
    // GitHub's pwsh runner exports a PowerShell 7 module path that omits the
    // Windows PowerShell inbox modules. Let powershell.exe rebuild its native
    // default so security cmdlets such as Get-Acl and Set-Acl remain available.
    command.env_remove("PSModulePath");
    command
        .args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-File",
        ])
        .arg(script)
        .arg("-Action")
        .arg(action)
        .arg("-DataDir")
        .arg(data_dir)
        .arg("-Endpoint")
        .arg(endpoint.to_string());
    if let Some(repository) = repository {
        command.arg("-RepositoryPath").arg(repository);
    }
    if let Some(plan_id) = plan_id {
        command.arg("-PlanId").arg(plan_id);
    }
    command
        .args(confirmations)
        .output()
        .expect("run Windows repository-onboarding script")
}

fn decode_last_json_line(output: &Output) -> (String, Value) {
    let stdout = String::from_utf8(output.stdout.clone())
        .unwrap_or_else(|error| panic!("onboarding output was not UTF-8: {error}"));
    let line = stdout
        .lines()
        .rfind(|line| !line.trim().is_empty())
        .expect("onboarding JSON line")
        .trim()
        .to_owned();
    let value = serde_json::from_str(&line)
        .unwrap_or_else(|error| panic!("invalid onboarding JSON {line:?}: {error}"));
    (line, value)
}

fn assert_path_free_canonical_receipt(line: &str, receipt: &Value, repository: &Path) {
    let expected = format!(
        concat!(
            "{{\"schema_version\":1,\"status\":\"repository_onboarding_ready\",",
            "\"repository_id\":\"{}\",\"registration_grant_revision\":1,",
            "\"cloud_disclosure_grant_revision\":1,",
            "\"autonomous_publication_grant_revision\":1,\"base_branch\":\"main\",",
            "\"head_commit\":\"{}\",\"scope_sha256\":\"{}\",",
            "\"approval_plan_sha256\":\"{}\",\"preflight_fingerprint_sha256\":\"{}\"}}"
        ),
        receipt["repository_id"].as_str().expect("repository ID"),
        receipt["head_commit"].as_str().expect("HEAD commit"),
        receipt["scope_sha256"].as_str().expect("scope digest"),
        receipt["approval_plan_sha256"]
            .as_str()
            .expect("approval-plan digest"),
        receipt["preflight_fingerprint_sha256"]
            .as_str()
            .expect("preflight fingerprint"),
    );
    assert_eq!(line, expected, "authoring receipt was not canonical");
    assert!(!line.contains("repository_path"));
    assert!(!line.contains(&repository.to_string_lossy().to_string()));
    assert!(
        !line.contains(":\\"),
        "authoring receipt leaked a drive path"
    );
    assert!(
        !line.contains("\\\\"),
        "authoring receipt leaked a UNC path"
    );
}

fn authenticated_get(endpoint: SocketAddr, token: &str, path: &str) -> Value {
    let mut stream = TcpStream::connect(endpoint).expect("connect to owner-loopback master");
    write!(
        stream,
        "GET {path} HTTP/1.1\r\nHost: {endpoint}\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
        token.trim()
    )
    .expect("write authenticated owner request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read authenticated owner response");
    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "unexpected grant projection response: {response}"
    );
    let (_, body) = response
        .split_once("\r\n\r\n")
        .expect("HTTP response separator");
    serde_json::from_str(body)
        .unwrap_or_else(|error| panic!("invalid grant projection JSON: {error}; body={body}"))
}

fn authenticated_post(endpoint: SocketAddr, token: &str, path: &str, body: &Value) -> Value {
    let body = serde_json::to_string(body).expect("encode authenticated owner request");
    let mut stream = TcpStream::connect(endpoint).expect("connect to owner-loopback master");
    write!(
        stream,
        "POST {path} HTTP/1.1\r\nHost: {endpoint}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        token.trim(),
        body.len()
    )
    .expect("write authenticated owner request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read authenticated owner response");
    assert!(
        response.starts_with("HTTP/1.1 200 OK"),
        "unexpected owner mutation response: {response}"
    );
    let (_, body) = response
        .split_once("\r\n\r\n")
        .expect("HTTP response separator");
    serde_json::from_str(body)
        .unwrap_or_else(|error| panic!("invalid owner mutation JSON: {error}; body={body}"))
}

fn unused_loopback_addr() -> SocketAddr {
    TcpListener::bind("127.0.0.1:0")
        .expect("reserve loopback address")
        .local_addr()
        .expect("read loopback address")
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical workspace root")
}

fn assert_success(output: &Output, operation: &str) {
    assert!(
        output.status.success(),
        "{operation} failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
