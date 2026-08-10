use assemblywright_master::{
    ApprovedFeatureSpecification, DeviceRegistration, FeatureAbandonmentEvidence,
    FeatureConveyorGuidanceReason, FeatureConveyorGuidanceState, FeatureConveyorNextOwnerAction,
    FeatureGrantRevisions, FeatureLifecycleStatus, FeatureTransitionEvidence, MasterError,
    MasterKernel, MasterProcess, RepositoryGrantKind, RepositoryGrantRevision,
    VerifiedFeatureSuccess, MAX_CONVEYOR_NONTERMINAL_FEATURES, MAX_CONVEYOR_STATUS_FEATURES,
};
use assemblywright_protocol::{CapabilityDescriptor, DeviceId, DeviceRole};
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
                0,
                0,
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

fn bridge_registration(name: &str) -> DeviceRegistration {
    DeviceRegistration {
        device_id: DeviceId::new(Uuid::new_v4()),
        device_name: name.to_string(),
        role: DeviceRole::MacBridge,
        registry_revision: 1,
        capabilities: vec![CapabilityDescriptor::mlx_reasoning(
            "owner-control-mlx",
            32 * 1024,
            32 * 1024,
        )],
    }
}

#[test]
fn owner_control_designation_is_explicit_cas_bound_role_checked_and_audited() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    let mut kernel = MasterKernel::open(&database).unwrap();
    assert_eq!(kernel.schema_version().unwrap(), 8);
    assert_eq!(kernel.owner_control_bridge_designation().unwrap(), None);

    let fixture = DeviceRegistration {
        device_id: DeviceId::new(Uuid::new_v4()),
        device_name: "fixture-bridge".to_string(),
        role: DeviceRole::MacBridge,
        registry_revision: 1,
        capabilities: vec![CapabilityDescriptor::fixture_reasoning()],
    };
    let worker = DeviceRegistration {
        role: DeviceRole::InferenceWorker,
        ..bridge_registration("worker")
    };
    let first = bridge_registration("owner-bridge-a");
    let second = bridge_registration("owner-bridge-b");
    for registration in [&fixture, &worker, &first, &second] {
        kernel.register_device(registration).unwrap();
    }

    assert!(matches!(
        kernel
            .designate_owner_control_bridge(DeviceId::new(Uuid::new_v4()), 0, 10)
            .unwrap_err(),
        MasterError::DeviceNotRegistered
    ));
    for denied in [&fixture, &worker] {
        assert!(matches!(
            kernel
                .designate_owner_control_bridge(denied.device_id, 0, 11)
                .unwrap_err(),
            MasterError::OwnerControlBridgeUnauthorized
        ));
    }
    assert_eq!(kernel.owner_control_bridge_designation().unwrap(), None);

    install_audit_failure(&database, "owner_control_bridge_designated");
    assert!(matches!(
        kernel
            .designate_owner_control_bridge(first.device_id, 0, 12)
            .unwrap_err(),
        MasterError::Storage(_)
    ));
    assert_eq!(kernel.owner_control_bridge_designation().unwrap(), None);
    remove_audit_failure(&database);

    let designated = kernel
        .designate_owner_control_bridge(first.device_id, 0, 13)
        .unwrap();
    assert_eq!(designated.device_id, first.device_id);
    assert_eq!(designated.registry_revision, first.registry_revision);
    assert_eq!(designated.designation_revision, 1);
    assert!(matches!(
        kernel
            .designate_owner_control_bridge(second.device_id, 0, 14)
            .unwrap_err(),
        MasterError::StaleOwnerControlDesignationRevision {
            expected: 0,
            found: 1
        }
    ));
    assert_eq!(
        kernel.owner_control_bridge_designation().unwrap(),
        Some(designated)
    );

    let rebound = kernel
        .designate_owner_control_bridge(second.device_id, 1, 15)
        .unwrap();
    assert_eq!(rebound.device_id, second.device_id);
    assert_eq!(rebound.designation_revision, 2);
    let connection = Connection::open(database).unwrap();
    let audits = connection
        .prepare(
            "SELECT redacted_metadata_json FROM feature_conveyor_audit
             WHERE event_kind = 'owner_control_bridge_designated' ORDER BY audit_id",
        )
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(audits.len(), 2);
    for audit in audits {
        assert!(!audit.contains(&first.device_id.0.to_string()));
        assert!(!audit.contains(&second.device_id.0.to_string()));
        assert!(!audit.contains("owner-control-mlx"));
        assert!(!audit.contains("owner-bridge"));
    }
}

#[test]
fn repository_grant_owner_control_is_cas_pause_bound_redacted_and_projected() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    let mut kernel = MasterKernel::open(&database).unwrap();
    let repository_id = Uuid::new_v4();
    let empty = kernel.repository_grant_set(repository_id, 10).unwrap();
    assert_eq!(empty.schema_version, 1);
    assert_eq!(empty.repository_id, repository_id);
    assert!(!empty.emergency_paused);
    assert_eq!(empty.emergency_pause_revision, 0);
    assert_eq!(empty.registration, None);
    assert_eq!(empty.cloud_disclosure, None);
    assert_eq!(empty.autonomous_publication, None);

    let registration = RepositoryGrantRevision {
        repository_id,
        kind: RepositoryGrantKind::Registration,
        revision: 1,
        scope_sha256: digest("private repository path and workflow scope"),
        owner_approval_sha256: digest("private registration approval"),
        expires_at_ms: Some(100),
        revoked: false,
    };
    kernel
        .record_repository_grant_revision(&registration, 0, 0, 10)
        .unwrap();
    let active = kernel.repository_grant_set(repository_id, 99).unwrap();
    let active_registration = active.registration.unwrap();
    assert_eq!(active_registration.revision, 1);
    assert!(active_registration.active);
    assert!(!active_registration.revoked);
    assert_eq!(active_registration.expires_at_ms, Some(100));
    assert!(
        !kernel
            .repository_grant_set(repository_id, 100)
            .unwrap()
            .registration
            .unwrap()
            .active
    );

    let revision_two = RepositoryGrantRevision {
        revision: 2,
        scope_sha256: digest("replacement registration scope"),
        owner_approval_sha256: digest("replacement registration approval"),
        expires_at_ms: None,
        ..registration
    };
    assert!(matches!(
        kernel
            .record_repository_grant_revision(&revision_two, 0, 0, 20)
            .unwrap_err(),
        MasterError::StaleRepositoryGrantRevision {
            expected: 0,
            found: 1
        }
    ));
    let skipped = RepositoryGrantRevision {
        revision: 3,
        ..revision_two
    };
    assert!(matches!(
        kernel
            .record_repository_grant_revision(&skipped, 1, 0, 20)
            .unwrap_err(),
        MasterError::InvalidFeatureConveyorInput(_)
    ));
    let expired = RepositoryGrantRevision {
        repository_id,
        kind: RepositoryGrantKind::CloudDisclosure,
        revision: 1,
        scope_sha256: digest("expired cloud scope"),
        owner_approval_sha256: digest("expired cloud approval"),
        expires_at_ms: Some(20),
        revoked: false,
    };
    assert!(matches!(
        kernel
            .record_repository_grant_revision(&expired, 0, 0, 20)
            .unwrap_err(),
        MasterError::InvalidFeatureConveyorInput(_)
    ));

    kernel.set_emergency_paused_at(true, 30).unwrap();
    let cloud = RepositoryGrantRevision {
        expires_at_ms: None,
        ..expired
    };
    assert!(matches!(
        kernel
            .record_repository_grant_revision(&cloud, 0, 1, 31)
            .unwrap_err(),
        MasterError::EmergencyPaused
    ));
    let revoked_cloud = RepositoryGrantRevision {
        revoked: true,
        ..cloud
    };
    kernel
        .record_repository_grant_revision(&revoked_cloud, 0, 1, 32)
        .unwrap();
    let paused = kernel.repository_grant_set(repository_id, 33).unwrap();
    assert!(paused.emergency_paused);
    assert_eq!(paused.emergency_pause_revision, 1);
    assert!(paused.cloud_disclosure.unwrap().revoked);
    assert!(!paused.cloud_disclosure.unwrap().active);

    let publication = RepositoryGrantRevision {
        repository_id,
        kind: RepositoryGrantKind::AutonomousPublication,
        revision: 1,
        scope_sha256: digest("publication scope"),
        owner_approval_sha256: digest("publication approval"),
        expires_at_ms: None,
        revoked: false,
    };
    assert!(matches!(
        kernel
            .record_repository_grant_revision(&publication, 0, 0, 34)
            .unwrap_err(),
        MasterError::StaleEmergencyPauseRevision {
            expected: 0,
            found: 1
        }
    ));
    kernel.set_emergency_paused_at(false, 35).unwrap();
    install_audit_failure(&database, "repository_grant_revision_recorded");
    assert!(matches!(
        kernel
            .record_repository_grant_revision(&publication, 0, 2, 36)
            .unwrap_err(),
        MasterError::Storage(_)
    ));
    assert_eq!(
        kernel
            .repository_grant_set(repository_id, 36)
            .unwrap()
            .autonomous_publication,
        None
    );
    remove_audit_failure(&database);
    kernel
        .record_repository_grant_revision(&publication, 0, 2, 37)
        .unwrap();

    let connection = Connection::open(database).unwrap();
    let audits = connection
        .prepare(
            "SELECT redacted_metadata_json FROM feature_conveyor_audit
             WHERE event_kind = 'repository_grant_revision_recorded' ORDER BY audit_id",
        )
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(audits.len(), 3);
    for audit in audits {
        assert!(!audit.contains(&repository_id.to_string()));
        assert!(!audit.contains("private"));
        assert!(audit.contains("\"side_effect_executed\":false"));
    }
}

#[test]
fn repository_preflight_rechecks_active_registration_pause_and_redacted_audit_atomically() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    let mut kernel = MasterKernel::open(&database).unwrap();
    let repository_id = Uuid::new_v4();
    let scope_sha256 = digest("exact canonical path branch and head scope");
    let registration = RepositoryGrantRevision {
        repository_id,
        kind: RepositoryGrantKind::Registration,
        revision: 1,
        scope_sha256,
        owner_approval_sha256: digest("owner registration approval"),
        expires_at_ms: Some(100),
        revoked: false,
    };
    kernel
        .record_repository_grant_revision(&registration, 0, 0, 10)
        .unwrap();

    kernel
        .authorize_repository_preflight(repository_id, 1, &scope_sha256, 0, 50)
        .unwrap();
    assert!(matches!(
        kernel
            .authorize_repository_preflight(repository_id, 2, &scope_sha256, 0, 50)
            .unwrap_err(),
        MasterError::RepositoryGrantUnavailable
    ));
    assert!(matches!(
        kernel
            .authorize_repository_preflight(repository_id, 1, &digest("wrong scope"), 0, 50)
            .unwrap_err(),
        MasterError::RepositoryGrantUnavailable
    ));
    assert!(matches!(
        kernel
            .authorize_repository_preflight(Uuid::new_v4(), 1, &scope_sha256, 0, 50)
            .unwrap_err(),
        MasterError::RepositoryGrantUnavailable
    ));
    assert!(matches!(
        kernel
            .authorize_repository_preflight(repository_id, 1, &scope_sha256, 0, 100)
            .unwrap_err(),
        MasterError::RepositoryGrantUnavailable
    ));
    assert!(matches!(
        kernel
            .authorize_repository_preflight(Uuid::nil(), 1, &scope_sha256, 0, 50)
            .unwrap_err(),
        MasterError::InvalidFeatureConveyorInput(_)
    ));
    assert!(matches!(
        kernel
            .authorize_repository_preflight(repository_id, 0, &scope_sha256, 0, 50)
            .unwrap_err(),
        MasterError::InvalidFeatureConveyorInput(_)
    ));

    kernel.set_emergency_paused_at(true, 51).unwrap();
    assert!(matches!(
        kernel
            .authorize_repository_preflight(repository_id, 1, &scope_sha256, 1, 52)
            .unwrap_err(),
        MasterError::EmergencyPaused
    ));
    assert!(matches!(
        kernel
            .authorize_repository_preflight(repository_id, 1, &scope_sha256, 0, 52)
            .unwrap_err(),
        MasterError::StaleEmergencyPauseRevision {
            expected: 0,
            found: 1
        }
    ));
    kernel.set_emergency_paused_at(false, 53).unwrap();

    install_audit_failure(&database, "repository_identity_preflight_eligible");
    assert!(matches!(
        kernel
            .record_repository_preflight(repository_id, 1, &scope_sha256, 2, 54)
            .unwrap_err(),
        MasterError::Storage(_)
    ));
    assert_eq!(
        Connection::open(&database)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM feature_conveyor_audit
                 WHERE event_kind = 'repository_identity_preflight_eligible'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
    remove_audit_failure(&database);
    kernel
        .record_repository_preflight(repository_id, 1, &scope_sha256, 2, 55)
        .unwrap();

    let revoked = RepositoryGrantRevision {
        revision: 2,
        expires_at_ms: None,
        revoked: true,
        ..registration
    };
    kernel
        .record_repository_grant_revision(&revoked, 1, 2, 56)
        .unwrap();
    assert!(matches!(
        kernel
            .authorize_repository_preflight(repository_id, 2, &scope_sha256, 2, 57)
            .unwrap_err(),
        MasterError::RepositoryGrantUnavailable
    ));

    let audit: String = Connection::open(database)
        .unwrap()
        .query_row(
            "SELECT redacted_metadata_json FROM feature_conveyor_audit
             WHERE event_kind = 'repository_identity_preflight_eligible'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!audit.contains(&repository_id.to_string()));
    assert!(!audit.contains("path"));
    assert!(!audit.contains("branch"));
    assert!(!audit.contains("commit"));
    assert!(!audit.contains("error"));
    assert!(audit.contains("\"point_in_time\":true"));
    assert!(audit.contains("\"identity_only\":true"));
    assert!(!audit.contains("clean"));
    assert!(audit.contains("\"side_effect_executed\":false"));
}

#[test]
fn owner_bridge_enqueue_binds_designation_queue_pause_and_existing_approval_mechanics() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    let mut kernel = MasterKernel::open(&database).unwrap();
    let owner = bridge_registration("owner-bridge");
    let other = bridge_registration("other-bridge");
    kernel.register_device(&owner).unwrap();
    kernel.register_device(&other).unwrap();
    let repository_id = Uuid::new_v4();
    install_grants(&mut kernel, repository_id);
    let feature = specification(Uuid::new_v4(), repository_id, vec![]);

    assert!(matches!(
        kernel
            .enqueue_approved_feature_from_owner_bridge(&feature, 0, 1, 0, &owner, 20)
            .unwrap_err(),
        MasterError::StaleOwnerControlDesignationRevision {
            expected: 1,
            found: 0
        }
    ));
    let designation = kernel
        .designate_owner_control_bridge(owner.device_id, 0, 21)
        .unwrap();
    Connection::open(&database)
        .unwrap()
        .execute(
            "UPDATE master_devices SET registry_revision = 2 WHERE device_id = ?1",
            [owner.device_id.0.to_string()],
        )
        .unwrap();
    let drifted_owner = DeviceRegistration {
        registry_revision: 2,
        ..owner.clone()
    };
    assert!(matches!(
        kernel
            .enqueue_approved_feature_from_owner_bridge(
                &feature,
                0,
                designation.designation_revision,
                0,
                &drifted_owner,
                21,
            )
            .unwrap_err(),
        MasterError::OwnerControlBridgeUnauthorized
    ));
    Connection::open(&database)
        .unwrap()
        .execute(
            "UPDATE master_devices SET registry_revision = 1 WHERE device_id = ?1",
            [owner.device_id.0.to_string()],
        )
        .unwrap();
    assert!(matches!(
        kernel
            .enqueue_approved_feature_from_owner_bridge(
                &feature,
                0,
                designation.designation_revision,
                0,
                &other,
                22,
            )
            .unwrap_err(),
        MasterError::OwnerControlBridgeUnauthorized
    ));
    assert!(matches!(
        kernel
            .enqueue_approved_feature_from_owner_bridge(&feature, 0, 2, 0, &owner, 23)
            .unwrap_err(),
        MasterError::StaleOwnerControlDesignationRevision { .. }
    ));

    kernel.set_emergency_paused_at(true, 24).unwrap();
    assert!(matches!(
        kernel
            .enqueue_approved_feature_from_owner_bridge(&feature, 0, 1, 0, &owner, 25)
            .unwrap_err(),
        MasterError::StaleEmergencyPauseRevision {
            expected: 0,
            found: 1
        }
    ));
    assert!(matches!(
        kernel
            .enqueue_approved_feature_from_owner_bridge(&feature, 0, 1, 1, &owner, 26)
            .unwrap_err(),
        MasterError::EmergencyPaused
    ));
    kernel.set_emergency_paused_at(false, 27).unwrap();
    assert!(matches!(
        kernel
            .enqueue_approved_feature_from_owner_bridge(&feature, 0, 1, 1, &owner, 28)
            .unwrap_err(),
        MasterError::StaleEmergencyPauseRevision {
            expected: 1,
            found: 2
        }
    ));
    install_audit_failure(&database, "feature_enqueued");
    assert!(matches!(
        kernel
            .enqueue_approved_feature_from_owner_bridge(&feature, 0, 1, 2, &owner, 28)
            .unwrap_err(),
        MasterError::Storage(_)
    ));
    assert_eq!(kernel.feature_queue_revision().unwrap(), 0);
    assert!(matches!(
        kernel.feature_snapshot(feature.feature_id).unwrap_err(),
        MasterError::FeatureNotFound
    ));
    remove_audit_failure(&database);
    let queued = kernel
        .enqueue_approved_feature_from_owner_bridge(&feature, 0, 1, 2, &owner, 29)
        .unwrap();
    assert_eq!(queued.status, FeatureLifecycleStatus::Queued);
    assert_eq!(queued.lifecycle_revision, 1);
    assert_eq!(kernel.feature_queue_revision().unwrap(), 1);
    assert!(matches!(
        kernel
            .enqueue_approved_feature_from_owner_bridge(&feature, 1, 1, 2, &owner, 30)
            .unwrap_err(),
        MasterError::FeatureSpecificationImmutable
    ));
    assert_eq!(kernel.feature_queue_revision().unwrap(), 1);
}

fn assert_exact_object_keys(value: &Value, expected: &[&str]) {
    let object = value.as_object().expect("JSON object");
    let mut actual = object.keys().map(String::as_str).collect::<Vec<_>>();
    let mut expected = expected.to_vec();
    actual.sort_unstable();
    expected.sort_unstable();
    assert_eq!(actual, expected);
}

fn assert_status_json_allowlist(value: &Value) {
    assert_exact_object_keys(
        value,
        &[
            "schema_version",
            "queue_revision",
            "startup_quarantine_count",
            "counts_by_status",
            "visible_feature_count",
            "features_truncated",
            "features",
            "owner_guidance",
        ],
    );
    assert_exact_object_keys(
        &value["counts_by_status"],
        &[
            "queued",
            "implementing",
            "validating",
            "reviewing",
            "publishing",
            "verifying_main",
            "succeeded",
            "cancelled",
            "abandoned",
            "quarantined",
        ],
    );
    for feature in value["features"].as_array().expect("status feature array") {
        assert_exact_object_keys(
            feature,
            &[
                "feature_id",
                "specification_revision",
                "lifecycle_revision",
                "queue_position",
                "status",
                "lease_present",
                "effect_possible",
            ],
        );
    }
    assert_exact_object_keys(
        &value["owner_guidance"],
        &[
            "state",
            "reason_code",
            "next_owner_action",
            "feature_id",
            "specification_revision",
            "lifecycle_revision",
            "queue_revision",
            "emergency_pause_revision",
        ],
    );
}

fn complete_feature(kernel: &mut MasterKernel, feature: &ApprovedFeatureSpecification, now: u64) {
    let queue_revision = kernel.feature_queue_revision().unwrap();
    kernel
        .enqueue_approved_feature(feature, queue_revision, now)
        .unwrap();
    let claim = kernel
        .claim_next_feature(kernel.feature_queue_revision().unwrap(), now + 1)
        .unwrap();
    let evidence = FeatureTransitionEvidence {
        repository_snapshot_sha256: digest("history-snapshot"),
        accepted_evidence_sha256: digest("history-evidence"),
    };
    let mut lifecycle_revision = claim.lifecycle_revision;
    for (offset, next) in [
        (2, FeatureLifecycleStatus::Validating),
        (3, FeatureLifecycleStatus::Reviewing),
        (4, FeatureLifecycleStatus::Publishing),
        (5, FeatureLifecycleStatus::VerifyingMain),
    ] {
        lifecycle_revision = kernel
            .advance_feature_lifecycle(
                feature.feature_id,
                lifecycle_revision,
                next,
                evidence,
                now + offset,
            )
            .unwrap()
            .lifecycle_revision;
    }
    kernel
        .mark_feature_succeeded(
            feature.feature_id,
            lifecycle_revision,
            kernel.feature_queue_revision().unwrap(),
            VerifiedFeatureSuccess {
                main_commit_sha256: digest("history-main"),
                post_merge_evidence_sha256: digest("history-post-merge"),
                main_healthy: true,
            },
            now + 6,
        )
        .unwrap();
}

fn abandon_feature(kernel: &mut MasterKernel, feature: &ApprovedFeatureSpecification, now: u64) {
    kernel
        .enqueue_approved_feature(feature, kernel.feature_queue_revision().unwrap(), now)
        .unwrap();
    let claim = kernel
        .claim_next_feature(kernel.feature_queue_revision().unwrap(), now + 1)
        .unwrap();
    let cancelled = kernel
        .cancel_active_feature(feature.feature_id, claim.lifecycle_revision, now + 2)
        .unwrap();
    kernel
        .abandon_and_advance(
            feature.feature_id,
            cancelled.lifecycle_revision,
            kernel.feature_queue_revision().unwrap(),
            FeatureAbandonmentEvidence {
                safe_reconciliation_sha256: digest("history-safe-reconciliation"),
                merged: false,
                verified_healthy_main_sha256: None,
            },
            now + 3,
        )
        .unwrap();
}

#[test]
fn status_projection_is_empty_bounded_and_redacted() {
    let mut kernel = MasterKernel::in_memory().unwrap();
    let empty = kernel.feature_conveyor_status().unwrap();
    assert_eq!(empty.schema_version, 8);
    assert_eq!(empty.queue_revision, 0);
    assert_eq!(empty.startup_quarantine_count, 0);
    assert_eq!(empty.visible_feature_count, 0);
    assert!(!empty.features_truncated);
    assert!(empty.features.is_empty());
    assert_eq!(empty.counts_by_status, Default::default());
    assert_eq!(
        empty.owner_guidance.state,
        FeatureConveyorGuidanceState::Idle
    );
    assert_eq!(
        empty.owner_guidance.reason_code,
        FeatureConveyorGuidanceReason::QueueEmpty
    );
    assert_eq!(
        empty.owner_guidance.next_owner_action,
        FeatureConveyorNextOwnerAction::PrepareApprovedFeature
    );
    assert_eq!(empty.owner_guidance.feature_id, None);
    assert_eq!(empty.owner_guidance.queue_revision, 0);
    assert_eq!(empty.owner_guidance.emergency_pause_revision, 0);
    assert_status_json_allowlist(&serde_json::to_value(&empty).unwrap());

    let repository_id = Uuid::new_v4();
    install_grants(&mut kernel, repository_id);
    let features = (0..=MAX_CONVEYOR_NONTERMINAL_FEATURES)
        .map(|_| specification(Uuid::new_v4(), repository_id, vec![]))
        .collect::<Vec<_>>();
    for (index, feature) in features
        .iter()
        .take(MAX_CONVEYOR_NONTERMINAL_FEATURES as usize)
        .enumerate()
    {
        kernel
            .enqueue_approved_feature(feature, index as u64, 10 + index as u64)
            .unwrap();
    }
    let claim = kernel
        .claim_next_feature(MAX_CONVEYOR_NONTERMINAL_FEATURES, 200)
        .unwrap();
    kernel
        .cancel_active_feature(claim.feature_id, claim.lifecycle_revision, 201)
        .unwrap();
    kernel
        .enqueue_approved_feature(
            features.last().unwrap(),
            MAX_CONVEYOR_NONTERMINAL_FEATURES + 1,
            202,
        )
        .unwrap();

    let status = kernel.feature_conveyor_status().unwrap();
    assert_eq!(
        status.visible_feature_count,
        MAX_CONVEYOR_NONTERMINAL_FEATURES + 1
    );
    assert!(status.features_truncated);
    assert_eq!(status.features.len(), MAX_CONVEYOR_STATUS_FEATURES);
    assert_eq!(status.features[0].feature_id, claim.feature_id);
    assert_eq!(status.features[0].status, FeatureLifecycleStatus::Cancelled);
    assert!(status.features[0].lease_present);
    assert!(status.features[0].effect_possible);
    assert_eq!(
        status.counts_by_status.queued,
        MAX_CONVEYOR_NONTERMINAL_FEATURES
    );
    assert_eq!(status.counts_by_status.cancelled, 1);
    assert_eq!(status.counts_by_status.implementing, 0);
    assert_eq!(
        status.owner_guidance.state,
        FeatureConveyorGuidanceState::Blocked
    );
    assert_eq!(
        status.owner_guidance.reason_code,
        FeatureConveyorGuidanceReason::ActiveRequiresReconciliation
    );
    assert_eq!(
        status.owner_guidance.next_owner_action,
        FeatureConveyorNextOwnerAction::ReconcileActiveFeature
    );
    assert_eq!(status.owner_guidance.feature_id, Some(claim.feature_id));
    assert_eq!(status.owner_guidance.queue_revision, status.queue_revision);
    assert_eq!(
        kernel.feature_conveyor_status().unwrap(),
        status,
        "read-only projection changed durable Feature Conveyor state"
    );
    let positions = status
        .features
        .iter()
        .map(|feature| feature.queue_position)
        .collect::<Vec<_>>();
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "status features were not deterministically queue ordered"
    );

    assert_status_json_allowlist(&serde_json::to_value(&status).unwrap());
}

#[test]
fn owner_guidance_reports_ready_dependency_blocked_and_pause_precedence() {
    let mut kernel = MasterKernel::in_memory().unwrap();
    let repository_id = Uuid::new_v4();
    install_grants(&mut kernel, repository_id);
    let dependency = specification(Uuid::new_v4(), repository_id, vec![]);
    let dependent = specification(Uuid::new_v4(), repository_id, vec![dependency.feature_id]);
    kernel.enqueue_approved_feature(&dependency, 0, 10).unwrap();
    let ready = kernel.feature_conveyor_status().unwrap();
    assert_eq!(ready.owner_guidance.emergency_pause_revision, 0);
    assert_eq!(
        ready.owner_guidance.state,
        FeatureConveyorGuidanceState::Ready
    );
    assert_eq!(
        ready.owner_guidance.reason_code,
        FeatureConveyorGuidanceReason::HeadDependencySatisfied
    );
    assert_eq!(
        ready.owner_guidance.next_owner_action,
        FeatureConveyorNextOwnerAction::AwaitOwnerControlSurface
    );
    assert_eq!(ready.owner_guidance.feature_id, Some(dependency.feature_id));
    assert_eq!(ready.owner_guidance.specification_revision, Some(1));
    assert_eq!(ready.owner_guidance.lifecycle_revision, Some(1));

    kernel.enqueue_approved_feature(&dependent, 1, 11).unwrap();
    kernel
        .reorder_queued_features(&[dependent.feature_id, dependency.feature_id], 2, 12)
        .unwrap();
    let blocked = kernel.feature_conveyor_status().unwrap();
    assert_eq!(
        blocked.owner_guidance.state,
        FeatureConveyorGuidanceState::Blocked
    );
    assert_eq!(
        blocked.owner_guidance.reason_code,
        FeatureConveyorGuidanceReason::HeadDependencyUnsatisfied
    );
    assert_eq!(
        blocked.owner_guidance.next_owner_action,
        FeatureConveyorNextOwnerAction::ResolveHeadDependency
    );
    assert_eq!(
        blocked.owner_guidance.feature_id,
        Some(dependent.feature_id)
    );
    assert_eq!(blocked.owner_guidance.queue_revision, 3);

    kernel.set_emergency_paused_at(true, 13).unwrap();
    let paused = kernel.feature_conveyor_status().unwrap();
    assert_eq!(
        paused.owner_guidance.state,
        FeatureConveyorGuidanceState::Blocked
    );
    assert_eq!(
        paused.owner_guidance.reason_code,
        FeatureConveyorGuidanceReason::EmergencyPaused
    );
    assert_eq!(
        paused.owner_guidance.next_owner_action,
        FeatureConveyorNextOwnerAction::ResumeEmergencyPause
    );
    assert_eq!(paused.owner_guidance.feature_id, None);
    assert_eq!(paused.owner_guidance.queue_revision, 3);
    assert_eq!(paused.owner_guidance.emergency_pause_revision, 1);
    kernel.set_emergency_paused_at(true, 14).unwrap();
    assert_eq!(kernel.emergency_pause_revision().unwrap(), 1);
    kernel.set_emergency_paused_at(false, 15).unwrap();
    let resumed = kernel.feature_conveyor_status().unwrap();
    assert_eq!(resumed.owner_guidance.emergency_pause_revision, 2);
    assert_eq!(resumed.owner_guidance.queue_revision, 3);
    assert_eq!(
        resumed.owner_guidance.reason_code,
        FeatureConveyorGuidanceReason::HeadDependencyUnsatisfied
    );
    assert_eq!(
        kernel.feature_conveyor_status().unwrap(),
        resumed,
        "owner guidance changed durable Feature Conveyor state"
    );
    assert_status_json_allowlist(&serde_json::to_value(resumed).unwrap());
}

#[test]
fn owner_guidance_fails_closed_on_impossible_unleased_queue_state() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    let repository_id = Uuid::new_v4();
    let feature = specification(Uuid::new_v4(), repository_id, vec![]);
    {
        let mut kernel = MasterKernel::open(&database).unwrap();
        install_grants(&mut kernel, repository_id);
        kernel.enqueue_approved_feature(&feature, 0, 10).unwrap();
    }
    Connection::open(&database)
        .unwrap()
        .execute(
            "UPDATE feature_conveyor_features SET status = 'cancelled'
             WHERE feature_id = ?1",
            [feature.feature_id.to_string()],
        )
        .unwrap();

    let kernel = MasterKernel::open(&database).unwrap();
    assert!(matches!(
        kernel.feature_conveyor_status(),
        Err(MasterError::InvalidStoredState(message))
            if message == "unleased Feature Conveyor queue head is not queued"
    ));
}

#[test]
fn status_projection_excludes_substantial_terminal_history() {
    let mut kernel = MasterKernel::in_memory().unwrap();
    let repository_id = Uuid::new_v4();
    install_grants(&mut kernel, repository_id);
    for index in 0..24 {
        let succeeded = specification(Uuid::new_v4(), repository_id, vec![]);
        complete_feature(&mut kernel, &succeeded, 1_000 + index * 20);
        let abandoned = specification(Uuid::new_v4(), repository_id, vec![]);
        abandon_feature(&mut kernel, &abandoned, 1_010 + index * 20);
    }
    let current = specification(Uuid::new_v4(), repository_id, vec![]);
    kernel
        .enqueue_approved_feature(&current, kernel.feature_queue_revision().unwrap(), 10_000)
        .unwrap();

    let status = kernel.feature_conveyor_status().unwrap();
    assert_eq!(status.visible_feature_count, 1);
    assert_eq!(status.features.len(), 1);
    assert_eq!(status.features[0].feature_id, current.feature_id);
    assert_eq!(status.counts_by_status.queued, 1);
    assert_eq!(status.counts_by_status.succeeded, 0);
    assert_eq!(status.counts_by_status.abandoned, 0);
    assert!(!status.features_truncated);
    assert_status_json_allowlist(&serde_json::to_value(status).unwrap());
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
        let active = process.kernel().feature_conveyor_status().unwrap();
        assert_eq!(
            active.owner_guidance.state,
            FeatureConveyorGuidanceState::InProgress
        );
        assert_eq!(
            active.owner_guidance.reason_code,
            FeatureConveyorGuidanceReason::ActiveFeatureLeased
        );
        assert_eq!(
            active.owner_guidance.next_owner_action,
            FeatureConveyorNextOwnerAction::Wait
        );
        assert_eq!(active.owner_guidance.feature_id, Some(feature.feature_id));
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

    let stale_grant = kernel
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
            0,
            0,
            4,
        )
        .unwrap_err();
    assert!(matches!(
        stale_grant,
        MasterError::StaleRepositoryGrantRevision {
            expected: 0,
            found: 1
        }
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
            1,
            0,
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
    let status = kernel.feature_conveyor_status().unwrap();
    assert_eq!(
        status.owner_guidance.state,
        FeatureConveyorGuidanceState::Blocked
    );
    assert_eq!(
        status.owner_guidance.reason_code,
        FeatureConveyorGuidanceReason::ActiveRequiresReconciliation
    );
    assert_eq!(
        status.owner_guidance.next_owner_action,
        FeatureConveyorNextOwnerAction::ReconcileActiveFeature
    );
    assert_eq!(status.owner_guidance.feature_id, Some(feature.feature_id));
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
             DROP TRIGGER master_identity_rebind_audit_no_update;
             DROP TRIGGER master_identity_rebind_audit_no_delete;
             DROP TRIGGER master_pending_capability_rebinds_no_delete;
             DROP TRIGGER master_pending_capability_rebinds_terminal_only;
             DROP TABLE master_identity_rebind_audit;
             DROP TABLE master_pending_capability_rebinds;
             DROP TABLE feature_owner_control_state;
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
        assert_eq!(process.kernel().schema_version().unwrap(), 8);
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
        8
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
            .starts_with("master.pre-v8.")));
}

#[test]
fn master_process_v6_backup_migration_binds_pause_and_default_inert_owner_control() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    drop(MasterKernel::open(&database).unwrap());
    Connection::open(&database)
        .unwrap()
        .execute_batch(
            "DROP TABLE feature_owner_control_state;
             DELETE FROM master_metadata WHERE key = 'emergency_pause_revision';
             PRAGMA user_version = 6;",
        )
        .unwrap();

    assert!(matches!(
        MasterKernel::open(&database),
        Err(MasterError::MigrationBackup(_))
    ));

    let process = MasterProcess::acquire(directory.path()).unwrap();
    assert_eq!(process.kernel().schema_version().unwrap(), 8);
    assert_eq!(process.kernel().emergency_pause_revision().unwrap(), 0);
    assert_eq!(
        process.kernel().owner_control_bridge_designation().unwrap(),
        None
    );
    let backup = process.migration_backup_path().unwrap();
    assert!(backup
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("master.pre-v8."));
    let backup = Connection::open(backup).unwrap();
    assert_eq!(
        backup
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        6
    );
    assert_eq!(
        backup
            .query_row(
                "SELECT COUNT(*) FROM master_metadata
                 WHERE key = 'emergency_pause_revision'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
}

#[test]
fn forward_schema_version_fails_closed_without_backup() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    drop(MasterKernel::open(&database).unwrap());
    Connection::open(&database)
        .unwrap()
        .execute_batch("PRAGMA user_version = 9;")
        .unwrap();
    let forward_error = match MasterProcess::acquire(directory.path()) {
        Ok(_) => panic!("forward schema unexpectedly opened"),
        Err(error) => error,
    };
    assert!(matches!(
        forward_error,
        MasterError::UnsupportedSchemaVersion {
            expected: 8,
            found: 9
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
            .starts_with("master.pre-v8.")));
}
