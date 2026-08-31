#[test]
fn disposable_windows_planning_service_e2e_is_bounded_and_self_cleaning() {
    let script = include_str!("../../../scripts/windows-planning-service-e2e.ps1");
    let fixture = include_str!("fixtures/windows_planning_codex_fixture.rs");

    for required in [
        "^AssemblywrightPlanningE2E_[A-Za-z0-9]{1,32}$",
        "The production service is never an E2E target.",
        "-identity local-system",
        "provision-planning-runtime.ps1",
        "/v1/assembly-line/project-brainstorms",
        "windows_planning_service_e2e_passed",
        "codex_stderr_bytes_drained = 32768",
        "production_service_untouched = $true",
        "service uninstall",
        "Remove-Item -LiteralPath $runtime -Recurse -Force",
        "Remove-Item -LiteralPath $scratch -Recurse -Force",
    ] {
        assert!(
            script.contains(required),
            "missing E2E contract: {required}"
        );
    }
    assert!(!script.contains("AssemblywrightMaster' --"));
    assert!(!script.contains("sc.exe delete AssemblywrightMaster"));
    assert!(fixture.contains("for _ in 0..4"));
    assert!(fixture.contains("The Windows service launches the provider inside the AppContainer."));
    assert!(!fixture.contains("std::env::vars"));
}
