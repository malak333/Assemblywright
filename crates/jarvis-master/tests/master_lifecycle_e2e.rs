use jarvis_master::{
    AttemptStatus, DeviceRegistration, MasterError, MasterKernel, NewStep, StepStatus,
};
use jarvis_protocol::{
    CapabilityDescriptor, CapabilityKind, DeviceId, DeviceRole, HandshakeRequest, HandshakeStatus,
    JobEnvelope, JobResultEnvelope, JobResultStatus, LeaseId, ProtocolError, Sensitivity, StepId,
    TaskId, PROTOCOL_VERSION,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tempfile::tempdir;
use uuid::Uuid;

fn id(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("fixed master-kernel UUID")
}

fn registration() -> DeviceRegistration {
    DeviceRegistration {
        device_id: DeviceId::new(id("11111111-1111-4111-8111-111111111111")),
        device_name: "fake-m1-worker".to_string(),
        role: DeviceRole::MacBridge,
        registry_revision: 9,
        capabilities: vec![CapabilityDescriptor {
            id: "m1.reasoning".to_string(),
            kind: CapabilityKind::LocalInference,
            provider: "fake-mlx".to_string(),
            model: "fake-qwen".to_string(),
            max_context_bytes: 262_144,
            max_result_bytes: 786_432,
        }],
    }
}

fn handshake(registration: &DeviceRegistration) -> HandshakeRequest {
    HandshakeRequest {
        protocol_version: PROTOCOL_VERSION,
        device_id: registration.device_id,
        device_name: registration.device_name.clone(),
        role: registration.role,
        registry_revision: registration.registry_revision,
        capabilities: registration.capabilities.clone(),
    }
}

fn step(task_id: &str, step_id: &str, prompt: &str) -> NewStep {
    NewStep {
        task_id: TaskId::new(id(task_id)),
        step_id: StepId::new(id(step_id)),
        capability_id: "m1.reasoning".to_string(),
        sensitivity: Sensitivity::Workspace,
        context: json!({"prompt":prompt,"retain":false}),
        lease_duration_ms: 60_000,
        deadline_after_ms: 300_000,
    }
}

fn json_sha256(value: &Value) -> [u8; 32] {
    Sha256::digest(serde_json::to_vec(value).expect("serialize fake-worker JSON")).into()
}

fn fake_worker_result(job: &JobEnvelope, payload: Value) -> JobResultEnvelope {
    JobResultEnvelope {
        protocol_version: PROTOCOL_VERSION,
        connection_epoch: job.connection_epoch,
        sequence: job.sequence + 1,
        task_id: job.task_id,
        step_id: job.step_id,
        attempt_id: job.attempt_id,
        lease_id: job.lease_id,
        cancellation_id: job.cancellation_id,
        status: JobResultStatus::Completed,
        context_sha256: job.context_sha256,
        payload_sha256: json_sha256(&payload),
        payload,
    }
}

#[test]
fn windows_master_kernel_accepts_fake_worker_result_durably() {
    let directory = tempdir().expect("temporary master directory");
    let database = directory.path().join("master.sqlite3");
    let registration = registration();
    let queued = step(
        "22222222-2222-4222-8222-222222222222",
        "33333333-3333-4333-8333-333333333333",
        "review the durable master kernel",
    );

    let mut master = MasterKernel::open(&database).expect("open master kernel");
    master
        .register_device(&registration)
        .expect("register fake worker");
    let accepted = master
        .accept_handshake(&handshake(&registration), 1_000)
        .expect("accept registered fake worker");
    assert_eq!(accepted.status, HandshakeStatus::Accepted);
    master
        .enqueue_step(&queued, 2_000)
        .expect("durably enqueue step");
    let job = master
        .lease_next_step(registration.device_id, accepted.connection_epoch, 3_000)
        .expect("lease eligible step");
    let result = fake_worker_result(&job, json!({"summary":"kernel is durable"}));
    let accepted_result = master
        .accept_result(&result, 4_000)
        .expect("accept exact fake-worker result");
    assert_eq!(accepted_result.status, StepStatus::Succeeded);
    drop(master);

    let mut reopened = MasterKernel::open(&database).expect("reopen master kernel");
    assert_eq!(
        reopened.startup_reconciliation().disconnected_connections,
        1
    );
    assert_eq!(reopened.startup_reconciliation().abandoned_attempts, 0);
    let snapshot = reopened
        .step_snapshot(queued.step_id)
        .expect("load durable step");
    assert_eq!(snapshot.status, StepStatus::Succeeded);
    assert_eq!(
        snapshot.accepted_payload_sha256,
        Some(result.payload_sha256)
    );
    assert!(matches!(
        reopened.accept_result(&result, 5_000),
        Err(MasterError::ResultNotAccepting(AttemptStatus::Succeeded))
    ));
}

#[test]
fn windows_master_kernel_reconciles_fake_worker_across_restart() {
    let directory = tempdir().expect("temporary master directory");
    let database = directory.path().join("master.sqlite3");
    let registration = registration();
    let queued = step(
        "44444444-4444-4444-8444-444444444444",
        "55555555-5555-4555-8555-555555555555",
        "survive a master restart",
    );

    let mut first = MasterKernel::open(&database).expect("open first master");
    first
        .register_device(&registration)
        .expect("register fake worker");
    let first_handshake = first
        .accept_handshake(&handshake(&registration), 10_000)
        .expect("accept first connection");
    first
        .enqueue_step(&queued, 11_000)
        .expect("enqueue durable step");
    let first_job = first
        .lease_next_step(
            registration.device_id,
            first_handshake.connection_epoch,
            12_000,
        )
        .expect("lease first attempt");
    let late_result = fake_worker_result(&first_job, json!({"late":true}));
    drop(first);

    let mut restarted = MasterKernel::open(&database).expect("restart master");
    assert_eq!(
        restarted.startup_reconciliation().disconnected_connections,
        1
    );
    assert_eq!(restarted.startup_reconciliation().abandoned_attempts, 1);
    assert_eq!(restarted.startup_reconciliation().requeued_steps, 1);
    assert_eq!(
        restarted
            .attempt_status(first_job.attempt_id)
            .expect("load abandoned attempt"),
        AttemptStatus::Abandoned
    );
    assert_eq!(
        restarted
            .step_snapshot(queued.step_id)
            .expect("load requeued step")
            .status,
        StepStatus::Queued
    );
    assert!(matches!(
        restarted.accept_result(&late_result, 13_000),
        Err(MasterError::ResultNotAccepting(AttemptStatus::Abandoned))
    ));

    let second_handshake = restarted
        .accept_handshake(&handshake(&registration), 14_000)
        .expect("accept replacement connection");
    assert!(second_handshake.connection_epoch > first_handshake.connection_epoch);
    let second_job = restarted
        .lease_next_step(
            registration.device_id,
            second_handshake.connection_epoch,
            15_000,
        )
        .expect("reissue only after durable abandonment");
    assert_ne!(second_job.attempt_id, first_job.attempt_id);
    assert_ne!(second_job.lease_id, first_job.lease_id);
    restarted
        .accept_result(
            &fake_worker_result(&second_job, json!({"recovered":true})),
            16_000,
        )
        .expect("accept replacement attempt");
}

#[test]
fn master_rejects_duplicate_wrong_lease_cancelled_and_expired_work() {
    let registration = registration();
    let mut master = MasterKernel::in_memory().expect("open in-memory master");
    let unknown = master
        .accept_handshake(&handshake(&registration), 19_999)
        .expect("return bounded unknown-device rejection");
    assert_eq!(unknown.status, HandshakeStatus::Rejected);
    assert_eq!(unknown.reason_code.as_deref(), Some("unknown_device"));
    assert_eq!(unknown.accepted_registry_revision, 0);
    master
        .register_device(&registration)
        .expect("register fake worker");
    let accepted = master
        .accept_handshake(&handshake(&registration), 20_000)
        .expect("accept worker");
    let duplicate = master
        .accept_handshake(&handshake(&registration), 20_001)
        .expect("return bounded duplicate rejection");
    assert_eq!(duplicate.status, HandshakeStatus::Rejected);
    assert_eq!(
        duplicate.reason_code.as_deref(),
        Some("duplicate_active_connection")
    );

    let cancellable = step(
        "66666666-6666-4666-8666-666666666666",
        "77777777-7777-4777-8777-777777777777",
        "cancel this attempt",
    );
    master
        .enqueue_step(&cancellable, 21_000)
        .expect("enqueue cancellable step");
    let job = master
        .lease_next_step(registration.device_id, accepted.connection_epoch, 22_000)
        .expect("lease cancellable step");
    let result = fake_worker_result(&job, json!({"should_not_commit":true}));
    let mut wrong_lease = result.clone();
    wrong_lease.lease_id = LeaseId::new(id("88888888-8888-4888-8888-888888888888"));
    assert!(matches!(
        master.accept_result(&wrong_lease, 22_500),
        Err(MasterError::Protocol(ProtocolError::ResultIdentityMismatch))
    ));
    master
        .cancel_step(cancellable.step_id, 23_000)
        .expect("durably cancel leased step");
    assert!(matches!(
        master.accept_result(&result, 23_500),
        Err(MasterError::ResultNotAccepting(AttemptStatus::Cancelled))
    ));

    let expiring = step(
        "99999999-9999-4999-8999-999999999999",
        "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
        "expire this attempt",
    );
    master
        .enqueue_step(&expiring, 24_000)
        .expect("enqueue expiring step");
    let expiring_job = master
        .lease_next_step(registration.device_id, accepted.connection_epoch, 25_000)
        .expect("lease expiring step");
    let reconciliation = master
        .reconcile_expired_leases(25_000 + expiring.lease_duration_ms)
        .expect("expire leased step");
    assert_eq!(reconciliation.expired_attempts, 1);
    assert_eq!(reconciliation.requeued_steps, 1);
    assert_eq!(
        master
            .attempt_status(expiring_job.attempt_id)
            .expect("load expired attempt"),
        AttemptStatus::Expired
    );
    assert!(matches!(
        master.accept_result(
            &fake_worker_result(&expiring_job, json!({"too_late":true})),
            90_000,
        ),
        Err(MasterError::ResultNotAccepting(AttemptStatus::Expired))
    ));
    master
        .revoke_device(registration.device_id, 90_001)
        .expect("revoke registered device and disconnect it");
    let revoked = master
        .accept_handshake(&handshake(&registration), 90_002)
        .expect("return bounded revoked-device rejection");
    assert_eq!(revoked.status, HandshakeStatus::Rejected);
    assert_eq!(revoked.reason_code.as_deref(), Some("revoked_device"));
}

#[test]
fn master_enforces_registered_capability_context_and_result_limits() {
    let mut context_limited = registration();
    context_limited.capabilities[0].max_context_bytes = 8;
    let mut master = MasterKernel::in_memory().expect("open context-limit master");
    master
        .register_device(&context_limited)
        .expect("register context-limited worker");
    let accepted = master
        .accept_handshake(&handshake(&context_limited), 100_000)
        .expect("accept context-limited worker");
    master
        .enqueue_step(
            &step(
                "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
                "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
                "larger than eight bytes",
            ),
            100_001,
        )
        .expect("queue globally bounded context");
    assert!(matches!(
        master.lease_next_step(
            context_limited.device_id,
            accepted.connection_epoch,
            100_002,
        ),
        Err(MasterError::NoEligibleStep)
    ));

    let mut result_limited = registration();
    result_limited.device_id = DeviceId::new(id("dddddddd-dddd-4ddd-8ddd-dddddddddddd"));
    result_limited.capabilities[0].max_result_bytes = 16;
    let mut master = MasterKernel::in_memory().expect("open result-limit master");
    master
        .register_device(&result_limited)
        .expect("register result-limited worker");
    let accepted = master
        .accept_handshake(&handshake(&result_limited), 110_000)
        .expect("accept result-limited worker");
    let queued = NewStep {
        task_id: TaskId::new(id("eeeeeeee-eeee-4eee-8eee-eeeeeeeeeeee")),
        step_id: StepId::new(id("ffffffff-ffff-4fff-8fff-ffffffffffff")),
        capability_id: "m1.reasoning".to_string(),
        sensitivity: Sensitivity::Workspace,
        context: json!({"p":"x"}),
        lease_duration_ms: 60_000,
        deadline_after_ms: 300_000,
    };
    master
        .enqueue_step(&queued, 110_001)
        .expect("queue result-limited work");
    let job = master
        .lease_next_step(result_limited.device_id, accepted.connection_epoch, 110_002)
        .expect("lease result-limited work");
    assert!(matches!(
        master.accept_result(
            &fake_worker_result(&job, json!({"summary":"too large"})),
            110_003,
        ),
        Err(MasterError::CapabilityLimitExceeded)
    ));
}
