use assemblywright_protocol::{
    feature_conveyor_validation_plan_sha256, feature_conveyor_validation_request_binding_sha256,
    FeatureConveyorGrantRevisions, FeatureConveyorValidationCommandId,
    FeatureConveyorValidationGateRequest,
};
use uuid::Uuid;

fn request() -> FeatureConveyorValidationGateRequest {
    let command_ids = FeatureConveyorValidationCommandId::REQUIRED.to_vec();
    FeatureConveyorValidationGateRequest {
        schema_version: 1,
        validation_id: Uuid::from_u128(1),
        feature_id: Uuid::from_u128(2),
        specification_revision: 3,
        expected_lifecycle_revision: 4,
        feature_lease_id: Uuid::from_u128(5),
        snapshot_id: Uuid::from_u128(6),
        snapshot_sha256: [7; 32],
        integration_id: Uuid::from_u128(8),
        artifact_set_sha256: [9; 32],
        candidate_commit: "1111111111111111111111111111111111111111".to_string(),
        candidate_tree: "2222222222222222222222222222222222222222".to_string(),
        base_commit: "3333333333333333333333333333333333333333".to_string(),
        plan_sha256: feature_conveyor_validation_plan_sha256(&command_ids).unwrap(),
        command_ids,
        expected_queue_revision: 10,
        expected_emergency_pause_revision: 11,
        grants: FeatureConveyorGrantRevisions {
            registration: 12,
            cloud_disclosure: 13,
            autonomous_publication: 14,
        },
    }
}

#[test]
fn strict_validation_request_round_trips_and_binding_is_canonical() {
    let request = request();
    let encoded = serde_json::to_vec(&request).unwrap();
    assert_eq!(
        FeatureConveyorValidationGateRequest::decode_frame(&encoded).unwrap(),
        request
    );
    let reordered = serde_json::json!({
        "validation_id": request.validation_id,
        "schema_version": request.schema_version,
        "feature_id": request.feature_id,
        "specification_revision": request.specification_revision,
        "expected_lifecycle_revision": request.expected_lifecycle_revision,
        "feature_lease_id": request.feature_lease_id,
        "snapshot_id": request.snapshot_id,
        "snapshot_sha256": request.snapshot_sha256,
        "integration_id": request.integration_id,
        "artifact_set_sha256": request.artifact_set_sha256,
        "candidate_commit": request.candidate_commit,
        "candidate_tree": request.candidate_tree,
        "base_commit": request.base_commit,
        "command_ids": request.command_ids,
        "plan_sha256": request.plan_sha256,
        "expected_queue_revision": request.expected_queue_revision,
        "expected_emergency_pause_revision": request.expected_emergency_pause_revision,
        "grants": request.grants,
    });
    let decoded = FeatureConveyorValidationGateRequest::decode_frame(
        &serde_json::to_vec(&reordered).unwrap(),
    )
    .unwrap();
    assert_eq!(
        feature_conveyor_validation_request_binding_sha256(&decoded).unwrap(),
        feature_conveyor_validation_request_binding_sha256(&request).unwrap()
    );
}

#[test]
fn validation_request_rejects_missing_reordered_unknown_and_digest_drift() {
    let valid = request();
    let mut missing = valid.clone();
    missing.command_ids.pop();
    assert!(missing.validate().is_err());

    let mut reordered = valid.clone();
    reordered.command_ids.swap(0, 1);
    assert!(reordered.validate().is_err());

    let mut drift = valid.clone();
    drift.plan_sha256[0] ^= 1;
    assert!(drift.validate().is_err());

    let mut unknown = serde_json::to_value(valid).unwrap();
    unknown
        .as_object_mut()
        .unwrap()
        .insert("command".to_string(), serde_json::json!("cargo test"));
    assert!(FeatureConveyorValidationGateRequest::decode_frame(
        &serde_json::to_vec(&unknown).unwrap()
    )
    .is_err());
}
