use assemblywright_protocol::{
    feature_conveyor_publication_request_binding_sha256,
    feature_conveyor_publication_required_checks_sha256, FeatureConveyorGrantRevisions,
    FeatureConveyorPublicationAction, FeatureConveyorPublicationActionEvidence,
    FeatureConveyorPublicationRequest, FEATURE_CONVEYOR_PUBLICATION_COORDINATOR_SCHEMA_VERSION,
};
use uuid::Uuid;

fn request() -> FeatureConveyorPublicationRequest {
    FeatureConveyorPublicationRequest {
        schema_version: FEATURE_CONVEYOR_PUBLICATION_COORDINATOR_SCHEMA_VERSION,
        publication_id: Uuid::from_u128(1),
        feature_id: Uuid::from_u128(2),
        specification_revision: 3,
        expected_lifecycle_revision: 4,
        feature_lease_id: Uuid::from_u128(5),
        integration_id: Uuid::from_u128(6),
        validation_id: Uuid::from_u128(7),
        review_call_id: Uuid::from_u128(8),
        candidate_commit: "1111111111111111111111111111111111111111".to_string(),
        candidate_tree: "2222222222222222222222222222222222222222".to_string(),
        candidate_diff_sha256: [9; 32],
        evidence_manifest_sha256: [10; 32],
        review_decision_sha256: [11; 32],
        provider_id: "local.review".to_string(),
        model_id: "review-v1".to_string(),
        remote_base_commit: "3333333333333333333333333333333333333333".to_string(),
        branch_policy_sha256: [12; 32],
        expected_queue_revision: 13,
        expected_emergency_pause_revision: 14,
        grants: FeatureConveyorGrantRevisions {
            registration: 15,
            cloud_disclosure: 16,
            autonomous_publication: 17,
        },
    }
}

#[test]
fn publication_request_is_strict_path_free_and_every_binding_changes_identity() {
    let request = request();
    request.validate().unwrap();
    let bytes = serde_json::to_vec(&request).unwrap();
    let decoded = FeatureConveyorPublicationRequest::decode_frame(&bytes).unwrap();
    assert_eq!(decoded, request);
    let text = String::from_utf8(bytes).unwrap();
    for forbidden in ["path", "command", "credential", "token", "output"] {
        assert!(!text.contains(forbidden));
    }

    let baseline = feature_conveyor_publication_request_binding_sha256(&request).unwrap();
    type Mutation = Box<dyn Fn(&mut FeatureConveyorPublicationRequest)>;
    let mut mutations: Vec<Mutation> = vec![
        Box::new(|value| value.specification_revision += 1),
        Box::new(|value| value.expected_lifecycle_revision += 1),
        Box::new(|value| value.candidate_commit.replace_range(..1, "4")),
        Box::new(|value| value.candidate_diff_sha256 = [18; 32]),
        Box::new(|value| value.evidence_manifest_sha256 = [19; 32]),
        Box::new(|value| value.review_decision_sha256 = [20; 32]),
        Box::new(|value| value.provider_id = "other.review".to_string()),
        Box::new(|value| value.remote_base_commit.replace_range(..1, "5")),
        Box::new(|value| value.branch_policy_sha256 = [21; 32]),
        Box::new(|value| value.expected_queue_revision += 1),
        Box::new(|value| value.expected_emergency_pause_revision += 1),
        Box::new(|value| value.grants.autonomous_publication += 1),
    ];
    for mutation in &mut mutations {
        let mut changed = request.clone();
        mutation(&mut changed);
        assert_ne!(
            feature_conveyor_publication_request_binding_sha256(&changed).unwrap(),
            baseline
        );
    }
}

#[test]
fn publication_request_rejects_unknown_fields_and_malformed_evidence() {
    let mut value = serde_json::to_value(request()).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("credential".to_string(), serde_json::json!("secret"));
    assert!(
        FeatureConveyorPublicationRequest::decode_frame(&serde_json::to_vec(&value).unwrap())
            .is_err()
    );

    let mut malformed = request();
    malformed.review_decision_sha256 = [0; 32];
    assert!(malformed.validate().is_err());
}

#[test]
fn publication_action_evidence_is_stage_specific_self_bound_and_no_bypass() {
    let checks = vec!["release-local".to_string(), "windows-protocol".to_string()];
    let evidence = FeatureConveyorPublicationActionEvidence {
        schema_version: FEATURE_CONVEYOR_PUBLICATION_COORDINATOR_SCHEMA_VERSION,
        publication_id: Uuid::from_u128(1),
        action: FeatureConveyorPublicationAction::MergePullRequest,
        remote_base_commit: "1111111111111111111111111111111111111111".to_string(),
        candidate_commit: "2222222222222222222222222222222222222222".to_string(),
        feature_branch: "assemblywright-feature".to_string(),
        base_branch: "main".to_string(),
        pull_request_number: Some(42),
        observed_head_commit: "2222222222222222222222222222222222222222".to_string(),
        required_checks_sha256: Some(
            feature_conveyor_publication_required_checks_sha256(&checks).unwrap(),
        ),
        required_check_count: 2,
        required_checks_passed: true,
        branch_protection_enforced: true,
        bypass_used: false,
        merge_strategy: Some("merge".to_string()),
        resulting_main_commit: Some("3333333333333333333333333333333333333333".to_string()),
        post_merge_gate_id: None,
        post_merge_gate_passed: false,
        evidence_sha256: [0; 32],
    }
    .seal()
    .unwrap();
    evidence.validate().unwrap();

    let mut tampered = evidence.clone();
    tampered.observed_head_commit = "4444444444444444444444444444444444444444".to_string();
    assert!(tampered.validate().is_err());
    let mut bypass = evidence.clone();
    bypass.bypass_used = true;
    bypass.evidence_sha256 = [0; 32];
    assert!(bypass.seal().is_err());
    let mut incomplete_checks = evidence;
    incomplete_checks.required_check_count = 0;
    incomplete_checks.evidence_sha256 = [0; 32];
    assert!(incomplete_checks.seal().is_err());

    let reversed = vec!["windows-protocol".to_string(), "release-local".to_string()];
    assert_eq!(
        feature_conveyor_publication_required_checks_sha256(&checks).unwrap(),
        feature_conveyor_publication_required_checks_sha256(&reversed).unwrap()
    );
    assert!(feature_conveyor_publication_required_checks_sha256(&[
        "release-local".to_string(),
        "release-local".to_string(),
    ])
    .is_err());
}
