use assemblywright_master::validation_containment::{
    run_internal_validation_check, run_validation_command, validation_command_arguments,
    validation_command_execution, ValidationCancellation, ValidationCommandExecution,
    ValidationContainmentError, ValidationToolchainConfig, VerifiedValidationCopy,
};
#[cfg(windows)]
use assemblywright_master::validation_containment::{
    run_validation_fixture_with_cancellation, ValidationFixtureCommand,
};
use assemblywright_protocol::FeatureConveyorValidationCommandId as CommandId;
use git2::{Repository, Signature, Time};
use std::fs;
use std::sync::{atomic::AtomicBool, Arc};
use std::time::Duration;
use tempfile::tempdir;

fn candidate(path: &std::path::Path) -> (String, String) {
    let repository = Repository::init(path).unwrap();
    fs::write(path.join("README.md"), b"candidate\n").unwrap();
    let mut index = repository.index().unwrap();
    index.add_path(std::path::Path::new("README.md")).unwrap();
    let tree_id = index.write_tree().unwrap();
    index.write().unwrap();
    let tree = repository.find_tree(tree_id).unwrap();
    let signature = Signature::new("fixture", "fixture@example.invalid", &Time::new(1, 0)).unwrap();
    let commit = repository
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            "candidate",
            &tree,
            &[],
        )
        .unwrap();
    (commit.to_string(), tree_id.to_string())
}

fn validation_candidate(
    path: &std::path::Path,
    secret: bool,
) -> (String, String, String, Vec<String>) {
    let repository = Repository::init(path).unwrap();
    fs::write(path.join("README.md"), b"base\n").unwrap();
    let mut index = repository.index().unwrap();
    index.add_path(std::path::Path::new("README.md")).unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = repository.find_tree(tree_id).unwrap();
    let signature = Signature::new("fixture", "fixture@example.invalid", &Time::new(1, 0)).unwrap();
    let base = repository
        .commit(Some("HEAD"), &signature, &signature, "base", &tree, &[])
        .unwrap();
    drop(tree);
    fs::create_dir_all(path.join("docs/knowledge-base")).unwrap();
    let readme: &[u8] = if secret {
        &[103, 104, 112, 95, 101, 120, 97, 109, 112, 108, 101, 10]
    } else {
        b"candidate\n"
    };
    fs::write(path.join("README.md"), readme).unwrap();
    fs::write(path.join("DESIGN.md"), b"design\n").unwrap();
    fs::write(path.join("docs/safety-rules.md"), b"safety\n").unwrap();
    fs::write(path.join("docs/knowledge-base/facts.md"), b"facts\n").unwrap();
    let paths = vec![
        "DESIGN.md",
        "README.md",
        "docs/knowledge-base/facts.md",
        "docs/safety-rules.md",
    ]
    .into_iter()
    .map(str::to_string)
    .collect::<Vec<_>>();
    let mut index = repository.index().unwrap();
    for path in &paths {
        index.add_path(std::path::Path::new(path)).unwrap();
    }
    index.write().unwrap();
    let candidate_tree_id = index.write_tree().unwrap();
    let candidate_tree = repository.find_tree(candidate_tree_id).unwrap();
    let base_commit = repository.find_commit(base).unwrap();
    let commit = repository
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            "candidate",
            &candidate_tree,
            &[&base_commit],
        )
        .unwrap();
    (
        base.to_string(),
        commit.to_string(),
        candidate_tree_id.to_string(),
        paths,
    )
}

fn fixture_toolchain(root: &std::path::Path) -> ValidationToolchainConfig {
    let bin = root.join("toolchain").join("bin");
    fs::create_dir_all(&bin).unwrap();
    let executable = std::env::current_exe().unwrap();
    for name in [
        "cargo.exe",
        "cargo-llvm-cov.exe",
        "cargo-clippy.exe",
        "cargo-fmt.exe",
        "rustc.exe",
        "rustfmt.exe",
    ] {
        fs::copy(&executable, bin.join(name)).unwrap();
    }
    let cache = root.join("cache-seed");
    fs::create_dir(&cache).unwrap();
    ValidationToolchainConfig::resolve(&root.join("toolchain"), &cache).unwrap()
}

#[test]
fn closed_command_classification_never_treats_platform_or_internal_proof_as_a_process() {
    for command in [
        CommandId::Coverage,
        CommandId::FocusedUnitTests,
        CommandId::NativeE2e,
        CommandId::Formatting,
        CommandId::Lint,
        CommandId::Build,
        CommandId::RepositoryValidation,
    ] {
        assert_eq!(
            validation_command_execution(command),
            ValidationCommandExecution::ContainedProcess
        );
    }
    for command in [
        CommandId::RequirementsBinding,
        CommandId::Documentation,
        CommandId::KnowledgeBase,
        CommandId::Safety,
        CommandId::ChangedPaths,
        CommandId::SecretScan,
    ] {
        assert_eq!(
            validation_command_execution(command),
            ValidationCommandExecution::InternalDeterministicCheck
        );
    }
}

#[test]
fn coverage_command_emits_a_report_and_enforces_the_protocol_threshold() {
    let arguments = validation_command_arguments(CommandId::Coverage).unwrap();
    assert!(arguments.contains("--summary-only"));
    assert!(arguments.contains("--fail-under-lines 70"));
    assert!(!arguments.contains("--no-report"));
}

#[test]
fn fixed_process_commands_expose_no_caller_selected_arguments() {
    let expected = [
        (
            CommandId::FocusedUnitTests,
            "test --workspace --lib --bins --offline --locked --no-fail-fast",
        ),
        (
            CommandId::NativeE2e,
            "test --workspace --tests --all-features --offline --locked --no-fail-fast",
        ),
        (CommandId::Formatting, "fmt --all -- --check"),
        (
            CommandId::Lint,
            "clippy --workspace --all-targets --all-features --offline --locked -- -D warnings",
        ),
        (
            CommandId::Build,
            "build --workspace --all-targets --all-features --offline --locked",
        ),
        (
            CommandId::RepositoryValidation,
            "test --workspace --all-targets --all-features --offline --locked --no-fail-fast",
        ),
    ];
    for (command, arguments) in expected {
        assert_eq!(validation_command_arguments(command).unwrap(), arguments);
    }
    assert!(matches!(
        validation_command_arguments(CommandId::RequirementsBinding),
        Err(ValidationContainmentError::InternalCheckRequired)
    ));
}

#[test]
fn prepared_copy_must_match_a_clean_no_remote_exact_commit_and_tree() {
    let directory = tempdir().unwrap();
    let repository_path = directory.path().join("candidate-copy");
    let (commit, tree) = candidate(&repository_path);
    VerifiedValidationCopy::verify(&repository_path, &commit, &tree).unwrap();

    fs::write(repository_path.join("README.md"), b"drift\n").unwrap();
    assert!(matches!(
        VerifiedValidationCopy::verify(&repository_path, &commit, &tree),
        Err(ValidationContainmentError::CandidateDrift)
    ));
}

#[test]
fn cancellation_and_non_process_evidence_fail_before_launch() {
    let directory = tempdir().unwrap();
    let repository_path = directory.path().join("candidate-copy");
    let (commit, tree) = candidate(&repository_path);
    let candidate = VerifiedValidationCopy::verify(&repository_path, &commit, &tree).unwrap();
    let toolchain = fixture_toolchain(directory.path());
    let cancelled = ValidationCancellation::new(Arc::new(AtomicBool::new(true)));

    assert!(matches!(
        run_validation_command(
            CommandId::Formatting,
            &candidate,
            &toolchain,
            Duration::from_secs(1),
            &cancelled,
        ),
        Err(ValidationContainmentError::Cancelled)
    ));
    let active = ValidationCancellation::new(Arc::new(AtomicBool::new(false)));
    assert!(matches!(
        run_validation_command(
            CommandId::SecretScan,
            &candidate,
            &toolchain,
            Duration::from_secs(1),
            &active,
        ),
        Err(ValidationContainmentError::InternalCheckRequired)
    ));
}

#[test]
fn dependency_cache_seed_rejects_credentials_and_configuration() {
    let directory = tempdir().unwrap();
    let bin = directory.path().join("toolchain").join("bin");
    fs::create_dir_all(&bin).unwrap();
    let executable = std::env::current_exe().unwrap();
    for name in [
        "cargo.exe",
        "cargo-llvm-cov.exe",
        "cargo-clippy.exe",
        "cargo-fmt.exe",
        "rustc.exe",
        "rustfmt.exe",
    ] {
        fs::copy(&executable, bin.join(name)).unwrap();
    }
    let cache = directory.path().join("cache-seed");
    fs::create_dir(&cache).unwrap();
    fs::write(cache.join("credentials.toml"), b"token = 'secret'\n").unwrap();
    assert!(matches!(
        ValidationToolchainConfig::resolve(&directory.path().join("toolchain"), &cache),
        Err(ValidationContainmentError::PrivateDependencyCacheUnavailable)
    ));
}

#[test]
fn internal_checks_bind_exact_paths_docs_knowledge_safety_and_secret_scan() {
    let directory = tempdir().unwrap();
    let repository_path = directory.path().join("candidate-copy");
    let (base, commit, tree, paths) = validation_candidate(&repository_path, false);
    let candidate = VerifiedValidationCopy::verify(&repository_path, &commit, &tree).unwrap();
    for command in [
        CommandId::RequirementsBinding,
        CommandId::Documentation,
        CommandId::KnowledgeBase,
        CommandId::Safety,
        CommandId::ChangedPaths,
        CommandId::SecretScan,
    ] {
        let result =
            run_internal_validation_check(command, &candidate, &base, &paths, 4, [7; 32]).unwrap();
        assert!(result.passed, "{command:?}");
        assert_ne!(result.result_sha256, [0; 32]);
    }

    let secret_root = directory.path().join("secret-copy");
    let (base, commit, tree, paths) = validation_candidate(&secret_root, true);
    let candidate = VerifiedValidationCopy::verify(&secret_root, &commit, &tree).unwrap();
    assert!(
        !run_internal_validation_check(
            CommandId::SecretScan,
            &candidate,
            &base,
            &paths,
            4,
            [7; 32]
        )
        .unwrap()
        .passed
    );
}

#[test]
fn requirements_binding_rejects_empty_or_drifted_authority() {
    let directory = tempdir().unwrap();
    let repository_path = directory.path().join("candidate-copy");
    let (base, commit, tree, paths) = validation_candidate(&repository_path, false);
    let candidate = VerifiedValidationCopy::verify(&repository_path, &commit, &tree).unwrap();

    for (approved_paths, acceptance_count, design_sha256) in [
        (paths.clone(), 0, [7; 32]),
        (paths.clone(), 4, [0; 32]),
        (vec!["README.md".to_string()], 4, [7; 32]),
        (Vec::new(), 4, [7; 32]),
    ] {
        let result = run_internal_validation_check(
            CommandId::RequirementsBinding,
            &candidate,
            &base,
            &approved_paths,
            acceptance_count,
            design_sha256,
        )
        .unwrap();
        assert!(!result.passed);
        assert_ne!(result.result_sha256, [0; 32]);
    }
}

#[cfg(windows)]
#[test]
fn windows_runner_launches_only_the_staged_fixed_tool_under_containment() {
    let directory = tempdir().unwrap();
    let repository_path = directory.path().join("candidate-copy");
    let (commit, tree) = candidate(&repository_path);
    let candidate = VerifiedValidationCopy::verify(&repository_path, &commit, &tree).unwrap();
    let toolchain = fixture_toolchain(directory.path());
    let active = ValidationCancellation::new(Arc::new(AtomicBool::new(false)));

    let result = run_validation_command(
        CommandId::Formatting,
        &candidate,
        &toolchain,
        Duration::from_secs(10),
        &active,
    )
    .unwrap();
    assert_eq!(result.command_id, CommandId::Formatting);
    assert!(!result.timed_out);
    assert_ne!(result.exit_code, 0, "the libtest fixture is not cargo-fmt");
    assert!(result.stdout_len <= 64 * 1024);
    assert!(result.stderr_len <= 64 * 1024);
    candidate.revalidate().unwrap();
}

#[cfg(windows)]
#[test]
fn active_cancellation_terminates_and_reaps_the_contained_job_tree() {
    let root = tempdir().unwrap();
    let signal = Arc::new(AtomicBool::new(false));
    let cancellation = ValidationCancellation::new(signal.clone());
    let setter = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        signal.store(true, std::sync::atomic::Ordering::Release);
    });
    let result = run_validation_fixture_with_cancellation(
        ValidationFixtureCommand::TimeoutChildTree,
        root.path(),
        Duration::from_secs(10),
        &cancellation,
    );
    setter.join().unwrap();
    assert!(matches!(result, Err(ValidationContainmentError::Cancelled)));
    std::thread::sleep(Duration::from_secs(3));
    assert!(!root.path().join("descendant-survived.txt").exists());
}

#[cfg(windows)]
#[test]
#[ignore = "contained child fixture"]
fn fixture_timeout_spawns_child_tree_that_must_be_killed() {
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "fixture_timeout_descendant_never_finishes",
            "--ignored",
            "--nocapture",
        ])
        .spawn()
        .unwrap();
    fs::write("descendant-pid.txt", child.id().to_string()).unwrap();
    std::thread::sleep(Duration::from_secs(60));
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(windows)]
#[test]
#[ignore = "contained descendant fixture"]
fn fixture_timeout_descendant_never_finishes() {
    std::thread::sleep(Duration::from_secs(2));
    fs::write("descendant-survived.txt", b"late").unwrap();
    std::thread::sleep(Duration::from_secs(60));
}
