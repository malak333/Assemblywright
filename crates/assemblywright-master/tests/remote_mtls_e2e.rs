#![cfg(windows)]

use assemblywright_master::{
    current_time_ms, EnrollmentGrantSpec, EnrollmentRequest, IdentityAuthority, MasterProcess,
    NewStep, PlatformSecretProtector, RepositoryGrantKind, RepositoryGrantRevision,
};
use assemblywright_protocol::{
    AuthenticatedHandshakeRequest, CapabilityDescriptor, DeviceRole, DistributedEventBatch,
    DistributedEventBatchRequest, DistributedEventKind, FeatureConveyorApprovedFeatureRequest,
    FeatureConveyorApprovedSpecification, FeatureConveyorGrantRevisions, HandshakeRequest,
    HandshakeResponse, HandshakeStatus, JobEnvelope, JobResultEnvelope, JobResultStatus,
    Sensitivity, StepId, TaskId, FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION, PROTOCOL_VERSION,
};
use rcgen::{CertificateParams, DistinguishedName, DnType, IsCa, KeyPair};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName};
use rustls::{ClientConfig, RootCertStore};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_rustls::TlsConnector;
use uuid::Uuid;

const TLS_EXPORTER_LABEL: &[u8] = b"EXPORTER-Assemblywright-Developer-Mode-v1";

struct ChildGuard {
    child: Child,
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct EnrolledClient {
    config: Arc<ClientConfig>,
    tls12_config: Arc<ClientConfig>,
    handshake: HandshakeRequest,
    ca_certificate: CertificateDer<'static>,
}

#[tokio::test(flavor = "multi_thread")]
async fn remote_listener_requires_enrollment_tls13_and_channel_bound_identity() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = tempfile::tempdir().expect("remote mTLS data directory");
    let binary = env!("CARGO_BIN_EXE_assemblywright-master");
    let setup = Command::new(binary)
        .arg("--data-dir")
        .arg(directory.path())
        .arg("setup")
        .output()
        .expect("run setup");
    assert_success(&setup, "setup");

    let valid = enroll_client(
        directory.path(),
        "owner-mac-bridge",
        DeviceRole::MacBridge,
        false,
    );
    let owner_control = enroll_client_with_capabilities(
        directory.path(),
        "designated-owner-control",
        DeviceRole::MacBridge,
        false,
        vec![CapabilityDescriptor::mlx_reasoning(
            "owner-control-mlx",
            32 * 1024,
            32 * 1024,
        )],
    );
    let non_designated_bridge = enroll_client_with_capabilities(
        directory.path(),
        "non-designated-owner-control",
        DeviceRole::MacBridge,
        false,
        vec![CapabilityDescriptor::mlx_reasoning(
            "owner-control-mlx",
            32 * 1024,
            32 * 1024,
        )],
    );
    let inference_worker = enroll_client(
        directory.path(),
        "inference-worker",
        DeviceRole::InferenceWorker,
        false,
    );
    let revoked = enroll_client(
        directory.path(),
        "revoked-worker",
        DeviceRole::InferenceWorker,
        true,
    );
    let repository_id = Uuid::new_v4();
    let approved = approved_feature_request(repository_id);
    {
        let mut process = MasterProcess::acquire(directory.path()).expect("seed owner control");
        install_repository_grants(process.kernel_mut(), repository_id);
        process
            .kernel_mut()
            .designate_owner_control_bridge(owner_control.handshake.device_id, 0, 10)
            .expect("designate exact owner-control bridge");
    }
    let local_endpoint = unused_loopback_addr();
    let remote_endpoint = unused_loopback_addr();
    let mut server = spawn_server(binary, directory.path(), local_endpoint, remote_endpoint);
    let ready = read_ready(&mut server.child);
    assert_eq!(ready["endpoint"], local_endpoint.to_string());
    assert_eq!(ready["remote_endpoint"], remote_endpoint.to_string());
    assert_eq!(
        ready["boundary"],
        "authenticated_loopback_plus_tls13_mtls_enrolled_devices"
    );

    let mut anonymous_roots = RootCertStore::empty();
    anonymous_roots
        .add(valid.ca_certificate.clone())
        .expect("trust enrollment CA for anonymous-client denial");
    let anonymous_config = Arc::new(
        ClientConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
            .with_root_certificates(anonymous_roots)
            .with_no_client_auth(),
    );
    let anonymous_result = try_tls_request(
        remote_endpoint,
        anonymous_config,
        "GET",
        "/health",
        None::<&Value>,
    )
    .await;
    assert!(
        anonymous_result
            .map(|response| !response.0.starts_with("HTTP/1.1 200"))
            .unwrap_or(true),
        "client without an enrolled certificate reached remote health"
    );
    let tls12_result = try_tls_request(
        remote_endpoint,
        valid.tls12_config.clone(),
        "GET",
        "/health",
        None::<&Value>,
    )
    .await;
    assert!(
        tls12_result.is_err(),
        "remote listener negotiated a protocol below TLS 1.3"
    );

    let (pre_handshake_health, _) = tls_request(
        remote_endpoint,
        valid.config.clone(),
        "GET",
        "/health",
        None::<&Value>,
    )
    .await;
    assert!(
        pre_handshake_health.starts_with("HTTP/1.1 401 Unauthorized"),
        "{pre_handshake_health}"
    );
    let (pre_handshake_status, _) = tls_request(
        remote_endpoint,
        valid.config.clone(),
        "GET",
        "/v1/distributed/feature-conveyor/status",
        None::<&Value>,
    )
    .await;
    assert!(
        pre_handshake_status.starts_with("HTTP/1.1 401 Unauthorized"),
        "pre-handshake client reached Feature Conveyor status: {pre_handshake_status}"
    );
    let (pre_handshake_enqueue, _) = tls_request(
        remote_endpoint,
        owner_control.config.clone(),
        "POST",
        "/v1/distributed/feature-conveyor/approved-features",
        Some(&approved),
    )
    .await;
    assert!(
        pre_handshake_enqueue.starts_with("HTTP/1.1 401 Unauthorized"),
        "pre-handshake client reached owner-control enqueue: {pre_handshake_enqueue}"
    );

    let (health_handshake, health) = authenticated_application_request(
        remote_endpoint,
        &non_designated_bridge,
        "GET",
        "/health",
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(health_handshake.status, HandshakeStatus::Accepted);
    assert!(health.starts_with("HTTP/1.1 200 OK"), "{health}");
    assert!(health.contains("developer_remote_master"), "{health}");
    tokio::time::sleep(Duration::from_millis(50)).await;

    let (status_handshake, remote_feature_status) = authenticated_application_request(
        remote_endpoint,
        &valid,
        "GET",
        "/v1/distributed/feature-conveyor/status",
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status_handshake.status, HandshakeStatus::Accepted);
    assert!(
        remote_feature_status.starts_with("HTTP/1.1 200 OK"),
        "authenticated MacBridge could not observe Feature Conveyor status: {remote_feature_status}"
    );
    let remote_feature_status: Value = response_json(&remote_feature_status);
    assert_exact_object_keys(
        &remote_feature_status,
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
        &remote_feature_status["counts_by_status"],
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
    assert_exact_object_keys(
        &remote_feature_status["owner_guidance"],
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
    assert_eq!(remote_feature_status["schema_version"], 8);
    assert_eq!(remote_feature_status["visible_feature_count"], 0);
    assert_eq!(remote_feature_status["features"], serde_json::json!([]));
    let redacted = serde_json::to_string(&remote_feature_status).expect("serialize status");
    for forbidden in [
        "repository_id",
        "provider_id",
        "model_id",
        "manifest",
        "grant",
        "audit",
        "evidence",
        "lease_id",
        "owner_token",
    ] {
        assert!(
            !redacted.contains(forbidden),
            "remote status leaked forbidden field {forbidden}: {redacted}"
        );
    }
    tokio::time::sleep(Duration::from_millis(50)).await;

    let (local_status_handshake, local_status_over_remote) = authenticated_application_request(
        remote_endpoint,
        &valid,
        "GET",
        "/v1/feature-conveyor/status",
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(local_status_handshake.status, HandshakeStatus::Accepted);
    assert!(
        local_status_over_remote.starts_with("HTTP/1.1 404 Not Found"),
        "owner-token local status route leaked onto the remote router: {local_status_over_remote}"
    );
    tokio::time::sleep(Duration::from_millis(50)).await;

    let (local_grants_handshake, local_grants_over_remote) = authenticated_application_request(
        remote_endpoint,
        &valid,
        "GET",
        &format!("/v1/feature-conveyor/repositories/{repository_id}/grants"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(local_grants_handshake.status, HandshakeStatus::Accepted);
    assert!(
        local_grants_over_remote.starts_with("HTTP/1.1 404 Not Found"),
        "owner-token repository-grant status leaked onto the remote router: {local_grants_over_remote}"
    );
    let (local_grant_post_handshake, local_grant_post_over_remote) =
        authenticated_application_request(
            remote_endpoint,
            &valid,
            "POST",
            "/v1/feature-conveyor/repository-grants",
            &serde_json::json!({}),
        )
        .await;
    assert_eq!(local_grant_post_handshake.status, HandshakeStatus::Accepted);
    assert!(
        local_grant_post_over_remote.starts_with("HTTP/1.1 404 Not Found"),
        "owner-token repository-grant mutation leaked onto the remote router: {local_grant_post_over_remote}"
    );
    let (local_preflight_handshake, local_preflight_over_remote) =
        authenticated_application_request(
            remote_endpoint,
            &valid,
            "POST",
            "/v1/feature-conveyor/repository-preflight",
            &serde_json::json!({}),
        )
        .await;
    assert_eq!(local_preflight_handshake.status, HandshakeStatus::Accepted);
    assert!(
        local_preflight_over_remote.starts_with("HTTP/1.1 404 Not Found"),
        "owner-token repository preflight leaked onto the remote router: {local_preflight_over_remote}"
    );
    tokio::time::sleep(Duration::from_millis(50)).await;

    let (first_response, first_exporter) = tls_request_with_body(
        remote_endpoint,
        valid.config.clone(),
        "POST",
        "/v1/distributed/connections/accept",
        |tls_exporter_sha256| AuthenticatedHandshakeRequest {
            handshake: valid.handshake.clone(),
            tls_exporter_sha256,
        },
    )
    .await;
    assert!(
        first_response.starts_with("HTTP/1.1 200 OK"),
        "{first_response}"
    );
    let first: HandshakeResponse = response_json(&first_response);
    assert_eq!(first.status, HandshakeStatus::Accepted);
    assert!(first_exporter.iter().any(|byte| *byte != 0));

    tokio::time::sleep(Duration::from_millis(50)).await;
    let replay = AuthenticatedHandshakeRequest {
        handshake: valid.handshake.clone(),
        tls_exporter_sha256: first_exporter,
    };
    let (replay_response, second_exporter) = tls_request(
        remote_endpoint,
        valid.config.clone(),
        "POST",
        "/v1/distributed/connections/accept",
        Some(&replay),
    )
    .await;
    assert_ne!(first_exporter, second_exporter);
    assert!(
        replay_response.starts_with("HTTP/1.1 401 Unauthorized"),
        "{replay_response}"
    );

    let (second_response, _) = tls_request_with_body(
        remote_endpoint,
        valid.config.clone(),
        "POST",
        "/v1/distributed/connections/accept",
        |tls_exporter_sha256| AuthenticatedHandshakeRequest {
            handshake: valid.handshake.clone(),
            tls_exporter_sha256,
        },
    )
    .await;
    let second: HandshakeResponse = response_json(&second_response);
    assert_eq!(second.status, HandshakeStatus::Accepted);
    assert!(second.connection_epoch > first.connection_epoch);

    let (bridge_handshake, bridge_enqueue) = authenticated_application_request(
        remote_endpoint,
        &valid,
        "POST",
        "/v1/distributed/steps",
        &test_step("bridge may enqueue"),
    )
    .await;
    assert_eq!(bridge_handshake.status, HandshakeStatus::Accepted);
    assert!(
        bridge_enqueue.starts_with("HTTP/1.1 404 Not Found"),
        "remote raw step enqueue remained available: {bridge_enqueue}"
    );

    let (_, non_designated_enqueue) = authenticated_application_request(
        remote_endpoint,
        &non_designated_bridge,
        "POST",
        "/v1/distributed/feature-conveyor/approved-features",
        &approved,
    )
    .await;
    assert_fixed_error(
        &non_designated_enqueue,
        "HTTP/1.1 409 Conflict",
        "approved_feature_enqueue_rejected",
    );
    let (_, wrong_role_enqueue) = authenticated_application_request(
        remote_endpoint,
        &inference_worker,
        "POST",
        "/v1/distributed/feature-conveyor/approved-features",
        &approved,
    )
    .await;
    assert_fixed_error(
        &wrong_role_enqueue,
        "HTTP/1.1 401 Unauthorized",
        "unauthorized",
    );

    let mut stale_designation = approved.clone();
    stale_designation.owner_control_designation_revision = 2;
    let (_, stale_designation_response) = authenticated_application_request(
        remote_endpoint,
        &owner_control,
        "POST",
        "/v1/distributed/feature-conveyor/approved-features",
        &stale_designation,
    )
    .await;
    assert_fixed_error(
        &stale_designation_response,
        "HTTP/1.1 409 Conflict",
        "approved_feature_enqueue_rejected",
    );

    let mut unknown_dependency = approved.clone();
    unknown_dependency.specification.dependencies = vec![Uuid::new_v4()];
    let (_, unknown_response) = authenticated_application_request(
        remote_endpoint,
        &owner_control,
        "POST",
        "/v1/distributed/feature-conveyor/approved-features",
        &unknown_dependency,
    )
    .await;
    assert_fixed_error(
        &unknown_response,
        "HTTP/1.1 409 Conflict",
        "approved_feature_enqueue_rejected",
    );

    let mut malformed = serde_json::to_value(&approved).unwrap();
    malformed["secret_error_detail"] = serde_json::json!("must-never-echo");
    let (_, malformed_response) = authenticated_application_request(
        remote_endpoint,
        &owner_control,
        "POST",
        "/v1/distributed/feature-conveyor/approved-features",
        &malformed,
    )
    .await;
    assert_fixed_error(
        &malformed_response,
        "HTTP/1.1 422 Unprocessable Entity",
        "approved_feature_request_rejected",
    );
    assert!(!malformed_response.contains("must-never-echo"));

    let mut oversized = serde_json::to_value(&approved).unwrap();
    oversized["specification"]["manifest"] =
        serde_json::json!({"private_content": "x".repeat(256 * 1024 + 1)});
    let (_, oversized_response) = authenticated_application_request(
        remote_endpoint,
        &owner_control,
        "POST",
        "/v1/distributed/feature-conveyor/approved-features",
        &oversized,
    )
    .await;
    assert_fixed_error(
        &oversized_response,
        "HTTP/1.1 422 Unprocessable Entity",
        "approved_feature_request_rejected",
    );
    assert!(!oversized_response.contains("private_content"));

    let development_token = std::fs::read_to_string(directory.path().join("development.token"))
        .expect("read owner token");
    let pause = local_post(
        local_endpoint,
        "/v1/development/emergency-pause/activate",
        development_token.trim(),
        "{}",
    );
    assert!(pause.starts_with("HTTP/1.1 200 OK"), "{pause}");
    let mut paused_request = approved.clone();
    paused_request.emergency_pause_revision = 1;
    let (_, paused_response) = authenticated_application_request(
        remote_endpoint,
        &owner_control,
        "POST",
        "/v1/distributed/feature-conveyor/approved-features",
        &paused_request,
    )
    .await;
    assert_fixed_error(
        &paused_response,
        "HTTP/1.1 409 Conflict",
        "approved_feature_enqueue_rejected",
    );
    let resume = local_post(
        local_endpoint,
        "/v1/development/emergency-pause/resume",
        development_token.trim(),
        "{}",
    );
    assert!(resume.starts_with("HTTP/1.1 200 OK"), "{resume}");
    let (_, stale_pause_response) = authenticated_application_request(
        remote_endpoint,
        &owner_control,
        "POST",
        "/v1/distributed/feature-conveyor/approved-features",
        &paused_request,
    )
    .await;
    assert_fixed_error(
        &stale_pause_response,
        "HTTP/1.1 409 Conflict",
        "approved_feature_enqueue_rejected",
    );

    let mut exact = approved.clone();
    exact.emergency_pause_revision = 2;
    let (_, enqueue_response) = authenticated_application_request(
        remote_endpoint,
        &owner_control,
        "POST",
        "/v1/distributed/feature-conveyor/approved-features",
        &exact,
    )
    .await;
    assert!(
        enqueue_response.starts_with("HTTP/1.1 200 OK"),
        "{enqueue_response}"
    );
    let receipt: Value = response_json(&enqueue_response);
    assert_exact_object_keys(
        &receipt,
        &[
            "schema_version",
            "feature_id",
            "specification_revision",
            "lifecycle_revision",
            "queue_revision",
            "owner_control_designation_revision",
            "emergency_pause_revision",
            "status",
        ],
    );
    assert_eq!(receipt["schema_version"], 1);
    assert_eq!(receipt["queue_revision"], 1);
    assert_eq!(receipt["owner_control_designation_revision"], 1);
    assert_eq!(receipt["emergency_pause_revision"], 2);
    assert_eq!(receipt["status"], "queued");
    let receipt_json = serde_json::to_string(&receipt).unwrap();
    for forbidden in [
        "repository_id",
        "manifest",
        "provider_id",
        "model_id",
        "grant",
        "approval",
        "audit",
    ] {
        assert!(
            !receipt_json.contains(forbidden),
            "receipt leaked {forbidden}"
        );
    }

    let mut duplicate = exact.clone();
    duplicate.expected_queue_revision = 1;
    let (_, duplicate_response) = authenticated_application_request(
        remote_endpoint,
        &owner_control,
        "POST",
        "/v1/distributed/feature-conveyor/approved-features",
        &duplicate,
    )
    .await;
    assert_fixed_error(
        &duplicate_response,
        "HTTP/1.1 409 Conflict",
        "approved_feature_enqueue_rejected",
    );
    assert!(!duplicate_response.contains(&repository_id.to_string()));

    let (pause_handshake, remote_pause) = authenticated_application_request(
        remote_endpoint,
        &valid,
        "POST",
        "/v1/development/emergency-pause/activate",
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(pause_handshake.status, HandshakeStatus::Accepted);
    assert!(
        remote_pause.starts_with("HTTP/1.1 404 Not Found"),
        "owner-local emergency pause control leaked onto the enrolled-device router: {remote_pause}"
    );
    let (events_handshake, events_response) = authenticated_application_request_with_body(
        remote_endpoint,
        &valid,
        "POST",
        "/v1/distributed/events/next",
        |handshake| DistributedEventBatchRequest {
            protocol_version: PROTOCOL_VERSION,
            connection_epoch: handshake.connection_epoch,
            after: None,
            limit: 64,
        },
    )
    .await;
    assert_eq!(events_handshake.status, HandshakeStatus::Accepted);
    assert!(
        events_response.starts_with("HTTP/1.1 200 OK"),
        "{events_response}"
    );
    let events: DistributedEventBatch = response_json(&events_response);
    assert!(events
        .events
        .iter()
        .any(|event| event.kind == DistributedEventKind::DeviceConnected));
    assert!(
        !events_response.contains("bridge may enqueue"),
        "metadata event stream leaked step context"
    );

    let (worker_handshake, worker_enqueue) = authenticated_application_request(
        remote_endpoint,
        &inference_worker,
        "POST",
        "/v1/distributed/steps",
        &test_step("worker must not enqueue"),
    )
    .await;
    assert_eq!(worker_handshake.status, HandshakeStatus::Accepted);
    assert!(
        worker_enqueue.starts_with("HTTP/1.1 404 Not Found"),
        "{worker_enqueue}"
    );
    let (worker_events_handshake, worker_events) = authenticated_application_request_with_body(
        remote_endpoint,
        &inference_worker,
        "POST",
        "/v1/distributed/events/next",
        |handshake| DistributedEventBatchRequest {
            protocol_version: PROTOCOL_VERSION,
            connection_epoch: handshake.connection_epoch,
            after: None,
            limit: 64,
        },
    )
    .await;
    assert_eq!(worker_events_handshake.status, HandshakeStatus::Accepted);
    assert!(
        worker_events.starts_with("HTTP/1.1 401 Unauthorized"),
        "inference worker reached MacBridge-only event route: {worker_events}"
    );
    let (worker_status_handshake, worker_status) = authenticated_application_request(
        remote_endpoint,
        &inference_worker,
        "GET",
        "/v1/distributed/feature-conveyor/status",
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(worker_status_handshake.status, HandshakeStatus::Accepted);
    assert!(
        worker_status.starts_with("HTTP/1.1 401 Unauthorized"),
        "non-MacBridge reached Feature Conveyor status: {worker_status}"
    );

    let revoked_result = try_tls_request(
        remote_endpoint,
        revoked.config,
        "GET",
        "/health",
        None::<&Value>,
    )
    .await;
    assert!(
        revoked_result
            .map(|response| !response.0.starts_with("HTTP/1.1 200"))
            .unwrap_or(true),
        "revoked enrolled certificate reached remote health"
    );
}

fn assert_exact_object_keys(value: &Value, expected: &[&str]) {
    let object = value.as_object().expect("JSON object");
    let mut actual = object.keys().map(String::as_str).collect::<Vec<_>>();
    let mut expected = expected.to_vec();
    actual.sort_unstable();
    expected.sort_unstable();
    assert_eq!(actual, expected);
}

#[tokio::test(flavor = "multi_thread")]
async fn remote_mlx_contract_accepts_exact_singleton_and_rejects_mixed_capabilities() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let directory = tempfile::tempdir().expect("remote MLX data directory");
    let binary = env!("CARGO_BIN_EXE_assemblywright-master");
    let setup = Command::new(binary)
        .arg("--data-dir")
        .arg(directory.path())
        .arg("setup")
        .output()
        .expect("run setup");
    assert_success(&setup, "setup");
    let mlx_capability =
        CapabilityDescriptor::mlx_reasoning("local-mlx-model", 64 * 1024, 64 * 1024);
    let mlx = enroll_client_with_capabilities(
        directory.path(),
        "mlx-worker",
        DeviceRole::InferenceWorker,
        false,
        vec![mlx_capability.clone()],
    );
    let mixed = enroll_client_with_capabilities(
        directory.path(),
        "mixed-worker",
        DeviceRole::InferenceWorker,
        false,
        vec![mlx_capability, CapabilityDescriptor::fixture_reasoning()],
    );
    let context = serde_json::json!({
        "operation":"generate_text",
        "prompt":"bounded",
        "max_tokens":32,
        "temperature_milli":700
    });
    let queued = NewStep {
        task_id: TaskId::new(Uuid::new_v4()),
        step_id: StepId::new(Uuid::new_v4()),
        capability_id: "mlx.reasoning".to_string(),
        sensitivity: Sensitivity::Public,
        context,
        lease_duration_ms: 60_000,
        deadline_after_ms: 60_000,
    };
    {
        let mut process = MasterProcess::acquire(directory.path()).expect("open local master");
        process
            .kernel_mut()
            .enqueue_step(&queued, current_time_ms().expect("time"))
            .expect("Windows-local generic enqueue");
    }
    let local_endpoint = unused_loopback_addr();
    let remote_endpoint = unused_loopback_addr();
    let mut server = spawn_server(binary, directory.path(), local_endpoint, remote_endpoint);
    read_ready(&mut server.child);

    let (mixed_handshake, mixed_lease) = authenticated_application_request_with_body(
        remote_endpoint,
        &mixed,
        "POST",
        "/v1/distributed/leases/next",
        |handshake| {
            serde_json::json!({
                "device_id": mixed.handshake.device_id,
                "connection_epoch": handshake.connection_epoch
            })
        },
    )
    .await;
    assert_eq!(mixed_handshake.status, HandshakeStatus::Accepted);
    assert!(
        mixed_lease.starts_with("HTTP/1.1 401 Unauthorized"),
        "{mixed_lease}"
    );

    let mlx_tcp = tokio::net::TcpStream::connect(remote_endpoint)
        .await
        .expect("connect exact MLX remote listener");
    let server_name = ServerName::IpAddress(remote_endpoint.ip().into());
    let mut mlx_stream = TlsConnector::from(mlx.config.clone())
        .connect(server_name, mlx_tcp)
        .await
        .expect("complete exact MLX mutual TLS handshake");
    let handshake_body = serde_json::to_vec(&AuthenticatedHandshakeRequest {
        handshake: mlx.handshake.clone(),
        tls_exporter_sha256: exporter_digest(mlx_stream.get_ref().1),
    })
    .expect("serialize exact MLX handshake");
    let handshake_response = send_http_keep_alive(
        &mut mlx_stream,
        remote_endpoint,
        "POST",
        "/v1/distributed/connections/accept",
        &handshake_body,
    )
    .await
    .expect("accept exact MLX application handshake");
    assert!(
        handshake_response.starts_with("HTTP/1.1 200 OK"),
        "{handshake_response}"
    );
    let mlx_handshake: HandshakeResponse = response_json(&handshake_response);
    assert_eq!(mlx_handshake.status, HandshakeStatus::Accepted);
    let lease_body = serde_json::to_vec(&serde_json::json!({
        "device_id": mlx.handshake.device_id,
        "connection_epoch": mlx_handshake.connection_epoch
    }))
    .expect("serialize exact MLX lease request");
    let lease_response = send_http_keep_alive(
        &mut mlx_stream,
        remote_endpoint,
        "POST",
        "/v1/distributed/leases/next",
        &lease_body,
    )
    .await
    .expect("lease exact MLX work on accepted connection");
    assert!(
        lease_response.starts_with("HTTP/1.1 200 OK"),
        "{lease_response}"
    );
    let job: JobEnvelope = response_json(&lease_response);
    let payload = serde_json::json!({
        "operation":"generate_text",
        "output":"bounded output",
        "model":"local-mlx-model"
    });
    let result = JobResultEnvelope {
        protocol_version: PROTOCOL_VERSION,
        connection_epoch: job.connection_epoch,
        sequence: job.sequence + 1,
        task_id: job.task_id,
        step_id: job.step_id,
        attempt_id: job.attempt_id,
        lease_id: job.lease_id,
        cancellation_id: job.cancellation_id,
        status: JobResultStatus::Completed,
        context_sha256: job.context_sha256,
        payload_sha256: Sha256::digest(serde_json::to_vec(&payload).unwrap()).into(),
        payload,
    };
    let result_body = serde_json::to_vec(&result).expect("serialize exact MLX result");
    let result_response = send_http(
        &mut mlx_stream,
        remote_endpoint,
        "POST",
        "/v1/distributed/results",
        &result_body,
    )
    .await
    .expect("submit exact MLX result on leased connection");
    assert!(
        result_response.starts_with("HTTP/1.1 200 OK"),
        "{result_response}"
    );
}

fn approved_feature_request(repository_id: Uuid) -> FeatureConveyorApprovedFeatureRequest {
    let feature_id = Uuid::new_v4();
    let manifest = serde_json::json!({
        "allowed_paths": ["crates/assemblywright-master/src/lib.rs"],
        "feature_id": feature_id,
        "outcome": "bounded remote owner-control enqueue"
    });
    let canonical = format!(
        r#"{{"allowed_paths":["crates/assemblywright-master/src/lib.rs"],"feature_id":"{feature_id}","outcome":"bounded remote owner-control enqueue"}}"#
    );
    FeatureConveyorApprovedFeatureRequest {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        expected_queue_revision: 0,
        owner_control_designation_revision: 1,
        emergency_pause_revision: 0,
        specification: FeatureConveyorApprovedSpecification {
            feature_id,
            revision: 1,
            repository_id,
            manifest,
            manifest_sha256: Sha256::digest(canonical.as_bytes()).into(),
            design_sha256: Sha256::digest(b"remote-design").into(),
            brainstorming_sha256: Sha256::digest(b"remote-brainstorming").into(),
            owner_approval_sha256: Sha256::digest(b"remote-owner-approval").into(),
            grants: FeatureConveyorGrantRevisions {
                registration: 1,
                cloud_disclosure: 1,
                autonomous_publication: 1,
            },
            provider_id: "local.review".to_string(),
            model_id: "review-v1".to_string(),
            dependencies: vec![],
        },
    }
}

fn install_repository_grants(
    kernel: &mut assemblywright_master::MasterKernel,
    repository_id: Uuid,
) {
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
                    scope_sha256: Sha256::digest(format!("remote-scope-{index}")).into(),
                    owner_approval_sha256: Sha256::digest(format!("grant-owner-{index}")).into(),
                    expires_at_ms: None,
                    revoked: false,
                },
                0,
                0,
                1,
            )
            .expect("record repository grant");
    }
}

fn assert_fixed_error(response: &str, status: &str, error: &str) {
    assert!(response.starts_with(status), "{response}");
    let value: Value = response_json(response);
    assert_eq!(value, serde_json::json!({"error": error}));
}

fn local_post(endpoint: SocketAddr, path: &str, token: &str, body: &str) -> String {
    let mut stream = TcpStream::connect(endpoint).expect("connect local owner route");
    let request = format!(
        "POST {path} HTTP/1.1\r\nHost: {endpoint}\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream
        .write_all(request.as_bytes())
        .expect("send local owner request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read local owner response");
    response
}

fn enroll_client(data_dir: &Path, name: &str, role: DeviceRole, revoke: bool) -> EnrolledClient {
    enroll_client_with_capabilities(
        data_dir,
        name,
        role,
        revoke,
        vec![CapabilityDescriptor::fixture_reasoning()],
    )
}

fn enroll_client_with_capabilities(
    data_dir: &Path,
    name: &str,
    role: DeviceRole,
    revoke: bool,
    capabilities: Vec<CapabilityDescriptor>,
) -> EnrolledClient {
    let now_ms = current_time_ms().expect("current time");
    let protector = PlatformSecretProtector;
    let mut process = MasterProcess::acquire(data_dir).expect("acquire enrollment owner");
    let authority = IdentityAuthority::open_or_initialize(data_dir, &protector, now_ms)
        .expect("initialize enrollment authority");
    process
        .kernel_mut()
        .record_identity_authority(authority.receipt())
        .expect("bind enrollment authority");
    let grant = process
        .kernel_mut()
        .create_enrollment_grant(
            EnrollmentGrantSpec {
                device_name: name.to_string(),
                role,
                capabilities: capabilities.clone(),
            },
            now_ms,
        )
        .expect("create enrollment grant");
    let key = KeyPair::generate().expect("generate client key");
    let mut params = CertificateParams::default();
    let mut distinguished_name = DistinguishedName::new();
    distinguished_name.push(DnType::CommonName, "ignored-client-claim");
    params.distinguished_name = distinguished_name;
    params.is_ca = IsCa::NoCa;
    let csr_pem = params
        .serialize_request(&key)
        .expect("serialize signed CSR")
        .pem()
        .expect("encode CSR PEM");
    let issued = process
        .kernel_mut()
        .issue_device_certificate(
            &authority,
            &EnrollmentRequest {
                grant_id: grant.grant_id,
                grant_secret: grant.grant_secret,
                csr_pem,
            },
            now_ms,
        )
        .expect("issue client certificate");
    if revoke {
        process
            .kernel_mut()
            .revoke_device(issued.device_id, now_ms.saturating_add(1))
            .expect("revoke enrolled device");
    }

    let (_, leaf) = x509_parser::pem::parse_x509_pem(issued.certificate_pem.as_bytes())
        .expect("decode client certificate PEM");
    let (_, ca) = x509_parser::pem::parse_x509_pem(issued.ca_certificate_pem.as_bytes())
        .expect("decode CA certificate PEM");
    let ca_certificate = CertificateDer::from(ca.contents);
    let leaf_certificate = CertificateDer::from(leaf.contents);
    let private_key_der = key.serialize_der();
    let mut roots = RootCertStore::empty();
    roots
        .add(ca_certificate.clone())
        .expect("trust enrollment CA");
    let private_key = PrivateKeyDer::from(PrivatePkcs8KeyDer::from(private_key_der.clone()));
    let config = ClientConfig::builder_with_protocol_versions(&[&rustls::version::TLS13])
        .with_root_certificates(roots)
        .with_client_auth_cert(vec![leaf_certificate.clone()], private_key)
        .expect("build enrolled TLS client");
    let mut tls12_roots = RootCertStore::empty();
    tls12_roots
        .add(ca_certificate.clone())
        .expect("trust enrollment CA for TLS 1.2 denial");
    let tls12_config = ClientConfig::builder_with_protocol_versions(&[&rustls::version::TLS12])
        .with_root_certificates(tls12_roots)
        .with_client_auth_cert(
            vec![leaf_certificate],
            PrivateKeyDer::from(PrivatePkcs8KeyDer::from(private_key_der)),
        )
        .expect("build TLS 1.2 enrolled client for protocol denial");
    EnrolledClient {
        config: Arc::new(config),
        tls12_config: Arc::new(tls12_config),
        handshake: HandshakeRequest {
            protocol_version: PROTOCOL_VERSION,
            device_id: issued.device_id,
            device_name: issued.device_name,
            role: issued.role,
            registry_revision: issued.registry_revision,
            capabilities,
        },
        ca_certificate,
    }
}

fn test_step(prompt: &str) -> NewStep {
    NewStep {
        task_id: TaskId::new(Uuid::new_v4()),
        step_id: StepId::new(Uuid::new_v4()),
        capability_id: "fixture.reasoning".to_string(),
        sensitivity: Sensitivity::Public,
        context: serde_json::json!({
            "operation":"synthetic_echo",
            "input":prompt,
            "delay_ms":0
        }),
        lease_duration_ms: 60_000,
        deadline_after_ms: 300_000,
    }
}

async fn tls_request<T: Serialize + ?Sized>(
    endpoint: SocketAddr,
    config: Arc<ClientConfig>,
    method: &str,
    path: &str,
    body: Option<&T>,
) -> (String, [u8; 32]) {
    try_tls_request(endpoint, config, method, path, body)
        .await
        .expect("complete TLS request")
}

async fn try_tls_request<T: Serialize + ?Sized>(
    endpoint: SocketAddr,
    config: Arc<ClientConfig>,
    method: &str,
    path: &str,
    body: Option<&T>,
) -> Result<(String, [u8; 32]), String> {
    let body = body
        .map(serde_json::to_vec)
        .transpose()
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    tls_request_bytes(endpoint, config, method, path, body).await
}

async fn tls_request_with_body<T: Serialize>(
    endpoint: SocketAddr,
    config: Arc<ClientConfig>,
    method: &str,
    path: &str,
    body: impl FnOnce([u8; 32]) -> T,
) -> (String, [u8; 32]) {
    let stream = tokio::net::TcpStream::connect(endpoint)
        .await
        .expect("connect remote listener");
    let server_name = ServerName::IpAddress(endpoint.ip().into());
    let mut stream = TlsConnector::from(config)
        .connect(server_name, stream)
        .await
        .expect("complete mutual TLS handshake");
    let exporter = exporter_digest(stream.get_ref().1);
    let body = serde_json::to_vec(&body(exporter)).expect("serialize request body");
    let response = send_http(&mut stream, endpoint, method, path, &body)
        .await
        .expect("complete remote HTTP request");
    (response, exporter)
}

async fn authenticated_application_request<T: Serialize + ?Sized>(
    endpoint: SocketAddr,
    client: &EnrolledClient,
    method: &str,
    path: &str,
    body: &T,
) -> (HandshakeResponse, String) {
    let stream = tokio::net::TcpStream::connect(endpoint)
        .await
        .expect("connect remote listener for authenticated application request");
    let server_name = ServerName::IpAddress(endpoint.ip().into());
    let mut stream = TlsConnector::from(client.config.clone())
        .connect(server_name, stream)
        .await
        .expect("complete mutual TLS application handshake");
    let handshake_body = serde_json::to_vec(&AuthenticatedHandshakeRequest {
        handshake: client.handshake.clone(),
        tls_exporter_sha256: exporter_digest(stream.get_ref().1),
    })
    .expect("serialize authenticated handshake");
    let handshake_response = send_http_keep_alive(
        &mut stream,
        endpoint,
        "POST",
        "/v1/distributed/connections/accept",
        &handshake_body,
    )
    .await
    .expect("accept application handshake on persistent TLS connection");
    assert!(
        handshake_response.starts_with("HTTP/1.1 200 OK"),
        "{handshake_response}"
    );
    let handshake = response_json(&handshake_response);
    let body = serde_json::to_vec(body).expect("serialize authenticated application request");
    let response = send_http(&mut stream, endpoint, method, path, &body)
        .await
        .expect("complete authenticated application request");
    (handshake, response)
}

async fn authenticated_application_request_with_body<T: Serialize>(
    endpoint: SocketAddr,
    client: &EnrolledClient,
    method: &str,
    path: &str,
    body: impl FnOnce(&HandshakeResponse) -> T,
) -> (HandshakeResponse, String) {
    let stream = tokio::net::TcpStream::connect(endpoint)
        .await
        .expect("connect remote listener for authenticated application request");
    let server_name = ServerName::IpAddress(endpoint.ip().into());
    let mut stream = TlsConnector::from(client.config.clone())
        .connect(server_name, stream)
        .await
        .expect("complete mutual TLS application handshake");
    let handshake_body = serde_json::to_vec(&AuthenticatedHandshakeRequest {
        handshake: client.handshake.clone(),
        tls_exporter_sha256: exporter_digest(stream.get_ref().1),
    })
    .expect("serialize authenticated handshake");
    let handshake_response = send_http_keep_alive(
        &mut stream,
        endpoint,
        "POST",
        "/v1/distributed/connections/accept",
        &handshake_body,
    )
    .await
    .expect("accept application handshake on persistent TLS connection");
    assert!(
        handshake_response.starts_with("HTTP/1.1 200 OK"),
        "{handshake_response}"
    );
    let handshake: HandshakeResponse = response_json(&handshake_response);
    let body =
        serde_json::to_vec(&body(&handshake)).expect("serialize authenticated application request");
    let response = send_http(&mut stream, endpoint, method, path, &body)
        .await
        .expect("complete authenticated application request");
    (handshake, response)
}

async fn tls_request_bytes(
    endpoint: SocketAddr,
    config: Arc<ClientConfig>,
    method: &str,
    path: &str,
    body: Vec<u8>,
) -> Result<(String, [u8; 32]), String> {
    let stream = tokio::net::TcpStream::connect(endpoint)
        .await
        .map_err(|error| error.to_string())?;
    let server_name = ServerName::IpAddress(endpoint.ip().into());
    let mut stream = TlsConnector::from(config)
        .connect(server_name, stream)
        .await
        .map_err(|error| error.to_string())?;
    let exporter = exporter_digest(stream.get_ref().1);
    let response = send_http(&mut stream, endpoint, method, path, &body)
        .await
        .map_err(|error| error.to_string())?;
    Ok((response, exporter))
}

fn exporter_digest(connection: &rustls::ClientConnection) -> [u8; 32] {
    let mut exporter = [0_u8; 32];
    connection
        .export_keying_material(&mut exporter, TLS_EXPORTER_LABEL, None)
        .expect("derive TLS exporter");
    Sha256::digest(exporter).into()
}

async fn send_http(
    stream: &mut tokio_rustls::client::TlsStream<tokio::net::TcpStream>,
    endpoint: SocketAddr,
    method: &str,
    path: &str,
    body: &[u8],
) -> std::io::Result<String> {
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {endpoint}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(request.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await?;
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await?;
    Ok(String::from_utf8_lossy(&response).into_owned())
}

async fn send_http_keep_alive(
    stream: &mut tokio_rustls::client::TlsStream<tokio::net::TcpStream>,
    endpoint: SocketAddr,
    method: &str,
    path: &str,
    body: &[u8],
) -> std::io::Result<String> {
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {endpoint}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n",
        body.len()
    );
    stream.write_all(request.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.flush().await?;

    let mut response = Vec::new();
    while !response.ends_with(b"\r\n\r\n") {
        let mut byte = [0_u8; 1];
        stream.read_exact(&mut byte).await?;
        response.push(byte[0]);
    }
    let headers = String::from_utf8_lossy(&response);
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    let mut response_body = vec![0_u8; content_length];
    stream.read_exact(&mut response_body).await?;
    response.extend_from_slice(&response_body);
    Ok(String::from_utf8_lossy(&response).into_owned())
}

fn response_json<T: serde::de::DeserializeOwned>(response: &str) -> T {
    let (_, body) = response
        .split_once("\r\n\r\n")
        .expect("HTTP response body delimiter");
    serde_json::from_str(body).expect("decode response JSON")
}

fn spawn_server(
    binary: &str,
    data_dir: &Path,
    local_endpoint: SocketAddr,
    remote_endpoint: SocketAddr,
) -> ChildGuard {
    let child = Command::new(binary)
        .arg("--data-dir")
        .arg(data_dir)
        .arg("serve")
        .arg("--bind")
        .arg(local_endpoint.to_string())
        .arg("--remote-bind")
        .arg(remote_endpoint.to_string())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mTLS master");
    ChildGuard { child }
}

fn read_ready(child: &mut Child) -> Value {
    let stdout = child.stdout.take().expect("master stdout");
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();
    reader.read_line(&mut line).expect("read ready receipt");
    assert!(!line.is_empty(), "master exited without ready receipt");
    serde_json::from_str(&line)
        .unwrap_or_else(|error| panic!("invalid ready receipt {line:?}: {error}"))
}

fn unused_loopback_addr() -> SocketAddr {
    TcpListener::bind("127.0.0.1:0")
        .expect("reserve loopback address")
        .local_addr()
        .expect("read loopback address")
}

fn assert_success(output: &std::process::Output, operation: &str) {
    assert!(
        output.status.success(),
        "{operation} failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
