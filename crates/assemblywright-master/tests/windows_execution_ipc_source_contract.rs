use std::fs;
use std::path::PathBuf;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn ipc_contract_is_local_authenticated_durable_and_inert() {
    let protocol = fs::read_to_string(repo().join("crates/assemblywright-protocol/src/lib.rs"))
        .expect("protocol");
    let state = fs::read_to_string(
        repo().join("crates/assemblywright-protocol/src/execution_ipc_state.rs"),
    )
    .expect("durable state");
    let pipe = fs::read_to_string(
        repo().join("crates/assemblywright-protocol/src/windows_execution_pipe.rs"),
    )
    .expect("pipe transport");
    let master = fs::read_to_string(repo().join("crates/assemblywright-master/src/main.rs"))
        .expect("master");
    for required in [
        "assemblywright.windows-execution-control.v1",
        "assemblywright.windows-execution-ack.v1",
        "MasterToBroker",
        "MasterToExecutor",
        "forwarded_executor_frame_sha256",
        "effects_applied != 0",
    ] {
        assert!(
            protocol.contains(required),
            "missing IPC contract: {required}"
        );
    }
    for required in [
        "previous_record_sha256",
        "RecoverPending",
        "Replay(WindowsExecutionAck)",
        "frame.request_sequence != self.state.next_sequence",
        "self.state.quarantined = true",
        "sync_data()",
    ] {
        assert!(
            state.contains(required),
            "missing durability contract: {required}"
        );
    }
    for required in [
        "PIPE_REJECT_REMOTE_CLIENTS",
        "FILE_FLAG_FIRST_PIPE_INSTANCE",
        "ImpersonateNamedPipeClient",
        "GetNamedPipeServerProcessId",
        "let request = read_frame(&mut pipe)?;",
        "SECURITY_SQOS_PRESENT",
        "SECURITY_IDENTIFICATION",
        "SecurityIdentification",
        "RevertToSelf() } == 0",
        "token_has_sid",
        "validate_service_sid_text",
        "server_service_sid",
        "SE_GROUP_ENABLED | SE_GROUP_OWNER",
        "O:{server_service_sid}D:P",
        "(A;;GA;;;{server_service_sid})",
        "S-1-5-80-",
        "expected_client_service_sid",
        "expected_server_service_sid",
    ] {
        assert!(pipe.contains(required), "missing pipe contract: {required}");
    }
    assert!(
        pipe.find("let request = read_frame(&mut pipe)?;").unwrap()
            < pipe
                .find("impersonated_client_has_sid(raw, expected_client_service_sid)")
                .unwrap()
    );
    assert!(master.contains("UnavailableAssemblyLineEffectDispatcher"));
    assert!(!protocol.contains("ShutdownAcceptedNoEffects"));
    assert!(state.contains("Zeroizing<[u8; 32]>") && state.contains("hold_parent_ancestry"));
}

#[test]
fn service_hosts_wire_only_inert_processors_and_out_of_band_secrets() {
    let broker =
        fs::read_to_string(repo().join("crates/assemblywright-broker/src/windows_service_host.rs"))
            .expect("broker service host");
    let executor = fs::read_to_string(
        repo().join("crates/assemblywright-executor/src/windows_service_host.rs"),
    )
    .expect("executor service host");
    let broker_runtime =
        fs::read_to_string(repo().join("crates/assemblywright-broker/src/runtime.rs"))
            .expect("broker runtime");
    let executor_runtime =
        fs::read_to_string(repo().join("crates/assemblywright-executor/src/runtime.rs"))
            .expect("executor runtime");
    assert!(broker.contains("InertBrokerIpc::open"));
    assert!(broker.contains("&*seed"));
    assert!(broker.contains("transact_executor"));
    assert!(executor.contains("InertExecutorIpc::open"));
    assert!(executor.contains("&*seed"));
    assert!(executor.contains("serve_broker_once"));
    assert!(broker_runtime.contains("pub ack_seed_path: PathBuf"));
    assert!(executor_runtime.contains("pub ack_seed_path: PathBuf"));
    assert!(broker_runtime.contains("pub broker_service_sid: String"));
    assert!(executor_runtime.contains("pub executor_service_sid: String"));
    assert!(!broker_runtime.contains("pub ack_signing_seed"));
    assert!(!executor_runtime.contains("pub ack_signing_seed"));
    assert!(!executor_runtime.contains("pub receipt_signing_seed"));
}

#[test]
fn native_e2e_covers_three_services_hostile_frames_restart_and_zero_effects() {
    let script = fs::read_to_string(repo().join("scripts/windows-execution-ipc-e2e.ps1"))
        .expect("native IPC E2E");
    for required in [
        "AssemblywrightMasterE2E",
        "AssemblywrightBrokerE2E",
        "AssemblywrightExecutorE2E",
        "'unsigned','tampered','gap','stale','stale_authority'",
        "New-Fixture 'wrong_sid'",
        "New-Fixture 'localservice_dacl_denied'",
        "$clientPipe = if ($LocalServiceClient) { $executorPipe }",
        "$clientServerSid = if ($LocalServiceClient) { $executorSid }",
        "*$runningMasterSid`:(OI)(CI)F",
        "--scenario replay",
        "restart_exact_ack_replay = $true",
        "server_self_sid_dacl_binding = $true",
        "unrelated_localservice_open_and_write_dac_denied = $true",
        "effects_applied = 0",
        "client_impersonation_level = 'identification_only'",
        "New-Fixture 'delayed_write'",
        "delayed_client_write_authenticated = $true",
        "Remove-FixtureService",
        "Fixture root remained after cleanup",
    ] {
        assert!(
            script.contains(required),
            "missing native E2E proof: {required}"
        );
    }
}

#[cfg(windows)]
#[test]
fn powershell_native_e2e_script_parses() {
    let script = repo().join("scripts/windows-execution-ipc-e2e.ps1");
    let escaped = script.to_string_lossy().replace('"', "`\"");
    let command =
        format!("[void][scriptblock]::Create((Get-Content -Raw -LiteralPath \"{escaped}\"))");
    let status = std::process::Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &command])
        .status()
        .expect("parse PowerShell");
    assert!(status.success());
}
