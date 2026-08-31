#![cfg(target_os = "macos")]

use assemblywright_executor::{
    ExecutorAuthoritySnapshot, ExecutorError, ExecutorIdentity, ExecutorPolicy,
    UnprivilegedProcessOperation,
};
use assemblywright_protocol::*;
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::MetadataExt;
use tempfile::tempdir;
use uuid::Uuid;

#[test]
fn macos_execution_fails_closed_before_descendant_can_escape_process_group() {
    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let feature = root.join("feature");
    let protected = root.join("control-plane");
    fs::create_dir(&feature).unwrap();
    fs::create_dir(&protected).unwrap();
    let shell = std::path::PathBuf::from("/bin/sh");

    let authority = SigningKey::from_bytes(&[21; 32]);
    let receipt = SigningKey::from_bytes(&[22; 32]);
    let executor_id = Uuid::new_v4();
    let broker_id = Uuid::new_v4();
    let child_epoch_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();
    let protected_path = protected.to_str().unwrap().to_string();
    let manifest = ProtectedControlPlanePathManifest {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        platform: ExecutionHostPlatform::Macos,
        master_binary: protected_path.clone(),
        broker_binary: protected_path.clone(),
        service_configuration: protected_path.clone(),
        authority_database: protected_path.clone(),
        database_backups: protected_path.clone(),
        audit: protected_path.clone(),
        owner_tokens_and_signing: protected_path.clone(),
        trust_and_update_roots: protected_path.clone(),
        ipc_and_enforcement_state: protected_path.clone(),
        release_evidence: protected_path.clone(),
        resource_reservations: protected_path,
    };
    let protected_digest = manifest.canonical_sha256().unwrap();
    let identity = ExecutorIdentity {
        platform: ExecutionHostPlatform::Macos,
        executor_id,
        executor_revision: 1,
        executor_executable_sha256: [1; 32],
        broker_id,
        broker_revision: 1,
        broker_executable_sha256: [2; 32],
        protected_control_plane_sha256: protected_digest,
        authority_key_id: "master-action-v1".into(),
        authority_verifying_key: authority.verifying_key(),
        receipt_key_id: "executor-receipt-v1".into(),
        receipt_signing_key: receipt.clone(),
        bound_child_epoch_id: child_epoch_id,
        bound_session_id: session_id,
        bound_session_revision: 1,
        bound_child_epoch_revision: 1,
        bound_feature_lifecycle_revision: 1,
        bound_authority_revision: 1,
        bound_authority_snapshot_sha256: authority_snapshot(
            &authority,
            session_id,
            child_epoch_id,
            1,
            false,
            false,
        )
        .sha256()
        .unwrap(),
        next_action_sequence: 1,
    };
    let policy = ExecutorPolicy::new(
        identity,
        manifest,
        authority_snapshot(&authority, session_id, child_epoch_id, 1, false, false),
    )
    .unwrap();
    let operation = UnprivilegedProcessOperation {
        executable: shell.to_str().unwrap().into(),
        arguments: vec![
            "-c".into(),
            "perl -MPOSIX -e 'POSIX::setsid(); open(my $f, q(>), q(escaped-marker)); print $f q(escaped)'"
                .into(),
        ],
        working_directory: feature.to_str().unwrap().into(),
        environment: BTreeMap::new(),
    };
    let mut envelope = ExecutionActionEnvelope {
        schema_version: EXECUTION_ACTION_ENVELOPE_SCHEMA_VERSION,
        action_id: Uuid::new_v4(),
        action_sequence: 1,
        feature_id: Uuid::new_v4(),
        repository_id: Uuid::new_v4(),
        session_id,
        session_revision: 1,
        child_epoch_id,
        child_epoch_revision: 1,
        feature_lifecycle_revision: 1,
        authority_revision: 1,
        executor_id,
        executor_revision: 1,
        executor_executable_sha256: [1; 32],
        broker_id,
        broker_revision: 1,
        broker_executable_sha256: [2; 32],
        protected_control_plane_sha256: protected_digest,
        host_platform: ExecutionHostPlatform::Macos,
        action_type: ExecutionActionType::RunUnprivilegedProcess,
        targets: vec![ExecutionTargetIdentity {
            platform: ExecutionHostPlatform::Macos,
            canonical_path: feature.to_str().unwrap().into(),
            canonical_path_sha256: execution_path_sha256(
                ExecutionHostPlatform::Macos,
                feature.to_str().unwrap(),
            )
            .unwrap(),
            canonical_parent_sha256: object_identity_sha256(feature.parent().unwrap()),
            expected_object_sha256: Some(object_identity_sha256(&feature)),
            expected_single_link: true,
        }],
        operation_sha256: operation.sha256().unwrap(),
        working_directory_sha256: execution_path_sha256(
            ExecutionHostPlatform::Macos,
            feature.to_str().unwrap(),
        )
        .unwrap(),
        environment_keys: Vec::new(),
        effect_classification: ExecutionEffectClassification::LocalReversible,
        deadline_ms: now_ms() + 60_000,
        cancellation_behavior: ExecutionCancellationBehavior::ImmediateTerminate,
        reconciliation_strategy: ExecutionReconciliationStrategy::NoEffectRetry,
        issued_at_ms: now_ms() - 1_000,
        nonce: Uuid::new_v4(),
        signer_key_id: "master-action-v1".into(),
        signature: Vec::new(),
    };
    envelope.sign(&authority).unwrap();

    let admission = policy.admit(&envelope, &operation).unwrap();
    assert_eq!(
        policy.admit(&envelope, &operation).err().unwrap(),
        ExecutorError::Replay
    );
    assert_eq!(
        admission.spawn().err().unwrap(),
        ExecutorError::ContainmentFailed
    );
    assert!(!feature.join("escaped-marker").exists());
}

#[test]
fn signed_identity_rejects_replacement_before_prepare_and_spawn_remains_disabled() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let feature = root.join("feature");
    let held_feature = root.join("held-feature");
    let protected = root.join("control-plane");
    fs::create_dir(&feature).unwrap();
    fs::create_dir(&protected).unwrap();

    let authority = SigningKey::from_bytes(&[31; 32]);
    let receipt = SigningKey::from_bytes(&[32; 32]);
    let executor_id = Uuid::new_v4();
    let broker_id = Uuid::new_v4();
    let child_epoch_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();
    let protected_path = protected.to_str().unwrap().to_string();
    let manifest = ProtectedControlPlanePathManifest {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        platform: ExecutionHostPlatform::Macos,
        master_binary: protected_path.clone(),
        broker_binary: protected_path.clone(),
        service_configuration: protected_path.clone(),
        authority_database: protected_path.clone(),
        database_backups: protected_path.clone(),
        audit: protected_path.clone(),
        owner_tokens_and_signing: protected_path.clone(),
        trust_and_update_roots: protected_path.clone(),
        ipc_and_enforcement_state: protected_path.clone(),
        release_evidence: protected_path.clone(),
        resource_reservations: protected_path,
    };
    let protected_digest = manifest.canonical_sha256().unwrap();
    let identity = ExecutorIdentity {
        platform: ExecutionHostPlatform::Macos,
        executor_id,
        executor_revision: 1,
        executor_executable_sha256: [1; 32],
        broker_id,
        broker_revision: 1,
        broker_executable_sha256: [2; 32],
        protected_control_plane_sha256: protected_digest,
        authority_key_id: "master-action-v1".into(),
        authority_verifying_key: authority.verifying_key(),
        receipt_key_id: "executor-receipt-v1".into(),
        receipt_signing_key: receipt.clone(),
        bound_child_epoch_id: child_epoch_id,
        bound_session_id: session_id,
        bound_session_revision: 1,
        bound_child_epoch_revision: 1,
        bound_feature_lifecycle_revision: 1,
        bound_authority_revision: 1,
        bound_authority_snapshot_sha256: authority_snapshot(
            &authority,
            session_id,
            child_epoch_id,
            1,
            false,
            false,
        )
        .sha256()
        .unwrap(),
        next_action_sequence: 1,
    };
    let policy = ExecutorPolicy::new(
        identity,
        manifest,
        authority_snapshot(&authority, session_id, child_epoch_id, 1, false, false),
    )
    .unwrap();

    let swappable_executable = root.join("worker-executable");
    fs::write(
        &swappable_executable,
        "#!/bin/sh\nprintf attacker > attacker-marker\n",
    )
    .unwrap();
    fs::set_permissions(&swappable_executable, fs::Permissions::from_mode(0o755)).unwrap();
    let unsafe_operation = UnprivilegedProcessOperation {
        executable: swappable_executable.to_str().unwrap().into(),
        arguments: Vec::new(),
        working_directory: feature.to_str().unwrap().into(),
        environment: BTreeMap::new(),
    };
    let unsafe_envelope = signed_envelope(
        &authority,
        &unsafe_operation,
        &feature,
        executor_id,
        broker_id,
        session_id,
        child_epoch_id,
        protected_digest,
        1,
    );
    assert_eq!(
        policy
            .admit(&unsafe_envelope, &unsafe_operation)
            .err()
            .unwrap(),
        ExecutorError::UnsafePath
    );

    let safe_operation = UnprivilegedProcessOperation {
        executable: "/bin/sh".into(),
        arguments: vec!["-c".into(), "printf held > cwd-marker; sleep 60".into()],
        working_directory: feature.to_str().unwrap().into(),
        environment: BTreeMap::new(),
    };
    let safe_envelope = signed_envelope(
        &authority,
        &safe_operation,
        &feature,
        executor_id,
        broker_id,
        session_id,
        child_epoch_id,
        protected_digest,
        1,
    );
    fs::rename(&feature, &held_feature).unwrap();
    fs::create_dir(&feature).unwrap();
    assert_eq!(
        policy.admit(&safe_envelope, &safe_operation).err().unwrap(),
        ExecutorError::UnsafePath
    );

    fs::remove_dir(&feature).unwrap();
    fs::rename(&held_feature, &feature).unwrap();
    let mut invalid_parent_envelope = signed_envelope(
        &authority,
        &safe_operation,
        &feature,
        executor_id,
        broker_id,
        session_id,
        child_epoch_id,
        protected_digest,
        1,
    );
    invalid_parent_envelope.targets[0].canonical_parent_sha256 = [91; 32];
    invalid_parent_envelope.signature.clear();
    invalid_parent_envelope.sign(&authority).unwrap();
    assert_eq!(
        policy
            .admit(&invalid_parent_envelope, &safe_operation)
            .err()
            .unwrap(),
        ExecutorError::UnsafePath
    );

    let safe_envelope = signed_envelope(
        &authority,
        &safe_operation,
        &feature,
        executor_id,
        broker_id,
        session_id,
        child_epoch_id,
        protected_digest,
        1,
    );
    let admission = policy.admit(&safe_envelope, &safe_operation).unwrap();
    fs::rename(&feature, &held_feature).unwrap();
    fs::create_dir(&feature).unwrap();
    assert_eq!(
        admission.spawn().err().unwrap(),
        ExecutorError::ContainmentFailed
    );
    assert!(!held_feature.join("cwd-marker").exists());
    assert!(!feature.join("cwd-marker").exists());
    assert!(!feature.join("attacker-marker").exists());
}

#[allow(clippy::too_many_arguments)]
fn signed_envelope(
    authority: &SigningKey,
    operation: &UnprivilegedProcessOperation,
    feature: &std::path::Path,
    executor_id: Uuid,
    broker_id: Uuid,
    session_id: Uuid,
    child_epoch_id: Uuid,
    protected_digest: [u8; 32],
    action_sequence: u64,
) -> ExecutionActionEnvelope {
    let mut envelope = ExecutionActionEnvelope {
        schema_version: EXECUTION_ACTION_ENVELOPE_SCHEMA_VERSION,
        action_id: Uuid::new_v4(),
        action_sequence,
        feature_id: Uuid::new_v4(),
        repository_id: Uuid::new_v4(),
        session_id,
        session_revision: 1,
        child_epoch_id,
        child_epoch_revision: 1,
        feature_lifecycle_revision: 1,
        authority_revision: 1,
        executor_id,
        executor_revision: 1,
        executor_executable_sha256: [1; 32],
        broker_id,
        broker_revision: 1,
        broker_executable_sha256: [2; 32],
        protected_control_plane_sha256: protected_digest,
        host_platform: ExecutionHostPlatform::Macos,
        action_type: ExecutionActionType::RunUnprivilegedProcess,
        targets: vec![ExecutionTargetIdentity {
            platform: ExecutionHostPlatform::Macos,
            canonical_path: feature.to_str().unwrap().into(),
            canonical_path_sha256: execution_path_sha256(
                ExecutionHostPlatform::Macos,
                feature.to_str().unwrap(),
            )
            .unwrap(),
            canonical_parent_sha256: object_identity_sha256(feature.parent().unwrap()),
            expected_object_sha256: Some(object_identity_sha256(feature)),
            expected_single_link: true,
        }],
        operation_sha256: operation.sha256().unwrap(),
        working_directory_sha256: execution_path_sha256(
            ExecutionHostPlatform::Macos,
            feature.to_str().unwrap(),
        )
        .unwrap(),
        environment_keys: Vec::new(),
        effect_classification: ExecutionEffectClassification::LocalReversible,
        deadline_ms: now_ms() + 60_000,
        cancellation_behavior: ExecutionCancellationBehavior::ImmediateTerminate,
        reconciliation_strategy: ExecutionReconciliationStrategy::NoEffectRetry,
        issued_at_ms: now_ms() - 1_000,
        nonce: Uuid::new_v4(),
        signer_key_id: "master-action-v1".into(),
        signature: Vec::new(),
    };
    envelope.sign(authority).unwrap();
    envelope
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn authority_snapshot(
    authority: &SigningKey,
    session_id: Uuid,
    child_epoch_id: Uuid,
    authority_revision: u64,
    emergency_paused: bool,
    revoked: bool,
) -> ExecutorAuthoritySnapshot {
    let mut snapshot = ExecutorAuthoritySnapshot {
        authority_revision,
        session_id,
        session_revision: 1,
        child_epoch_id,
        child_epoch_revision: 1,
        feature_lifecycle_revision: 1,
        emergency_paused,
        revoked,
        signer_key_id: "master-action-v1".into(),
        signature: Vec::new(),
    };
    snapshot.sign(authority).unwrap();
    snapshot
}

fn object_identity_sha256(path: &std::path::Path) -> [u8; 32] {
    let canonical = path.canonicalize().unwrap();
    let metadata = fs::symlink_metadata(&canonical).unwrap();
    let mut hasher = Sha256::new();
    hasher.update(canonical.to_str().unwrap().as_bytes());
    hasher.update([0]);
    hasher.update(metadata.dev().to_le_bytes());
    hasher.update(metadata.ino().to_le_bytes());
    hasher.finalize().into()
}
