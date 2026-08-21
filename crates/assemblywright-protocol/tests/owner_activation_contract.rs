use assemblywright_protocol::{
    FeatureConveyorActivationBlocker, FeatureConveyorActivationEvidenceAdmissionProjection,
    FeatureConveyorActivationEvidenceAdmissionRequest, FeatureConveyorActivationEvidenceCategory,
    FeatureConveyorActivationEvidenceOrigin, FeatureConveyorActivationEvidenceProjection,
    FeatureConveyorActivationEvidenceReference, FeatureConveyorActivationRequest,
    FeatureConveyorActivationStatus, FeatureConveyorOrchestrationStage,
    FeatureConveyorOwnerActiveFeature, FeatureConveyorOwnerControlProjection,
    FeatureConveyorOwnerLifecycleStatus, FEATURE_CONVEYOR_OWNER_ACTIVATION_SCHEMA_VERSION,
};
use serde_json::json;
use uuid::Uuid;

fn reference(byte: u8, revision: u64) -> FeatureConveyorActivationEvidenceReference {
    FeatureConveyorActivationEvidenceReference {
        evidence_id: Uuid::from_bytes([byte; 16]),
        revision,
        receipt_sha256: [byte; 32],
    }
}

fn complete_evidence() -> FeatureConveyorActivationEvidenceProjection {
    FeatureConveyorActivationEvidenceProjection {
        repository_gate_proof: Some(reference(1, 1)),
        restricted_worker_live: Some(reference(2, 1)),
        review_provider_live: Some(reference(3, 1)),
        github_publication_live: Some(reference(4, 1)),
        restart_recovery_live: Some(reference(5, 1)),
        mac_windows_control_event_streaming_live: Some(reference(6, 1)),
    }
}

fn empty_evidence() -> FeatureConveyorActivationEvidenceProjection {
    FeatureConveyorActivationEvidenceProjection {
        repository_gate_proof: None,
        restricted_worker_live: None,
        review_provider_live: None,
        github_publication_live: None,
        restart_recovery_live: None,
        mac_windows_control_event_streaming_live: None,
    }
}

#[test]
fn owner_activation_is_ready_only_with_all_six_admitted_evidence_categories() {
    let active_feature = FeatureConveyorOwnerActiveFeature {
        feature_id: Uuid::new_v4(),
        specification_revision: 1,
        lifecycle_revision: 1,
        orchestration_revision: 0,
        lifecycle_status: FeatureConveyorOwnerLifecycleStatus::Implementing,
        stage: FeatureConveyorOrchestrationStage::Implementing,
        owner_paused: false,
    };
    let ready = FeatureConveyorOwnerControlProjection {
        schema_version: FEATURE_CONVEYOR_OWNER_ACTIVATION_SCHEMA_VERSION,
        queue_revision: 7,
        emergency_paused: false,
        emergency_pause_revision: 2,
        owner_control_designation_revision: 3,
        activation_status: FeatureConveyorActivationStatus::Inactive,
        activation_id: None,
        activation_ready: true,
        activation_blocker: FeatureConveyorActivationBlocker::None,
        active_feature: Some(active_feature),
        evidence: complete_evidence(),
    };
    ready.validate().unwrap();
    let request = FeatureConveyorActivationRequest {
        schema_version: FEATURE_CONVEYOR_OWNER_ACTIVATION_SCHEMA_VERSION,
        expected_queue_revision: ready.queue_revision,
        expected_owner_control_designation_revision: ready.owner_control_designation_revision,
        expected_emergency_pause_revision: ready.emergency_pause_revision,
        evidence: ready.evidence.complete().unwrap(),
    };
    request.validate().unwrap();

    for remove in 0..6 {
        let mut missing = ready;
        match remove {
            0 => missing.evidence.repository_gate_proof = None,
            1 => missing.evidence.restricted_worker_live = None,
            2 => missing.evidence.review_provider_live = None,
            3 => missing.evidence.github_publication_live = None,
            4 => missing.evidence.restart_recovery_live = None,
            _ => missing.evidence.mac_windows_control_event_streaming_live = None,
        }
        missing.activation_ready = false;
        missing.activation_blocker = FeatureConveyorActivationBlocker::EvidenceRequired;
        missing.validate().unwrap();
        assert!(missing.evidence.complete().is_none());
    }
}

#[test]
fn activation_evidence_admission_is_strict_contiguous_category_bound_and_nonzero() {
    let valid = FeatureConveyorActivationEvidenceAdmissionRequest {
        schema_version: FEATURE_CONVEYOR_OWNER_ACTIVATION_SCHEMA_VERSION,
        category: FeatureConveyorActivationEvidenceCategory::RepositoryGateProof,
        origin: FeatureConveyorActivationEvidenceOrigin::RepositoryGateProofController,
        evidence_id: Uuid::new_v4(),
        revision: 1,
        expected_current_revision: 0,
        receipt_sha256: [9; 32],
        observed_at_ms: 10,
        expected_emergency_pause_revision: 0,
    };
    valid.validate().unwrap();
    let encoded = serde_json::to_vec(&valid).unwrap();
    assert_eq!(
        FeatureConveyorActivationEvidenceAdmissionRequest::decode_frame(&encoded).unwrap(),
        valid
    );

    let mut wrong_origin = valid;
    wrong_origin.origin = FeatureConveyorActivationEvidenceOrigin::ReviewProviderProofController;
    assert!(wrong_origin.validate().is_err());
    let mut skipped = valid;
    skipped.revision = 2;
    assert!(skipped.validate().is_err());
    let mut zero = valid;
    zero.receipt_sha256 = [0; 32];
    assert!(zero.validate().is_err());

    let mut extra = serde_json::to_value(valid).unwrap();
    extra
        .as_object_mut()
        .unwrap()
        .insert("path".to_string(), json!("C:\\secret"));
    assert!(
        FeatureConveyorActivationEvidenceAdmissionRequest::decode_frame(
            &serde_json::to_vec(&extra).unwrap()
        )
        .is_err()
    );
}

#[test]
fn activation_evidence_admission_projection_is_bounded_and_activation_consistent() {
    let mut projection = FeatureConveyorActivationEvidenceAdmissionProjection {
        schema_version: FEATURE_CONVEYOR_OWNER_ACTIVATION_SCHEMA_VERSION,
        emergency_paused: false,
        emergency_pause_revision: 3,
        activation_status: FeatureConveyorActivationStatus::Inactive,
        activation_id: None,
        evidence: empty_evidence(),
    };
    projection.validate().unwrap();

    projection.activation_id = Some(Uuid::new_v4());
    assert!(projection.validate().is_err());
    projection.activation_status = FeatureConveyorActivationStatus::Active;
    assert!(projection.validate().is_err());
    projection.activation_id = Some(Uuid::nil());
    assert!(projection.validate().is_err());
}

#[test]
fn owner_control_projection_rejects_false_readiness_pause_and_partial_active_evidence() {
    let active_feature = FeatureConveyorOwnerActiveFeature {
        feature_id: Uuid::new_v4(),
        specification_revision: 1,
        lifecycle_revision: 2,
        orchestration_revision: 1,
        lifecycle_status: FeatureConveyorOwnerLifecycleStatus::Paused,
        stage: FeatureConveyorOrchestrationStage::Paused,
        owner_paused: true,
    };
    let mut projection = FeatureConveyorOwnerControlProjection {
        schema_version: FEATURE_CONVEYOR_OWNER_ACTIVATION_SCHEMA_VERSION,
        queue_revision: 1,
        emergency_paused: false,
        emergency_pause_revision: 0,
        owner_control_designation_revision: 1,
        activation_status: FeatureConveyorActivationStatus::Inactive,
        activation_id: None,
        activation_ready: false,
        activation_blocker: FeatureConveyorActivationBlocker::None,
        active_feature: Some(active_feature),
        evidence: complete_evidence(),
    };
    projection.activation_ready = true;
    projection.validate().unwrap();
    projection.activation_ready = false;
    assert!(projection.validate().is_err());

    projection.activation_ready = false;
    projection.activation_status = FeatureConveyorActivationStatus::Active;
    projection.activation_id = Some(Uuid::new_v4());
    projection.activation_blocker = FeatureConveyorActivationBlocker::AlreadyActivated;
    projection.evidence.repository_gate_proof = None;
    assert!(projection.validate().is_err());
}
