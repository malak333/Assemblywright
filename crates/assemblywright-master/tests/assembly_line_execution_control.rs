use assemblywright_master::{
    AssemblyLineExecutionCapabilityBinding, AssemblyLineExecutionRuntimeStatus, MasterError,
    MasterKernel, MasterProcess, MASTER_SCHEMA_VERSION,
};
use assemblywright_protocol::{
    AssemblyLineEmergencyPauseRequest, AssemblyLineLifecycleState, AssemblyLineStartRequest,
    AssemblyLineStopRequest, ExecutionActivationReceipt, ExecutionCheckpointPhase,
    ExecutionCheckpointReceipt, ExecutionDescendantScope, ExecutionHostPlatform,
    ExecutionTerminationMode, ExecutionTerminationOutcome, ExecutionTerminationReceipt,
    FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
};
use rusqlite::{params, Connection};
use tempfile::TempDir;
use uuid::Uuid;

const WINDOWS_SIGNING_SEED: [u8; 32] = [7; 32];
const MAC_SIGNING_SEED: [u8; 32] = [8; 32];
const FORGED_SIGNING_SEED: [u8; 32] = [9; 32];

fn receipt_verifying_key(seed: [u8; 32]) -> [u8; 32] {
    let signing_key = seed.into();
    let mut receipt = ExecutionTerminationReceipt {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        receipt_id: Uuid::new_v4(),
        child_epoch_id: Uuid::new_v4(),
        mode: ExecutionTerminationMode::Stop,
        outcome: ExecutionTerminationOutcome::Reaped,
        tracked_root_process_count: 1,
        graceful_root_termination_count: 1,
        forced_root_termination_count: 0,
        reaped_root_process_count: 1,
        survivor_root_process_count: 0,
        descendant_scope: ExecutionDescendantScope::WindowsJobObject,
        descendants_reaped: true,
        last_checkpoint_sha256: [1; 32],
        observed_at_ms: 1,
        signer_key_id: "key".to_string(),
        signature: Vec::new(),
    };
    receipt.sign(&signing_key).unwrap();
    signing_key.verifying_key().to_bytes()
}

fn sign_termination(receipt: &mut ExecutionTerminationReceipt, seed: [u8; 32]) {
    let signing_key = seed.into();
    receipt.signature.clear();
    receipt.sign(&signing_key).unwrap();
}

fn sign_checkpoint(receipt: &mut ExecutionCheckpointReceipt, seed: [u8; 32]) {
    let signing_key = seed.into();
    receipt.signature.clear();
    receipt.sign(&signing_key).unwrap();
}

fn sign_activation(receipt: &mut ExecutionActivationReceipt, seed: [u8; 32]) {
    let signing_key = seed.into();
    receipt.signature.clear();
    receipt.sign(&signing_key).unwrap();
}

fn termination_receipt(
    child_epoch_id: Uuid,
    checkpoint_sha256: [u8; 32],
    scope: ExecutionDescendantScope,
    signer_key_id: &str,
    observed_at_ms: u64,
) -> ExecutionTerminationReceipt {
    ExecutionTerminationReceipt {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        receipt_id: Uuid::new_v4(),
        child_epoch_id,
        mode: ExecutionTerminationMode::Stop,
        outcome: ExecutionTerminationOutcome::Reaped,
        tracked_root_process_count: 1,
        graceful_root_termination_count: 1,
        forced_root_termination_count: 0,
        reaped_root_process_count: 1,
        survivor_root_process_count: 0,
        descendant_scope: scope,
        descendants_reaped: true,
        last_checkpoint_sha256: checkpoint_sha256,
        observed_at_ms,
        signer_key_id: signer_key_id.to_string(),
        signature: Vec::new(),
    }
}

#[derive(Clone)]
struct Fixture {
    windows_executor_id: Uuid,
    mac_executor_id: Uuid,
    windows_broker_id: Uuid,
    mac_broker_id: Uuid,
}

impl Fixture {
    fn new() -> Self {
        Self {
            windows_executor_id: Uuid::new_v4(),
            mac_executor_id: Uuid::new_v4(),
            windows_broker_id: Uuid::new_v4(),
            mac_broker_id: Uuid::new_v4(),
        }
    }

    fn capability(
        &self,
        state_revision: u64,
        emergency_pause_revision: u64,
        healthy: bool,
    ) -> AssemblyLineExecutionCapabilityBinding {
        AssemblyLineExecutionCapabilityBinding {
            binding_revision: 1,
            expected_state_revision: state_revision,
            expected_emergency_pause_revision: emergency_pause_revision,
            windows_executor_id: self.windows_executor_id,
            windows_executor_revision: 1,
            windows_executor_sha256: [1; 32],
            mac_executor_id: self.mac_executor_id,
            mac_executor_revision: 1,
            mac_executor_sha256: [2; 32],
            windows_broker_id: self.windows_broker_id,
            windows_broker_revision: 1,
            windows_broker_sha256: [3; 32],
            mac_broker_id: self.mac_broker_id,
            mac_broker_revision: 1,
            mac_broker_sha256: [4; 32],
            protected_control_plane_sha256: [5; 32],
            windows_receipt_signer_key_id: "windows.executor.receipts.v1".to_string(),
            windows_receipt_verifying_key: receipt_verifying_key(WINDOWS_SIGNING_SEED),
            mac_receipt_signer_key_id: "mac.executor.receipts.v1".to_string(),
            mac_receipt_verifying_key: receipt_verifying_key(MAC_SIGNING_SEED),
            healthy,
            provisioning_evidence_sha256: [6; 32],
        }
    }

    fn start_request(
        &self,
        state_revision: u64,
        queue_revision: u64,
        pause_revision: u64,
        queue_count: u16,
        auto_run: bool,
    ) -> AssemblyLineStartRequest {
        let mut request = AssemblyLineStartRequest {
            schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
            request_id: Uuid::new_v4(),
            expected_state_revision: state_revision,
            expected_queue_revision: queue_revision,
            expected_emergency_pause_revision: pause_revision,
            queue_count,
            windows_executor_id: self.windows_executor_id,
            windows_executor_revision: 1,
            mac_executor_id: self.mac_executor_id,
            mac_executor_revision: 1,
            auto_run,
            owner_start_approval_sha256: [0; 32],
        };
        request.owner_start_approval_sha256 =
            request.canonical_owner_start_approval_sha256().unwrap();
        request
    }
}

fn seed_queue(database: &std::path::Path, count: u16) {
    let connection = Connection::open(database).unwrap();
    for position in 1..=count {
        let repository_id = Uuid::new_v4();
        let feature_id = Uuid::new_v4();
        let specification_id = Uuid::new_v4();
        connection
            .execute(
                "INSERT INTO assembly_line_repositories
                 (repository_id,git_url,repository_revision,lifecycle_revision,visibility,
                  approved_specification_id,approved_specification_revision,
                  approved_specification_sha256,owner_approval_sha256,lifecycle,effect_possible,
                  creation_evidence_sha256,created_at_ms)
                 VALUES(?1,?2,1,2,'public',?3,1,?4,?5,'created',1,?6,100)",
                params![
                    repository_id.to_string(),
                    format!("https://github.com/owner/execution-{position}"),
                    specification_id.to_string(),
                    [10 + position as u8; 32].as_slice(),
                    [20 + position as u8; 32].as_slice(),
                    [30 + position as u8; 32].as_slice(),
                ],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO assembly_line_queue
                 (feature_id,repository_id,specification_id,specification_revision,
                  specification_sha256,owner_approval_sha256,queue_position,
                  lifecycle_revision,lifecycle,enqueued_at_ms)
                 VALUES(?1,?2,?3,1,?4,?5,?6,1,'queued',100)",
                params![
                    feature_id.to_string(),
                    repository_id.to_string(),
                    specification_id.to_string(),
                    [10 + position as u8; 32].as_slice(),
                    [20 + position as u8; 32].as_slice(),
                    i64::from(position),
                ],
            )
            .unwrap();
    }
    connection
        .execute(
            "UPDATE assembly_line_state SET queue_revision=?1 WHERE singleton=1",
            [i64::from(count)],
        )
        .unwrap();
}

fn open_seeded(count: u16) -> (TempDir, std::path::PathBuf, MasterKernel, Fixture) {
    let temp = TempDir::new().unwrap();
    let database = temp.path().join("master.sqlite3");
    let kernel = MasterKernel::open(&database).unwrap();
    seed_queue(&database, count);
    (temp, database, kernel, Fixture::new())
}

fn configure_and_start(
    kernel: &mut MasterKernel,
    fixture: &Fixture,
    now_ms: u64,
) -> (
    AssemblyLineStartRequest,
    assemblywright_protocol::AssemblyLineStartReceipt,
) {
    let projection = kernel.assembly_line_owner_projection(now_ms).unwrap();
    kernel
        .record_assembly_line_execution_capabilities(
            &fixture.capability(
                projection.assembly_line.state_revision,
                projection.emergency_pause_revision,
                true,
            ),
            now_ms + 1,
        )
        .unwrap();
    let request = fixture.start_request(
        projection.assembly_line.state_revision,
        projection.assembly_line.queue_revision,
        projection.emergency_pause_revision,
        projection.assembly_line.queue_count,
        projection.assembly_line.auto_run,
    );
    let receipt = kernel.start_assembly_line(&request, now_ms + 2).unwrap();
    (request, receipt)
}

#[test]
fn start_denies_empty_queue_before_creating_authority_or_audit() {
    let (_temp, database, mut kernel, fixture) = open_seeded(0);
    let connection = Connection::open(&database).unwrap();
    connection
        .execute(
            "UPDATE assembly_line_state SET queue_revision=1 WHERE singleton=1",
            [],
        )
        .unwrap();
    drop(connection);
    let projection = kernel.assembly_line_owner_projection(10).unwrap();
    kernel
        .record_assembly_line_execution_capabilities(
            &fixture.capability(projection.assembly_line.state_revision, 0, true),
            11,
        )
        .unwrap();
    let request = fixture.start_request(
        projection.assembly_line.state_revision,
        1,
        0,
        1,
        projection.assembly_line.auto_run,
    );
    assert!(matches!(
        kernel.start_assembly_line(&request, 12),
        Err(MasterError::AssemblyLineExecutionControlUnavailable)
    ));
    let connection = Connection::open(&database).unwrap();
    let counts: (i64, i64) = connection
        .query_row(
            "SELECT
               (SELECT COUNT(*) FROM assembly_line_execution_sessions),
               (SELECT COUNT(*) FROM assembly_line_execution_requests)",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(counts, (0, 0));
}

#[test]
fn start_denies_missing_unhealthy_and_state_stale_capabilities() {
    let (_temp, _database, mut missing, fixture) = open_seeded(1);
    let projection = missing.assembly_line_owner_projection(10).unwrap();
    let request = fixture.start_request(1, 1, 0, 1, true);
    assert!(matches!(
        missing.start_assembly_line(&request, 11),
        Err(MasterError::AssemblyLineExecutionCapabilityUnavailable)
    ));

    let (_temp, _database, mut unhealthy, fixture) = open_seeded(1);
    unhealthy
        .record_assembly_line_execution_capabilities(&fixture.capability(1, 0, false), 12)
        .unwrap();
    assert!(matches!(
        unhealthy.start_assembly_line(&fixture.start_request(1, 1, 0, 1, true), 13),
        Err(MasterError::AssemblyLineExecutionCapabilityUnavailable)
    ));

    let (_temp, _database, mut stale, fixture) = open_seeded(1);
    stale
        .record_assembly_line_execution_capabilities(&fixture.capability(1, 0, true), 14)
        .unwrap();
    let auto = assemblywright_protocol::AssemblyLineAutoRunRequest {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        request_id: Uuid::new_v4(),
        expected_state_revision: 1,
        auto_run: false,
    };
    stale.set_assembly_line_auto_run(&auto, 15).unwrap();
    assert!(matches!(
        stale.start_assembly_line(&fixture.start_request(2, 1, 0, 1, false), 16),
        Err(MasterError::AssemblyLineExecutionCapabilityUnavailable)
    ));
    assert_eq!(projection.assembly_line.queue_count, 1);
}

#[test]
fn start_is_fifo_revision_bound_and_exact_replay_is_immutable() {
    let (_temp, database, mut kernel, fixture) = open_seeded(2);
    let expected_head: String = Connection::open(&database)
        .unwrap()
        .query_row(
            "SELECT feature_id FROM assembly_line_queue ORDER BY queue_position LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let (request, first) = configure_and_start(&mut kernel, &fixture, 20);
    let replay = kernel.start_assembly_line(&request, 23).unwrap();
    assert_eq!(first, replay);
    assert_eq!(first.child.feature_id.to_string(), expected_head);
    assert_eq!(
        first.resulting_state.lifecycle,
        AssemblyLineLifecycleState::Starting
    );
    let connection = Connection::open(&database).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM assembly_line_execution_sessions",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );

    let mut stale = fixture.start_request(1, 2, 0, 2, true);
    stale.request_id = Uuid::new_v4();
    stale.owner_start_approval_sha256 = stale.canonical_owner_start_approval_sha256().unwrap();
    assert!(matches!(
        kernel.start_assembly_line(&stale, 24),
        Err(MasterError::StaleAssemblyLineStateRevision { .. })
    ));
}

#[test]
fn start_dispatch_is_once_only_and_running_requires_both_signed_platform_receipts() {
    let (_temp, database, mut kernel, fixture) = open_seeded(1);
    let (_request, start) = configure_and_start(&mut kernel, &fixture, 25);
    let authority_revision = Connection::open(&database)
        .unwrap()
        .query_row(
            "SELECT authority_revision FROM assembly_line_execution_authority",
            [],
            |row| row.get::<_, i64>(0),
        )
        .unwrap() as u64;
    let mut windows = ExecutionActivationReceipt {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        receipt_id: Uuid::new_v4(),
        session_id: start.session.session_id,
        child_epoch_id: start.child.child_epoch_id,
        authority_revision,
        host_platform: ExecutionHostPlatform::Windows,
        executor_id: fixture.windows_executor_id,
        executor_revision: 1,
        observed_at_ms: 27,
        signer_key_id: "windows.executor.receipts.v1".to_string(),
        signature: Vec::new(),
    };
    sign_activation(&mut windows, WINDOWS_SIGNING_SEED);
    assert!(matches!(
        kernel.record_assembly_line_activation_receipt(&windows, 27),
        Err(MasterError::AssemblyLineExecutionReceiptMismatch)
    ));
    let intent = kernel
        .claim_assembly_line_start_dispatch(&start, 28)
        .unwrap()
        .unwrap();
    assert_eq!(intent.request_id, start.request_id);
    assert!(kernel
        .claim_assembly_line_start_dispatch(&start, 29)
        .unwrap()
        .is_none());

    assert_eq!(intent.authority_revision, authority_revision);
    assert_eq!(
        kernel
            .record_assembly_line_activation_receipt(&windows, 31)
            .unwrap()
            .lifecycle,
        AssemblyLineLifecycleState::Starting
    );

    let mut forged_mac = ExecutionActivationReceipt {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        receipt_id: Uuid::new_v4(),
        session_id: intent.session_id,
        child_epoch_id: intent.child_epoch_id,
        authority_revision: intent.authority_revision,
        host_platform: ExecutionHostPlatform::Macos,
        executor_id: fixture.mac_executor_id,
        executor_revision: 1,
        observed_at_ms: 32,
        signer_key_id: "mac.executor.receipts.v1".to_string(),
        signature: Vec::new(),
    };
    sign_activation(&mut forged_mac, FORGED_SIGNING_SEED);
    assert!(matches!(
        kernel.record_assembly_line_activation_receipt(&forged_mac, 33),
        Err(MasterError::AssemblyLineExecutionReceiptMismatch)
    ));

    sign_activation(&mut forged_mac, MAC_SIGNING_SEED);
    let running = kernel
        .record_assembly_line_activation_receipt(&forged_mac, 34)
        .unwrap();
    assert_eq!(running.lifecycle, AssemblyLineLifecycleState::Running);
    let projection = kernel
        .assembly_line_owner_projection_with_runtime(
            35,
            None,
            Some(AssemblyLineExecutionRuntimeStatus {
                binding_revision: 1,
                dispatcher_sha256: [55; 32],
            }),
        )
        .unwrap();
    assert_eq!(
        projection.assembly_line.lifecycle,
        AssemblyLineLifecycleState::Running
    );
    assert_eq!(
        Connection::open(database)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM assembly_line_effect_dispatches",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
}

#[test]
fn global_pause_dominates_start_and_creates_no_session() {
    let (_temp, database, mut kernel, fixture) = open_seeded(1);
    kernel.set_emergency_paused_at(true, 30).unwrap();
    let projection = kernel.assembly_line_owner_projection(31).unwrap();
    assert!(projection.emergency_paused);
    kernel
        .record_assembly_line_execution_capabilities(
            &fixture.capability(
                projection.assembly_line.state_revision,
                projection.emergency_pause_revision,
                true,
            ),
            32,
        )
        .unwrap();
    let request = fixture.start_request(1, 1, 1, 1, true);
    assert!(matches!(
        kernel.start_assembly_line(&request, 33),
        Err(MasterError::EmergencyPaused)
    ));
    assert_eq!(
        Connection::open(&database)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM assembly_line_execution_sessions",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
}

#[test]
fn stop_revokes_authority_records_intent_and_rejects_mismatched_receipt() {
    let (_temp, database, mut kernel, fixture) = open_seeded(1);
    let (_start, receipt) = configure_and_start(&mut kernel, &fixture, 40);
    let request = AssemblyLineStopRequest {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        request_id: Uuid::new_v4(),
        session_id: receipt.session.session_id,
        expected_state_revision: receipt.resulting_state.state_revision,
        expected_child_epoch_id: receipt.child.child_epoch_id,
    };
    let intent = kernel.stop_assembly_line(&request, 43).unwrap();
    let replay = kernel.stop_assembly_line(&request, 44).unwrap();
    assert_eq!(intent, replay);
    assert!(kernel
        .claim_assembly_line_termination_dispatch(&intent, 45)
        .unwrap());
    assert!(!kernel
        .claim_assembly_line_termination_dispatch(&intent, 46)
        .unwrap());
    assert_eq!(
        intent.resulting_state.lifecycle,
        AssemblyLineLifecycleState::Stopping
    );
    assert!(!intent.external_effect_performed);
    let connection = Connection::open(&database).unwrap();
    let authority: (i64, i64) = connection
        .query_row(
            "SELECT authority_revision,revoked FROM assembly_line_execution_authority",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(authority, (intent.authority_revision as i64, 1));

    let mismatched = ExecutionTerminationReceipt {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        receipt_id: Uuid::new_v4(),
        child_epoch_id: intent.child_epoch_id,
        mode: ExecutionTerminationMode::Stop,
        outcome: ExecutionTerminationOutcome::Reaped,
        tracked_root_process_count: 1,
        graceful_root_termination_count: 1,
        forced_root_termination_count: 0,
        reaped_root_process_count: 1,
        survivor_root_process_count: 0,
        descendant_scope: ExecutionDescendantScope::WindowsJobObject,
        descendants_reaped: true,
        last_checkpoint_sha256: [99; 32],
        observed_at_ms: 45,
        signer_key_id: "windows.executor.receipts.v1".to_string(),
        signature: vec![1; 64],
    };
    assert!(matches!(
        kernel.record_assembly_line_termination_receipt(request.request_id, &mismatched, 47),
        Err(MasterError::AssemblyLineExecutionReceiptMismatch)
    ));
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM assembly_line_termination_receipts",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
}

#[test]
fn termination_receipts_require_exact_platform_key_and_valid_signature() {
    let (_temp, _database, mut kernel, fixture) = open_seeded(1);
    let (_start, start) = configure_and_start(&mut kernel, &fixture, 47);
    let request = AssemblyLineStopRequest {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        request_id: Uuid::new_v4(),
        session_id: start.session.session_id,
        expected_state_revision: start.resulting_state.state_revision,
        expected_child_epoch_id: start.child.child_epoch_id,
    };
    let intent = kernel.stop_assembly_line(&request, 50).unwrap();
    assert!(kernel
        .claim_assembly_line_termination_dispatch(&intent, 51)
        .unwrap());

    let mut forged = termination_receipt(
        intent.child_epoch_id,
        intent.checkpoint_sha256,
        ExecutionDescendantScope::WindowsJobObject,
        "windows.executor.receipts.v1",
        52,
    );
    sign_termination(&mut forged, FORGED_SIGNING_SEED);
    assert!(matches!(
        kernel.record_assembly_line_termination_receipt(request.request_id, &forged, 53),
        Err(MasterError::AssemblyLineExecutionReceiptMismatch)
    ));

    let mut swapped = termination_receipt(
        intent.child_epoch_id,
        intent.checkpoint_sha256,
        ExecutionDescendantScope::WindowsJobObject,
        "mac.executor.receipts.v1",
        54,
    );
    sign_termination(&mut swapped, MAC_SIGNING_SEED);
    assert!(matches!(
        kernel.record_assembly_line_termination_receipt(request.request_id, &swapped, 55),
        Err(MasterError::AssemblyLineExecutionReceiptMismatch)
    ));

    let mut windows = termination_receipt(
        intent.child_epoch_id,
        intent.checkpoint_sha256,
        ExecutionDescendantScope::WindowsJobObject,
        "windows.executor.receipts.v1",
        56,
    );
    sign_termination(&mut windows, WINDOWS_SIGNING_SEED);
    let state = kernel
        .record_assembly_line_termination_receipt(request.request_id, &windows, 57)
        .unwrap();
    assert_eq!(state.lifecycle, AssemblyLineLifecycleState::Stopping);
    kernel
        .record_assembly_line_termination_receipt(request.request_id, &windows, 58)
        .unwrap();
    assert!(matches!(
        kernel.record_assembly_line_termination_receipt(Uuid::new_v4(), &windows, 59),
        Err(MasterError::AssemblyLineExecutionReceiptMismatch)
    ));

    let mut mac = termination_receipt(
        intent.child_epoch_id,
        intent.checkpoint_sha256,
        ExecutionDescendantScope::MacosProcessGroup,
        "mac.executor.receipts.v1",
        60,
    );
    sign_termination(&mut mac, MAC_SIGNING_SEED);
    let state = kernel
        .record_assembly_line_termination_receipt(request.request_id, &mac, 61)
        .unwrap();
    assert_eq!(
        state.lifecycle,
        AssemblyLineLifecycleState::PausedAtCheckpoint
    );
}

#[test]
fn checkpoint_receipts_require_action_host_key_and_valid_signature() {
    let (_temp, database, mut kernel, fixture) = open_seeded(1);
    let (_start, start) = configure_and_start(&mut kernel, &fixture, 70);
    let authority_revision: i64 = Connection::open(&database)
        .unwrap()
        .query_row(
            "SELECT authority_revision FROM assembly_line_execution_authority",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let action_id = Uuid::new_v4();
    Connection::open(&database)
        .unwrap()
        .execute(
            "INSERT INTO assembly_line_action_ledger
             (action_id,action_sequence,session_id,child_epoch_id,host_platform,
              authority_revision,envelope_sha256,effect_possible,reconciliation_strategy,
              recorded_at_ms)
             VALUES(?1,1,?2,?3,'windows',?4,?5,0,'no_effect_retry',73)",
            params![
                action_id.to_string(),
                start.session.session_id.to_string(),
                start.child.child_epoch_id.to_string(),
                authority_revision,
                [44_u8; 32].as_slice(),
            ],
        )
        .unwrap();
    let mut forged = ExecutionCheckpointReceipt {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        action_id,
        action_sequence: 1,
        child_epoch_id: start.child.child_epoch_id,
        phase: ExecutionCheckpointPhase::BeforeEffect,
        checkpoint_sha256: [45; 32],
        result_sha256: None,
        observed_at_ms: 74,
        signer_key_id: "windows.executor.receipts.v1".to_string(),
        signature: Vec::new(),
    };
    sign_checkpoint(&mut forged, FORGED_SIGNING_SEED);
    assert!(matches!(
        kernel.record_assembly_line_checkpoint_receipt(&forged, 75),
        Err(MasterError::AssemblyLineExecutionReceiptMismatch)
    ));

    let mut swapped = forged.clone();
    swapped.signer_key_id = "mac.executor.receipts.v1".to_string();
    sign_checkpoint(&mut swapped, MAC_SIGNING_SEED);
    assert!(matches!(
        kernel.record_assembly_line_checkpoint_receipt(&swapped, 76),
        Err(MasterError::AssemblyLineExecutionReceiptMismatch)
    ));

    let mut valid = forged;
    sign_checkpoint(&mut valid, WINDOWS_SIGNING_SEED);
    kernel
        .record_assembly_line_checkpoint_receipt(&valid, 77)
        .unwrap();
    kernel
        .record_assembly_line_checkpoint_receipt(&valid, 78)
        .unwrap();
    assert_eq!(
        Connection::open(&database)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM assembly_line_checkpoint_receipts",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
}

#[test]
fn emergency_pause_monotonically_revokes_and_records_immediate_termination_intent() {
    let (_temp, _database, mut kernel, fixture) = open_seeded(1);
    let (_start, receipt) = configure_and_start(&mut kernel, &fixture, 50);
    let request = AssemblyLineEmergencyPauseRequest {
        schema_version: FULL_MACHINE_ASSEMBLY_LINE_SCHEMA_VERSION,
        request_id: Uuid::new_v4(),
        session_id: receipt.session.session_id,
        expected_child_epoch_id: receipt.child.child_epoch_id,
        expected_state_revision: receipt.resulting_state.state_revision,
        expected_emergency_pause_revision: 0,
    };
    let intent = kernel.emergency_pause_assembly_line(&request, 53).unwrap();
    assert_eq!(intent.mode, ExecutionTerminationMode::EmergencyPause);
    assert_eq!(
        intent.resulting_state.lifecycle,
        AssemblyLineLifecycleState::EmergencyPaused
    );
    assert!(kernel.emergency_paused().unwrap());
    assert_eq!(kernel.emergency_pause_revision().unwrap(), 1);
    assert!(intent.authority_revision > receipt.child.child_epoch_revision);
}

#[test]
fn restart_quarantines_effect_possible_state_without_retrying() {
    let (_temp, database, mut kernel, fixture) = open_seeded(1);
    let (_start, receipt) = configure_and_start(&mut kernel, &fixture, 60);
    Connection::open(&database)
        .unwrap()
        .execute(
            "UPDATE assembly_line_state SET effect_possible=1 WHERE singleton=1",
            [],
        )
        .unwrap();
    drop(kernel);
    let reopened = MasterKernel::open(&database).unwrap();
    let state = reopened
        .assembly_line_owner_projection(64)
        .unwrap()
        .assembly_line;
    assert_eq!(
        state.lifecycle,
        AssemblyLineLifecycleState::ReconciliationRequired
    );
    assert_eq!(state.session_id, Some(receipt.session.session_id));
    assert!(
        reopened
            .assembly_line_startup_reconciliation()
            .quarantined_effect_possible_session
    );
    let connection = Connection::open(&database).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT revoked FROM assembly_line_execution_authority",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
}

#[test]
fn schema_v20_upgrade_is_backup_first_and_adds_execution_ledger() {
    let temp = TempDir::new().unwrap();
    let database = temp.path().join("master.sqlite3");
    drop(MasterKernel::open(&database).unwrap());
    let connection = Connection::open(&database).unwrap();
    connection
        .execute_batch("PRAGMA foreign_keys=OFF;")
        .unwrap();
    for trigger in [
        "assembly_line_execution_capabilities_no_update",
        "assembly_line_execution_capabilities_no_delete",
        "assembly_line_execution_sessions_no_update",
        "assembly_line_execution_sessions_no_delete",
        "assembly_line_action_ledger_no_update",
        "assembly_line_action_ledger_no_delete",
        "assembly_line_checkpoint_receipts_no_update",
        "assembly_line_checkpoint_receipts_no_delete",
        "assembly_line_activation_receipts_no_update",
        "assembly_line_activation_receipts_no_delete",
        "assembly_line_control_intents_no_update",
        "assembly_line_control_intents_no_delete",
        "assembly_line_termination_receipts_no_update",
        "assembly_line_termination_receipts_no_delete",
        "assembly_line_execution_requests_no_update",
        "assembly_line_execution_requests_no_delete",
        "assembly_line_effect_dispatches_no_update",
        "assembly_line_effect_dispatches_no_delete",
    ] {
        connection
            .execute_batch(&format!("DROP TRIGGER {trigger};"))
            .unwrap();
    }
    for table in [
        "assembly_line_termination_receipts",
        "assembly_line_control_intents",
        "assembly_line_activation_receipts",
        "assembly_line_checkpoint_receipts",
        "assembly_line_action_ledger",
        "assembly_line_child_epochs",
        "assembly_line_execution_sessions",
        "assembly_line_execution_authority",
        "assembly_line_execution_capabilities",
        "assembly_line_execution_requests",
        "assembly_line_effect_dispatches",
    ] {
        connection
            .execute_batch(&format!("DROP TABLE {table};"))
            .unwrap();
    }
    connection
        .execute_batch(
            "ALTER TABLE assembly_line_state RENAME TO assembly_line_state_v21;
             CREATE TABLE assembly_line_state (
               singleton INTEGER PRIMARY KEY NOT NULL CHECK(singleton=1),
               owner_control_revision INTEGER NOT NULL CHECK(owner_control_revision>0),
               state_revision INTEGER NOT NULL CHECK(state_revision>0),
               queue_revision INTEGER NOT NULL CHECK(queue_revision>=0),
               auto_run INTEGER NOT NULL CHECK(auto_run IN(0,1)),
               lifecycle TEXT NOT NULL CHECK(lifecycle='stopped')
             );
             INSERT INTO assembly_line_state
             SELECT singleton,owner_control_revision,state_revision,queue_revision,auto_run,lifecycle
             FROM assembly_line_state_v21;
             DROP TABLE assembly_line_state_v21;
             PRAGMA user_version=20;",
        )
        .unwrap();
    drop(connection);

    let process = MasterProcess::acquire(temp.path()).unwrap();
    assert_eq!(
        process.kernel().schema_version().unwrap(),
        MASTER_SCHEMA_VERSION
    );
    let backup = process.migration_backup_path().unwrap();
    assert_eq!(
        Connection::open(backup)
            .unwrap()
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        20
    );
    assert_eq!(
        Connection::open(&database)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type='table' AND name='assembly_line_action_ledger'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
}

#[test]
fn schema_v21_activation_upgrade_is_backup_first_and_adds_dispatch_receipts() {
    let temp = TempDir::new().unwrap();
    let database = temp.path().join("master.sqlite3");
    drop(MasterKernel::open(&database).unwrap());
    Connection::open(&database)
        .unwrap()
        .execute_batch(
            "DROP TRIGGER assembly_line_activation_receipts_no_update;
             DROP TRIGGER assembly_line_activation_receipts_no_delete;
             DROP TRIGGER assembly_line_effect_dispatches_no_update;
             DROP TRIGGER assembly_line_effect_dispatches_no_delete;
             DROP TABLE assembly_line_activation_receipts;
             DROP TABLE assembly_line_effect_dispatches;
             PRAGMA user_version=21;",
        )
        .unwrap();

    let process = MasterProcess::acquire(temp.path()).unwrap();
    assert_eq!(process.kernel().schema_version().unwrap(), 22);
    let backup = process.migration_backup_path().unwrap();
    assert_eq!(
        Connection::open(backup)
            .unwrap()
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        21
    );
    assert_eq!(
        Connection::open(&database)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table'
                 AND name IN('assembly_line_activation_receipts','assembly_line_effect_dispatches')",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        2
    );
}
