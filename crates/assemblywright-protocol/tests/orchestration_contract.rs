use assemblywright_protocol::{
    FeatureConveyorOrchestrationAction, FeatureConveyorOrchestrationPauseKind,
    FeatureConveyorOrchestrationProjection, FeatureConveyorOrchestrationReason,
    FeatureConveyorOrchestrationStage, FEATURE_CONVEYOR_ORCHESTRATION_SCHEMA_VERSION,
    MAX_FEATURE_CONVEYOR_ACTIVE_PROCESSING_MS, MAX_FEATURE_CONVEYOR_REPLACEMENT_CANDIDATES,
};
use uuid::Uuid;

fn projection() -> FeatureConveyorOrchestrationProjection {
    FeatureConveyorOrchestrationProjection {
        schema_version: FEATURE_CONVEYOR_ORCHESTRATION_SCHEMA_VERSION,
        feature_id: Uuid::new_v4(),
        lifecycle_revision: 2,
        orchestration_revision: 1,
        stage: FeatureConveyorOrchestrationStage::Reviewing,
        action: FeatureConveyorOrchestrationAction::AwaitReviewDecision,
        reason: FeatureConveyorOrchestrationReason::CheckpointEffectFree,
        checkpoint_id: Uuid::new_v4(),
        checkpoint_sha256: [1; 32],
        replacement_candidates_used: 0,
        active_processing_ms: 100,
        active_processing_budget_ms: MAX_FEATURE_CONVEYOR_ACTIVE_PROCESSING_MS,
        pause_kind: None,
        next_retry_at_ms: None,
        effect_possible: false,
        activated: true,
    }
}

#[test]
fn orchestration_projection_accepts_exact_bounds_and_pause_shape() {
    let mut value = projection();
    value.replacement_candidates_used = MAX_FEATURE_CONVEYOR_REPLACEMENT_CANDIDATES;
    value.active_processing_ms = MAX_FEATURE_CONVEYOR_ACTIVE_PROCESSING_MS;
    value.validate().unwrap();

    value.stage = FeatureConveyorOrchestrationStage::Paused;
    value.reason = FeatureConveyorOrchestrationReason::ReviewTransportBackoff;
    value.pause_kind = Some(FeatureConveyorOrchestrationPauseKind::Provider);
    value.next_retry_at_ms = Some(60_000);
    value.validate().unwrap();
}

#[test]
fn orchestration_projection_rejects_invalid_inert_budget_pause_and_retry_shapes() {
    let cases: [fn(&mut FeatureConveyorOrchestrationProjection); 10] = [
        |value| value.schema_version = 0,
        |value| value.feature_id = Uuid::nil(),
        |value| value.checkpoint_id = Uuid::nil(),
        |value| value.checkpoint_sha256 = [0; 32],
        |value| value.replacement_candidates_used += 4,
        |value| value.active_processing_ms += MAX_FEATURE_CONVEYOR_ACTIVE_PROCESSING_MS,
        |value| value.active_processing_budget_ms -= 1,
        |value| value.pause_kind = Some(FeatureConveyorOrchestrationPauseKind::Owner),
        |value| value.next_retry_at_ms = Some(1),
        |value| value.activated = false,
    ];
    for mutate in cases {
        let mut value = projection();
        mutate(&mut value);
        assert!(value.validate().is_err());
    }
}
