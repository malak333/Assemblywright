use assemblywright_broker::ipc::{BrokerIpcError, InertBrokerIpc};
use assemblywright_protocol::{
    WindowsExecutionAckStatus, WindowsExecutionControlFrame, WindowsExecutionControlKind,
    WindowsExecutionIpcEndpoint, WINDOWS_EXECUTION_IPC_SCHEMA_VERSION,
};
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn frame(
    endpoint: WindowsExecutionIpcEndpoint,
    service_id: Uuid,
    sequence: u64,
    key: &SigningKey,
) -> WindowsExecutionControlFrame {
    let mut value = WindowsExecutionControlFrame {
        schema_version: WINDOWS_EXECUTION_IPC_SCHEMA_VERSION,
        endpoint,
        frame_id: Uuid::new_v4(),
        request_sequence: sequence,
        nonce: Uuid::new_v4(),
        master_id: Uuid::from_u128(1),
        service_id,
        session_id: Uuid::from_u128(2),
        session_revision: 1,
        child_epoch_id: Uuid::from_u128(3),
        child_epoch_revision: 1,
        feature_lifecycle_revision: 1,
        authority_revision: 4,
        issued_at_ms: 1_000,
        expires_at_ms: 10_000,
        kind: if endpoint == WindowsExecutionIpcEndpoint::MasterToExecutor {
            WindowsExecutionControlKind::ValidateDispatch
        } else {
            WindowsExecutionControlKind::Health
        },
        forwarded_executor_frame_sha256: [0; 32],
        forwarded_executor_frame: Vec::new(),
        signer_key_id: "master-ipc-v1".into(),
        signature: Vec::new(),
    };
    value.sign(key).unwrap();
    value
}

#[test]
fn validates_and_forwards_exact_master_signed_executor_bytes_without_effect() {
    let temp = tempfile::tempdir().unwrap();
    let master = SigningKey::from_bytes(&[61; 32]);
    let broker_ack = SigningKey::from_bytes(&[62; 32]);
    let broker_id = Uuid::from_u128(10);
    let executor = frame(
        WindowsExecutionIpcEndpoint::MasterToExecutor,
        Uuid::from_u128(11),
        1,
        &master,
    );
    let executor_bytes = executor.encode_frame().unwrap();
    let mut broker = frame(
        WindowsExecutionIpcEndpoint::MasterToBroker,
        broker_id,
        1,
        &master,
    );
    broker.kind = WindowsExecutionControlKind::ValidateDispatch;
    broker.forwarded_executor_frame_sha256 = Sha256::digest(&executor_bytes).into();
    broker.forwarded_executor_frame = executor_bytes.clone();
    broker.signature.clear();
    broker.sign(&master).unwrap();
    let state_path = temp.path().join("broker.journal");
    let mut ipc = InertBrokerIpc::open(
        &state_path,
        broker_id,
        4,
        1,
        "master-ipc-v1".into(),
        master.verifying_key(),
        "broker-ack-v1".into(),
        &broker_ack.to_bytes(),
    )
    .unwrap();
    let accepted = ipc.handle(&broker.encode_frame().unwrap(), 2_000).unwrap();
    assert_eq!(
        accepted.forwarded_executor_frame,
        Some(executor_bytes.clone())
    );
    assert_eq!(
        accepted.ack.status,
        WindowsExecutionAckStatus::DispatchValidatedEffectDisabled
    );
    assert_eq!(accepted.ack.effects_applied, 0);
    accepted
        .ack
        .verify_for(&broker, "broker-ack-v1", &broker_ack.verifying_key())
        .unwrap();
    drop(ipc);
    let mut restarted = InertBrokerIpc::open(
        &state_path,
        broker_id,
        4,
        99,
        "master-ipc-v1".into(),
        master.verifying_key(),
        "broker-ack-v1".into(),
        &broker_ack.to_bytes(),
    )
    .unwrap();
    let replay = restarted
        .handle(&broker.encode_frame().unwrap(), 20_000)
        .unwrap();
    assert_eq!(replay.ack, accepted.ack);
    assert_eq!(replay.forwarded_executor_frame, Some(executor_bytes));
}

#[test]
fn tamper_then_valid_request_stays_durably_quarantined() {
    let temp = tempfile::tempdir().unwrap();
    let master = SigningKey::from_bytes(&[63; 32]);
    let broker_id = Uuid::from_u128(20);
    let path = temp.path().join("broker.journal");
    let mut ipc = InertBrokerIpc::open(
        &path,
        broker_id,
        4,
        1,
        "master-ipc-v1".into(),
        master.verifying_key(),
        "broker-ack-v1".into(),
        &[64; 32],
    )
    .unwrap();
    let mut request = frame(
        WindowsExecutionIpcEndpoint::MasterToBroker,
        broker_id,
        1,
        &master,
    );
    request.kind = WindowsExecutionControlKind::Health;
    request.signature.clear();
    request.sign(&master).unwrap();
    let valid = request.encode_frame().unwrap();
    let mut tampered = valid.clone();
    *tampered.last_mut().unwrap() ^= 1;
    assert!(matches!(
        ipc.handle(&tampered, 2_000),
        Err(BrokerIpcError::Rejected)
    ));
    assert!(ipc.handle(&valid, 2_000).is_err());
    drop(ipc);
    assert!(InertBrokerIpc::open(
        &path,
        broker_id,
        4,
        1,
        "master-ipc-v1".into(),
        master.verifying_key(),
        "broker-ack-v1".into(),
        &[64; 32],
    )
    .is_err());
}
