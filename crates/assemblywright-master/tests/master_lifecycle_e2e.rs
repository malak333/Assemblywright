use assemblywright_master::{
    AttemptStatus, DeviceRegistration, MasterError, MasterKernel, NewStep, RemoteWorkContract,
    StepStatus,
};
use assemblywright_protocol::{
    CancellationAcknowledgement, CancellationAcknowledgementStatus, CapabilityDescriptor,
    CapabilityKind, DeviceId, DeviceRole, DistributedEventBatchRequest, DistributedEventKind,
    HandshakeRequest, HandshakeStatus, JobEnvelope, JobResultEnvelope, JobResultStatus, LeaseId,
    ProtocolError, Sensitivity, StepId, TaskId, PROTOCOL_VERSION,
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
    let blocked_while_cancelling = step(
        "19191919-1919-4191-8191-191919191919",
        "20202020-2020-4202-8202-202020202020",
        "must wait for cancellation acknowledgement",
    );
    master
        .enqueue_step(&blocked_while_cancelling, 23_001)
        .expect("queue successor");
    assert!(matches!(
        master.lease_next_step(registration.device_id, accepted.connection_epoch, 23_002),
        Err(MasterError::DeviceAlreadyLeased)
    ));
    master
        .cancel_step(blocked_while_cancelling.step_id, 23_003)
        .expect("remove queued concurrency probe");
    assert!(matches!(
        master.accept_result(&result, 23_500),
        Err(MasterError::ResultNotAccepting(
            AttemptStatus::CancellationPending
        ))
    ));
    let instruction = master
        .next_cancellation(registration.device_id, accepted.connection_epoch, 23_500)
        .expect("poll cancellation")
        .expect("pending cancellation");
    master
        .accept_cancellation_ack_from(
            registration.device_id,
            &CancellationAcknowledgement {
                protocol_version: PROTOCOL_VERSION,
                connection_epoch: instruction.connection_epoch,
                sequence: instruction.sequence + 1,
                task_id: instruction.task_id,
                step_id: instruction.step_id,
                attempt_id: instruction.attempt_id,
                lease_id: instruction.lease_id,
                cancellation_id: instruction.cancellation_id,
                status: CancellationAcknowledgementStatus::Cancelled,
            },
            23_501,
        )
        .expect("accept bounded cancellation acknowledgement");
    assert_eq!(
        master
            .step_snapshot(cancellable.step_id)
            .expect("cancelled snapshot")
            .status,
        StepStatus::Cancelled
    );

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

#[test]
fn authenticated_result_is_bound_to_the_leased_device() {
    let owner = registration();
    let mut wrong = registration();
    wrong.device_id = DeviceId::new(id("12121212-1212-4121-8121-121212121212"));
    wrong.device_name = "wrong-worker".to_string();
    let mut master = MasterKernel::in_memory().expect("open master");
    master.register_device(&owner).expect("register owner");
    master
        .register_device(&wrong)
        .expect("register wrong worker");
    let accepted = master
        .accept_handshake(&handshake(&owner), 120_000)
        .expect("accept owner");
    let queued = step(
        "13131313-1313-4131-8131-131313131313",
        "14141414-1414-4141-8141-141414141414",
        "bind result identity",
    );
    master.enqueue_step(&queued, 120_001).expect("queue");
    let job = master
        .lease_next_step(owner.device_id, accepted.connection_epoch, 120_002)
        .expect("lease");
    let result = fake_worker_result(&job, json!({"bound":true}));
    assert!(matches!(
        master.accept_result_from(wrong.device_id, &result, 120_003),
        Err(MasterError::ResultDeviceMismatch)
    ));
    master
        .accept_result_from(owner.device_id, &result, 120_004)
        .expect("accept owning authenticated device");
}

#[test]
fn cancellation_expiry_revokes_connection_and_restart_suppresses_old_attempt() {
    let directory = tempdir().expect("temporary master directory");
    let database = directory.path().join("master.sqlite3");
    let registration = registration();
    let queued = step(
        "15151515-1515-4151-8151-151515151515",
        "16161616-1616-4161-8161-161616161616",
        "expire cancellation",
    );
    let mut master = MasterKernel::open(&database).expect("open master");
    master.register_device(&registration).expect("register");
    let accepted = master
        .accept_handshake(&handshake(&registration), 130_000)
        .expect("accept");
    master.enqueue_step(&queued, 130_001).expect("queue");
    let job = master
        .lease_next_step(registration.device_id, accepted.connection_epoch, 130_002)
        .expect("lease");
    master
        .cancel_step(queued.step_id, 130_003)
        .expect("request cancellation");
    assert_eq!(
        master
            .reconcile_cancellation_deadlines(132_003)
            .expect("expire cancellation"),
        1
    );
    assert_eq!(
        master.attempt_status(job.attempt_id).expect("attempt"),
        AttemptStatus::Abandoned
    );
    assert_eq!(
        master.step_snapshot(queued.step_id).expect("step").status,
        StepStatus::Cancelled
    );
    assert!(matches!(
        master.accept_result(&fake_worker_result(&job, json!({"late":true})), 132_004),
        Err(MasterError::ResultNotAccepting(AttemptStatus::Abandoned))
    ));
    let evidence = master
        .distributed_events(&DistributedEventBatchRequest {
            protocol_version: PROTOCOL_VERSION,
            connection_epoch: accepted.connection_epoch,
            after: None,
            limit: 64,
        })
        .expect("metadata cancellation evidence");
    for expected in [
        DistributedEventKind::StepCancellationRequested,
        DistributedEventKind::StepCancellationExpired,
        DistributedEventKind::StepCancelled,
        DistributedEventKind::DeviceDisconnected,
    ] {
        assert!(
            evidence.events.iter().any(|event| event.kind == expected),
            "missing transactional metadata event {expected:?}"
        );
    }
    let fresh = master
        .accept_handshake(&handshake(&registration), 132_005)
        .expect("fresh epoch after revocation");
    assert!(fresh.connection_epoch > accepted.connection_epoch);

    let restart_step = step(
        "17171717-1717-4171-8171-171717171717",
        "18181818-1818-4181-8181-181818181818",
        "restart pending cancellation",
    );
    master.enqueue_step(&restart_step, 132_006).expect("queue");
    let restart_job = master
        .lease_next_step(registration.device_id, fresh.connection_epoch, 132_007)
        .expect("lease");
    master
        .cancel_step(restart_step.step_id, 132_008)
        .expect("request cancellation");
    drop(master);

    let restarted = MasterKernel::open(&database).expect("restart master");
    assert_eq!(
        restarted
            .attempt_status(restart_job.attempt_id)
            .expect("reconciled attempt"),
        AttemptStatus::Abandoned
    );
    assert_eq!(
        restarted
            .step_snapshot(restart_step.step_id)
            .expect("reconciled step")
            .status,
        StepStatus::Cancelled
    );
}

#[test]
fn remote_fixture_kernel_revalidates_stored_job_and_result_contracts() {
    let fixture_registration = DeviceRegistration {
        device_id: DeviceId::new(id("21212121-2121-4212-8212-212121212121")),
        device_name: "fixture-only-worker".to_string(),
        role: DeviceRole::MacBridge,
        registry_revision: 1,
        capabilities: vec![CapabilityDescriptor::fixture_reasoning()],
    };
    let mut master = MasterKernel::in_memory().expect("fixture master");
    master
        .register_device(&fixture_registration)
        .expect("register exact fixture");
    let accepted = master
        .accept_handshake(&handshake(&fixture_registration), 140_000)
        .expect("accept fixture");
    let invalid = NewStep {
        task_id: TaskId::new(id("22222222-2121-4212-8212-212121212121")),
        step_id: StepId::new(id("23232323-2323-4232-8232-232323232323")),
        capability_id: "fixture.reasoning".to_string(),
        sensitivity: Sensitivity::Workspace,
        context: json!({"operation":"synthetic_echo","input":"not-public","delay_ms":0}),
        lease_duration_ms: 60_000,
        deadline_after_ms: 60_000,
    };
    master
        .enqueue_step(&invalid, 140_001)
        .expect("queue invalid");
    assert!(matches!(
        master.lease_next_fixture_step(
            fixture_registration.device_id,
            accepted.connection_epoch,
            140_002,
        ),
        Err(MasterError::Protocol(ProtocolError::InvalidFixtureJob))
    ));
    assert_eq!(
        master
            .step_snapshot(invalid.step_id)
            .expect("invalid step")
            .status,
        StepStatus::Queued
    );
    master
        .cancel_step(invalid.step_id, 140_003)
        .expect("remove invalid fixture");

    let valid = NewStep {
        task_id: TaskId::new(id("24242424-2424-4242-8242-242424242424")),
        step_id: StepId::new(id("25252525-2525-4252-8252-252525252525")),
        capability_id: "fixture.reasoning".to_string(),
        sensitivity: Sensitivity::Public,
        context: json!({"operation":"synthetic_echo","input":"exact","delay_ms":0}),
        lease_duration_ms: 60_000,
        deadline_after_ms: 60_000,
    };
    master.enqueue_step(&valid, 140_004).expect("queue valid");
    let job = master
        .lease_next_fixture_step(
            fixture_registration.device_id,
            accepted.connection_epoch,
            140_005,
        )
        .expect("lease exact fixture");
    let wrong_payload = json!({"operation":"synthetic_echo","output":"wrong","synthetic":true});
    let wrong_result = fake_worker_result(&job, wrong_payload);
    assert!(matches!(
        master.accept_fixture_result_from(fixture_registration.device_id, &wrong_result, 140_006,),
        Err(MasterError::Protocol(ProtocolError::InvalidFixtureJob))
    ));
    master
        .accept_fixture_result_from(
            fixture_registration.device_id,
            &fake_worker_result(
                &job,
                json!({"operation":"synthetic_echo","output":"exact","synthetic":true}),
            ),
            140_007,
        )
        .expect("accept exact fixture result");

    let ordinary = registration();
    let mut ordinary_master = MasterKernel::in_memory().expect("ordinary master");
    ordinary_master
        .register_device(&ordinary)
        .expect("register ordinary worker");
    let ordinary_connection = ordinary_master
        .accept_handshake(&handshake(&ordinary), 141_000)
        .expect("accept ordinary worker");
    let ordinary_step = step(
        "26262626-2626-4262-8262-262626262626",
        "27272727-2727-4272-8272-272727272727",
        "non-fixture stored attempt",
    );
    ordinary_master
        .enqueue_step(&ordinary_step, 141_001)
        .expect("queue ordinary");
    let ordinary_job = ordinary_master
        .lease_next_step(
            ordinary.device_id,
            ordinary_connection.connection_epoch,
            141_002,
        )
        .expect("lease ordinary");
    assert!(matches!(
        ordinary_master.accept_fixture_result_from(
            ordinary.device_id,
            &fake_worker_result(&ordinary_job, json!({"ordinary":true})),
            141_003,
        ),
        Err(MasterError::Protocol(ProtocolError::InvalidFixtureJob))
    ));
}

#[test]
fn authoritative_emergency_pause_dominates_fixture_lease_and_result_acceptance() {
    let registration = DeviceRegistration {
        device_id: DeviceId::new(id("28282828-2828-4282-8282-282828282828")),
        device_name: "paused-fixture-worker".to_string(),
        role: DeviceRole::MacBridge,
        registry_revision: 1,
        capabilities: vec![CapabilityDescriptor::fixture_reasoning()],
    };
    let mut master = MasterKernel::in_memory().expect("paused fixture master");
    master.register_device(&registration).expect("register");
    let connection = master
        .accept_handshake(&handshake(&registration), 145_000)
        .expect("connect");
    let queued = NewStep {
        task_id: TaskId::new(id("29292929-2929-4292-8292-292929292929")),
        step_id: StepId::new(id("30303030-3030-4303-8303-303030303030")),
        capability_id: "fixture.reasoning".to_string(),
        sensitivity: Sensitivity::Public,
        context: json!({"operation":"synthetic_echo","input":"pause-wins","delay_ms":0}),
        lease_duration_ms: 60_000,
        deadline_after_ms: 60_000,
    };
    master.enqueue_step(&queued, 145_001).expect("queue");
    master
        .set_emergency_paused_at(true, 145_002)
        .expect("activate authoritative pause");
    assert!(matches!(
        master.lease_next_fixture_step(
            registration.device_id,
            connection.connection_epoch,
            145_003,
        ),
        Err(MasterError::EmergencyPaused)
    ));
    master
        .set_emergency_paused_at(false, 145_004)
        .expect("resume for lease");
    let job = master
        .lease_next_fixture_step(registration.device_id, connection.connection_epoch, 145_005)
        .expect("lease after deliberate resume");
    let result = fake_worker_result(
        &job,
        json!({"operation":"synthetic_echo","output":"pause-wins","synthetic":true}),
    );
    master
        .set_emergency_paused_at(true, 145_006)
        .expect("pause before result");
    assert!(matches!(
        master.accept_fixture_result_from(registration.device_id, &result, 145_007),
        Err(MasterError::EmergencyPaused)
    ));
    assert_eq!(
        master
            .step_snapshot(queued.step_id)
            .expect("paused step")
            .status,
        StepStatus::Leased
    );
    assert_eq!(
        master.attempt_status(job.attempt_id).expect("attempt"),
        AttemptStatus::CancellationPending
    );
    assert_eq!(
        master
            .health_snapshot()
            .expect("paused health")
            .active_attempts,
        1,
        "pause cancellation must retain its concurrency slot"
    );
    let instruction = master
        .next_cancellation(registration.device_id, connection.connection_epoch, 145_008)
        .expect("poll pause cancellation")
        .expect("pause cancellation instruction");
    assert_eq!(instruction.attempt_id, job.attempt_id);
    master
        .set_emergency_paused_at(false, 145_009)
        .expect("deliberately resume admission");
    assert!(matches!(
        master.accept_fixture_result_from(registration.device_id, &result, 145_010),
        Err(MasterError::ResultNotAccepting(
            AttemptStatus::CancellationPending
        ))
    ));
    master
        .accept_cancellation_ack_from(
            registration.device_id,
            &CancellationAcknowledgement {
                protocol_version: PROTOCOL_VERSION,
                connection_epoch: instruction.connection_epoch,
                sequence: instruction.sequence + 1,
                task_id: instruction.task_id,
                step_id: instruction.step_id,
                attempt_id: instruction.attempt_id,
                lease_id: instruction.lease_id,
                cancellation_id: instruction.cancellation_id,
                status: CancellationAcknowledgementStatus::Cancelled,
            },
            145_011,
        )
        .expect("acknowledge pause cancellation");
    assert!(matches!(
        master.accept_fixture_result_from(registration.device_id, &result, 145_012),
        Err(MasterError::ResultNotAccepting(AttemptStatus::Cancelled))
    ));
    assert_eq!(
        master
            .step_snapshot(queued.step_id)
            .expect("cancelled step")
            .status,
        StepStatus::Cancelled
    );
    let evidence = master
        .distributed_events(&DistributedEventBatchRequest {
            protocol_version: PROTOCOL_VERSION,
            connection_epoch: connection.connection_epoch,
            after: None,
            limit: 64,
        })
        .expect("pause cancellation evidence");
    for expected in [
        DistributedEventKind::StepCancellationRequested,
        DistributedEventKind::StepCancellationAcknowledged,
        DistributedEventKind::StepCancelled,
    ] {
        assert!(
            evidence.events.iter().any(|event| event.kind == expected),
            "missing pause cancellation event {expected:?}"
        );
    }
}

#[test]
fn cancellation_pending_consumes_global_concurrency_until_terminal() {
    let mut master = MasterKernel::in_memory().expect("global concurrency master");
    let mut workers = Vec::new();
    for index in 0..5_u128 {
        let registration = DeviceRegistration {
            device_id: DeviceId::new(Uuid::from_u128(
                0x30000000000040008000000000000000_u128 + index,
            )),
            device_name: format!("worker-{index}"),
            role: DeviceRole::MacBridge,
            registry_revision: 1,
            capabilities: vec![CapabilityDescriptor {
                id: "m1.reasoning".to_string(),
                kind: CapabilityKind::LocalInference,
                provider: "fake-mlx".to_string(),
                model: "fake-qwen".to_string(),
                max_context_bytes: 262_144,
                max_result_bytes: 786_432,
            }],
        };
        master
            .register_device(&registration)
            .expect("register concurrency worker");
        let connection = master
            .accept_handshake(&handshake(&registration), 150_000 + index as u64)
            .expect("accept concurrency worker");
        workers.push((registration, connection.connection_epoch));
    }
    for index in 0..5_u128 {
        master
            .enqueue_step(
                &NewStep {
                    task_id: TaskId::new(Uuid::from_u128(
                        0x40000000000040008000000000000000_u128 + index,
                    )),
                    step_id: StepId::new(Uuid::from_u128(
                        0x50000000000040008000000000000000_u128 + index,
                    )),
                    capability_id: "m1.reasoning".to_string(),
                    sensitivity: Sensitivity::Workspace,
                    context: json!({"prompt":format!("job-{index}")}),
                    lease_duration_ms: 60_000,
                    deadline_after_ms: 60_000,
                },
                151_000 + index as u64,
            )
            .expect("queue concurrency job");
    }
    for (index, (registration, epoch)) in workers.iter().take(4).enumerate() {
        let job = master
            .lease_next_step(registration.device_id, *epoch, 152_000 + index as u64)
            .expect("lease concurrency slot");
        master
            .cancel_step(job.step_id, 153_000 + index as u64)
            .expect("leave slot cancellation pending");
    }
    assert_eq!(master.health_snapshot().expect("health").active_attempts, 4);
    assert!(matches!(
        master.lease_next_step(workers[4].0.device_id, workers[4].1, 154_000),
        Err(MasterError::ConcurrentJobLimit)
    ));
}

#[test]
fn exact_singleton_mlx_remote_contract_rejects_mixed_and_model_drift() {
    let capability = CapabilityDescriptor::mlx_reasoning("mlx-model-v1", 64 * 1024, 64 * 1024);
    let registration = DeviceRegistration {
        device_id: DeviceId::new(Uuid::new_v4()),
        device_name: "mlx-only-worker".to_string(),
        role: DeviceRole::MacBridge,
        registry_revision: 1,
        capabilities: vec![capability.clone()],
    };
    let contract =
        RemoteWorkContract::from_registration(&registration).expect("derive exact MLX contract");
    let mut mixed = registration.clone();
    mixed
        .capabilities
        .push(CapabilityDescriptor::fixture_reasoning());
    assert!(matches!(
        RemoteWorkContract::from_registration(&mixed),
        Err(MasterError::InvalidRemoteWorkContract)
    ));

    let mut master = MasterKernel::in_memory().expect("MLX master");
    master.register_device(&registration).expect("register MLX");
    let connection = master
        .accept_handshake(&handshake(&registration), 160_000)
        .expect("connect MLX");
    let queued = NewStep {
        task_id: TaskId::new(Uuid::new_v4()),
        step_id: StepId::new(Uuid::new_v4()),
        capability_id: "mlx.reasoning".to_string(),
        sensitivity: Sensitivity::Public,
        context: json!({
            "operation":"generate_text",
            "prompt":"bounded",
            "max_tokens":64,
            "temperature_milli":700
        }),
        lease_duration_ms: 60_000,
        deadline_after_ms: 60_000,
    };
    master.enqueue_step(&queued, 160_001).expect("enqueue MLX");
    let job = master
        .lease_next_remote_step(
            registration.device_id,
            connection.connection_epoch,
            160_002,
            &contract,
        )
        .expect("lease MLX");
    assert_eq!(job.selected_model, "mlx-model-v1");

    let wrong = fake_worker_result(
        &job,
        json!({
            "operation":"generate_text",
            "output":"result",
            "model":"mlx-model-v2"
        }),
    );
    assert!(matches!(
        master.accept_remote_result_from(registration.device_id, &wrong, 160_003, &contract),
        Err(MasterError::Protocol(ProtocolError::InvalidMlxResult))
    ));
    let exact = fake_worker_result(
        &job,
        json!({
            "operation":"generate_text",
            "output":"result",
            "model":"mlx-model-v1"
        }),
    );
    master
        .accept_remote_result_from(registration.device_id, &exact, 160_004, &contract)
        .expect("accept exact MLX result");
}

#[test]
fn emergency_pause_cancels_active_mlx_and_rejects_late_result() {
    let capability = CapabilityDescriptor::mlx_reasoning("mlx-model", 64 * 1024, 64 * 1024);
    let registration = DeviceRegistration {
        device_id: DeviceId::new(Uuid::new_v4()),
        device_name: "paused-mlx-worker".to_string(),
        role: DeviceRole::InferenceWorker,
        registry_revision: 1,
        capabilities: vec![capability],
    };
    let contract = RemoteWorkContract::from_registration(&registration).expect("MLX contract");
    let mut master = MasterKernel::in_memory().expect("paused MLX master");
    master.register_device(&registration).expect("register MLX");
    let connection = master
        .accept_handshake(&handshake(&registration), 170_000)
        .expect("connect MLX");
    let queued = NewStep {
        task_id: TaskId::new(Uuid::new_v4()),
        step_id: StepId::new(Uuid::new_v4()),
        capability_id: "mlx.reasoning".to_string(),
        sensitivity: Sensitivity::Public,
        context: json!({
            "operation":"generate_text",
            "prompt":"pause must win",
            "max_tokens":64,
            "temperature_milli":0
        }),
        lease_duration_ms: 60_000,
        deadline_after_ms: 60_000,
    };
    master.enqueue_step(&queued, 170_001).expect("enqueue");
    let job = master
        .lease_next_remote_step(
            registration.device_id,
            connection.connection_epoch,
            170_002,
            &contract,
        )
        .expect("lease");
    master
        .set_emergency_paused_at(true, 170_003)
        .expect("pause");
    assert_eq!(
        master.attempt_status(job.attempt_id).expect("attempt"),
        AttemptStatus::CancellationPending
    );
    let late = fake_worker_result(
        &job,
        json!({
            "operation":"generate_text",
            "output":"must not escape",
            "model":"mlx-model"
        }),
    );
    assert!(matches!(
        master.accept_remote_result_from(registration.device_id, &late, 170_004, &contract),
        Err(MasterError::EmergencyPaused)
    ));
    let cancellation = master
        .next_remote_cancellation(
            registration.device_id,
            connection.connection_epoch,
            170_005,
            &contract,
        )
        .expect("poll")
        .expect("MLX cancellation");
    assert_eq!(cancellation.attempt_id, job.attempt_id);
}
