#[test]
fn production_execution_host_provisioning_is_fail_closed_and_path_free() {
    let script = include_str!("../../../scripts/windows-execution-host-provision.ps1");

    for required in [
        "ValidateSet('DryRun', 'Check', 'Apply', 'SelfTest')",
        "Apply requires -ConfirmStoppedServiceCeremony.",
        "Owner-account service deployment is not the production execution-host substrate.",
        "Required execution-host service substrate is not installed.",
        "Assert-AllServicesStopped",
        "sidtype', $masterService, 'unrestricted'",
        "sidtype', $brokerService, 'unrestricted'",
        "sidtype', $executorService, 'restricted'",
        "NT AUTHORITY\\LocalService",
        "(D;;GA;;;$FeatureSid)",
        "SetAccessRuleProtection($true, $false)",
        "NT AUTHORITY\\SYSTEM",
        "fsutil.exe hardlink list",
        "control_plane_reserved_commit_bytes",
        "control_plane_reserved_process_slots",
        "control_plane_disk_reserve_bytes",
        "The control-plane disk reserve was sparse or compressed.",
        "Get-AuthenticodeSignature",
        "execution-host-release.json",
        "The service image was not the exact fixed production executable.",
        "The service command was not the exact canonical role-specific argv.",
        "--data-dir \"{1}\" service-run --service-name AssemblywrightMaster --bind 127.0.0.1:7791 --service-identity LocalSystem",
        "--service-host --service-name AssemblywrightBroker",
        "--service-host --service-name AssemblywrightExecutor",
        "The service type, noninteractive mode, start mode, or error control drifted.",
        "The service contained an unexpected trigger.",
        "The service contained unexpected dependency, recovery, or launch persistence.",
        "Invoke-StoppedMutationCluster",
        "The execution-host registry gate or policy binding drifted.",
        "A protected execution-host ACL contained an unexpected or broad entry.",
        "A service-host configuration was not read-only.",
        "Write, Delete, ChangePermissions, TakeOwnership",
        "[Security.AccessControl.FileSystemRights]::Synchronize",
        "A service definition DACL was not the exact protected contract.",
        "[Security.AccessControl.RawSecurityDescriptor]::new($Sddl)",
        "[UInt32]$serviceAllAccess = 0x000F01FF",
        "[UInt32]$serviceObserve = 0x0002018D",
        "This is deliberately the first mutation.",
        "FileMode]::CreateNew",
        "The real policy validator did not reject the hostile hardlink unchanged.",
        "The real reserve validator did not reject the hostile symlink unchanged.",
        "The real registry validator did not reject effects-enabled drift unchanged.",
        "The service DACL validator accepted an allow-before-deny descriptor.",
        "reordered_service_dacl_rejected = $true",
        "The real service validator accepted hostile argv drift.",
        "The real service persistence validator accepted hostile drift.",
        "executor_readonly_acl_contract_passed = $true",
        "non_inheritable_acl_drift_rejected = $true",
        "inheritable_executor_read_acl_drift_rejected = $true",
        "executor_inherited_sibling_read_denied = $true",
        "The Executor ancestor grant leaked read access to an inherited sibling.",
        "$rule.IsInherited",
        "$rule.InheritanceFlags -ne $expectedInheritance",
        "$rule.PropagationFlags -ne $expectedPropagation",
        "effect_activation_requires_exact_policy_attestation = $true",
        "CpuPriorityClass",
        "production_effects_enabled = $false",
    ] {
        assert!(
            script.contains(required),
            "missing host-security contract: {required}"
        );
    }

    for forbidden in [
        "Start-Service",
        "production_effects_enabled = $true",
        "ConvertTo-SecureString",
        "Password",
        "cmd.exe /c",
        "powershell.exe -Command",
    ] {
        assert!(
            !script.contains(forbidden),
            "forbidden host-security behavior: {forbidden}"
        );
    }
}

#[test]
fn dry_run_and_check_reject_apply_only_inputs() {
    let script = include_str!("../../../scripts/windows-execution-host-provision.ps1");
    assert!(script.contains("DryRun accepts no ceremony confirmation."));
    assert!(script.contains("Check accepts no ceremony confirmation."));
    assert!(script.contains(
        "Apply cannot create service hosts; exact Broker and Executor services must already exist."
    ));
    assert!(script.contains("A protected execution-host path component is a reparse point."));
    assert!(script.contains("A protected execution-host file was not ordinary and single-link."));
}

#[test]
fn native_hostile_e2e_is_disposable_bounded_and_does_not_target_production() {
    let script = include_str!("../../../scripts/windows-execution-host-security-e2e.ps1");
    for required in [
        "requires elevated Windows SCM access",
        "AssemblywrightHostE2EM",
        "AssemblywrightHostE2EB",
        "AssemblywrightHostE2EF",
        "AssemblywrightBrokerE2E",
        "AssemblywrightExecutorE2E",
        "sidtype', $service.Name, $service.SidType",
        "The hostile feature SID was not denied by the native filesystem DACL.",
        "The hostile feature SID could alter a protected service definition.",
        "Assert-FeatureServiceDeny $sddl $featureSid",
        "The hostile restricted-service payload did not prove execution.",
        "sc.exe start $featureName",
        "started;read;attempted",
        "The hostile feature token altered a protected file.",
        "The hostile feature token altered the protected disk reserve.",
        "The hostile feature token altered a protected service definition.",
        "The restricted feature token mutated its read-only configuration grant.",
        "executor_configuration_read_allowed_mutation_denied = $true",
        "executor_inherited_sibling_read_denied = $true",
        ";sibling-read",
        "windows-execution-host-provision.ps1",
        "-Mode DryRun",
        "-Mode Check",
        "Parser, undefined-variable, or unrelated policy failures",
        "Owner-account service deployment is not the production execution-host substrate.",
        "Required execution-host service substrate is not installed.",
        "-Mode SelfTest",
        "production_provisioner_self_test_passed = $true",
        "$selfTest.non_inheritable_acl_drift_rejected -ne $true",
        "$selfTest.reordered_service_dacl_rejected -ne $true",
        "$selfTest.inheritable_executor_read_acl_drift_rejected -ne $true",
        "$selfTest.executor_inherited_sibling_read_denied -ne $true",
        "cargo build --locked -p assemblywright-broker -p assemblywright-executor --bins",
        "real_broker_service_host_started_running_and_stopped = $true",
        "real_executor_service_host_started_running_and_stopped = $true",
        "real_service_host_digest_and_argv_rejected = $true",
        "real_service_host_semantic_config_rejected = $true",
        "accepted a digest-matched invalid runtime schema",
        "The production loaders require a single-link, read-only configuration",
        "Restricted service tokens must satisfy both the ordinary LocalService ACL",
        "The hostile hardlink prestate was not detected.",
        "The hostile symlink prestate was not detected.",
        "effects_enabled_drift_detected = $true",
        "protected_services_queryable_under_bounded_pressure = $true",
        "production_services_untouched = $true",
        "sc.exe delete $serviceCreated[$index]",
    ] {
        assert!(
            script.contains(required),
            "missing hostile E2E contract: {required}"
        );
    }
    assert!(!script.contains("sc.exe delete AssemblywrightMaster"));
    assert!(!script.contains("Stop-Service -Name AssemblywrightMaster"));
}

#[test]
fn windows_service_hosts_validate_semantic_bootstrap_before_running() {
    let broker = include_str!("../../assemblywright-broker/src/windows_service_host.rs");
    let executor = include_str!("../../assemblywright-executor/src/windows_service_host.rs");
    let executor_runtime = include_str!("../../assemblywright-executor/src/runtime.rs");
    let executor_fixture = include_str!(
        "../../assemblywright-executor/examples/windows_executor_service_config_fixture.rs"
    );
    assert!(broker.contains(".and_then(BrokerRuntime::new)"));
    assert!(executor.contains(".and_then(validate_service_bootstrap)"));
    assert!(!executor_runtime.contains("pub receipt_signing_seed"));
    assert!(!executor_fixture.contains("receipt_signing_seed"));
    assert!(executor_fixture.contains(r#"C:\ProgramData\Assemblywright\authority\master.sqlite3"#));
    assert!(executor_runtime.contains("return Err(RuntimeError::InvalidConfig);"));
    for source in [broker, executor] {
        let validation = source.find(".and_then(").expect("semantic validation");
        let running = source
            .find("ServiceState::Running")
            .expect("running status");
        assert!(
            validation < running,
            "service reported running before validation"
        );
    }
}

#[cfg(windows)]
#[test]
fn powershell_parses_and_executes_the_real_dry_run() {
    use std::path::PathBuf;
    use std::process::Command;

    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("scripts/windows-execution-host-provision.ps1");
    let parse = Command::new("powershell.exe")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$t=$null;$e=$null;[Management.Automation.Language.Parser]::ParseFile($args[0],[ref]$t,[ref]$e)|Out-Null;if($e.Count){$e|% Message;exit 1}",
        ])
        .arg(&script)
        .output()
        .expect("run Windows PowerShell parser");
    assert!(
        parse.status.success(),
        "{}",
        String::from_utf8_lossy(&parse.stderr)
    );

    let dry_run = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-File"])
        .arg(&script)
        .args(["-Mode", "DryRun"])
        .output()
        .expect("run production DryRun");
    assert!(
        dry_run.status.success(),
        "{}",
        String::from_utf8_lossy(&dry_run.stderr)
    );
    let receipt: serde_json::Value =
        serde_json::from_slice(&dry_run.stdout).expect("decode path-free DryRun receipt");
    assert_eq!(receipt["status"], "execution_host_dry_run_passed");
    assert_eq!(receipt["production_effects_enabled"], false);
}
