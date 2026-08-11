use assemblywright_agent::validate_local_coding_dispatch;
use assemblywright_protocol::{
    AttemptId, CancellationId, ContextHandlingPolicy, DeviceId,
    FeatureConveyorCodingWorkPacketMetadata, JobEnvelope, LeaseId, LocalCodingJobRequest,
    ProtocolError, Sensitivity, StepId, TaskId, LOCAL_CODING_CAPABILITY_ID, LOCAL_CODING_MODEL,
    PROTOCOL_VERSION,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn job() -> JobEnvelope {
    let context = serde_json::to_value(LocalCodingJobRequest {
        feature_id: Uuid::new_v4(),
        specification_revision: 1,
        lifecycle_revision: 2,
        feature_lease_id: Uuid::new_v4(),
        snapshot_id: Uuid::new_v4(),
        snapshot_sha256: [1; 32],
        work_packet_sha256: [2; 32],
        work_packet: FeatureConveyorCodingWorkPacketMetadata {
            packet_id: Uuid::new_v4(),
            ordinal: 1,
            acceptance_criteria_count: 1,
        },
        device_id: DeviceId::new(Uuid::new_v4()),
        device_registry_revision: 1,
        queue_revision: 2,
        emergency_pause_revision: 0,
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

#[test]
fn native_agent_admits_only_path_free_snapshot_bound_metadata_without_executing_it() {
    let valid = job();
    let admitted = validate_local_coding_dispatch(&valid).unwrap();
    assert_eq!(admitted.snapshot_sha256, [1; 32]);
    assert_eq!(admitted.work_packet_sha256, [2; 32]);

    let mut injected = valid;
    injected.context["repository_path"] = json!("/private/canonical-repository");
    injected.context_sha256 = Sha256::digest(serde_json::to_vec(&injected.context).unwrap()).into();
    assert_eq!(
        validate_local_coding_dispatch(&injected),
        Err(ProtocolError::InvalidLocalCodingJob)
    );
}
