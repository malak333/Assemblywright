use assemblywright_master::execution_ipc::{
    InertWindowsExecutionIpcFoundation, WindowsExecutionIpcBinding,
};
use assemblywright_protocol::{
    WindowsExecutionAck, WindowsExecutionAckStatus, WindowsExecutionIpcEndpoint,
    WINDOWS_EXECUTION_IPC_SCHEMA_VERSION,
};
use ed25519_dalek::SigningKey;
use uuid::Uuid;

fn foundation() -> (InertWindowsExecutionIpcFoundation, SigningKey, SigningKey) {
    let authority = SigningKey::from_bytes(&[51; 32]);
    let broker = SigningKey::from_bytes(&[52; 32]);
    let executor = SigningKey::from_bytes(&[53; 32]);
    let binding = WindowsExecutionIpcBinding {
        master_id: Uuid::from_u128(1),
        broker_id: Uuid::from_u128(2),
        executor_id: Uuid::from_u128(3),
        session_id: Uuid::from_u128(4),
        session_revision: 5,
        child_epoch_id: Uuid::from_u128(6),
        child_epoch_revision: 7,
        feature_lifecycle_revision: 8,
        authority_revision: 9,
        authority_key_id: "master-ipc-v1".into(),
        broker_ack_key_id: "broker-ipc-v1".into(),
        broker_ack_key: broker.verifying_key(),
        executor_ack_key_id: "executor-ipc-v1".into(),
        executor_ack_key: executor.verifying_key(),
    };
    (
        InertWindowsExecutionIpcFoundation::new(binding, authority).unwrap(),
        broker,
        executor,
    )
}

fn ack(
    frame: &assemblywright_protocol::WindowsExecutionControlFrame,
    key_id: &str,
    key: &SigningKey,
) -> WindowsExecutionAck {
    let mut ack = WindowsExecutionAck {
        schema_version: WINDOWS_EXECUTION_IPC_SCHEMA_VERSION,
        endpoint: frame.endpoint,
        ack_id: Uuid::new_v4(),
        frame_id: frame.frame_id,
        request_sequence: frame.request_sequence,
        authority_revision: frame.authority_revision,
        frame_sha256: frame.canonical_sha256().unwrap(),
        status: match frame.kind {
            assemblywright_protocol::WindowsExecutionControlKind::Health => {
                WindowsExecutionAckStatus::HealthyEffectDisabled
            }
            assemblywright_protocol::WindowsExecutionControlKind::ValidateDispatch => {
                WindowsExecutionAckStatus::DispatchValidatedEffectDisabled
            }
        },
        effects_applied: 0,
        signer_key_id: key_id.into(),
        signature: Vec::new(),
    };
    ack.sign(key).unwrap();
    ack
}

#[test]
fn signs_independent_hops_and_verifies_only_pinned_service_acks() {
    let (foundation, broker_key, executor_key) = foundation();
    let (broker, executor) = foundation
        .sign_dispatch_validation(11, 21, 10_000, 20_000)
        .unwrap();
    assert_eq!(broker.forwarded_executor().unwrap(), Some(executor.clone()));
    let broker_ack = ack(&broker, "broker-ipc-v1", &broker_key);
    let executor_ack = ack(&executor, "executor-ipc-v1", &executor_key);
    foundation.verify_ack(&broker, &broker_ack).unwrap();
    foundation.verify_ack(&executor, &executor_ack).unwrap();
    assert!(foundation.verify_ack(&broker, &executor_ack).is_err());

    let wrong = SigningKey::from_bytes(&[54; 32]);
    let forged = ack(&broker, "broker-ipc-v1", &wrong);
    assert!(foundation.verify_ack(&broker, &forged).is_err());
}

#[test]
fn direct_health_remains_inert_and_endpoint_bound() {
    let (foundation, broker_key, _) = foundation();
    let broker = foundation
        .sign_health(WindowsExecutionIpcEndpoint::MasterToBroker, 1, 1_000, 2_000)
        .unwrap();
    let ack = ack(&broker, "broker-ipc-v1", &broker_key);
    foundation.verify_ack(&broker, &ack).unwrap();
    assert_eq!(ack.effects_applied, 0);
}
