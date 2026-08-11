use assemblywright_protocol::{
    FeatureConveyorAbandonAndAdvanceReceipt, FeatureConveyorAbandonAndAdvanceRequest,
    FeatureConveyorAbandonAndAdvanceStatus, FeatureConveyorAbandonmentEvidence,
    FeatureConveyorCancelActiveFeatureReceipt, FeatureConveyorCancelActiveFeatureRequest,
    FeatureConveyorCancelActiveFeatureStatus, ProtocolError,
    FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
    MAX_FEATURE_CONVEYOR_OWNER_RESOLUTION_REQUEST_BYTES,
};
use uuid::Uuid;

fn cancel_request() -> FeatureConveyorCancelActiveFeatureRequest {
    FeatureConveyorCancelActiveFeatureRequest {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        feature_id: Uuid::new_v4(),
        expected_lifecycle_revision: 2,
        expected_queue_revision: 3,
        expected_emergency_pause_revision: 4,
    }
}

fn abandon_request() -> FeatureConveyorAbandonAndAdvanceRequest {
    FeatureConveyorAbandonAndAdvanceRequest {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        feature_id: Uuid::new_v4(),
        expected_lifecycle_revision: 3,
        expected_queue_revision: 4,
        expected_emergency_pause_revision: 5,
        evidence: FeatureConveyorAbandonmentEvidence {
            safe_reconciliation_sha256: [1; 32],
            merged: true,
            verified_healthy_main_sha256: Some([2; 32]),
        },
    }
}

#[test]
fn owner_resolution_requests_are_strict_bounded_and_revision_bound() {
    let cancel = cancel_request();
    let cancel_json = serde_json::to_vec(&cancel).unwrap();
    assert_eq!(
        FeatureConveyorCancelActiveFeatureRequest::decode_frame(&cancel_json).unwrap(),
        cancel
    );
    let duplicate = String::from_utf8(cancel_json).unwrap().replacen(
        "\"feature_id\":",
        "\"feature_id\":\"00000000-0000-0000-0000-000000000001\",\"feature_id\":",
        1,
    );
    assert!(FeatureConveyorCancelActiveFeatureRequest::decode_frame(duplicate.as_bytes()).is_err());
    let unknown = serde_json::to_string(&cancel).unwrap().replacen(
        "\"feature_id\":",
        "\"repository_path\":\"private\",\"feature_id\":",
        1,
    );
    assert!(FeatureConveyorCancelActiveFeatureRequest::decode_frame(unknown.as_bytes()).is_err());
    assert!(FeatureConveyorCancelActiveFeatureRequest::decode_frame(b"{}").is_err());

    for mutate in [
        |value: &mut FeatureConveyorCancelActiveFeatureRequest| value.schema_version = 0,
        |value: &mut FeatureConveyorCancelActiveFeatureRequest| value.feature_id = Uuid::nil(),
        |value: &mut FeatureConveyorCancelActiveFeatureRequest| {
            value.expected_lifecycle_revision = 0
        },
    ] {
        let mut invalid = cancel;
        mutate(&mut invalid);
        assert!(invalid.validate().is_err());
    }
    let mut maximum = cancel;
    maximum.expected_lifecycle_revision = u64::MAX;
    maximum.expected_queue_revision = u64::MAX;
    maximum.expected_emergency_pause_revision = u64::MAX;
    maximum.validate().unwrap();
    assert!(matches!(
        FeatureConveyorCancelActiveFeatureRequest::decode_frame(&vec![
            b' ';
            MAX_FEATURE_CONVEYOR_OWNER_RESOLUTION_REQUEST_BYTES
                + 1
        ]),
        Err(ProtocolError::FrameTooLarge { .. })
    ));
}

#[test]
fn abandonment_requires_safe_reconciliation_and_healthy_main_after_merge() {
    let valid = abandon_request();
    let encoded = serde_json::to_vec(&valid).unwrap();
    assert_eq!(
        FeatureConveyorAbandonAndAdvanceRequest::decode_frame(&encoded).unwrap(),
        valid
    );
    assert!(FeatureConveyorAbandonAndAdvanceRequest::decode_frame(b"{}").is_err());

    let nested_duplicate = String::from_utf8(encoded).unwrap().replacen(
        "\"merged\":",
        "\"merged\":false,\"merged\":",
        1,
    );
    assert!(
        FeatureConveyorAbandonAndAdvanceRequest::decode_frame(nested_duplicate.as_bytes()).is_err()
    );
    let mut zero_reconciliation = valid;
    zero_reconciliation.evidence.safe_reconciliation_sha256 = [0; 32];
    assert!(zero_reconciliation.validate().is_err());
    let mut zero_healthy_main = valid;
    zero_healthy_main.evidence.verified_healthy_main_sha256 = Some([0; 32]);
    assert!(zero_healthy_main.validate().is_err());
    let mut missing_healthy_main = valid;
    missing_healthy_main.evidence.verified_healthy_main_sha256 = None;
    assert!(missing_healthy_main.validate().is_err());
    let mut non_advancing_maximum = valid;
    non_advancing_maximum.expected_queue_revision = u64::MAX;
    assert!(non_advancing_maximum.validate().is_err());

    let mut unmerged = valid;
    unmerged.evidence.merged = false;
    unmerged.evidence.verified_healthy_main_sha256 = None;
    unmerged.validate().unwrap();
}

#[test]
fn owner_resolution_receipts_are_fixed_redacted_outcomes() {
    let cancel = FeatureConveyorCancelActiveFeatureReceipt {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        feature_id: Uuid::new_v4(),
        lifecycle_revision: 3,
        queue_revision: 4,
        emergency_pause_revision: 5,
        lease_retained: true,
        advancement_authorized: false,
        status: FeatureConveyorCancelActiveFeatureStatus::Cancelled,
    };
    let cancel_json = serde_json::to_vec(&cancel).unwrap();
    assert_eq!(
        FeatureConveyorCancelActiveFeatureReceipt::decode_frame(&cancel_json).unwrap(),
        cancel
    );
    let mut false_lease = cancel;
    false_lease.lease_retained = false;
    assert!(false_lease.validate().is_err());
    let mut false_advancement = cancel;
    false_advancement.advancement_authorized = true;
    assert!(false_advancement.validate().is_err());

    let abandoned = FeatureConveyorAbandonAndAdvanceReceipt {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        feature_id: Uuid::new_v4(),
        lifecycle_revision: 4,
        queue_revision: 5,
        emergency_pause_revision: 6,
        lease_released: true,
        status: FeatureConveyorAbandonAndAdvanceStatus::Abandoned,
    };
    let abandoned_json = serde_json::to_vec(&abandoned).unwrap();
    assert_eq!(
        FeatureConveyorAbandonAndAdvanceReceipt::decode_frame(&abandoned_json).unwrap(),
        abandoned
    );
    let text = String::from_utf8(abandoned_json).unwrap();
    for forbidden in ["repository", "path", "reason", "owner", "payload", "error"] {
        assert!(
            !text.contains(forbidden),
            "receipt leaked {forbidden}: {text}"
        );
    }
}
