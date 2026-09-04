#![cfg(unix)]

use assemblywright_executor::runtime::{
    ExecutorRuntime, ExecutorRuntimeConfig, ExecutorRuntimeIntent, ExecutorRuntimeRequest,
    ExecutorRuntimeResponse, ExecutorRuntimeResult, RuntimeError, RUNTIME_SCHEMA_VERSION,
};
use assemblywright_executor::ExecutorAuthoritySnapshot;
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

fn fixture(root: &std::path::Path, key: &SigningKey) -> ExecutorRuntimeConfig {
    let protected = root.join("protected");
    fs::create_dir(&protected).unwrap();
    let protected = protected.canonicalize().unwrap();
    let manifest = manifest(protected.to_str().unwrap());
    let session_id = Uuid::from_u128(15);
    let child_epoch_id = Uuid::from_u128(14);
    let mut snapshot = ExecutorAuthoritySnapshot {
        authority_revision: 9,
        session_id,
        session_revision: 1,
        child_epoch_id,
        child_epoch_revision: 1,
        feature_lifecycle_revision: 1,
        emergency_paused: false,
        revoked: false,
        signer_key_id: "master-v1".into(),
        signature: Vec::new(),
    };
    snapshot.sign(key).unwrap();
    ExecutorRuntimeConfig {
        schema_version: RUNTIME_SCHEMA_VERSION,
        runtime_id: Uuid::from_u128(11),
        runtime_revision: 1,
        platform: ExecutionHostPlatform::Macos,
        owner_uid: Some(unsafe { libc::geteuid() }),
        next_request_sequence: 7,
        restart_quarantined: false,
        executor_id: Uuid::from_u128(12),
        executor_revision: 1,
        executor_executable_sha256: Sha256::digest(
            fs::read(env!("CARGO_BIN_EXE_assemblywright-executor")).unwrap(),
        )
        .into(),
        broker_id: Uuid::from_u128(13),
        broker_revision: 1,
        broker_executable_sha256: [2; 32],
        protected_control_plane_sha256: manifest.canonical_sha256().unwrap(),
        authority_key_id: "master-v1".into(),
        authority_verifying_key: key.verifying_key().to_bytes(),
        receipt_key_id: "executor-receipt-v1".into(),
        bound_child_epoch_id: child_epoch_id,
        bound_session_id: session_id,
        bound_session_revision: 1,
        bound_child_epoch_revision: 1,
        bound_feature_lifecycle_revision: 1,
        bound_authority_revision: 9,
        bound_authority_snapshot_sha256: snapshot.sha256().unwrap(),
        next_action_sequence: 11,
        protected_manifest: manifest,
        authority_snapshot: snapshot,
    }
}

fn request_object(
    config: &ExecutorRuntimeConfig,
    key: &SigningKey,
    sequence: u64,
    intent: ExecutorRuntimeIntent,
) -> ExecutorRuntimeRequest {
    let mut request = ExecutorRuntimeRequest {
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
    request
}

fn request(config: &ExecutorRuntimeConfig, key: &SigningKey, sequence: u64) -> Vec<u8> {
    serde_json::to_vec(&request_object(
        config,
        key,
        sequence,
        ExecutorRuntimeIntent::Shutdown,
    ))
    .unwrap()
}

#[test]
fn restart_quarantine_and_stop_without_an_active_child_fail_closed() {
    let temp = tempdir().unwrap();
    let key = SigningKey::from_bytes(&[27; 32]);
    let mut quarantined_config = fixture(temp.path(), &key);
    quarantined_config.restart_quarantined = true;
    let mut runtime = ExecutorRuntime::new(quarantined_config.clone(), [31; 32]).unwrap();
    let shutdown = request_object(
        &quarantined_config,
        &key,
        7,
        ExecutorRuntimeIntent::Shutdown,
    );
    assert_eq!(
        runtime.handle(shutdown).unwrap_err(),
        RuntimeError::Quarantined
    );

    let second = tempdir().unwrap();
    let config = fixture(second.path(), &key);
    assert_eq!(
        ExecutorRuntime::new(config.clone(), [0; 32]).err(),
        Some(RuntimeError::InvalidConfig)
    );
    let mut runtime = ExecutorRuntime::new(config.clone(), [31; 32]).unwrap();
    let stop = request_object(
        &config,
        &key,
        7,
        ExecutorRuntimeIntent::Stop {
            last_checkpoint_sha256: [7; 32],
            graceful_window_ms: 100,
            forced_window_ms: 100,
        },
    );
    assert_eq!(
        runtime.handle(stop).unwrap_err(),
        RuntimeError::InvalidRequest
    );
    let shutdown = request_object(&config, &key, 7, ExecutorRuntimeIntent::Shutdown);
    assert_eq!(
        runtime.handle(shutdown).unwrap_err(),
        RuntimeError::Quarantined
    );
}

fn write_config(root: &std::path::Path, config: &ExecutorRuntimeConfig) -> (String, String) {
    let bytes = serde_json::to_vec(config).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("receipt_signing_seed"));
    let path = root.join("executor-runtime.json");
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
fn inherited_pipe_accepts_only_signed_exact_authority_shutdown() {
    let temp = tempdir().unwrap();
    let key = SigningKey::from_bytes(&[25; 32]);
    let config = fixture(temp.path(), &key);
    let (path, digest) = write_config(temp.path(), &config);
    let mut child = Command::new(env!("CARGO_BIN_EXE_assemblywright-executor"))
        .args(["--config", &path, "--config-sha256", &digest])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.as_mut().unwrap().write_all(&[31; 32]).unwrap();
    send_frame(child.stdin.as_mut().unwrap(), &request(&config, &key, 7));
    drop(child.stdin.take());
    let mut stdout = child.stdout.take().unwrap();
    let mut length = [0_u8; 4];
    stdout.read_exact(&mut length).unwrap();
    let mut body = vec![0; u32::from_be_bytes(length) as usize];
    stdout.read_exact(&mut body).unwrap();
    let response: ExecutorRuntimeResponse = serde_json::from_slice(&body).unwrap();
    assert!(matches!(response.result, ExecutorRuntimeResult::Shutdown));
    assert!(child.wait().unwrap().success());
}

#[test]
fn wrong_authority_revision_quarantines_without_a_response() {
    let temp = tempdir().unwrap();
    let key = SigningKey::from_bytes(&[26; 32]);
    let config = fixture(temp.path(), &key);
    let (path, digest) = write_config(temp.path(), &config);
    let mut signed: ExecutorRuntimeRequest =
        serde_json::from_slice(&request(&config, &key, 7)).unwrap();
    signed.authority_revision += 1;
    signed.signature.clear();
    signed.sign(&key).unwrap();
    let frame = serde_json::to_vec(&signed).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_assemblywright-executor"))
        .args(["--config", &path, "--config-sha256", &digest])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.as_mut().unwrap().write_all(&[31; 32]).unwrap();
    send_frame(child.stdin.as_mut().unwrap(), &frame);
    drop(child.stdin.take());
    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(78));
    assert!(output.stdout.is_empty());
}
