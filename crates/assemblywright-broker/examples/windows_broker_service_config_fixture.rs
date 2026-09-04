use assemblywright_broker::runtime::{BrokerRuntimeConfig, RUNTIME_SCHEMA_VERSION};
use assemblywright_protocol::{ExecutionHostPlatform, ProtectedControlPlanePathManifest};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};
use uuid::Uuid;

fn main() {
    let mut args = std::env::args_os().skip(1);
    let output = PathBuf::from(args.next().expect("output"));
    let executable = PathBuf::from(args.next().expect("executable"));
    assert!(args.next().is_none());
    let root = output.parent().unwrap().canonicalize().unwrap();
    let path = root.to_string_lossy().into_owned();
    let manifest = ProtectedControlPlanePathManifest {
        schema_version: assemblywright_protocol::FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        platform: ExecutionHostPlatform::Windows,
        master_binary: path.clone(),
        broker_binary: path.clone(),
        service_configuration: path.clone(),
        authority_database: path.clone(),
        database_backups: path.clone(),
        audit: path.clone(),
        owner_tokens_and_signing: path.clone(),
        trust_and_update_roots: path.clone(),
        ipc_and_enforcement_state: path.clone(),
        release_evidence: path.clone(),
        resource_reservations: path,
    };
    let key = SigningKey::from_bytes(&[41; 32]);
    let config = BrokerRuntimeConfig {
        schema_version: RUNTIME_SCHEMA_VERSION,
        runtime_id: Uuid::new_v4(),
        runtime_revision: 1,
        platform: ExecutionHostPlatform::Windows,
        owner_uid: None,
        next_request_sequence: 1,
        restart_quarantined: false,
        broker_id: Uuid::new_v4(),
        broker_revision: 1,
        broker_executable_sha256: Sha256::digest(fs::read(executable).unwrap()).into(),
        executor_id: Uuid::new_v4(),
        executor_revision: 1,
        executor_executable_sha256: [2; 32],
        protected_control_plane_sha256: manifest.canonical_sha256().unwrap(),
        authority_key_id: "fixture-master-v1".into(),
        authority_verifying_key: key.verifying_key().to_bytes(),
        bound_child_epoch_id: Uuid::new_v4(),
        bound_session_id: Uuid::new_v4(),
        bound_session_revision: 1,
        bound_child_epoch_revision: 1,
        bound_feature_lifecycle_revision: 1,
        bound_authority_revision: 1,
        next_action_sequence: 1,
        protected_manifest: manifest,
    };
    let bytes = serde_json::to_vec(&config).unwrap();
    fs::write(output, &bytes).unwrap();
    println!("{:x}", Sha256::digest(bytes));
}
