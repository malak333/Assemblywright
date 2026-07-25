use assemblywright_master::{
    ApprovedFeatureSpecification, FeatureAbandonmentEvidence, FeatureGrantRevisions,
    FeatureLifecycleStatus, FeatureTransitionEvidence, MasterError, MasterKernel, MasterProcess,
    RepositoryGrantKind, RepositoryGrantRevision, VerifiedFeatureSuccess,
    MAX_CONVEYOR_NONTERMINAL_FEATURES,
};
use rusqlite::Connection;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tempfile::tempdir;
use uuid::Uuid;

fn digest(label: &str) -> [u8; 32] {
    Sha256::digest(label.as_bytes()).into()
}

fn canonical_manifest(value: &Value) -> String {
    fn write(value: &Value, output: &mut String) {
        match value {
            Value::Null => output.push_str("null"),
            Value::Bool(value) => output.push_str(if *value { "true" } else { "false" }),
            Value::Number(value) => output.push_str(&value.to_string()),
            Value::String(value) => output.push_str(&serde_json::to_string(value).unwrap()),
            Value::Array(values) => {
                output.push('[');
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        output.push(',');
                    }
                    write(value, output);
                }
                output.push(']');
            }
            Value::Object(values) => {
                output.push('{');
                let mut keys = values.keys().collect::<Vec<_>>();
                keys.sort();
                for (index, key) in keys.into_iter().enumerate() {
                    if index > 0 {
                        output.push(',');
                    }
                    output.push_str(&serde_json::to_string(key).unwrap());
                    output.push(':');
                    write(&values[key], output);
                }
                output.push('}');
            }
        }
    }
    let mut output = String::new();
    write(value, &mut output);
    output
}

fn install_grants(kernel: &mut MasterKernel, repository_id: Uuid) {
    for (index, kind) in [
        RepositoryGrantKind::Registration,
        RepositoryGrantKind::CloudDisclosure,
        RepositoryGrantKind::AutonomousPublication,
    ]
    .into_iter()
    .enumerate()
    {
        kernel
            .record_repository_grant_revision(
                &RepositoryGrantRevision {
                    repository_id,
                    kind,
                    revision: 1,
                    scope_sha256: digest(&format!("scope-{index}")),
                    owner_approval_sha256: digest(&format!("grant-approval-{index}")),
                    expires_at_ms: None,
                    revoked: false,
                },
                1,
            )
            .unwrap();
    }
}

fn specification(
    feature_id: Uuid,
    repository_id: Uuid,
    dependencies: Vec<Uuid>,
) -> ApprovedFeatureSpecification {
    let manifest = json!({
        "feature_id": feature_id,
        "outcome": "bounded kernel test",
        "allowed_paths": ["crates/assemblywright-master/src/lib.rs"]
    });
    let canonical = canonical_manifest(&manifest);
    ApprovedFeatureSpecification {
        feature_id,
        revision: 1,
        repository_id,
        manifest,
        manifest_sha256: Sha256::digest(canonical.as_bytes()).into(),
        design_sha256: digest("design"),
        brainstorming_sha256: digest("brainstorming"),
        owner_approval_sha256: digest("feature-owner-approval"),
        grants: FeatureGrantRevisions {
            registration: 1,
            cloud_disclosure: 1,
            autonomous_publication: 1,
        },
        provider_id: "local.review".to_string(),
        model_id: "review-v1".to_string(),
        dependencies,
    }
}

#[test]
fn feature_conveyor_owner_journey_persists_end_to_end() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    let repository_id = Uuid::new_v4();
    let feature = specification(Uuid::new_v4(), repository_id, vec![]);

    {
        let mut process = MasterProcess::acquire(directory.path()).unwrap();
        install_grants(process.kernel_mut(), repository_id);
        process
            .kernel_mut()
            .enqueue_approved_feature(&feature, 0, 10)
            .unwrap();
        assert_eq!(process.kernel().feature_queue_revision().unwrap(), 1);
    }

    {
        let mut process = MasterProcess::acquire(directory.path()).unwrap();
        assert_eq!(process.kernel().feature_startup_quarantines(), 0);
        let claim = process.kernel_mut().claim_next_feature(1, 20).unwrap();
        assert_eq!(claim.feature_id, feature.feature_id);
        assert_eq!(claim.provider_id, feature.provider_id);
        assert_eq!(claim.model_id, feature.model_id);

        let evidence = FeatureTransitionEvidence {
            repository_snapshot_sha256: digest("owner-journey-snapshot"),
            accepted_evidence_sha256: digest("owner-journey-evidence"),
        };
        let mut snapshot = process
            .kernel_mut()
            .advance_feature_lifecycle(
                feature.feature_id,
                claim.lifecycle_revision,
                FeatureLifecycleStatus::Validating,
                evidence,
                21,
            )
            .unwrap();
        for (now_ms, next_status) in [
            (22, FeatureLifecycleStatus::Reviewing),
            (23, FeatureLifecycleStatus::Publishing),
            (24, FeatureLifecycleStatus::VerifyingMain),
        ] {
            snapshot = process
                .kernel_mut()
                .advance_feature_lifecycle(
                    feature.feature_id,
                    snapshot.lifecycle_revision,
                    next_status,
                    evidence,
                    now_ms,
                )
                .unwrap();
        }
        let queue_revision = process.kernel().feature_queue_revision().unwrap();
        assert_eq!(queue_revision, 2);
        let succeeded = process
            .kernel_mut()
            .mark_feature_succeeded(
                feature.feature_id,
                snapshot.lifecycle_revision,
                queue_revision,
                VerifiedFeatureSuccess {
                    main_commit_sha256: digest("owner-journey-main"),
                    post_merge_evidence_sha256: digest("owner-journey-post-merge"),
                    main_healthy: true,
                },
                25,
            )
            .unwrap();
        assert_eq!(succeeded.status, FeatureLifecycleStatus::Succeeded);
        assert!(succeeded.active_lease_id.is_none());
        assert!(!succeeded.effect_possible);
    }

    let process = MasterProcess::acquire(directory.path()).unwrap();
    let persisted = process
        .kernel()
        .feature_snapshot(feature.feature_id)
        .unwrap();
    assert_eq!(persisted.status, FeatureLifecycleStatus::Succeeded);
    assert!(persisted.active_lease_id.is_none());
    assert_eq!(process.kernel().feature_queue_revision().unwrap(), 3);
    assert_eq!(process.kernel().feature_startup_quarantines(), 0);
    drop(process);

    let connection = Connection::open(database).unwrap();
    let audit_rows = connection
        .prepare(
            "SELECT event_kind, redacted_metadata_json
             FROM feature_conveyor_audit
             WHERE feature_id = ?1
             ORDER BY audit_id",
        )
        .unwrap()
        .query_map([feature.feature_id.to_string()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(
        audit_rows
            .iter()
            .map(|(event_kind, _)| event_kind.as_str())
            .collect::<Vec<_>>(),
        vec![
            "feature_enqueued",
            "feature_claimed",
            "feature_lifecycle_advanced",
            "feature_lifecycle_advanced",
            "feature_lifecycle_advanced",
            "feature_lifecycle_advanced",
            "feature_succeeded",
        ]
    );
    for (_, metadata) in audit_rows {
        assert!(!metadata.contains(&repository_id.to_string()));
        assert!(!metadata.contains("allowed_paths"));
        assert!(!metadata.contains("bounded kernel test"));
    }
}

#[test]
fn capacity_101_and_immutable_revisions_roll_back_atomically() {
    let mut kernel = MasterKernel::in_memory().unwrap();
    let repository_id = Uuid::new_v4();
    install_grants(&mut kernel, repository_id);
    let first = specification(Uuid::new_v4(), repository_id, vec![]);
    kernel.enqueue_approved_feature(&first, 0, 2).unwrap();

    let duplicate = kernel.enqueue_approved_feature(&first, 1, 3).unwrap_err();
    assert!(matches!(
        duplicate,
        MasterError::FeatureSpecificationImmutable
    ));
    assert_eq!(kernel.feature_queue_revision().unwrap(), 1);

    let duplicate_grant = kernel
        .record_repository_grant_revision(
            &RepositoryGrantRevision {
                repository_id,
                kind: RepositoryGrantKind::Registration,
                revision: 1,
                scope_sha256: digest("different"),
                owner_approval_sha256: digest("different-approval"),
                expires_at_ms: None,
                revoked: false,
            },
            4,
        )
        .unwrap_err();
    assert!(matches!(
        duplicate_grant,
        MasterError::RepositoryGrantImmutable
    ));

    for revision in 1..MAX_CONVEYOR_NONTERMINAL_FEATURES {
        let feature = specification(Uuid::new_v4(), repository_id, vec![]);
        kernel
            .enqueue_approved_feature(&feature, revision, 10 + revision)
            .unwrap();
    }
    let overflow = specification(Uuid::new_v4(), repository_id, vec![]);
    assert!(matches!(
        kernel
            .enqueue_approved_feature(&overflow, MAX_CONVEYOR_NONTERMINAL_FEATURES, 500)
            .unwrap_err(),
        MasterError::FeatureQueueFull
    ));
    assert_eq!(
        kernel.feature_queue_revision().unwrap(),
        MAX_CONVEYOR_NONTERMINAL_FEATURES
    );
    assert!(matches!(
        kernel.feature_snapshot(overflow.feature_id).unwrap_err(),
        MasterError::FeatureNotFound
    ));
}

#[test]
fn stale_cas_blocked_head_no_skip_reorder_and_singleton_lease() {
    let mut kernel = MasterKernel::in_memory().unwrap();
    let repository_id = Uuid::new_v4();
    install_grants(&mut kernel, repository_id);
    let dependency = specification(Uuid::new_v4(), repository_id, vec![]);
    let blocked = specification(Uuid::new_v4(), repository_id, vec![dependency.feature_id]);
    kernel.enqueue_approved_feature(&dependency, 0, 10).unwrap();
    kernel.enqueue_approved_feature(&blocked, 1, 11).unwrap();

    assert!(matches!(
        kernel
            .reorder_queued_features(&[blocked.feature_id, dependency.feature_id], 0, 12)
            .unwrap_err(),
        MasterError::StaleFeatureQueueRevision { .. }
    ));
    let revision = kernel
        .reorder_queued_features(&[blocked.feature_id, dependency.feature_id], 2, 13)
        .unwrap();
    assert_eq!(revision, 3);
    assert!(matches!(
        kernel.claim_next_feature(3, 14).unwrap_err(),
        MasterError::FeatureDependencyBlocked
    ));
    assert_eq!(
        kernel
            .feature_snapshot(dependency.feature_id)
            .unwrap()
            .status,
        FeatureLifecycleStatus::Queued
    );

    let revision = kernel
        .reorder_queued_features(&[dependency.feature_id, blocked.feature_id], 3, 15)
        .unwrap();
    let claim = kernel.claim_next_feature(revision, 16).unwrap();
    assert_eq!(claim.feature_id, dependency.feature_id);
    assert!(matches!(
        kernel
            .claim_next_feature(kernel.feature_queue_revision().unwrap(), 17)
            .unwrap_err(),
        MasterError::FeatureLeaseAlreadyActive
    ));
}

#[test]
fn exact_lifecycle_requires_lease_evidence_and_verified_healthy_main() {
    let mut kernel = MasterKernel::in_memory().unwrap();
    let repository_id = Uuid::new_v4();
    install_grants(&mut kernel, repository_id);
    let feature = specification(Uuid::new_v4(), repository_id, vec![]);
    kernel.enqueue_approved_feature(&feature, 0, 10).unwrap();
    let evidence = FeatureTransitionEvidence {
        repository_snapshot_sha256: digest("snapshot"),
        accepted_evidence_sha256: digest("evidence"),
    };
    assert!(matches!(
        kernel
            .advance_feature_lifecycle(
                feature.feature_id,
                1,
                FeatureLifecycleStatus::Validating,
                evidence,
                10
            )
            .unwrap_err(),
        MasterError::InvalidFeatureTransition
    ));
    let claim = kernel.claim_next_feature(1, 11).unwrap();

    let zero_evidence = FeatureTransitionEvidence {
        repository_snapshot_sha256: [0; 32],
        accepted_evidence_sha256: digest("evidence"),
    };
    assert!(matches!(
        kernel
            .advance_feature_lifecycle(
                feature.feature_id,
                claim.lifecycle_revision,
                FeatureLifecycleStatus::Validating,
                zero_evidence,
                12
            )
            .unwrap_err(),
        MasterError::InvalidFeatureConveyorInput(_)
    ));
    let mut snapshot = kernel
        .advance_feature_lifecycle(
            feature.feature_id,
            claim.lifecycle_revision,
            FeatureLifecycleStatus::Validating,
            evidence,
            13,
        )
        .unwrap();
    for next in [
        FeatureLifecycleStatus::Reviewing,
        FeatureLifecycleStatus::Publishing,
        FeatureLifecycleStatus::VerifyingMain,
    ] {
        snapshot = kernel
            .advance_feature_lifecycle(
                feature.feature_id,
                snapshot.lifecycle_revision,
                next,
                evidence,
                14 + snapshot.lifecycle_revision,
            )
            .unwrap();
        if matches!(
            next,
            FeatureLifecycleStatus::Publishing | FeatureLifecycleStatus::VerifyingMain
        ) {
            assert!(snapshot.effect_possible);
        }
    }
    assert!(matches!(
        kernel
            .mark_feature_succeeded(
                feature.feature_id,
                snapshot.lifecycle_revision,
                kernel.feature_queue_revision().unwrap(),
                VerifiedFeatureSuccess {
                    main_commit_sha256: digest("main"),
                    post_merge_evidence_sha256: digest("post-merge"),
                    main_healthy: false,
                },
                30
            )
            .unwrap_err(),
        MasterError::VerifiedHealthyMainRequired
    ));
    let succeeded = kernel
        .mark_feature_succeeded(
            feature.feature_id,
            snapshot.lifecycle_revision,
            kernel.feature_queue_revision().unwrap(),
            VerifiedFeatureSuccess {
                main_commit_sha256: digest("main"),
                post_merge_evidence_sha256: digest("post-merge"),
                main_healthy: true,
            },
            31,
        )
        .unwrap();
    assert_eq!(succeeded.status, FeatureLifecycleStatus::Succeeded);
    assert!(succeeded.active_lease_id.is_none());
}

#[test]
fn changed_grant_invalidates_active_feature_before_lifecycle_advancement() {
    let mut kernel = MasterKernel::in_memory().unwrap();
    let repository_id = Uuid::new_v4();
    install_grants(&mut kernel, repository_id);
    let feature = specification(Uuid::new_v4(), repository_id, vec![]);
    kernel.enqueue_approved_feature(&feature, 0, 10).unwrap();
    let claim = kernel.claim_next_feature(1, 11).unwrap();
    kernel
        .record_repository_grant_revision(
            &RepositoryGrantRevision {
                repository_id,
                kind: RepositoryGrantKind::AutonomousPublication,
                revision: 2,
                scope_sha256: digest("revoked-publication-scope"),
                owner_approval_sha256: digest("revoked-publication-approval"),
                expires_at_ms: None,
                revoked: true,
            },
            12,
        )
        .unwrap();

    assert!(matches!(
        kernel
            .advance_feature_lifecycle(
                feature.feature_id,
                claim.lifecycle_revision,
                FeatureLifecycleStatus::Validating,
                FeatureTransitionEvidence {
                    repository_snapshot_sha256: digest("snapshot"),
                    accepted_evidence_sha256: digest("evidence"),
                },
                13,
            )
            .unwrap_err(),
        MasterError::RepositoryGrantUnavailable
    ));
    let snapshot = kernel.feature_snapshot(feature.feature_id).unwrap();
    assert_eq!(snapshot.status, FeatureLifecycleStatus::Implementing);
    assert_eq!(snapshot.lifecycle_revision, claim.lifecycle_revision);
    assert_eq!(snapshot.active_lease_id, Some(claim.lease_id));
}

#[test]
fn cancellation_blocks_until_safe_abandonment_and_merged_main_is_healthy() {
    let mut kernel = MasterKernel::in_memory().unwrap();
    let repository_id = Uuid::new_v4();
    install_grants(&mut kernel, repository_id);
    let feature = specification(Uuid::new_v4(), repository_id, vec![]);
    kernel.enqueue_approved_feature(&feature, 0, 10).unwrap();
    let claim = kernel.claim_next_feature(1, 11).unwrap();
    let cancelled = kernel
        .cancel_active_feature(feature.feature_id, claim.lifecycle_revision, 12)
        .unwrap();
    assert_eq!(cancelled.status, FeatureLifecycleStatus::Cancelled);
    assert_eq!(cancelled.active_lease_id, Some(claim.lease_id));
    assert!(matches!(
        kernel
            .claim_next_feature(kernel.feature_queue_revision().unwrap(), 13)
            .unwrap_err(),
        MasterError::FeatureLeaseAlreadyActive
    ));
    assert!(matches!(
        kernel
            .abandon_and_advance(
                feature.feature_id,
                cancelled.lifecycle_revision,
                kernel.feature_queue_revision().unwrap(),
                FeatureAbandonmentEvidence {
                    safe_reconciliation_sha256: digest("safe"),
                    merged: false,
                    verified_healthy_main_sha256: Some([0; 32]),
                },
                14
            )
            .unwrap_err(),
        MasterError::InvalidFeatureConveyorInput(_)
    ));
    assert!(matches!(
        kernel
            .abandon_and_advance(
                feature.feature_id,
                cancelled.lifecycle_revision,
                kernel.feature_queue_revision().unwrap(),
                FeatureAbandonmentEvidence {
                    safe_reconciliation_sha256: digest("safe"),
                    merged: true,
                    verified_healthy_main_sha256: None,
                },
                15
            )
            .unwrap_err(),
        MasterError::VerifiedHealthyMainRequired
    ));
    let abandoned = kernel
        .abandon_and_advance(
            feature.feature_id,
            cancelled.lifecycle_revision,
            kernel.feature_queue_revision().unwrap(),
            FeatureAbandonmentEvidence {
                safe_reconciliation_sha256: digest("safe"),
                merged: true,
                verified_healthy_main_sha256: Some(digest("healthy-main")),
            },
            16,
        )
        .unwrap();
    assert_eq!(abandoned.status, FeatureLifecycleStatus::Abandoned);
    assert!(abandoned.active_lease_id.is_none());
}

fn install_audit_failure(database: &std::path::Path, event_kind: &str) {
    let connection = Connection::open(database).unwrap();
    connection
        .execute_batch("DROP TRIGGER IF EXISTS fail_feature_audit")
        .unwrap();
    connection
        .execute_batch(&format!(
            "CREATE TRIGGER fail_feature_audit
             BEFORE INSERT ON feature_conveyor_audit
             WHEN NEW.event_kind = '{event_kind}'
             BEGIN SELECT RAISE(ABORT, 'injected audit failure'); END;"
        ))
        .unwrap();
}

fn remove_audit_failure(database: &std::path::Path) {
    Connection::open(database)
        .unwrap()
        .execute_batch("DROP TRIGGER IF EXISTS fail_feature_audit")
        .unwrap();
}

#[test]
fn emergency_pause_and_audit_failures_roll_back_authoritative_mutations() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    let mut kernel = MasterKernel::open(&database).unwrap();
    let repository_id = Uuid::new_v4();
    install_grants(&mut kernel, repository_id);
    let feature = specification(Uuid::new_v4(), repository_id, vec![]);

    install_audit_failure(&database, "feature_enqueued");
    assert!(matches!(
        kernel.enqueue_approved_feature(&feature, 0, 9).unwrap_err(),
        MasterError::Storage(_)
    ));
    assert!(matches!(
        kernel.feature_snapshot(feature.feature_id).unwrap_err(),
        MasterError::FeatureNotFound
    ));
    assert_eq!(kernel.feature_queue_revision().unwrap(), 0);
    remove_audit_failure(&database);

    kernel.enqueue_approved_feature(&feature, 0, 10).unwrap();
    install_audit_failure(&database, "feature_queue_reordered");
    assert!(matches!(
        kernel
            .reorder_queued_features(&[feature.feature_id], 1, 11)
            .unwrap_err(),
        MasterError::Storage(_)
    ));
    assert_eq!(kernel.feature_queue_revision().unwrap(), 1);
    assert_eq!(
        kernel
            .feature_snapshot(feature.feature_id)
            .unwrap()
            .queue_position,
        1
    );
    remove_audit_failure(&database);

    kernel.set_emergency_paused_at(true, 11).unwrap();
    assert!(matches!(
        kernel.claim_next_feature(1, 12).unwrap_err(),
        MasterError::EmergencyPaused
    ));
    kernel.set_emergency_paused_at(false, 13).unwrap();
    install_audit_failure(&database, "feature_claimed");
    assert!(matches!(
        kernel.claim_next_feature(1, 14).unwrap_err(),
        MasterError::Storage(_)
    ));
    let snapshot = kernel.feature_snapshot(feature.feature_id).unwrap();
    assert_eq!(snapshot.status, FeatureLifecycleStatus::Queued);
    assert!(snapshot.active_lease_id.is_none());
    assert_eq!(kernel.feature_queue_revision().unwrap(), 1);
    remove_audit_failure(&database);

    let claim = kernel.claim_next_feature(1, 15).unwrap();
    let transition = FeatureTransitionEvidence {
        repository_snapshot_sha256: digest("audit-snapshot"),
        accepted_evidence_sha256: digest("audit-evidence"),
    };
    install_audit_failure(&database, "feature_lifecycle_advanced");
    assert!(matches!(
        kernel
            .advance_feature_lifecycle(
                feature.feature_id,
                claim.lifecycle_revision,
                FeatureLifecycleStatus::Validating,
                transition,
                16,
            )
            .unwrap_err(),
        MasterError::Storage(_)
    ));
    let snapshot = kernel.feature_snapshot(feature.feature_id).unwrap();
    assert_eq!(snapshot.status, FeatureLifecycleStatus::Implementing);
    assert_eq!(snapshot.lifecycle_revision, claim.lifecycle_revision);
    remove_audit_failure(&database);

    let validating = kernel
        .advance_feature_lifecycle(
            feature.feature_id,
            claim.lifecycle_revision,
            FeatureLifecycleStatus::Validating,
            transition,
            17,
        )
        .unwrap();
    install_audit_failure(&database, "feature_cancelled");
    assert!(matches!(
        kernel
            .cancel_active_feature(feature.feature_id, validating.lifecycle_revision, 18)
            .unwrap_err(),
        MasterError::Storage(_)
    ));
    let snapshot = kernel.feature_snapshot(feature.feature_id).unwrap();
    assert_eq!(snapshot.status, FeatureLifecycleStatus::Validating);
    assert_eq!(snapshot.lifecycle_revision, validating.lifecycle_revision);
    assert_eq!(snapshot.active_lease_id, Some(claim.lease_id));
    remove_audit_failure(&database);

    let cancelled = kernel
        .cancel_active_feature(feature.feature_id, validating.lifecycle_revision, 19)
        .unwrap();
    install_audit_failure(&database, "feature_abandoned");
    assert!(matches!(
        kernel
            .abandon_and_advance(
                feature.feature_id,
                cancelled.lifecycle_revision,
                kernel.feature_queue_revision().unwrap(),
                FeatureAbandonmentEvidence {
                    safe_reconciliation_sha256: digest("audit-safe"),
                    merged: false,
                    verified_healthy_main_sha256: None,
                },
                20,
            )
            .unwrap_err(),
        MasterError::Storage(_)
    ));
    let snapshot = kernel.feature_snapshot(feature.feature_id).unwrap();
    assert_eq!(snapshot.status, FeatureLifecycleStatus::Cancelled);
    assert_eq!(snapshot.lifecycle_revision, cancelled.lifecycle_revision);
    assert_eq!(snapshot.active_lease_id, Some(claim.lease_id));
}

#[test]
fn restart_quarantines_ambiguous_active_feature_without_retry() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    let repository_id = Uuid::new_v4();
    let feature = specification(Uuid::new_v4(), repository_id, vec![]);
    let lease_id;
    {
        let mut kernel = MasterKernel::open(&database).unwrap();
        install_grants(&mut kernel, repository_id);
        kernel.enqueue_approved_feature(&feature, 0, 10).unwrap();
        lease_id = kernel.claim_next_feature(1, 11).unwrap().lease_id;
    }
    let kernel = MasterKernel::open(&database).unwrap();
    assert_eq!(kernel.feature_startup_quarantines(), 1);
    let snapshot = kernel.feature_snapshot(feature.feature_id).unwrap();
    assert_eq!(snapshot.status, FeatureLifecycleStatus::Quarantined);
    assert!(snapshot.effect_possible);
    assert_eq!(snapshot.active_lease_id, Some(lease_id));
}

#[test]
fn startup_quarantine_audit_failure_rolls_back_and_blocks_open() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    let repository_id = Uuid::new_v4();
    let feature = specification(Uuid::new_v4(), repository_id, vec![]);
    let claim;
    {
        let mut kernel = MasterKernel::open(&database).unwrap();
        install_grants(&mut kernel, repository_id);
        kernel.enqueue_approved_feature(&feature, 0, 10).unwrap();
        claim = kernel.claim_next_feature(1, 11).unwrap();
        install_audit_failure(&database, "feature_startup_quarantined");
    }

    assert!(matches!(
        MasterKernel::open(&database),
        Err(MasterError::Storage(_))
    ));
    let connection = Connection::open(&database).unwrap();
    let (status, lifecycle_revision): (String, i64) = connection
        .query_row(
            "SELECT status, lifecycle_revision
             FROM feature_conveyor_features WHERE feature_id = ?1",
            [feature.feature_id.to_string()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(status, "implementing");
    assert_eq!(
        u64::try_from(lifecycle_revision).unwrap(),
        claim.lifecycle_revision
    );
    drop(connection);

    remove_audit_failure(&database);
    let kernel = MasterKernel::open(&database).unwrap();
    let snapshot = kernel.feature_snapshot(feature.feature_id).unwrap();
    assert_eq!(snapshot.status, FeatureLifecycleStatus::Quarantined);
    assert!(snapshot.effect_possible);
    assert_eq!(snapshot.active_lease_id, Some(claim.lease_id));
}

fn downgrade_v5_database_to_v4(path: &std::path::Path, sabotage: bool) {
    drop(MasterKernel::open(path).unwrap());
    let connection = Connection::open(path).unwrap();
    connection
        .execute_batch(
            "DROP TRIGGER feature_specification_revisions_no_update;
             DROP TRIGGER feature_specification_revisions_no_delete;
             DROP TRIGGER feature_repository_grants_no_update;
             DROP TRIGGER feature_repository_grants_no_delete;
             DROP TRIGGER feature_conveyor_audit_no_update;
             DROP TRIGGER feature_conveyor_audit_no_delete;
             DROP TRIGGER feature_transition_evidence_no_update;
             DROP TRIGGER feature_transition_evidence_no_delete;
             DROP TABLE feature_conveyor_audit;
             DROP TABLE feature_transition_evidence;
             DROP TABLE feature_active_lease;
             DROP TABLE feature_conveyor_queue;
             DROP TABLE feature_dependencies;
             DROP TABLE feature_conveyor_features;
             DROP TABLE feature_specification_revisions;
             DROP TABLE feature_repository_grants;
             DROP TABLE feature_conveyor_state;
             PRAGMA user_version = 4;",
        )
        .unwrap();
    if sabotage {
        connection
            .execute(
                "CREATE TABLE feature_conveyor_state (collision INTEGER)",
                [],
            )
            .unwrap();
    }
}

#[test]
fn master_process_v4_backup_migration_reopen_and_restore_on_failure() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    downgrade_v5_database_to_v4(&database, false);
    assert!(matches!(
        MasterKernel::open(&database),
        Err(MasterError::MigrationBackup(_))
    ));
    {
        let process = MasterProcess::acquire(directory.path()).unwrap();
        assert_eq!(process.kernel().schema_version().unwrap(), 5);
        let backup = process.migration_backup_path().unwrap();
        assert!(backup.exists());
        let backup_connection = Connection::open(backup).unwrap();
        assert_eq!(
            backup_connection
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            4
        );
        assert_eq!(
            backup_connection
                .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
                .unwrap(),
            "ok"
        );
    }
    assert_eq!(
        MasterProcess::acquire(directory.path())
            .unwrap()
            .kernel()
            .schema_version()
            .unwrap(),
        5
    );

    let failed_directory = tempdir().unwrap();
    let failed_database = failed_directory.path().join("master.sqlite3");
    downgrade_v5_database_to_v4(&failed_database, true);
    let migration_error = match MasterProcess::acquire(failed_directory.path()) {
        Ok(_) => panic!("sabotaged v4 migration unexpectedly succeeded"),
        Err(error) => error,
    };
    assert!(matches!(migration_error, MasterError::Storage(_)));
    let restored = Connection::open(&failed_database).unwrap();
    assert_eq!(
        restored
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        4
    );
    assert_eq!(
        restored
            .query_row("PRAGMA integrity_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    assert!(!failed_directory
        .path()
        .read_dir()
        .unwrap()
        .any(|entry| entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".master.restore.")));
    assert!(failed_directory
        .path()
        .read_dir()
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| entry
            .file_name()
            .to_string_lossy()
            .starts_with("master.pre-v5.")));
}

#[test]
fn forward_schema_version_fails_closed_without_backup() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    drop(MasterKernel::open(&database).unwrap());
    Connection::open(&database)
        .unwrap()
        .execute_batch("PRAGMA user_version = 6;")
        .unwrap();
    let forward_error = match MasterProcess::acquire(directory.path()) {
        Ok(_) => panic!("forward schema unexpectedly opened"),
        Err(error) => error,
    };
    assert!(matches!(
        forward_error,
        MasterError::UnsupportedSchemaVersion {
            expected: 5,
            found: 6
        }
    ));
    assert!(!directory
        .path()
        .read_dir()
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| entry
            .file_name()
            .to_string_lossy()
            .starts_with("master.pre-v5.")));
}
