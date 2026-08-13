use assemblywright_master::{
    publication_branch_policy_sha256, ApprovedFeatureSpecification,
    ArtifactIntegrationAuthorization, ArtifactIntegrationStore, DeviceRegistration,
    FeatureAbandonmentEvidence, FeatureConveyorGuidanceReason, FeatureConveyorGuidanceState,
    FeatureConveyorNextOwnerAction, FeatureGrantRevisions, FeatureLifecycleStatus,
    FeatureSnapshotClaimPlan, FeatureTransitionEvidence, MasterError, MasterKernel, MasterProcess,
    PublicationActionEvidence, PublicationActionKind, PublicationAuthorization, RemoteWorkContract,
    RepositoryGrantKind, RepositoryGrantRevision, RepositorySnapshotEvidence,
    ReviewGatewayAuthorization, ReviewTransportFailure, ValidationCommandEvidence,
    ValidationGateAuthorization, ValidationGateEvidence, VerifiedFeatureSuccess,
    MASTER_SCHEMA_VERSION, MAX_CONVEYOR_NONTERMINAL_FEATURES, MAX_CONVEYOR_STATUS_FEATURES,
};
use assemblywright_protocol::{
    build_local_coding_fixture_patch_artifact, local_coding_admission_sha256, CapabilityDescriptor,
    DeviceId, DeviceRole, FeatureConveyorArtifactIntegrationRequest,
    FeatureConveyorCodingDispatchRequest, FeatureConveyorCodingWorkPacketMetadata,
    FeatureConveyorGrantRevisions, FeatureConveyorKnowledgeBaseDetermination,
    FeatureConveyorPublicationRequest, FeatureConveyorReviewCoverageStatus,
    FeatureConveyorReviewDecision, FeatureConveyorReviewFinding,
    FeatureConveyorReviewGatewayRequest, FeatureConveyorReviewPacket,
    FeatureConveyorReviewProviderOutput, FeatureConveyorReviewRequirementCoverage,
    FeatureConveyorValidationCommandId, FeatureConveyorValidationGateRequest, HandshakeRequest,
    JobEnvelope, JobResultEnvelope, JobResultStatus, LocalCodingJobResult,
    LocalCodingResultArtifact, LocalCodingResultArtifactAdmission, LocalCodingSnapshotChunkRequest,
    FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
    FEATURE_CONVEYOR_PUBLICATION_COORDINATOR_SCHEMA_VERSION,
    FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION, LOCAL_CODING_COMPLETED_STATUS,
    LOCAL_CODING_FIXTURE_CONTENT, LOCAL_CODING_FIXTURE_TEST_STATUS, PROTOCOL_VERSION,
};
use rusqlite::{params, Connection};
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs;
use std::process::Command;
use tempfile::tempdir;
use uuid::Uuid;

#[test]
fn artifact_integration_and_validation_gate_freeze_candidate_advance_and_reject_drift() {
    let directory = tempdir().unwrap();
    let source_path = directory.path().join("source");
    let source = git2::Repository::init(&source_path).unwrap();
    fs::write(source_path.join("README.md"), b"before\n").unwrap();
    let mut index = source.index().unwrap();
    index.add_path(std::path::Path::new("README.md")).unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = source.find_tree(tree_id).unwrap();
    let sig =
        git2::Signature::new("fixture", "fixture@example.invalid", &git2::Time::new(1, 0)).unwrap();
    let base = source
        .commit(Some("HEAD"), &sig, &sig, "base", &tree, &[])
        .unwrap()
        .to_string();
    drop(tree);
    drop(source);
    let snapshot = assemblywright_master::RepositorySnapshotStore::open(directory.path())
        .unwrap()
        .prepare(&source_path, &base)
        .unwrap();
    let database = directory.path().join("master.sqlite3");
    let mut kernel = MasterKernel::open(&database).unwrap();
    let repository_id = Uuid::new_v4();
    install_grants(&mut kernel, repository_id);
    let feature = specification(Uuid::new_v4(), repository_id, vec![]);
    kernel.enqueue_approved_feature(&feature, 0, 10).unwrap();
    let mut plan = snapshot_plan(&feature, 1, 0);
    plan.base_commit = base.clone();
    let plan = kernel.prepare_repository_snapshot_claim(&plan, 11).unwrap();
    let claim = kernel
        .finalize_repository_snapshot_claim(
            &plan,
            &RepositorySnapshotEvidence {
                snapshot_id: snapshot.snapshot_id,
                snapshot_sha256: snapshot.snapshot_sha256,
                base_commit: base.clone(),
            },
            11,
        )
        .unwrap();
    let device = coding_registration("integration-worker");
    kernel.register_device(&device).unwrap();
    let mut dispatch = coding_dispatch_request(&claim, &device, 2, 0);
    dispatch.work_packet = FeatureConveyorCodingWorkPacketMetadata::fixture(
        dispatch.work_packet.packet_id,
        Sha256::digest(b"before\n").into(),
    );
    dispatch.work_packet_sha256 = dispatch.work_packet.canonical_sha256().unwrap();
    kernel.dispatch_feature_coding(&dispatch, 12).unwrap();
    let epoch = kernel
        .accept_handshake(
            &HandshakeRequest {
                protocol_version: PROTOCOL_VERSION,
                device_id: device.device_id,
                device_name: device.device_name.clone(),
                role: device.role,
                registry_revision: 1,
                capabilities: device.capabilities.clone(),
            },
            13,
        )
        .unwrap()
        .connection_epoch;
    let contract = RemoteWorkContract::from_registration(&device).unwrap();
    let job = kernel
        .lease_next_remote_step(device.device_id, epoch, 14, &contract)
        .unwrap();
    let result = coding_ack(&job, job.sequence + 1);
    let admission = coding_artifact_admission(&job, &result);
    kernel
        .finalize_local_coding_result_artifact(device.device_id, &admission, 15)
        .unwrap();
    let artifact_store =
        assemblywright_master::ResultArtifactStore::open(directory.path()).unwrap();
    let bytes = admission.artifact.validate().unwrap();
    let mut artifact = artifact_store
        .prepare(
            admission.artifact.artifact_id,
            admission.artifact.artifact_sha256,
            &bytes,
        )
        .unwrap();
    artifact.mark_committed().unwrap();
    kernel
        .accept_remote_result_from_with_artifact(
            device.device_id,
            &result,
            16,
            &contract,
            &artifact_store,
            artifact.verified_mut(),
        )
        .unwrap();
    let request = FeatureConveyorArtifactIntegrationRequest {
        schema_version: 1,
        integration_id: Uuid::new_v4(),
        feature_id: feature.feature_id,
        specification_revision: 1,
        expected_lifecycle_revision: claim.lifecycle_revision,
        feature_lease_id: claim.lease_id,
        snapshot_id: claim.snapshot_id,
        snapshot_sha256: claim.snapshot_sha256,
        artifact_ids: vec![admission.artifact.artifact_id],
        expected_queue_revision: 2,
        expected_emergency_pause_revision: 0,
        grants: FeatureConveyorGrantRevisions {
            registration: 1,
            cloud_disclosure: 1,
            autonomous_publication: 1,
        },
        base_commit: base,
    };
    let planned = match kernel.prepare_artifact_integration(&request, 17).unwrap() {
        ArtifactIntegrationAuthorization::Planned(p) => p,
        _ => panic!(),
    };
    let store = ArtifactIntegrationStore::open(directory.path()).unwrap();
    let mut candidate = store
        .prepare(
            request.integration_id,
            request.snapshot_id,
            &request.base_commit,
            &planned.artifacts,
        )
        .unwrap();
    candidate.revalidate_artifacts(&artifact_store).unwrap();
    candidate.revalidate_candidate(&store).unwrap();
    let receipt = kernel
        .finalize_artifact_integration(&planned, &candidate.evidence, 18)
        .unwrap();
    candidate.retain();
    snapshot.retain();
    assert_eq!(receipt.lifecycle_revision, claim.lifecycle_revision + 1);
    assert!(matches!(
        kernel.prepare_artifact_integration(&request, 19).unwrap(),
        ArtifactIntegrationAuthorization::Existing(_)
    ));
    let mut drift = request;
    drift.expected_queue_revision += 1;
    assert!(matches!(
        kernel.prepare_artifact_integration(&drift, 19),
        Err(MasterError::ArtifactIntegrationUnavailable)
    ));

    let command_ids = FeatureConveyorValidationCommandId::REQUIRED.to_vec();
    let validation_request = FeatureConveyorValidationGateRequest {
        schema_version: 1,
        validation_id: Uuid::new_v4(),
        feature_id: feature.feature_id,
        specification_revision: feature.revision,
        expected_lifecycle_revision: receipt.lifecycle_revision,
        feature_lease_id: receipt.feature_lease_id,
        snapshot_id: receipt.snapshot_id,
        snapshot_sha256: receipt.snapshot_sha256,
        integration_id: receipt.integration_id,
        artifact_set_sha256: receipt.artifact_set_sha256,
        candidate_commit: receipt.candidate_commit.clone(),
        candidate_tree: receipt.candidate_tree.clone(),
        base_commit: receipt.base_commit.clone(),
        plan_sha256: assemblywright_protocol::feature_conveyor_validation_plan_sha256(&command_ids)
            .unwrap(),
        command_ids: command_ids.clone(),
        expected_queue_revision: receipt.queue_revision,
        expected_emergency_pause_revision: receipt.emergency_pause_revision,
        grants: receipt.grants,
    };
    let validation_plan = match kernel
        .plan_validation_gate(&validation_request, 20)
        .unwrap()
    {
        ValidationGateAuthorization::Planned(plan) => plan,
        other => panic!("unexpected validation authorization: {other:?}"),
    };
    let evidence = ValidationGateEvidence {
        commands: command_ids
            .iter()
            .enumerate()
            .map(|(index, command_id)| ValidationCommandEvidence {
                command_id: *command_id,
                passed: true,
                result_sha256: digest(&format!("validation-command-{index}")),
                duration_ms: index as u64,
                output_truncated: false,
            })
            .collect(),
    };
    let incomplete = ValidationGateEvidence {
        commands: evidence.commands[..evidence.commands.len() - 1].to_vec(),
    };
    assert!(matches!(
        kernel.finalize_validation_gate(&validation_plan, &incomplete, 21),
        Err(MasterError::ValidationGateUnavailable)
    ));
    assert!(kernel
        .validation_gate_execution_is_current(&validation_request, 21)
        .unwrap());
    let validation_receipt = kernel
        .finalize_validation_gate(&validation_plan, &evidence, 21)
        .unwrap();
    assert_eq!(
        validation_receipt.lifecycle_revision,
        receipt.lifecycle_revision + 1
    );
    assert!(matches!(
        kernel
            .plan_validation_gate(&validation_request, 22)
            .unwrap(),
        ValidationGateAuthorization::ExistingPassed { .. }
    ));
    let mut validation_drift = validation_request;
    validation_drift.expected_queue_revision += 1;
    assert!(matches!(
        kernel.plan_validation_gate(&validation_drift, 22),
        Err(MasterError::ValidationGateUnavailable)
    ));
    assert!(!kernel
        .validation_gate_execution_is_current(&validation_drift, 22)
        .unwrap());

    drop(kernel);
    let connection = Connection::open(&database).unwrap();
    assert!(connection
        .execute(
            "UPDATE feature_validation_attempts SET plan_sha256=?1 WHERE validation_id=?2",
            rusqlite::params![
                [0x55_u8; 32].as_slice(),
                validation_receipt.validation_id.to_string()
            ]
        )
        .is_err());
    assert!(connection
        .execute(
            "DELETE FROM feature_validation_command_evidence WHERE validation_id=?1",
            [validation_receipt.validation_id.to_string()]
        )
        .is_err());
}

fn digest(label: &str) -> [u8; 32] {
    Sha256::digest(label.as_bytes()).into()
}

fn integrated_validation_fixture(
    validation_gate: Option<Value>,
) -> (
    tempfile::TempDir,
    MasterKernel,
    FeatureConveyorValidationGateRequest,
    ValidationGateEvidence,
) {
    let directory = tempdir().unwrap();
    let source_path = directory.path().join("source");
    let source = git2::Repository::init(&source_path).unwrap();
    fs::write(source_path.join("README.md"), b"before\n").unwrap();
    let mut index = source.index().unwrap();
    index.add_path(std::path::Path::new("README.md")).unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = source.find_tree(tree_id).unwrap();
    let sig =
        git2::Signature::new("fixture", "fixture@example.invalid", &git2::Time::new(1, 0)).unwrap();
    let base = source
        .commit(Some("HEAD"), &sig, &sig, "base", &tree, &[])
        .unwrap()
        .to_string();
    drop(tree);
    drop(source);
    let snapshot = assemblywright_master::RepositorySnapshotStore::open(directory.path())
        .unwrap()
        .prepare(&source_path, &base)
        .unwrap();
    let mut kernel = MasterKernel::open(directory.path().join("master.sqlite3")).unwrap();
    let repository_id = Uuid::new_v4();
    install_grants(&mut kernel, repository_id);
    let mut feature = specification(Uuid::new_v4(), repository_id, vec![]);
    match validation_gate {
        Some(gate) => {
            feature
                .manifest
                .as_object_mut()
                .unwrap()
                .insert("validation_gate".to_string(), gate);
        }
        None => {
            feature
                .manifest
                .as_object_mut()
                .unwrap()
                .remove("validation_gate");
        }
    }
    feature.manifest_sha256 =
        Sha256::digest(canonical_manifest(&feature.manifest).as_bytes()).into();
    kernel.enqueue_approved_feature(&feature, 0, 10).unwrap();
    let mut snapshot_plan = snapshot_plan(&feature, 1, 0);
    snapshot_plan.base_commit = base.clone();
    let snapshot_plan = kernel
        .prepare_repository_snapshot_claim(&snapshot_plan, 11)
        .unwrap();
    let claim = kernel
        .finalize_repository_snapshot_claim(
            &snapshot_plan,
            &RepositorySnapshotEvidence {
                snapshot_id: snapshot.snapshot_id,
                snapshot_sha256: snapshot.snapshot_sha256,
                base_commit: base.clone(),
            },
            11,
        )
        .unwrap();
    let device = coding_registration("validation-worker");
    kernel.register_device(&device).unwrap();
    let mut dispatch = coding_dispatch_request(&claim, &device, 2, 0);
    dispatch.work_packet = FeatureConveyorCodingWorkPacketMetadata::fixture(
        dispatch.work_packet.packet_id,
        Sha256::digest(b"before\n").into(),
    );
    dispatch.work_packet_sha256 = dispatch.work_packet.canonical_sha256().unwrap();
    kernel.dispatch_feature_coding(&dispatch, 12).unwrap();
    let epoch = kernel
        .accept_handshake(
            &HandshakeRequest {
                protocol_version: PROTOCOL_VERSION,
                device_id: device.device_id,
                device_name: device.device_name.clone(),
                role: device.role,
                registry_revision: 1,
                capabilities: device.capabilities.clone(),
            },
            13,
        )
        .unwrap()
        .connection_epoch;
    let contract = RemoteWorkContract::from_registration(&device).unwrap();
    let job = kernel
        .lease_next_remote_step(device.device_id, epoch, 14, &contract)
        .unwrap();
    let result = coding_ack(&job, job.sequence + 1);
    let admission = coding_artifact_admission(&job, &result);
    kernel
        .finalize_local_coding_result_artifact(device.device_id, &admission, 15)
        .unwrap();
    let artifact_store =
        assemblywright_master::ResultArtifactStore::open(directory.path()).unwrap();
    let bytes = admission.artifact.validate().unwrap();
    let mut artifact = artifact_store
        .prepare(
            admission.artifact.artifact_id,
            admission.artifact.artifact_sha256,
            &bytes,
        )
        .unwrap();
    artifact.mark_committed().unwrap();
    kernel
        .accept_remote_result_from_with_artifact(
            device.device_id,
            &result,
            16,
            &contract,
            &artifact_store,
            artifact.verified_mut(),
        )
        .unwrap();
    let integration_request = FeatureConveyorArtifactIntegrationRequest {
        schema_version: 1,
        integration_id: Uuid::new_v4(),
        feature_id: feature.feature_id,
        specification_revision: feature.revision,
        expected_lifecycle_revision: claim.lifecycle_revision,
        feature_lease_id: claim.lease_id,
        snapshot_id: claim.snapshot_id,
        snapshot_sha256: claim.snapshot_sha256,
        artifact_ids: vec![admission.artifact.artifact_id],
        expected_queue_revision: 2,
        expected_emergency_pause_revision: 0,
        grants: FeatureConveyorGrantRevisions {
            registration: 1,
            cloud_disclosure: 1,
            autonomous_publication: 1,
        },
        base_commit: base,
    };
    let integration_plan = match kernel
        .prepare_artifact_integration(&integration_request, 17)
        .unwrap()
    {
        ArtifactIntegrationAuthorization::Planned(plan) => plan,
        other => panic!("unexpected integration authorization: {other:?}"),
    };
    let store = ArtifactIntegrationStore::open(directory.path()).unwrap();
    let mut candidate = store
        .prepare(
            integration_request.integration_id,
            integration_request.snapshot_id,
            &integration_request.base_commit,
            &integration_plan.artifacts,
        )
        .unwrap();
    candidate.revalidate_artifacts(&artifact_store).unwrap();
    candidate.revalidate_candidate(&store).unwrap();
    let receipt = kernel
        .finalize_artifact_integration(&integration_plan, &candidate.evidence, 18)
        .unwrap();
    candidate.retain();
    snapshot.retain();
    let command_ids = FeatureConveyorValidationCommandId::REQUIRED.to_vec();
    let request = FeatureConveyorValidationGateRequest {
        schema_version: 1,
        validation_id: Uuid::new_v4(),
        feature_id: feature.feature_id,
        specification_revision: feature.revision,
        expected_lifecycle_revision: receipt.lifecycle_revision,
        feature_lease_id: receipt.feature_lease_id,
        snapshot_id: receipt.snapshot_id,
        snapshot_sha256: receipt.snapshot_sha256,
        integration_id: receipt.integration_id,
        artifact_set_sha256: receipt.artifact_set_sha256,
        candidate_commit: receipt.candidate_commit,
        candidate_tree: receipt.candidate_tree,
        base_commit: receipt.base_commit,
        plan_sha256: assemblywright_protocol::feature_conveyor_validation_plan_sha256(&command_ids)
            .unwrap(),
        command_ids: command_ids.clone(),
        expected_queue_revision: receipt.queue_revision,
        expected_emergency_pause_revision: receipt.emergency_pause_revision,
        grants: receipt.grants,
    };
    let evidence = ValidationGateEvidence {
        commands: command_ids
            .iter()
            .enumerate()
            .map(|(index, command_id)| ValidationCommandEvidence {
                command_id: *command_id,
                passed: true,
                result_sha256: digest(&format!("fixture-validation-{index}")),
                duration_ms: index as u64,
                output_truncated: false,
            })
            .collect(),
    };
    (directory, kernel, request, evidence)
}

#[test]
fn validation_gate_preparation_is_read_only_until_resources_are_preflighted() {
    let (directory, mut kernel, request, _) = integrated_validation_fixture(Some(
        specification(Uuid::new_v4(), Uuid::new_v4(), vec![]).manifest["validation_gate"].clone(),
    ));
    assert!(matches!(
        kernel.prepare_validation_gate(&request, 19).unwrap(),
        ValidationGateAuthorization::Planned(_)
    ));
    drop(kernel);
    let connection = Connection::open(directory.path().join("master.sqlite3")).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM feature_validation_attempts",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM feature_conveyor_audit
                 WHERE event_kind='feature_validation_started'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
}

#[test]
fn validation_gate_failure_is_durable_idempotent_and_does_not_advance() {
    let gate = serde_json::json!({
        "schema_version": 1,
        "command_ids": [
            "requirements_binding", "coverage", "focused_unit_tests", "native_e2e",
            "documentation", "knowledge_base", "formatting", "lint", "build", "safety",
            "changed_paths", "secret_scan", "repository_validation"
        ]
    });
    let (_directory, mut kernel, request, mut evidence) = integrated_validation_fixture(Some(gate));
    let plan = match kernel.plan_validation_gate(&request, 20).unwrap() {
        ValidationGateAuthorization::Planned(plan) => plan,
        other => panic!("unexpected validation authorization: {other:?}"),
    };
    evidence.commands[3].passed = false;
    assert!(matches!(
        kernel.finalize_validation_gate(&plan, &evidence, 21),
        Err(MasterError::ValidationGateFailed)
    ));
    let status = kernel.feature_conveyor_status().unwrap();
    assert_eq!(
        status.features[0].status,
        FeatureLifecycleStatus::Validating
    );
    assert!(matches!(
        kernel.plan_validation_gate(&request, 22).unwrap(),
        ValidationGateAuthorization::ExistingFailed
    ));
    let mut drift = request;
    drift.candidate_tree = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string();
    assert!(matches!(
        kernel.plan_validation_gate(&drift, 22),
        Err(MasterError::ValidationGateUnavailable)
    ));
}

#[test]
fn validation_gate_rejects_every_stale_candidate_and_authority_binding() {
    let gate = serde_json::json!({
        "schema_version": 1,
        "command_ids": [
            "requirements_binding", "coverage", "focused_unit_tests", "native_e2e",
            "documentation", "knowledge_base", "formatting", "lint", "build", "safety",
            "changed_paths", "secret_scan", "repository_validation"
        ]
    });
    let (_directory, mut kernel, request, _) = integrated_validation_fixture(Some(gate));
    macro_rules! rejected {
        ($mutation:expr) => {{
            let mut drift = request.clone();
            $mutation(&mut drift);
            assert!(kernel.plan_validation_gate(&drift, 20).is_err());
        }};
    }
    rejected!(|r: &mut FeatureConveyorValidationGateRequest| r.specification_revision += 1);
    rejected!(|r: &mut FeatureConveyorValidationGateRequest| r.expected_lifecycle_revision += 1);
    rejected!(|r: &mut FeatureConveyorValidationGateRequest| r.feature_lease_id = Uuid::new_v4());
    rejected!(|r: &mut FeatureConveyorValidationGateRequest| r.snapshot_id = Uuid::new_v4());
    rejected!(|r: &mut FeatureConveyorValidationGateRequest| r.snapshot_sha256[0] ^= 1);
    rejected!(|r: &mut FeatureConveyorValidationGateRequest| r.integration_id = Uuid::new_v4());
    rejected!(|r: &mut FeatureConveyorValidationGateRequest| r.artifact_set_sha256[0] ^= 1);
    rejected!(
        |r: &mut FeatureConveyorValidationGateRequest| r.candidate_commit =
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()
    );
    rejected!(
        |r: &mut FeatureConveyorValidationGateRequest| r.candidate_tree =
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string()
    );
    rejected!(
        |r: &mut FeatureConveyorValidationGateRequest| r.base_commit =
            "cccccccccccccccccccccccccccccccccccccccc".to_string()
    );
    rejected!(|r: &mut FeatureConveyorValidationGateRequest| r.expected_queue_revision += 1);
    rejected!(|r: &mut FeatureConveyorValidationGateRequest| r
        .expected_emergency_pause_revision +=
        1);
    rejected!(|r: &mut FeatureConveyorValidationGateRequest| r.grants.registration += 1);
    rejected!(|r: &mut FeatureConveyorValidationGateRequest| r.grants.cloud_disclosure += 1);
    rejected!(|r: &mut FeatureConveyorValidationGateRequest| r.grants.autonomous_publication += 1);

    assert!(matches!(
        kernel.plan_validation_gate(&request, 20).unwrap(),
        ValidationGateAuthorization::Planned(_)
    ));
    assert!(kernel
        .validation_gate_execution_is_current(&request, 20)
        .unwrap());
    kernel.set_emergency_paused_at(true, 21).unwrap();
    assert!(!kernel
        .validation_gate_execution_is_current(&request, 21)
        .unwrap());
}

#[test]
fn validation_gate_requires_strict_immutable_manifest_plan() {
    let (_directory, mut absent_kernel, absent_request, _) = integrated_validation_fixture(None);
    assert!(matches!(
        absent_kernel.plan_validation_gate(&absent_request, 20),
        Err(MasterError::ValidationGateUnavailable)
    ));
}

fn reviewing_fixture() -> (
    tempfile::TempDir,
    MasterKernel,
    FeatureConveyorReviewGatewayRequest,
    FeatureConveyorReviewPacket,
) {
    let gate = serde_json::json!({
        "schema_version": 1,
        "command_ids": [
            "requirements_binding", "coverage", "focused_unit_tests", "native_e2e",
            "documentation", "knowledge_base", "formatting", "lint", "build", "safety",
            "changed_paths", "secret_scan", "repository_validation"
        ]
    });
    let (directory, mut kernel, validation_request, evidence) =
        integrated_validation_fixture(Some(gate));
    let validation_plan = match kernel
        .plan_validation_gate(&validation_request, 20)
        .unwrap()
    {
        ValidationGateAuthorization::Planned(plan) => plan,
        other => panic!("unexpected validation authorization: {other:?}"),
    };
    let validation = kernel
        .finalize_validation_gate(&validation_plan, &evidence, 21)
        .unwrap();
    let store = ArtifactIntegrationStore::open(directory.path()).unwrap();
    let candidate = kernel
        .candidate_references()
        .unwrap()
        .into_iter()
        .find(|candidate| candidate.integration_id == validation.integration_id)
        .unwrap();
    let candidate_diff = store.review_diff(&candidate).unwrap();
    assert!(candidate_diff.contains("-before\n"));
    assert!(candidate_diff.contains("+assemblywright contained coding fixture\n"));
    let mut request = FeatureConveyorReviewGatewayRequest {
        schema_version: FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION,
        review_call_id: Uuid::new_v4(),
        feature_id: validation.feature_id,
        specification_revision: validation.specification_revision,
        expected_lifecycle_revision: validation.lifecycle_revision,
        feature_lease_id: validation.feature_lease_id,
        integration_id: validation.integration_id,
        validation_id: validation.validation_id,
        candidate_commit: validation.candidate_commit.clone(),
        candidate_tree: validation.candidate_tree.clone(),
        base_commit: validation_request.base_commit.clone(),
        candidate_diff_sha256: Sha256::digest(candidate_diff.as_bytes()).into(),
        evidence_manifest_sha256: validation.evidence_manifest_sha256,
        review_packet_sha256: [1; 32],
        provider_id: "local.review".to_string(),
        model_id: "review-v1".to_string(),
        expected_queue_revision: validation.queue_revision,
        expected_emergency_pause_revision: validation.emergency_pause_revision,
        grants: validation.grants,
    };
    let plan = match kernel.prepare_review_gateway(&request, 22).unwrap() {
        ReviewGatewayAuthorization::Planned(plan) => plan,
        other => panic!("unexpected review authorization: {other:?}"),
    };
    let packet = FeatureConveyorReviewPacket {
        schema_version: FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION,
        feature_id: request.feature_id,
        specification_revision: request.specification_revision,
        approved_specification: plan.approved_specification,
        approved_specification_sha256: plan.approved_specification_sha256,
        candidate_commit: request.candidate_commit.clone(),
        candidate_tree: request.candidate_tree.clone(),
        base_commit: request.base_commit.clone(),
        candidate_diff,
        candidate_diff_sha256: request.candidate_diff_sha256,
        evidence_manifest_sha256: request.evidence_manifest_sha256,
        evidence_digests: plan.evidence_digests,
        requirements_sha256: plan.requirements_sha256,
        requirement_ids: plan.requirement_ids,
        provider_id: request.provider_id.clone(),
        model_id: request.model_id.clone(),
        grants: request.grants,
    };
    request.review_packet_sha256 = packet.sha256().unwrap();
    (directory, kernel, request, packet)
}

fn review_output(
    packet: &FeatureConveyorReviewPacket,
    decision: FeatureConveyorReviewDecision,
) -> FeatureConveyorReviewProviderOutput {
    let rejected = decision == FeatureConveyorReviewDecision::Rejected;
    FeatureConveyorReviewProviderOutput {
        schema_version: FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION,
        review_packet_sha256: packet.sha256().unwrap(),
        provider_id: packet.provider_id.clone(),
        model_id: packet.model_id.clone(),
        decision,
        blocking_findings: if rejected {
            vec![FeatureConveyorReviewFinding {
                finding_id: "blocking-review-finding".to_string(),
                requirement_id: packet.requirement_ids[0].clone(),
                evidence_sha256: packet.evidence_digests[0],
            }]
        } else {
            vec![]
        },
        non_blocking_findings: vec![],
        requirement_coverage: vec![FeatureConveyorReviewRequirementCoverage {
            requirement_id: packet.requirement_ids[0].clone(),
            status: if rejected {
                FeatureConveyorReviewCoverageStatus::Uncovered
            } else {
                FeatureConveyorReviewCoverageStatus::Covered
            },
            evidence_sha256: packet.evidence_digests[0],
        }],
        evidence_digests: packet.evidence_digests.clone(),
        knowledge_base_determination: FeatureConveyorKnowledgeBaseDetermination::NoNewKnowledge,
        knowledge_base_evidence_sha256: packet.evidence_digests[0],
    }
}

fn publishing_fixture() -> (
    tempfile::TempDir,
    MasterKernel,
    FeatureConveyorPublicationRequest,
) {
    let (directory, mut kernel, review_request, packet) = reviewing_fixture();
    let plan = match kernel
        .begin_review_gateway(&review_request, &packet, 23)
        .unwrap()
    {
        ReviewGatewayAuthorization::Planned(plan) => *plan,
        other => panic!("unexpected review authorization: {other:?}"),
    };
    let receipt = kernel
        .finalize_review_decision(
            &plan,
            &packet,
            &review_output(&packet, FeatureConveyorReviewDecision::Approved),
            24,
        )
        .unwrap();
    let repository_id = {
        let connection = Connection::open(directory.path().join("master.sqlite3")).unwrap();
        let value: String = connection
            .query_row(
                "SELECT repository_id FROM feature_specification_revisions WHERE feature_id=?1 AND revision=1",
                [receipt.feature_id.to_string()],
                |row| row.get(0),
            )
            .unwrap();
        Uuid::parse_str(&value).unwrap()
    };
    let publication = FeatureConveyorPublicationRequest {
        schema_version: FEATURE_CONVEYOR_PUBLICATION_COORDINATOR_SCHEMA_VERSION,
        publication_id: Uuid::new_v4(),
        feature_id: receipt.feature_id,
        specification_revision: receipt.specification_revision,
        expected_lifecycle_revision: receipt.lifecycle_revision,
        feature_lease_id: receipt.feature_lease_id,
        integration_id: receipt.integration_id,
        validation_id: receipt.validation_id,
        review_call_id: receipt.review_call_id,
        candidate_commit: receipt.candidate_commit,
        candidate_tree: review_request.candidate_tree,
        candidate_diff_sha256: receipt.candidate_diff_sha256,
        evidence_manifest_sha256: receipt.evidence_manifest_sha256,
        review_decision_sha256: receipt.decision_sha256,
        provider_id: receipt.provider_id,
        model_id: receipt.model_id,
        remote_base_commit: review_request.base_commit,
        branch_policy_sha256: publication_branch_policy_sha256(
            repository_id,
            receipt.feature_id,
            "main",
            &["release-local".to_string()],
            "merge",
            "release-local",
        )
        .unwrap(),
        expected_queue_revision: receipt.queue_revision,
        expected_emergency_pause_revision: receipt.emergency_pause_revision,
        grants: receipt.grants,
    };
    (directory, kernel, publication)
}

fn publication_evidence(
    action: PublicationActionKind,
    plan: &assemblywright_master::PublicationExecutionPlan,
    merge: &str,
) -> PublicationActionEvidence {
    let checks = !matches!(
        action,
        PublicationActionKind::PushBranch | PublicationActionKind::UpsertPullRequest
    );
    let merged = matches!(
        action,
        PublicationActionKind::MergePullRequest
            | PublicationActionKind::ReconcileRemoteMain
            | PublicationActionKind::RunPostMergeGate
    );
    let pull_request_number =
        match action {
            PublicationActionKind::PushBranch | PublicationActionKind::VerifyPullRequestHead => {
                (action == PublicationActionKind::VerifyPullRequestHead).then_some(41)
            }
            PublicationActionKind::UpsertPullRequest
            | PublicationActionKind::ObserveRequiredChecks => Some(41),
            PublicationActionKind::MergePullRequest => Some(41),
            PublicationActionKind::ReconcileRemoteMain
            | PublicationActionKind::RunPostMergeGate => None,
        };
    PublicationActionEvidence {
        schema_version: FEATURE_CONVEYOR_PUBLICATION_COORDINATOR_SCHEMA_VERSION,
        publication_id: plan.request.publication_id,
        action,
        remote_base_commit: plan.request.remote_base_commit.clone(),
        candidate_commit: plan.request.candidate_commit.clone(),
        feature_branch: plan.feature_branch.clone(),
        base_branch: plan.base_branch.clone(),
        pull_request_number,
        observed_head_commit: if matches!(
            action,
            PublicationActionKind::ReconcileRemoteMain | PublicationActionKind::RunPostMergeGate
        ) {
            merge.to_string()
        } else {
            plan.request.candidate_commit.clone()
        },
        required_checks_sha256: checks.then(|| {
            assemblywright_protocol::feature_conveyor_publication_required_checks_sha256(
                &plan.required_checks,
            )
            .unwrap()
        }),
        required_check_count: if checks {
            u16::try_from(plan.required_checks.len()).unwrap()
        } else {
            0
        },
        required_checks_passed: checks,
        branch_protection_enforced: true,
        bypass_used: false,
        merge_strategy: merged.then(|| plan.merge_strategy.clone()),
        resulting_main_commit: merged.then(|| merge.to_string()),
        post_merge_gate_id: (action == PublicationActionKind::RunPostMergeGate)
            .then(|| plan.post_merge_gate.clone()),
        post_merge_gate_passed: action == PublicationActionKind::RunPostMergeGate,
        evidence_sha256: [0; 32],
    }
    .seal()
    .unwrap()
}

#[test]
fn publication_coordinator_is_exact_idempotent_and_advances_only_after_healthy_main() {
    let (_directory, mut kernel, request) = publishing_fixture();
    let plan = match kernel.prepare_publication(&request, 25).unwrap() {
        PublicationAuthorization::Planned(plan) => *plan,
        other => panic!("unexpected publication authorization: {other:?}"),
    };
    let mut drift = request.clone();
    drift.branch_policy_sha256 = digest("drifted-policy");
    assert!(matches!(
        kernel.prepare_publication(&drift, 25),
        Err(MasterError::PublicationCoordinatorUnavailable)
    ));
    kernel.begin_publication(&plan, 26).unwrap();
    assert!(matches!(
        kernel.prepare_publication(&request, 27),
        Err(MasterError::PublicationEffectAmbiguous)
    ));

    let merge = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let mut final_receipt = None;
    for (index, action) in PublicationActionKind::ORDERED.iter().enumerate() {
        assert!(kernel
            .publication_execution_is_current(&request, *action, 27 + index as u64)
            .unwrap());
        let result = kernel
            .complete_publication_action(
                &plan,
                &publication_evidence(*action, &plan, merge),
                27 + index as u64,
            )
            .unwrap();
        if *action != PublicationActionKind::RunPostMergeGate {
            assert!(result.is_none());
        } else {
            final_receipt = result;
        }
    }
    let receipt = final_receipt.unwrap();
    receipt.validate().unwrap();
    assert_eq!(receipt.merge_commit, merge);
    assert_eq!(receipt.remote_main_commit, merge);
    assert_eq!(
        kernel.feature_snapshot(request.feature_id).unwrap().status,
        FeatureLifecycleStatus::Succeeded
    );
    assert!(matches!(
        kernel.prepare_publication(&request, 40).unwrap(),
        PublicationAuthorization::Existing(existing) if *existing == receipt
    ));
}

#[test]
fn publication_pause_cancellation_and_ambiguous_remote_evidence_fail_closed() {
    let (_directory, mut kernel, request) = publishing_fixture();
    let plan = match kernel.prepare_publication(&request, 25).unwrap() {
        PublicationAuthorization::Planned(plan) => *plan,
        _ => panic!(),
    };
    kernel.begin_publication(&plan, 26).unwrap();
    let mut malformed = publication_evidence(
        PublicationActionKind::PushBranch,
        &plan,
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    );
    malformed.observed_head_commit = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string();
    assert!(kernel
        .complete_publication_action(&plan, &malformed, 27)
        .is_err());
    kernel.set_emergency_paused_at(true, 28).unwrap();
    assert!(!kernel
        .publication_execution_is_current(&request, PublicationActionKind::PushBranch, 28)
        .unwrap());
    kernel
        .quarantine_ambiguous_publication(&plan, PublicationActionKind::PushBranch, 29)
        .unwrap();
    let snapshot = kernel.feature_snapshot(request.feature_id).unwrap();
    assert_eq!(snapshot.status, FeatureLifecycleStatus::Quarantined);
    assert!(snapshot.effect_possible);
    assert!(snapshot.active_lease_id.is_some());
}

#[test]
fn publication_stage_evidence_rejects_remote_pr_checks_and_strategy_drift() {
    let (_directory, mut kernel, request) = publishing_fixture();
    let plan = match kernel.prepare_publication(&request, 25).unwrap() {
        PublicationAuthorization::Planned(plan) => *plan,
        _ => panic!(),
    };
    kernel.begin_publication(&plan, 26).unwrap();
    let merge = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    let mut remote_drift = publication_evidence(PublicationActionKind::PushBranch, &plan, merge);
    remote_drift.remote_base_commit = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string();
    remote_drift.evidence_sha256 = [0; 32];
    let remote_drift = remote_drift.seal().unwrap();
    assert!(matches!(
        kernel.complete_publication_action(&plan, &remote_drift, 27),
        Err(MasterError::PublicationCoordinatorUnavailable)
    ));
    kernel
        .complete_publication_action(
            &plan,
            &publication_evidence(PublicationActionKind::PushBranch, &plan, merge),
            28,
        )
        .unwrap();
    kernel
        .complete_publication_action(
            &plan,
            &publication_evidence(PublicationActionKind::UpsertPullRequest, &plan, merge),
            29,
        )
        .unwrap();

    let mut pr_drift =
        publication_evidence(PublicationActionKind::ObserveRequiredChecks, &plan, merge);
    pr_drift.pull_request_number = Some(99);
    pr_drift.evidence_sha256 = [0; 32];
    let pr_drift = pr_drift.seal().unwrap();
    assert!(matches!(
        kernel.complete_publication_action(&plan, &pr_drift, 30),
        Err(MasterError::PublicationCoordinatorUnavailable)
    ));
    let mut checks_drift =
        publication_evidence(PublicationActionKind::ObserveRequiredChecks, &plan, merge);
    checks_drift.required_checks_sha256 = Some(digest("wrong-required-checks"));
    checks_drift.evidence_sha256 = [0; 32];
    let checks_drift = checks_drift.seal().unwrap();
    assert!(matches!(
        kernel.complete_publication_action(&plan, &checks_drift, 30),
        Err(MasterError::PublicationCoordinatorUnavailable)
    ));
    kernel
        .complete_publication_action(
            &plan,
            &publication_evidence(PublicationActionKind::ObserveRequiredChecks, &plan, merge),
            31,
        )
        .unwrap();
    kernel
        .complete_publication_action(
            &plan,
            &publication_evidence(PublicationActionKind::VerifyPullRequestHead, &plan, merge),
            32,
        )
        .unwrap();
    let mut strategy_drift =
        publication_evidence(PublicationActionKind::MergePullRequest, &plan, merge);
    strategy_drift.merge_strategy = Some("squash".to_string());
    strategy_drift.evidence_sha256 = [0; 32];
    let strategy_drift = strategy_drift.seal().unwrap();
    assert!(matches!(
        kernel.complete_publication_action(&plan, &strategy_drift, 33),
        Err(MasterError::PublicationCoordinatorUnavailable)
    ));
}

#[test]
fn publication_restart_with_unresolved_external_intent_quarantines_without_retry() {
    let (directory, mut kernel, request) = publishing_fixture();
    let plan = match kernel.prepare_publication(&request, 25).unwrap() {
        PublicationAuthorization::Planned(plan) => *plan,
        _ => panic!(),
    };
    kernel.begin_publication(&plan, 26).unwrap();
    drop(kernel);

    let restarted = MasterKernel::open(directory.path().join("master.sqlite3")).unwrap();
    assert_eq!(restarted.feature_startup_quarantines(), 1);
    let snapshot = restarted.feature_snapshot(request.feature_id).unwrap();
    assert_eq!(snapshot.status, FeatureLifecycleStatus::Quarantined);
    assert!(snapshot.effect_possible);
    assert!(snapshot.active_lease_id.is_some());
}

#[test]
fn publication_ambiguous_merge_intent_blocks_unhealthy_abandonment_from_publishing_origin() {
    let (_directory, mut kernel, request) = publishing_fixture();
    let plan = match kernel.prepare_publication(&request, 25).unwrap() {
        PublicationAuthorization::Planned(plan) => *plan,
        _ => panic!(),
    };
    kernel.begin_publication(&plan, 26).unwrap();
    let merge = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    for (offset, action) in PublicationActionKind::ORDERED[..4].iter().enumerate() {
        assert!(kernel
            .complete_publication_action(
                &plan,
                &publication_evidence(*action, &plan, merge),
                27 + offset as u64,
            )
            .unwrap()
            .is_none());
    }
    assert!(kernel
        .publication_execution_is_current(&request, PublicationActionKind::MergePullRequest, 31,)
        .unwrap());
    kernel
        .quarantine_ambiguous_publication(&plan, PublicationActionKind::MergePullRequest, 32)
        .unwrap();
    let quarantined = kernel.feature_snapshot(request.feature_id).unwrap();
    assert_eq!(quarantined.status, FeatureLifecycleStatus::Quarantined);
    assert!(matches!(
        kernel.abandon_and_advance(
            request.feature_id,
            quarantined.lifecycle_revision,
            request.expected_queue_revision,
            request.expected_emergency_pause_revision,
            FeatureAbandonmentEvidence {
                safe_reconciliation_sha256: digest("merge-ambiguity-safe"),
                merged: false,
                verified_healthy_main_sha256: None,
            },
            33,
        ),
        Err(MasterError::VerifiedHealthyMainRequired)
    ));
    let retained = kernel.feature_snapshot(request.feature_id).unwrap();
    assert_eq!(retained.status, FeatureLifecycleStatus::Quarantined);
    assert!(retained.active_lease_id.is_some());
}

struct MissingEvidenceAdapter;

impl assemblywright_master::publication::PublicationAdapter for MissingEvidenceAdapter {
    fn is_available(&self) -> bool {
        true
    }

    fn execute(
        &mut self,
        _plan: &assemblywright_master::PublicationExecutionPlan,
        _action: PublicationActionKind,
        _control: &assemblywright_master::publication::PublicationExecutionControl,
    ) -> Result<
        PublicationActionEvidence,
        assemblywright_master::publication::PublicationAdapterError,
    > {
        Err(assemblywright_master::publication::PublicationAdapterError::MissingEvidence)
    }
}

struct UnavailablePublicationAdapter;

impl assemblywright_master::publication::PublicationAdapter for UnavailablePublicationAdapter {
    fn is_available(&self) -> bool {
        false
    }

    fn execute(
        &mut self,
        _plan: &assemblywright_master::PublicationExecutionPlan,
        _action: PublicationActionKind,
        _control: &assemblywright_master::publication::PublicationExecutionControl,
    ) -> Result<
        PublicationActionEvidence,
        assemblywright_master::publication::PublicationAdapterError,
    > {
        Err(assemblywright_master::publication::PublicationAdapterError::Unavailable)
    }
}

fn publication_test_control() -> assemblywright_master::publication::PublicationExecutionControl {
    assemblywright_master::publication::PublicationExecutionControl::new(
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        std::time::Instant::now() + assemblywright_master::publication::PUBLICATION_ACTION_DEADLINE,
        std::sync::Arc::new(|| true),
    )
}

struct SlowPollingPublicationAdapter {
    started: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl assemblywright_master::publication::PublicationAdapter for SlowPollingPublicationAdapter {
    fn is_available(&self) -> bool {
        true
    }

    fn execute(
        &mut self,
        _plan: &assemblywright_master::PublicationExecutionPlan,
        _action: PublicationActionKind,
        control: &assemblywright_master::publication::PublicationExecutionControl,
    ) -> Result<
        PublicationActionEvidence,
        assemblywright_master::publication::PublicationAdapterError,
    > {
        self.started
            .store(true, std::sync::atomic::Ordering::Release);
        loop {
            control.poll()?;
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

#[test]
fn publication_missing_adapter_evidence_quarantines_after_durable_intent() {
    let (_directory, mut kernel, request) = publishing_fixture();
    assert!(matches!(
        assemblywright_master::publication::run_publication(
            &mut kernel,
            &request,
            &mut MissingEvidenceAdapter,
            publication_test_control(),
        ),
        Err(MasterError::PublicationEffectAmbiguous)
    ));
    let snapshot = kernel.feature_snapshot(request.feature_id).unwrap();
    assert_eq!(snapshot.status, FeatureLifecycleStatus::Quarantined);
    assert!(snapshot.effect_possible);
}

#[test]
fn publication_unavailable_adapter_creates_no_intent_or_effect_possible_state() {
    let (_directory, mut kernel, request) = publishing_fixture();
    assert!(matches!(
        assemblywright_master::publication::run_publication(
            &mut kernel,
            &request,
            &mut UnavailablePublicationAdapter,
            publication_test_control(),
        ),
        Err(MasterError::PublicationCoordinatorUnavailable)
    ));
    let snapshot = kernel.feature_snapshot(request.feature_id).unwrap();
    assert_eq!(snapshot.status, FeatureLifecycleStatus::Publishing);
    assert!(!snapshot.effect_possible);
    assert!(matches!(
        kernel.prepare_publication(&request, 25).unwrap(),
        PublicationAuthorization::Planned(_)
    ));
}

#[test]
fn publication_in_flight_cancellation_probe_suppresses_late_adapter_result() {
    let (_directory, mut kernel, request) = publishing_fixture();
    let cancelled = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let started = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let signal_cancelled = cancelled.clone();
    let signal_started = started.clone();
    let signal = std::thread::spawn(move || {
        while !signal_started.load(std::sync::atomic::Ordering::Acquire) {
            std::thread::yield_now();
        }
        signal_cancelled.store(true, std::sync::atomic::Ordering::Release);
    });
    let control = assemblywright_master::publication::PublicationExecutionControl::new(
        cancelled,
        std::time::Instant::now() + std::time::Duration::from_secs(2),
        std::sync::Arc::new(|| true),
    );
    assert!(matches!(
        assemblywright_master::publication::run_publication(
            &mut kernel,
            &request,
            &mut SlowPollingPublicationAdapter { started },
            control,
        ),
        Err(MasterError::PublicationEffectAmbiguous)
    ));
    signal.join().unwrap();
    let snapshot = kernel.feature_snapshot(request.feature_id).unwrap();
    assert_eq!(snapshot.status, FeatureLifecycleStatus::Quarantined);
    assert!(snapshot.effect_possible);
}

#[test]
fn publication_concurrent_emergency_pause_probe_dominates_in_flight_adapter() {
    let (directory, mut kernel, request) = publishing_fixture();
    let database = directory.path().join("master.sqlite3");
    let started = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let signal_started = started.clone();
    let signal_database = database.clone();
    let signal = std::thread::spawn(move || {
        while !signal_started.load(std::sync::atomic::Ordering::Acquire) {
            std::thread::yield_now();
        }
        Connection::open(signal_database)
            .unwrap()
            .execute_batch(
                "BEGIN IMMEDIATE;
                 UPDATE master_metadata SET integer_value=1 WHERE key='emergency_paused';
                 UPDATE master_metadata SET integer_value=integer_value+1
                   WHERE key='emergency_pause_revision';
                 COMMIT;",
            )
            .unwrap();
    });
    let authority_database = database.clone();
    let authority_current = std::sync::Arc::new(move || {
        Connection::open(&authority_database)
            .and_then(|connection| {
                connection.query_row(
                    "SELECT integer_value FROM master_metadata WHERE key='emergency_paused'",
                    [],
                    |row| row.get::<_, i64>(0),
                )
            })
            .is_ok_and(|paused| paused == 0)
    });
    let control = assemblywright_master::publication::PublicationExecutionControl::new(
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        std::time::Instant::now() + std::time::Duration::from_secs(2),
        authority_current,
    );
    assert!(matches!(
        assemblywright_master::publication::run_publication(
            &mut kernel,
            &request,
            &mut SlowPollingPublicationAdapter { started },
            control,
        ),
        Err(MasterError::PublicationEffectAmbiguous)
    ));
    signal.join().unwrap();
    let snapshot = kernel.feature_snapshot(request.feature_id).unwrap();
    assert_eq!(snapshot.status, FeatureLifecycleStatus::Quarantined);
    assert!(snapshot.effect_possible);
}

struct ControlledBareGitAdapter {
    candidate_path: std::path::PathBuf,
    remote_path: std::path::PathBuf,
}

impl assemblywright_master::publication::PublicationAdapter for ControlledBareGitAdapter {
    fn is_available(&self) -> bool {
        true
    }

    fn execute(
        &mut self,
        plan: &assemblywright_master::PublicationExecutionPlan,
        action: PublicationActionKind,
        control: &assemblywright_master::publication::PublicationExecutionControl,
    ) -> Result<
        PublicationActionEvidence,
        assemblywright_master::publication::PublicationAdapterError,
    > {
        control.poll()?;
        let candidate = &plan.request.candidate_commit;
        let run = |arguments: &[&str]| {
            Command::new("git")
                .args(arguments)
                .status()
                .ok()
                .is_some_and(|status| status.success())
        };
        let remote = self.remote_path.to_string_lossy().to_string();
        let candidate_path = self.candidate_path.to_string_lossy().to_string();
        match action {
            PublicationActionKind::PushBranch => {
                let refspec = format!("{candidate}:refs/heads/{}", plan.feature_branch);
                if !run(&["-C", &candidate_path, "push", &remote, &refspec]) {
                    return Err(
                        assemblywright_master::publication::PublicationAdapterError::AmbiguousEffect,
                    );
                }
            }
            PublicationActionKind::UpsertPullRequest
            | PublicationActionKind::ObserveRequiredChecks
            | PublicationActionKind::VerifyPullRequestHead => {}
            PublicationActionKind::MergePullRequest => {
                if !run(&[
                    "--git-dir",
                    &remote,
                    "update-ref",
                    "refs/heads/main",
                    candidate,
                    &plan.request.remote_base_commit,
                ]) {
                    return Err(
                        assemblywright_master::publication::PublicationAdapterError::AmbiguousEffect,
                    );
                }
            }
            PublicationActionKind::ReconcileRemoteMain
            | PublicationActionKind::RunPostMergeGate => {}
        }
        let observed = if action == PublicationActionKind::PushBranch
            || action == PublicationActionKind::UpsertPullRequest
            || action == PublicationActionKind::ObserveRequiredChecks
            || action == PublicationActionKind::VerifyPullRequestHead
            || action == PublicationActionKind::MergePullRequest
        {
            candidate.clone()
        } else {
            let output = Command::new("git")
                .args(["--git-dir", &remote, "rev-parse", "refs/heads/main"])
                .output()
                .map_err(|_| {
                    assemblywright_master::publication::PublicationAdapterError::MissingEvidence
                })?;
            String::from_utf8(output.stdout)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| output.status.success() && value.len() == 40)
                .ok_or(
                    assemblywright_master::publication::PublicationAdapterError::MissingEvidence,
                )?
        };
        control.poll()?;
        let mut evidence = publication_evidence(action, plan, candidate);
        evidence.observed_head_commit = observed;
        evidence.pull_request_number = matches!(
            action,
            PublicationActionKind::UpsertPullRequest
                | PublicationActionKind::ObserveRequiredChecks
                | PublicationActionKind::VerifyPullRequestHead
                | PublicationActionKind::MergePullRequest
        )
        .then_some(1);
        evidence.evidence_sha256 = [0; 32];
        evidence.seal().map_err(|_| {
            assemblywright_master::publication::PublicationAdapterError::MissingEvidence
        })
    }
}

#[test]
fn publication_controlled_bare_git_remote_native_e2e() {
    let (directory, mut kernel, request) = publishing_fixture();
    let remote_path = directory.path().join("controlled-remote.git");
    assert!(Command::new("git")
        .args(["init", "--bare", remote_path.to_str().unwrap()])
        .status()
        .unwrap()
        .success());
    assert!(Command::new("git")
        .args([
            "--git-dir",
            remote_path.to_str().unwrap(),
            "config",
            "receive.shallowUpdate",
            "true",
        ])
        .status()
        .unwrap()
        .success());
    let candidate_path = directory
        .path()
        .join("feature-conveyor-candidates")
        .join("candidates")
        .join(request.integration_id.to_string());
    let remote = remote_path.to_string_lossy().to_string();
    let base_refspec = format!("{}:refs/heads/main", request.remote_base_commit);
    assert!(Command::new("git")
        .args([
            "-C",
            candidate_path.to_str().unwrap(),
            "push",
            &remote,
            &base_refspec,
        ])
        .status()
        .unwrap()
        .success());
    let mut adapter = ControlledBareGitAdapter {
        candidate_path,
        remote_path,
    };
    let receipt = assemblywright_master::publication::run_publication(
        &mut kernel,
        &request,
        &mut adapter,
        publication_test_control(),
    )
    .unwrap();
    assert_eq!(receipt.merge_commit, request.candidate_commit);
    assert_eq!(receipt.remote_main_commit, request.candidate_commit);
    assert_eq!(
        kernel.feature_snapshot(request.feature_id).unwrap().status,
        FeatureLifecycleStatus::Succeeded
    );
}

#[test]
fn review_approval_is_exact_idempotent_and_only_transition_to_publishing() {
    let (_directory, mut kernel, request, packet) = reviewing_fixture();
    assert!(matches!(
        kernel.prepare_review_gateway(&request, 22).unwrap(),
        ReviewGatewayAuthorization::Planned(_)
    ));
    let plan = match kernel.begin_review_gateway(&request, &packet, 23).unwrap() {
        ReviewGatewayAuthorization::Planned(plan) => plan,
        other => panic!("unexpected review authorization: {other:?}"),
    };
    assert!(kernel
        .review_gateway_execution_is_current(&request, 23)
        .unwrap());
    let mut invented_coverage = review_output(&packet, FeatureConveyorReviewDecision::Approved);
    invented_coverage.requirement_coverage[0].requirement_id = "invented".to_string();
    assert!(matches!(
        kernel.finalize_review_decision(&plan, &packet, &invented_coverage, 24),
        Err(MasterError::ReviewGatewayUnavailable)
    ));
    let mut invented_finding = review_output(&packet, FeatureConveyorReviewDecision::Approved);
    invented_finding.non_blocking_findings = vec![FeatureConveyorReviewFinding {
        finding_id: "invented-requirement-finding".to_string(),
        requirement_id: "invented".to_string(),
        evidence_sha256: packet.evidence_digests[0],
    }];
    assert!(matches!(
        kernel.finalize_review_decision(&plan, &packet, &invented_finding, 24),
        Err(MasterError::ReviewGatewayUnavailable)
    ));
    let mut unbound_evidence = review_output(&packet, FeatureConveyorReviewDecision::Approved);
    unbound_evidence.requirement_coverage[0].evidence_sha256 = digest("invented-evidence");
    assert!(matches!(
        kernel.finalize_review_decision(&plan, &packet, &unbound_evidence, 24),
        Err(MasterError::ReviewGatewayUnavailable)
    ));
    let receipt = kernel
        .finalize_review_decision(
            &plan,
            &packet,
            &review_output(&packet, FeatureConveyorReviewDecision::Approved),
            24,
        )
        .unwrap();
    assert_eq!(
        receipt.lifecycle_revision,
        request.expected_lifecycle_revision + 1
    );
    assert_eq!(
        kernel.feature_conveyor_status().unwrap().features[0].status,
        FeatureLifecycleStatus::Publishing
    );
    assert!(!kernel
        .review_gateway_execution_is_current(&request, 24)
        .unwrap());
    assert!(matches!(
        kernel.prepare_review_gateway(&request, 25).unwrap(),
        ReviewGatewayAuthorization::ExistingDecision(existing) if *existing == receipt
    ));
}

#[test]
fn review_rejection_is_immutable_and_keeps_active_lease_for_later_repair() {
    let (_directory, mut kernel, request, packet) = reviewing_fixture();
    let plan = match kernel.begin_review_gateway(&request, &packet, 23).unwrap() {
        ReviewGatewayAuthorization::Planned(plan) => plan,
        other => panic!("unexpected review authorization: {other:?}"),
    };
    let receipt = kernel
        .finalize_review_decision(
            &plan,
            &packet,
            &review_output(&packet, FeatureConveyorReviewDecision::Rejected),
            24,
        )
        .unwrap();
    assert_eq!(
        receipt.lifecycle_revision,
        request.expected_lifecycle_revision
    );
    let status = kernel.feature_conveyor_status().unwrap();
    assert_eq!(status.features[0].status, FeatureLifecycleStatus::Reviewing);
    assert!(status.features[0].lease_present);
    let mut new_call = request.clone();
    new_call.review_call_id = Uuid::new_v4();
    assert!(matches!(
        kernel.prepare_review_gateway(&new_call, 25),
        Err(MasterError::ReviewGatewayUnavailable)
    ));
}

#[test]
fn review_transport_failures_backoff_without_repair_and_enforce_three_attempts() {
    let (_directory, mut kernel, mut request, packet) = reviewing_fixture();
    let first = match kernel.begin_review_gateway(&request, &packet, 23).unwrap() {
        ReviewGatewayAuthorization::Planned(plan) => plan,
        other => panic!("unexpected review authorization: {other:?}"),
    };
    let next = kernel
        .finalize_review_transport_failure(&first, ReviewTransportFailure::ProviderOutage, 24)
        .unwrap();
    assert_eq!(next, 60_024);
    request.review_call_id = Uuid::new_v4();
    assert!(matches!(
        kernel.prepare_review_gateway(&request, 60_023),
        Err(MasterError::ReviewRetryNotReady { .. })
    ));
    let second = match kernel
        .begin_review_gateway(&request, &packet, 60_024)
        .unwrap()
    {
        ReviewGatewayAuthorization::Planned(plan) => plan,
        other => panic!("unexpected review authorization: {other:?}"),
    };
    assert_eq!(second.candidate_attempt, 2);
    let next = kernel
        .finalize_review_transport_failure(&second, ReviewTransportFailure::MalformedOutput, 60_025)
        .unwrap();
    assert_eq!(next, 360_025);
    request.review_call_id = Uuid::new_v4();
    let third = match kernel
        .begin_review_gateway(&request, &packet, 360_025)
        .unwrap()
    {
        ReviewGatewayAuthorization::Planned(plan) => plan,
        other => panic!("unexpected review authorization: {other:?}"),
    };
    assert_eq!(third.candidate_attempt, 3);
    kernel
        .finalize_review_transport_failure(
            &third,
            ReviewTransportFailure::IncompleteTransport,
            360_026,
        )
        .unwrap();
    request.review_call_id = Uuid::new_v4();
    assert!(matches!(
        kernel.prepare_review_gateway(&request, 1_260_026),
        Err(MasterError::ReviewBudgetExhausted)
    ));
    assert_eq!(
        kernel.feature_conveyor_status().unwrap().features[0].status,
        FeatureLifecycleStatus::Reviewing
    );
}

#[test]
fn review_gateway_enforces_twelve_feature_calls_across_candidate_commits() {
    let (directory, mut kernel, mut request, _packet) = reviewing_fixture();
    let connection = Connection::open(directory.path().join("master.sqlite3")).unwrap();
    for feature_call in 1..=12_i64 {
        connection
            .execute(
                "INSERT INTO feature_review_calls (
                   review_call_id,feature_id,specification_revision,lifecycle_revision,
                   feature_lease_id,integration_id,validation_id,candidate_commit,candidate_tree,
                   base_commit,candidate_diff_sha256,evidence_manifest_sha256,review_packet_sha256,
                   provider_id,model_id,candidate_attempt,feature_call,queue_revision,
                   emergency_pause_revision,registration_grant_revision,
                   cloud_disclosure_grant_revision,publication_grant_revision,
                   request_binding_sha256,started_at_ms
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,1,
                           ?16,?17,?18,?19,?20,?21,?22,?23)",
                rusqlite::params![
                    Uuid::new_v4().to_string(),
                    request.feature_id.to_string(),
                    request.specification_revision as i64,
                    request.expected_lifecycle_revision as i64,
                    request.feature_lease_id.to_string(),
                    request.integration_id.to_string(),
                    request.validation_id.to_string(),
                    format!("{feature_call:040x}"),
                    request.candidate_tree,
                    request.base_commit,
                    request.candidate_diff_sha256.as_slice(),
                    request.evidence_manifest_sha256.as_slice(),
                    digest(&format!("packet-{feature_call}")).as_slice(),
                    request.provider_id,
                    request.model_id,
                    feature_call,
                    request.expected_queue_revision as i64,
                    request.expected_emergency_pause_revision as i64,
                    request.grants.registration as i64,
                    request.grants.cloud_disclosure as i64,
                    request.grants.autonomous_publication as i64,
                    digest(&format!("binding-{feature_call}")).as_slice(),
                    feature_call,
                ],
            )
            .unwrap();
    }
    request.review_call_id = Uuid::new_v4();
    assert!(matches!(
        kernel.prepare_review_gateway(&request, 100),
        Err(MasterError::ReviewBudgetExhausted)
    ));
}

#[test]
fn restart_quarantines_an_indeterminate_review_call_without_retry() {
    let (directory, mut kernel, request, packet) = reviewing_fixture();
    assert!(matches!(
        kernel.begin_review_gateway(&request, &packet, 23).unwrap(),
        ReviewGatewayAuthorization::Planned(_)
    ));
    drop(kernel);
    let restarted = MasterKernel::open(directory.path().join("master.sqlite3")).unwrap();
    assert_eq!(restarted.feature_startup_quarantines(), 1);
    let status = restarted.feature_conveyor_status().unwrap();
    assert_eq!(
        status.features[0].status,
        FeatureLifecycleStatus::Quarantined
    );
    assert!(status.features[0].effect_possible);
    assert!(!restarted
        .review_gateway_execution_is_current(&request, 24)
        .unwrap());
}

#[test]
fn review_decision_and_lifecycle_roll_back_when_redacted_audit_fails() {
    let (directory, mut kernel, request, packet) = reviewing_fixture();
    let plan = match kernel.begin_review_gateway(&request, &packet, 23).unwrap() {
        ReviewGatewayAuthorization::Planned(plan) => plan,
        other => panic!("unexpected review authorization: {other:?}"),
    };
    let database = directory.path().join("master.sqlite3");
    install_audit_failure(&database, "feature_review_approved");
    assert!(matches!(
        kernel.finalize_review_decision(
            &plan,
            &packet,
            &review_output(&packet, FeatureConveyorReviewDecision::Approved),
            24
        ),
        Err(MasterError::Storage(_))
    ));
    assert_eq!(
        kernel.feature_conveyor_status().unwrap().features[0].status,
        FeatureLifecycleStatus::Reviewing
    );
    let connection = Connection::open(&database).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM feature_review_decisions", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM feature_review_call_outcomes",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn review_binding_drift_and_pause_cancel_inflight_authority() {
    let (_directory, mut kernel, request, packet) = reviewing_fixture();
    macro_rules! rejected {
        ($mutation:expr) => {{
            let mut drift = request.clone();
            drift.review_call_id = Uuid::new_v4();
            $mutation(&mut drift);
            assert!(kernel.prepare_review_gateway(&drift, 22).is_err());
        }};
    }
    rejected!(
        |r: &mut FeatureConveyorReviewGatewayRequest| r.candidate_commit =
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string()
    );
    let mut diff_drift = request.clone();
    diff_drift.review_call_id = Uuid::new_v4();
    diff_drift.candidate_diff_sha256[0] ^= 1;
    assert!(kernel
        .begin_review_gateway(&diff_drift, &packet, 22)
        .is_err());
    rejected!(|r: &mut FeatureConveyorReviewGatewayRequest| r.evidence_manifest_sha256[0] ^= 1);
    rejected!(
        |r: &mut FeatureConveyorReviewGatewayRequest| r.provider_id = "other.review".to_string()
    );
    rejected!(|r: &mut FeatureConveyorReviewGatewayRequest| r.grants.cloud_disclosure += 1);
    rejected!(|r: &mut FeatureConveyorReviewGatewayRequest| r.expected_queue_revision += 1);

    let plan = match kernel.begin_review_gateway(&request, &packet, 23).unwrap() {
        ReviewGatewayAuthorization::Planned(plan) => plan,
        other => panic!("unexpected review authorization: {other:?}"),
    };
    kernel.set_emergency_paused_at(true, 24).unwrap();
    assert!(!kernel
        .review_gateway_execution_is_current(&request, 24)
        .unwrap());
    assert!(matches!(
        kernel.finalize_review_decision(
            &plan,
            &packet,
            &review_output(&packet, FeatureConveyorReviewDecision::Approved),
            25
        ),
        Err(MasterError::EmergencyPaused)
    ));
    kernel.finalize_interrupted_review_call(&plan, 26).unwrap();
    let status = kernel.feature_conveyor_status().unwrap();
    assert_eq!(
        status.features[0].status,
        FeatureLifecycleStatus::Quarantined
    );
    let connection = Connection::open(_directory.path().join("master.sqlite3")).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT outcome_kind FROM feature_review_call_outcomes WHERE review_call_id=?1",
                [request.review_call_id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .unwrap(),
        "interrupted"
    );
}

#[test]
fn review_revalidates_legacy_manifest_disclosure_before_provider_admission() {
    let (directory, mut kernel, mut request, _packet) = reviewing_fixture();
    let legacy_manifest = serde_json::to_vec(&json!({
        "acceptance": ["bounded-kernel-test"],
        "outcome": "legacy fixture",
        "transcript": "legacy raw conversation must never reach review"
    }))
    .unwrap();
    let connection = Connection::open(directory.path().join("master.sqlite3")).unwrap();
    connection
        .execute_batch("DROP TRIGGER feature_specification_revisions_no_update;")
        .unwrap();
    connection
        .execute(
            "UPDATE feature_specification_revisions
             SET canonical_manifest_json=?1,manifest_sha256=?2
             WHERE feature_id=?3 AND revision=?4",
            params![
                String::from_utf8(legacy_manifest.clone()).unwrap(),
                Sha256::digest(&legacy_manifest).to_vec(),
                request.feature_id.to_string(),
                i64::try_from(request.specification_revision).unwrap()
            ],
        )
        .unwrap();
    request.review_call_id = Uuid::new_v4();
    assert!(matches!(
        kernel.prepare_review_gateway(&request, 22),
        Err(MasterError::InvalidFeatureConveyorInput(_))
            | Err(MasterError::ReviewGatewayUnavailable)
    ));
}

#[test]
fn review_gateway_schema_v15_migrates_backup_first_to_immutable_v16_tables() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    drop(MasterKernel::open(&database).unwrap());
    let connection = Connection::open(&database).unwrap();
    drop_review_gateway_schema_for_legacy_fixture(&connection);
    connection.pragma_update(None, "user_version", 15).unwrap();
    let process = MasterProcess::acquire(directory.path()).unwrap();
    assert_eq!(process.kernel().schema_version().unwrap(), 17);
    let backup = process.migration_backup_path().unwrap();
    assert!(backup
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("master.pre-v17."));
    assert_eq!(
        Connection::open(backup)
            .unwrap()
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        15
    );
    let connection = Connection::open(&database).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='trigger'
                 AND name IN('feature_review_calls_no_update','feature_review_calls_no_delete',
                             'feature_review_call_outcomes_no_update',
                             'feature_review_decisions_no_update')",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        4
    );
}

#[test]
fn publication_schema_v16_migrates_backup_first_to_immutable_v17_tables() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    drop(MasterKernel::open(&database).unwrap());
    let connection = Connection::open(&database).unwrap();
    drop_publication_schema_for_legacy_fixture(&connection);
    connection.pragma_update(None, "user_version", 16).unwrap();
    drop(connection);

    let process = MasterProcess::acquire(directory.path()).unwrap();
    assert_eq!(process.kernel().schema_version().unwrap(), 17);
    let backup = process.migration_backup_path().unwrap();
    assert!(backup
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("master.pre-v17."));
    assert_eq!(
        Connection::open(backup)
            .unwrap()
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        16
    );
    let connection = Connection::open(&database).unwrap();
    for table in [
        "feature_publications",
        "feature_publication_action_intents",
        "feature_publication_action_outcomes",
        "feature_publication_completions",
    ] {
        let present: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert!(present, "missing migrated table {table}");
    }
}

#[test]
fn validation_gate_schema_v14_migrates_backup_first_to_immutable_v15_tables() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    drop(MasterKernel::open(&database).unwrap());
    let connection = Connection::open(&database).unwrap();
    drop_validation_gate_schema_for_legacy_fixture(&connection);
    connection.pragma_update(None, "user_version", 14).unwrap();
    let process = MasterProcess::acquire(directory.path()).unwrap();
    assert_eq!(
        process.kernel().schema_version().unwrap(),
        MASTER_SCHEMA_VERSION
    );
    let backup = process.migration_backup_path().unwrap();
    assert!(backup
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("master.pre-v17."));
    assert_eq!(
        Connection::open(backup)
            .unwrap()
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        14
    );
    let connection = Connection::open(&database).unwrap();
    let tables: HashSet<String> = connection
        .prepare("SELECT name FROM sqlite_master WHERE type='table'")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(tables.contains("feature_validation_attempts"));
    assert!(tables.contains("feature_validation_command_evidence"));
    assert!(tables.contains("feature_validation_completions"));
}

#[derive(Serialize)]
struct HistoricalV4Artifact<'a> {
    format: &'a str,
    path: &'a str,
    expected_before_sha256: [u8; 32],
    replacement_sha256: [u8; 32],
    replacement_hex: String,
}

fn historical_v4_artifact_bytes() -> Vec<u8> {
    serde_json::to_vec(&HistoricalV4Artifact {
        format: "assemblywright.readme-replacement.v1",
        path: "README.md",
        expected_before_sha256: [0x42; 32],
        replacement_sha256: Sha256::digest(LOCAL_CODING_FIXTURE_CONTENT).into(),
        replacement_hex: LOCAL_CODING_FIXTURE_CONTENT
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    })
    .unwrap()
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
        "acceptance": ["bounded-kernel-test"],
        "outcome": "bounded kernel test",
        "allowed_paths": ["crates/assemblywright-master/src/lib.rs"],
        "validation_gate": {
            "schema_version": 1,
            "command_ids": [
                "requirements_binding", "coverage", "focused_unit_tests", "native_e2e",
                "documentation", "knowledge_base", "formatting", "lint", "build", "safety",
                "changed_paths", "secret_scan", "repository_validation"
            ]
        },
        "publication_checks": ["release-local"],
        "base_branch": "main",
        "merge_strategy": "merge",
        "post_merge_gate": "release-local"
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

fn snapshot_plan(
    feature: &ApprovedFeatureSpecification,
    expected_queue_revision: u64,
    expected_emergency_pause_revision: u64,
) -> FeatureSnapshotClaimPlan {
    FeatureSnapshotClaimPlan {
        feature_id: feature.feature_id,
        specification_revision: feature.revision,
        repository_id: feature.repository_id,
        expected_queue_revision,
        expected_emergency_pause_revision,
        scope_sha256: digest("scope-0"),
        provider_id: feature.provider_id.clone(),
        model_id: feature.model_id.clone(),
        grants: feature.grants,
        base_commit: "1234567890abcdef1234567890abcdef12345678".to_string(),
    }
}

fn claim_feature(
    kernel: &mut MasterKernel,
    feature: &ApprovedFeatureSpecification,
    expected_queue_revision: u64,
    now_ms: u64,
) -> Result<assemblywright_master::FeatureClaim, MasterError> {
    let plan = snapshot_plan(
        feature,
        expected_queue_revision,
        kernel.emergency_pause_revision()?,
    );
    let plan = kernel.prepare_repository_snapshot_claim(&plan, now_ms)?;
    kernel.finalize_repository_snapshot_claim(
        &plan,
        &RepositorySnapshotEvidence {
            snapshot_id: Uuid::new_v4(),
            snapshot_sha256: digest(&format!("snapshot-{}", feature.feature_id)),
            base_commit: plan.base_commit.clone(),
        },
        now_ms,
    )
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

fn coding_registration(name: &str) -> DeviceRegistration {
    DeviceRegistration {
        device_id: DeviceId::new(Uuid::new_v4()),
        device_name: name.to_string(),
        role: DeviceRole::InferenceWorker,
        registry_revision: 1,
        capabilities: vec![CapabilityDescriptor::local_coding()],
    }
}

fn coding_dispatch_request(
    claim: &assemblywright_master::FeatureClaim,
    device: &DeviceRegistration,
    queue_revision: u64,
    pause_revision: u64,
) -> FeatureConveyorCodingDispatchRequest {
    let work_packet = FeatureConveyorCodingWorkPacketMetadata::fixture(Uuid::new_v4(), [0x42; 32]);
    FeatureConveyorCodingDispatchRequest {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        feature_id: claim.feature_id,
        specification_revision: claim.specification_revision,
        expected_lifecycle_revision: claim.lifecycle_revision,
        feature_lease_id: claim.lease_id,
        snapshot_id: claim.snapshot_id,
        snapshot_sha256: claim.snapshot_sha256,
        work_packet_sha256: work_packet.canonical_sha256().unwrap(),
        work_packet,
        device_id: device.device_id,
        device_registry_revision: device.registry_revision,
        expected_queue_revision: queue_revision,
        expected_emergency_pause_revision: pause_revision,
    }
}

fn coding_ack(job: &JobEnvelope, sequence: u64) -> JobResultEnvelope {
    let context = job.validate_local_coding().unwrap();
    let allowed_paths_sha256 = assemblywright_protocol::local_coding_fixture_allowed_paths_sha256();
    let artifact_bytes =
        assemblywright_protocol::build_local_coding_patch_artifact(&context.work_packet).unwrap();
    let artifact = LocalCodingResultArtifact::from_bytes(Uuid::new_v4(), &artifact_bytes).unwrap();
    let payload = serde_json::to_value(LocalCodingJobResult {
        status: LOCAL_CODING_COMPLETED_STATUS.to_string(),
        work_packet_sha256: context.work_packet_sha256,
        admission_sha256: local_coding_admission_sha256(job),
        snapshot_sha256: context.snapshot_sha256,
        allowed_paths_sha256,
        changed_paths_sha256: allowed_paths_sha256,
        patch_sha256: artifact.artifact_sha256,
        artifact_id: artifact.artifact_id,
        artifact_sha256: artifact.artifact_sha256,
        artifact_size_bytes: artifact.artifact_size_bytes,
        changed_file_count: 1,
        test_status: LOCAL_CODING_FIXTURE_TEST_STATUS.to_string(),
        mutation_performed: true,
        workspace_retained: true,
        workspace_expires_at_ms: 3_000_000,
        ambiguous: false,
    })
    .unwrap();
    JobResultEnvelope {
        protocol_version: PROTOCOL_VERSION,
        connection_epoch: job.connection_epoch,
        sequence,
        task_id: job.task_id,
        step_id: job.step_id,
        attempt_id: job.attempt_id,
        lease_id: job.lease_id,
        cancellation_id: job.cancellation_id,
        status: JobResultStatus::Completed,
        context_sha256: job.context_sha256,
        payload_sha256: Sha256::digest(serde_json::to_vec(&payload).unwrap()).into(),
        payload,
    }
}

fn coding_artifact_admission(
    job: &JobEnvelope,
    result: &JobResultEnvelope,
) -> LocalCodingResultArtifactAdmission {
    let context = job.validate_local_coding().unwrap();
    let payload: LocalCodingJobResult = serde_json::from_value(result.payload.clone()).unwrap();
    let artifact_bytes =
        assemblywright_protocol::build_local_coding_patch_artifact(&context.work_packet).unwrap();
    LocalCodingResultArtifactAdmission {
        protocol_version: job.protocol_version,
        connection_epoch: job.connection_epoch,
        sequence: result.sequence,
        task_id: job.task_id,
        step_id: job.step_id,
        attempt_id: job.attempt_id,
        lease_id: job.lease_id,
        cancellation_id: job.cancellation_id,
        context_sha256: job.context_sha256,
        feature_id: context.feature_id,
        feature_lease_id: context.feature_lease_id,
        snapshot_id: context.snapshot_id,
        snapshot_sha256: context.snapshot_sha256,
        work_packet_sha256: context.work_packet_sha256,
        workspace_retained: payload.workspace_retained,
        workspace_expires_at_ms: payload.workspace_expires_at_ms,
        artifact: LocalCodingResultArtifact::from_bytes(payload.artifact_id, &artifact_bytes)
            .unwrap(),
    }
}

fn persist_referenced_result_artifact(
    data_dir: &std::path::Path,
    create_file: bool,
) -> (LocalCodingResultArtifactAdmission, Vec<u8>) {
    let database = data_dir.join("master.sqlite3");
    let mut kernel = MasterKernel::open(&database).unwrap();
    let repository_id = Uuid::new_v4();
    install_grants(&mut kernel, repository_id);
    let feature = specification(Uuid::new_v4(), repository_id, vec![]);
    kernel.enqueue_approved_feature(&feature, 0, 10).unwrap();
    let claim = claim_feature(&mut kernel, &feature, 1, 11).unwrap();
    let device = coding_registration("persisted-artifact-worker");
    kernel.register_device(&device).unwrap();
    let request = coding_dispatch_request(
        &claim,
        &device,
        kernel.feature_queue_revision().unwrap(),
        kernel.emergency_pause_revision().unwrap(),
    );
    kernel.dispatch_feature_coding(&request, 12).unwrap();
    let epoch = kernel
        .accept_handshake(
            &HandshakeRequest {
                protocol_version: PROTOCOL_VERSION,
                device_id: device.device_id,
                device_name: device.device_name.clone(),
                role: device.role,
                registry_revision: device.registry_revision,
                capabilities: device.capabilities.clone(),
            },
            13,
        )
        .unwrap()
        .connection_epoch;
    let contract = RemoteWorkContract::from_registration(&device).unwrap();
    let job = kernel
        .lease_next_remote_step(device.device_id, epoch, 14, &contract)
        .unwrap();
    let result = coding_ack(&job, job.sequence + 1);
    let admission = coding_artifact_admission(&job, &result);
    kernel
        .finalize_local_coding_result_artifact(device.device_id, &admission, 15)
        .unwrap();
    drop(kernel);
    let bytes = admission.artifact.validate().unwrap();
    if create_file {
        let store = assemblywright_master::ResultArtifactStore::open(data_dir).unwrap();
        drop(
            store
                .prepare(
                    admission.artifact.artifact_id,
                    admission.artifact.artifact_sha256,
                    &bytes,
                )
                .unwrap(),
        );
    }
    (admission, bytes)
}

#[test]
fn owner_control_designation_is_explicit_cas_bound_role_checked_and_audited() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    let mut kernel = MasterKernel::open(&database).unwrap();
    assert_eq!(kernel.schema_version().unwrap(), MASTER_SCHEMA_VERSION);
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
    let claim = claim_feature(
        kernel,
        feature,
        kernel.feature_queue_revision().unwrap(),
        now + 1,
    )
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
    let claim = claim_feature(
        kernel,
        feature,
        kernel.feature_queue_revision().unwrap(),
        now + 1,
    )
    .unwrap();
    let cancelled = kernel
        .cancel_active_feature(
            feature.feature_id,
            claim.lifecycle_revision,
            kernel.feature_queue_revision().unwrap(),
            kernel.emergency_pause_revision().unwrap(),
            now + 2,
        )
        .unwrap();
    kernel
        .abandon_and_advance(
            feature.feature_id,
            cancelled.lifecycle_revision,
            kernel.feature_queue_revision().unwrap(),
            kernel.emergency_pause_revision().unwrap(),
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
    let claim = claim_feature(
        &mut kernel,
        &features[0],
        MAX_CONVEYOR_NONTERMINAL_FEATURES,
        200,
    )
    .unwrap();
    kernel
        .cancel_active_feature(
            claim.feature_id,
            claim.lifecycle_revision,
            kernel.feature_queue_revision().unwrap(),
            kernel.emergency_pause_revision().unwrap(),
            201,
        )
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
        let claim = claim_feature(process.kernel_mut(), &feature, 1, 20).unwrap();
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
            "feature_snapshot_claimed",
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
        kernel
            .prepare_repository_snapshot_claim(&snapshot_plan(&blocked, 3, 0), 14)
            .unwrap_err(),
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
    let claim = claim_feature(&mut kernel, &dependency, revision, 16).unwrap();
    assert_eq!(claim.feature_id, dependency.feature_id);
    assert!(matches!(
        kernel
            .prepare_repository_snapshot_claim(
                &snapshot_plan(&blocked, kernel.feature_queue_revision().unwrap(), 0),
                17,
            )
            .unwrap_err(),
        MasterError::FeatureLeaseAlreadyActive
    ));
}

#[test]
fn snapshot_claim_finalizer_rechecks_stale_queue_pause_grants_and_zero_evidence() {
    let mut kernel = MasterKernel::in_memory().unwrap();
    let repository_id = Uuid::new_v4();
    install_grants(&mut kernel, repository_id);
    let head = specification(Uuid::new_v4(), repository_id, vec![]);
    let tail = specification(Uuid::new_v4(), repository_id, vec![]);
    kernel.enqueue_approved_feature(&head, 0, 10).unwrap();
    let plan = snapshot_plan(&head, 1, 0);
    kernel.prepare_repository_snapshot_claim(&plan, 11).unwrap();
    kernel.enqueue_approved_feature(&tail, 1, 12).unwrap();
    let evidence = RepositorySnapshotEvidence {
        snapshot_id: Uuid::new_v4(),
        snapshot_sha256: digest("stale-queue-snapshot"),
        base_commit: plan.base_commit.clone(),
    };
    assert!(matches!(
        kernel
            .finalize_repository_snapshot_claim(&plan, &evidence, 13)
            .unwrap_err(),
        MasterError::StaleFeatureQueueRevision {
            expected: 1,
            found: 2
        }
    ));
    assert!(kernel.repository_snapshot_ids().unwrap().is_empty());
    assert!(kernel
        .feature_snapshot(head.feature_id)
        .unwrap()
        .active_lease_id
        .is_none());

    let plan = snapshot_plan(&head, 2, 0);
    kernel.prepare_repository_snapshot_claim(&plan, 14).unwrap();
    kernel.set_emergency_paused_at(true, 15).unwrap();
    assert!(matches!(
        kernel
            .finalize_repository_snapshot_claim(&plan, &evidence, 16)
            .unwrap_err(),
        MasterError::StaleEmergencyPauseRevision { .. }
    ));
    assert!(kernel.repository_snapshot_ids().unwrap().is_empty());
    kernel.set_emergency_paused_at(false, 17).unwrap();

    let plan = snapshot_plan(&head, 2, 2);
    assert!(matches!(
        kernel
            .finalize_repository_snapshot_claim(
                &plan,
                &RepositorySnapshotEvidence {
                    snapshot_id: Uuid::new_v4(),
                    snapshot_sha256: [0; 32],
                    base_commit: plan.base_commit.clone(),
                },
                18,
            )
            .unwrap_err(),
        MasterError::InvalidFeatureConveyorInput(_)
    ));
    kernel
        .record_repository_grant_revision(
            &RepositoryGrantRevision {
                repository_id,
                kind: RepositoryGrantKind::CloudDisclosure,
                revision: 2,
                scope_sha256: digest("new-cloud-scope"),
                owner_approval_sha256: digest("new-cloud-owner"),
                expires_at_ms: None,
                revoked: true,
            },
            1,
            2,
            19,
        )
        .unwrap();
    assert!(matches!(
        kernel
            .prepare_repository_snapshot_claim(&plan, 20)
            .unwrap_err(),
        MasterError::RepositoryGrantUnavailable
    ));
    assert!(kernel.repository_snapshot_ids().unwrap().is_empty());
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
    let claim = claim_feature(&mut kernel, &feature, 1, 11).unwrap();

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
    let claim = claim_feature(&mut kernel, &feature, 1, 11).unwrap();
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
fn coding_dispatch_is_atomic_snapshot_device_and_revision_bound_then_cancellation_dominates() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    let mut kernel = MasterKernel::open(&database).unwrap();
    let repository_id = Uuid::new_v4();
    install_grants(&mut kernel, repository_id);
    let feature = specification(Uuid::new_v4(), repository_id, vec![]);
    kernel.enqueue_approved_feature(&feature, 0, 10).unwrap();
    let claim = claim_feature(&mut kernel, &feature, 1, 11).unwrap();
    let device = coding_registration("coding-worker");
    kernel.register_device(&device).unwrap();
    let request = coding_dispatch_request(
        &claim,
        &device,
        kernel.feature_queue_revision().unwrap(),
        kernel.emergency_pause_revision().unwrap(),
    );

    let mut stale = request.clone();
    stale.snapshot_sha256 = digest("wrong-snapshot");
    assert!(matches!(
        kernel.dispatch_feature_coding(&stale, 12),
        Err(MasterError::FeatureCodingDispatchUnavailable)
    ));
    stale = request.clone();
    stale.device_registry_revision += 1;
    assert!(matches!(
        kernel.dispatch_feature_coding(&stale, 12),
        Err(MasterError::FeatureCodingDispatchUnavailable)
    ));
    stale = request.clone();
    stale.expected_queue_revision += 1;
    assert!(matches!(
        kernel.dispatch_feature_coding(&stale, 12),
        Err(MasterError::StaleFeatureQueueRevision { .. })
    ));

    install_audit_failure(&database, "feature_coding_dispatched");
    assert!(matches!(
        kernel.dispatch_feature_coding(&request, 12),
        Err(MasterError::Storage(_))
    ));
    let connection = Connection::open(&database).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM feature_coding_dispatches",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM master_steps WHERE capability_id = 'local.coding.v1'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
    drop(connection);
    remove_audit_failure(&database);

    let receipt = kernel.dispatch_feature_coding(&request, 13).unwrap();
    assert_eq!(
        kernel.step_snapshot(receipt.step_id).unwrap().status,
        assemblywright_master::StepStatus::Queued
    );
    let transition_evidence = FeatureTransitionEvidence {
        repository_snapshot_sha256: claim.snapshot_sha256,
        accepted_evidence_sha256: digest("coding-complete-evidence"),
    };
    assert!(matches!(
        kernel.advance_feature_lifecycle(
            feature.feature_id,
            claim.lifecycle_revision,
            FeatureLifecycleStatus::Validating,
            transition_evidence,
            13,
        ),
        Err(MasterError::FeatureCodingWorkOutstanding)
    ));
    let audit: String = Connection::open(&database)
        .unwrap()
        .query_row(
            "SELECT redacted_metadata_json FROM feature_conveyor_audit
             WHERE event_kind = 'feature_coding_dispatched'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!audit.contains(&request.device_id.0.to_string()));
    assert!(!audit.contains(&request.snapshot_id.to_string()));
    assert!(!audit.contains("path"));
    assert!(audit.contains("\"repository_material_present\":false"));

    let handshake = HandshakeRequest {
        protocol_version: PROTOCOL_VERSION,
        device_id: device.device_id,
        device_name: device.device_name.clone(),
        role: device.role,
        registry_revision: device.registry_revision,
        capabilities: device.capabilities.clone(),
    };
    let epoch = kernel
        .accept_handshake(&handshake, 14)
        .unwrap()
        .connection_epoch;
    let contract = RemoteWorkContract::from_registration(&device).unwrap();
    let job = kernel
        .lease_next_remote_step(device.device_id, epoch, 15, &contract)
        .unwrap();
    assert_eq!(job.step_id, receipt.step_id);
    let coding_context = job.validate_local_coding().unwrap();
    let transfer = LocalCodingSnapshotChunkRequest {
        protocol_version: job.protocol_version,
        connection_epoch: job.connection_epoch,
        task_id: job.task_id,
        step_id: job.step_id,
        attempt_id: job.attempt_id,
        lease_id: job.lease_id,
        cancellation_id: job.cancellation_id,
        snapshot_id: coding_context.snapshot_id,
        snapshot_sha256: coding_context.snapshot_sha256,
        offset: 0,
    };
    kernel
        .authorize_local_coding_snapshot_chunk(device.device_id, &transfer, 15)
        .unwrap();
    let mut wrong_attempt = transfer.clone();
    wrong_attempt.attempt_id = assemblywright_protocol::AttemptId::new(Uuid::new_v4());
    assert!(kernel
        .authorize_local_coding_snapshot_chunk(device.device_id, &wrong_attempt, 15)
        .is_err());
    assert!(kernel
        .authorize_local_coding_snapshot_chunk(DeviceId::new(Uuid::new_v4()), &transfer, 15,)
        .is_err());
    assert!(matches!(
        kernel.advance_feature_lifecycle(
            feature.feature_id,
            claim.lifecycle_revision,
            FeatureLifecycleStatus::Validating,
            transition_evidence,
            15,
        ),
        Err(MasterError::FeatureCodingWorkOutstanding)
    ));

    let cancelled = kernel
        .cancel_active_feature(
            feature.feature_id,
            claim.lifecycle_revision,
            kernel.feature_queue_revision().unwrap(),
            kernel.emergency_pause_revision().unwrap(),
            16,
        )
        .unwrap();
    assert_eq!(cancelled.status, FeatureLifecycleStatus::Cancelled);
    assert_eq!(
        kernel.attempt_status(job.attempt_id).unwrap(),
        assemblywright_master::AttemptStatus::CancellationPending
    );
    assert!(matches!(
        kernel.authorize_local_coding_snapshot_chunk(device.device_id, &transfer, 16),
        Err(MasterError::FeatureCodingDispatchUnavailable)
    ));
}

#[test]
fn coding_dispatch_stays_ineligible_after_restart_quarantines_active_feature() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    let (device, receipt) = {
        let mut kernel = MasterKernel::open(&database).unwrap();
        let repository_id = Uuid::new_v4();
        install_grants(&mut kernel, repository_id);
        let feature = specification(Uuid::new_v4(), repository_id, vec![]);
        kernel.enqueue_approved_feature(&feature, 0, 10).unwrap();
        let claim = claim_feature(&mut kernel, &feature, 1, 11).unwrap();
        let device = coding_registration("restart-coding-worker");
        kernel.register_device(&device).unwrap();
        let request = coding_dispatch_request(
            &claim,
            &device,
            kernel.feature_queue_revision().unwrap(),
            kernel.emergency_pause_revision().unwrap(),
        );
        let receipt = kernel.dispatch_feature_coding(&request, 12).unwrap();
        (device, receipt)
    };
    let mut restarted = MasterKernel::open(&database).unwrap();
    assert_eq!(restarted.feature_startup_quarantines(), 1);
    let epoch = restarted
        .accept_handshake(
            &HandshakeRequest {
                protocol_version: PROTOCOL_VERSION,
                device_id: device.device_id,
                device_name: device.device_name.clone(),
                role: device.role,
                registry_revision: device.registry_revision,
                capabilities: device.capabilities.clone(),
            },
            20,
        )
        .unwrap()
        .connection_epoch;
    let contract = RemoteWorkContract::from_registration(&device).unwrap();
    assert!(matches!(
        restarted.lease_next_remote_step(device.device_id, epoch, 21, &contract),
        Err(MasterError::NoEligibleStep)
    ));
    assert_eq!(
        restarted.step_snapshot(receipt.step_id).unwrap().status,
        assemblywright_master::StepStatus::Queued
    );
}

#[test]
fn emergency_pause_cancels_coding_attempt_and_resume_rejects_pre_pause_acknowledgement() {
    let mut kernel = MasterKernel::in_memory().unwrap();
    let repository_id = Uuid::new_v4();
    install_grants(&mut kernel, repository_id);
    let feature = specification(Uuid::new_v4(), repository_id, vec![]);
    kernel.enqueue_approved_feature(&feature, 0, 10).unwrap();
    let claim = claim_feature(&mut kernel, &feature, 1, 11).unwrap();
    let device = coding_registration("pause-coding-worker");
    kernel.register_device(&device).unwrap();
    let request = coding_dispatch_request(
        &claim,
        &device,
        kernel.feature_queue_revision().unwrap(),
        kernel.emergency_pause_revision().unwrap(),
    );
    kernel.dispatch_feature_coding(&request, 12).unwrap();
    let epoch = kernel
        .accept_handshake(
            &HandshakeRequest {
                protocol_version: PROTOCOL_VERSION,
                device_id: device.device_id,
                device_name: device.device_name.clone(),
                role: device.role,
                registry_revision: device.registry_revision,
                capabilities: device.capabilities.clone(),
            },
            13,
        )
        .unwrap()
        .connection_epoch;
    let contract = RemoteWorkContract::from_registration(&device).unwrap();
    let job = kernel
        .lease_next_remote_step(device.device_id, epoch, 14, &contract)
        .unwrap();
    let pre_pause_ack = coding_ack(&job, job.sequence + 2);

    kernel.set_emergency_paused_at(true, 15).unwrap();
    assert_eq!(
        kernel.attempt_status(job.attempt_id).unwrap(),
        assemblywright_master::AttemptStatus::CancellationPending
    );
    kernel.set_emergency_paused_at(false, 16).unwrap();
    assert!(matches!(
        kernel.accept_remote_result_from(device.device_id, &pre_pause_ack, 17, &contract),
        Err(MasterError::FeatureCodingDispatchUnavailable)
    ));
    assert_eq!(
        kernel.attempt_status(job.attempt_id).unwrap(),
        assemblywright_master::AttemptStatus::CancellationPending
    );
}

#[test]
fn terminal_coding_ack_allows_validation_and_lifecycle_change_invalidates_replay() {
    let mut kernel = MasterKernel::in_memory().unwrap();
    let repository_id = Uuid::new_v4();
    install_grants(&mut kernel, repository_id);
    let feature = specification(Uuid::new_v4(), repository_id, vec![]);
    kernel.enqueue_approved_feature(&feature, 0, 10).unwrap();
    let claim = claim_feature(&mut kernel, &feature, 1, 11).unwrap();
    let device = coding_registration("terminal-coding-worker");
    kernel.register_device(&device).unwrap();
    let request = coding_dispatch_request(
        &claim,
        &device,
        kernel.feature_queue_revision().unwrap(),
        kernel.emergency_pause_revision().unwrap(),
    );
    kernel.dispatch_feature_coding(&request, 12).unwrap();
    let epoch = kernel
        .accept_handshake(
            &HandshakeRequest {
                protocol_version: PROTOCOL_VERSION,
                device_id: device.device_id,
                device_name: device.device_name.clone(),
                role: device.role,
                registry_revision: device.registry_revision,
                capabilities: device.capabilities.clone(),
            },
            13,
        )
        .unwrap()
        .connection_epoch;
    let contract = RemoteWorkContract::from_registration(&device).unwrap();
    let job = kernel
        .lease_next_remote_step(device.device_id, epoch, 14, &contract)
        .unwrap();
    let ack = coding_ack(&job, job.sequence + 1);
    let artifact = coding_artifact_admission(&job, &ack);
    kernel
        .authorize_local_coding_result_artifact(device.device_id, &artifact, 15)
        .unwrap();
    kernel
        .finalize_local_coding_result_artifact(device.device_id, &artifact, 15)
        .unwrap();
    let artifact_directory = tempdir().unwrap();
    let store =
        assemblywright_master::ResultArtifactStore::open(artifact_directory.path()).unwrap();
    let artifact_bytes = artifact.artifact.validate().unwrap();
    let mut prepared = store
        .prepare(
            artifact.artifact.artifact_id,
            artifact.artifact.artifact_sha256,
            &artifact_bytes,
        )
        .unwrap();
    kernel
        .accept_remote_result_from_with_artifact(
            device.device_id,
            &ack,
            15,
            &contract,
            &store,
            prepared.verified_mut(),
        )
        .unwrap();
    let validating = kernel
        .advance_feature_lifecycle(
            feature.feature_id,
            claim.lifecycle_revision,
            FeatureLifecycleStatus::Validating,
            FeatureTransitionEvidence {
                repository_snapshot_sha256: claim.snapshot_sha256,
                accepted_evidence_sha256: digest("terminal-coding-evidence"),
            },
            16,
        )
        .unwrap();
    assert_eq!(validating.status, FeatureLifecycleStatus::Validating);
    assert!(matches!(
        kernel.accept_remote_result_from_with_artifact(
            device.device_id,
            &ack,
            17,
            &contract,
            &store,
            prepared.verified_mut()
        ),
        Err(MasterError::FeatureCodingDispatchUnavailable)
    ));
}

#[test]
fn result_artifact_admission_is_exact_idempotent_and_required_before_result() {
    let kernel_directory = tempdir().unwrap();
    let database = kernel_directory.path().join("master.sqlite3");
    let mut kernel = MasterKernel::open(&database).unwrap();
    let repository_id = Uuid::new_v4();
    install_grants(&mut kernel, repository_id);
    let feature = specification(Uuid::new_v4(), repository_id, vec![]);
    kernel.enqueue_approved_feature(&feature, 0, 10).unwrap();
    let claim = claim_feature(&mut kernel, &feature, 1, 11).unwrap();
    let device = coding_registration("artifact-worker");
    kernel.register_device(&device).unwrap();
    let request = coding_dispatch_request(
        &claim,
        &device,
        kernel.feature_queue_revision().unwrap(),
        kernel.emergency_pause_revision().unwrap(),
    );
    kernel.dispatch_feature_coding(&request, 12).unwrap();
    let epoch = kernel
        .accept_handshake(
            &HandshakeRequest {
                protocol_version: PROTOCOL_VERSION,
                device_id: device.device_id,
                device_name: device.device_name.clone(),
                role: device.role,
                registry_revision: device.registry_revision,
                capabilities: device.capabilities.clone(),
            },
            13,
        )
        .unwrap()
        .connection_epoch;
    let contract = RemoteWorkContract::from_registration(&device).unwrap();
    let job = kernel
        .lease_next_remote_step(device.device_id, epoch, 14, &contract)
        .unwrap();
    let result = coding_ack(&job, job.sequence + 1);
    assert!(matches!(
        kernel.accept_remote_result_from(device.device_id, &result, 15, &contract),
        Err(MasterError::ResultArtifactUnavailable)
    ));

    let admission = coding_artifact_admission(&job, &result);
    let context = job.validate_local_coding().unwrap();
    let mut different_packet = context.work_packet.clone();
    different_packet.acceptance_criteria_count += 1;
    let different_bytes =
        assemblywright_protocol::build_local_coding_patch_artifact(&different_packet).unwrap();
    let mut different_artifact = admission.clone();
    different_artifact.artifact =
        LocalCodingResultArtifact::from_bytes(Uuid::new_v4(), &different_bytes).unwrap();
    assert!(kernel
        .authorize_local_coding_result_artifact(device.device_id, &different_artifact, 15)
        .is_err());
    assert!(!kernel
        .authorize_local_coding_result_artifact(device.device_id, &admission, 15)
        .unwrap());
    let first = kernel
        .finalize_local_coding_result_artifact(device.device_id, &admission, 15)
        .unwrap();
    let retry = kernel
        .finalize_local_coding_result_artifact(device.device_id, &admission, 16)
        .unwrap();
    assert_eq!(retry, first);
    assert!(kernel
        .authorize_local_coding_result_artifact(device.device_id, &admission, 16)
        .unwrap());
    assert_eq!(
        kernel.result_artifact_ids().unwrap(),
        HashSet::from([first.artifact_id])
    );
    let audit_connection = Connection::open(&database).unwrap();
    let audits: Vec<String> = audit_connection
        .prepare(
            "SELECT redacted_metadata_json FROM feature_conveyor_audit
             WHERE event_kind = 'result_artifact_admitted'",
        )
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(audits.len(), 1);
    let audit: serde_json::Value = serde_json::from_str(&audits[0]).unwrap();
    assert_eq!(
        audit
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<HashSet<_>>(),
        HashSet::from([
            "artifact_id".to_string(),
            "artifact_sha256".to_string(),
            "artifact_size_bytes".to_string(),
            "attempt_id".to_string(),
            "step_id".to_string(),
            "workspace_retained".to_string(),
            "workspace_expiry_present".to_string(),
        ])
    );
    assert!(!audits[0].contains("artifact_hex"));
    assert!(!audits[0].contains("README.md"));

    let mut mismatch = admission.clone();
    mismatch.artifact.artifact_id = Uuid::new_v4();
    assert!(matches!(
        kernel.authorize_local_coding_result_artifact(device.device_id, &mismatch, 16),
        Err(MasterError::ResultArtifactUnavailable)
    ));

    let artifact_directory = tempdir().unwrap();
    let store =
        assemblywright_master::ResultArtifactStore::open(artifact_directory.path()).unwrap();
    let artifact_bytes = admission.artifact.validate().unwrap();
    let mut prepared = store
        .prepare(
            admission.artifact.artifact_id,
            admission.artifact.artifact_sha256,
            &artifact_bytes,
        )
        .unwrap();
    prepared.verified_mut().revalidate(&store).unwrap();

    let mut mismatched_result = result.clone();
    let mut mismatched_payload: LocalCodingJobResult =
        serde_json::from_value(mismatched_result.payload.clone()).unwrap();
    mismatched_payload.artifact_id = Uuid::new_v4();
    mismatched_result.payload = serde_json::to_value(mismatched_payload).unwrap();
    mismatched_result.payload_sha256 =
        Sha256::digest(serde_json::to_vec(&mismatched_result.payload).unwrap()).into();
    assert!(matches!(
        kernel.accept_remote_result_from_with_artifact(
            device.device_id,
            &mismatched_result,
            16,
            &contract,
            &store,
            prepared.verified_mut()
        ),
        Err(MasterError::ResultArtifactUnavailable)
    ));

    let mut expiry_drift = result.clone();
    expiry_drift.payload["workspace_expires_at_ms"] = serde_json::json!(3_000_001_u64);
    expiry_drift.payload_sha256 =
        Sha256::digest(serde_json::to_vec(&expiry_drift.payload).unwrap()).into();
    assert!(matches!(
        kernel.accept_remote_result_from_with_artifact(
            device.device_id,
            &expiry_drift,
            16,
            &contract,
            &store,
            prepared.verified_mut()
        ),
        Err(MasterError::ResultArtifactUnavailable)
    ));
    let mut retained_drift = result.clone();
    retained_drift.payload["workspace_retained"] = serde_json::json!(false);
    retained_drift.payload_sha256 =
        Sha256::digest(serde_json::to_vec(&retained_drift.payload).unwrap()).into();
    assert!(kernel
        .accept_remote_result_from_with_artifact(
            device.device_id,
            &retained_drift,
            16,
            &contract,
            &store,
            prepared.verified_mut()
        )
        .is_err());

    // Artifact admission and its SQLite transaction are complete before the
    // later result request. Release the admission request's stable handles so
    // this test models tampering between those two remote requests on Windows
    // as well as Unix.
    let artifact_reference = prepared.verified_mut().reference();
    prepared.mark_committed().unwrap();
    prepared.cleanup_if_unreferenced(true).unwrap();

    let artifact_path = artifact_directory
        .path()
        .join("feature-result-artifacts")
        .join(admission.artifact.artifact_id.to_string())
        .join("artifact.patch");
    let mut tampered = artifact_bytes.clone();
    tampered[0] ^= 1;
    fs::write(&artifact_path, tampered).unwrap();
    assert!(store.open_verified(artifact_reference).is_err());
    {
        use std::io::Write;
        let mut restored = fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&artifact_path)
            .unwrap();
        restored.write_all(&artifact_bytes).unwrap();
        restored.sync_all().unwrap();
    }
    let mut verified = store.open_verified(artifact_reference).unwrap();
    kernel
        .accept_remote_result_from_with_artifact(
            device.device_id,
            &result,
            16,
            &contract,
            &store,
            &mut verified,
        )
        .unwrap();
    kernel.set_emergency_paused_at(true, 17).unwrap();
    assert!(matches!(
        kernel.authorize_local_coding_result_artifact(device.device_id, &admission, 18),
        Err(MasterError::EmergencyPaused)
    ));
}

#[test]
fn artifact_store_exact_retry_and_startup_orphan_cleanup_fail_closed() {
    let directory = tempdir().unwrap();
    let bytes = build_local_coding_fixture_patch_artifact([0x42; 32]).unwrap();
    let artifact = LocalCodingResultArtifact::from_bytes(Uuid::new_v4(), &bytes).unwrap();
    let store = assemblywright_master::ResultArtifactStore::open(directory.path()).unwrap();
    let first = store
        .prepare(artifact.artifact_id, artifact.artifact_sha256, &bytes)
        .unwrap();
    let retry = store
        .prepare(artifact.artifact_id, artifact.artifact_sha256, &bytes)
        .unwrap();
    let mut mismatch = bytes.clone();
    mismatch.push(b' ');
    assert!(store
        .prepare(artifact.artifact_id, artifact.artifact_sha256, &mismatch)
        .is_err());
    drop(retry);
    drop(first);
    drop(store);

    let process = MasterProcess::acquire(directory.path()).unwrap();
    assert!(process.kernel().result_artifact_ids().unwrap().is_empty());
    assert!(!directory
        .path()
        .join("feature-result-artifacts")
        .join(artifact.artifact_id.to_string())
        .exists());
}

#[test]
fn artifact_store_recovers_crash_prepared_and_concurrent_exact_retries() {
    let directory = tempdir().unwrap();
    let bytes = build_local_coding_fixture_patch_artifact([0x43; 32]).unwrap();
    let artifact = LocalCodingResultArtifact::from_bytes(Uuid::new_v4(), &bytes).unwrap();
    let store = assemblywright_master::ResultArtifactStore::open(directory.path()).unwrap();

    // Dropping the guard models a process crash after durable rename but before
    // the SQLite finalizer. Exact bytes must remain recoverable.
    drop(
        store
            .prepare(artifact.artifact_id, artifact.artifact_sha256, &bytes)
            .unwrap(),
    );
    drop(
        store
            .prepare(artifact.artifact_id, artifact.artifact_sha256, &bytes)
            .unwrap(),
    );

    let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
    let mut threads = Vec::new();
    for _ in 0..2 {
        let store = store.clone();
        let bytes = bytes.clone();
        let barrier = barrier.clone();
        threads.push(std::thread::spawn(move || {
            let guard = store
                .prepare(artifact.artifact_id, artifact.artifact_sha256, &bytes)
                .unwrap();
            barrier.wait();
            barrier.wait();
            drop(guard);
        }));
    }
    barrier.wait();
    barrier.wait();
    for thread in threads {
        thread.join().unwrap();
    }

    let first_cleanup = store
        .prepare(artifact.artifact_id, artifact.artifact_sha256, &bytes)
        .unwrap();
    let second_cleanup = store
        .prepare(artifact.artifact_id, artifact.artifact_sha256, &bytes)
        .unwrap();
    first_cleanup.cleanup_if_unreferenced(false).unwrap();
    let artifact_path = directory
        .path()
        .join("feature-result-artifacts")
        .join(artifact.artifact_id.to_string());
    assert!(artifact_path.exists());
    second_cleanup.cleanup_if_unreferenced(false).unwrap();
    assert!(!artifact_path.exists());

    // Recreate for portable permission assertions below.
    drop(
        store
            .prepare(artifact.artifact_id, artifact.artifact_sha256, &bytes)
            .unwrap(),
    );
    let failed_cleanup = store
        .prepare(artifact.artifact_id, artifact.artifact_sha256, &bytes)
        .unwrap();
    let mut committed_retry = store
        .prepare(artifact.artifact_id, artifact.artifact_sha256, &bytes)
        .unwrap();
    committed_retry.mark_committed().unwrap();
    failed_cleanup.cleanup_if_unreferenced(false).unwrap();
    committed_retry.cleanup_if_unreferenced(false).unwrap();
    assert!(artifact_path.exists());

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let root = directory.path().join("feature-result-artifacts");
        let artifact_directory = root.join(artifact.artifact_id.to_string());
        let artifact_file = artifact_directory.join("artifact.patch");
        assert_eq!(fs::metadata(root).unwrap().mode() & 0o777, 0o700);
        assert_eq!(
            fs::metadata(artifact_directory).unwrap().mode() & 0o777,
            0o700
        );
        assert_eq!(fs::metadata(artifact_file).unwrap().mode() & 0o777, 0o600);
        assert_eq!(
            fs::metadata(
                directory
                    .path()
                    .join("feature-result-artifacts")
                    .join(artifact.artifact_id.to_string())
                    .join("artifact.patch")
            )
            .unwrap()
            .nlink(),
            1
        );
    }
}

#[test]
fn referenced_artifact_missing_or_corrupt_blocks_startup_and_is_not_cleaned() {
    let missing = tempdir().unwrap();
    let (missing_admission, _) = persist_referenced_result_artifact(missing.path(), false);
    assert!(MasterProcess::acquire(missing.path()).is_err());
    assert_eq!(
        Connection::open(missing.path().join("master.sqlite3"))
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM feature_result_artifacts WHERE artifact_id = ?1",
                [missing_admission.artifact.artifact_id.to_string()],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );

    let corrupt = tempdir().unwrap();
    let (corrupt_admission, bytes) = persist_referenced_result_artifact(corrupt.path(), true);
    let corrupt_path = corrupt
        .path()
        .join("feature-result-artifacts")
        .join(corrupt_admission.artifact.artifact_id.to_string())
        .join("artifact.patch");
    let mut tampered = bytes;
    tampered[0] ^= 1;
    fs::write(&corrupt_path, tampered).unwrap();
    assert!(MasterProcess::acquire(corrupt.path()).is_err());
    assert!(corrupt_path.exists());
}

#[cfg(unix)]
#[test]
fn referenced_artifact_reparse_hardlink_and_permissions_block_startup() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    let reparse = tempdir().unwrap();
    let (reparse_admission, bytes) = persist_referenced_result_artifact(reparse.path(), true);
    let reparse_path = reparse
        .path()
        .join("feature-result-artifacts")
        .join(reparse_admission.artifact.artifact_id.to_string())
        .join("artifact.patch");
    fs::remove_file(&reparse_path).unwrap();
    let target = reparse.path().join("outside.patch");
    fs::write(&target, &bytes).unwrap();
    symlink(&target, &reparse_path).unwrap();
    assert!(MasterProcess::acquire(reparse.path()).is_err());
    assert!(fs::symlink_metadata(&reparse_path)
        .unwrap()
        .file_type()
        .is_symlink());

    let hardlink = tempdir().unwrap();
    let (hardlink_admission, _) = persist_referenced_result_artifact(hardlink.path(), true);
    let hardlink_path = hardlink
        .path()
        .join("feature-result-artifacts")
        .join(hardlink_admission.artifact.artifact_id.to_string())
        .join("artifact.patch");
    fs::hard_link(
        &hardlink_path,
        hardlink.path().join("outside-hardlink.patch"),
    )
    .unwrap();
    assert!(MasterProcess::acquire(hardlink.path()).is_err());
    assert!(hardlink_path.exists());

    let permissions = tempdir().unwrap();
    let (permissions_admission, _) = persist_referenced_result_artifact(permissions.path(), true);
    let permissions_path = permissions
        .path()
        .join("feature-result-artifacts")
        .join(permissions_admission.artifact.artifact_id.to_string())
        .join("artifact.patch");
    fs::set_permissions(&permissions_path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(MasterProcess::acquire(permissions.path()).is_err());
    assert!(permissions_path.exists());
}

#[test]
fn post_admission_artifact_tamper_invalidates_stable_result_evidence() {
    let directory = tempdir().unwrap();
    let bytes = build_local_coding_fixture_patch_artifact([0x44; 32]).unwrap();
    let artifact = LocalCodingResultArtifact::from_bytes(Uuid::new_v4(), &bytes).unwrap();
    let store = assemblywright_master::ResultArtifactStore::open(directory.path()).unwrap();
    let mut prepared = store
        .prepare(artifact.artifact_id, artifact.artifact_sha256, &bytes)
        .unwrap();
    let reference = prepared.verified_mut().reference();
    prepared.mark_committed().unwrap();
    prepared.cleanup_if_unreferenced(true).unwrap();
    let path = directory
        .path()
        .join("feature-result-artifacts")
        .join(artifact.artifact_id.to_string())
        .join("artifact.patch");
    let mut tampered = bytes;
    tampered[0] ^= 1;
    fs::write(path, tampered).unwrap();
    assert!(store.open_verified(reference).is_err());
}

#[test]
fn abandonment_rejects_malformed_durable_resolution_transition_evidence() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    let (feature, cancelled, queue_revision, pause_revision) = {
        let mut kernel = MasterKernel::open(&database).unwrap();
        let repository_id = Uuid::new_v4();
        install_grants(&mut kernel, repository_id);
        let feature = specification(Uuid::new_v4(), repository_id, vec![]);
        kernel.enqueue_approved_feature(&feature, 0, 10).unwrap();
        let claim = claim_feature(&mut kernel, &feature, 1, 11).unwrap();
        let transition = FeatureTransitionEvidence {
            repository_snapshot_sha256: claim.snapshot_sha256,
            accepted_evidence_sha256: digest("stored-merge-transition-evidence"),
        };
        let mut snapshot = kernel.feature_snapshot(feature.feature_id).unwrap();
        for next in [
            FeatureLifecycleStatus::Validating,
            FeatureLifecycleStatus::Reviewing,
            FeatureLifecycleStatus::Publishing,
            FeatureLifecycleStatus::VerifyingMain,
        ] {
            snapshot = kernel
                .advance_feature_lifecycle(
                    feature.feature_id,
                    snapshot.lifecycle_revision,
                    next,
                    transition,
                    12 + snapshot.lifecycle_revision,
                )
                .unwrap();
        }
        let queue_revision = kernel.feature_queue_revision().unwrap();
        let pause_revision = kernel.emergency_pause_revision().unwrap();
        let cancelled = kernel
            .cancel_active_feature(
                feature.feature_id,
                snapshot.lifecycle_revision,
                queue_revision,
                pause_revision,
                20,
            )
            .unwrap();
        (feature, cancelled, queue_revision, pause_revision)
    };
    let connection = Connection::open(&database).unwrap();
    connection
        .execute_batch("DROP TRIGGER feature_transition_evidence_no_update")
        .unwrap();
    connection
        .execute(
            "UPDATE feature_transition_evidence
             SET accepted_evidence_sha256 = NULL
             WHERE feature_id = ?1 AND lifecycle_revision = ?2",
            rusqlite::params![
                feature.feature_id.to_string(),
                (cancelled.lifecycle_revision - 1) as i64,
            ],
        )
        .unwrap();
    drop(connection);

    let mut reopened = MasterKernel::open(&database).unwrap();
    assert!(matches!(
        reopened.abandon_and_advance(
            feature.feature_id,
            cancelled.lifecycle_revision,
            queue_revision,
            pause_revision,
            FeatureAbandonmentEvidence {
                safe_reconciliation_sha256: digest("safe-reconciliation"),
                merged: true,
                verified_healthy_main_sha256: Some(digest("healthy-main")),
            },
            21,
        ),
        Err(MasterError::InvalidStoredState(_))
    ));
    let retained = reopened.feature_snapshot(feature.feature_id).unwrap();
    assert_eq!(retained.status, FeatureLifecycleStatus::Cancelled);
    assert!(retained.active_lease_id.is_some());
    assert_eq!(reopened.feature_queue_revision().unwrap(), queue_revision);
}

#[test]
fn cancellation_from_verifying_main_cannot_bypass_healthy_main_evidence() {
    let mut kernel = MasterKernel::in_memory().unwrap();
    let repository_id = Uuid::new_v4();
    install_grants(&mut kernel, repository_id);
    let feature = specification(Uuid::new_v4(), repository_id, vec![]);
    kernel.enqueue_approved_feature(&feature, 0, 10).unwrap();
    let claim = claim_feature(&mut kernel, &feature, 1, 11).unwrap();
    let transition = FeatureTransitionEvidence {
        repository_snapshot_sha256: claim.snapshot_sha256,
        accepted_evidence_sha256: digest("merged-transition-evidence"),
    };
    let mut snapshot = kernel.feature_snapshot(feature.feature_id).unwrap();
    for next in [
        FeatureLifecycleStatus::Validating,
        FeatureLifecycleStatus::Reviewing,
        FeatureLifecycleStatus::Publishing,
        FeatureLifecycleStatus::VerifyingMain,
    ] {
        snapshot = kernel
            .advance_feature_lifecycle(
                feature.feature_id,
                snapshot.lifecycle_revision,
                next,
                transition,
                12 + snapshot.lifecycle_revision,
            )
            .unwrap();
    }
    let queue_revision = kernel.feature_queue_revision().unwrap();
    let pause_revision = kernel.emergency_pause_revision().unwrap();
    let cancelled = kernel
        .cancel_active_feature(
            feature.feature_id,
            snapshot.lifecycle_revision,
            queue_revision,
            pause_revision,
            20,
        )
        .unwrap();

    for evidence in [
        FeatureAbandonmentEvidence {
            safe_reconciliation_sha256: digest("verified-safe-reconciliation"),
            merged: false,
            verified_healthy_main_sha256: None,
        },
        FeatureAbandonmentEvidence {
            safe_reconciliation_sha256: digest("verified-safe-reconciliation"),
            merged: false,
            verified_healthy_main_sha256: Some(digest("healthy-main")),
        },
    ] {
        assert!(matches!(
            kernel.abandon_and_advance(
                feature.feature_id,
                cancelled.lifecycle_revision,
                queue_revision,
                pause_revision,
                evidence,
                21,
            ),
            Err(MasterError::VerifiedHealthyMainRequired)
        ));
        let retained = kernel.feature_snapshot(feature.feature_id).unwrap();
        assert_eq!(retained.status, FeatureLifecycleStatus::Cancelled);
        assert_eq!(retained.active_lease_id, Some(claim.lease_id));
        assert_eq!(kernel.feature_queue_revision().unwrap(), queue_revision);
    }

    let abandoned = kernel
        .abandon_and_advance(
            feature.feature_id,
            cancelled.lifecycle_revision,
            queue_revision,
            pause_revision,
            FeatureAbandonmentEvidence {
                safe_reconciliation_sha256: digest("verified-safe-reconciliation"),
                merged: true,
                verified_healthy_main_sha256: Some(digest("healthy-main")),
            },
            22,
        )
        .unwrap();
    assert_eq!(abandoned.status, FeatureLifecycleStatus::Abandoned);
    assert!(abandoned.active_lease_id.is_none());
}

#[test]
fn verifying_main_startup_quarantine_also_requires_healthy_main_before_abandonment() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    let (feature, claim, lifecycle_revision, queue_revision, pause_revision) = {
        let mut kernel = MasterKernel::open(&database).unwrap();
        let repository_id = Uuid::new_v4();
        install_grants(&mut kernel, repository_id);
        let feature = specification(Uuid::new_v4(), repository_id, vec![]);
        kernel.enqueue_approved_feature(&feature, 0, 10).unwrap();
        let claim = claim_feature(&mut kernel, &feature, 1, 11).unwrap();
        let transition = FeatureTransitionEvidence {
            repository_snapshot_sha256: claim.snapshot_sha256,
            accepted_evidence_sha256: digest("restart-merged-transition-evidence"),
        };
        let mut snapshot = kernel.feature_snapshot(feature.feature_id).unwrap();
        for next in [
            FeatureLifecycleStatus::Validating,
            FeatureLifecycleStatus::Reviewing,
            FeatureLifecycleStatus::Publishing,
            FeatureLifecycleStatus::VerifyingMain,
        ] {
            snapshot = kernel
                .advance_feature_lifecycle(
                    feature.feature_id,
                    snapshot.lifecycle_revision,
                    next,
                    transition,
                    12 + snapshot.lifecycle_revision,
                )
                .unwrap();
        }
        (
            feature,
            claim,
            snapshot.lifecycle_revision,
            kernel.feature_queue_revision().unwrap(),
            kernel.emergency_pause_revision().unwrap(),
        )
    };

    let mut restarted = MasterKernel::open(&database).unwrap();
    assert_eq!(restarted.feature_startup_quarantines(), 1);
    let quarantined = restarted.feature_snapshot(feature.feature_id).unwrap();
    assert_eq!(quarantined.status, FeatureLifecycleStatus::Quarantined);
    assert_eq!(quarantined.lifecycle_revision, lifecycle_revision + 1);
    assert_eq!(quarantined.active_lease_id, Some(claim.lease_id));
    assert!(matches!(
        restarted.abandon_and_advance(
            feature.feature_id,
            quarantined.lifecycle_revision,
            queue_revision,
            pause_revision,
            FeatureAbandonmentEvidence {
                safe_reconciliation_sha256: digest("restart-safe-reconciliation"),
                merged: false,
                verified_healthy_main_sha256: None,
            },
            30,
        ),
        Err(MasterError::VerifiedHealthyMainRequired)
    ));
    let retained = restarted.feature_snapshot(feature.feature_id).unwrap();
    assert_eq!(retained.status, FeatureLifecycleStatus::Quarantined);
    assert_eq!(retained.active_lease_id, Some(claim.lease_id));
}

#[test]
fn owner_resolution_is_exact_queue_and_pause_bound_even_while_paused() {
    let mut kernel = MasterKernel::in_memory().unwrap();
    let repository_id = Uuid::new_v4();
    install_grants(&mut kernel, repository_id);
    let feature = specification(Uuid::new_v4(), repository_id, vec![]);
    kernel.enqueue_approved_feature(&feature, 0, 10).unwrap();
    let claim = claim_feature(&mut kernel, &feature, 1, 11).unwrap();
    let queue_revision = kernel.feature_queue_revision().unwrap();
    kernel.set_emergency_paused_at(true, 12).unwrap();
    let pause_revision = kernel.emergency_pause_revision().unwrap();

    assert!(matches!(
        kernel.cancel_active_feature(
            feature.feature_id,
            claim.lifecycle_revision,
            queue_revision + 1,
            pause_revision,
            13,
        ),
        Err(MasterError::StaleFeatureQueueRevision { .. })
    ));
    assert!(matches!(
        kernel.cancel_active_feature(
            feature.feature_id,
            claim.lifecycle_revision,
            queue_revision,
            pause_revision - 1,
            14,
        ),
        Err(MasterError::StaleEmergencyPauseRevision { .. })
    ));
    let unchanged = kernel.feature_snapshot(feature.feature_id).unwrap();
    assert_eq!(unchanged.status, FeatureLifecycleStatus::Implementing);
    assert_eq!(unchanged.lifecycle_revision, claim.lifecycle_revision);
    assert_eq!(unchanged.active_lease_id, Some(claim.lease_id));

    let cancelled = kernel
        .cancel_active_feature(
            feature.feature_id,
            claim.lifecycle_revision,
            queue_revision,
            pause_revision,
            15,
        )
        .unwrap();
    assert_eq!(cancelled.status, FeatureLifecycleStatus::Cancelled);
    assert_eq!(cancelled.active_lease_id, Some(claim.lease_id));
    let evidence = FeatureAbandonmentEvidence {
        safe_reconciliation_sha256: digest("paused-safe-reconciliation"),
        merged: false,
        verified_healthy_main_sha256: None,
    };

    assert!(matches!(
        kernel.abandon_and_advance(
            feature.feature_id,
            cancelled.lifecycle_revision,
            queue_revision + 1,
            pause_revision,
            evidence,
            16,
        ),
        Err(MasterError::StaleFeatureQueueRevision { .. })
    ));
    assert!(matches!(
        kernel.abandon_and_advance(
            feature.feature_id,
            cancelled.lifecycle_revision,
            queue_revision,
            pause_revision - 1,
            evidence,
            17,
        ),
        Err(MasterError::StaleEmergencyPauseRevision { .. })
    ));
    let abandoned = kernel
        .abandon_and_advance(
            feature.feature_id,
            cancelled.lifecycle_revision,
            queue_revision,
            pause_revision,
            evidence,
            18,
        )
        .unwrap();
    assert_eq!(abandoned.status, FeatureLifecycleStatus::Abandoned);
    assert!(abandoned.active_lease_id.is_none());
    assert_eq!(kernel.feature_queue_revision().unwrap(), queue_revision + 1);
    assert!(kernel.emergency_paused().unwrap());
}

#[test]
fn cancellation_blocks_until_safe_abandonment_and_merged_main_is_healthy() {
    let mut kernel = MasterKernel::in_memory().unwrap();
    let repository_id = Uuid::new_v4();
    install_grants(&mut kernel, repository_id);
    let feature = specification(Uuid::new_v4(), repository_id, vec![]);
    kernel.enqueue_approved_feature(&feature, 0, 10).unwrap();
    let claim = claim_feature(&mut kernel, &feature, 1, 11).unwrap();
    let cancelled = kernel
        .cancel_active_feature(
            feature.feature_id,
            claim.lifecycle_revision,
            kernel.feature_queue_revision().unwrap(),
            kernel.emergency_pause_revision().unwrap(),
            12,
        )
        .unwrap();
    assert_eq!(cancelled.status, FeatureLifecycleStatus::Cancelled);
    assert_eq!(cancelled.active_lease_id, Some(claim.lease_id));
    assert!(matches!(
        kernel
            .prepare_repository_snapshot_claim(
                &snapshot_plan(&feature, kernel.feature_queue_revision().unwrap(), 0),
                13,
            )
            .unwrap_err(),
        MasterError::FeatureLeaseAlreadyActive
    ));
    assert!(matches!(
        kernel
            .abandon_and_advance(
                feature.feature_id,
                cancelled.lifecycle_revision,
                kernel.feature_queue_revision().unwrap(),
                kernel.emergency_pause_revision().unwrap(),
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
                kernel.emergency_pause_revision().unwrap(),
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
            kernel.emergency_pause_revision().unwrap(),
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
        kernel
            .prepare_repository_snapshot_claim(&snapshot_plan(&feature, 1, 1), 12)
            .unwrap_err(),
        MasterError::EmergencyPaused
    ));
    kernel.set_emergency_paused_at(false, 13).unwrap();
    install_audit_failure(&database, "feature_snapshot_claimed");
    assert!(matches!(
        claim_feature(&mut kernel, &feature, 1, 14).unwrap_err(),
        MasterError::Storage(_)
    ));
    let snapshot = kernel.feature_snapshot(feature.feature_id).unwrap();
    assert_eq!(snapshot.status, FeatureLifecycleStatus::Queued);
    assert!(snapshot.active_lease_id.is_none());
    assert_eq!(kernel.feature_queue_revision().unwrap(), 1);
    remove_audit_failure(&database);

    let claim = claim_feature(&mut kernel, &feature, 1, 15).unwrap();
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
            .cancel_active_feature(
                feature.feature_id,
                validating.lifecycle_revision,
                kernel.feature_queue_revision().unwrap(),
                kernel.emergency_pause_revision().unwrap(),
                18,
            )
            .unwrap_err(),
        MasterError::Storage(_)
    ));
    let snapshot = kernel.feature_snapshot(feature.feature_id).unwrap();
    assert_eq!(snapshot.status, FeatureLifecycleStatus::Validating);
    assert_eq!(snapshot.lifecycle_revision, validating.lifecycle_revision);
    assert_eq!(snapshot.active_lease_id, Some(claim.lease_id));
    remove_audit_failure(&database);

    let cancelled = kernel
        .cancel_active_feature(
            feature.feature_id,
            validating.lifecycle_revision,
            kernel.feature_queue_revision().unwrap(),
            kernel.emergency_pause_revision().unwrap(),
            19,
        )
        .unwrap();
    install_audit_failure(&database, "feature_abandoned");
    assert!(matches!(
        kernel
            .abandon_and_advance(
                feature.feature_id,
                cancelled.lifecycle_revision,
                kernel.feature_queue_revision().unwrap(),
                kernel.emergency_pause_revision().unwrap(),
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
        lease_id = claim_feature(&mut kernel, &feature, 1, 11)
            .unwrap()
            .lease_id;
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
        claim = claim_feature(&mut kernel, &feature, 1, 11).unwrap();
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

fn drop_validation_gate_schema_for_legacy_fixture(connection: &Connection) {
    drop_review_gateway_schema_for_legacy_fixture(connection);
    connection
        .execute_batch(
            "DROP TRIGGER feature_validation_completions_no_update;
             DROP TRIGGER feature_validation_completions_no_delete;
             DROP TABLE feature_validation_completions;
             DROP TRIGGER feature_validation_command_evidence_no_update;
             DROP TRIGGER feature_validation_command_evidence_no_delete;
             DROP TABLE feature_validation_command_evidence;
             DROP TRIGGER feature_validation_attempts_no_update;
             DROP TRIGGER feature_validation_attempts_no_delete;
             DROP TABLE feature_validation_attempts;",
        )
        .unwrap();
}

fn drop_review_gateway_schema_for_legacy_fixture(connection: &Connection) {
    drop_publication_schema_for_legacy_fixture(connection);
    connection
        .execute_batch(
            "DROP TRIGGER feature_review_decisions_no_update;
             DROP TRIGGER feature_review_decisions_no_delete;
             DROP TABLE feature_review_decisions;
             DROP TRIGGER feature_review_call_outcomes_no_update;
             DROP TRIGGER feature_review_call_outcomes_no_delete;
             DROP TABLE feature_review_call_outcomes;
             DROP TRIGGER feature_review_calls_no_update;
             DROP TRIGGER feature_review_calls_no_delete;
             DROP INDEX feature_review_calls_candidate_idx;
             DROP TABLE feature_review_calls;",
        )
        .unwrap();
}

fn drop_publication_schema_for_legacy_fixture(connection: &Connection) {
    connection
        .execute_batch(
            "DROP TRIGGER feature_publication_completions_no_update;
             DROP TRIGGER feature_publication_completions_no_delete;
             DROP TABLE feature_publication_completions;
             DROP TRIGGER feature_publication_action_outcomes_no_update;
             DROP TRIGGER feature_publication_action_outcomes_no_delete;
             DROP TABLE feature_publication_action_outcomes;
             DROP TRIGGER feature_publication_action_intents_no_update;
             DROP TRIGGER feature_publication_action_intents_no_delete;
             DROP TABLE feature_publication_action_intents;
             DROP TRIGGER feature_publications_no_update;
             DROP TRIGGER feature_publications_no_delete;
             DROP TABLE feature_publications;",
        )
        .unwrap();
}

fn downgrade_v5_database_to_v4(path: &std::path::Path, sabotage: bool) {
    drop(MasterKernel::open(path).unwrap());
    let connection = Connection::open(path).unwrap();
    drop_validation_gate_schema_for_legacy_fixture(&connection);
    connection
        .execute_batch(
            "DROP TRIGGER feature_artifact_integration_conflicts_no_update;
             DROP TRIGGER feature_artifact_integration_conflicts_no_delete;
             DROP TABLE feature_artifact_integration_conflicts;
             DROP TRIGGER feature_artifact_integration_artifacts_no_update;
             DROP TRIGGER feature_artifact_integration_artifacts_no_delete;
             DROP TABLE feature_artifact_integration_artifacts;
             DROP TRIGGER feature_artifact_integrations_no_update;
             DROP TRIGGER feature_artifact_integrations_no_delete;
             DROP TABLE feature_artifact_integrations;
             DROP TRIGGER feature_result_artifacts_no_update;
             DROP TRIGGER feature_result_artifacts_no_delete;
             DROP TABLE feature_result_artifacts;
             DROP TRIGGER feature_specification_revisions_no_update;
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
             DROP TRIGGER feature_repository_snapshot_claims_no_update;
             DROP TRIGGER feature_repository_snapshot_claims_no_delete;
             DROP TRIGGER feature_active_lease_requires_snapshot;
             DROP TRIGGER feature_coding_dispatches_no_update;
             DROP TRIGGER feature_coding_dispatches_no_delete;
             DROP TABLE feature_coding_dispatches;
             DROP TABLE master_identity_rebind_audit;
             DROP TABLE master_pending_capability_rebinds;
             DROP TABLE feature_owner_control_state;
             DROP TABLE feature_conveyor_audit;
             DROP TABLE feature_transition_evidence;
             DROP TABLE feature_active_lease;
             DROP TABLE feature_repository_snapshot_claims;
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

fn remove_resolution_receipt_for_v10_fixture(
    database: &std::path::Path,
    feature_id: Uuid,
    lifecycle_revision: u64,
    legacy_cancellation_audit: bool,
) {
    let connection = Connection::open(database).unwrap();
    drop_validation_gate_schema_for_legacy_fixture(&connection);
    connection
        .execute_batch("DROP TRIGGER feature_transition_evidence_no_delete;")
        .unwrap();
    connection
        .execute(
            "DELETE FROM feature_transition_evidence
             WHERE feature_id = ?1 AND lifecycle_revision = ?2",
            rusqlite::params![feature_id.to_string(), lifecycle_revision as i64],
        )
        .unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER feature_transition_evidence_no_delete
               BEFORE DELETE ON feature_transition_evidence
               BEGIN SELECT RAISE(ABORT, 'durable transition evidence'); END;",
        )
        .unwrap();
    if legacy_cancellation_audit {
        connection
            .execute_batch("DROP TRIGGER feature_conveyor_audit_no_update;")
            .unwrap();
        connection
            .execute(
                "UPDATE feature_conveyor_audit
                 SET redacted_metadata_json = ?1
                 WHERE event_kind = 'feature_cancelled' AND feature_id = ?2",
                rusqlite::params![
                    canonical_manifest(&json!({
                        "from_status": "implementing",
                        "to_status": "cancelled",
                        "lifecycle_revision": lifecycle_revision,
                        "lease_retained": true,
                        "advancement_authorized": false,
                        "effect_possible": true,
                        "side_effect_executed": false
                    })),
                    feature_id.to_string(),
                ],
            )
            .unwrap();
        connection
            .execute_batch(
                "CREATE TRIGGER feature_conveyor_audit_no_update
                   BEFORE UPDATE ON feature_conveyor_audit
                   BEGIN SELECT RAISE(ABORT, 'append-only feature audit'); END;",
            )
            .unwrap();
    }
    connection
        .execute_batch(
            "DROP TRIGGER feature_artifact_integration_conflicts_no_update;
             DROP TRIGGER feature_artifact_integration_conflicts_no_delete;
             DROP TABLE feature_artifact_integration_conflicts;
             DROP TRIGGER feature_artifact_integration_artifacts_no_update;
             DROP TRIGGER feature_artifact_integration_artifacts_no_delete;
             DROP TABLE feature_artifact_integration_artifacts;
             DROP TRIGGER feature_artifact_integrations_no_update;
             DROP TRIGGER feature_artifact_integrations_no_delete;
             DROP TABLE feature_artifact_integrations;
             DROP TRIGGER feature_result_artifacts_no_update;
             DROP TRIGGER feature_result_artifacts_no_delete;
             DROP TABLE feature_result_artifacts;
             PRAGMA user_version = 10;",
        )
        .unwrap();
}

#[test]
fn master_process_v10_backfills_resolution_receipts_from_exact_immutable_audit() {
    for startup_quarantine in [false, true] {
        let directory = tempdir().unwrap();
        let database = directory.path().join("master.sqlite3");
        let repository_id = Uuid::new_v4();
        let feature = specification(Uuid::new_v4(), repository_id, vec![]);
        let resolved = {
            let mut kernel = MasterKernel::open(&database).unwrap();
            install_grants(&mut kernel, repository_id);
            kernel.enqueue_approved_feature(&feature, 0, 10).unwrap();
            let claim = claim_feature(&mut kernel, &feature, 1, 11).unwrap();
            if startup_quarantine {
                drop(kernel);
                MasterKernel::open(&database)
                    .unwrap()
                    .feature_snapshot(feature.feature_id)
                    .unwrap()
            } else {
                kernel
                    .cancel_active_feature(
                        feature.feature_id,
                        claim.lifecycle_revision,
                        kernel.feature_queue_revision().unwrap(),
                        kernel.emergency_pause_revision().unwrap(),
                        12,
                    )
                    .unwrap()
            }
        };
        assert_eq!(
            resolved.status,
            if startup_quarantine {
                FeatureLifecycleStatus::Quarantined
            } else {
                FeatureLifecycleStatus::Cancelled
            }
        );
        remove_resolution_receipt_for_v10_fixture(
            &database,
            feature.feature_id,
            resolved.lifecycle_revision,
            !startup_quarantine,
        );

        let mut process = MasterProcess::acquire(directory.path()).unwrap();
        assert_eq!(
            process.kernel().schema_version().unwrap(),
            MASTER_SCHEMA_VERSION
        );
        let backup = process.migration_backup_path().unwrap();
        assert!(backup
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("master.pre-v17."));
        assert_eq!(
            Connection::open(backup)
                .unwrap()
                .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
                .unwrap(),
            10
        );
        let receipt: (String, String) = Connection::open(&database)
            .unwrap()
            .query_row(
                "SELECT from_status, to_status FROM feature_transition_evidence
                 WHERE feature_id = ?1 AND lifecycle_revision = ?2",
                rusqlite::params![
                    feature.feature_id.to_string(),
                    resolved.lifecycle_revision as i64,
                ],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(receipt.0, "implementing");
        assert_eq!(
            receipt.1,
            if startup_quarantine {
                "quarantined"
            } else {
                "cancelled"
            }
        );
        let queue_revision = process.kernel().feature_queue_revision().unwrap();
        let pause_revision = process.kernel().emergency_pause_revision().unwrap();
        let abandoned = process
            .kernel_mut()
            .abandon_and_advance(
                feature.feature_id,
                resolved.lifecycle_revision,
                queue_revision,
                pause_revision,
                FeatureAbandonmentEvidence {
                    safe_reconciliation_sha256: digest("migrated-safe-reconciliation"),
                    merged: false,
                    verified_healthy_main_sha256: None,
                },
                20,
            )
            .unwrap();
        assert_eq!(abandoned.status, FeatureLifecycleStatus::Abandoned);
        assert!(abandoned.active_lease_id.is_none());
    }
}

#[test]
fn master_process_v10_ambiguous_resolution_audit_fails_closed_and_restores_backup() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    let repository_id = Uuid::new_v4();
    let feature = specification(Uuid::new_v4(), repository_id, vec![]);
    let cancelled = {
        let mut kernel = MasterKernel::open(&database).unwrap();
        install_grants(&mut kernel, repository_id);
        kernel.enqueue_approved_feature(&feature, 0, 10).unwrap();
        let claim = claim_feature(&mut kernel, &feature, 1, 11).unwrap();
        kernel
            .cancel_active_feature(
                feature.feature_id,
                claim.lifecycle_revision,
                kernel.feature_queue_revision().unwrap(),
                kernel.emergency_pause_revision().unwrap(),
                12,
            )
            .unwrap()
    };
    remove_resolution_receipt_for_v10_fixture(
        &database,
        feature.feature_id,
        cancelled.lifecycle_revision,
        true,
    );
    let connection = Connection::open(&database).unwrap();
    connection
        .execute(
            "INSERT INTO feature_conveyor_audit (
               event_kind, feature_id, occurred_at_ms, redacted_metadata_json
             ) SELECT event_kind, feature_id, occurred_at_ms, redacted_metadata_json
               FROM feature_conveyor_audit
               WHERE event_kind = 'feature_cancelled' AND feature_id = ?1",
            [feature.feature_id.to_string()],
        )
        .unwrap();
    drop(connection);

    assert!(matches!(
        MasterProcess::acquire(directory.path()),
        Err(MasterError::InvalidStoredState(_))
    ));
    let restored = Connection::open(&database).unwrap();
    assert_eq!(
        restored
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        10
    );
    assert_eq!(
        restored
            .query_row(
                "SELECT COUNT(*) FROM feature_transition_evidence
                 WHERE feature_id = ?1 AND lifecycle_revision = ?2",
                rusqlite::params![
                    feature.feature_id.to_string(),
                    cancelled.lifecycle_revision as i64,
                ],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
    assert!(directory
        .path()
        .read_dir()
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| entry
            .file_name()
            .to_string_lossy()
            .starts_with("master.pre-v17.")));
}

#[test]
fn master_process_v10_malformed_resolution_audit_fails_closed_and_restores_backup() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    let repository_id = Uuid::new_v4();
    let feature = specification(Uuid::new_v4(), repository_id, vec![]);
    let cancelled = {
        let mut kernel = MasterKernel::open(&database).unwrap();
        install_grants(&mut kernel, repository_id);
        kernel.enqueue_approved_feature(&feature, 0, 10).unwrap();
        let claim = claim_feature(&mut kernel, &feature, 1, 11).unwrap();
        kernel
            .cancel_active_feature(
                feature.feature_id,
                claim.lifecycle_revision,
                kernel.feature_queue_revision().unwrap(),
                kernel.emergency_pause_revision().unwrap(),
                12,
            )
            .unwrap()
    };
    remove_resolution_receipt_for_v10_fixture(
        &database,
        feature.feature_id,
        cancelled.lifecycle_revision,
        true,
    );
    let connection = Connection::open(&database).unwrap();
    connection
        .execute_batch("DROP TRIGGER feature_conveyor_audit_no_update;")
        .unwrap();
    connection
        .execute(
            "UPDATE feature_conveyor_audit
             SET redacted_metadata_json = ?1
             WHERE event_kind = 'feature_cancelled' AND feature_id = ?2",
            rusqlite::params![
                canonical_manifest(&json!({
                    "from_status": "implementing",
                    "to_status": "cancelled",
                    "lifecycle_revision": cancelled.lifecycle_revision,
                    "lease_retained": true,
                    "advancement_authorized": false,
                    "effect_possible": true,
                    "side_effect_executed": false,
                    "unexpected_authority": true
                })),
                feature.feature_id.to_string(),
            ],
        )
        .unwrap();
    connection
        .execute_batch(
            "CREATE TRIGGER feature_conveyor_audit_no_update
               BEFORE UPDATE ON feature_conveyor_audit
               BEGIN SELECT RAISE(ABORT, 'append-only feature audit'); END;",
        )
        .unwrap();
    drop(connection);

    assert!(matches!(
        MasterProcess::acquire(directory.path()),
        Err(MasterError::InvalidStoredState(_))
    ));
    let restored = Connection::open(&database).unwrap();
    assert_eq!(
        restored
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        10
    );
    assert_eq!(
        restored
            .query_row(
                "SELECT COUNT(*) FROM feature_transition_evidence
                 WHERE feature_id = ?1 AND lifecycle_revision = ?2",
                rusqlite::params![
                    feature.feature_id.to_string(),
                    cancelled.lifecycle_revision as i64,
                ],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        0
    );
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
        assert_eq!(
            process.kernel().schema_version().unwrap(),
            MASTER_SCHEMA_VERSION
        );
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
        MASTER_SCHEMA_VERSION
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
            .starts_with("master.pre-v17.")));
}

#[test]
fn master_process_v12_backup_first_migration_adds_retained_workspace_binding() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    let (admission, _) = persist_referenced_result_artifact(directory.path(), true);
    let historical_bytes = historical_v4_artifact_bytes();
    fs::write(
        directory
            .path()
            .join("feature-result-artifacts")
            .join(admission.artifact.artifact_id.to_string())
            .join("artifact.patch"),
        &historical_bytes,
    )
    .unwrap();
    let connection = Connection::open(&database).unwrap();
    drop_validation_gate_schema_for_legacy_fixture(&connection);
    connection
        .execute_batch("DROP TRIGGER feature_result_artifacts_no_update;")
        .unwrap();
    connection
        .execute(
            "UPDATE feature_result_artifacts
             SET artifact_sha256 = ?1, artifact_size_bytes = ?2
             WHERE artifact_id = ?3",
            rusqlite::params![
                Sha256::digest(&historical_bytes).as_slice(),
                historical_bytes.len() as i64,
                admission.artifact.artifact_id.to_string()
            ],
        )
        .unwrap();
    connection
        .execute_batch(
            "DROP TRIGGER feature_artifact_integration_conflicts_no_update;
             DROP TRIGGER feature_artifact_integration_conflicts_no_delete;
             DROP TABLE feature_artifact_integration_conflicts;
             DROP TRIGGER feature_artifact_integration_artifacts_no_update;
             DROP TRIGGER feature_artifact_integration_artifacts_no_delete;
             DROP TABLE feature_artifact_integration_artifacts;
             DROP TRIGGER feature_artifact_integrations_no_update;
             DROP TRIGGER feature_artifact_integrations_no_delete;
             DROP TABLE feature_artifact_integrations;
             CREATE TRIGGER feature_result_artifacts_no_update
               BEFORE UPDATE ON feature_result_artifacts
               BEGIN SELECT RAISE(ABORT, 'immutable feature result artifact'); END;
             ALTER TABLE feature_result_artifacts DROP COLUMN workspace_expires_at_ms;
             ALTER TABLE feature_result_artifacts DROP COLUMN workspace_retained;
             PRAGMA user_version = 12;",
        )
        .unwrap();
    drop(connection);

    let process = MasterProcess::acquire(directory.path()).unwrap();
    assert_eq!(
        process.kernel().schema_version().unwrap(),
        MASTER_SCHEMA_VERSION
    );
    let backup = process.migration_backup_path().unwrap();
    assert!(backup
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("master.pre-v17."));
    assert_eq!(
        Connection::open(backup)
            .unwrap()
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        12
    );
    let columns: HashSet<String> = Connection::open(&database)
        .unwrap()
        .prepare("PRAGMA table_info(feature_result_artifacts)")
        .unwrap()
        .query_map([], |row| row.get(1))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert!(columns.contains("workspace_retained"));
    assert!(columns.contains("workspace_expires_at_ms"));
}

#[test]
fn master_process_v12_failed_migration_restores_verified_backup() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    drop(MasterKernel::open(&database).unwrap());
    let connection = Connection::open(&database).unwrap();
    drop_validation_gate_schema_for_legacy_fixture(&connection);
    connection
        .execute_batch(
            "DROP TRIGGER feature_artifact_integration_conflicts_no_update;
             DROP TRIGGER feature_artifact_integration_conflicts_no_delete;
             DROP TABLE feature_artifact_integration_conflicts;
             DROP TRIGGER feature_artifact_integration_artifacts_no_update;
             DROP TRIGGER feature_artifact_integration_artifacts_no_delete;
             DROP TABLE feature_artifact_integration_artifacts;
             DROP TRIGGER feature_artifact_integrations_no_update;
             DROP TRIGGER feature_artifact_integrations_no_delete;
             DROP TABLE feature_artifact_integrations;
             PRAGMA user_version = 12;",
        )
        .unwrap();
    drop(connection);
    assert!(matches!(
        MasterProcess::acquire(directory.path()),
        Err(MasterError::Storage(_))
    ));
    assert_eq!(
        Connection::open(&database)
            .unwrap()
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        12
    );
    assert!(directory
        .path()
        .read_dir()
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| entry
            .file_name()
            .to_string_lossy()
            .starts_with("master.pre-v17.")));
}

#[test]
fn master_process_v6_backup_migration_binds_pause_and_default_inert_owner_control() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    drop(MasterKernel::open(&database).unwrap());
    let connection = Connection::open(&database).unwrap();
    drop_validation_gate_schema_for_legacy_fixture(&connection);
    connection
        .execute_batch(
            "DROP TRIGGER feature_result_artifacts_no_update;
             DROP TRIGGER feature_result_artifacts_no_delete;
             DROP TRIGGER feature_artifact_integration_conflicts_no_update;
             DROP TRIGGER feature_artifact_integration_conflicts_no_delete;
             DROP TABLE feature_artifact_integration_conflicts;
             DROP TRIGGER feature_artifact_integration_artifacts_no_update;
             DROP TRIGGER feature_artifact_integration_artifacts_no_delete;
             DROP TABLE feature_artifact_integration_artifacts;
             DROP TRIGGER feature_artifact_integrations_no_update;
             DROP TRIGGER feature_artifact_integrations_no_delete;
             DROP TABLE feature_artifact_integrations;
             DROP TABLE feature_result_artifacts;
             DROP TRIGGER feature_coding_dispatches_no_update;
             DROP TRIGGER feature_coding_dispatches_no_delete;
             DROP TABLE feature_coding_dispatches;
             DROP TRIGGER feature_active_lease_requires_snapshot;
             DROP TRIGGER feature_repository_snapshot_claims_no_update;
             DROP TRIGGER feature_repository_snapshot_claims_no_delete;
             ALTER TABLE feature_active_lease RENAME TO feature_active_lease_v9;
             CREATE TABLE feature_active_lease (
               singleton INTEGER PRIMARY KEY NOT NULL CHECK (singleton = 1),
               feature_id TEXT NOT NULL UNIQUE REFERENCES feature_conveyor_features(feature_id),
               lease_id TEXT NOT NULL UNIQUE,
               claimed_at_ms INTEGER NOT NULL CHECK (claimed_at_ms >= 0)
             );
             INSERT INTO feature_active_lease (singleton, feature_id, lease_id, claimed_at_ms)
               SELECT singleton, feature_id, lease_id, claimed_at_ms FROM feature_active_lease_v9;
             DROP TABLE feature_active_lease_v9;
             DROP TABLE feature_repository_snapshot_claims;
             DROP TABLE feature_owner_control_state;
             DELETE FROM master_metadata WHERE key = 'emergency_pause_revision';
             PRAGMA user_version = 6;",
        )
        .unwrap();

    assert!(matches!(
        MasterKernel::open(&database),
        Err(MasterError::MigrationBackup(_))
    ));

    let process = MasterProcess::acquire(directory.path()).unwrap();
    assert_eq!(
        process.kernel().schema_version().unwrap(),
        MASTER_SCHEMA_VERSION
    );
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
        .starts_with("master.pre-v17."));
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
fn master_process_v9_backup_first_migration_adds_immutable_coding_dispatch_evidence() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    drop(MasterKernel::open(&database).unwrap());
    let connection = Connection::open(&database).unwrap();
    drop_validation_gate_schema_for_legacy_fixture(&connection);
    connection
        .execute_batch(
            "DROP TRIGGER feature_artifact_integration_conflicts_no_update;
             DROP TRIGGER feature_artifact_integration_conflicts_no_delete;
             DROP TABLE feature_artifact_integration_conflicts;
             DROP TRIGGER feature_artifact_integration_artifacts_no_update;
             DROP TRIGGER feature_artifact_integration_artifacts_no_delete;
             DROP TABLE feature_artifact_integration_artifacts;
             DROP TRIGGER feature_artifact_integrations_no_update;
             DROP TRIGGER feature_artifact_integrations_no_delete;
             DROP TABLE feature_artifact_integrations;
             DROP TRIGGER feature_result_artifacts_no_update;
             DROP TRIGGER feature_result_artifacts_no_delete;
             DROP TABLE feature_result_artifacts;
             DROP TRIGGER feature_coding_dispatches_no_update;
             DROP TRIGGER feature_coding_dispatches_no_delete;
             DROP TABLE feature_coding_dispatches;
             PRAGMA user_version = 9;",
        )
        .unwrap();
    assert!(matches!(
        MasterKernel::open(&database),
        Err(MasterError::MigrationBackup(_))
    ));
    let process = MasterProcess::acquire(directory.path()).unwrap();
    assert_eq!(
        process.kernel().schema_version().unwrap(),
        MASTER_SCHEMA_VERSION
    );
    let backup = process.migration_backup_path().unwrap();
    assert!(backup.exists());
    let backup_connection = Connection::open(backup).unwrap();
    assert_eq!(
        backup_connection
            .query_row("PRAGMA user_version", [], |row| row.get::<_, i64>(0))
            .unwrap(),
        9
    );
    assert_eq!(
        Connection::open(&database)
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name = 'feature_coding_dispatches'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap(),
        1
    );
}

#[test]
fn forward_schema_version_fails_closed_without_backup() {
    let directory = tempdir().unwrap();
    let database = directory.path().join("master.sqlite3");
    drop(MasterKernel::open(&database).unwrap());
    let forward_schema = MASTER_SCHEMA_VERSION + 1;
    Connection::open(&database)
        .unwrap()
        .pragma_update(None, "user_version", forward_schema)
        .unwrap();
    let forward_error = match MasterProcess::acquire(directory.path()) {
        Ok(_) => panic!("forward schema unexpectedly opened"),
        Err(error) => error,
    };
    match forward_error {
        MasterError::UnsupportedSchemaVersion { expected, found } => {
            assert_eq!(expected, MASTER_SCHEMA_VERSION);
            assert_eq!(found, forward_schema);
        }
        error => panic!("unexpected forward-schema error: {error}"),
    }
    assert!(!directory
        .path()
        .read_dir()
        .unwrap()
        .filter_map(Result::ok)
        .any(|entry| entry
            .file_name()
            .to_string_lossy()
            .starts_with("master.pre-v17.")));
}
