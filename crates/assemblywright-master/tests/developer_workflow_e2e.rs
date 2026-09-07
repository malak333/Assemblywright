//! Runs the same disposable HTTP/file/process boundary on both required CI hosts.
#[test]
fn supervised_developer_workflow_runs_native_processes_and_recovers_checkpoints() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let python = if cfg!(windows) { "python" } else { "python3" };
    let developer_binary = std::path::Path::new(env!("CARGO_BIN_EXE_assemblywright-developer"));
    let review_fixture = developer_binary
        .parent()
        .unwrap()
        .join("examples")
        .join(format!(
            "developer_review_fixture{}",
            std::env::consts::EXE_SUFFIX
        ));
    if cfg!(windows) {
        assert!(
            review_fixture.is_file(),
            "Windows CI must prebuild the native reviewer fixture at {}",
            review_fixture.display()
        );
    }
    for script in [
        "developer-runner-e2e.py",
        "developer-runner-repair-e2e.py",
        "developer-runner-review-e2e.py",
        "developer-runner-planning-e2e.py",
        "developer-runner-model-target-e2e.py",
        "developer-runner-chat-e2e.py",
        "developer-runner-escalation-e2e.py",
    ] {
        let mut command = std::process::Command::new(python);
        command
            .arg(root.join("scripts").join(script))
            .args(["--binary", env!("CARGO_BIN_EXE_assemblywright-developer")]);
        if cfg!(windows) {
            command.env("ASSEMBLYWRIGHT_DEVELOPER_REVIEW_FIXTURE", &review_fixture);
        }
        let output = command
            .output()
            .expect("Python is required by the native developer E2E gate");
        assert!(
            output.status.success(),
            "native developer E2E {script} failed: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        println!("{script}: {}", String::from_utf8_lossy(&output.stdout));
    }
}
