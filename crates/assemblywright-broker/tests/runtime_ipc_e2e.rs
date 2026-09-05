#![cfg(unix)]

use assemblywright_broker::runtime::{
    BrokerIpcBootstrap, BrokerRuntime, BrokerRuntimeConfig, BrokerRuntimeIntent,
    BrokerRuntimeRequest, BrokerRuntimeResponse, BrokerRuntimeResult, RUNTIME_SCHEMA_VERSION,
};
use assemblywright_protocol::{ExecutionHostPlatform, ProtectedControlPlanePathManifest};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use tempfile::tempdir;
use uuid::Uuid;

fn manifest(path: &str) -> ProtectedControlPlanePathManifest {
    ProtectedControlPlanePathManifest {
        schema_version: assemblywright_protocol::FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        platform: ExecutionHostPlatform::Macos,
        master_binary: path.into(),
        broker_binary: path.into(),
        service_configuration: path.into(),
        authority_database: path.into(),
        database_backups: path.into(),
        audit: path.into(),
        owner_tokens_and_signing: path.into(),
        trust_and_update_roots: path.into(),
        ipc_and_enforcement_state: path.into(),
        release_evidence: path.into(),
        resource_reservations: path.into(),
    }
}

#[test]
fn ipc_bootstrap_rejects_lexical_escape_from_protected_root() {
    let temp = tempdir().unwrap();
    let key = SigningKey::from_bytes(&[20; 32]);
    let mut config = fixture(temp.path(), &key);
    let protected = std::path::PathBuf::from(&config.protected_manifest.ipc_and_enforcement_state);
    config.ipc = Some(BrokerIpcBootstrap {
        pipe_name: r"\\.\pipe\Assemblywright.MasterBroker.Unit".into(),
        broker_service_sid: "S-1-5-80-11-12-13-14-15".into(),
        expected_master_service_sid: "S-1-5-80-1-2-3-4-5".into(),
        executor_pipe_name: r"\\.\pipe\Assemblywright.BrokerExecutor.Unit".into(),
        expected_executor_service_sid: "S-1-5-80-6-7-8-9-10".into(),
        durable_state_path: protected.join("..").join("outside.journal"),
        ack_seed_path: protected.join("ack.seed"),
        ack_key_id: "broker-ack-v1".into(),
    });
    assert!(BrokerRuntime::new(config).is_err());
}

#[test]
fn ipc_bootstrap_requires_server_self_sid() {
    let temp = tempdir().unwrap();
    let key = SigningKey::from_bytes(&[20; 32]);
    let mut config = fixture(temp.path(), &key);
    let protected = std::path::PathBuf::from(&config.protected_manifest.ipc_and_enforcement_state);
    config.ipc = Some(BrokerIpcBootstrap {
        pipe_name: r"\\.\pipe\Assemblywright.MasterBroker.Unit".into(),
        broker_service_sid: String::new(),
        expected_master_service_sid: "S-1-5-80-1-2-3-4-5".into(),
        executor_pipe_name: r"\\.\pipe\Assemblywright.BrokerExecutor.Unit".into(),
        expected_executor_service_sid: "S-1-5-80-6-7-8-9-10".into(),
        durable_state_path: protected.join("ipc.journal"),
        ack_seed_path: protected.join("ack.seed"),
        ack_key_id: "broker-ack-v1".into(),
    });
    assert!(BrokerRuntime::new(config).is_err());
}

fn fixture(root: &std::path::Path, key: &SigningKey) -> BrokerRuntimeConfig {
    let protected = root.join("protected");
    fs::create_dir(&protected).unwrap();
    let protected = protected.canonicalize().unwrap();
    let protected = protected.to_str().unwrap();
    let manifest = manifest(protected);
    BrokerRuntimeConfig {
        schema_version: RUNTIME_SCHEMA_VERSION,
        runtime_id: Uuid::from_u128(1),
        runtime_revision: 1,
        platform: ExecutionHostPlatform::Macos,
        owner_uid: Some(unsafe { libc::geteuid() }),
        next_request_sequence: 7,
        restart_quarantined: false,
        broker_id: Uuid::from_u128(2),
        broker_revision: 1,
        broker_executable_sha256: Sha256::digest(
            fs::read(env!("CARGO_BIN_EXE_assemblywright-broker")).unwrap(),
        )
        .into(),
        executor_id: Uuid::from_u128(3),
        executor_revision: 1,
        executor_executable_sha256: [2; 32],
        protected_control_plane_sha256: manifest.canonical_sha256().unwrap(),
        authority_key_id: "master-v1".into(),
        authority_verifying_key: key.verifying_key().to_bytes(),
        bound_child_epoch_id: Uuid::from_u128(4),
        bound_session_id: Uuid::from_u128(5),
        bound_session_revision: 1,
        bound_child_epoch_revision: 1,
        bound_feature_lifecycle_revision: 1,
        bound_authority_revision: 9,
        next_action_sequence: 11,
        protected_manifest: manifest,
        ipc: None,
    }
}

fn request(
    config: &BrokerRuntimeConfig,
    key: &SigningKey,
    sequence: u64,
    intent: BrokerRuntimeIntent,
) -> Vec<u8> {
    let mut request = BrokerRuntimeRequest {
        schema_version: RUNTIME_SCHEMA_VERSION,
        runtime_id: config.runtime_id,
        runtime_revision: config.runtime_revision,
        request_id: Uuid::new_v4(),
        request_sequence: sequence,
        nonce: Uuid::new_v4(),
        session_id: config.bound_session_id,
        session_revision: config.bound_session_revision,
        child_epoch_id: config.bound_child_epoch_id,
        child_epoch_revision: config.bound_child_epoch_revision,
        feature_lifecycle_revision: config.bound_feature_lifecycle_revision,
        authority_revision: config.bound_authority_revision,
        signer_key_id: config.authority_key_id.clone(),
        intent,
        signature: Vec::new(),
    };
    request.sign(key).unwrap();
    serde_json::to_vec(&request).unwrap()
}

fn write_config(root: &std::path::Path, config: &BrokerRuntimeConfig) -> (String, String) {
    let bytes = serde_json::to_vec(config).unwrap();
    let path = root.join("broker-runtime.json");
    fs::write(&path, &bytes).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let digest = Sha256::digest(&bytes);
    (path.to_str().unwrap().into(), format!("{digest:x}"))
}

fn send_frame(writer: &mut impl Write, frame: &[u8]) {
    writer
        .write_all(&(frame.len() as u32).to_be_bytes())
        .unwrap();
    writer.write_all(frame).unwrap();
    writer.flush().unwrap();
}

#[test]
fn inherited_pipe_accepts_only_signed_exact_fifo_shutdown() {
    let temp = tempdir().unwrap();
    let key = SigningKey::from_bytes(&[21; 32]);
    let config = fixture(temp.path(), &key);
    let (path, digest) = write_config(temp.path(), &config);
    let mut child = Command::new(env!("CARGO_BIN_EXE_assemblywright-broker"))
        .args(["--config", &path, "--config-sha256", &digest])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    send_frame(
        child.stdin.as_mut().unwrap(),
        &request(&config, &key, 7, BrokerRuntimeIntent::Shutdown),
    );
    drop(child.stdin.take());
    let mut stdout = child.stdout.take().unwrap();
    let mut length = [0_u8; 4];
    stdout.read_exact(&mut length).unwrap();
    let mut body = vec![0; u32::from_be_bytes(length) as usize];
    stdout.read_exact(&mut body).unwrap();
    let response: BrokerRuntimeResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(response.result, BrokerRuntimeResult::Shutdown);
    assert!(child.wait().unwrap().success());
}

#[test]
fn sequence_gap_quarantines_without_a_response() {
    let temp = tempdir().unwrap();
    let key = SigningKey::from_bytes(&[22; 32]);
    let config = fixture(temp.path(), &key);
    let (path, digest) = write_config(temp.path(), &config);
    let mut child = Command::new(env!("CARGO_BIN_EXE_assemblywright-broker"))
        .args(["--config", &path, "--config-sha256", &digest])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    send_frame(
        child.stdin.as_mut().unwrap(),
        &request(&config, &key, 8, BrokerRuntimeIntent::Shutdown),
    );
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(78));
    assert!(output.stdout.is_empty());
}

#[test]
fn emergency_intent_is_authenticated_acknowledged_and_terminates_runtime() {
    let temp = tempdir().unwrap();
    let key = SigningKey::from_bytes(&[23; 32]);
    let config = fixture(temp.path(), &key);
    let (path, digest) = write_config(temp.path(), &config);
    let mut child = Command::new(env!("CARGO_BIN_EXE_assemblywright-broker"))
        .args(["--config", &path, "--config-sha256", &digest])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    send_frame(
        child.stdin.as_mut().unwrap(),
        &request(&config, &key, 7, BrokerRuntimeIntent::EmergencyTerminate),
    );
    drop(child.stdin.take());
    let mut stdout = child.stdout.take().unwrap();
    let mut length = [0_u8; 4];
    stdout.read_exact(&mut length).unwrap();
    let mut body = vec![0; u32::from_be_bytes(length) as usize];
    stdout.read_exact(&mut body).unwrap();
    let response: BrokerRuntimeResponse = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        response.result,
        BrokerRuntimeResult::TerminationIntentAccepted { active_effects: 0 }
    );
    assert!(child.wait().unwrap().success());
}
