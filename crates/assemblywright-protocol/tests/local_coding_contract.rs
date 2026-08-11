use assemblywright_protocol::{
    AttemptId, CancellationId, CapabilityDescriptor, CapabilityKind, ContextHandlingPolicy,
    DeviceId, FeatureConveyorCodingDispatchRequest, FeatureConveyorCodingWorkPacketMetadata,
    JobEnvelope, JobResultEnvelope, JobResultStatus, LeaseId, LocalCodingJobRequest,
    LocalCodingJobResult, LocalCodingSnapshotChunk, LocalCodingSnapshotChunkRequest, ProtocolError,
    Sensitivity, StepId, TaskId, FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
    LOCAL_CODING_CAPABILITY_ID, LOCAL_CODING_MODEL, LOCAL_CODING_PROVIDER,
    MAX_FEATURE_CONVEYOR_CODING_DISPATCH_REQUEST_BYTES, PROTOCOL_VERSION,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn request() -> FeatureConveyorCodingDispatchRequest {
    FeatureConveyorCodingDispatchRequest {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        feature_id: Uuid::new_v4(),
        specification_revision: 1,
        expected_lifecycle_revision: 2,
        feature_lease_id: Uuid::new_v4(),
        snapshot_id: Uuid::new_v4(),
        snapshot_sha256: [1; 32],
        work_packet_sha256: [2; 32],
        work_packet: FeatureConveyorCodingWorkPacketMetadata {
            packet_id: Uuid::new_v4(),
            ordinal: 1,
            acceptance_criteria_count: 2,
        },
        device_id: DeviceId::new(Uuid::new_v4()),
        device_registry_revision: 3,
        expected_queue_revision: 4,
        expected_emergency_pause_revision: 5,
    }
}

fn coding_job(owner: &FeatureConveyorCodingDispatchRequest) -> JobEnvelope {
    let context = serde_json::to_value(LocalCodingJobRequest {
        feature_id: owner.feature_id,
        specification_revision: owner.specification_revision,
        lifecycle_revision: owner.expected_lifecycle_revision,
        feature_lease_id: owner.feature_lease_id,
        snapshot_id: owner.snapshot_id,
        snapshot_sha256: owner.snapshot_sha256,
        work_packet_sha256: owner.work_packet_sha256,
        work_packet: owner.work_packet.clone(),
        device_id: owner.device_id,
        device_registry_revision: owner.device_registry_revision,
        queue_revision: owner.expected_queue_revision,
        emergency_pause_revision: owner.expected_emergency_pause_revision,
    })
    .unwrap();
    JobEnvelope {
        protocol_version: PROTOCOL_VERSION,
        connection_epoch: 1,
        sequence: 1,
        task_id: TaskId::new(Uuid::new_v4()),
        step_id: StepId::new(Uuid::new_v4()),
        attempt_id: AttemptId::new(Uuid::new_v4()),
        lease_id: LeaseId::new(Uuid::new_v4()),
        cancellation_id: CancellationId::new(Uuid::new_v4()),
        capability_id: LOCAL_CODING_CAPABILITY_ID.to_string(),
        selected_model: LOCAL_CODING_MODEL.to_string(),
        sensitivity: Sensitivity::Workspace,
        context_handling: ContextHandlingPolicy::EphemeralNoRetention,
        lease_duration_ms: 60_000,
        deadline_after_ms: 60_000,
        context_sha256: Sha256::digest(serde_json::to_vec(&context).unwrap()).into(),
        context,
    }
}

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

#[test]
fn local_coding_capability_is_one_exact_contract() {
    let capability = CapabilityDescriptor::local_coding();
    capability.validate().unwrap();
    assert_eq!(capability.id, LOCAL_CODING_CAPABILITY_ID);
    assert_eq!(capability.kind, CapabilityKind::LocalCoding);
    assert_eq!(capability.provider, LOCAL_CODING_PROVIDER);
    assert_eq!(capability.model, LOCAL_CODING_MODEL);
    for mutate in [
        |value: &mut CapabilityDescriptor| value.kind = CapabilityKind::LocalInference,
        |value: &mut CapabilityDescriptor| value.provider.push_str("-cloud"),
        |value: &mut CapabilityDescriptor| value.model.push_str("-drifted"),
        |value: &mut CapabilityDescriptor| value.max_context_bytes += 1,
        |value: &mut CapabilityDescriptor| value.max_result_bytes += 1,
    ] {
        let mut invalid = capability.clone();
        mutate(&mut invalid);
        assert_eq!(
            invalid.validate(),
            Err(ProtocolError::InvalidLocalCodingCapability)
        );
    }
}

#[test]
fn owner_dispatch_rejects_zero_authority_bindings_and_accepts_numeric_maxima() {
    let invalid_mutations: [fn(&mut FeatureConveyorCodingDispatchRequest); 12] = [
        |value| value.schema_version = 0,
        |value| value.feature_id = Uuid::nil(),
        |value| value.specification_revision = 0,
        |value| value.expected_lifecycle_revision = 0,
        |value| value.feature_lease_id = Uuid::nil(),
        |value| value.snapshot_id = Uuid::nil(),
        |value| value.snapshot_sha256 = [0; 32],
        |value| value.work_packet.packet_id = Uuid::nil(),
        |value| value.work_packet.ordinal = 0,
        |value| value.work_packet.acceptance_criteria_count = 0,
        |value| value.device_id = DeviceId::new(Uuid::nil()),
        |value| value.device_registry_revision = 0,
    ];
    for mutate in invalid_mutations {
        let mut invalid = request();
        mutate(&mut invalid);
        assert!(invalid.validate().is_err());
    }

    let mut maximum = request();
    maximum.specification_revision = u64::MAX;
    maximum.expected_lifecycle_revision = u64::MAX;
    maximum.device_registry_revision = u64::MAX;
    maximum.expected_queue_revision = u64::MAX;
    maximum.expected_emergency_pause_revision = u64::MAX;
    maximum.work_packet.ordinal = u16::MAX;
    maximum.work_packet.acceptance_criteria_count = u16::MAX;
    maximum.validate().unwrap();
}

#[test]
fn owner_dispatch_is_strict_bounded_digest_bound_and_path_free() {
    let valid = request();
    valid.validate().unwrap();
    let encoded = serde_json::to_vec(&valid).unwrap();
    assert_eq!(
        FeatureConveyorCodingDispatchRequest::decode_frame(&encoded).unwrap(),
        valid
    );
    let encoded_text = String::from_utf8(encoded).unwrap();
    assert!(!encoded_text.contains("repository_path"));
    assert!(!encoded_text.contains("allowed_paths"));
    assert!(!encoded_text.contains("provider"));
    let unknown = encoded_text.replacen(
        "\"feature_id\":",
        "\"repository_path\":\"/private/repo\",\"feature_id\":",
        1,
    );
    assert!(FeatureConveyorCodingDispatchRequest::decode_frame(unknown.as_bytes()).is_err());
    let mut zero = valid;
    zero.work_packet_sha256 = [0; 32];
    assert!(zero.validate().is_err());
    assert!(matches!(
        FeatureConveyorCodingDispatchRequest::decode_frame(&vec![
            b' ';
            MAX_FEATURE_CONVEYOR_CODING_DISPATCH_REQUEST_BYTES
                + 1
        ]),
        Err(ProtocolError::FrameTooLarge { .. })
    ));
}

#[test]
fn coding_job_and_ack_are_exact_attempt_bound_and_forbid_mutation_claims() {
    let owner = request();
    let mut job = coding_job(&owner);
    job.validate_local_coding().unwrap();
    job.context["repository_path"] = json!("/private/repo");
    job.context_sha256 = Sha256::digest(serde_json::to_vec(&job.context).unwrap()).into();
    assert_eq!(
        job.validate_local_coding(),
        Err(ProtocolError::InvalidLocalCodingJob)
    );

    job.context
        .as_object_mut()
        .unwrap()
        .remove("repository_path");
    job.context_sha256 = Sha256::digest(serde_json::to_vec(&job.context).unwrap()).into();
    let payload = serde_json::to_value(LocalCodingJobResult {
        status: "snapshot_materialized".to_string(),
        work_packet_sha256: owner.work_packet_sha256,
        admission_sha256: [9; 32],
        snapshot_sha256: owner.snapshot_sha256,
        mutation_performed: false,
    })
    .unwrap();
    let result = JobResultEnvelope {
        protocol_version: PROTOCOL_VERSION,
        connection_epoch: job.connection_epoch,
        sequence: 2,
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
    result.validate_local_coding_result(&job).unwrap();
    let valid_result = result;
    let refresh_digest = |value: &mut JobResultEnvelope| {
        value.payload_sha256 = Sha256::digest(serde_json::to_vec(&value.payload).unwrap()).into();
    };
    let mut wrong_envelope_status = valid_result.clone();
    wrong_envelope_status.status = JobResultStatus::Failed;
    let mut wrong_result_status = valid_result.clone();
    wrong_result_status.payload["status"] = json!("completed");
    refresh_digest(&mut wrong_result_status);
    let mut wrong_packet = valid_result.clone();
    wrong_packet.payload["work_packet_sha256"] = serde_json::to_value([8_u8; 32]).unwrap();
    refresh_digest(&mut wrong_packet);
    let mut zero_admission = valid_result.clone();
    zero_admission.payload["admission_sha256"] = serde_json::to_value([0_u8; 32]).unwrap();
    refresh_digest(&mut zero_admission);
    let mut mutation_claim = valid_result.clone();
    mutation_claim.payload["mutation_performed"] = json!(true);
    refresh_digest(&mut mutation_claim);
    let mut unknown_payload_field = valid_result;
    unknown_payload_field.payload["repository_path"] = json!("/private/repo");
    refresh_digest(&mut unknown_payload_field);
    for invalid in [
        wrong_envelope_status,
        wrong_result_status,
        wrong_packet,
        zero_admission,
        mutation_claim,
        unknown_payload_field,
    ] {
        assert_eq!(
            invalid.validate_local_coding_result(&job),
            Err(ProtocolError::InvalidLocalCodingResult)
        );
    }
}

#[test]
fn snapshot_chunks_are_strict_sequential_digest_and_attempt_bound() {
    let owner = request();
    let job = coding_job(&owner);
    let request = LocalCodingSnapshotChunkRequest {
        protocol_version: job.protocol_version,
        connection_epoch: job.connection_epoch,
        task_id: job.task_id,
        step_id: job.step_id,
        attempt_id: job.attempt_id,
        lease_id: job.lease_id,
        cancellation_id: job.cancellation_id,
        snapshot_id: owner.snapshot_id,
        snapshot_sha256: owner.snapshot_sha256,
        offset: 0,
    };
    request.validate_for_job(&job).unwrap();
    let content = b"bounded snapshot chunk";
    let chunk = LocalCodingSnapshotChunk {
        protocol_version: request.protocol_version,
        connection_epoch: request.connection_epoch,
        task_id: request.task_id,
        step_id: request.step_id,
        attempt_id: request.attempt_id,
        lease_id: request.lease_id,
        cancellation_id: request.cancellation_id,
        snapshot_id: request.snapshot_id,
        snapshot_sha256: request.snapshot_sha256,
        offset: request.offset,
        total_bytes: content.len() as u64,
        content_sha256: Sha256::digest(content).into(),
        content_hex: lower_hex(content),
        complete: true,
    };
    chunk.validate_for_request(&request).unwrap();
    assert_eq!(chunk.decode_content().unwrap(), content);
    assert_eq!(
        LocalCodingSnapshotChunk::decode_frame(&serde_json::to_vec(&chunk).unwrap()).unwrap(),
        chunk
    );

    let mut wrong_attempt = request.clone();
    wrong_attempt.attempt_id = AttemptId::new(Uuid::new_v4());
    assert_eq!(
        wrong_attempt.validate_for_job(&job),
        Err(ProtocolError::InvalidLocalCodingSnapshotTransfer)
    );
    let mut wrong_offset = chunk.clone();
    wrong_offset.offset = 1;
    assert!(wrong_offset.validate().is_err());
    let mut uppercase = chunk.clone();
    uppercase.content_hex.make_ascii_uppercase();
    assert!(uppercase.validate().is_err());
    let mut corrupt = chunk.clone();
    corrupt.content_sha256 = [7; 32];
    assert!(corrupt.validate().is_err());
    let unknown = String::from_utf8(serde_json::to_vec(&chunk).unwrap())
        .unwrap()
        .replacen(
            "\"offset\":",
            "\"repository_path\":\"C:/private\",\"offset\":",
            1,
        );
    assert!(LocalCodingSnapshotChunk::decode_frame(unknown.as_bytes()).is_err());
}
