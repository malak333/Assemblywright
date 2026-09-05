use assemblywright_executor::ipc::{ExecutorIpcError, InertExecutorIpc};
use assemblywright_protocol::{
    WindowsExecutionAckStatus, WindowsExecutionControlFrame, WindowsExecutionControlKind,
    WindowsExecutionIpcEndpoint, WINDOWS_EXECUTION_IPC_SCHEMA_VERSION,
};
use ed25519_dalek::SigningKey;
use uuid::Uuid;

fn frame(service_id: Uuid, sequence: u64, key: &SigningKey) -> WindowsExecutionControlFrame {
    let mut value = WindowsExecutionControlFrame {
        schema_version: WINDOWS_EXECUTION_IPC_SCHEMA_VERSION,
        endpoint: WindowsExecutionIpcEndpoint::MasterToExecutor,
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
        kind: WindowsExecutionControlKind::ValidateDispatch,
        forwarded_executor_frame_sha256: [0; 32],
        forwarded_executor_frame: Vec::new(),
        signer_key_id: "master-ipc-v1".into(),
        signature: Vec::new(),
    };
    value.sign(key).unwrap();
    value
}

#[test]
fn exact_pending_restart_recovers_to_same_zero_effect_ack() {
    let temp = tempfile::tempdir().unwrap();
    let master = SigningKey::from_bytes(&[71; 32]);
    let receipt = SigningKey::from_bytes(&[72; 32]);
    let executor_id = Uuid::from_u128(10);
    let path = temp.path().join("executor.journal");
    let request = frame(executor_id, 1, &master);
    let bytes = request.encode_frame().unwrap();
    let mut ipc = InertExecutorIpc::open(
        &path,
        executor_id,
        4,
        1,
        "master-ipc-v1".into(),
        master.verifying_key(),
        "executor-ack-v1".into(),
        &receipt.to_bytes(),
    )
    .unwrap();
    let ack = ipc.handle(&bytes, 2_000).unwrap();
    assert_eq!(
        ack.status,
        WindowsExecutionAckStatus::DispatchValidatedEffectDisabled
    );
    assert_eq!(ack.effects_applied, 0);
    drop(ipc);
    let mut restarted = InertExecutorIpc::open(
        &path,
        executor_id,
        4,
        99,
        "master-ipc-v1".into(),
        master.verifying_key(),
        "executor-ack-v1".into(),
        &receipt.to_bytes(),
    )
    .unwrap();
    let replay = restarted.handle(&bytes, 20_000).unwrap();
    assert_eq!(replay, ack);
}

#[test]
fn wrong_endpoint_sequence_gap_and_stale_authority_fail_closed() {
    for mutation in 0..3 {
        let temp = tempfile::tempdir().unwrap();
        let master = SigningKey::from_bytes(&[73; 32]);
        let executor_id = Uuid::from_u128(20 + mutation);
        let mut request = frame(executor_id, 1, &master);
        match mutation {
            0 => {
                request.endpoint = WindowsExecutionIpcEndpoint::MasterToBroker;
                request.kind = WindowsExecutionControlKind::Health;
            }
            1 => request.request_sequence = 2,
            _ => request.authority_revision = 5,
        }
        request.signature.clear();
        request.sign(&master).unwrap();
        let mut ipc = InertExecutorIpc::open(
            temp.path().join("executor.journal"),
            executor_id,
            4,
            1,
            "master-ipc-v1".into(),
            master.verifying_key(),
            "executor-ack-v1".into(),
            &[74; 32],
        )
        .unwrap();
        assert!(matches!(
            ipc.handle(&request.encode_frame().unwrap(), 2_000),
            Err(ExecutorIpcError::Rejected)
                | Err(ExecutorIpcError::Durable(
                    assemblywright_protocol::execution_ipc_state::DurableIpcError::Quarantined
                ))
        ));
    }
}
