#![cfg(windows)]

use assemblywright_broker::runtime::{
    BrokerRuntime, BrokerRuntimeConfig, BrokerRuntimeIntent, BrokerRuntimeRequest,
    BrokerRuntimeResult, RuntimeError, RUNTIME_SCHEMA_VERSION,
};
use assemblywright_broker::{
    execute_windows_create_directory_once, object_identity_sha256,
    prepare_windows_create_directory_proof, BrokerError, BrokerExecutionOutcome, BrokerIdentity,
    BrokerOperation, BrokerPolicy,
};
use assemblywright_protocol::*;
use ed25519_dalek::SigningKey;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::tempdir;
use uuid::Uuid;

fn protocol_path(path: &Path) -> PathBuf {
    let canonical = path.canonicalize().unwrap();
    let value = canonical.to_str().unwrap();
    PathBuf::from(value.strip_prefix(r"\\?\").unwrap_or(value))
}

fn identity(key: &SigningKey, manifest_sha256: [u8; 32]) -> BrokerIdentity {
    BrokerIdentity {
        platform: ExecutionHostPlatform::Windows,
        broker_id: Uuid::from_u128(1),
        broker_revision: 1,
        broker_executable_sha256: [1; 32],
        executor_id: Uuid::from_u128(2),
        executor_revision: 1,
        executor_executable_sha256: [2; 32],
        protected_control_plane_sha256: manifest_sha256,
        signer_key_id: "master-action-v1".into(),
        verifying_key: key.verifying_key(),
        bound_child_epoch_id: Uuid::from_u128(3),
        bound_session_id: Uuid::from_u128(4),
        bound_session_revision: 1,
        bound_child_epoch_revision: 1,
        bound_feature_lifecycle_revision: 1,
        bound_authority_revision: 1,
        next_action_sequence: 1,
    }
}

fn manifest(path: &Path) -> ProtectedControlPlanePathManifest {
    let path = path.to_str().unwrap().to_string();
    ProtectedControlPlanePathManifest {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
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
    }
}

fn envelope(
    key: &SigningKey,
    broker: &BrokerIdentity,
    operation: &BrokerOperation,
    target: &Path,
    sequence: u64,
) -> ExecutionActionEnvelope {
    let target = target.to_str().unwrap();
    let parent = Path::new(target).parent().unwrap();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    let mut envelope = ExecutionActionEnvelope {
        schema_version: EXECUTION_ACTION_ENVELOPE_SCHEMA_VERSION,
        action_id: Uuid::new_v4(),
        action_sequence: sequence,
        feature_id: Uuid::new_v4(),
        repository_id: Uuid::new_v4(),
        session_id: broker.bound_session_id,
        session_revision: broker.bound_session_revision,
        child_epoch_id: broker.bound_child_epoch_id,
        child_epoch_revision: broker.bound_child_epoch_revision,
        feature_lifecycle_revision: broker.bound_feature_lifecycle_revision,
        authority_revision: broker.bound_authority_revision,
        executor_id: broker.executor_id,
        executor_revision: broker.executor_revision,
        executor_executable_sha256: broker.executor_executable_sha256,
        broker_id: broker.broker_id,
        broker_revision: broker.broker_revision,
        broker_executable_sha256: broker.broker_executable_sha256,
        protected_control_plane_sha256: broker.protected_control_plane_sha256,
        host_platform: broker.platform,
        action_type: operation.action_type(),
        targets: vec![ExecutionTargetIdentity {
            platform: broker.platform,
            canonical_path: target.into(),
            canonical_path_sha256: execution_path_sha256(broker.platform, target).unwrap(),
            canonical_parent_sha256: object_identity_sha256(broker.platform, parent).unwrap(),
            expected_object_sha256: None,
            expected_single_link: true,
        }],
        operation_sha256: operation.sha256().unwrap(),
        working_directory_sha256: [3; 32],
        environment_keys: Vec::new(),
        effect_classification: ExecutionEffectClassification::LocalDurable,
        deadline_ms: now + 60_000,
        cancellation_behavior: ExecutionCancellationBehavior::CheckpointThenTerminate,
        reconciliation_strategy: ExecutionReconciliationStrategy::ExactPostStateDigest,
        issued_at_ms: now - 1,
        nonce: Uuid::new_v4(),
        signer_key_id: broker.signer_key_id.clone(),
        signature: Vec::new(),
    };
    envelope.sign(key).unwrap();
    envelope
}

fn fixture() -> (
    tempfile::TempDir,
    SigningKey,
    BrokerIdentity,
    BrokerPolicy,
    std::path::PathBuf,
) {
    let temp = tempdir().unwrap();
    let root = protocol_path(temp.path());
    let protected = root.join("protected");
    let allowed = root.join("allowed");
    fs::create_dir(&protected).unwrap();
    fs::create_dir(&allowed).unwrap();
    let key = SigningKey::from_bytes(&[31; 32]);
    let manifest = manifest(&protected);
    let broker = identity(&key, manifest.canonical_sha256().unwrap());
    let policy = BrokerPolicy::new(broker.clone(), manifest).unwrap();
    (temp, key, broker, policy, allowed)
}

#[test]
fn creates_one_leaf_and_returns_path_free_action_bound_post_state() {
    let (_temp, key, broker, policy, allowed) = fixture();
    let target = allowed.join("created");
    let operation = BrokerOperation::CreateDirectory {
        target: target.to_str().unwrap().into(),
    };
    let action = envelope(&key, &broker, &operation, &target, 1);
    let outcome = execute_windows_create_directory_once(&policy, &action, &operation).unwrap();
    let BrokerExecutionOutcome::Applied { result } = outcome else {
        panic!("unexpected reconciliation-required outcome");
    };

    assert_eq!(result.action_id, action.action_id);
    assert_eq!(
        result.post_state_identity_sha256,
        object_identity_sha256(ExecutionHostPlatform::Windows, &target).unwrap()
    );
    assert_ne!(result.post_state_sha256, [0; 32]);
    assert!(target.is_dir());
    let encoded = serde_json::to_string(&result).unwrap();
    assert!(!encoded.contains(target.to_str().unwrap()));
}

#[test]
fn retained_parent_handle_blocks_rename_until_effect_is_verified() {
    let (_temp, key, broker, policy, allowed) = fixture();
    let target = allowed.join("never-created");
    let operation = BrokerOperation::CreateDirectory {
        target: target.to_str().unwrap().into(),
    };
    let action = envelope(&key, &broker, &operation, &target, 1);
    let proof = prepare_windows_create_directory_proof(&policy, &action, &operation).unwrap();

    let moved = allowed.with_file_name("allowed-moved");
    assert!(fs::rename(&allowed, &moved).is_err());
    assert!(!moved.join("never-created").exists());
    let BrokerExecutionOutcome::Applied { result } = proof.execute().unwrap() else {
        panic!("unexpected reconciliation-required outcome");
    };
    assert_eq!(result.action_id, action.action_id);
    assert!(target.is_dir());
}

#[test]
fn retained_ancestor_handles_block_ancestor_rename_until_effect_is_verified() {
    let (_temp, key, broker, policy, allowed) = fixture();
    let ancestor = allowed.join("ancestor");
    let parent = ancestor.join("parent");
    fs::create_dir(&ancestor).unwrap();
    fs::create_dir(&parent).unwrap();
    let target = parent.join("created");
    let operation = BrokerOperation::CreateDirectory {
        target: target.to_str().unwrap().into(),
    };
    let action = envelope(&key, &broker, &operation, &target, 1);
    let proof = prepare_windows_create_directory_proof(&policy, &action, &operation).unwrap();

    let moved = allowed.join("ancestor-moved");
    assert!(fs::rename(&ancestor, &moved).is_err());
    assert!(!moved.join("parent").join("created").exists());
    assert!(matches!(
        proof.execute().unwrap(),
        BrokerExecutionOutcome::Applied { .. }
    ));
    assert!(target.is_dir());
}

#[test]
fn target_race_and_case_alias_fail_closed() {
    let (_temp, key, broker, policy, allowed) = fixture();
    let target = allowed.join("Race");
    let operation = BrokerOperation::CreateDirectory {
        target: target.to_str().unwrap().into(),
    };
    let action = envelope(&key, &broker, &operation, &target, 1);
    let proof = prepare_windows_create_directory_proof(&policy, &action, &operation).unwrap();
    fs::create_dir(allowed.join("race")).unwrap();
    assert_eq!(proof.execute(), Err(BrokerError::AmbiguousTarget));
    let names: Vec<_> = fs::read_dir(&allowed)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert!(names.iter().any(|name| name == "race"));
    assert!(!names.iter().any(|name| name == "Race"));
}

#[test]
fn runtime_keeps_dispatch_effect_disabled_and_replay_quarantines() {
    let temp = tempdir().unwrap();
    let root = protocol_path(temp.path());
    let protected = root.join("protected");
    let allowed = root.join("allowed");
    fs::create_dir(&protected).unwrap();
    fs::create_dir(&allowed).unwrap();
    let key = SigningKey::from_bytes(&[32; 32]);
    let manifest = manifest(&protected);
    let broker = identity(&key, manifest.canonical_sha256().unwrap());
    let config = BrokerRuntimeConfig {
        schema_version: RUNTIME_SCHEMA_VERSION,
        runtime_id: Uuid::from_u128(20),
        runtime_revision: 1,
        platform: ExecutionHostPlatform::Windows,
        owner_uid: None,
        next_request_sequence: 1,
        restart_quarantined: false,
        broker_id: broker.broker_id,
        broker_revision: broker.broker_revision,
        broker_executable_sha256: broker.broker_executable_sha256,
        executor_id: broker.executor_id,
        executor_revision: broker.executor_revision,
        executor_executable_sha256: broker.executor_executable_sha256,
        protected_control_plane_sha256: broker.protected_control_plane_sha256,
        authority_key_id: broker.signer_key_id.clone(),
        authority_verifying_key: key.verifying_key().to_bytes(),
        bound_child_epoch_id: broker.bound_child_epoch_id,
        bound_session_id: broker.bound_session_id,
        bound_session_revision: broker.bound_session_revision,
        bound_child_epoch_revision: broker.bound_child_epoch_revision,
        bound_feature_lifecycle_revision: broker.bound_feature_lifecycle_revision,
        bound_authority_revision: broker.bound_authority_revision,
        next_action_sequence: 1,
        protected_manifest: manifest,
    };
    let target = allowed.join("runtime-created");
    let operation = BrokerOperation::CreateDirectory {
        target: target.to_str().unwrap().into(),
    };
    let action = envelope(&key, &broker, &operation, &target, 1);
    let mut request = BrokerRuntimeRequest {
        schema_version: RUNTIME_SCHEMA_VERSION,
        runtime_id: config.runtime_id,
        runtime_revision: config.runtime_revision,
        request_id: Uuid::new_v4(),
        request_sequence: 1,
        nonce: Uuid::new_v4(),
        session_id: config.bound_session_id,
        session_revision: config.bound_session_revision,
        child_epoch_id: config.bound_child_epoch_id,
        child_epoch_revision: config.bound_child_epoch_revision,
        feature_lifecycle_revision: config.bound_feature_lifecycle_revision,
        authority_revision: config.bound_authority_revision,
        signer_key_id: config.authority_key_id.clone(),
        intent: BrokerRuntimeIntent::Dispatch {
            envelope: Box::new(action),
            operation,
        },
        signature: Vec::new(),
    };
    request.sign(&key).unwrap();
    let replay = request.clone();
    let mut runtime = BrokerRuntime::new(config).unwrap();

    let response = runtime.handle(request).unwrap();
    assert_eq!(
        response.result,
        BrokerRuntimeResult::ValidatedEffectDisabled
    );
    assert!(!target.exists());
    assert!(matches!(
        runtime.handle(replay),
        Err(RuntimeError::InvalidRequest)
    ));
    assert!(matches!(
        runtime.handle(BrokerRuntimeRequest {
            schema_version: RUNTIME_SCHEMA_VERSION,
            runtime_id: Uuid::from_u128(20),
            runtime_revision: 1,
            request_id: Uuid::new_v4(),
            request_sequence: 2,
            nonce: Uuid::new_v4(),
            session_id: broker.bound_session_id,
            session_revision: 1,
            child_epoch_id: broker.bound_child_epoch_id,
            child_epoch_revision: 1,
            feature_lifecycle_revision: 1,
            authority_revision: 1,
            signer_key_id: broker.signer_key_id,
            intent: BrokerRuntimeIntent::Shutdown,
            signature: Vec::new(),
        }),
        Err(RuntimeError::Quarantined)
    ));
}
