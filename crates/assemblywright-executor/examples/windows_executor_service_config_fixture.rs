use assemblywright_executor::runtime::{ExecutorRuntimeConfig, RUNTIME_SCHEMA_VERSION};
use assemblywright_executor::ExecutorAuthoritySnapshot;
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
    let manifest = ProtectedControlPlanePathManifest {
        schema_version: assemblywright_protocol::FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        platform: ExecutionHostPlatform::Windows,
        master_binary: r"C:\Program Files\Assemblywright\bin\assemblywright-master.exe".into(),
        broker_binary: r"C:\Program Files\Assemblywright\bin\assemblywright-broker.exe".into(),
        service_configuration: r"C:\ProgramData\Assemblywright\config\executor.json".into(),
        authority_database: r"C:\ProgramData\Assemblywright\authority\master.sqlite3".into(),
        database_backups: r"C:\ProgramData\Assemblywright\backups".into(),
        audit: r"C:\ProgramData\Assemblywright\audit".into(),
        owner_tokens_and_signing: r"C:\ProgramData\Assemblywright\secrets".into(),
        trust_and_update_roots: r"C:\ProgramData\Assemblywright\updates".into(),
        ipc_and_enforcement_state: r"C:\ProgramData\Assemblywright\ipc".into(),
        release_evidence: r"C:\ProgramData\Assemblywright\release-evidence".into(),
        resource_reservations: r"C:\ProgramData\Assemblywright\reserve".into(),
    };
    let key = SigningKey::from_bytes(&[42; 32]);
    let session = Uuid::new_v4();
    let child = Uuid::new_v4();
    let mut snapshot = ExecutorAuthoritySnapshot {
        authority_revision: 1,
        session_id: session,
        session_revision: 1,
        child_epoch_id: child,
        child_epoch_revision: 1,
        feature_lifecycle_revision: 1,
        emergency_paused: false,
        revoked: false,
        signer_key_id: "fixture-master-v1".into(),
        signature: Vec::new(),
    };
    snapshot.sign(&key).unwrap();
    let config = ExecutorRuntimeConfig {
        schema_version: RUNTIME_SCHEMA_VERSION,
        runtime_id: Uuid::new_v4(),
        runtime_revision: 1,
        platform: ExecutionHostPlatform::Windows,
        owner_uid: None,
        next_request_sequence: 1,
        restart_quarantined: false,
        executor_id: Uuid::new_v4(),
        executor_revision: 1,
        executor_executable_sha256: Sha256::digest(fs::read(executable).unwrap()).into(),
        broker_id: Uuid::new_v4(),
        broker_revision: 1,
        broker_executable_sha256: [2; 32],
        protected_control_plane_sha256: manifest.canonical_sha256().unwrap(),
        authority_key_id: "fixture-master-v1".into(),
        authority_verifying_key: key.verifying_key().to_bytes(),
        receipt_key_id: "fixture-receipt-v1".into(),
        bound_child_epoch_id: child,
        bound_session_id: session,
        bound_session_revision: 1,
        bound_child_epoch_revision: 1,
        bound_feature_lifecycle_revision: 1,
        bound_authority_revision: 1,
        bound_authority_snapshot_sha256: snapshot.sha256().unwrap(),
        next_action_sequence: 1,
        protected_manifest: manifest,
        authority_snapshot: snapshot,
        ipc: None,
    };
    let bytes = serde_json::to_vec(&config).unwrap();
    fs::write(output, &bytes).unwrap();
    println!("{:x}", Sha256::digest(bytes));
}
