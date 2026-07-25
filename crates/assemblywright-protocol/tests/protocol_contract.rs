use assemblywright_protocol::{
    AttemptId, AuthenticatedHandshakeRequest, CancellationId, CapabilityDescriptor, CapabilityKind,
    ContextHandlingPolicy, DeviceId, DeviceRole, EnrollmentCsrReply, EnrollmentInvitation,
    HandshakeRequest, HandshakeResponse, JobEnvelope, JobResultEnvelope, JobResultStatus, LeaseId,
    ProtocolError, Sensitivity, StepId, TaskId, ENROLLMENT_CSR_READY_STATUS,
    ENROLLMENT_INVITATION_READY_STATUS, ENROLLMENT_PAIRING_SCHEMA_VERSION,
    MAX_ENROLLMENT_CSR_PEM_BYTES, MAX_ENROLLMENT_PAIRING_FRAME_BYTES, MAX_JOB_CONTEXT_BYTES,
    MAX_JOB_RESULT_BYTES, MAX_LEASE_DURATION_MS, MAX_WIRE_FRAME_BYTES, PROTOCOL_VERSION,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn fixed_uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("fixed UUID")
}

fn digest_json(value: &Value) -> [u8; 32] {
    Sha256::digest(serde_json::to_vec(value).expect("serialize JSON for digest")).into()
}

fn sample_job() -> JobEnvelope {
    let context = json!({"prompt":"review this bounded plan"});
    JobEnvelope {
        protocol_version: PROTOCOL_VERSION,
        connection_epoch: 9,
        sequence: 12,
        task_id: TaskId::new(fixed_uuid("22222222-2222-4222-8222-222222222222")),
        step_id: StepId::new(fixed_uuid("33333333-3333-4333-8333-333333333333")),
        attempt_id: AttemptId::new(fixed_uuid("44444444-4444-4444-8444-444444444444")),
        lease_id: LeaseId::new(fixed_uuid("55555555-5555-4555-8555-555555555555")),
        cancellation_id: CancellationId::new(fixed_uuid("66666666-6666-4666-8666-666666666666")),
        capability_id: "m1.reasoning".to_string(),
        selected_model: "qwen3.6-27b".to_string(),
        sensitivity: Sensitivity::Workspace,
        context_handling: ContextHandlingPolicy::EphemeralNoRetention,
        lease_duration_ms: MAX_LEASE_DURATION_MS,
        deadline_after_ms: 60_000,
        context_sha256: digest_json(&context),
        context,
    }
}

fn sample_invitation() -> EnrollmentInvitation {
    EnrollmentInvitation {
        schema_version: ENROLLMENT_PAIRING_SCHEMA_VERSION,
        status: ENROLLMENT_INVITATION_READY_STATUS.to_string(),
        grant_id: fixed_uuid("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa"),
        device_id: DeviceId::new(fixed_uuid("11111111-1111-4111-8111-111111111111")),
        device_name: "owner-mac-bridge".to_string(),
        role: DeviceRole::MacBridge,
        registry_revision: 1,
        expires_at_ms: 2_000_000,
        capabilities: vec![CapabilityDescriptor {
            id: "m1.reasoning".to_string(),
            kind: CapabilityKind::LocalInference,
            provider: "mlx".to_string(),
            model: "qwen3.6-27b".to_string(),
            max_context_bytes: 262_144,
            max_result_bytes: 786_432,
        }],
        master_endpoint: "100.64.23.14:7792".parse().expect("fixed endpoint"),
        ca_fingerprint_sha256: "ab".repeat(32),
    }
}

#[test]
fn enrollment_pairing_documents_have_exact_secret_free_v1_json() {
    let invitation = sample_invitation();
    invitation.validate_at(1_999_999).expect("valid invitation");
    let encoded = serde_json::to_value(&invitation).expect("encode invitation");
    assert_eq!(
        encoded,
        json!({
            "schema_version": 1,
            "status": "enrollment_invitation_ready",
            "grant_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "device_id": "11111111-1111-4111-8111-111111111111",
            "device_name": "owner-mac-bridge",
            "role": "mac_bridge",
            "registry_revision": 1,
            "expires_at_ms": 2_000_000,
            "capabilities": [{
                "id": "m1.reasoning",
                "kind": "local_inference",
                "provider": "mlx",
                "model": "qwen3.6-27b",
                "max_context_bytes": 262_144,
                "max_result_bytes": 786_432
            }],
            "master_endpoint": "100.64.23.14:7792",
            "ca_fingerprint_sha256": "ab".repeat(32)
        })
    );
    assert!(encoded.get("grant_secret").is_none());

    let reply = EnrollmentCsrReply {
        schema_version: ENROLLMENT_PAIRING_SCHEMA_VERSION,
        status: ENROLLMENT_CSR_READY_STATUS.to_string(),
        grant_id: invitation.grant_id,
        device_id: invitation.device_id,
        csr_pem: "-----BEGIN CERTIFICATE REQUEST-----\npublic-key-only\n-----END CERTIFICATE REQUEST-----".to_string(),
    };
    assert_eq!(
        serde_json::to_value(&reply).expect("encode CSR reply"),
        json!({
            "schema_version": 1,
            "status": "enrollment_csr_ready",
            "grant_id": "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
            "device_id": "11111111-1111-4111-8111-111111111111",
            "csr_pem": "-----BEGIN CERTIFICATE REQUEST-----\npublic-key-only\n-----END CERTIFICATE REQUEST-----"
        })
    );
}

#[test]
fn enrollment_pairing_documents_fail_closed_on_expiry_identity_and_bounds() {
    let invitation = sample_invitation();
    assert_eq!(
        invitation.validate_at(invitation.expires_at_ms),
        Err(ProtocolError::EnrollmentInvitationExpired)
    );

    let mut invalid_endpoint = invitation.clone();
    invalid_endpoint.master_endpoint = "0.0.0.0:7792".parse().unwrap();
    assert_eq!(
        invalid_endpoint.validate(),
        Err(ProtocolError::InvalidSocketEndpoint {
            field: "master_endpoint"
        })
    );

    let mut invalid_fingerprint = invitation.clone();
    invalid_fingerprint.ca_fingerprint_sha256 = "A".repeat(64);
    assert_eq!(
        invalid_fingerprint.validate(),
        Err(ProtocolError::InvalidSha256Hex {
            field: "ca_fingerprint_sha256"
        })
    );

    let mut wrong_role = invitation.clone();
    wrong_role.role = DeviceRole::InferenceWorker;
    assert!(matches!(
        wrong_role.validate(),
        Err(ProtocolError::UnsupportedFixedValue { field: "role", .. })
    ));

    let mut empty_capabilities = invitation.clone();
    empty_capabilities.capabilities.clear();
    assert_eq!(
        empty_capabilities.validate(),
        Err(ProtocolError::EmptyField {
            field: "capabilities"
        })
    );

    let mut unknown = serde_json::to_value(&invitation).expect("encode invitation");
    unknown["grant_secret"] = json!("must-never-cross-this-boundary");
    assert!(matches!(
        EnrollmentInvitation::decode_frame(&serde_json::to_vec(&unknown).unwrap()),
        Err(ProtocolError::Deserialization { .. })
    ));

    let oversized_reply = EnrollmentCsrReply {
        schema_version: ENROLLMENT_PAIRING_SCHEMA_VERSION,
        status: ENROLLMENT_CSR_READY_STATUS.to_string(),
        grant_id: invitation.grant_id,
        device_id: invitation.device_id,
        csr_pem: "x".repeat(MAX_ENROLLMENT_CSR_PEM_BYTES + 1),
    };
    assert!(matches!(
        oversized_reply.validate(),
        Err(ProtocolError::FieldTooLarge {
            field: "csr_pem",
            ..
        })
    ));
    assert_eq!(
        EnrollmentCsrReply::decode_frame(&vec![b' '; MAX_ENROLLMENT_PAIRING_FRAME_BYTES + 1]),
        Err(ProtocolError::FrameTooLarge {
            field: "enrollment_csr_reply",
            maximum: MAX_ENROLLMENT_PAIRING_FRAME_BYTES,
        })
    );
}

#[test]
fn mac_bridge_handshake_matches_v1_golden_fixture() {
    let fixture = include_str!("fixtures/mac_bridge_hello_v1.json");
    let request =
        HandshakeRequest::decode_frame(fixture.as_bytes()).expect("decode golden request");

    request.validate().expect("valid golden handshake");
    assert_eq!(request.role, DeviceRole::MacBridge);
    assert_eq!(request.capabilities.len(), 1);
    assert_eq!(request.capabilities[0].kind, CapabilityKind::LocalInference);

    let expected: Value = serde_json::from_str(fixture).expect("decode golden JSON");
    let encoded = serde_json::to_value(request).expect("encode handshake");
    assert_eq!(encoded, expected);
}

#[test]
fn authenticated_handshake_requires_a_bounded_nonzero_tls_exporter_digest() {
    let handshake = HandshakeRequest::decode_frame(
        include_str!("fixtures/mac_bridge_hello_v1.json").as_bytes(),
    )
    .expect("decode handshake fixture");
    let request = AuthenticatedHandshakeRequest {
        handshake: handshake.clone(),
        tls_exporter_sha256: [7; 32],
    };
    let encoded = serde_json::to_vec(&request).expect("encode authenticated handshake");
    assert_eq!(
        AuthenticatedHandshakeRequest::decode_frame(&encoded).expect("decode envelope"),
        request
    );

    let unbound = AuthenticatedHandshakeRequest {
        handshake,
        tls_exporter_sha256: [0; 32],
    };
    assert_eq!(
        unbound.validate(),
        Err(ProtocolError::InvalidChannelBinding)
    );
}

#[test]
fn handshake_rejects_unknown_fields_and_duplicate_capabilities() {
    let unknown = json!({
        "protocol_version": 1,
        "device_id": "11111111-1111-4111-8111-111111111111",
        "device_name": "worker",
        "role": "inference_worker",
        "registry_revision": 1,
        "capabilities": [],
        "unexpected": true
    });
    let unknown = serde_json::to_vec(&unknown).expect("encode unknown-field fixture");
    assert!(matches!(
        HandshakeRequest::decode_frame(&unknown),
        Err(ProtocolError::Deserialization { .. })
    ));

    let capability = CapabilityDescriptor {
        id: "rtx.fast".to_string(),
        kind: CapabilityKind::LocalInference,
        provider: "ollama".to_string(),
        model: "qwen".to_string(),
        max_context_bytes: 4096,
        max_result_bytes: 4096,
    };
    let request = HandshakeRequest {
        protocol_version: PROTOCOL_VERSION,
        device_id: DeviceId::new(Uuid::new_v4()),
        device_name: "windows-master-worker".to_string(),
        role: DeviceRole::InferenceWorker,
        registry_revision: 1,
        capabilities: vec![capability.clone(), capability],
    };
    assert!(matches!(
        request.validate(),
        Err(ProtocolError::DuplicateCapability(id)) if id == "rtx.fast"
    ));
}

#[test]
fn handshake_rejects_incompatible_protocol_version() {
    let request = HandshakeRequest {
        protocol_version: PROTOCOL_VERSION + 1,
        device_id: DeviceId::new(Uuid::new_v4()),
        device_name: "future-worker".to_string(),
        role: DeviceRole::InferenceWorker,
        registry_revision: 1,
        capabilities: vec![],
    };
    assert_eq!(
        request.validate(),
        Err(ProtocolError::UnsupportedVersion {
            expected: PROTOCOL_VERSION,
            received: PROTOCOL_VERSION + 1,
        })
    );
}

#[test]
fn wire_decoders_reject_oversized_frames_before_json_decoding() {
    let oversized_handshake = vec![b' '; assemblywright_protocol::MAX_HANDSHAKE_FRAME_BYTES + 1];
    assert_eq!(
        HandshakeRequest::decode_frame(&oversized_handshake),
        Err(ProtocolError::FrameTooLarge {
            field: "handshake",
            maximum: assemblywright_protocol::MAX_HANDSHAKE_FRAME_BYTES,
        })
    );
    assert_eq!(
        HandshakeResponse::decode_frame(&oversized_handshake),
        Err(ProtocolError::FrameTooLarge {
            field: "handshake_response",
            maximum: assemblywright_protocol::MAX_HANDSHAKE_FRAME_BYTES,
        })
    );

    let unknown_response = serde_json::to_vec(&json!({
        "protocol_version": PROTOCOL_VERSION,
        "status": "accepted",
        "connection_epoch": 1,
        "accepted_registry_revision": 1,
        "reason_code": null,
        "unexpected": true
    }))
    .expect("encode unknown response fixture");
    assert!(matches!(
        HandshakeResponse::decode_frame(&unknown_response),
        Err(ProtocolError::Deserialization { .. })
    ));

    let oversized_job = vec![b' '; MAX_WIRE_FRAME_BYTES + 1];
    assert_eq!(
        JobEnvelope::decode_frame(&oversized_job),
        Err(ProtocolError::FrameTooLarge {
            field: "job",
            maximum: MAX_WIRE_FRAME_BYTES,
        })
    );
    assert_eq!(
        JobResultEnvelope::decode_frame(&oversized_job),
        Err(ProtocolError::FrameTooLarge {
            field: "job_result",
            maximum: MAX_WIRE_FRAME_BYTES,
        })
    );
}

#[test]
fn protocol_identifiers_reject_nil_uuids() {
    let request = HandshakeRequest {
        protocol_version: PROTOCOL_VERSION,
        device_id: DeviceId::new(Uuid::nil()),
        device_name: "nil-device".to_string(),
        role: DeviceRole::InferenceWorker,
        registry_revision: 1,
        capabilities: vec![],
    };
    assert_eq!(
        request.validate(),
        Err(ProtocolError::NilIdentifier { field: "device_id" })
    );

    let valid = sample_job();
    let nil = Uuid::nil();
    let cases = [
        ("task_id", {
            let mut job = valid.clone();
            job.task_id = TaskId::new(nil);
            job
        }),
        ("step_id", {
            let mut job = valid.clone();
            job.step_id = StepId::new(nil);
            job
        }),
        ("attempt_id", {
            let mut job = valid.clone();
            job.attempt_id = AttemptId::new(nil);
            job
        }),
        ("lease_id", {
            let mut job = valid.clone();
            job.lease_id = LeaseId::new(nil);
            job
        }),
        ("cancellation_id", {
            let mut job = valid.clone();
            job.cancellation_id = CancellationId::new(nil);
            job
        }),
    ];
    for (field, job) in cases {
        assert_eq!(job.validate(), Err(ProtocolError::NilIdentifier { field }));
    }
}

#[test]
fn job_envelope_enforces_context_and_lease_bounds() {
    let mut job = sample_job();
    job.validate().expect("valid job");

    job.lease_duration_ms = MAX_LEASE_DURATION_MS + 1;
    assert!(matches!(
        job.validate(),
        Err(ProtocolError::InvalidLimit {
            field: "lease_duration_ms",
            ..
        })
    ));

    job.lease_duration_ms = MAX_LEASE_DURATION_MS;
    job.context = json!({"prompt":"x".repeat(MAX_JOB_CONTEXT_BYTES)});
    job.context_sha256 = digest_json(&job.context);
    assert!(matches!(
        job.validate(),
        Err(ProtocolError::SerializedValueTooLarge {
            field: "context",
            ..
        })
    ));
}

#[test]
fn result_must_match_the_exact_leased_job() {
    let job = sample_job();
    let payload = json!({"answer":"bounded result"});
    let mut result = JobResultEnvelope {
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
        payload_sha256: digest_json(&payload),
        payload,
    };
    result
        .validate_for_job(&job)
        .expect("matching result identity");

    result.lease_id = LeaseId::new(Uuid::new_v4());
    assert_eq!(
        result.validate_for_job(&job),
        Err(ProtocolError::ResultIdentityMismatch)
    );

    result.lease_id = job.lease_id;
    result.sequence = job.sequence;
    assert_eq!(
        result.validate_for_job(&job),
        Err(ProtocolError::ResultIdentityMismatch)
    );

    result.sequence = job.sequence + 1;
    result.cancellation_id = CancellationId::new(Uuid::nil());
    assert_eq!(
        result.validate(),
        Err(ProtocolError::NilIdentifier {
            field: "cancellation_id"
        })
    );
}

#[test]
fn result_payload_and_wire_frame_are_bounded() {
    let job = sample_job();
    let payload = json!({"answer":"x".repeat(MAX_JOB_RESULT_BYTES)});
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
        payload_sha256: digest_json(&payload),
        payload,
    };
    assert!(matches!(
        result.validate(),
        Err(ProtocolError::SerializedValueTooLarge {
            field: "payload",
            ..
        })
    ));
}

#[test]
fn job_and_result_reject_payload_digest_tampering() {
    let mut job = sample_job();
    job.context = json!({"prompt":"tampered after digest"});
    assert_eq!(
        job.validate(),
        Err(ProtocolError::PayloadDigestMismatch {
            field: "context_sha256"
        })
    );

    let job = sample_job();
    let payload = json!({"answer":"bounded result"});
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
        payload_sha256: [0; 32],
        payload,
    };
    assert_eq!(
        result.validate(),
        Err(ProtocolError::PayloadDigestMismatch {
            field: "payload_sha256"
        })
    );
}
