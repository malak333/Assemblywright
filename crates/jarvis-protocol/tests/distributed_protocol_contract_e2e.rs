use jarvis_protocol::{
    AttemptId, CancellationId, CapabilityDescriptor, CapabilityKind, ContextHandlingPolicy,
    DeviceId, DeviceRole, HandshakeRequest, HandshakeResponse, HandshakeStatus, JobEnvelope,
    JobResultEnvelope, JobResultStatus, LeaseId, ProtocolError, Sensitivity, StepId, TaskId,
    PROTOCOL_VERSION,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn id(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("fixed protocol UUID")
}

fn json_sha256(value: &Value) -> [u8; 32] {
    Sha256::digest(serde_json::to_vec(value).expect("serialize bounded JSON")).into()
}

#[test]
fn windows_master_and_mac_worker_complete_one_bounded_protocol_story() {
    let hello = HandshakeRequest {
        protocol_version: PROTOCOL_VERSION,
        device_id: DeviceId::new(id("11111111-1111-4111-8111-111111111111")),
        device_name: "personal-m1".to_string(),
        role: DeviceRole::MacBridge,
        registry_revision: 7,
        capabilities: vec![CapabilityDescriptor {
            id: "m1.reasoning".to_string(),
            kind: CapabilityKind::LocalInference,
            provider: "mlx".to_string(),
            model: "qwen3.6-27b".to_string(),
            max_context_bytes: 262_144,
            max_result_bytes: 786_432,
        }],
    };
    let hello_frame = serde_json::to_vec(&hello).expect("encode worker handshake");
    let master_hello =
        HandshakeRequest::decode_frame(&hello_frame).expect("master accepts handshake");
    assert_eq!(master_hello.capabilities[0].id, "m1.reasoning");

    let accepted = HandshakeResponse {
        protocol_version: PROTOCOL_VERSION,
        status: HandshakeStatus::Accepted,
        connection_epoch: 42,
        accepted_registry_revision: master_hello.registry_revision,
        reason_code: None,
    };
    let accepted_frame = serde_json::to_vec(&accepted).expect("encode master response");
    let worker_acceptance = HandshakeResponse::decode_frame(&accepted_frame)
        .expect("worker accepts bounded handshake response");

    let context = json!({"prompt":"review the protocol foundation","retain":false});
    let job = JobEnvelope {
        protocol_version: PROTOCOL_VERSION,
        connection_epoch: worker_acceptance.connection_epoch,
        sequence: 1,
        task_id: TaskId::new(id("22222222-2222-4222-8222-222222222222")),
        step_id: StepId::new(id("33333333-3333-4333-8333-333333333333")),
        attempt_id: AttemptId::new(id("44444444-4444-4444-8444-444444444444")),
        lease_id: LeaseId::new(id("55555555-5555-4555-8555-555555555555")),
        cancellation_id: CancellationId::new(id("66666666-6666-4666-8666-666666666666")),
        capability_id: master_hello.capabilities[0].id.clone(),
        selected_model: master_hello.capabilities[0].model.clone(),
        sensitivity: Sensitivity::Workspace,
        context_handling: ContextHandlingPolicy::EphemeralNoRetention,
        lease_duration_ms: 60_000,
        deadline_after_ms: 300_000,
        context_sha256: json_sha256(&context),
        context,
    };
    let job_frame = serde_json::to_vec(&job).expect("encode leased job");
    let worker_job = JobEnvelope::decode_frame(&job_frame).expect("worker accepts leased job");

    let payload = json!({"summary":"protocol foundation is internally consistent"});
    let result = JobResultEnvelope {
        protocol_version: PROTOCOL_VERSION,
        connection_epoch: worker_job.connection_epoch,
        sequence: worker_job.sequence + 1,
        task_id: worker_job.task_id,
        step_id: worker_job.step_id,
        attempt_id: worker_job.attempt_id,
        lease_id: worker_job.lease_id,
        cancellation_id: worker_job.cancellation_id,
        status: JobResultStatus::Completed,
        context_sha256: worker_job.context_sha256,
        payload_sha256: json_sha256(&payload),
        payload,
    };
    let result_frame = serde_json::to_vec(&result).expect("encode worker result");
    let master_result =
        JobResultEnvelope::decode_frame(&result_frame).expect("master decodes result");
    master_result
        .validate_for_job(&job)
        .expect("master accepts exact leased-attempt result");

    let mut replayed_against_other_lease = master_result;
    replayed_against_other_lease.lease_id =
        LeaseId::new(id("77777777-7777-4777-8777-777777777777"));
    assert_eq!(
        replayed_against_other_lease.validate_for_job(&job),
        Err(ProtocolError::ResultIdentityMismatch)
    );
}
