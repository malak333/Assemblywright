#![cfg(windows)]

use serde_json::{json, Value};
use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use uuid::Uuid;

struct Fixture {
    _temp: TempDir,
    root: PathBuf,
    state: PathBuf,
    project: PathBuf,
    candidate: PathBuf,
    gh: PathBuf,
    git: PathBuf,
    real_git: PathBuf,
    source: String,
    candidate_head: String,
}

impl Fixture {
    fn new() -> Self {
        let temp = TempDir::new().unwrap();
        let root = temp.path().to_path_buf();
        let state = root.join("state");
        let project = root.join("project");
        fs::create_dir_all(&state).unwrap();
        init_repo(&project);
        let source = git_text(&project, &["rev-parse", "HEAD"]);
        let candidate = root.join("candidate");
        run(Command::new("git")
            .args(["clone", "--quiet"])
            .arg(&project)
            .arg(&candidate));
        fs::write(candidate.join("feature.txt"), "implemented\n").unwrap();
        run(Command::new("git")
            .arg("-C")
            .arg(&candidate)
            .args(["add", "feature.txt"]));
        run(Command::new("git").arg("-C").arg(&candidate).args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "feature",
        ]));
        run(Command::new("git")
            .arg("-C")
            .arg(&candidate)
            .args(["remote", "remove", "origin"]));
        let candidate_head = git_text(&candidate, &["rev-parse", "HEAD"]);
        let real_git = which("git.exe");
        let gh = root.join("fake-gh.cmd");
        let git = root.join("fake-git.cmd");
        fs::write(&gh, "@echo off\r\npowershell.exe -NoProfile -ExecutionPolicy Bypass -File \"%~dp0fake-gh.ps1\" %*\r\n").unwrap();
        fs::write(&git, "@echo off\r\npowershell.exe -NoProfile -ExecutionPolicy Bypass -File \"%~dp0fake-git.ps1\" %*\r\n").unwrap();
        fs::write(root.join("fake-git.ps1"), FAKE_GIT).unwrap();
        fs::write(root.join("fake-gh.ps1"), FAKE_GH).unwrap();
        Self {
            _temp: temp,
            root,
            state,
            project,
            candidate,
            gh,
            git,
            real_git,
            source,
            candidate_head,
        }
    }

    fn command(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_assemblywright-github"));
        c.arg("--fixture-gh")
            .arg(&self.gh)
            .arg("--fixture-git")
            .arg(&self.git)
            .env("ASSEMBLYWRIGHT_GITHUB_TEST_FIXTURE", "1")
            .env("FIXTURE_REAL_GIT", &self.real_git)
            .env("FIXTURE_DIR", &self.root)
            .env("FIXTURE_SOURCE_HEAD", &self.source)
            .env("FIXTURE_CANDIDATE_HEAD", &self.candidate_head)
            .env("FIXTURE_CANDIDATE_REPO", &self.candidate);
        c
    }

    fn command_with_check_timeout(&self, milliseconds: u64) -> Command {
        let mut command = self.command();
        command
            .arg("--fixture-check-timeout-ms")
            .arg(milliseconds.to_string());
        command
    }

    fn invoke(&self, args: &[&str]) -> (Output, Value) {
        let output = self.command().args(args).output().unwrap();
        let value: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|_| {
            panic!(
                "invalid JSON stdout: {}\nstderr: {}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        });
        (output, value)
    }

    fn state_arg(&self) -> String {
        self.state.to_string_lossy().into_owned()
    }
    fn project_arg(&self) -> String {
        self.project.to_string_lossy().into_owned()
    }

    fn completed_run(&self, id: Uuid) {
        self.completed_run_with(id, &self.source, &self.candidate, &self.candidate_head);
    }

    fn completed_run_with(&self, id: Uuid, source: &str, candidate: &Path, candidate_head: &str) {
        let dir = self.state.join("runs").join(id.to_string());
        fs::create_dir_all(&dir).unwrap();
        let retained_candidate = dir.join("candidate");
        run(Command::new("git")
            .args(["clone", "--quiet"])
            .arg(candidate)
            .arg(&retained_candidate));
        run(Command::new("git")
            .arg("-C")
            .arg(&retained_candidate)
            .args(["remote", "remove", "origin"]));
        fs::write(dir.join("source-head.txt"), format!("{source}\n")).unwrap();
        fs::write(
            dir.join("submitted-job.json"),
            serde_json::to_vec_pretty(&json!({"repository":self.project})).unwrap(),
        )
        .unwrap();
        fs::write(dir.join("report.json"), serde_json::to_vec_pretty(&json!({
            "run_id":id.to_string(),"status":"completed","candidate":retained_candidate,"candidate_commit":candidate_head
        })).unwrap()).unwrap();
    }
}

#[test]
fn account_repository_project_and_connection_contracts() {
    let f = Fixture::new();
    let state = f.state_arg();
    let project = f.project_arg();
    let (out, account) = f.invoke(&["account", "--state-dir", &state]);
    assert!(out.status.success());
    assert_eq!(account["status"], "signed_in");
    assert_eq!(account["account"], "fixture-owner");
    let (out, repos) = f.invoke(&["repositories", "--state-dir", &state]);
    assert!(out.status.success());
    assert_eq!(
        repos["repositories"][0]["nameWithOwner"],
        "fixture-owner/example"
    );

    let empty = f.root.join("empty project");
    let empty_text = empty.to_string_lossy().into_owned();
    let (out, created) = f.invoke(&[
        "create-project",
        "--state-dir",
        &state,
        "--directory",
        &empty_text,
        "--repository",
        "fixture-owner/example",
    ]);
    assert!(out.status.success());
    assert_eq!(created["empty"], true);
    assert!(fs::read_dir(&empty).unwrap().next().is_none());
    let (out, empty_connection) = f.invoke(&[
        "connection",
        "get",
        "--state-dir",
        &state,
        "--project",
        &empty_text,
    ]);
    assert!(out.status.success());
    assert!(empty_connection["connection"].is_null());

    let (out, stale) = f.invoke(&[
        "connection",
        "save",
        "--state-dir",
        &state,
        "--project",
        &project,
        "--repository",
        "fixture-owner/example",
        "--base-branch",
        "main",
        "--expected-revision",
        "7",
    ]);
    assert!(!out.status.success());
    assert_eq!(stale["status"], "error");
    assert!(stale["message"]
        .as_str()
        .unwrap()
        .contains("stale revision"));
    let (out, saved) = f.invoke(&[
        "connection",
        "save",
        "--state-dir",
        &state,
        "--project",
        &project,
        "--repository",
        "fixture-owner/example",
        "--base-branch",
        "main",
        "--expected-revision",
        "0",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(saved["revision"], 1);
    assert_eq!(saved["connection"]["source_head"], f.source);
    let (out, prepared) = f.invoke(&[
        "connection",
        "save",
        "--state-dir",
        &state,
        "--project",
        &empty_text,
        "--repository",
        "fixture-owner/example",
        "--base-branch",
        "main",
        "--expected-revision",
        "1",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(prepared["prepared_checkout"], true);
    assert!(empty.join(".git").is_dir());
    let (out, got) = f.invoke(&[
        "connection",
        "get",
        "--state-dir",
        &state,
        "--project",
        &project,
    ]);
    assert!(out.status.success());
    assert_eq!(got["connection"]["repository"], "fixture-owner/example");
}

#[test]
fn connection_rejects_remote_base_drift_without_saving_authority() {
    let f = Fixture::new();
    let state = f.state_arg();
    let project = f.project_arg();
    let mut command = f.command();
    command
        .env(
            "FIXTURE_SOURCE_HEAD",
            "1111111111111111111111111111111111111111",
        )
        .args([
            "connection",
            "save",
            "--state-dir",
            &state,
            "--project",
            &project,
            "--repository",
            "fixture-owner/example",
            "--base-branch",
            "main",
            "--expected-revision",
            "0",
        ]);
    let output = command.output().unwrap();
    assert!(!output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value["message"]
        .as_str()
        .unwrap()
        .contains("does not match"));
    let (_, status) = f.invoke(&["status", "--state-dir", &state]);
    assert_eq!(status["revision"], 0);
    assert_eq!(status["connections"].as_array().unwrap().len(), 0);
}

#[test]
fn connection_rejects_dirty_project_and_wrong_origin() {
    let f = Fixture::new();
    let state = f.state_arg();
    let project = f.project_arg();
    fs::write(f.project.join("untracked.txt"), "dirty\n").unwrap();
    let (dirty, dirty_value) = f.invoke(&[
        "connection",
        "save",
        "--state-dir",
        &state,
        "--project",
        &project,
        "--repository",
        "fixture-owner/example",
        "--base-branch",
        "main",
        "--expected-revision",
        "0",
    ]);
    assert!(!dirty.status.success());
    assert!(dirty_value["message"].as_str().unwrap().contains("clean"));
    fs::remove_file(f.project.join("untracked.txt")).unwrap();
    run(Command::new("git").arg("-C").arg(&f.project).args([
        "remote",
        "set-url",
        "origin",
        "https://github.com/fixture-owner/other.git",
    ]));
    let (wrong, wrong_value) = f.invoke(&[
        "connection",
        "save",
        "--state-dir",
        &state,
        "--project",
        &project,
        "--repository",
        "fixture-owner/example",
        "--base-branch",
        "main",
        "--expected-revision",
        "0",
    ]);
    assert!(!wrong.status.success());
    assert!(wrong_value["message"].as_str().unwrap().contains("origin"));
}

#[test]
fn repository_creation_requires_explicit_visibility_and_verifies_result() {
    let f = Fixture::new();
    let state = f.state_arg();
    let mut command = f.command();
    command.env("FIXTURE_REPO_ABSENT", "1").args([
        "create-repository",
        "--state-dir",
        &state,
        "--name",
        "fixture-owner/example",
        "--visibility",
        "private",
    ]);
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["status"], "created");
    assert_eq!(
        value["repository"]["url"],
        "https://github.com/fixture-owner/example"
    );
}

#[test]
fn repository_creation_fails_closed_on_permission_and_network_lookup_errors() {
    for message in [
        "GraphQL: Resource not accessible by personal access token (repository)",
        "error connecting to api.github.com: network is unreachable",
    ] {
        let f = Fixture::new();
        let state = f.state_arg();
        let mut command = f.command();
        command.env("FIXTURE_REPO_VIEW_ERROR", message).args([
            "create-repository",
            "--state-dir",
            &state,
            "--name",
            "fixture-owner/example",
            "--visibility",
            "private",
        ]);
        let output = command.output().unwrap();
        assert!(!output.status.success());
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert!(value["message"]
            .as_str()
            .unwrap()
            .contains("could not determine whether repository already exists"));
        assert!(!f.root.join("repo-created.flag").exists());
    }
}

#[test]
fn existing_repository_must_match_the_requested_visibility_without_mutation() {
    let f = Fixture::new();
    let state = f.state_arg();
    let (wrong, value) = f.invoke(&[
        "create-repository",
        "--state-dir",
        &state,
        "--name",
        "fixture-owner/example",
        "--visibility",
        "public",
    ]);
    assert!(!wrong.status.success());
    assert!(value["message"]
        .as_str()
        .unwrap()
        .contains("existing repository visibility"));
    assert!(!f.root.join("repo-created.flag").exists());

    let (matching, value) = f.invoke(&[
        "create-repository",
        "--state-dir",
        &state,
        "--name",
        "fixture-owner/example",
        "--visibility",
        "private",
    ]);
    assert!(matching.status.success());
    assert_eq!(value["status"], "existing");
    assert_eq!(value["repository"]["visibility"], "PRIVATE");
    assert!(!f.root.join("repo-created.flag").exists());
}

#[test]
fn repository_creation_rejects_wrong_visibility_and_external_secrets_are_redacted() {
    let f = Fixture::new();
    let state = f.state_arg();
    let mut create = f.command();
    create.env("FIXTURE_REPO_ABSENT", "1").args([
        "create-repository",
        "--state-dir",
        &state,
        "--name",
        "fixture-owner/example",
        "--visibility",
        "public",
    ]);
    let output = create.output().unwrap();
    assert!(!output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value["message"].as_str().unwrap().contains("visibility"));

    let secret = "ghp_fixture_secret_that_must_not_escape";
    let mut account = f.command();
    account
        .env("FIXTURE_SECRET", secret)
        .args(["account", "--state-dir", &state]);
    let output = account.output().unwrap();
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!combined.contains(secret));
    assert!(combined.contains("REDACTED"));
}

#[test]
fn publish_is_exact_idempotent_and_records_merge_receipt() {
    let f = Fixture::new();
    let state = f.state_arg();
    let project = f.project_arg();
    let (out, _) = f.invoke(&[
        "connection",
        "save",
        "--state-dir",
        &state,
        "--project",
        &project,
        "--repository",
        "fixture-owner/example",
        "--base-branch",
        "main",
        "--expected-revision",
        "0",
    ]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let id = Uuid::new_v4();
    f.completed_run(id);
    let id_text = id.to_string();
    let (out, published) = f.invoke(&[
        "publish",
        "--state-dir",
        &state,
        "--run-id",
        &id_text,
        "--expected-revision",
        "1",
    ]);
    assert!(
        out.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(published["status"], "succeeded");
    assert_eq!(
        published["publication"]["candidate_commit"],
        f.candidate_head
    );
    assert_eq!(
        published["publication"]["pull_request_url"],
        "https://github.com/fixture-owner/example/pull/17"
    );
    let revision = published["revision"].as_u64().unwrap().to_string();
    let (again, repeated) = f.invoke(&[
        "publish",
        "--state-dir",
        &state,
        "--run-id",
        &id_text,
        "--expected-revision",
        &revision,
    ]);
    assert!(again.status.success());
    assert_eq!(
        repeated["revision"].as_u64(),
        published["revision"].as_u64()
    );
    assert!(f
        .state
        .join("github")
        .join("publications")
        .join(&id_text)
        .join("publication.json")
        .is_file());
    let (_, durable) = f.invoke(&["status", "--state-dir", &state]);
    assert_eq!(durable["publication_receipts"][&id_text]["verified"], true);
    assert_eq!(
        durable["publication_receipts"][&id_text]["revision"],
        durable["publications"][&id_text]["completion_revision"]
    );
}

#[test]
fn real_local_runner_candidate_is_publishable_with_exact_source_ancestry() {
    let f = Fixture::new();
    let state = f.state_arg();
    let project = f.project_arg();
    let (saved, _) = f.invoke(&[
        "connection",
        "save",
        "--state-dir",
        &state,
        "--project",
        &project,
        "--repository",
        "fixture-owner/example",
        "--base-branch",
        "main",
        "--expected-revision",
        "0",
    ]);
    assert!(saved.status.success());

    let model = LocalModel::start();
    let job_path = f.root.join("real-local-job.json");
    let validator = PathBuf::from(std::env::var_os("WINDIR").unwrap())
        .join("System32")
        .join("cmd.exe");
    fs::write(
        &job_path,
        serde_json::to_vec_pretty(&json!({
            "repository": f.project,
            "specification": "Create feature.txt containing implemented.",
            "allowed_files": ["feature.txt"],
            "validation": {
                "program": validator,
                "args": ["/D", "/C", "exit /B 0"],
                "timeout_seconds": 30
            },
            "endpoint": model.endpoint(),
            "model": "fixture-model",
            "max_attempts": 1
        }))
        .unwrap(),
    )
    .unwrap();
    let runner = Command::new(env!("CARGO_BIN_EXE_assemblywright-local"))
        .args(["run", "--job"])
        .arg(&job_path)
        .arg("--state-dir")
        .arg(&f.state)
        .output()
        .unwrap();
    model.join();
    assert!(
        runner.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&runner.stdout),
        String::from_utf8_lossy(&runner.stderr)
    );
    let report: Value = serde_json::from_slice(&runner.stdout).unwrap();
    let run_id = report["run_id"].as_str().unwrap();
    let candidate = PathBuf::from(report["candidate"].as_str().unwrap());
    let candidate_text = candidate.to_string_lossy();
    let candidate_for_git = PathBuf::from(
        candidate_text
            .strip_prefix(r"\\?\")
            .unwrap_or(candidate_text.as_ref()),
    );
    let candidate_head = report["candidate_commit"].as_str().unwrap();
    assert_eq!(git_text(&candidate, &["rev-parse", "HEAD^"]), f.source);
    assert!(git_text(&candidate, &["remote"]).is_empty());
    assert_eq!(git_text(&f.project, &["rev-parse", "HEAD"]), f.source);

    let mut publish = f.command();
    publish
        .env("FIXTURE_SOURCE_HEAD", &f.source)
        .env("FIXTURE_CANDIDATE_HEAD", candidate_head)
        .env("FIXTURE_CANDIDATE_REPO", &candidate_for_git)
        .args([
            "publish",
            "--state-dir",
            &state,
            "--run-id",
            run_id,
            "--expected-revision",
            "1",
        ]);
    let published = publish.output().unwrap();
    assert!(
        published.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&published.stdout),
        String::from_utf8_lossy(&published.stderr)
    );
    let value: Value = serde_json::from_slice(&published.stdout).unwrap();
    assert_eq!(value["status"], "succeeded");
    assert_eq!(value["publication"]["candidate_commit"], candidate_head);
}

#[test]
fn two_successive_publications_advance_the_connected_clean_source() {
    let f = Fixture::new();
    let state = f.state_arg();
    let project = f.project_arg();
    let (saved, _) = f.invoke(&[
        "connection",
        "save",
        "--state-dir",
        &state,
        "--project",
        &project,
        "--repository",
        "fixture-owner/example",
        "--base-branch",
        "main",
        "--expected-revision",
        "0",
    ]);
    assert!(saved.status.success());
    let first = Uuid::new_v4();
    f.completed_run(first);
    let first_text = first.to_string();
    let (first_out, first_value) = f.invoke(&[
        "publish",
        "--state-dir",
        &state,
        "--run-id",
        &first_text,
        "--expected-revision",
        "1",
    ]);
    assert!(
        first_out.status.success(),
        "{}",
        String::from_utf8_lossy(&first_out.stderr)
    );
    assert_eq!(
        git_text(&f.project, &["rev-parse", "HEAD"]),
        f.candidate_head
    );
    for name in ["pr.json", "merged.flag", "branch.txt"] {
        let path = f.root.join(name);
        if path.exists() {
            fs::remove_file(path).unwrap();
        }
    }
    let candidate2 = f.root.join("candidate-two");
    run(Command::new("git")
        .args(["clone", "--quiet"])
        .arg(&f.project)
        .arg(&candidate2));
    fs::write(candidate2.join("feature-two.txt"), "second\n").unwrap();
    run(Command::new("git")
        .arg("-C")
        .arg(&candidate2)
        .args(["add", "feature-two.txt"]));
    run(Command::new("git").arg("-C").arg(&candidate2).args([
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "--quiet",
        "-m",
        "second feature",
    ]));
    run(Command::new("git")
        .arg("-C")
        .arg(&candidate2)
        .args(["remote", "remove", "origin"]));
    let second_head = git_text(&candidate2, &["rev-parse", "HEAD"]);
    let second = Uuid::new_v4();
    f.completed_run_with(second, &f.candidate_head, &candidate2, &second_head);
    let second_text = second.to_string();
    let revision = first_value["revision"].as_u64().unwrap().to_string();
    let mut command = f.command();
    command
        .env("FIXTURE_SOURCE_HEAD", &f.candidate_head)
        .env("FIXTURE_CANDIDATE_HEAD", &second_head)
        .env("FIXTURE_CANDIDATE_REPO", &candidate2)
        .args([
            "publish",
            "--state-dir",
            &state,
            "--run-id",
            &second_text,
            "--expected-revision",
            &revision,
        ]);
    let second_out = command.output().unwrap();
    assert!(
        second_out.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&second_out.stdout),
        String::from_utf8_lossy(&second_out.stderr)
    );
    assert_eq!(git_text(&f.project, &["rev-parse", "HEAD"]), second_head);
    assert!(git_text(
        &f.project,
        &["status", "--porcelain=v1", "--untracked-files=all"]
    )
    .is_empty());
}

#[test]
fn missing_success_receipt_is_downgraded_to_attention() {
    let f = Fixture::new();
    let state = f.state_arg();
    let project = f.project_arg();
    let (saved, _) = f.invoke(&[
        "connection",
        "save",
        "--state-dir",
        &state,
        "--project",
        &project,
        "--repository",
        "fixture-owner/example",
        "--base-branch",
        "main",
        "--expected-revision",
        "0",
    ]);
    assert!(saved.status.success());
    let id = Uuid::new_v4();
    f.completed_run(id);
    let id_text = id.to_string();
    let (published, value) = f.invoke(&[
        "publish",
        "--state-dir",
        &state,
        "--run-id",
        &id_text,
        "--expected-revision",
        "1",
    ]);
    assert!(published.status.success());
    fs::remove_file(
        f.state
            .join("github")
            .join("publications")
            .join(&id_text)
            .join("publication.json"),
    )
    .unwrap();
    let revision = value["revision"].as_u64().unwrap().to_string();
    let (disconnect, disconnect_value) = f.invoke(&[
        "connection",
        "disconnect",
        "--state-dir",
        &state,
        "--project",
        &project,
        "--expected-revision",
        &revision,
    ]);
    assert!(!disconnect.status.success());
    assert!(disconnect_value["message"]
        .as_str()
        .unwrap()
        .contains("unresolved publication"));
    let (resave, resave_value) = f.invoke(&[
        "connection",
        "save",
        "--state-dir",
        &state,
        "--project",
        &project,
        "--repository",
        "fixture-owner/example",
        "--base-branch",
        "main",
        "--expected-revision",
        &revision,
    ]);
    assert!(!resave.status.success());
    assert!(resave_value["message"]
        .as_str()
        .unwrap()
        .contains("unresolved publication"));
    let (_, status) = f.invoke(&["status", "--state-dir", &state]);
    assert_eq!(status["publications"][&id_text]["status"], "attention");
    assert_eq!(status["publication_receipts"][&id_text]["verified"], false);
    assert!(status["revision"].as_u64().unwrap() > value["revision"].as_u64().unwrap());
}

#[test]
fn pull_request_merged_during_creation_still_syncs_connected_source() {
    let f = Fixture::new();
    let state = f.state_arg();
    let project = f.project_arg();
    let (saved, _) = f.invoke(&[
        "connection",
        "save",
        "--state-dir",
        &state,
        "--project",
        &project,
        "--repository",
        "fixture-owner/example",
        "--base-branch",
        "main",
        "--expected-revision",
        "0",
    ]);
    assert!(saved.status.success());
    let id = Uuid::new_v4();
    f.completed_run(id);
    let id_text = id.to_string();
    let mut command = f.command();
    command.env("FIXTURE_MERGED_ON_CREATE", "1").args([
        "publish",
        "--state-dir",
        &state,
        "--run-id",
        &id_text,
        "--expected-revision",
        "1",
    ]);
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["status"], "succeeded");
    assert_eq!(
        git_text(&f.project, &["rev-parse", "HEAD"]),
        f.candidate_head
    );
    let (_, durable) = f.invoke(&["status", "--state-dir", &state]);
    assert_eq!(durable["connections"][0]["source_head"], f.candidate_head);
    assert_eq!(durable["publication_receipts"][&id_text]["verified"], true);
}

#[test]
fn structured_check_polling_handles_arrival_pending_and_pass() {
    let f = Fixture::new();
    let state = f.state_arg();
    let project = f.project_arg();
    let (saved, _) = f.invoke(&[
        "connection",
        "save",
        "--state-dir",
        &state,
        "--project",
        &project,
        "--repository",
        "fixture-owner/example",
        "--base-branch",
        "main",
        "--expected-revision",
        "0",
    ]);
    assert!(saved.status.success());
    let id = Uuid::new_v4();
    f.completed_run(id);
    let id_text = id.to_string();
    let mut command = f.command();
    command.env("FIXTURE_CHECK_ARRIVAL", "1").args([
        "publish",
        "--state-dir",
        &state,
        "--run-id",
        &id_text,
        "--expected-revision",
        "1",
    ]);
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        fs::read_to_string(f.root.join("checks-count.txt"))
            .unwrap()
            .trim(),
        "3"
    );
    assert!(f.root.join("merged.flag").exists());
}

#[test]
fn pending_structured_checks_are_promptly_interruptible_by_cancel_and_pause() {
    for cancel in [true, false] {
        let f = Fixture::new();
        let state = f.state_arg();
        let project = f.project_arg();
        let (saved, _) = f.invoke(&[
            "connection",
            "save",
            "--state-dir",
            &state,
            "--project",
            &project,
            "--repository",
            "fixture-owner/example",
            "--base-branch",
            "main",
            "--expected-revision",
            "0",
        ]);
        assert!(saved.status.success());
        let id = Uuid::new_v4();
        f.completed_run(id);
        let id_text = id.to_string();
        let mut command = f.command();
        command
            .env("FIXTURE_PENDING_CHECKS", "1")
            .args([
                "publish",
                "--state-dir",
                &state,
                "--run-id",
                &id_text,
                "--expected-revision",
                "1",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = command.spawn().unwrap();
        let arrival_deadline = Instant::now() + Duration::from_secs(15);
        while !f.root.join("checks-count.txt").exists() && Instant::now() < arrival_deadline {
            thread::sleep(Duration::from_millis(50));
        }
        assert!(f.root.join("checks-count.txt").exists());
        let interrupted_at = Instant::now();
        if cancel {
            let (cancelled, _) = f.invoke(&["cancel", "--state-dir", &state]);
            assert!(cancelled.status.success());
            assert!(f.state.join("github").join("cancel.flag").exists());
        } else {
            fs::write(f.state.join("pause.flag"), "paused\n").unwrap();
        }
        let output = child.wait_with_output().unwrap();
        assert!(!output.status.success());
        assert!(
            interrupted_at.elapsed() < Duration::from_secs(3),
            "pending check wait did not stop promptly"
        );
        assert!(!f.root.join("merged.flag").exists());
        let (_, status) = f.invoke(&["status", "--state-dir", &state]);
        assert_eq!(
            status["publications"][&id_text]["status"],
            if cancel { "cancelled" } else { "attention" },
            "stdout={} stderr={} durable={status}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn failed_and_unknown_structured_checks_never_merge() {
    for flag in ["FIXTURE_FAILED_CHECKS", "FIXTURE_UNKNOWN_CHECKS"] {
        let f = Fixture::new();
        let state = f.state_arg();
        let project = f.project_arg();
        let (saved, _) = f.invoke(&[
            "connection",
            "save",
            "--state-dir",
            &state,
            "--project",
            &project,
            "--repository",
            "fixture-owner/example",
            "--base-branch",
            "main",
            "--expected-revision",
            "0",
        ]);
        assert!(saved.status.success());
        let id = Uuid::new_v4();
        f.completed_run(id);
        let id_text = id.to_string();
        let mut command = f.command();
        command.env(flag, "1").args([
            "publish",
            "--state-dir",
            &state,
            "--run-id",
            &id_text,
            "--expected-revision",
            "1",
        ]);
        let output = command.output().unwrap();
        assert!(!output.status.success());
        assert!(!f.root.join("merged.flag").exists());
        let (_, status) = f.invoke(&["status", "--state-dir", &state]);
        assert_eq!(status["publications"][&id_text]["status"], "attention");
    }
}

#[test]
fn pending_structured_checks_stop_at_the_bounded_deadline_without_merge() {
    let f = Fixture::new();
    let state = f.state_arg();
    let project = f.project_arg();
    let (saved, _) = f.invoke(&[
        "connection",
        "save",
        "--state-dir",
        &state,
        "--project",
        &project,
        "--repository",
        "fixture-owner/example",
        "--base-branch",
        "main",
        "--expected-revision",
        "0",
    ]);
    assert!(saved.status.success());
    let id = Uuid::new_v4();
    f.completed_run(id);
    let id_text = id.to_string();
    let mut command = f.command_with_check_timeout(1_200);
    command.env("FIXTURE_PENDING_CHECKS", "1").args([
        "publish",
        "--state-dir",
        &state,
        "--run-id",
        &id_text,
        "--expected-revision",
        "1",
    ]);
    let started = Instant::now();
    let output = command.output().unwrap();
    assert!(!output.status.success());
    assert!(started.elapsed() < Duration::from_secs(15));
    assert!(String::from_utf8_lossy(&output.stdout).contains("bounded deadline"));
    assert!(!f.root.join("merged.flag").exists());
}

#[test]
fn publication_fails_closed_on_pause_and_missing_required_checks() {
    let f = Fixture::new();
    let state = f.state_arg();
    let project = f.project_arg();
    let (out, _) = f.invoke(&[
        "connection",
        "save",
        "--state-dir",
        &state,
        "--project",
        &project,
        "--repository",
        "fixture-owner/example",
        "--base-branch",
        "main",
        "--expected-revision",
        "0",
    ]);
    assert!(out.status.success());
    let id = Uuid::new_v4();
    f.completed_run(id);
    let id_text = id.to_string();
    fs::write(f.state.join("pause.flag"), "paused\n").unwrap();
    let (paused, value) = f.invoke(&[
        "publish",
        "--state-dir",
        &state,
        "--run-id",
        &id_text,
        "--expected-revision",
        "1",
    ]);
    assert!(!paused.status.success());
    assert!(value["message"]
        .as_str()
        .unwrap()
        .contains("Emergency Pause"));
    fs::remove_file(f.state.join("pause.flag")).unwrap();
    let (failed, value) = {
        let mut c = f.command();
        c.env("FIXTURE_EMPTY_CHECKS", "1").args([
            "publish",
            "--state-dir",
            &state,
            "--run-id",
            &id_text,
            "--expected-revision",
            "1",
        ]);
        let output = c.output().unwrap();
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        (output, value)
    };
    assert!(!failed.status.success());
    assert!(value["message"]
        .as_str()
        .unwrap()
        .contains("no required checks"));
    let (_, status) = f.invoke(&["status", "--state-dir", &state]);
    assert_eq!(status["publications"][&id_text]["status"], "attention");
    let revision = status["revision"].as_u64().unwrap().to_string();
    let (resave, resave_value) = f.invoke(&[
        "connection",
        "save",
        "--state-dir",
        &state,
        "--project",
        &project,
        "--repository",
        "fixture-owner/example",
        "--base-branch",
        "main",
        "--expected-revision",
        &revision,
    ]);
    assert!(!resave.status.success());
    assert!(resave_value["message"]
        .as_str()
        .unwrap()
        .contains("unresolved publication"));
    let (retry, retry_value) = f.invoke(&[
        "publish",
        "--state-dir",
        &state,
        "--run-id",
        &id_text,
        "--expected-revision",
        &revision,
    ]);
    assert!(!retry.status.success());
    assert!(retry_value["message"]
        .as_str()
        .unwrap()
        .contains("explicit reconcile"));
}

#[test]
fn cancellation_terminates_external_work_and_retains_cancelled_status() {
    let f = Fixture::new();
    let state = f.state_arg();
    let project = f.project_arg();
    let (out, _) = f.invoke(&[
        "connection",
        "save",
        "--state-dir",
        &state,
        "--project",
        &project,
        "--repository",
        "fixture-owner/example",
        "--base-branch",
        "main",
        "--expected-revision",
        "0",
    ]);
    assert!(out.status.success());
    let id = Uuid::new_v4();
    f.completed_run(id);
    let id_text = id.to_string();
    let mut command = f.command();
    command
        .env("FIXTURE_SLOW_CHECKS", "1")
        .args([
            "publish",
            "--state-dir",
            &state,
            "--run-id",
            &id_text,
            "--expected-revision",
            "1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = command.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(15);
    while !f.root.join("pr.json").exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(100));
    }
    assert!(
        f.root.join("pr.json").exists(),
        "publication never reached pull request stage"
    );
    thread::sleep(Duration::from_millis(500));
    let (cancel_out, cancel) = f.invoke(&["cancel", "--state-dir", &state]);
    assert!(cancel_out.status.success());
    assert_eq!(cancel["cancelled"], true);
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("cancelled or paused"));
    let (_, status) = f.invoke(&["status", "--state-dir", &state]);
    assert_eq!(status["publications"][&id_text]["status"], "cancelled");
}

#[test]
fn explicit_reconcile_recovers_an_ambiguous_completed_merge() {
    let f = Fixture::new();
    let state = f.state_arg();
    let project = f.project_arg();
    let (saved, _) = f.invoke(&[
        "connection",
        "save",
        "--state-dir",
        &state,
        "--project",
        &project,
        "--repository",
        "fixture-owner/example",
        "--base-branch",
        "main",
        "--expected-revision",
        "0",
    ]);
    assert!(saved.status.success());
    let id = Uuid::new_v4();
    f.completed_run(id);
    let id_text = id.to_string();
    let mut command = f.command();
    command.env("FIXTURE_MERGE_AMBIGUOUS", "1").args([
        "publish",
        "--state-dir",
        &state,
        "--run-id",
        &id_text,
        "--expected-revision",
        "1",
    ]);
    let first = command.output().unwrap();
    assert!(!first.status.success());
    let (_, observed) = f.invoke(&["status", "--state-dir", &state]);
    assert_eq!(observed["publications"][&id_text]["status"], "attention");
    let revision = observed["revision"].as_u64().unwrap().to_string();
    let (reconciled, value) = f.invoke(&[
        "reconcile",
        "--state-dir",
        &state,
        "--run-id",
        &id_text,
        "--expected-revision",
        &revision,
    ]);
    assert!(
        reconciled.status.success(),
        "{}",
        String::from_utf8_lossy(&reconciled.stderr)
    );
    assert_eq!(value["status"], "succeeded");
    assert_eq!(value["publication"]["merge_commit"], f.candidate_head);
}

#[test]
fn production_rejects_fixture_program_injection() {
    let f = Fixture::new();
    let output = Command::new(env!("CARGO_BIN_EXE_assemblywright-github"))
        .arg("--fixture-gh")
        .arg(&f.gh)
        .arg("account")
        .arg("--state-dir")
        .arg(&f.state)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value["message"].as_str().unwrap().contains("test-only"));

    let output = Command::new(env!("CARGO_BIN_EXE_assemblywright-github"))
        .args(["--fixture-check-timeout-ms", "1", "account", "--state-dir"])
        .arg(&f.state)
        .output()
        .unwrap();
    assert!(!output.status.success());
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value["message"].as_str().unwrap().contains("test-only"));
}

struct LocalModel {
    address: SocketAddr,
    thread: Option<thread::JoinHandle<()>>,
}

impl LocalModel {
    fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let thread = thread::spawn(move || {
            for response in [
                json!({"object":"list","data":[]}).to_string(),
                json!({"choices":[{"message":{"role":"assistant","content":"{\"edits\":[{\"path\":\"feature.txt\",\"content\":\"implemented\\n\"}]}"}}]}).to_string(),
                json!({"choices":[{"message":{"role":"assistant","content":"{\"approve\":true,\"findings\":[]}"}}]}).to_string(),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                read_http_request(&mut stream);
                write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", response.len(), response).unwrap();
            }
        });
        Self {
            address,
            thread: Some(thread),
        }
    }

    fn endpoint(&self) -> String {
        format!("http://{}/v1", self.address)
    }

    fn join(mut self) {
        self.thread.take().unwrap().join().unwrap();
    }
}

fn read_http_request(stream: &mut TcpStream) {
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
            if let Some(split) = bytes.windows(4).position(|value| value == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&bytes[..split]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(str::trim)
                            .and_then(|value| value.parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                expected = Some(split + 4 + length);
            }
        }
        if expected.is_some_and(|size| bytes.len() >= size) {
            break;
        }
    }
}

fn init_repo(path: &Path) {
    fs::create_dir_all(path).unwrap();
    run(Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["init", "--quiet", "--initial-branch=main"]));
    fs::write(path.join("README.md"), "baseline\n").unwrap();
    run(Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["add", "README.md"]));
    run(Command::new("git").arg("-C").arg(path).args([
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "--quiet",
        "-m",
        "baseline",
    ]));
    run(Command::new("git").arg("-C").arg(path).args([
        "remote",
        "add",
        "origin",
        "https://github.com/fixture-owner/example.git",
    ]));
}

fn git_text(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .unwrap();
    assert!(output.status.success());
    String::from_utf8(output.stdout).unwrap().trim().into()
}
fn run(command: &mut Command) {
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
fn which(name: &str) -> PathBuf {
    let output = Command::new("where.exe").arg(name).output().unwrap();
    assert!(output.status.success());
    PathBuf::from(
        String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .next()
            .unwrap(),
    )
}

const FAKE_GIT: &str = r#"
$ErrorActionPreference = 'Stop'
if ($args -contains 'push') {
  Set-Content -LiteralPath (Join-Path $env:FIXTURE_DIR 'branch.txt') -Value $env:FIXTURE_CANDIDATE_HEAD -NoNewline
  Write-Output 'ok refs/heads/assemblywright fixture'
  exit 0
}
if ($args -contains 'fetch') {
  $cwdIndex = [Array]::IndexOf($args, '-C'); $cwd = $args[$cwdIndex + 1]
  & $env:FIXTURE_REAL_GIT -C $cwd fetch --quiet $env:FIXTURE_CANDIDATE_REPO $env:FIXTURE_CANDIDATE_HEAD
  if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
  & $env:FIXTURE_REAL_GIT -C $cwd update-ref refs/remotes/origin/main $env:FIXTURE_CANDIDATE_HEAD
  exit $LASTEXITCODE
}
& $env:FIXTURE_REAL_GIT @args
exit $LASTEXITCODE
"#;

const FAKE_GH: &str = r#"
$ErrorActionPreference = 'Stop'
$joined = $args -join ' '
$pr = Join-Path $env:FIXTURE_DIR 'pr.json'
$merged = Join-Path $env:FIXTURE_DIR 'merged.flag'
$visibility = if ($env:FIXTURE_VISIBILITY) { $env:FIXTURE_VISIBILITY } else { 'PRIVATE' }
if ($joined -eq 'auth status --hostname github.com' -and $env:FIXTURE_SECRET) { [Console]::Error.WriteLine($env:FIXTURE_SECRET); exit 1 }
if ($joined -eq 'auth status --hostname github.com') { Write-Output 'signed in'; exit 0 }
if ($joined -eq 'api user --jq .login') { Write-Output 'fixture-owner'; exit 0 }
if ($args[0] -eq 'repo' -and $args[1] -eq 'list') {
  Write-Output ('[{"nameWithOwner":"fixture-owner/example","url":"https://github.com/fixture-owner/example","visibility":"' + $visibility + '","defaultBranchRef":{"name":"main"}}]'); exit 0
}
if ($args[0] -eq 'repo' -and $args[1] -eq 'view') {
  if ($env:FIXTURE_REPO_VIEW_ERROR) { [Console]::Error.WriteLine($env:FIXTURE_REPO_VIEW_ERROR); exit 1 }
  if ($env:FIXTURE_REPO_ABSENT -eq '1' -and -not (Test-Path (Join-Path $env:FIXTURE_DIR 'repo-created.flag'))) {
    [Console]::Error.WriteLine("GraphQL: Could not resolve to a Repository with the name '" + $args[2] + "'. (repository)"); exit 1
  }
  Write-Output ('{"nameWithOwner":"fixture-owner/example","url":"https://github.com/fixture-owner/example","visibility":"' + $visibility + '","defaultBranchRef":{"name":"main"}}'); exit 0
}
if ($args[0] -eq 'repo' -and $args[1] -eq 'create') { Set-Content -LiteralPath (Join-Path $env:FIXTURE_DIR 'repo-created.flag') -Value '1' -NoNewline; Write-Output 'created'; exit 0 }
if ($args[0] -eq 'repo' -and $args[1] -eq 'clone') { & $env:FIXTURE_REAL_GIT clone --quiet (Join-Path $env:FIXTURE_DIR 'project') $args[3]; if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }; & $env:FIXTURE_REAL_GIT -C $args[3] remote set-url origin 'https://github.com/fixture-owner/example.git'; exit $LASTEXITCODE }
if ($args[0] -eq 'api') {
  if ($args[1] -like '*assemblywright/*') {
    $branch = Join-Path $env:FIXTURE_DIR 'branch.txt'; if (Test-Path $branch) { Get-Content $branch; exit 0 }; [Console]::Error.WriteLine('HTTP 404'); exit 1
  }
  if ($args[1] -like '*/git/commits/*') { Write-Output ('{"parents":[{"sha":"' + $env:FIXTURE_SOURCE_HEAD + '"},{"sha":"' + $env:FIXTURE_CANDIDATE_HEAD + '"}]}'); exit 0 }
  if (Test-Path $merged) { Write-Output $env:FIXTURE_CANDIDATE_HEAD } else { Write-Output $env:FIXTURE_SOURCE_HEAD }; exit 0
}
if ($args[0] -eq 'pr' -and $args[1] -eq 'create') {
  Set-Content -LiteralPath $pr -Value '{}' -NoNewline
  $headIndex = [Array]::IndexOf($args, '--head'); Set-Content -LiteralPath (Join-Path $env:FIXTURE_DIR 'pr-head.txt') -Value $args[$headIndex + 1] -NoNewline
  if ($env:FIXTURE_MERGED_ON_CREATE -eq '1') { Set-Content -LiteralPath $merged -Value '1' -NoNewline }
  Write-Output 'https://github.com/fixture-owner/example/pull/17'; exit 0
}
if ($args[0] -eq 'pr' -and $args[1] -eq 'list') {
  if (-not (Test-Path $pr)) { Write-Output '[]'; exit 0 }
  $state = if (Test-Path $merged) { 'MERGED' } else { 'OPEN' }
  $merge = if (Test-Path $merged) { ',"mergeCommit":{"oid":"' + $env:FIXTURE_CANDIDATE_HEAD + '"}' } else { ',"mergeCommit":null' }
  $headIndex = [Array]::IndexOf($args, '--head'); $head = $args[$headIndex + 1]
  Write-Output ('[{"number":17,"url":"https://github.com/fixture-owner/example/pull/17","state":"' + $state + '","headRefOid":"' + $env:FIXTURE_CANDIDATE_HEAD + '","headRefName":"' + $head + '","baseRefName":"main","isCrossRepository":false' + $merge + '}]'); exit 0
}
if ($args[0] -eq 'pr' -and $args[1] -eq 'checks') {
  if ($args -contains '--watch' -and $args -contains '--json') { [Console]::Error.WriteLine('cannot use `--watch` with `--json` flag'); exit 1 }
  $countPath = Join-Path $env:FIXTURE_DIR 'checks-count.txt'; $count = if (Test-Path $countPath) { [int](Get-Content $countPath) + 1 } else { 1 }; Set-Content -LiteralPath $countPath -Value $count -NoNewline
  if ($env:FIXTURE_SLOW_CHECKS -eq '1') { Start-Sleep -Seconds 30 }
  if ($env:FIXTURE_CHECK_ARRIVAL -eq '1' -and $count -eq 1) { $head = Get-Content (Join-Path $env:FIXTURE_DIR 'pr-head.txt'); [Console]::Error.WriteLine("no required checks reported on the '" + $head + "' branch"); exit 1 }
  if (($env:FIXTURE_CHECK_ARRIVAL -eq '1' -and $count -eq 2) -or $env:FIXTURE_PENDING_CHECKS -eq '1') { Write-Output '[{"bucket":"pending","name":"test","state":"IN_PROGRESS","workflow":"CI"}]'; exit 8 }
  if ($env:FIXTURE_EMPTY_CHECKS -eq '1') { Write-Output '[]'; exit 0 }
  if ($env:FIXTURE_FAILED_CHECKS -eq '1') { Write-Output '[{"bucket":"fail","name":"test","state":"FAILURE","workflow":"CI"}]'; exit 0 }
  if ($env:FIXTURE_UNKNOWN_CHECKS -eq '1') { Write-Output '[{"bucket":"mystery","name":"test","state":"ALIEN","workflow":"CI"}]'; exit 0 }
  Write-Output '[{"bucket":"pass","name":"test","state":"SUCCESS","workflow":"CI"}]'; exit 0
}
if ($args[0] -eq 'pr' -and $args[1] -eq 'merge') {
  $matchIndex = [Array]::IndexOf($args, '--match-head-commit')
  if ($matchIndex -lt 0 -or $args[$matchIndex + 1] -ne $env:FIXTURE_CANDIDATE_HEAD) { [Console]::Error.WriteLine('missing exact head guard'); exit 3 }
  Set-Content -LiteralPath $merged -Value '1' -NoNewline
  if ($env:FIXTURE_MERGE_AMBIGUOUS -eq '1') { [Console]::Error.WriteLine('network response lost'); exit 1 }
  exit 0
}
Write-Error ('unexpected gh invocation: ' + $joined); exit 2
"#;
