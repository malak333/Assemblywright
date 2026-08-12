use assemblywright_protocol::{
    FeatureConveyorArtifactIntegrationPlan, FeatureConveyorArtifactIntegrationReceipt,
    FeatureConveyorArtifactIntegrationRequest, FeatureConveyorArtifactIntegrationStatus,
    FeatureConveyorGrantRevisions, FEATURE_CONVEYOR_ARTIFACT_INTEGRATION_SCHEMA_VERSION,
};
use uuid::Uuid;

fn request() -> FeatureConveyorArtifactIntegrationRequest {
    FeatureConveyorArtifactIntegrationRequest {
        schema_version: FEATURE_CONVEYOR_ARTIFACT_INTEGRATION_SCHEMA_VERSION,
        integration_id: Uuid::from_u128(1),
        feature_id: Uuid::from_u128(2),
        specification_revision: 1,
        expected_lifecycle_revision: 2,
        feature_lease_id: Uuid::from_u128(3),
        snapshot_id: Uuid::from_u128(4),
        snapshot_sha256: [1; 32],
        artifact_ids: vec![Uuid::from_u128(5), Uuid::from_u128(6)],
        expected_queue_revision: 2,
        expected_emergency_pause_revision: 0,
        grants: FeatureConveyorGrantRevisions {
            registration: 1,
            cloud_disclosure: 1,
            autonomous_publication: 1,
        },
        base_commit: "1234567890abcdef1234567890abcdef12345678".to_string(),
    }
}

#[test]
fn integration_plan_is_strict_and_path_free() {
    let request = request();
    let plan = FeatureConveyorArtifactIntegrationPlan {
        schema_version: 1,
        feature_id: request.feature_id,
        specification_revision: request.specification_revision,
        lifecycle_revision: request.expected_lifecycle_revision,
        feature_lease_id: request.feature_lease_id,
        snapshot_id: request.snapshot_id,
        snapshot_sha256: request.snapshot_sha256,
        artifact_ids: request.artifact_ids,
        queue_revision: request.expected_queue_revision,
        emergency_pause_revision: request.expected_emergency_pause_revision,
        grants: request.grants,
        base_commit: request.base_commit,
    };
    let encoded = serde_json::to_vec(&plan).unwrap();
    assert_eq!(
        FeatureConveyorArtifactIntegrationPlan::decode_frame(&encoded).unwrap(),
        plan
    );
    let unknown =
        String::from_utf8(encoded)
            .unwrap()
            .replacen("{", "{\"repository_path\":\"secret\",", 1);
    assert!(FeatureConveyorArtifactIntegrationPlan::decode_frame(unknown.as_bytes()).is_err());
}

#[test]
fn integration_request_is_strict_sorted_bounded_and_nonnil() {
    let request = request();
    let encoded = serde_json::to_vec(&request).unwrap();
    assert_eq!(
        FeatureConveyorArtifactIntegrationRequest::decode_frame(&encoded).unwrap(),
        request
    );
    let duplicate = String::from_utf8(encoded.clone()).unwrap().replacen(
        "\"integration_id\":",
        "\"integration_id\":\"00000000-0000-0000-0000-000000000001\",\"integration_id\":",
        1,
    );
    assert!(FeatureConveyorArtifactIntegrationRequest::decode_frame(duplicate.as_bytes()).is_err());
    let unknown =
        String::from_utf8(encoded)
            .unwrap()
            .replacen("{", "{\"repository_path\":\"secret\",", 1);
    assert!(FeatureConveyorArtifactIntegrationRequest::decode_frame(unknown.as_bytes()).is_err());
    for ids in [
        vec![],
        vec![Uuid::from_u128(6), Uuid::from_u128(5)],
        vec![Uuid::from_u128(5), Uuid::from_u128(5)],
        vec![Uuid::nil()],
    ] {
        let mut invalid = request.clone();
        invalid.artifact_ids = ids;
        assert!(invalid.validate().is_err());
    }
    let mut too_many = request;
    too_many.artifact_ids = (5..=8).map(Uuid::from_u128).collect();
    assert!(too_many.validate().is_err());
}

#[test]
fn integration_receipt_strictly_binds_candidate_and_rejects_malformed() {
    let request = request();
    let receipt = FeatureConveyorArtifactIntegrationReceipt {
        schema_version: 1,
        integration_id: request.integration_id,
        feature_id: request.feature_id,
        specification_revision: 1,
        lifecycle_revision: 3,
        feature_lease_id: request.feature_lease_id,
        snapshot_id: request.snapshot_id,
        snapshot_sha256: request.snapshot_sha256,
        artifact_set_sha256: [2; 32],
        candidate_commit: request.base_commit.clone(),
        candidate_tree: "abcdef1234567890abcdef1234567890abcdef12".to_string(),
        base_commit: request.base_commit,
        queue_revision: 2,
        emergency_pause_revision: 0,
        grants: request.grants,
        status: FeatureConveyorArtifactIntegrationStatus::CandidateFrozen,
    };
    let encoded = serde_json::to_vec(&receipt).unwrap();
    assert_eq!(
        FeatureConveyorArtifactIntegrationReceipt::decode_frame(&encoded).unwrap(),
        receipt
    );
    let unknown = String::from_utf8(encoded.clone()).unwrap().replacen(
        "{",
        "{\"private_path\":\"secret\",",
        1,
    );
    assert!(FeatureConveyorArtifactIntegrationReceipt::decode_frame(unknown.as_bytes()).is_err());
    let duplicate = String::from_utf8(encoded).unwrap().replacen(
        "\"candidate_commit\":",
        "\"candidate_commit\":\"1234567890abcdef1234567890abcdef12345678\",\"candidate_commit\":",
        1,
    );
    assert!(FeatureConveyorArtifactIntegrationReceipt::decode_frame(duplicate.as_bytes()).is_err());
    let mut invalid = receipt;
    invalid.candidate_tree = "not-a-tree".to_string();
    assert!(invalid.validate().is_err());
}
