use assemblywright_protocol::{
    feature_conveyor_review_request_binding_sha256, FeatureConveyorGrantRevisions,
    FeatureConveyorKnowledgeBaseDetermination, FeatureConveyorReviewCoverageStatus,
    FeatureConveyorReviewDecision, FeatureConveyorReviewFinding,
    FeatureConveyorReviewGatewayRequest, FeatureConveyorReviewPacket,
    FeatureConveyorReviewProviderOutput, FeatureConveyorReviewRequirementCoverage,
    FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn request() -> FeatureConveyorReviewGatewayRequest {
    FeatureConveyorReviewGatewayRequest {
        schema_version: FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION,
        review_call_id: Uuid::from_u128(1),
        feature_id: Uuid::from_u128(2),
        specification_revision: 3,
        expected_lifecycle_revision: 4,
        feature_lease_id: Uuid::from_u128(5),
        integration_id: Uuid::from_u128(6),
        validation_id: Uuid::from_u128(7),
        candidate_commit: "1111111111111111111111111111111111111111".to_string(),
        candidate_tree: "2222222222222222222222222222222222222222".to_string(),
        base_commit: "3333333333333333333333333333333333333333".to_string(),
        candidate_diff_sha256: [8; 32],
        evidence_manifest_sha256: [9; 32],
        review_packet_sha256: [10; 32],
        provider_id: "local.review".to_string(),
        model_id: "review-v1".to_string(),
        expected_queue_revision: 11,
        expected_emergency_pause_revision: 12,
        grants: FeatureConveyorGrantRevisions {
            registration: 13,
            cloud_disclosure: 14,
            autonomous_publication: 15,
        },
    }
}

fn packet() -> FeatureConveyorReviewPacket {
    let approved_specification = json!({"outcome":"bounded review"});
    let specification_bytes = b"{\"outcome\":\"bounded review\"}";
    let candidate_diff = "diff --git a/README.md b/README.md\n".to_string();
    FeatureConveyorReviewPacket {
        schema_version: FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION,
        feature_id: Uuid::from_u128(2),
        specification_revision: 3,
        approved_specification,
        approved_specification_sha256: Sha256::digest(specification_bytes).into(),
        candidate_commit: "1111111111111111111111111111111111111111".to_string(),
        candidate_tree: "2222222222222222222222222222222222222222".to_string(),
        base_commit: "3333333333333333333333333333333333333333".to_string(),
        candidate_diff_sha256: Sha256::digest(candidate_diff.as_bytes()).into(),
        candidate_diff,
        evidence_manifest_sha256: [9; 32],
        evidence_digests: vec![[9; 32], [16; 32]],
        requirements_sha256: [17; 32],
        requirement_ids: vec!["requirement-1".to_string()],
        provider_id: "local.review".to_string(),
        model_id: "review-v1".to_string(),
        grants: FeatureConveyorGrantRevisions {
            registration: 13,
            cloud_disclosure: 14,
            autonomous_publication: 15,
        },
    }
}

fn approved_output(packet_sha256: [u8; 32]) -> FeatureConveyorReviewProviderOutput {
    FeatureConveyorReviewProviderOutput {
        schema_version: FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION,
        review_packet_sha256: packet_sha256,
        provider_id: "local.review".to_string(),
        model_id: "review-v1".to_string(),
        decision: FeatureConveyorReviewDecision::Approved,
        blocking_findings: vec![],
        non_blocking_findings: vec![FeatureConveyorReviewFinding {
            finding_id: "observation-1".to_string(),
            requirement_id: "requirement-1".to_string(),
            evidence_sha256: [18; 32],
        }],
        requirement_coverage: vec![FeatureConveyorReviewRequirementCoverage {
            requirement_id: "requirement-1".to_string(),
            status: FeatureConveyorReviewCoverageStatus::Covered,
            evidence_sha256: [19; 32],
        }],
        evidence_digests: vec![[9; 32], [16; 32]],
        knowledge_base_determination: FeatureConveyorKnowledgeBaseDetermination::Updated,
        knowledge_base_evidence_sha256: [20; 32],
    }
}

#[test]
fn strict_review_request_round_trips_and_binds_canonically() {
    let request = request();
    let encoded = serde_json::to_vec(&request).unwrap();
    assert_eq!(
        FeatureConveyorReviewGatewayRequest::decode_frame(&encoded).unwrap(),
        request
    );
    let mut reordered = serde_json::to_value(&request).unwrap();
    let provider = reordered
        .as_object_mut()
        .unwrap()
        .remove("provider_id")
        .unwrap();
    reordered
        .as_object_mut()
        .unwrap()
        .insert("provider_id".to_string(), provider);
    let decoded =
        FeatureConveyorReviewGatewayRequest::decode_frame(&serde_json::to_vec(&reordered).unwrap())
            .unwrap();
    assert_eq!(
        feature_conveyor_review_request_binding_sha256(&decoded).unwrap(),
        feature_conveyor_review_request_binding_sha256(&request).unwrap()
    );
}

#[test]
fn review_request_rejects_unknown_zero_and_unbounded_fields() {
    let mut unknown = serde_json::to_value(request()).unwrap();
    unknown
        .as_object_mut()
        .unwrap()
        .insert("transcript".to_string(), json!("forbidden"));
    assert!(FeatureConveyorReviewGatewayRequest::decode_frame(
        &serde_json::to_vec(&unknown).unwrap()
    )
    .is_err());

    let mut zero = request();
    zero.review_packet_sha256 = [0; 32];
    assert!(zero.validate().is_err());

    let mut invalid_provider = request();
    invalid_provider.provider_id = "provider with spaces".to_string();
    assert!(invalid_provider.validate().is_err());
}

#[test]
fn review_packet_binds_exact_spec_diff_evidence_and_size() {
    let packet = packet();
    assert!(packet.validate().is_ok());
    let encoded = packet.canonical_bytes().unwrap();
    assert_eq!(
        FeatureConveyorReviewPacket::decode_frame(&encoded).unwrap(),
        packet
    );
    let expected: [u8; 32] = Sha256::digest(packet.canonical_bytes().unwrap()).into();
    assert_eq!(packet.sha256().unwrap(), expected);

    let mut diff_drift = packet.clone();
    diff_drift.candidate_diff.push_str("drift");
    assert!(diff_drift.validate().is_err());

    let mut evidence_duplicate = packet.clone();
    evidence_duplicate.evidence_digests.push([9; 32]);
    assert!(evidence_duplicate.validate().is_err());

    let mut oversized = packet;
    oversized.candidate_diff = "x".repeat(256 * 1024);
    oversized.candidate_diff_sha256 = Sha256::digest(oversized.candidate_diff.as_bytes()).into();
    assert!(oversized.validate().is_err());

    let duplicate = encoded
        .strip_suffix(b"}")
        .unwrap()
        .iter()
        .copied()
        .chain(br#",\"schema_version\":1}"#.iter().copied())
        .collect::<Vec<_>>();
    assert!(FeatureConveyorReviewPacket::decode_frame(&duplicate).is_err());
}

#[test]
fn provider_output_is_strict_and_cannot_waive_blocking_or_uncovered_requirements() {
    let packet_sha256 = packet().sha256().unwrap();
    let approved = approved_output(packet_sha256);
    assert_eq!(
        FeatureConveyorReviewProviderOutput::decode_frame(&serde_json::to_vec(&approved).unwrap())
            .unwrap(),
        approved
    );

    let mut blocking_approval = approved.clone();
    blocking_approval
        .blocking_findings
        .push(FeatureConveyorReviewFinding {
            finding_id: "blocking-1".to_string(),
            requirement_id: "requirement-1".to_string(),
            evidence_sha256: [21; 32],
        });
    assert!(blocking_approval.validate().is_err());

    let mut uncovered_approval = approved.clone();
    uncovered_approval.requirement_coverage[0].status =
        FeatureConveyorReviewCoverageStatus::Uncovered;
    assert!(uncovered_approval.validate().is_err());

    let mut unsupported_rejection = approved.clone();
    unsupported_rejection.decision = FeatureConveyorReviewDecision::Rejected;
    assert!(unsupported_rejection.validate().is_err());

    let mut unresolved_knowledge = approved.clone();
    unresolved_knowledge.knowledge_base_determination =
        FeatureConveyorKnowledgeBaseDetermination::UpdateRequired;
    assert!(unresolved_knowledge.validate().is_err());

    let mut unknown = serde_json::to_value(approved).unwrap();
    unknown
        .as_object_mut()
        .unwrap()
        .insert("raw_response".to_string(), json!("forbidden"));
    assert!(FeatureConveyorReviewProviderOutput::decode_frame(
        &serde_json::to_vec(&unknown).unwrap()
    )
    .is_err());
}
