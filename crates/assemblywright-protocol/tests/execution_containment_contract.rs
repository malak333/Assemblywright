use assemblywright_protocol::*;
use ed25519_dalek::SigningKey;
use serde_json::json;
use uuid::Uuid;

fn id(value: &str) -> Uuid {
    Uuid::parse_str(value).unwrap()
}

fn signed_envelope() -> (ExecutionActionEnvelope, SigningKey) {
    let key = SigningKey::from_bytes(&[7; 32]);
    let path = "/private/tmp/assemblywright-action";
    let mut envelope = ExecutionActionEnvelope {
        schema_version: EXECUTION_ACTION_ENVELOPE_SCHEMA_VERSION,
        action_id: id("10000000-0000-4000-8000-000000000001"),
        action_sequence: 1,
        feature_id: id("10000000-0000-4000-8000-000000000002"),
        repository_id: id("10000000-0000-4000-8000-000000000003"),
        session_id: id("10000000-0000-4000-8000-000000000004"),
        session_revision: 1,
        child_epoch_id: id("10000000-0000-4000-8000-000000000005"),
        child_epoch_revision: 1,
        feature_lifecycle_revision: 1,
        authority_revision: 1,
        executor_id: id("10000000-0000-4000-8000-000000000006"),
        executor_revision: 1,
        executor_executable_sha256: [1; 32],
        broker_id: id("10000000-0000-4000-8000-000000000007"),
        broker_revision: 1,
        broker_executable_sha256: [2; 32],
        protected_control_plane_sha256: [6; 32],
        host_platform: ExecutionHostPlatform::Macos,
        action_type: ExecutionActionType::CreateDirectory,
        targets: vec![ExecutionTargetIdentity {
            platform: ExecutionHostPlatform::Macos,
            canonical_path: path.into(),
            canonical_path_sha256: execution_path_sha256(ExecutionHostPlatform::Macos, path)
                .unwrap(),
            canonical_parent_sha256: [3; 32],
            expected_object_sha256: None,
            expected_single_link: true,
        }],
        operation_sha256: [4; 32],
        working_directory_sha256: [5; 32],
        environment_keys: vec!["PATH".into(), "SYSTEMROOT".into()],
        effect_classification: ExecutionEffectClassification::LocalDurable,
        deadline_ms: 2_000,
        cancellation_behavior: ExecutionCancellationBehavior::CheckpointThenTerminate,
        reconciliation_strategy: ExecutionReconciliationStrategy::ExactPostStateDigest,
        issued_at_ms: 1_000,
        nonce: id("10000000-0000-4000-8000-000000000008"),
        signer_key_id: "windows-master-action-v1".into(),
        signature: Vec::new(),
    };
    envelope.sign(&key).unwrap();
    (envelope, key)
}

#[test]
fn signed_action_is_strict_digest_bound_and_domain_verified() {
    let (envelope, key) = signed_envelope();
    envelope.verify_signature(&key.verifying_key()).unwrap();
    assert_eq!(
        ExecutionActionEnvelope::decode_frame(&serde_json::to_vec(&envelope).unwrap()).unwrap(),
        envelope
    );

    let mut tampered = envelope.clone();
    tampered.deadline_ms += 1;
    assert!(tampered.verify_signature(&key.verifying_key()).is_err());

    let mut stale_authority = envelope.clone();
    stale_authority.authority_revision += 1;
    assert!(stale_authority
        .verify_signature(&key.verifying_key())
        .is_err());

    let mut unknown = serde_json::to_value(&envelope).unwrap();
    unknown["credential"] = json!("forbidden");
    assert!(ExecutionActionEnvelope::decode_frame(&serde_json::to_vec(&unknown).unwrap()).is_err());

    let duplicate =
        serde_json::to_string(&envelope)
            .unwrap()
            .replacen("{", "{\"schema_version\":1,", 1);
    assert!(ExecutionActionEnvelope::decode_frame(duplicate.as_bytes()).is_err());
}

#[test]
fn action_rejects_path_case_schema_identity_and_environment_ambiguity() {
    let (envelope, _) = signed_envelope();
    let mut invalid = envelope.clone();
    invalid.signature.clear();
    invalid.targets[0].canonical_path_sha256 = [9; 32];
    assert!(invalid.validate().is_err());

    invalid = envelope.clone();
    invalid.signature.clear();
    invalid.environment_keys.push("PATH".into());
    assert!(invalid.validate().is_err());

    invalid = envelope.clone();
    invalid.signature.clear();
    invalid.schema_version = FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION;
    assert!(invalid.validate().is_err());

    invalid = envelope;
    invalid.signature.clear();
    invalid.targets[0].canonical_path = "/private/tmp/../master".into();
    assert!(invalid.validate().is_err());
}

#[test]
fn checkpoint_and_termination_receipts_are_signed_and_internally_consistent() {
    let key = SigningKey::from_bytes(&[8; 32]);
    let mut activation = ExecutionActivationReceipt {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        receipt_id: id("20000000-0000-4000-8000-000000000010"),
        session_id: id("20000000-0000-4000-8000-000000000011"),
        child_epoch_id: id("20000000-0000-4000-8000-000000000002"),
        authority_revision: 3,
        host_platform: ExecutionHostPlatform::Macos,
        executor_id: id("20000000-0000-4000-8000-000000000012"),
        executor_revision: 2,
        observed_at_ms: 1,
        signer_key_id: "executor-receipt-v1".into(),
        signature: Vec::new(),
    };
    activation.sign(&key).unwrap();
    activation.verify_signature(&key.verifying_key()).unwrap();
    assert_eq!(
        ExecutionActivationReceipt::decode_frame(&serde_json::to_vec(&activation).unwrap())
            .unwrap(),
        activation
    );
    let mut drifted_activation = activation;
    drifted_activation.authority_revision += 1;
    assert!(drifted_activation
        .verify_signature(&key.verifying_key())
        .is_err());

    let mut checkpoint = ExecutionCheckpointReceipt {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        action_id: id("20000000-0000-4000-8000-000000000001"),
        action_sequence: 1,
        child_epoch_id: id("20000000-0000-4000-8000-000000000002"),
        phase: ExecutionCheckpointPhase::BeforeEffect,
        checkpoint_sha256: [1; 32],
        result_sha256: None,
        observed_at_ms: 1,
        signer_key_id: "executor-receipt-v1".into(),
        signature: Vec::new(),
    };
    checkpoint.sign(&key).unwrap();
    checkpoint.verify_signature(&key.verifying_key()).unwrap();
    assert_eq!(
        ExecutionCheckpointReceipt::decode_frame(&serde_json::to_vec(&checkpoint).unwrap())
            .unwrap(),
        checkpoint
    );
    let mut phase_drift = checkpoint;
    phase_drift.phase = ExecutionCheckpointPhase::AfterEffect;
    assert!(phase_drift.verify_signature(&key.verifying_key()).is_err());

    let mut termination = ExecutionTerminationReceipt {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        receipt_id: id("20000000-0000-4000-8000-000000000003"),
        child_epoch_id: id("20000000-0000-4000-8000-000000000002"),
        mode: ExecutionTerminationMode::EmergencyPause,
        outcome: ExecutionTerminationOutcome::Reaped,
        tracked_root_process_count: 1,
        graceful_root_termination_count: 0,
        forced_root_termination_count: 1,
        reaped_root_process_count: 1,
        survivor_root_process_count: 0,
        descendant_scope: ExecutionDescendantScope::MacosProcessGroup,
        descendants_reaped: true,
        last_checkpoint_sha256: [2; 32],
        observed_at_ms: 2,
        signer_key_id: "executor-receipt-v1".into(),
        signature: Vec::new(),
    };
    termination.sign(&key).unwrap();
    termination.verify_signature(&key.verifying_key()).unwrap();
    assert_eq!(
        ExecutionTerminationReceipt::decode_frame(&serde_json::to_vec(&termination).unwrap())
            .unwrap(),
        termination
    );
    termination.survivor_root_process_count = 1;
    assert!(termination.validate().is_err());
}

#[test]
fn windows_target_identity_rejects_nt_alias_and_reserved_name_ambiguity() {
    for path in [
        r"c:\data\file",
        r"C:\data\file.",
        r"C:\data\file ",
        r"C:\data\CON",
        r"C:\data\LPT1.txt",
        r"C:\data\file:stream",
        r"C:\data\..\master",
        r"C:\data\😀",
    ] {
        assert!(
            execution_path_sha256(ExecutionHostPlatform::Windows, path).is_err(),
            "accepted {path}"
        );
    }
    execution_path_sha256(ExecutionHostPlatform::Windows, r"C:\data\ordinary.txt").unwrap();
}
