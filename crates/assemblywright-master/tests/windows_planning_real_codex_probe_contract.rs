#[test]
fn real_codex_probe_is_owner_confirmed_stopped_service_only_and_self_recovering() {
    let main = include_str!("../src/main.rs");
    let runtime = include_str!("../src/planning_runtime.rs");
    let containment = include_str!("../src/planning_runtime/windows_containment.rs");
    let script = include_str!("../../../scripts/windows-planning-real-codex-probe.ps1");

    for required in [
        "PlanningProviderNativeProbe",
        "native planning-provider containment probe",
        "NativeProbeAuthority::open",
        "try_lock_exclusive()",
        "ServiceConfig",
        ".with_authority(continuous_authority)",
        "service stop --service-name $ServiceName",
        "planning-provider-native-probe --service-name $ServiceName --confirm",
        "service start --service-name $ServiceName",
        "Get-ExactServiceHealthEndpoint",
        "health --endpoint $healthEndpoint",
        "native_probe_post_command_authority_current",
    ] {
        assert!(
            main.contains(required)
                || runtime.contains(required)
                || containment.contains(required)
                || script.contains(required),
            "missing native probe contract: {required}"
        );
    }
    assert!(
        script.find("service stop --service-name").unwrap()
            < script.find("planning-provider-native-probe").unwrap()
    );
    assert!(
        script.find("planning-provider-native-probe").unwrap()
            < script.find("service start --service-name").unwrap()
    );
    assert!(
        script
            .find("$healthEndpoint = Get-ExactServiceHealthEndpoint")
            .unwrap()
            < script.find("service stop --service-name").unwrap()
    );
    assert!(
        script.find("service start --service-name").unwrap()
            < script.find("health --endpoint $healthEndpoint").unwrap()
    );
    assert!(!script.contains("if ($null -ne $receipt) {\n            & $master"));
    assert!(script.contains("[Net.IPAddress]::TryParse"));
    assert!(!script.contains("[Net.IPEndPoint]::TryParse"));
}

#[test]
fn real_codex_probe_reuses_containment_and_never_returns_process_text_or_authority() {
    let runtime = include_str!("../src/planning_runtime.rs");
    let containment = include_str!("../src/planning_runtime/windows_containment.rs");

    for required in [
        "FILE_BACKED_CODEX_AUTH_CONFIG",
        "cli_auth_credentials_store=\\\"file\\\"",
        "native_provider_probe_exec_args",
        "require_service_stopped(&service).map_err(|_| 3)?",
        "revalidate_service()",
        "planning_effect_pause_snapshot",
        "control.poll()",
        "service_account_sid",
        "current_token_sid",
        "provisioning_owner_sid",
        "receipt_domain",
        "strict_decode::<BrainstormingSpecificationDocument>",
        "provider_profile_name",
        "provider_profile_sid",
        "codex_executable_sha256",
        "probe_contract_sha256",
        "PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES",
        "PrivateDesktop::create(profile)?",
        "AssignProcessToJobObject",
        "terminate_job(&job",
        "restricted_token(profile.provisioning_owner_sid())",
        "SetTokenInformation",
        "TokenOwner",
        "TOKEN_ADJUST_DEFAULT",
        "token_information_sid_matches(token.raw(), TokenOwner, selected_owner)",
        "TokenDefaultOwnerPolicy::LocalSystemOwnerApplied",
        "matches!(scope, AclScope::ProfileWriteChild(_, true))",
    ] {
        assert!(
            runtime.contains(required) || containment.contains(required),
            "missing exact containment or receipt contract: {required}"
        );
    }
    let receipt_start = runtime
        .find("pub struct PlanningProviderNativeProbeReceipt")
        .unwrap();
    let receipt_end = runtime[receipt_start..].find('}').unwrap() + receipt_start;
    let receipt = &runtime[receipt_start..receipt_end];
    for forbidden in [
        "stdout",
        "stderr",
        "token_value",
        "credential",
        "prompt",
        "path",
        "idea",
        "specification",
    ] {
        assert!(
            !receipt.contains(forbidden),
            "receipt exposes forbidden field category: {forbidden}"
        );
    }
    assert!(!runtime.contains("repo create"));
}

#[test]
fn native_probe_stderr_diagnostics_are_bounded_closed_and_never_expose_raw_content() {
    let runtime = include_str!("../src/planning_runtime.rs");
    let containment = include_str!("../src/planning_runtime/windows_containment.rs");

    for required in [
        "NATIVE_PROVIDER_PROBE_LOGIN_STDERR_MAX_BYTES: usize = 2 * 1024",
        "NATIVE_PROVIDER_PROBE_EXEC_STDERR_MAX_BYTES: usize = 4 * 1024",
        "enum NativeProbeLoginDiagnostic",
        "enum NativeProbeExecDiagnostic",
        "RequirementsFileReadFailed => 924",
        "ConfigFileReadFailed => 925",
        "AuthenticationRequirementsNoUsableLoginMethod => 926",
        "AccessDeniedIo => 927",
        "SystemCodexPathAccessDenied => 928",
        "ProviderCodexHomeAccessDenied => 929",
        "ProviderLocalAppDataAccessDenied => 930",
        "ProviderTempAccessDenied => 931",
        "ProviderRootOrCurrentDirectoryAccessDenied => 932",
        "CodexHomeCanonicalizationFailed => 933",
        "CodexHomeReadFailed => 934",
        "NotLoggedIn => 920",
        "ConfigurationLoadFailed => 921",
        "LoginStatusFailed => 922",
        "Other => 923",
        "Self::ConfigurationLoadFailed => 940",
        "Self::NotLoggedIn => 941",
        "Self::AuthenticationFailed => 942",
        "Self::NetworkUnavailable => 943",
        "Self::CliArgumentRejected => 944",
        "Self::OutputSchemaRejected => 945",
        "Self::ModelUnavailable => 946",
        "Self::SystemCodexPathAccessDenied => 947",
        "Self::ProviderCodexHomeAccessDenied => 948",
        "Self::ProviderLocalAppDataAccessDenied => 949",
        "Self::ProviderTempAccessDenied => 950",
        "Self::ProviderRootOrCurrentDirectoryAccessDenied => 951",
        "Self::CodexHomeCanonicalizationFailed => 954",
        "Self::ExplicitCwdCanonicalizationFailed => 955",
        "Self::AccessDeniedIo => 952",
        "Self::Other => 953",
        "native_probe_login_diagnostic(&login_stderr)",
        "native_probe_exec_diagnostic(&exec_stderr)",
        "ascii_contains_ignore_case(stderr, b\"failed to read requirements file\")",
        "ascii_contains_ignore_case(stderr, b\"failed to read config file\")",
        "authentication requirements do not permit any usable login method",
        "ascii_contains_ignore_case(stderr, b\"access is denied\")",
        "ascii_contains_ignore_case(stderr, b\"permission denied\")",
        "ascii_contains_ignore_case(stderr, b\"os error 5\")",
        "b\"\\\\ProgramData\\\\OpenAI\\\\Codex\\\\\"",
        "b\"\\\\planning-runtime\\\\provider\\\\codex-home\"",
        "b\"\\\\planning-runtime\\\\provider\\\\local-app-data\"",
        "b\"\\\\planning-runtime\\\\provider\\\\temp\"",
        "b\"current working directory\"",
        "let access_denied = native_probe_stderr_contains_access_denied(stderr)",
        "bytes.zeroize()",
        "fn bounded_diagnostic_reader(",
        "buffer[..count].zeroize()",
        "retained.extend_from_slice(&buffer[..count.min(remaining)])",
    ] {
        assert!(
            runtime.contains(required) || containment.contains(required),
            "missing bounded native-probe diagnostic contract: {required}"
        );
    }

    let login_start = runtime.find("let login_args =").unwrap();
    let login_end = runtime[login_start..].find("let exec_args =").unwrap() + login_start;
    let login = &runtime[login_start..login_end];
    assert!(login.contains("stderr: CommandStderrMode::CaptureBounded"));
    assert!(login.contains("Some(login_diagnostic)"));

    let exec_start = login_end;
    let exec_end = runtime[exec_start..]
        .find("pub fn validated_status")
        .unwrap()
        + exec_start;
    let exec = &runtime[exec_start..exec_end];
    assert!(exec.contains("stderr: CommandStderrMode::CaptureBounded"));
    assert!(exec.contains("max_bytes: NATIVE_PROVIDER_PROBE_EXEC_STDERR_MAX_BYTES"));
    assert!(exec.contains("receipt.exec_diagnostic_code = Some(diagnostic.code())"));

    let receipt_start = runtime
        .find("pub struct PlanningProviderNativeProbeReceipt")
        .unwrap();
    let receipt_end = runtime[receipt_start..].find('}').unwrap() + receipt_start;
    let receipt = &runtime[receipt_start..receipt_end];
    assert!(receipt.contains("login_diagnostic_code: Option<u32>"));
    assert!(receipt.contains("exec_diagnostic_code: Option<u32>"));
    assert!(!receipt.contains("stderr"));
    assert!(!receipt.contains("diagnostic_text"));
}

#[test]
fn real_codex_probe_environment_and_tool_disable_contracts_are_exact() {
    let runtime = include_str!("../src/planning_runtime.rs");
    let containment = include_str!("../src/planning_runtime/windows_containment.rs");

    for variable in ["CODEX_HOME", "LOCALAPPDATA", "SystemRoot", "TEMP", "TMP"] {
        assert!(runtime.contains(variable) || containment.contains(variable));
    }
    let probe_environment_start = runtime.find("let codex_home_environment =").unwrap();
    let probe_environment_end = runtime[probe_environment_start..]
        .find("let probe_contract_sha256")
        .unwrap()
        + probe_environment_start;
    let probe_environment = &runtime[probe_environment_start..probe_environment_end];
    assert_eq!(
        probe_environment
            .matches("windows_containment::codex_windows_environment_path")
            .count(),
        3
    );
    assert!(!probe_environment.contains("self.brainstorming.codex_home.as_os_str()"));
    assert!(!probe_environment.contains("self.brainstorming.local_app_data.as_os_str()"));
    assert!(!probe_environment.contains("self.brainstorming.temporary.as_os_str()"));
    assert!(containment.contains(r#"Path::new(r"\\?\UNC\server\share\state")"#));
    for disabled in [
        "cli_auth_credentials_store=\\\"file\\\"",
        "project_root_markers=[]",
        "features.shell_tool=false",
        "features.plugins=false",
        "features.multi_agent=false",
        "features.apps=false",
        "features.browser_use=false",
        "features.computer_use=false",
        r#"web_search=\"disabled\""#,
        "tools.web_search=false",
    ] {
        assert!(
            runtime.contains(disabled),
            "missing disabled surface: {disabled}"
        );
    }
    assert!(containment.contains("temporary.1 == temporary_alias.1"));
    assert!(containment.contains("_ => return Err(CommandError::Failed)"));
}

#[test]
fn every_windows_planning_launch_derives_only_the_child_visible_current_directory() {
    let runtime = include_str!("../src/planning_runtime.rs");
    let containment = include_str!("../src/planning_runtime/windows_containment.rs");
    let production = containment
        .split("\n#[cfg(test)]\nmod tests")
        .next()
        .unwrap();
    assert_eq!(production.matches("CreateProcessAsUserW(").count(), 1);

    let run_start = production.find("pub(super) fn run_command(").unwrap();
    let run_end = production[run_start..]
        .find("\nfn complete_signaled_process(")
        .unwrap()
        + run_start;
    let run = &production[run_start..run_end];
    let derive = run
        .find("child_visible_current_directory(invocation.current_dir)")
        .unwrap();
    let encode = run
        .find("wide(child_visible_current_dir.as_os_str())")
        .unwrap();
    let create = run.find("CreateProcessAsUserW(").unwrap();
    assert!(derive < encode && encode < create);
    assert!(run.contains("current_dir_w.as_ptr(),"));
    assert!(!run.contains("wide(invocation.current_dir.as_os_str())"));

    let bridge_start = runtime.find("fn run_command(\n").unwrap();
    let bridge_end = runtime[bridge_start..]
        .find("\n#[cfg(not(windows))]\nfn run_command_portable(")
        .unwrap()
        + bridge_start;
    assert!(runtime[bridge_start..bridge_end].contains("windows_containment::run_command("));

    for unchanged in [
        r#"Path::new(r"\\?\UNC\server\share\state")"#,
        r#"Path::new(r"\\?\Volume{1234}\state")"#,
    ] {
        assert!(containment.contains(unchanged));
    }
}

#[test]
fn real_codex_probe_disables_cwd_ancestor_project_discovery_for_login_and_exec() {
    let runtime = include_str!("../src/planning_runtime.rs");

    let login_start = runtime
        .find("fn native_provider_probe_login_args()")
        .unwrap();
    let login_end = runtime[login_start..]
        .find("fn native_provider_probe_exec_args(")
        .unwrap()
        + login_start;
    let login = &runtime[login_start..login_end];
    assert!(login.contains("\"--config\",\n        \"project_root_markers=[]\""));
    assert!(login.find("project_root_markers=[]").unwrap() < login.find("\"login\"").unwrap());

    let exec_start = login_end;
    let exec_end = runtime[exec_start..]
        .find("struct CommandInvocation")
        .unwrap()
        + exec_start;
    let exec = &runtime[exec_start..exec_end];
    assert!(exec.contains("\"--config\",\n        \"project_root_markers=[]\""));
    assert_eq!(exec.matches("project_root_markers=[]").count(), 1);
    assert!(!exec.contains("\"--cd\""));
    assert!(exec.contains("args == native_provider_probe_exec_args(output_schema)"));
}

#[test]
fn real_codex_probe_open_failures_expose_only_fixed_stage_codes() {
    let runtime = include_str!("../src/planning_runtime.rs");
    let containment = include_str!("../src/planning_runtime/windows_containment.rs");

    assert!(
        runtime.contains(".map_err(|reason| PlanningRuntimeConfigError::Boundary(reason.code()))?")
    );
    for code in [
        "native_probe_service_configuration",
        "native_probe_executable_command_binding",
        "native_probe_account_identity",
        "native_probe_profile_revalidation",
        "native_probe_stopped_state",
        "native_probe_binding_digest",
    ] {
        assert_eq!(
            containment.matches(code).count(),
            2,
            "stage code must appear exactly once in production mapping and once in its native test: {code}"
        );
    }
}

#[test]
fn native_probe_deadline_is_extended_without_widening_product_planning_effects() {
    let runtime = include_str!("../src/planning_runtime.rs");

    assert!(runtime
        .contains("const NATIVE_PROVIDER_PROBE_DEADLINE: Duration = Duration::from_secs(300);"));
    assert!(runtime
        .contains("pub const PLANNING_EFFECT_DEADLINE: Duration = Duration::from_secs(120);"));
    let probe_start = runtime.find("pub fn run_provider_native_probe(").unwrap();
    let probe_end = runtime[probe_start..]
        .find("pub fn validated_status")
        .unwrap()
        + probe_start;
    let probe = &runtime[probe_start..probe_end];
    assert!(probe.contains("Instant::now() + NATIVE_PROVIDER_PROBE_DEADLINE"));
    assert!(!probe.contains("Instant::now() + PLANNING_EFFECT_DEADLINE"));
}

#[test]
fn in_flight_probe_checks_immutable_authority_without_racing_writable_provider_state() {
    let runtime = include_str!("../src/planning_runtime.rs");

    let probe_start = runtime.find("pub fn run_provider_native_probe(").unwrap();
    let closure_start = runtime[probe_start..]
        .find("let continuous_authority")
        .unwrap()
        + probe_start;
    let closure_end = runtime[closure_start..]
        .find("let control = PlanningEffectControl::new")
        .unwrap()
        + closure_start;
    let authority_poll = &runtime[closure_start..closure_end];
    assert!(authority_poll.contains("authority_check.revalidate_service()"));
    assert!(authority_poll.contains("planning_effect_pause_snapshot()"));
    assert!(!authority_poll.contains("validated_status()"));
    assert!(!authority_poll.contains("profile.revalidate()"));

    let probe_end = runtime[probe_start..]
        .find("pub fn validated_status")
        .unwrap()
        + probe_start;
    let probe = &runtime[probe_start..probe_end];
    assert_eq!(
        probe.matches("self.validated_status().is_none()").count(),
        2
    );
    assert_eq!(
        probe.matches("self.validated_status().is_some()").count(),
        2
    );
    assert_eq!(
        probe
            .matches("native_probe_post_command_authority_current(")
            .count(),
        2
    );
}
