use assemblywright_master::{
    ApprovedFeatureSpecification, DeviceRegistration, FeatureGrantRevisions,
    FeatureSnapshotClaimPlan, MasterKernel, MasterProcess, RemoteWorkContract, RepositoryGrantKind,
    RepositoryGrantRevision, RepositorySnapshotEvidence, RepositorySnapshotStore,
};
use assemblywright_protocol::{
    local_coding_admission_sha256, CapabilityDescriptor, DeviceId, DeviceRole,
    FeatureConveyorArtifactIntegrationRequest, FeatureConveyorCodingDispatchRequest,
    FeatureConveyorCodingWorkPacketMetadata, FeatureConveyorGrantRevisions, HandshakeRequest,
    JobEnvelope, JobResultEnvelope, JobResultStatus, LocalCodingJobResult,
    LocalCodingResultArtifact, LocalCodingResultArtifactAdmission, LOCAL_CODING_COMPLETED_STATUS,
    LOCAL_CODING_FIXTURE_TEST_STATUS, PROTOCOL_VERSION,
};
use git2::{Repository, Signature, Time};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use tempfile::tempdir;
use uuid::Uuid;

fn digest(label: &str) -> [u8; 32] {
    Sha256::digest(label.as_bytes()).into()
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn unused_addr() -> SocketAddr {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
}
fn http(endpoint: SocketAddr, method: &str, path: &str, token: &str, body: &str) -> String {
    let mut stream = TcpStream::connect(endpoint).unwrap();
    write!(stream,"{method} {path} HTTP/1.1\r\nHost: {endpoint}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    response
}
fn body(response: &str) -> serde_json::Value {
    serde_json::from_str(response.split("\r\n\r\n").nth(1).unwrap()).unwrap()
}

fn install_grants(kernel: &mut MasterKernel, repository_id: Uuid) {
    for kind in [
        RepositoryGrantKind::Registration,
        RepositoryGrantKind::CloudDisclosure,
        RepositoryGrantKind::AutonomousPublication,
    ] {
        kernel
            .record_repository_grant_revision(
                &RepositoryGrantRevision {
                    repository_id,
                    kind,
                    revision: 1,
                    scope_sha256: digest("scope"),
                    owner_approval_sha256: digest("approval"),
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

fn specification(feature_id: Uuid, repository_id: Uuid) -> ApprovedFeatureSpecification {
    let manifest = json!({"feature":"integration-e2e"});
    ApprovedFeatureSpecification {
        feature_id,
        revision: 1,
        repository_id,
        manifest_sha256: Sha256::digest(serde_json::to_vec(&manifest).unwrap()).into(),
        manifest,
        design_sha256: digest("design"),
        brainstorming_sha256: digest("brainstorming"),
        owner_approval_sha256: digest("owner"),
        grants: FeatureGrantRevisions {
            registration: 1,
            cloud_disclosure: 1,
            autonomous_publication: 1,
        },
        provider_id: "local".to_string(),
        model_id: "contained".to_string(),
        dependencies: vec![],
    }
}

fn coding_result(job: &JobEnvelope) -> (JobResultEnvelope, LocalCodingResultArtifact) {
    let context = job.validate_local_coding().unwrap();
    let bytes =
        assemblywright_protocol::build_local_coding_patch_artifact(&context.work_packet).unwrap();
    let artifact = LocalCodingResultArtifact::from_bytes(Uuid::new_v4(), &bytes).unwrap();
    let paths = assemblywright_protocol::local_coding_fixture_allowed_paths_sha256();
    let payload = serde_json::to_value(LocalCodingJobResult {
        status: LOCAL_CODING_COMPLETED_STATUS.to_string(),
        work_packet_sha256: context.work_packet_sha256,
        admission_sha256: local_coding_admission_sha256(job),
        snapshot_sha256: context.snapshot_sha256,
        allowed_paths_sha256: paths,
        changed_paths_sha256: paths,
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
    (
        JobResultEnvelope {
            protocol_version: PROTOCOL_VERSION,
            connection_epoch: job.connection_epoch,
            sequence: job.sequence + 1,
            task_id: job.task_id,
            step_id: job.step_id,
            attempt_id: job.attempt_id,
            lease_id: job.lease_id,
            cancellation_id: job.cancellation_id,
            context_sha256: job.context_sha256,
            status: JobResultStatus::Completed,
            payload_sha256: Sha256::digest(serde_json::to_vec(&payload).unwrap()).into(),
            payload,
        },
        artifact,
    )
}

#[test]
fn native_master_process_freezes_and_recovers_exact_candidate_without_source_mutation() {
    let directory = tempdir().unwrap();
    let source_path = directory.path().join("source");
    let source = Repository::init(&source_path).unwrap();
    fs::write(source_path.join("README.md"), b"before\n").unwrap();
    let mut index = source.index().unwrap();
    index.add_path(Path::new("README.md")).unwrap();
    let tree_id = index.write_tree().unwrap();
    let tree = source.find_tree(tree_id).unwrap();
    let signature = Signature::new("fixture", "fixture@example.invalid", &Time::new(1, 0)).unwrap();
    let base = source
        .commit(Some("HEAD"), &signature, &signature, "base", &tree, &[])
        .unwrap()
        .to_string();
    drop(tree);
    drop(source);
    let source_git = fs::read(source_path.join("README.md")).unwrap();
    let master_dir = directory.path().join("master");
    let endpoint = unused_addr();
    let binary = env!("CARGO_BIN_EXE_assemblywright-master");
    assert!(Command::new(binary)
        .arg("--data-dir")
        .arg(&master_dir)
        .arg("setup")
        .status()
        .unwrap()
        .success());
    let child = Command::new(binary)
        .arg("--data-dir")
        .arg(&master_dir)
        .arg("serve")
        .arg("--bind")
        .arg(endpoint.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut server = ChildGuard(child);
    let mut ready = String::new();
    BufReader::new(server.0.stdout.take().unwrap())
        .read_line(&mut ready)
        .unwrap();
    assert!(!ready.is_empty());
    let mut kernel = MasterKernel::open(master_dir.join("master.sqlite3")).unwrap();
    let snapshot = RepositorySnapshotStore::open(&master_dir)
        .unwrap()
        .prepare(&source_path, &base)
        .unwrap();
    let repository_id = Uuid::new_v4();
    install_grants(&mut kernel, repository_id);
    let feature = specification(Uuid::new_v4(), repository_id);
    kernel.enqueue_approved_feature(&feature, 0, 10).unwrap();
    let plan = FeatureSnapshotClaimPlan {
        feature_id: feature.feature_id,
        specification_revision: 1,
        repository_id,
        expected_queue_revision: 1,
        expected_emergency_pause_revision: 0,
        scope_sha256: digest("scope"),
        provider_id: "local".to_string(),
        model_id: "contained".to_string(),
        grants: feature.grants,
        base_commit: base.clone(),
    };
    kernel.prepare_repository_snapshot_claim(&plan, 11).unwrap();
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
    let device = DeviceRegistration {
        device_id: DeviceId::new(Uuid::new_v4()),
        device_name: "integration-e2e".to_string(),
        role: DeviceRole::InferenceWorker,
        registry_revision: 1,
        capabilities: vec![CapabilityDescriptor::local_coding()],
    };
    kernel.register_device(&device).unwrap();
    let packet = FeatureConveyorCodingWorkPacketMetadata::fixture(
        Uuid::new_v4(),
        Sha256::digest(b"before\n").into(),
    );
    kernel
        .dispatch_feature_coding(
            &FeatureConveyorCodingDispatchRequest {
                schema_version: 1,
                feature_id: claim.feature_id,
                specification_revision: 1,
                expected_lifecycle_revision: claim.lifecycle_revision,
                feature_lease_id: claim.lease_id,
                snapshot_id: claim.snapshot_id,
                snapshot_sha256: claim.snapshot_sha256,
                work_packet_sha256: packet.canonical_sha256().unwrap(),
                work_packet: packet,
                device_id: device.device_id,
                device_registry_revision: 1,
                expected_queue_revision: 2,
                expected_emergency_pause_revision: 0,
            },
            12,
        )
        .unwrap();
    assert!(kernel
        .artifact_integration_plan(feature.feature_id, 13)
        .is_err());
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
    let (result, artifact) = coding_result(&job);
    let context = job.validate_local_coding().unwrap();
    let admission = LocalCodingResultArtifactAdmission {
        protocol_version: PROTOCOL_VERSION,
        connection_epoch: epoch,
        sequence: result.sequence,
        task_id: job.task_id,
        step_id: job.step_id,
        attempt_id: job.attempt_id,
        lease_id: job.lease_id,
        cancellation_id: job.cancellation_id,
        context_sha256: job.context_sha256,
        feature_id: claim.feature_id,
        feature_lease_id: claim.lease_id,
        snapshot_id: claim.snapshot_id,
        snapshot_sha256: claim.snapshot_sha256,
        work_packet_sha256: context.work_packet_sha256,
        workspace_retained: true,
        workspace_expires_at_ms: 3_000_000,
        artifact,
    };
    kernel
        .finalize_local_coding_result_artifact(device.device_id, &admission, 15)
        .unwrap();
    let artifact_store = assemblywright_master::ResultArtifactStore::open(&master_dir).unwrap();
    let bytes = admission.artifact.validate().unwrap();
    let mut prepared_artifact = artifact_store
        .prepare(
            admission.artifact.artifact_id,
            admission.artifact.artifact_sha256,
            &bytes,
        )
        .unwrap();
    prepared_artifact.mark_committed().unwrap();
    kernel
        .accept_remote_result_from_with_artifact(
            device.device_id,
            &result,
            16,
            &contract,
            &artifact_store,
            prepared_artifact.verified_mut(),
        )
        .unwrap();
    kernel.set_emergency_paused_at(true, 17).unwrap();
    assert!(kernel
        .artifact_integration_plan(feature.feature_id, 17)
        .is_err());
    kernel.set_emergency_paused_at(false, 18).unwrap();
    let observed = kernel
        .artifact_integration_plan(feature.feature_id, 19)
        .unwrap();
    let request = FeatureConveyorArtifactIntegrationRequest {
        schema_version: 1,
        integration_id: Uuid::new_v4(),
        feature_id: observed.feature_id,
        specification_revision: observed.specification_revision,
        expected_lifecycle_revision: observed.lifecycle_revision,
        feature_lease_id: observed.feature_lease_id,
        snapshot_id: observed.snapshot_id,
        snapshot_sha256: observed.snapshot_sha256,
        artifact_ids: observed.artifact_ids,
        expected_queue_revision: observed.queue_revision,
        expected_emergency_pause_revision: observed.emergency_pause_revision,
        grants: FeatureConveyorGrantRevisions {
            registration: 1,
            cloud_disclosure: 1,
            autonomous_publication: 1,
        },
        base_commit: observed.base_commit,
    };
    drop(prepared_artifact);
    snapshot.retain();
    drop(kernel);
    let token = fs::read_to_string(directory.path().join("master/development.token")).unwrap();
    let plan_http = http(
        endpoint,
        "GET",
        &format!(
            "/v1/feature-conveyor/features/{}/integration-plan",
            feature.feature_id
        ),
        token.trim(),
        "",
    );
    assert!(plan_http.starts_with("HTTP/1.1 200 OK"), "{plan_http}");
    let http_plan: assemblywright_protocol::FeatureConveyorArtifactIntegrationPlan =
        serde_json::from_value(body(&plan_http)).unwrap();
    assert_eq!(http_plan.artifact_ids, request.artifact_ids);
    let integration_http = http(
        endpoint,
        "POST",
        "/v1/feature-conveyor/artifact-integrations",
        token.trim(),
        &serde_json::to_string(&request).unwrap(),
    );
    assert!(
        integration_http.starts_with("HTTP/1.1 200 OK"),
        "{integration_http}"
    );
    let receipt: assemblywright_protocol::FeatureConveyorArtifactIntegrationReceipt =
        serde_json::from_value(body(&integration_http)).unwrap();
    let retry = http(
        endpoint,
        "POST",
        "/v1/feature-conveyor/artifact-integrations",
        token.trim(),
        &serde_json::to_string(&request).unwrap(),
    );
    assert_eq!(body(&retry), serde_json::to_value(&receipt).unwrap());
    let artifact_path = master_dir
        .join("feature-result-artifacts")
        .join(request.artifact_ids[0].to_string())
        .join("artifact.patch");
    let original_artifact = fs::read(&artifact_path).unwrap();
    fs::write(&artifact_path, b"tampered-artifact\n").unwrap();
    let artifact_tampered_retry = http(
        endpoint,
        "POST",
        "/v1/feature-conveyor/artifact-integrations",
        token.trim(),
        &serde_json::to_string(&request).unwrap(),
    );
    assert!(artifact_tampered_retry.starts_with("HTTP/1.1 409 Conflict"));
    assert_eq!(
        body(&artifact_tampered_retry),
        serde_json::json!({"error":"artifact_integration_rejected"})
    );
    fs::write(&artifact_path, original_artifact).unwrap();
    let candidate_path = master_dir
        .join("feature-conveyor-candidates/candidates")
        .join(request.integration_id.to_string());
    fs::write(candidate_path.join("README.md"), b"retry-tamper\n").unwrap();
    let tampered_retry = http(
        endpoint,
        "POST",
        "/v1/feature-conveyor/artifact-integrations",
        token.trim(),
        &serde_json::to_string(&request).unwrap(),
    );
    assert!(tampered_retry.starts_with("HTTP/1.1 409 Conflict"));
    assert_eq!(
        body(&tampered_retry),
        serde_json::json!({"error":"artifact_integration_rejected"})
    );
    fs::write(
        candidate_path.join("README.md"),
        assemblywright_protocol::LOCAL_CODING_FIXTURE_CONTENT,
    )
    .unwrap();
    let post_success_plan = http(
        endpoint,
        "GET",
        &format!(
            "/v1/feature-conveyor/features/{}/integration-plan",
            feature.feature_id
        ),
        token.trim(),
        "",
    );
    assert!(post_success_plan.starts_with("HTTP/1.1 409 Conflict"));
    assert!(http(
        endpoint,
        "POST",
        "/v1/distributed/feature-conveyor/artifact-integrations",
        token.trim(),
        "{}"
    )
    .starts_with("HTTP/1.1 404 Not Found"));
    drop(server);
    let repository = Repository::open(&candidate_path).unwrap();
    assert_eq!(
        repository.head().unwrap().target().unwrap().to_string(),
        receipt.candidate_commit
    );
    let fsck = Command::new("git")
        .arg("-C")
        .arg(&candidate_path)
        .args(["fsck", "--no-dangling"])
        .output()
        .unwrap();
    assert!(
        fsck.status.success(),
        "{}",
        String::from_utf8_lossy(&fsck.stderr)
    );
    assert_eq!(
        repository
            .find_commit(repository.head().unwrap().target().unwrap())
            .unwrap()
            .tree_id()
            .to_string(),
        receipt.candidate_tree
    );
    assert_eq!(
        fs::read(candidate_path.join("README.md")).unwrap(),
        assemblywright_protocol::LOCAL_CODING_FIXTURE_CONTENT
    );
    assert_eq!(fs::read(source_path.join("README.md")).unwrap(), source_git);
    let reopened = MasterProcess::acquire(&master_dir).unwrap();
    assert_eq!(reopened.kernel().feature_startup_quarantines(), 1);
    assert_eq!(fs::read(source_path.join("README.md")).unwrap(), source_git);
}
