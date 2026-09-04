use assemblywright_protocol::execution_ipc_state::{
    DurableIpcAdmission, DurableIpcError, DurableIpcLedger,
};
use assemblywright_protocol::{
    WindowsExecutionAck, WindowsExecutionAckStatus, WindowsExecutionControlFrame,
    WindowsExecutionControlKind, WindowsExecutionIpcEndpoint, WINDOWS_EXECUTION_IPC_SCHEMA_VERSION,
};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn ack_for(frame: &WindowsExecutionControlFrame, key: &SigningKey) -> WindowsExecutionAck {
    let mut ack = WindowsExecutionAck {
        schema_version: WINDOWS_EXECUTION_IPC_SCHEMA_VERSION,
        endpoint: frame.endpoint,
        ack_id: Uuid::new_v4(),
        frame_id: frame.frame_id,
        request_sequence: frame.request_sequence,
        authority_revision: frame.authority_revision,
        frame_sha256: frame.canonical_sha256().unwrap(),
        status: WindowsExecutionAckStatus::HealthyEffectDisabled,
        effects_applied: 0,
        signer_key_id: "service-ack-v1".into(),
        signature: Vec::new(),
    };
    ack.sign(key).unwrap();
    ack
}

fn frame(
    endpoint: WindowsExecutionIpcEndpoint,
    service_id: Uuid,
    sequence: u64,
    key: &SigningKey,
) -> WindowsExecutionControlFrame {
    let mut frame = WindowsExecutionControlFrame {
        schema_version: WINDOWS_EXECUTION_IPC_SCHEMA_VERSION,
        endpoint,
        frame_id: Uuid::new_v4(),
        request_sequence: sequence,
        nonce: Uuid::new_v4(),
        master_id: Uuid::from_u128(1),
        service_id,
        session_id: Uuid::from_u128(2),
        session_revision: 3,
        child_epoch_id: Uuid::from_u128(4),
        child_epoch_revision: 5,
        feature_lifecycle_revision: 6,
        authority_revision: 7,
        issued_at_ms: 1_000,
        expires_at_ms: 31_000,
        kind: WindowsExecutionControlKind::Health,
        forwarded_executor_frame_sha256: [0; 32],
        forwarded_executor_frame: Vec::new(),
        signer_key_id: "master-ipc-v1".into(),
        signature: Vec::new(),
    };
    frame.sign(key).unwrap();
    frame
}

#[test]
fn independent_hop_signatures_and_path_free_ack_round_trip() {
    let master = SigningKey::from_bytes(&[11; 32]);
    let executor_id = Uuid::from_u128(20);
    let mut executor = frame(
        WindowsExecutionIpcEndpoint::MasterToExecutor,
        executor_id,
        91,
        &master,
    );
    executor.kind = WindowsExecutionControlKind::ValidateDispatch;
    executor.signature.clear();
    executor.sign(&master).unwrap();
    let executor_bytes = executor.encode_frame().unwrap();

    let mut broker = frame(
        WindowsExecutionIpcEndpoint::MasterToBroker,
        Uuid::from_u128(21),
        40,
        &master,
    );
    broker.kind = WindowsExecutionControlKind::ValidateDispatch;
    broker.forwarded_executor_frame_sha256 = Sha256::digest(&executor_bytes).into();
    broker.forwarded_executor_frame = executor_bytes.clone();
    broker.signature.clear();
    broker.sign(&master).unwrap();
    broker.verify_signature(&master.verifying_key()).unwrap();
    let forwarded = broker.forwarded_executor().unwrap().unwrap();
    assert_eq!(forwarded.encode_frame().unwrap(), executor_bytes);
    forwarded.verify_signature(&master.verifying_key()).unwrap();

    let broker_secret = SigningKey::from_bytes(&[12; 32]);
    let mut ack = WindowsExecutionAck {
        schema_version: WINDOWS_EXECUTION_IPC_SCHEMA_VERSION,
        endpoint: WindowsExecutionIpcEndpoint::MasterToBroker,
        ack_id: Uuid::new_v4(),
        frame_id: broker.frame_id,
        request_sequence: broker.request_sequence,
        authority_revision: broker.authority_revision,
        frame_sha256: broker.canonical_sha256().unwrap(),
        status: WindowsExecutionAckStatus::DispatchValidatedEffectDisabled,
        effects_applied: 0,
        signer_key_id: "broker-ack-v1".into(),
        signature: Vec::new(),
    };
    ack.sign(&broker_secret).unwrap();
    let bytes = ack.encode_frame().unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("path"));
    WindowsExecutionAck::decode_frame(&bytes)
        .unwrap()
        .verify_for(&broker, "broker-ack-v1", &broker_secret.verifying_key())
        .unwrap();

    let mut semantically_wrong = ack;
    semantically_wrong.status = WindowsExecutionAckStatus::HealthyEffectDisabled;
    semantically_wrong.signature.clear();
    semantically_wrong.sign(&broker_secret).unwrap();
    assert!(semantically_wrong
        .verify_for(&broker, "broker-ack-v1", &broker_secret.verifying_key())
        .is_err());
}

#[test]
fn tamper_wrong_hop_stale_and_nested_binding_drift_reject() {
    let master = SigningKey::from_bytes(&[13; 32]);
    let wrong = SigningKey::from_bytes(&[14; 32]);
    let executor_id = Uuid::from_u128(30);
    let mut executor = frame(
        WindowsExecutionIpcEndpoint::MasterToExecutor,
        executor_id,
        2,
        &master,
    );
    executor.kind = WindowsExecutionControlKind::ValidateDispatch;
    executor.signature.clear();
    executor.sign(&master).unwrap();
    assert!(executor.verify_signature(&wrong.verifying_key()).is_err());
    assert!(executor.validate_at(31_000).is_err());

    let executor_bytes = executor.encode_frame().unwrap();
    let mut broker = frame(
        WindowsExecutionIpcEndpoint::MasterToBroker,
        Uuid::from_u128(31),
        4,
        &master,
    );
    broker.kind = WindowsExecutionControlKind::ValidateDispatch;
    broker.forwarded_executor_frame_sha256 = Sha256::digest(&executor_bytes).into();
    broker.forwarded_executor_frame = executor_bytes;
    broker.signature.clear();
    broker.sign(&master).unwrap();

    let mut tampered = broker.clone();
    tampered.authority_revision += 1;
    assert!(tampered.verify_signature(&master.verifying_key()).is_err());

    let mut inner: WindowsExecutionControlFrame =
        WindowsExecutionControlFrame::decode_frame(&broker.forwarded_executor_frame).unwrap();
    inner.session_revision += 1;
    inner.signature.clear();
    inner.sign(&master).unwrap();
    let inner = inner.encode_frame().unwrap();
    let mut mismatched = broker;
    mismatched.forwarded_executor_frame_sha256 = Sha256::digest(&inner).into();
    mismatched.forwarded_executor_frame = inner;
    mismatched.signature.clear();
    mismatched.sign(&master).unwrap();
    assert!(mismatched.forwarded_executor().is_err());
}

#[test]
fn unsigned_duplicate_unknown_and_oversized_frames_reject() {
    let master = SigningKey::from_bytes(&[15; 32]);
    let signed = frame(
        WindowsExecutionIpcEndpoint::MasterToBroker,
        Uuid::from_u128(40),
        1,
        &master,
    );
    let mut unsigned = signed.clone();
    unsigned.signature.clear();
    assert!(unsigned.encode_frame().is_err());

    let encoded = String::from_utf8(signed.encode_frame().unwrap()).unwrap();
    let duplicate = encoded.replacen(
        "\"schema_version\":1",
        "\"schema_version\":1,\"schema_version\":1",
        1,
    );
    assert!(WindowsExecutionControlFrame::decode_frame(duplicate.as_bytes()).is_err());
    let unknown = encoded.replacen("{", "{\"secret\":\"forbidden\",", 1);
    assert!(WindowsExecutionControlFrame::decode_frame(unknown.as_bytes()).is_err());
    let noncanonical = format!(" {encoded}");
    assert!(WindowsExecutionControlFrame::decode_frame(noncanonical.as_bytes()).is_err());
    assert!(WindowsExecutionControlFrame::decode_frame(&vec![b'x'; 96 * 1024 + 1]).is_err());
}

#[test]
fn durable_intent_ack_replay_and_exact_pending_restart_recover() {
    let temp = tempfile::tempdir().unwrap();
    let master = SigningKey::from_bytes(&[16; 32]);
    let service = SigningKey::from_bytes(&[17; 32]);
    let service_id = Uuid::from_u128(50);
    let path = temp.path().join("ipc.journal");
    let first = frame(
        WindowsExecutionIpcEndpoint::MasterToBroker,
        service_id,
        1,
        &master,
    );
    let mut ledger = DurableIpcLedger::open(
        &path,
        WindowsExecutionIpcEndpoint::MasterToBroker,
        service_id,
        7,
        1,
    )
    .unwrap();
    assert_eq!(ledger.admit(&first).unwrap(), DurableIpcAdmission::New);
    drop(ledger);

    let mut recovered = DurableIpcLedger::open(
        &path,
        WindowsExecutionIpcEndpoint::MasterToBroker,
        service_id,
        7,
        1,
    )
    .unwrap();
    assert_eq!(
        recovered.admit(&first).unwrap(),
        DurableIpcAdmission::RecoverPending
    );
    let ack = ack_for(&first, &service);
    recovered.complete(ack.clone()).unwrap();
    drop(recovered);

    let mut replay = DurableIpcLedger::open(
        &path,
        WindowsExecutionIpcEndpoint::MasterToBroker,
        service_id,
        7,
        99,
    )
    .unwrap();
    assert_eq!(
        replay.admit(&first).unwrap(),
        DurableIpcAdmission::Replay(ack)
    );
}

#[test]
fn durable_gap_replay_drift_and_partial_restart_quarantine() {
    let temp = tempfile::tempdir().unwrap();
    let master = SigningKey::from_bytes(&[18; 32]);
    let service_id = Uuid::from_u128(60);
    let path = temp.path().join("gap.journal");
    let mut gap = DurableIpcLedger::open(
        &path,
        WindowsExecutionIpcEndpoint::MasterToExecutor,
        service_id,
        7,
        2,
    )
    .unwrap();
    let gap_frame = frame(
        WindowsExecutionIpcEndpoint::MasterToExecutor,
        service_id,
        3,
        &master,
    );
    assert!(matches!(
        gap.admit(&gap_frame),
        Err(DurableIpcError::Quarantined)
    ));
    drop(gap);
    assert!(matches!(
        DurableIpcLedger::open(
            &path,
            WindowsExecutionIpcEndpoint::MasterToExecutor,
            service_id,
            7,
            2,
        ),
        Err(DurableIpcError::Quarantined)
    ));

    let partial = temp.path().join("partial.journal");
    std::fs::write(&partial, b"{\"schema_version\":1").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&partial, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    assert!(matches!(
        DurableIpcLedger::open(
            &partial,
            WindowsExecutionIpcEndpoint::MasterToExecutor,
            service_id,
            7,
            1,
        ),
        Err(DurableIpcError::InvalidState)
    ));
}
