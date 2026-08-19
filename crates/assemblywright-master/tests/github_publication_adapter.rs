use assemblywright_master::{
    credential_git_process_boundary, sanitized_publication_command_path,
    validate_github_branch_protection_observation, validate_github_required_checks_observation,
    validate_github_workflow_content, validate_proof_cleanup_status, validate_proof_source_binding,
    validate_remote_base_observation, GithubPublicationLiveProofReceipt, ProcessGithubPublication,
    PublicationAdapterError, PublicationExecutionControl,
};
use serde_json::json;
#[cfg(not(windows))]
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[test]
fn default_configuration_is_unavailable_without_creating_state() {
    let directory = tempfile::tempdir().unwrap();
    assert!(ProcessGithubPublication::load(directory.path())
        .unwrap()
        .is_none());
    assert!(fs::read_dir(directory.path()).unwrap().next().is_none());
}

#[test]
#[cfg(not(windows))]
fn configuration_rejects_paths_commands_plaintext_tokens_and_identity_drift() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("github-publication");
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join("gh-config")).unwrap();
    fs::write(
        root.join("gh-config/hosts.yml"),
        b"github.com:\n  user: malak333\n",
    )
    .unwrap();
    fs::write(root.join("gh"), b"fixed-gh").unwrap();
    fs::write(root.join("git"), b"fixed-git").unwrap();
    make_private(&root, true);
    make_private(&root.join("gh-config"), true);
    for file in [
        root.join("gh-config/hosts.yml"),
        root.join("gh"),
        root.join("git"),
    ] {
        make_private(&file, false);
    }
    make_executable(&root.join("gh"));
    make_executable(&root.join("git"));

    let mut config = valid_config(b"fixed-gh", b"fixed-git");
    config["git_path"] = json!(r"C:\untrusted\git.exe");
    write_config(&root, &config);
    assert!(ProcessGithubPublication::load(directory.path()).is_err());

    write_config(&root, &valid_config(b"fixed-gh", b"fixed-git"));
    fs::write(
        root.join("gh-config/hosts.yml"),
        b"github.com:\n  oauth_token: forbidden\n",
    )
    .unwrap();
    assert!(ProcessGithubPublication::load(directory.path()).is_err());

    fs::write(
        root.join("gh-config/hosts.yml"),
        b"github.com:\n  user: malak333\n",
    )
    .unwrap();
    let runtime = ProcessGithubPublication::load(directory.path())
        .unwrap()
        .unwrap();
    fs::write(root.join("gh"), b"swapped-gh").unwrap();
    make_executable(&root.join("gh"));
    assert_eq!(
        runtime.verify_provisioned_assets(),
        Err(PublicationAdapterError::Unavailable)
    );
    fs::write(root.join("gh"), b"fixed-gh").unwrap();
    make_executable(&root.join("gh"));
    let mut stale_master = valid_config(b"fixed-gh", b"fixed-git");
    stale_master["master_executable_sha256"] = json!("00".repeat(32));
    write_config(&root, &stale_master);
    assert!(ProcessGithubPublication::load(directory.path()).is_err());
    write_config(&root, &valid_config(b"fixed-gh", b"fixed-git"));
    fs::write(root.join("git"), b"drifted-git").unwrap();
    make_executable(&root.join("git"));
    assert!(ProcessGithubPublication::load(directory.path()).is_err());
}

#[test]
fn branch_protection_parser_requires_strict_admin_no_bypass_policy() {
    let protected = protection_value();
    assert_eq!(
        validate_github_branch_protection_observation(&protected),
        Ok(())
    );

    let mut admin_bypass = protected.clone();
    admin_bypass["enforce_admins"]["enabled"] = json!(false);
    assert_eq!(
        validate_github_branch_protection_observation(&admin_bypass),
        Err(PublicationAdapterError::MissingEvidence)
    );

    let mut actor_bypass = protected;
    actor_bypass["required_pull_request_reviews"]["bypass_pull_request_allowances"]["users"] =
        json!([{"login":"bypass"}]);
    assert_eq!(
        validate_github_branch_protection_observation(&actor_bypass),
        Err(PublicationAdapterError::MissingEvidence)
    );

    let mut actual_shape = protection_value();
    actual_shape["required_pull_request_reviews"]
        .as_object_mut()
        .unwrap()
        .remove("bypass_pull_request_allowances");
    assert_eq!(
        validate_github_branch_protection_observation(&actual_shape),
        Ok(())
    );
    let mut missing_parent = actual_shape.clone();
    missing_parent
        .as_object_mut()
        .unwrap()
        .remove("required_pull_request_reviews");
    assert_eq!(
        validate_github_branch_protection_observation(&missing_parent),
        Err(PublicationAdapterError::MissingEvidence)
    );
    let mut extra_check = actual_shape.clone();
    extra_check["required_status_checks"]["checks"]
        .as_array_mut()
        .unwrap()
        .push(json!({"context":"hostile-extra","app_id":15368}));
    assert_eq!(
        validate_github_branch_protection_observation(&extra_check),
        Err(PublicationAdapterError::MissingEvidence)
    );
    let mut duplicate_check = actual_shape.clone();
    duplicate_check["required_status_checks"]["checks"]
        .as_array_mut()
        .unwrap()
        .push(json!({"context":"Release local gate","app_id":15368}));
    assert_eq!(
        validate_github_branch_protection_observation(&duplicate_check),
        Err(PublicationAdapterError::MissingEvidence)
    );
    actual_shape["required_conversation_resolution"]["enabled"] = json!(false);
    assert_eq!(
        validate_github_branch_protection_observation(&actual_shape),
        Err(PublicationAdapterError::MissingEvidence)
    );
}

#[test]
fn required_checks_parser_pins_github_app_and_uses_latest_run() {
    let commit = "1111111111111111111111111111111111111111";
    let exact = json!({
        "check_runs": [
            {"id": 10, "name": "Release local gate", "status": "completed", "conclusion": "success", "app":{"id":15368},"details_url":"https://github.com/malak333/Assemblywright/actions/runs/100/job/1"},
            {"id": 11, "name": "Protocol, master, identity, mTLS, and SCM", "status": "completed", "conclusion": "success", "app":{"id":15368},"details_url":"https://github.com/malak333/Assemblywright/actions/runs/101/job/2"}
        ]
    });
    let workflows = workflow_runs(commit);
    assert_ne!(
        validate_github_required_checks_observation(&exact, &workflows, commit).unwrap(),
        [0; 32]
    );

    let duplicate = json!({
        "check_runs": [
            {"id": 10, "name": "Release local gate", "status": "completed", "conclusion": "failure", "app":{"id":15368},"details_url":"https://github.com/malak333/Assemblywright/actions/runs/99/job/1"},
            {"id": 12, "name": "Release local gate", "status": "completed", "conclusion": "success", "app":{"id":15368},"details_url":"https://github.com/malak333/Assemblywright/actions/runs/100/job/1"},
            {"id": 11, "name": "Protocol, master, identity, mTLS, and SCM", "status": "completed", "conclusion": "success", "app":{"id":15368},"details_url":"https://github.com/malak333/Assemblywright/actions/runs/101/job/2"}
        ]
    });
    assert!(validate_github_required_checks_observation(&duplicate, &workflows, commit).is_ok());
    let mut forged = exact;
    forged["check_runs"][0]["app"]["id"] = json!(1);
    assert_eq!(
        validate_github_required_checks_observation(&forged, &workflows, commit),
        Err(PublicationAdapterError::MissingEvidence)
    );

    let mut hostile_workflow = workflows;
    hostile_workflow[0]["workflow_id"] = json!(314849303);
    assert_eq!(
        validate_github_required_checks_observation(&duplicate, &hostile_workflow, commit),
        Err(PublicationAdapterError::MissingEvidence)
    );

    let trusted_release = normalized_lf(include_bytes!(
        "../../../.github/workflows/release-local.yml"
    ));
    assert_eq!(
        validate_github_workflow_content(
            ".github/workflows/release-local.yml",
            "51e809a94f59193e213bdff6e49f3a86e612643f094e366055f42f8745026fd7",
            &trusted_release,
        ),
        Ok(())
    );
    let mut replaced = trusted_release;
    replaced.extend_from_slice(b"\n# hostile same-name workflow\n");
    assert_eq!(
        validate_github_workflow_content(
            ".github/workflows/release-local.yml",
            "51e809a94f59193e213bdff6e49f3a86e612643f094e366055f42f8745026fd7",
            &replaced,
        ),
        Err(PublicationAdapterError::MissingEvidence)
    );
}

#[test]
fn execution_control_and_proof_receipt_fail_closed() {
    let cancelled = Arc::new(AtomicBool::new(false));
    let authority = Arc::new(AtomicBool::new(true));
    let authority_for_control = Arc::clone(&authority);
    let control = PublicationExecutionControl::new(
        Arc::clone(&cancelled),
        Instant::now() + Duration::from_secs(1),
        Arc::new(move || authority_for_control.load(Ordering::Acquire)),
    );
    assert_eq!(control.poll(), Ok(()));
    authority.store(false, Ordering::Release);
    assert_eq!(control.poll(), Err(PublicationAdapterError::Cancelled));

    let expired = PublicationExecutionControl::new(
        Arc::new(AtomicBool::new(false)),
        Instant::now() - Duration::from_millis(1),
        Arc::new(|| true),
    );
    assert_eq!(
        expired.poll(),
        Err(PublicationAdapterError::DeadlineExceeded)
    );

    let mut receipt = valid_receipt();
    assert_eq!(receipt.validate(), Ok(()));
    receipt.repository = "/private/master/path".to_string();
    assert_eq!(
        receipt.validate(),
        Err(PublicationAdapterError::MissingEvidence)
    );
}

#[test]
fn source_base_and_sanitized_path_bindings_reject_drift() {
    let base = "1111111111111111111111111111111111111111";
    let drift = "2222222222222222222222222222222222222222";
    assert_eq!(validate_remote_base_observation(base, base), Ok(()));
    assert_eq!(
        validate_remote_base_observation(drift, base),
        Err(PublicationAdapterError::MissingEvidence)
    );
    assert_eq!(
        validate_proof_source_binding(base, base, base, base),
        Ok(())
    );
    assert_eq!(
        validate_proof_source_binding(base, base, base, drift),
        Err(PublicationAdapterError::AmbiguousEffect)
    );
    assert_eq!(validate_proof_cleanup_status(true, true), Ok(()));
    assert_eq!(
        validate_proof_cleanup_status(false, true),
        Err(PublicationAdapterError::AmbiguousEffect)
    );
    assert_eq!(
        validate_proof_cleanup_status(true, false),
        Err(PublicationAdapterError::AmbiguousEffect)
    );

    #[cfg(not(windows))]
    let (root, candidate, git, gh) = (
        Path::new("/fixed/publication"),
        Path::new("/fixed/feature-conveyor-candidates/candidate"),
        Path::new("/fixed/git/bin/git"),
        Path::new("/fixed/gh/bin/gh"),
    );
    #[cfg(windows)]
    let (root, candidate, git, gh) = (
        Path::new(r"C:\fixed\publication"),
        Path::new(r"C:\fixed\feature-conveyor-candidates\candidate"),
        Path::new(r"C:\fixed\git\bin\git.exe"),
        Path::new(r"C:\fixed\gh\bin\gh.exe"),
    );
    let path = sanitized_publication_command_path(git, gh, root).unwrap();
    let entries = std::env::split_paths(&path).collect::<Vec<_>>();
    assert_eq!(
        entries,
        vec![git.parent().unwrap(), gh.parent().unwrap(), root]
    );
    assert!(!entries.iter().any(|entry| entry == candidate));
    let (credential_cwd, git_dir) = credential_git_process_boundary(root, candidate).unwrap();
    assert_eq!(credential_cwd, root);
    assert_ne!(credential_cwd, candidate);
    assert_eq!(git_dir, candidate.join(".git"));
}

fn normalized_lf(bytes: &[u8]) -> Vec<u8> {
    let mut normalized = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index..].starts_with(b"\r\n") {
            normalized.push(b'\n');
            index += 2;
        } else {
            normalized.push(bytes[index]);
            index += 1;
        }
    }
    normalized
}

#[cfg(not(windows))]
fn valid_config(gh: &[u8], git: &[u8]) -> serde_json::Value {
    let master = fs::read(std::env::current_exe().unwrap()).unwrap();
    json!({
        "schema_version": 1,
        "enabled": true,
        "repository": "malak333/Assemblywright",
        "base_branch": "main",
        "merge_strategy": "merge",
        "post_merge_gate": "release-local",
        "required_checks": [
            {"id":"release-local","workflow":"Assemblywright Release Local Gate","context":"Release local gate","app_id":15368,"workflow_id":282605278,"workflow_path":".github/workflows/release-local.yml","workflow_sha256":"51e809a94f59193e213bdff6e49f3a86e612643f094e366055f42f8745026fd7"},
            {"id":"protocol-windows","workflow":"Assemblywright Windows Distributed Gate","context":"Protocol, master, identity, mTLS, and SCM","app_id":15368,"workflow_id":314849303,"workflow_path":".github/workflows/windows-protocol.yml","workflow_sha256":"5c2ad627ef130468ec217b55050ba985499010182b2877d4404435532a8067db"}
        ],
        "gh_executable_sha256": hex(&Sha256::digest(gh)),
        "git_executable_sha256": hex(&Sha256::digest(git)),
        "master_executable_sha256": hex(&Sha256::digest(master))
    })
}

#[cfg(not(windows))]
fn write_config(root: &Path, value: &serde_json::Value) {
    fs::write(
        root.join("publication.json"),
        serde_json::to_vec(value).unwrap(),
    )
    .unwrap();
    make_private(&root.join("publication.json"), false);
}

fn protection_value() -> serde_json::Value {
    json!({
        "required_status_checks": {
            "strict": true,
            "checks": [
                {"context":"Release local gate","app_id":15368},
                {"context":"Protocol, master, identity, mTLS, and SCM","app_id":15368}
            ]
        },
        "required_pull_request_reviews": {
            "dismiss_stale_reviews":false,
            "require_code_owner_reviews":false,
            "require_last_push_approval":false,
            "required_approving_review_count":0,
            "bypass_pull_request_allowances": {"users":[],"teams":[],"apps":[]}
        },
        "required_conversation_resolution": {"enabled":true},
        "enforce_admins": {"enabled":true},
        "allow_force_pushes": {"enabled":false},
        "allow_deletions": {"enabled":false}
    })
}

fn valid_receipt() -> GithubPublicationLiveProofReceipt {
    GithubPublicationLiveProofReceipt {
        schema_version: 1,
        status: "github_publication_live_proof_passed".to_string(),
        repository: "malak333/Assemblywright".to_string(),
        base_branch: "main".to_string(),
        source_head: "1111111111111111111111111111111111111111".to_string(),
        publication_commit: "2222222222222222222222222222222222222222".to_string(),
        resulting_main_commit: "3333333333333333333333333333333333333333".to_string(),
        pull_request_number: 1,
        pull_request_url_sha256: "44".repeat(32),
        branch_name_sha256: "55".repeat(32),
        required_checks_sha256: "66".repeat(32),
        post_merge_checks_sha256: "77".repeat(32),
        master_executable_sha256: "88".repeat(32),
        observed_at_ms: 1,
    }
}

fn workflow_runs(commit: &str) -> serde_json::Value {
    json!([
        {"id":100,"workflow_id":282605278,"path":".github/workflows/release-local.yml","head_sha":commit,"repository":{"full_name":"malak333/Assemblywright"},"event":"pull_request","status":"completed","conclusion":"success"},
        {"id":101,"workflow_id":314849303,"path":".github/workflows/windows-protocol.yml","head_sha":commit,"repository":{"full_name":"malak333/Assemblywright"},"event":"pull_request","status":"completed","conclusion":"success"}
    ])
}

#[cfg(not(windows))]
fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").unwrap();
    }
    encoded
}

#[cfg(all(not(windows), unix))]
fn make_private(path: &Path, directory: bool) {
    use std::os::unix::fs::PermissionsExt;
    let mode = if directory { 0o700 } else { 0o600 };
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

#[cfg(all(not(windows), not(unix)))]
fn make_private(_path: &Path, _directory: bool) {}

#[cfg(all(not(windows), unix))]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

#[cfg(all(not(windows), not(unix)))]
fn make_executable(_path: &Path) {}
