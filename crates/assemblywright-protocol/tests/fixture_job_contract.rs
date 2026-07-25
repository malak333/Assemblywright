use assemblywright_protocol::{
    AttemptId, CancellationAcknowledgement, CancellationAcknowledgementStatus, CancellationId,
    CancellationInstruction, CapabilityDescriptor, ContextHandlingPolicy, JobEnvelope,
    JobResultEnvelope, JobResultStatus, LeaseId, ProtocolError, Sensitivity, StepId, TaskId,
    CANCELLATION_ACK_DEADLINE_MS, FIXTURE_REASONING_CAPABILITY_ID, FIXTURE_REASONING_MODEL,
    FIXTURE_REASONING_PROVIDER, MAX_FIXTURE_INPUT_BYTES, PROTOCOL_VERSION,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn fixture_job() -> JobEnvelope {
    let context = json!({"operation":"synthetic_echo","input":"hello","delay_ms":0});
    JobEnvelope {
        protocol_version: PROTOCOL_VERSION,
        connection_epoch: 9,
        sequence: 1,
        task_id: TaskId::new(Uuid::new_v4()),
        step_id: StepId::new(Uuid::new_v4()),
        attempt_id: AttemptId::new(Uuid::new_v4()),
        lease_id: LeaseId::new(Uuid::new_v4()),
        cancellation_id: CancellationId::new(Uuid::new_v4()),
        capability_id: CapabilityDescriptor::fixture_reasoning().id,
        selected_model: FIXTURE_REASONING_MODEL.to_string(),
        sensitivity: Sensitivity::Public,
        context_handling: ContextHandlingPolicy::EphemeralNoRetention,
        lease_duration_ms: 60_000,
        deadline_after_ms: 60_000,
        context_sha256: Sha256::digest(serde_json::to_vec(&context).unwrap()).into(),
        context,
    }
}

#[test]
fn fixture_capability_and_job_are_exact_and_bounded() {
    CapabilityDescriptor::fixture_reasoning()
        .validate()
        .expect("exact fixture descriptor");
    fixture_job()
        .validate_fixture_reasoning()
        .expect("exact public synthetic fixture");

    let mut wrong_model = CapabilityDescriptor::fixture_reasoning();
    wrong_model.model = "jarvis-fixture-v2".to_string();
    assert_eq!(
        wrong_model.validate(),
        Err(ProtocolError::InvalidFixtureCapability)
    );

    let mut oversized = fixture_job();
    oversized.context = json!({"operation":"synthetic_echo","input":"x".repeat(MAX_FIXTURE_INPUT_BYTES + 1),"delay_ms":0});
    oversized.context_sha256 =
        Sha256::digest(serde_json::to_vec(&oversized.context).unwrap()).into();
    assert_eq!(
        oversized.validate_fixture_reasoning(),
        Err(ProtocolError::InvalidFixtureJob)
    );
}

#[test]
fn cancellation_is_attempt_lease_epoch_and_sequence_bound() {
    let job = fixture_job();
    let instruction = CancellationInstruction {
        protocol_version: PROTOCOL_VERSION,
        connection_epoch: job.connection_epoch,
        sequence: job.sequence + 1,
        task_id: job.task_id,
        step_id: job.step_id,
        attempt_id: job.attempt_id,
        lease_id: job.lease_id,
        cancellation_id: job.cancellation_id,
        deadline_after_ms: CANCELLATION_ACK_DEADLINE_MS,
    };
    instruction
        .validate_for_job(&job)
        .expect("exact cancellation instruction");
    let acknowledgement = CancellationAcknowledgement {
        protocol_version: PROTOCOL_VERSION,
        connection_epoch: instruction.connection_epoch,
        sequence: instruction.sequence + 1,
        task_id: instruction.task_id,
        step_id: instruction.step_id,
        attempt_id: instruction.attempt_id,
        lease_id: instruction.lease_id,
        cancellation_id: instruction.cancellation_id,
        status: CancellationAcknowledgementStatus::Cancelled,
    };
    acknowledgement
        .validate_for_instruction(&instruction)
        .expect("exact cancellation acknowledgement");

    let mut wrong_device_epoch = acknowledgement;
    wrong_device_epoch.connection_epoch += 1;
    assert_eq!(
        wrong_device_epoch.validate_for_instruction(&instruction),
        Err(ProtocolError::ResultIdentityMismatch)
    );
}

#[test]
fn fixture_result_revalidates_the_stored_job_and_exact_output_contract() {
    let job = fixture_job();
    let payload = json!({"operation":"synthetic_echo","output":"hello","synthetic":true});
    let mut result = JobResultEnvelope {
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
        payload_sha256: Sha256::digest(serde_json::to_vec(&payload).unwrap()).into(),
        payload,
    };
    result
        .validate_fixture_reasoning_result(&job)
        .expect("exact fixture result");
    result.payload["output"] = json!("different");
    result.payload_sha256 = Sha256::digest(serde_json::to_vec(&result.payload).unwrap()).into();
    assert_eq!(
        result.validate_fixture_reasoning_result(&job),
        Err(ProtocolError::InvalidFixtureJob)
    );
}

// The fixture capability's provider and model are protocol-version-1 wire
// values, not cosmetic names. An enrolled Mac agent advertises them verbatim and
// the master rejects anything that does not match exactly, so renaming them is a
// breaking wire change that needs a protocol version bump. The Assemblywright
// rename deliberately left them alone; pin them so a later rename pass fails
// here instead of against an enrolled device.
#[test]
fn fixture_capability_identity_is_a_frozen_protocol_v1_contract() {
    assert_eq!(PROTOCOL_VERSION, 1);
    assert_eq!(FIXTURE_REASONING_CAPABILITY_ID, "fixture.reasoning");
    assert_eq!(FIXTURE_REASONING_PROVIDER, "jarvis-fixture");
    assert_eq!(FIXTURE_REASONING_MODEL, "jarvis-fixture-v1");

    let advertised = CapabilityDescriptor::fixture_reasoning();
    assert_eq!(advertised.id, FIXTURE_REASONING_CAPABILITY_ID);
    assert_eq!(advertised.provider, FIXTURE_REASONING_PROVIDER);
    assert_eq!(advertised.model, FIXTURE_REASONING_MODEL);

    advertised.validate().expect("frozen fixture capability");

    // A renamed provider or model must fail closed rather than be tolerated.
    let mut renamed_provider = CapabilityDescriptor::fixture_reasoning();
    renamed_provider.provider = "assemblywright-fixture".to_string();
    assert_eq!(
        renamed_provider.validate(),
        Err(ProtocolError::InvalidFixtureCapability)
    );
    let mut renamed_model = CapabilityDescriptor::fixture_reasoning();
    renamed_model.model = "assemblywright-fixture-v1".to_string();
    assert_eq!(
        renamed_model.validate(),
        Err(ProtocolError::InvalidFixtureCapability)
    );
}
