#![cfg(unix)]

use assemblywright_broker::{
    object_identity_sha256, BrokerError, BrokerIdentity, BrokerOperation, BrokerPolicy,
};
use assemblywright_protocol::*;
use ed25519_dalek::SigningKey;
use std::fs;
use std::path::Path;
use tempfile::tempdir;
use uuid::Uuid;

fn identity(key: &SigningKey, protected_control_plane_sha256: [u8; 32]) -> BrokerIdentity {
    BrokerIdentity {
        platform: ExecutionHostPlatform::Macos,
        broker_id: Uuid::parse_str("30000000-0000-4000-8000-000000000001").unwrap(),
        broker_revision: 1,
        broker_executable_sha256: [1; 32],
        executor_id: Uuid::parse_str("30000000-0000-4000-8000-000000000002").unwrap(),
        executor_revision: 1,
        executor_executable_sha256: [2; 32],
        protected_control_plane_sha256,
        signer_key_id: "master-action-v1".into(),
        verifying_key: key.verifying_key(),
        bound_child_epoch_id: Uuid::parse_str("30000000-0000-4000-8000-000000000003").unwrap(),
        bound_session_id: Uuid::parse_str("30000000-0000-4000-8000-000000000004").unwrap(),
        bound_session_revision: 1,
        bound_child_epoch_revision: 1,
        bound_feature_lifecycle_revision: 1,
        bound_authority_revision: 1,
        next_action_sequence: 1,
    }
}

fn envelope(
    key: &SigningKey,
    broker: &BrokerIdentity,
    operation: &BrokerOperation,
    target: &Path,
    expected_object_sha256: Option<[u8; 32]>,
) -> ExecutionActionEnvelope {
    let target = target.to_str().unwrap();
    let parent = Path::new(target).parent().unwrap();
    let mut envelope = ExecutionActionEnvelope {
        schema_version: EXECUTION_ACTION_ENVELOPE_SCHEMA_VERSION,
        action_id: Uuid::new_v4(),
        action_sequence: 1,
        feature_id: Uuid::new_v4(),
        repository_id: Uuid::new_v4(),
        session_id: broker.bound_session_id,
        session_revision: 1,
        child_epoch_id: broker.bound_child_epoch_id,
        child_epoch_revision: 1,
        feature_lifecycle_revision: 1,
        authority_revision: 1,
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
            expected_object_sha256,
            expected_single_link: true,
        }],
        operation_sha256: operation.sha256().unwrap(),
        working_directory_sha256: [3; 32],
        environment_keys: Vec::new(),
        effect_classification: ExecutionEffectClassification::LocalDurable,
        deadline_ms: now_ms() + 60_000,
        cancellation_behavior: ExecutionCancellationBehavior::CheckpointThenTerminate,
        reconciliation_strategy: ExecutionReconciliationStrategy::ExactPostStateDigest,
        issued_at_ms: now_ms() - 1_000,
        nonce: Uuid::new_v4(),
        signer_key_id: broker.signer_key_id.clone(),
        signature: Vec::new(),
    };
    envelope.sign(key).unwrap();
    envelope
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn protected_manifest(path: &Path) -> ProtectedControlPlanePathManifest {
    let path = path.to_str().unwrap().to_string();
    ProtectedControlPlanePathManifest {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        platform: ExecutionHostPlatform::Macos,
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

#[test]
fn ordinary_create_is_digest_bound_single_use_and_protected_descendant_is_denied() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let protected = root.join("control-plane");
    fs::create_dir(&protected).unwrap();
    let allowed_parent = root.join("feature-data");
    fs::create_dir(&allowed_parent).unwrap();
    let key = SigningKey::from_bytes(&[11; 32]);
    let manifest = protected_manifest(&protected);
    let broker = identity(&key, manifest.canonical_sha256().unwrap());
    let policy = BrokerPolicy::new(broker.clone(), manifest.clone()).unwrap();

    let allowed = allowed_parent.join("created");
    let operation = BrokerOperation::CreateDirectory {
        target: allowed.to_str().unwrap().into(),
    };
    let action = envelope(&key, &broker, &operation, &allowed, None);
    let _admission = policy.admit(&action, &operation).unwrap();
    assert_eq!(
        policy.admit(&action, &operation).err().unwrap(),
        BrokerError::Replay
    );
    assert!(!allowed.exists());

    let protected_policy = BrokerPolicy::new(broker.clone(), manifest).unwrap();
    let protected_target = protected.join("master.sqlite3");
    let protected_operation = BrokerOperation::CreateDirectory {
        target: protected_target.to_str().unwrap().into(),
    };
    let protected_action = envelope(&key, &broker, &protected_operation, &protected_target, None);
    assert_eq!(
        protected_policy
            .admit(&protected_action, &protected_operation)
            .err()
            .unwrap(),
        BrokerError::ProtectedTarget
    );
    assert!(!protected_target.exists());
}

#[test]
fn signature_identity_digest_symlink_and_hardlink_attacks_fail_before_effect() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let protected = root.join("control-plane");
    fs::create_dir(&protected).unwrap();
    let data = root.join("data");
    fs::create_dir(&data).unwrap();
    let key = SigningKey::from_bytes(&[12; 32]);
    let manifest = protected_manifest(&protected);
    let broker = identity(&key, manifest.canonical_sha256().unwrap());
    let policy = BrokerPolicy::new(broker.clone(), manifest).unwrap();

    let target = data.join("new");
    let operation = BrokerOperation::CreateDirectory {
        target: target.to_str().unwrap().into(),
    };
    let mut forged = envelope(&key, &broker, &operation, &target, None);
    forged.broker_revision += 1;
    assert_eq!(
        policy.admit(&forged, &operation).err().unwrap(),
        BrokerError::InvalidIdentity
    );
    assert!(!target.exists());

    let destination = data.join("destination");
    fs::write(&destination, b"safe").unwrap();
    let alias = data.join("alias");
    symlink(&destination, &alias).unwrap();
    let remove_alias = BrokerOperation::RemoveFile {
        target: alias.to_str().unwrap().into(),
    };
    let alias_action = envelope(&key, &broker, &remove_alias, &alias, Some([9; 32]));
    assert_eq!(
        policy.admit(&alias_action, &remove_alias).err().unwrap(),
        BrokerError::UnsafeLink
    );

    let hardlink = data.join("hardlink");
    fs::hard_link(&destination, &hardlink).unwrap();
    let remove_hardlink = BrokerOperation::RemoveFile {
        target: hardlink.to_str().unwrap().into(),
    };
    let hardlink_action = envelope(&key, &broker, &remove_hardlink, &hardlink, Some([8; 32]));
    assert_eq!(
        policy
            .admit(&hardlink_action, &remove_hardlink)
            .err()
            .unwrap(),
        BrokerError::UnsafeLink
    );
    assert_eq!(fs::read(&destination).unwrap(), b"safe");
}

#[test]
fn restored_sequence_seed_rejects_stale_and_gapped_actions_without_mutation() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let protected = root.join("control-plane");
    let data = root.join("data");
    fs::create_dir(&protected).unwrap();
    fs::create_dir(&data).unwrap();
    let key = SigningKey::from_bytes(&[13; 32]);
    let manifest = protected_manifest(&protected);
    let mut broker = identity(&key, manifest.canonical_sha256().unwrap());
    broker.next_action_sequence = 2;
    let stale_policy = BrokerPolicy::new(broker.clone(), manifest.clone()).unwrap();
    let target = data.join("never-created");
    let operation = BrokerOperation::CreateDirectory {
        target: target.to_str().unwrap().into(),
    };

    let stale = envelope(&key, &broker, &operation, &target, None);
    assert_eq!(
        stale_policy.admit(&stale, &operation).err().unwrap(),
        BrokerError::Replay
    );
    assert_eq!(
        stale_policy.admit(&stale, &operation).err().unwrap(),
        BrokerError::StateUnavailable
    );
    let gapped_policy = BrokerPolicy::new(broker.clone(), manifest).unwrap();
    let mut gapped = stale;
    gapped.action_sequence = 3;
    gapped.signature.clear();
    gapped.sign(&key).unwrap();
    assert_eq!(
        gapped_policy.admit(&gapped, &operation).err().unwrap(),
        BrokerError::Replay
    );
    assert!(!target.exists());
}

#[test]
fn create_requires_exact_effect_contract_and_one_existing_parent() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let protected = root.join("control-plane");
    let data = root.join("data");
    fs::create_dir(&protected).unwrap();
    fs::create_dir(&data).unwrap();
    let key = SigningKey::from_bytes(&[14; 32]);
    let manifest = protected_manifest(&protected);
    let broker = identity(&key, manifest.canonical_sha256().unwrap());
    let policy = BrokerPolicy::new(broker.clone(), manifest).unwrap();

    let target = data.join("new");
    let operation = BrokerOperation::CreateDirectory {
        target: target.to_str().unwrap().into(),
    };
    let mut wrong_cancellation = envelope(&key, &broker, &operation, &target, None);
    wrong_cancellation.cancellation_behavior = ExecutionCancellationBehavior::ImmediateTerminate;
    wrong_cancellation.signature.clear();
    wrong_cancellation.sign(&key).unwrap();
    assert_eq!(
        policy.admit(&wrong_cancellation, &operation).err().unwrap(),
        BrokerError::InvalidOperation
    );

    let nested = data.join("missing-parent").join("leaf");
    let nested_operation = BrokerOperation::CreateDirectory {
        target: nested.to_str().unwrap().into(),
    };
    let mut nested_action = envelope(&key, &broker, &operation, &target, None);
    nested_action.targets[0].canonical_path = nested.to_str().unwrap().into();
    nested_action.targets[0].canonical_path_sha256 =
        execution_path_sha256(ExecutionHostPlatform::Macos, nested.to_str().unwrap()).unwrap();
    nested_action.operation_sha256 = nested_operation.sha256().unwrap();
    nested_action.signature.clear();
    nested_action.sign(&key).unwrap();
    assert_eq!(
        policy
            .admit(&nested_action, &nested_operation)
            .err()
            .unwrap(),
        BrokerError::AmbiguousTarget
    );
    assert!(!target.exists());
    assert!(!nested.exists());
}
