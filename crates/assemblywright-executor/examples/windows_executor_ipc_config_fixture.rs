use assemblywright_executor::runtime::{
    ExecutorIpcBootstrap, ExecutorRuntimeConfig, RUNTIME_SCHEMA_VERSION,
};
use assemblywright_executor::ExecutorAuthoritySnapshot;
use assemblywright_protocol::{ExecutionHostPlatform, ProtectedControlPlanePathManifest};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};
use uuid::Uuid;

fn main() {
    let mut args = std::env::args_os().skip(1);
    let output = PathBuf::from(args.next().expect("output"));
    let executor_executable = PathBuf::from(args.next().expect("executor executable"));
    let broker_executable = PathBuf::from(args.next().expect("broker executable"));
    let state = PathBuf::from(args.next().expect("state"));
    let seed = PathBuf::from(args.next().expect("seed"));
    let pipe_name = args.next().expect("pipe").into_string().unwrap();
    let broker_sid = args.next().expect("broker sid").into_string().unwrap();
    let executor_sid = args.next().expect("executor sid").into_string().unwrap();
    assert!(args.next().is_none());
    let root = output.parent().unwrap().to_string_lossy().into_owned();
    let manifest = ProtectedControlPlanePathManifest {
        schema_version: assemblywright_protocol::FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        platform: ExecutionHostPlatform::Windows,
        master_binary: root.clone(),
        broker_binary: root.clone(),
        service_configuration: root.clone(),
        authority_database: root.clone(),
        database_backups: root.clone(),
        audit: root.clone(),
        owner_tokens_and_signing: root.clone(),
        trust_and_update_roots: root.clone(),
        ipc_and_enforcement_state: root.clone(),
        release_evidence: root.clone(),
        resource_reservations: root,
    };
    let authority = SigningKey::from_bytes(&[81; 32]);
    let mut snapshot = ExecutorAuthoritySnapshot {
        authority_revision: 1,
        session_id: Uuid::from_u128(104),
        session_revision: 1,
        child_epoch_id: Uuid::from_u128(105),
        child_epoch_revision: 1,
        feature_lifecycle_revision: 1,
        emergency_paused: false,
        revoked: false,
        signer_key_id: "fixture-master-ipc-v1".into(),
        signature: Vec::new(),
    };
    snapshot.sign(&authority).unwrap();
    let config = ExecutorRuntimeConfig {
        schema_version: RUNTIME_SCHEMA_VERSION,
        runtime_id: Uuid::from_u128(101),
        runtime_revision: 1,
        platform: ExecutionHostPlatform::Windows,
        owner_uid: None,
        next_request_sequence: 1,
        restart_quarantined: false,
        executor_id: Uuid::from_u128(103),
        executor_revision: 1,
        executor_executable_sha256: Sha256::digest(fs::read(executor_executable).unwrap()).into(),
        broker_id: Uuid::from_u128(102),
        broker_revision: 1,
        broker_executable_sha256: Sha256::digest(fs::read(broker_executable).unwrap()).into(),
        protected_control_plane_sha256: manifest.canonical_sha256().unwrap(),
        authority_key_id: "fixture-master-ipc-v1".into(),
        authority_verifying_key: authority.verifying_key().to_bytes(),
        receipt_key_id: "fixture-executor-runtime-v1".into(),
        bound_child_epoch_id: Uuid::from_u128(105),
        bound_session_id: Uuid::from_u128(104),
        bound_session_revision: 1,
        bound_child_epoch_revision: 1,
        bound_feature_lifecycle_revision: 1,
        bound_authority_revision: 1,
        bound_authority_snapshot_sha256: snapshot.sha256().unwrap(),
        next_action_sequence: 1,
        protected_manifest: manifest,
        authority_snapshot: snapshot,
        ipc: Some(ExecutorIpcBootstrap {
            pipe_name,
            executor_service_sid: executor_sid,
            expected_broker_service_sid: broker_sid,
            durable_state_path: state,
            ack_seed_path: seed.clone(),
            ack_key_id: "fixture-executor-ack-v1".into(),
        }),
    };
    fs::write(&seed, [83; 32]).unwrap();
    let bytes = serde_json::to_vec(&config).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains(&"53".repeat(32)));
    fs::write(output, &bytes).unwrap();
    println!("{:x}", Sha256::digest(bytes));
}
