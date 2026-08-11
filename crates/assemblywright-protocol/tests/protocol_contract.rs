use assemblywright_protocol::{
    feature_conveyor_provider_binding_sha256, repository_preflight_fingerprint_sha256, AttemptId,
    AuthenticatedHandshakeRequest, CancellationId, CapabilityDescriptor, CapabilityKind,
    ContextHandlingPolicy, DeviceId, DeviceRole, EnrollmentCsrReply, EnrollmentInvitation,
    FeatureConveyorApprovedFeatureRequest, FeatureConveyorApprovedSpecification,
    FeatureConveyorGrantRevisions, FeatureConveyorOwnerBridgeDesignationRequest,
    FeatureConveyorRepositoryGrantKind, FeatureConveyorRepositoryGrantRequest,
    FeatureConveyorRepositoryGrantRevision, FeatureConveyorRepositoryPreflightReceipt,
    FeatureConveyorRepositoryPreflightRequest, FeatureConveyorRepositoryPreflightStatus,
    FeatureConveyorRepositoryScopeDocument, FeatureConveyorRepositorySnapshotClaimReceipt,
    FeatureConveyorRepositorySnapshotClaimRequest, FeatureConveyorRepositorySnapshotClaimStatus,
    HandshakeRequest, HandshakeResponse, JobEnvelope, JobResultEnvelope, JobResultStatus, LeaseId,
    ProtocolError, Sensitivity, StepId, TaskId, ENROLLMENT_CSR_READY_STATUS,
    ENROLLMENT_INVITATION_READY_STATUS, ENROLLMENT_PAIRING_SCHEMA_VERSION,
    FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION, MAX_ENROLLMENT_CSR_PEM_BYTES,
    MAX_ENROLLMENT_PAIRING_FRAME_BYTES, MAX_FEATURE_CONVEYOR_DEPENDENCIES,
    MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES, MAX_FEATURE_CONVEYOR_REPOSITORY_PATH_BYTES,
    MAX_FEATURE_CONVEYOR_REPOSITORY_PREFLIGHT_REQUEST_BYTES,
    MAX_FEATURE_CONVEYOR_SNAPSHOT_CLAIM_REQUEST_BYTES, MAX_JOB_CONTEXT_BYTES, MAX_JOB_RESULT_BYTES,
    MAX_LEASE_DURATION_MS, MAX_WIRE_FRAME_BYTES, PROTOCOL_VERSION,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

fn fixed_uuid(value: &str) -> Uuid {
    Uuid::parse_str(value).expect("fixed UUID")
}

#[test]
fn repository_snapshot_claim_contract_is_strict_exact_and_path_free_on_receipt() {
    let preflight = repository_preflight_request();
    let request = FeatureConveyorRepositorySnapshotClaimRequest {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        scope: preflight.scope,
        scope_sha256: preflight.scope_sha256,
        expected_feature_id: fixed_uuid("77777777-7777-4777-8777-777777777777"),
        expected_specification_revision: 2,
        expected_queue_revision: 4,
        expected_emergency_pause_revision: 3,
        grants: FeatureConveyorGrantRevisions {
            registration: 5,
            cloud_disclosure: 6,
            autonomous_publication: 7,
        },
        provider_id: "local.review".to_string(),
        model_id: "review-v1".to_string(),
    };
    request.validate().unwrap();
    let encoded = serde_json::to_vec(&request).unwrap();
    assert_eq!(
        FeatureConveyorRepositorySnapshotClaimRequest::decode_frame(&encoded).unwrap(),
        request
    );
    let duplicate = String::from_utf8(encoded.clone()).unwrap().replacen(
        "\"scope\":{",
        "\"scope\":{\"repository_path\":\"private\",",
        1,
    );
    assert!(
        FeatureConveyorRepositorySnapshotClaimRequest::decode_frame(duplicate.as_bytes()).is_err()
    );
    let unknown = String::from_utf8(encoded).unwrap().replacen(
        "\"provider_id\":",
        "\"owner_token\":\"forbidden\",\"provider_id\":",
        1,
    );
    assert!(
        FeatureConveyorRepositorySnapshotClaimRequest::decode_frame(unknown.as_bytes()).is_err()
    );
    let mut zero_grant = request.clone();
    zero_grant.grants.cloud_disclosure = 0;
    assert!(zero_grant.validate().is_err());
    let mut stale_scope = request.clone();
    stale_scope.scope.expected_head_commit = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into();
    assert!(stale_scope.validate().is_err());
    assert_eq!(
        FeatureConveyorRepositorySnapshotClaimRequest::decode_frame(&vec![
            b' ';
            MAX_FEATURE_CONVEYOR_SNAPSHOT_CLAIM_REQUEST_BYTES
                + 1
        ]),
        Err(ProtocolError::FrameTooLarge {
            field: "feature_conveyor_repository_snapshot_claim_request",
            maximum: MAX_FEATURE_CONVEYOR_SNAPSHOT_CLAIM_REQUEST_BYTES,
        })
    );

    let receipt = FeatureConveyorRepositorySnapshotClaimReceipt {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        feature_id: request.expected_feature_id,
        specification_revision: request.expected_specification_revision,
        lifecycle_revision: 2,
        queue_revision: 5,
        emergency_pause_revision: request.expected_emergency_pause_revision,
        lease_id: fixed_uuid("11111111-1111-4111-8111-111111111111"),
        snapshot_id: fixed_uuid("22222222-2222-4222-8222-222222222222"),
        snapshot_sha256: [8; 32],
        base_commit: request.scope.expected_head_commit.clone(),
        grants: request.grants,
        provider_binding_sha256: feature_conveyor_provider_binding_sha256(
            &request.provider_id,
            &request.model_id,
        ),
        status: FeatureConveyorRepositorySnapshotClaimStatus::SnapshotBound,
    };
    receipt.validate().unwrap();
    let receipt_json = serde_json::to_vec(&receipt).unwrap();
    assert_eq!(
        FeatureConveyorRepositorySnapshotClaimReceipt::decode_frame(&receipt_json).unwrap(),
        receipt
    );
    let text = String::from_utf8(receipt_json).unwrap();
    assert!(!text.contains("repository_path"));
    assert!(!text.contains("provider_id"));
    assert!(!text.contains("model_id"));
}

fn repository_preflight_request() -> FeatureConveyorRepositoryPreflightRequest {
    let scope = FeatureConveyorRepositoryScopeDocument {
        repository_id: fixed_uuid("88888888-8888-4888-8888-888888888888"),
        repository_path: "/private/owner/repository".to_string(),
        expected_base_branch: "main".to_string(),
        expected_head_commit: "1234567890abcdef1234567890abcdef12345678".to_string(),
    };
    FeatureConveyorRepositoryPreflightRequest {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        scope_sha256: scope.canonical_scope_sha256().unwrap(),
        scope,
        registration_grant_revision: 3,
        expected_emergency_pause_revision: 2,
    }
}

fn digest_json(value: &Value) -> [u8; 32] {
    Sha256::digest(serde_json::to_vec(value).expect("serialize JSON for digest")).into()
}

fn canonical_digest(value: &Value) -> [u8; 32] {
    let canonical = match value {
        Value::Object(object) => {
            let mut keys = object.keys().collect::<Vec<_>>();
            keys.sort();
            format!(
                "{{{}}}",
                keys.into_iter()
                    .map(|key| format!(
                        "{}:{}",
                        serde_json::to_string(key).unwrap(),
                        serde_json::to_string(&object[key]).unwrap()
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            )
        }
        _ => unreachable!("test manifest is an object"),
    };
    Sha256::digest(canonical.as_bytes()).into()
}

fn approved_feature_request() -> FeatureConveyorApprovedFeatureRequest {
    let feature_id = fixed_uuid("77777777-7777-4777-8777-777777777777");
    let manifest = json!({"allowed_paths":["crates/example"],"outcome":"bounded"});
    FeatureConveyorApprovedFeatureRequest {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        expected_queue_revision: 0,
        owner_control_designation_revision: 1,
        emergency_pause_revision: 0,
        specification: FeatureConveyorApprovedSpecification {
            feature_id,
            revision: 1,
            repository_id: fixed_uuid("88888888-8888-4888-8888-888888888888"),
            manifest_sha256: canonical_digest(&manifest),
            manifest,
            design_sha256: [1; 32],
            brainstorming_sha256: [2; 32],
            owner_approval_sha256: [3; 32],
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

#[test]
fn feature_conveyor_owner_control_dtos_are_strict_bounded_and_independently_versioned() {
    assert_eq!(PROTOCOL_VERSION, 2);
    let designation = FeatureConveyorOwnerBridgeDesignationRequest {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        device_id: DeviceId::new(fixed_uuid("99999999-9999-4999-8999-999999999999")),
        expected_designation_revision: 0,
    };
    designation.validate().unwrap();
    assert!(
        serde_json::from_value::<FeatureConveyorOwnerBridgeDesignationRequest>(json!({
            "schema_version": 1,
            "device_id": "99999999-9999-4999-8999-999999999999",
            "expected_designation_revision": 0,
            "owner_token": "must-not-be-accepted"
        }))
        .is_err()
    );

    let valid = approved_feature_request();
    valid.validate().unwrap();
    let encoded = serde_json::to_vec(&valid).unwrap();
    assert_eq!(
        FeatureConveyorApprovedFeatureRequest::decode_frame(&encoded).unwrap(),
        valid
    );
    let duplicate_nested_key = String::from_utf8(encoded).unwrap().replacen(
        "\"manifest\":{",
        "\"manifest\":{\"duplicate\":1,\"duplicate\":2,",
        1,
    );
    assert!(
        FeatureConveyorApprovedFeatureRequest::decode_frame(duplicate_nested_key.as_bytes())
            .is_err()
    );

    let mut wrong_schema = valid.clone();
    wrong_schema.schema_version += 1;
    assert!(wrong_schema.validate().is_err());
    let mut no_designation = valid.clone();
    no_designation.owner_control_designation_revision = 0;
    assert!(no_designation.validate().is_err());
    let mut wrong_digest = valid.clone();
    wrong_digest.specification.manifest_sha256 = [9; 32];
    assert!(wrong_digest.validate().is_err());
    let mut numeric_manifest = approved_feature_request();
    numeric_manifest.specification.manifest = json!({"ratio": 1.0});
    numeric_manifest.specification.manifest_sha256 =
        canonical_digest(&numeric_manifest.specification.manifest);
    numeric_manifest.validate().unwrap();
    let mut duplicate_dependencies = valid.clone();
    let dependency = fixed_uuid("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa");
    duplicate_dependencies.specification.dependencies = vec![dependency, dependency];
    assert!(duplicate_dependencies.validate().is_err());
    let mut self_dependency = valid.clone();
    self_dependency.specification.dependencies = vec![self_dependency.specification.feature_id];
    assert!(self_dependency.validate().is_err());
    let mut too_many_dependencies = valid;
    too_many_dependencies.specification.dependencies = (0..=MAX_FEATURE_CONVEYOR_DEPENDENCIES)
        .map(|index| Uuid::from_u128(0x4000 + index as u128))
        .collect();
    assert!(too_many_dependencies.validate().is_err());
}

#[test]
fn repository_grant_requests_are_strict_revision_bound_and_digest_only() {
    let repository_id = fixed_uuid("88888888-8888-4888-8888-888888888888");
    let valid = FeatureConveyorRepositoryGrantRequest {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        expected_current_revision: 0,
        expected_emergency_pause_revision: 2,
        grant: FeatureConveyorRepositoryGrantRevision {
            repository_id,
            kind: FeatureConveyorRepositoryGrantKind::Registration,
            revision: 1,
            scope_sha256: [4; 32],
            owner_approval_sha256: [5; 32],
            expires_at_ms: Some(2_000_000),
            revoked: false,
        },
    };
    valid.validate().unwrap();
    let encoded = serde_json::to_vec(&valid).unwrap();
    assert_eq!(
        FeatureConveyorRepositoryGrantRequest::decode_frame(&encoded).unwrap(),
        valid
    );
    let duplicate = String::from_utf8(encoded).unwrap().replacen(
        "\"grant\":{",
        "\"grant\":{\"revision\":1,",
        1,
    );
    assert!(FeatureConveyorRepositoryGrantRequest::decode_frame(duplicate.as_bytes()).is_err());

    for invalid in [
        FeatureConveyorRepositoryGrantRequest {
            schema_version: 2,
            ..valid
        },
        FeatureConveyorRepositoryGrantRequest {
            expected_current_revision: 1,
            ..valid
        },
        FeatureConveyorRepositoryGrantRequest {
            expected_current_revision: u64::MAX,
            grant: FeatureConveyorRepositoryGrantRevision {
                revision: u64::MAX,
                ..valid.grant
            },
            ..valid
        },
        FeatureConveyorRepositoryGrantRequest {
            grant: FeatureConveyorRepositoryGrantRevision {
                repository_id: Uuid::nil(),
                ..valid.grant
            },
            ..valid
        },
        FeatureConveyorRepositoryGrantRequest {
            grant: FeatureConveyorRepositoryGrantRevision {
                scope_sha256: [0; 32],
                ..valid.grant
            },
            ..valid
        },
        FeatureConveyorRepositoryGrantRequest {
            grant: FeatureConveyorRepositoryGrantRevision {
                owner_approval_sha256: [0; 32],
                ..valid.grant
            },
            ..valid
        },
        FeatureConveyorRepositoryGrantRequest {
            grant: FeatureConveyorRepositoryGrantRevision {
                expires_at_ms: Some(0),
                ..valid.grant
            },
            ..valid
        },
    ] {
        assert!(invalid.validate().is_err());
    }
    assert_eq!(
        FeatureConveyorRepositoryGrantRequest::decode_frame(&vec![
            b' ';
            MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES
                + 1
        ]),
        Err(ProtocolError::FrameTooLarge {
            field: "feature_conveyor_repository_grant_request",
            maximum: MAX_FEATURE_CONVEYOR_OWNER_CONTROL_REQUEST_BYTES,
        })
    );
}

#[test]
fn repository_preflight_scope_is_canonical_strict_bounded_and_revision_bound() {
    let valid = repository_preflight_request();
    valid.validate().unwrap();
    let expected_canonical = r#"{"expected_base_branch":"main","expected_head_commit":"1234567890abcdef1234567890abcdef12345678","repository_id":"88888888-8888-4888-8888-888888888888","repository_path":"/private/owner/repository"}"#;
    assert_eq!(
        valid.scope_sha256,
        Sha256::digest(expected_canonical.as_bytes()).as_slice()
    );
    let encoded = serde_json::to_vec(&valid).unwrap();
    assert_eq!(
        FeatureConveyorRepositoryPreflightRequest::decode_frame(&encoded).unwrap(),
        valid
    );

    let duplicate = String::from_utf8(encoded.clone()).unwrap().replacen(
        "\"scope\":{",
        "\"scope\":{\"repository_id\":\"88888888-8888-4888-8888-888888888888\",",
        1,
    );
    assert!(FeatureConveyorRepositoryPreflightRequest::decode_frame(duplicate.as_bytes()).is_err());
    let unknown = String::from_utf8(encoded).unwrap().replacen(
        "\"scope_sha256\":",
        "\"private_source\":\"forbidden\",\"scope_sha256\":",
        1,
    );
    assert!(FeatureConveyorRepositoryPreflightRequest::decode_frame(unknown.as_bytes()).is_err());

    let mut wrong_digest = valid.clone();
    wrong_digest.scope_sha256 = [9; 32];
    assert!(wrong_digest.validate().is_err());
    let mut zero_digest = valid.clone();
    zero_digest.scope_sha256 = [0; 32];
    assert!(zero_digest.validate().is_err());
    let mut no_grant = valid.clone();
    no_grant.registration_grant_revision = 0;
    assert!(no_grant.validate().is_err());
    let mut nil_repository = valid.clone();
    nil_repository.scope.repository_id = Uuid::nil();
    assert!(nil_repository.validate().is_err());
    let mut relative_path = valid.clone();
    relative_path.scope.repository_path = "relative/repository".to_string();
    assert!(relative_path.validate().is_err());
    let mut empty_path = valid.clone();
    empty_path.scope.repository_path.clear();
    assert!(empty_path.validate().is_err());
    let mut control_path = valid.clone();
    control_path.scope.repository_path = "/private/owner/repository\nsecret".to_string();
    assert!(control_path.validate().is_err());
    for forbidden_path in [
        "//server/share/repository",
        r"\\server\share\repository",
        r"\\?\C:\repository",
        r"\\.\C:\repository",
        "//?/C:/repository",
    ] {
        let mut forbidden = valid.clone();
        forbidden.scope.repository_path = forbidden_path.to_string();
        assert!(forbidden.validate().is_err(), "accepted {forbidden_path}");
    }
    let mut oversized_path = valid.clone();
    oversized_path.scope.repository_path = format!(
        "/{}",
        "r".repeat(MAX_FEATURE_CONVEYOR_REPOSITORY_PATH_BYTES)
    );
    assert!(oversized_path.validate().is_err());
    let mut malformed_branch = valid.clone();
    malformed_branch.scope.expected_base_branch = "refs/heads/../secret".to_string();
    assert!(malformed_branch.validate().is_err());
    let mut empty_branch = valid.clone();
    empty_branch.scope.expected_base_branch.clear();
    assert!(empty_branch.validate().is_err());
    let mut nested_branch = valid.clone();
    nested_branch.scope.expected_base_branch = "feature/nested".to_string();
    assert!(nested_branch.validate().is_err());
    let mut malformed_commit = valid.clone();
    malformed_commit.scope.expected_head_commit = "ABCDEF".repeat(7);
    assert!(malformed_commit.validate().is_err());
    let mut empty_commit = valid.clone();
    empty_commit.scope.expected_head_commit.clear();
    assert!(empty_commit.validate().is_err());
    let mut wrong_schema = valid.clone();
    wrong_schema.schema_version += 1;
    assert!(wrong_schema.validate().is_err());

    assert_eq!(
        FeatureConveyorRepositoryPreflightRequest::decode_frame(&vec![
            b' ';
            MAX_FEATURE_CONVEYOR_REPOSITORY_PREFLIGHT_REQUEST_BYTES
                + 1
        ]),
        Err(ProtocolError::FrameTooLarge {
            field: "feature_conveyor_repository_preflight_request",
            maximum: MAX_FEATURE_CONVEYOR_REPOSITORY_PREFLIGHT_REQUEST_BYTES,
        })
    );

    let mut receipt = FeatureConveyorRepositoryPreflightReceipt {
        schema_version: FEATURE_CONVEYOR_OWNER_CONTROL_SCHEMA_VERSION,
        repository_id: valid.scope.repository_id,
        registration_grant_revision: valid.registration_grant_revision,
        scope_sha256: valid.scope_sha256,
        emergency_pause_revision: valid.expected_emergency_pause_revision,
        base_branch: valid.scope.expected_base_branch.clone(),
        head_commit: valid.scope.expected_head_commit.clone(),
        preflight_fingerprint_sha256: [0; 32],
        observed_at_ms: 1_234,
        status: FeatureConveyorRepositoryPreflightStatus::IdentityEligible,
    };
    receipt.preflight_fingerprint_sha256 = repository_preflight_fingerprint_sha256(
        receipt.repository_id,
        receipt.registration_grant_revision,
        &receipt.scope_sha256,
        receipt.emergency_pause_revision,
        &receipt.base_branch,
        &receipt.head_commit,
        receipt.observed_at_ms,
    );
    receipt.validate().unwrap();
    let receipt_json = serde_json::to_vec(&receipt).unwrap();
    assert_eq!(
        FeatureConveyorRepositoryPreflightReceipt::decode_frame(&receipt_json).unwrap(),
        receipt
    );
    let unknown_receipt = String::from_utf8(receipt_json).unwrap().replacen(
        "\"status\":",
        "\"repository_path\":\"forbidden\",\"status\":",
        1,
    );
    assert!(
        FeatureConveyorRepositoryPreflightReceipt::decode_frame(unknown_receipt.as_bytes())
            .is_err()
    );
    let mut unbound = receipt.clone();
    unbound.observed_at_ms += 1;
    assert!(unbound.validate().is_err());
    let mut unobserved = receipt;
    unobserved.observed_at_ms = 0;
    unobserved.preflight_fingerprint_sha256 = repository_preflight_fingerprint_sha256(
        unobserved.repository_id,
        unobserved.registration_grant_revision,
        &unobserved.scope_sha256,
        unobserved.emergency_pause_revision,
        &unobserved.base_branch,
        &unobserved.head_commit,
        unobserved.observed_at_ms,
    );
    assert!(unobserved.validate().is_err());
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
    assert_eq!(
        wrong_role.validate(),
        Err(ProtocolError::InvalidLocalCodingCapability)
    );

    let mut local_coding_worker = invitation.clone();
    local_coding_worker.role = DeviceRole::InferenceWorker;
    local_coding_worker.capabilities = vec![CapabilityDescriptor::local_coding()];
    local_coding_worker
        .validate_at(invitation.expires_at_ms - 1)
        .expect("exact local-coding inference worker invitation");

    let mut local_coding_mac_bridge = invitation.clone();
    local_coding_mac_bridge.capabilities = vec![CapabilityDescriptor::local_coding()];
    assert_eq!(
        local_coding_mac_bridge.validate(),
        Err(ProtocolError::InvalidLocalCodingCapability)
    );

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
fn mac_bridge_handshake_matches_v2_golden_fixture() {
    let fixture = include_str!("fixtures/mac_bridge_hello_v2.json");
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
        include_str!("fixtures/mac_bridge_hello_v2.json").as_bytes(),
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
        "protocol_version": 2,
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
