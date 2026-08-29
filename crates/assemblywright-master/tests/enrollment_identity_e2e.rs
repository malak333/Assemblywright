use assemblywright_master::{
    CapabilityRebindAcknowledgement, EnrollmentGrantSpec, EnrollmentRequest, IdentityAuthority,
    IdentityError, MasterError, MasterKernel, MasterProcess, SecretProtector,
    DEVICE_CERTIFICATE_LIFETIME_MS, ENROLLMENT_GRANT_TTL_MS,
};
use assemblywright_protocol::{
    CapabilityDescriptor, CapabilityKind, DeviceRole, DistributedEventBatchRequest,
    MAX_JOB_CONTEXT_BYTES, MAX_JOB_RESULT_BYTES, PROTOCOL_VERSION,
};
use base64::Engine as _;
use p256::ecdsa::{signature::Verifier as _, Signature, VerifyingKey};
use rcgen::{CertificateParams, DistinguishedName, DnType, IsCa, KeyPair, SigningKey};
use x509_parser::prelude::{FromDer, GeneralName, X509Certificate};

#[derive(Debug, Clone, Copy)]
struct TestProtector;

impl SecretProtector for TestProtector {
    fn scheme(&self) -> &'static str {
        "test_reverse_v1"
    }

    fn protect(&self, plaintext: &[u8]) -> Result<Vec<u8>, IdentityError> {
        Ok(plaintext.iter().rev().copied().collect())
    }

    fn unprotect(&self, ciphertext: &[u8]) -> Result<Vec<u8>, IdentityError> {
        Ok(ciphertext.iter().rev().copied().collect())
    }
}

fn spec() -> EnrollmentGrantSpec {
    EnrollmentGrantSpec {
        device_name: "owner-mac-bridge".to_string(),
        role: DeviceRole::MacBridge,
        capabilities: vec![CapabilityDescriptor {
            id: "mlx.reasoning".to_string(),
            kind: CapabilityKind::LocalInference,
            provider: "mlx".to_string(),
            model: "test-model".to_string(),
            max_context_bytes: MAX_JOB_CONTEXT_BYTES as u32,
            max_result_bytes: MAX_JOB_RESULT_BYTES as u32,
        }],
    }
}

fn fixture_spec() -> EnrollmentGrantSpec {
    EnrollmentGrantSpec {
        device_name: "owner-mac-bridge".to_string(),
        role: DeviceRole::MacBridge,
        capabilities: vec![CapabilityDescriptor::fixture_reasoning()],
    }
}

fn csr(common_name: &str) -> String {
    csr_and_key(common_name).0
}

fn csr_and_key(common_name: &str) -> (String, KeyPair) {
    let key = KeyPair::generate().expect("generate client key");
    (csr_with_key(common_name, &key), key)
}

fn csr_with_key(common_name: &str, key: &KeyPair) -> String {
    let mut params = CertificateParams::default();
    let mut name = DistinguishedName::new();
    name.push(DnType::CommonName, common_name);
    params.distinguished_name = name;
    params.is_ca = IsCa::NoCa;
    params
        .serialize_request(&key)
        .expect("serialize signed CSR")
        .pem()
        .expect("encode CSR PEM")
}

fn sign_rebind_acknowledgement(
    acknowledgement: &mut CapabilityRebindAcknowledgement,
    key: &KeyPair,
) {
    let transcript = format!(
        "Assemblywright-Capability-Rebind-Acknowledgement-v1\ngrant_id={}\ndevice_id={}\nregistry_revision={}\nserial_hex={}\ncertificate_sha256={}\n",
        acknowledgement.grant_id,
        acknowledgement.device_id.0,
        acknowledgement.registry_revision,
        acknowledgement.serial_hex,
        acknowledgement.certificate_sha256
    );
    acknowledgement.signature_base64 = base64::engine::general_purpose::STANDARD.encode(
        key.sign(transcript.as_bytes())
            .expect("sign acknowledgement"),
    );
}

#[test]
fn enrollment_grants_issue_rotate_and_revoke_exact_device_identity() {
    let directory = tempfile::tempdir().expect("identity directory");
    let database = directory.path().join("master.sqlite3");
    let protector = TestProtector;
    let authority = IdentityAuthority::open_or_initialize(directory.path(), &protector, 1_000_000)
        .expect("initialize test identity authority");
    assert_eq!(authority.receipt().key_protection, "test_reverse_v1");
    assert!(authority.receipt().protected_ca_key_path.is_file());
    assert!(authority.receipt().ca_certificate_path.is_file());

    let mut master = MasterKernel::open(&database).expect("open master kernel");
    master
        .record_identity_authority(authority.receipt())
        .expect("record authority binding");
    let grant = master
        .create_enrollment_grant(spec(), 2_000_000)
        .expect("create enrollment grant");
    assert_eq!(grant.expires_at_ms, 2_000_000 + ENROLLMENT_GRANT_TTL_MS);
    assert_eq!(grant.registry_revision, 1);
    assert_eq!(grant.grant_secret.len(), 64);

    let wrong_request = EnrollmentRequest {
        grant_id: grant.grant_id,
        grant_secret: "0".repeat(64),
        csr_pem: csr("client-controlled-name"),
    };
    assert!(matches!(
        master.issue_device_certificate(&authority, &wrong_request, 2_000_001),
        Err(MasterError::InvalidEnrollmentGrantSecret)
    ));

    let request = EnrollmentRequest {
        grant_id: grant.grant_id,
        grant_secret: grant.grant_secret.clone(),
        csr_pem: csr("client-controlled-name"),
    };
    let first = master
        .issue_device_certificate(&authority, &request, 2_000_002)
        .expect("issue first device certificate");
    assert_eq!(
        first.grant_id, None,
        "normal enrollment receipt shape stays grant-free"
    );
    assert_eq!(first.device_id, grant.device_id);
    assert_eq!(first.device_name, "owner-mac-bridge");
    assert_eq!(first.role, DeviceRole::MacBridge);
    assert_eq!(
        first.not_after_ms,
        first.issued_at_ms + DEVICE_CERTIFICATE_LIFETIME_MS
    );
    assert!(first.certificate_pem.contains("BEGIN CERTIFICATE"));
    assert!(first.ca_certificate_pem.contains("BEGIN CERTIFICATE"));
    let (_, issued_pem) = x509_parser::pem::parse_x509_pem(first.certificate_pem.as_bytes())
        .expect("parse issued certificate PEM");
    let (_, issued_certificate) =
        X509Certificate::from_der(&issued_pem.contents).expect("parse issued certificate DER");
    let (_, ca_pem) = x509_parser::pem::parse_x509_pem(first.ca_certificate_pem.as_bytes())
        .expect("parse CA certificate PEM");
    let (_, ca_certificate) =
        X509Certificate::from_der(&ca_pem.contents).expect("parse CA certificate DER");
    issued_certificate
        .verify_signature(Some(&ca_certificate.tbs_certificate.subject_pki))
        .expect("verify issued certificate under persisted CA");
    assert_eq!(
        issued_certificate
            .subject()
            .iter_common_name()
            .next()
            .expect("issued CN")
            .as_str()
            .expect("UTF-8 issued CN"),
        "owner-mac-bridge"
    );
    let san = issued_certificate
        .subject_alternative_name()
        .expect("decode SAN")
        .expect("issued SAN");
    // The `urn:assemblywright:device:` SAN prefix is a frozen credential contract, not
    // leftover naming drift. It is baked into every certificate already issued to
    // an enrolled device, and the master strips exactly this prefix when it
    // verifies a presented client certificate, so renaming it voids those
    // certificates and fails enrollment closed. The Assemblywright rename left it
    // deliberately unchanged; see docs/brand.md "Compatibility Names".
    let issued_uri = format!("urn:assemblywright:device:{}", first.device_id.0);
    assert!(issued_uri.starts_with("urn:assemblywright:device:"));
    assert!(san.value.general_names.iter().any(|name| matches!(
        name,
        GeneralName::URI(uri) if *uri == issued_uri
    )));
    assert!(master
        .certificate_is_active(first.device_id, &first.serial_hex, 2_000_003)
        .expect("inspect active certificate"));
    assert!(matches!(
        master.issue_device_certificate(&authority, &request, 2_000_004),
        Err(MasterError::EnrollmentGrantConsumed)
    ));

    let (rotation, rotation_registration) = master
        .create_rotation_pairing_grant(first.device_id, 3_000_000)
        .expect("create interactive rotation grant");
    assert_eq!(rotation_registration.device_id, first.device_id);
    assert_eq!(rotation_registration.device_name, first.device_name);
    assert_eq!(rotation_registration.role, first.role);
    assert_eq!(
        rotation_registration.registry_revision,
        first.registry_revision
    );
    assert_eq!(rotation_registration.capabilities, spec().capabilities);
    let rotated = master
        .issue_device_certificate(
            &authority,
            &EnrollmentRequest {
                grant_id: rotation.grant_id,
                grant_secret: rotation.grant_secret,
                csr_pem: csr("another-client-controlled-name"),
            },
            3_000_001,
        )
        .expect("rotate device certificate");
    assert_eq!(rotated.grant_id, Some(rotation.grant_id));
    assert_eq!(rotated.device_id, first.device_id);
    assert_ne!(rotated.serial_hex, first.serial_hex);
    assert!(!master
        .certificate_is_active(first.device_id, &first.serial_hex, 3_000_002)
        .expect("old certificate is inactive"));
    assert!(master
        .certificate_is_active(rotated.device_id, &rotated.serial_hex, 3_000_002)
        .expect("new certificate is active"));

    master
        .revoke_device_with_reason(rotated.device_id, 4_000_000, "owner_requested")
        .expect("revoke device and active certificate");
    assert!(!master
        .certificate_is_active(rotated.device_id, &rotated.serial_hex, 4_000_001)
        .expect("revoked certificate is inactive"));
    assert!(master
        .create_rotation_grant(rotated.device_id, 4_000_002)
        .is_err());

    drop(master);
    let database_bytes = std::fs::read(&database).expect("read SQLite file");
    assert!(
        !database_bytes
            .windows(grant.grant_secret.len())
            .any(|window| window == grant.grant_secret.as_bytes()),
        "raw enrollment secret persisted in SQLite"
    );
}

#[test]
fn rotation_precommit_failure_rolls_back_and_recovery_validation_is_exact() {
    let directory = tempfile::tempdir().expect("rotation precommit directory");
    let authority =
        IdentityAuthority::open_or_initialize(directory.path(), &TestProtector, 1_000_000)
            .expect("initialize authority");
    let mut master = MasterKernel::in_memory().expect("in-memory master");
    master
        .record_identity_authority(authority.receipt())
        .expect("record authority");
    let enrollment = master
        .create_enrollment_grant(spec(), 2_000_000)
        .expect("enrollment grant");
    let active = master
        .issue_device_certificate(
            &authority,
            &EnrollmentRequest {
                grant_id: enrollment.grant_id,
                grant_secret: enrollment.grant_secret,
                csr_pem: csr("rotation-precommit"),
            },
            2_000_001,
        )
        .expect("active certificate");
    let rotation = master
        .create_rotation_grant(active.device_id, 2_000_002)
        .expect("rotation grant");
    let request = EnrollmentRequest {
        grant_id: rotation.grant_id,
        grant_secret: rotation.grant_secret,
        csr_pem: csr("rotation-precommit"),
    };
    let rejected = master.issue_device_certificate_with_precommit(
        &authority,
        &request,
        2_000_003,
        |receipt| {
            assert_eq!(receipt.grant_id, Some(rotation.grant_id));
            Err(MasterError::InvalidStoredState(
                "injected journal failure".to_string(),
            ))
        },
    );
    assert!(matches!(rejected, Err(MasterError::InvalidStoredState(_))));
    assert!(master
        .certificate_is_active(active.device_id, &active.serial_hex, 2_000_004)
        .expect("old certificate remains active"));

    let rotated = master
        .issue_device_certificate(&authority, &request, 2_000_005)
        .expect("grant remained retryable after precommit rollback");
    master
        .validate_rotation_recovery_receipt(&rotated, 2_000_006)
        .expect("exact committed receipt validates");
    let mut wrong_grant = rotated.clone();
    wrong_grant.grant_id = Some(uuid::Uuid::new_v4());
    assert!(master
        .validate_rotation_recovery_receipt(&wrong_grant, 2_000_006)
        .is_err());
    let mut wrong_pem = rotated.clone();
    wrong_pem.certificate_pem = active.certificate_pem;
    assert!(master
        .validate_rotation_recovery_receipt(&wrong_pem, 2_000_006)
        .is_err());
    assert!(master
        .validate_rotation_recovery_receipt(&rotated, rotated.not_after_ms)
        .is_err());
}

#[test]
fn expired_grant_and_invalid_csr_fail_without_consuming_the_grant() {
    let directory = tempfile::tempdir().expect("identity directory");
    let authority =
        IdentityAuthority::open_or_initialize(directory.path(), &TestProtector, 1_000_000)
            .expect("initialize authority");
    let mut master = MasterKernel::in_memory().expect("in-memory master");
    master
        .record_identity_authority(authority.receipt())
        .expect("record authority");

    let invalid_csr_grant = master
        .create_enrollment_grant(spec(), 2_000_000)
        .expect("create invalid-CSR grant");
    let invalid = EnrollmentRequest {
        grant_id: invalid_csr_grant.grant_id,
        grant_secret: invalid_csr_grant.grant_secret.clone(),
        csr_pem: "-----BEGIN CERTIFICATE REQUEST-----\ninvalid\n-----END CERTIFICATE REQUEST-----"
            .to_string(),
    };
    assert!(matches!(
        master.issue_device_certificate(&authority, &invalid, 2_000_001),
        Err(MasterError::Identity(
            IdentityError::InvalidCertificateRequest
        ))
    ));
    let recovered = master
        .issue_device_certificate(
            &authority,
            &EnrollmentRequest {
                csr_pem: csr("valid-after-invalid"),
                ..invalid
            },
            2_000_002,
        )
        .expect("valid CSR can still consume grant");
    assert_eq!(recovered.device_id, invalid_csr_grant.device_id);

    let expired = master
        .create_enrollment_grant(spec(), 3_000_000)
        .expect("create expiring grant");
    assert!(matches!(
        master.issue_device_certificate(
            &authority,
            &EnrollmentRequest {
                grant_id: expired.grant_id,
                grant_secret: expired.grant_secret,
                csr_pem: csr("expired"),
            },
            3_000_000 + ENROLLMENT_GRANT_TTL_MS,
        ),
        Err(MasterError::EnrollmentGrantExpired)
    ));
}

#[test]
fn interactive_rotation_rejects_fixture_registration_before_creating_a_grant() {
    let directory = tempfile::tempdir().expect("fixture rotation identity directory");
    let authority =
        IdentityAuthority::open_or_initialize(directory.path(), &TestProtector, 1_000_000)
            .expect("initialize authority");
    let mut master = MasterKernel::in_memory().expect("in-memory master");
    master
        .record_identity_authority(authority.receipt())
        .expect("record authority");
    let grant = master
        .create_enrollment_grant(fixture_spec(), 2_000_000)
        .expect("create fixture grant");
    let device_id = grant.device_id;
    master
        .issue_device_certificate(
            &authority,
            &EnrollmentRequest {
                grant_id: grant.grant_id,
                grant_secret: grant.grant_secret,
                csr_pem: csr("fixture-device"),
            },
            2_000_001,
        )
        .expect("issue fixture certificate");
    let before = master.health_snapshot().expect("health before rejection");

    assert!(matches!(
        master.create_rotation_pairing_grant(device_id, 2_000_002),
        Err(MasterError::InvalidEnrollmentGrant(message))
            if message.contains("non-fixture MacBridge")
    ));
    let after = master.health_snapshot().expect("health after rejection");
    assert_eq!(
        after.unconsumed_enrollment_grants, before.unconsumed_enrollment_grants,
        "rejected fixture rotation must not leave a digest-only grant"
    );
}

#[test]
fn capability_rebind_is_two_phase_stale_safe_replay_closed_and_preserves_old_certificate() {
    let directory = tempfile::tempdir().expect("rebind identity directory");
    let authority =
        IdentityAuthority::open_or_initialize(directory.path(), &TestProtector, 1_000_000)
            .expect("initialize authority");
    let mut master = MasterKernel::in_memory().expect("in-memory master");
    master
        .record_identity_authority(authority.receipt())
        .expect("record authority");
    let initial_grant = master
        .create_enrollment_grant(fixture_spec(), 2_000_000)
        .expect("create stale fixture enrollment");
    let initial = master
        .issue_device_certificate(
            &authority,
            &EnrollmentRequest {
                grant_id: initial_grant.grant_id,
                grant_secret: initial_grant.grant_secret,
                csr_pem: csr("initial-fixture"),
            },
            2_000_001,
        )
        .expect("issue initial fixture certificate");

    let target = spec().capabilities;
    let mixed = vec![target[0].clone(), CapabilityDescriptor::fixture_reasoning()];
    assert!(matches!(
        master.create_capability_rebind_grant(initial.device_id, mixed, 2_100_000),
        Err(MasterError::InvalidRemoteWorkContract) | Err(MasterError::InvalidEnrollmentGrant(_))
    ));
    let stale_grant = master
        .create_capability_rebind_grant(initial.device_id, target.clone(), 2_100_001)
        .expect("create stale replay probe");
    let grant = master
        .create_capability_rebind_grant(initial.device_id, target, 2_100_002)
        .expect("create capability rebind");
    assert_eq!(grant.registry_revision, initial.registry_revision + 1);
    let (replacement_csr, replacement_key) = csr_and_key("replacement-mlx");
    let pending = master
        .issue_pending_capability_rebind(
            &authority,
            &EnrollmentRequest {
                grant_id: grant.grant_id,
                grant_secret: grant.grant_secret.clone(),
                csr_pem: replacement_csr,
            },
            2_100_003,
        )
        .expect("issue registry-inactive pending certificate");
    assert!(master
        .certificate_is_active(initial.device_id, &initial.serial_hex, 2_100_004)
        .expect("old certificate remains active"));
    assert!(!master
        .certificate_is_active(initial.device_id, &pending.serial_hex, 2_100_004)
        .expect("pending certificate is not active"));
    assert!(matches!(
        master.issue_pending_capability_rebind(
            &authority,
            &EnrollmentRequest {
                grant_id: grant.grant_id,
                grant_secret: grant.grant_secret,
                csr_pem: csr("replay"),
            },
            2_100_004,
        ),
        Err(MasterError::EnrollmentGrantConsumed)
    ));

    let mut acknowledgement = CapabilityRebindAcknowledgement {
        status: "capability_rebind_certificate_staged".to_string(),
        grant_id: pending.grant_id,
        device_id: pending.device_id,
        registry_revision: pending.registry_revision,
        serial_hex: pending.serial_hex.clone(),
        certificate_sha256: pending.certificate_sha256.clone(),
        signature_algorithm: "ecdsa_p256_sha256_der".to_string(),
        signature_base64: String::new(),
    };
    acknowledgement.certificate_sha256 = "00".repeat(32);
    sign_rebind_acknowledgement(&mut acknowledgement, &replacement_key);
    assert!(master
        .activate_capability_rebind(&authority, &acknowledgement, 2_100_005)
        .is_err());
    assert!(master
        .certificate_is_active(initial.device_id, &initial.serial_hex, 2_100_006)
        .expect("bad acknowledgement preserves old certificate"));
    acknowledgement.certificate_sha256 = pending.certificate_sha256.to_uppercase();
    if acknowledgement.certificate_sha256 == pending.certificate_sha256 {
        acknowledgement.certificate_sha256.replace_range(..1, "A");
    }
    sign_rebind_acknowledgement(&mut acknowledgement, &replacement_key);
    assert!(matches!(
        master.activate_capability_rebind(&authority, &acknowledgement, 2_100_006),
        Err(MasterError::InvalidEnrollmentGrant(message))
            if message == "capability rebind acknowledgement is invalid"
    ));
    assert!(master
        .certificate_is_active(initial.device_id, &initial.serial_hex, 2_100_006)
        .expect("uppercase digest preserves old certificate"));
    assert!(!master
        .certificate_is_active(initial.device_id, &pending.serial_hex, 2_100_006)
        .expect("uppercase digest leaves replacement inactive"));
    acknowledgement.certificate_sha256 = pending.certificate_sha256.clone();
    let wrong_key = KeyPair::generate().expect("wrong replacement key");
    sign_rebind_acknowledgement(&mut acknowledgement, &wrong_key);
    assert!(master
        .activate_capability_rebind(&authority, &acknowledgement, 2_100_007)
        .is_err());
    assert!(master
        .certificate_is_active(initial.device_id, &initial.serial_hex, 2_100_007)
        .expect("wrong-key acknowledgement preserves old certificate"));
    sign_rebind_acknowledgement(&mut acknowledgement, &replacement_key);
    let activated = master
        .activate_capability_rebind(&authority, &acknowledgement, 2_100_008)
        .expect("activate exact staged replacement");
    assert_eq!(activated.registry_revision, initial.registry_revision + 1);
    assert!(!master
        .certificate_is_active(initial.device_id, &initial.serial_hex, 2_100_009)
        .expect("old certificate revoked after activation"));
    assert!(master
        .certificate_is_active(initial.device_id, &pending.serial_hex, 2_100_009)
        .expect("replacement certificate active after activation"));
    let retried = master
        .activate_capability_rebind(&authority, &acknowledgement, 2_100_010)
        .expect("lost activation output can be reissued");
    assert_eq!(retried.activated_at_ms, activated.activated_at_ms);
    assert_eq!(retried.grant_id, activated.grant_id);
    assert_ne!(retried.signature_base64, "");
    let mut mismatched_retry = acknowledgement.clone();
    mismatched_retry.certificate_sha256 = "11".repeat(32);
    sign_rebind_acknowledgement(&mut mismatched_retry, &replacement_key);
    assert!(master
        .activate_capability_rebind(&authority, &mismatched_retry, 2_100_011)
        .is_err());
    assert_eq!(activated.signature_algorithm, "ecdsa_p256_sha256_der");
    let (_, ca_pem) = x509_parser::pem::parse_x509_pem(initial.ca_certificate_pem.as_bytes())
        .expect("parse activation CA");
    let (_, ca) = X509Certificate::from_der(&ca_pem.contents).expect("parse activation CA DER");
    let verifying_key = VerifyingKey::from_sec1_bytes(
        ca.tbs_certificate
            .subject_pki
            .subject_public_key
            .data
            .as_ref(),
    )
    .expect("P-256 CA public key");
    let activation_signature = Signature::from_der(
        &base64::engine::general_purpose::STANDARD
            .decode(&activated.signature_base64)
            .expect("decode activation signature"),
    )
    .expect("parse activation signature");
    let activation_transcript = format!(
        "Assemblywright-Capability-Rebind-Activation-v1\ngrant_id={}\ndevice_id={}\nregistry_revision={}\nserial_hex={}\ncertificate_sha256={}\nactivated_at_ms={}\n",
        activated.grant_id,
        activated.device_id.0,
        activated.registry_revision,
        activated.serial_hex,
        activated.certificate_sha256,
        activated.activated_at_ms
    );
    verifying_key
        .verify(activation_transcript.as_bytes(), &activation_signature)
        .expect("activation receipt is signed by pinned CA");
    assert!(matches!(
        master.issue_pending_capability_rebind(
            &authority,
            &EnrollmentRequest {
                grant_id: stale_grant.grant_id,
                grant_secret: stale_grant.grant_secret,
                csr_pem: csr("stale"),
            },
            2_100_012,
        ),
        Err(MasterError::InvalidEnrollmentGrant(_))
    ));
}

#[test]
fn expired_pending_capability_rebind_cannot_activate_or_disable_working_identity() {
    let directory = tempfile::tempdir().expect("expired rebind identity directory");
    let authority =
        IdentityAuthority::open_or_initialize(directory.path(), &TestProtector, 1_000_000)
            .expect("initialize authority");
    let mut master = MasterKernel::in_memory().expect("in-memory master");
    master
        .record_identity_authority(authority.receipt())
        .expect("record authority");
    let enrolled = master
        .create_enrollment_grant(fixture_spec(), 2_000_000)
        .expect("create fixture enrollment");
    let active = master
        .issue_device_certificate(
            &authority,
            &EnrollmentRequest {
                grant_id: enrolled.grant_id,
                grant_secret: enrolled.grant_secret,
                csr_pem: csr("active-fixture"),
            },
            2_000_001,
        )
        .expect("issue active fixture certificate");
    let grant = master
        .create_capability_rebind_grant(active.device_id, spec().capabilities, 2_100_000)
        .expect("create rebind grant");
    let expires_at_ms = grant.expires_at_ms;
    let (expired_csr, expired_key) = csr_and_key("expired-pending");
    let pending = master
        .issue_pending_capability_rebind(
            &authority,
            &EnrollmentRequest {
                grant_id: grant.grant_id,
                grant_secret: grant.grant_secret,
                csr_pem: expired_csr,
            },
            2_100_001,
        )
        .expect("issue pending replacement");
    let mut acknowledgement = CapabilityRebindAcknowledgement {
        status: "capability_rebind_certificate_staged".to_string(),
        grant_id: pending.grant_id,
        device_id: pending.device_id,
        registry_revision: pending.registry_revision,
        serial_hex: pending.serial_hex.clone(),
        certificate_sha256: pending.certificate_sha256,
        signature_algorithm: "ecdsa_p256_sha256_der".to_string(),
        signature_base64: String::new(),
    };
    sign_rebind_acknowledgement(&mut acknowledgement, &expired_key);
    master
        .set_emergency_paused_at(true, 2_100_002)
        .expect("pause before activation");
    assert!(matches!(
        master.activate_capability_rebind(&authority, &acknowledgement, 2_100_003),
        Err(MasterError::EmergencyPaused)
    ));
    assert!(master
        .certificate_is_active(active.device_id, &active.serial_hex, 2_100_003)
        .expect("pause preserves working certificate"));
    assert!(!master
        .certificate_is_active(active.device_id, &pending.serial_hex, 2_100_003)
        .expect("pause leaves pending certificate inactive"));
    master
        .set_emergency_paused_at(false, 2_100_004)
        .expect("resume after activation rejection");
    assert!(matches!(
        master.activate_capability_rebind(&authority, &acknowledgement, expires_at_ms),
        Err(MasterError::EnrollmentGrantExpired)
    ));
    assert!(master
        .certificate_is_active(active.device_id, &active.serial_hex, expires_at_ms)
        .expect("working certificate remains active"));
    assert!(!master
        .certificate_is_active(active.device_id, &pending.serial_hex, expires_at_ms)
        .expect("expired pending certificate was never activated"));
    master
        .abort_capability_rebind(pending.grant_id, expires_at_ms + 1)
        .expect("abort expired pending rebind");
    assert!(master
        .activate_capability_rebind(&authority, &acknowledgement, expires_at_ms + 2)
        .is_err());
    assert!(master
        .certificate_is_active(active.device_id, &active.serial_hex, expires_at_ms + 2)
        .expect("abort preserves working certificate"));
}

#[test]
fn capability_rebind_audit_is_redacted_immutable_and_rolls_back_with_authority() {
    let directory = tempfile::tempdir().expect("rebind audit directory");
    let database = directory.path().join("master.sqlite3");
    let authority =
        IdentityAuthority::open_or_initialize(directory.path(), &TestProtector, 1_000_000)
            .expect("initialize authority");
    let mut master = MasterKernel::open(&database).expect("open master");
    master
        .record_identity_authority(authority.receipt())
        .expect("record authority");

    let enroll_fixture = |master: &mut MasterKernel, name: &str, now_ms: u64| {
        let mut fixture = fixture_spec();
        fixture.device_name = name.to_string();
        let grant = master
            .create_enrollment_grant(fixture, now_ms)
            .expect("create fixture enrollment");
        master
            .issue_device_certificate(
                &authority,
                &EnrollmentRequest {
                    grant_id: grant.grant_id,
                    grant_secret: grant.grant_secret,
                    csr_pem: csr(name),
                },
                now_ms + 1,
            )
            .expect("issue fixture identity")
    };
    let active_device = enroll_fixture(&mut master, "audit-active", 2_000_000);
    let abort_device = enroll_fixture(&mut master, "audit-abort", 2_010_000);
    let rollback_device = enroll_fixture(&mut master, "audit-rollback", 2_020_000);

    let active_grant = master
        .create_capability_rebind_grant(active_device.device_id, spec().capabilities, 2_100_000)
        .expect("create audited activation grant");
    let (active_csr, active_key) = csr_and_key("audit-active-replacement");
    let active_pending = master
        .issue_pending_capability_rebind(
            &authority,
            &EnrollmentRequest {
                grant_id: active_grant.grant_id,
                grant_secret: active_grant.grant_secret,
                csr_pem: active_csr,
            },
            2_100_001,
        )
        .expect("issue audited pending certificate");
    let mut active_ack = CapabilityRebindAcknowledgement {
        status: "capability_rebind_certificate_staged".to_string(),
        grant_id: active_pending.grant_id,
        device_id: active_pending.device_id,
        registry_revision: active_pending.registry_revision,
        serial_hex: active_pending.serial_hex,
        certificate_sha256: active_pending.certificate_sha256,
        signature_algorithm: "ecdsa_p256_sha256_der".to_string(),
        signature_base64: String::new(),
    };
    sign_rebind_acknowledgement(&mut active_ack, &active_key);
    master
        .activate_capability_rebind(&authority, &active_ack, 2_100_002)
        .expect("activate with audit");

    let abort_grant = master
        .create_capability_rebind_grant(abort_device.device_id, spec().capabilities, 2_110_000)
        .expect("create audited abort grant");
    let abort_pending = master
        .issue_pending_capability_rebind(
            &authority,
            &EnrollmentRequest {
                grant_id: abort_grant.grant_id,
                grant_secret: abort_grant.grant_secret,
                csr_pem: csr("audit-abort-replacement"),
            },
            2_110_001,
        )
        .expect("issue abort pending certificate");
    master
        .abort_capability_rebind(abort_pending.grant_id, 2_110_002)
        .expect("abort with audit");
    drop(master);

    let connection = rusqlite::Connection::open(&database).expect("inspect audit database");
    let rows = connection
        .prepare(
            "SELECT event_kind, grant_id, device_id, registry_revision, occurred_at_ms
             FROM master_identity_rebind_audit ORDER BY audit_id",
        )
        .expect("prepare audit query")
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, i64>(4)?,
            ))
        })
        .expect("query audit")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect audit");
    assert_eq!(
        rows.iter().map(|row| row.0.as_str()).collect::<Vec<_>>(),
        [
            "grant_created",
            "pending_issued",
            "activated",
            "grant_created",
            "pending_issued",
            "aborted"
        ]
    );
    assert!(rows
        .iter()
        .all(|row| { !row.1.is_empty() && !row.2.is_empty() && row.3 > 1 && row.4 > 0 }));
    let columns = connection
        .prepare("PRAGMA table_info(master_identity_rebind_audit)")
        .expect("prepare audit schema")
        .query_map([], |row| row.get::<_, String>(1))
        .expect("query audit schema")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect audit columns");
    assert_eq!(
        columns,
        [
            "audit_id",
            "event_kind",
            "grant_id",
            "device_id",
            "registry_revision",
            "occurred_at_ms"
        ]
    );
    assert!(connection
        .execute(
            "UPDATE master_identity_rebind_audit SET occurred_at_ms = 0 WHERE audit_id = 1",
            [],
        )
        .is_err());
    connection
        .execute_batch(
            "CREATE TRIGGER test_rebind_audit_insert_failure
             BEFORE INSERT ON master_identity_rebind_audit
             WHEN NEW.event_kind = 'grant_created'
             BEGIN SELECT RAISE(ABORT, 'test audit failure'); END;",
        )
        .expect("install audit failure trigger");
    let rebind_grants_before: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM master_enrollment_grants
             WHERE operation = 'capability_rebind'",
            [],
            |row| row.get(0),
        )
        .expect("count rebind grants");
    drop(connection);

    let mut master = MasterKernel::open(&database).expect("reopen master");
    assert!(master
        .create_capability_rebind_grant(rollback_device.device_id, spec().capabilities, 2_120_000,)
        .is_err());
    drop(master);
    let connection = rusqlite::Connection::open(&database).expect("verify rollback");
    let rebind_grants_after: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM master_enrollment_grants
             WHERE operation = 'capability_rebind'",
            [],
            |row| row.get(0),
        )
        .expect("count post-rollback rebind grants");
    assert_eq!(rebind_grants_after, rebind_grants_before);
}

#[test]
fn schema_v1_migrates_transactionally_through_capability_rebind_v7() {
    let directory = tempfile::tempdir().expect("migration directory");
    let database = directory.path().join("master.sqlite3");
    let connection = rusqlite::Connection::open(&database).expect("create v1 database");
    connection
        .execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE master_metadata (
               key TEXT PRIMARY KEY NOT NULL,
               integer_value INTEGER NOT NULL
             );
             INSERT INTO master_metadata (key, integer_value) VALUES ('next_connection_epoch', 0);
             CREATE TABLE master_devices (
               device_id TEXT PRIMARY KEY NOT NULL,
               device_name TEXT NOT NULL,
               role_json TEXT NOT NULL,
               registry_revision INTEGER NOT NULL,
               capabilities_json TEXT NOT NULL,
               revoked INTEGER NOT NULL CHECK (revoked IN (0, 1))
             );
             INSERT INTO master_devices VALUES (
               '11111111-1111-4111-8111-111111111111',
               'existing-worker',
               '\"inference_worker\"',
               1,
               '[]',
               0
             );
             CREATE TABLE master_connections (
               device_id TEXT PRIMARY KEY NOT NULL REFERENCES master_devices(device_id),
               connection_epoch INTEGER NOT NULL UNIQUE,
               active INTEGER NOT NULL CHECK (active IN (0, 1)),
               last_sequence INTEGER NOT NULL,
               connected_at_ms INTEGER NOT NULL,
               disconnected_at_ms INTEGER
             );
             CREATE TABLE master_steps (
               task_id TEXT NOT NULL,
               step_id TEXT PRIMARY KEY NOT NULL,
               status TEXT NOT NULL,
               capability_id TEXT NOT NULL,
               sensitivity_json TEXT NOT NULL,
               context_json TEXT NOT NULL,
               context_sha256 BLOB NOT NULL,
               lease_duration_ms INTEGER NOT NULL,
               deadline_after_ms INTEGER NOT NULL,
               created_at_ms INTEGER NOT NULL,
               accepted_payload_json TEXT,
               accepted_payload_sha256 BLOB,
               completed_at_ms INTEGER
             );
             CREATE TABLE master_attempts (
               attempt_id TEXT PRIMARY KEY NOT NULL,
               step_id TEXT NOT NULL REFERENCES master_steps(step_id),
               device_id TEXT NOT NULL REFERENCES master_devices(device_id),
               connection_epoch INTEGER NOT NULL,
               lease_id TEXT NOT NULL UNIQUE,
               cancellation_id TEXT NOT NULL UNIQUE,
               status TEXT NOT NULL,
               job_json TEXT NOT NULL,
               leased_at_ms INTEGER NOT NULL,
               lease_expires_at_ms INTEGER NOT NULL,
               completed_at_ms INTEGER,
               result_sequence INTEGER,
               payload_sha256 BLOB
             );
             PRAGMA user_version = 1;",
        )
        .expect("write v1 schema");
    drop(connection);

    let process = MasterProcess::acquire(directory.path()).expect("migrate v1 database");
    let master = process.kernel();
    assert_eq!(
        master.schema_version().expect("schema version"),
        assemblywright_master::MASTER_SCHEMA_VERSION
    );
    let health = master.health_snapshot().expect("migrated health");
    assert_eq!(health.registered_devices, 1);
    assert_eq!(health.active_device_certificates, 0);
    assert_eq!(health.unconsumed_enrollment_grants, 0);
    let events = master
        .distributed_events(&DistributedEventBatchRequest {
            protocol_version: PROTOCOL_VERSION,
            connection_epoch: 1,
            after: None,
            limit: 1,
        })
        .expect("read migrated event stream");
    assert!(!events.stream_id.is_nil());
    assert_eq!(events.next_sequence, 0);
    assert!(events.events.is_empty());
}

#[test]
fn authority_reload_rejects_expiry_and_a_mismatched_protected_key() {
    let first_directory = tempfile::tempdir().expect("first authority directory");
    let second_directory = tempfile::tempdir().expect("second authority directory");
    let first =
        IdentityAuthority::open_or_initialize(first_directory.path(), &TestProtector, 1_000_000)
            .expect("first authority");
    let expires_at_ms = first.receipt().not_after_ms;
    drop(first);
    assert!(matches!(
        IdentityAuthority::open_or_initialize(
            first_directory.path(),
            &TestProtector,
            expires_at_ms,
        ),
        Err(IdentityError::AuthorityExpired)
    ));

    let second =
        IdentityAuthority::open_or_initialize(second_directory.path(), &TestProtector, 1_000_000)
            .expect("second authority");
    let replacement_key =
        std::fs::read(&second.receipt().protected_ca_key_path).expect("read second protected key");
    drop(second);
    std::fs::write(
        first_directory.path().join("identity/ca-key.protected"),
        replacement_key,
    )
    .expect("replace first protected key with a different valid key");
    assert!(matches!(
        IdentityAuthority::open_or_initialize(first_directory.path(), &TestProtector, 1_000_001),
        Err(IdentityError::AuthorityMismatch)
    ));
}

#[test]
fn recorded_authority_with_missing_key_material_never_regenerates_silently() {
    let directory = tempfile::tempdir().expect("authority directory");
    let database = directory.path().join("master.sqlite3");
    let authority =
        IdentityAuthority::open_or_initialize(directory.path(), &TestProtector, 1_000_000)
            .expect("initialize authority");
    let mut master = MasterKernel::open(&database).expect("open master");
    master
        .record_identity_authority(authority.receipt())
        .expect("record authority");
    assert!(master
        .identity_authority_recorded()
        .expect("authority record status"));
    drop(authority);
    std::fs::remove_dir_all(directory.path().join("identity"))
        .expect("simulate missing independently protected authority files");

    assert!(matches!(
        IdentityAuthority::open_existing(directory.path(), &TestProtector, 1_000_001),
        Err(IdentityError::PartialAuthority)
    ));
    assert!(!directory.path().join("identity").exists());
}

#[cfg(windows)]
#[test]
fn windows_dpapi_protector_round_trips_without_plaintext_equivalence() {
    use assemblywright_master::PlatformSecretProtector;

    let protector = PlatformSecretProtector;
    let plaintext = b"assemblywright enrollment authority test key";
    let protected = protector.protect(plaintext).expect("DPAPI protect");
    assert_ne!(protected, plaintext);
    let recovered = protector.unprotect(&protected).expect("DPAPI unprotect");
    assert_eq!(recovered, plaintext);
}

#[cfg(windows)]
#[test]
fn windows_enrollment_cli_uses_stdin_for_grant_secrets_and_emits_bounded_receipts() {
    use serde_json::Value;
    use std::io::Write as _;
    use std::process::{Command, Stdio};

    let binary = env!("CARGO_BIN_EXE_assemblywright-master");
    let directory = tempfile::tempdir().expect("CLI identity directory");
    let data_dir = directory.path().to_string_lossy().into_owned();
    let initialize = Command::new(binary)
        .args(["--data-dir", &data_dir, "enrollment", "initialize"])
        .output()
        .expect("run enrollment initialize");
    assert!(
        initialize.status.success(),
        "initialize failed: {}",
        String::from_utf8_lossy(&initialize.stderr)
    );
    let initialize_receipt: Value =
        serde_json::from_slice(&initialize.stdout).expect("initialize JSON receipt");
    assert_eq!(
        initialize_receipt["key_protection"],
        "windows_dpapi_current_user"
    );
    let protected_key =
        std::fs::read(directory.path().join("identity/ca-key.protected")).expect("read DPAPI blob");
    assert!(!protected_key
        .windows(b"PRIVATE KEY".len())
        .any(|window| window == b"PRIVATE KEY"));

    let capabilities_path = directory.path().join("capabilities.json");
    std::fs::write(
        &capabilities_path,
        serde_json::to_vec(&spec().capabilities).expect("capability JSON"),
    )
    .expect("write capability fixture");
    let capabilities_path = capabilities_path.to_string_lossy().into_owned();
    let grant = Command::new(binary)
        .args([
            "--data-dir",
            &data_dir,
            "enrollment",
            "grant",
            "--device-name",
            "cli-mac-bridge",
            "--role",
            "mac-bridge",
            "--capabilities-file",
            &capabilities_path,
            "--confirm",
        ])
        .output()
        .expect("run enrollment grant");
    assert!(
        grant.status.success(),
        "grant failed: {}",
        String::from_utf8_lossy(&grant.stderr)
    );
    let grant_receipt: Value = serde_json::from_slice(&grant.stdout).expect("grant JSON receipt");
    let grant_secret = grant_receipt["grant_secret"]
        .as_str()
        .expect("grant secret")
        .to_string();
    let request = serde_json::json!({
        "grant_id": grant_receipt["grant_id"],
        "grant_secret": grant_secret,
        "csr_pem": csr("cli-request"),
    });
    let mut issue = Command::new(binary)
        .args([
            "--data-dir",
            &data_dir,
            "enrollment",
            "issue",
            "--request-stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn enrollment issue");
    issue
        .stdin
        .take()
        .expect("issue stdin")
        .write_all(&serde_json::to_vec(&request).expect("request JSON"))
        .expect("write secret-bearing request only to stdin");
    let issued = issue.wait_with_output().expect("wait for enrollment issue");
    assert!(
        issued.status.success(),
        "issue failed: {}",
        String::from_utf8_lossy(&issued.stderr)
    );
    let issued_receipt: Value =
        serde_json::from_slice(&issued.stdout).expect("issued certificate receipt");
    assert_eq!(issued_receipt["status"], "device_certificate_issued");
    assert!(issued_receipt["certificate_pem"]
        .as_str()
        .expect("certificate PEM")
        .contains("BEGIN CERTIFICATE"));

    let database =
        std::fs::read(directory.path().join("master.sqlite3")).expect("read enrollment database");
    assert!(!database
        .windows(grant_secret.len())
        .any(|window| window == grant_secret.as_bytes()));

    let missing_stdin_ack = Command::new(binary)
        .args(["--data-dir", &data_dir, "enrollment", "issue"])
        .output()
        .expect("run issue without stdin acknowledgement");
    assert!(!missing_stdin_ack.status.success());
    assert!(String::from_utf8_lossy(&missing_stdin_ack.stderr).contains("--request-stdin"));
}

#[cfg(windows)]
#[test]
fn windows_pair_and_rebind_cli_keep_secrets_on_stdin_and_activate_exact_replacement() {
    use assemblywright_protocol::{EnrollmentCsrReply, EnrollmentInvitation};
    use std::io::{BufRead as _, BufReader, Read as _, Write as _};
    use std::process::{Command, Stdio};

    let binary = env!("CARGO_BIN_EXE_assemblywright-master");
    let directory = tempfile::tempdir().expect("pairing identity directory");
    let data_dir = directory.path().to_string_lossy().into_owned();
    let capabilities_path = directory.path().join("capabilities.json");
    std::fs::write(
        &capabilities_path,
        serde_json::to_vec(&fixture_spec().capabilities).expect("capability JSON"),
    )
    .expect("write capability fixture");
    let capabilities_path = capabilities_path.to_string_lossy().into_owned();

    let unconfirmed_data = directory.path().join("unconfirmed-master");
    let unconfirmed_data_arg = unconfirmed_data.to_string_lossy().into_owned();
    let unconfirmed = Command::new(binary)
        .args([
            "--data-dir",
            &unconfirmed_data_arg,
            "enrollment",
            "pair",
            "--device-name",
            "cli-mac-bridge",
            "--role",
            "mac-bridge",
            "--capabilities-file",
            &capabilities_path,
            "--master-endpoint",
            "100.64.23.14:7792",
        ])
        .output()
        .expect("run unconfirmed pair");
    assert!(!unconfirmed.status.success());
    assert!(String::from_utf8_lossy(&unconfirmed.stderr).contains("--confirm"));
    assert!(
        !unconfirmed_data.exists(),
        "unconfirmed pairing must not create authority or database state"
    );

    let spawn_pair = || {
        Command::new(binary)
            .args([
                "--data-dir",
                &data_dir,
                "enrollment",
                "pair",
                "--device-name",
                "cli-mac-bridge",
                "--role",
                "mac-bridge",
                "--capabilities-file",
                &capabilities_path,
                "--master-endpoint",
                "100.64.23.14:7792",
                "--confirm",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn enrollment pair")
    };

    let mut pair = spawn_pair();
    let mut stdout = BufReader::new(pair.stdout.take().expect("pair stdout"));
    let mut invitation_line = String::new();
    stdout
        .read_line(&mut invitation_line)
        .expect("read flushed invitation");
    assert!(!invitation_line.contains("grant_secret"));
    let invitation = EnrollmentInvitation::decode_frame(invitation_line.as_bytes())
        .expect("decode secret-free invitation");
    let reply = EnrollmentCsrReply {
        schema_version: 1,
        status: "enrollment_csr_ready".to_string(),
        grant_id: invitation.grant_id,
        device_id: invitation.device_id,
        csr_pem: csr("pair-cli-request"),
    };
    pair.stdin
        .take()
        .expect("pair stdin")
        .write_all(&serde_json::to_vec(&reply).expect("reply JSON"))
        .expect("write CSR reply");
    let mut issued_line = String::new();
    stdout
        .read_to_string(&mut issued_line)
        .expect("read issued receipt");
    let status = pair.wait().expect("wait for successful pair");
    assert!(status.success());
    assert!(!issued_line.contains("grant_secret"));
    let issued: serde_json::Value =
        serde_json::from_str(&issued_line).expect("issued certificate receipt");
    assert_eq!(issued["status"], "device_certificate_issued");

    let mut mismatch = spawn_pair();
    let mut mismatch_stdout = BufReader::new(mismatch.stdout.take().expect("mismatch stdout"));
    let mut mismatch_invitation_line = String::new();
    mismatch_stdout
        .read_line(&mut mismatch_invitation_line)
        .expect("read mismatch invitation");
    let mismatch_invitation =
        EnrollmentInvitation::decode_frame(mismatch_invitation_line.as_bytes())
            .expect("decode mismatch invitation");
    let wrong_reply = EnrollmentCsrReply {
        schema_version: 1,
        status: "enrollment_csr_ready".to_string(),
        grant_id: mismatch_invitation.grant_id,
        device_id: assemblywright_protocol::DeviceId::new(uuid::Uuid::new_v4()),
        csr_pem: csr("wrong-device-request"),
    };
    mismatch
        .stdin
        .take()
        .expect("mismatch stdin")
        .write_all(&serde_json::to_vec(&wrong_reply).expect("wrong reply JSON"))
        .expect("write mismatched reply");
    let mut unexpected_receipt = String::new();
    mismatch_stdout
        .read_to_string(&mut unexpected_receipt)
        .expect("read mismatch stdout");
    let mut mismatch_stderr = String::new();
    mismatch
        .stderr
        .take()
        .expect("mismatch stderr")
        .read_to_string(&mut mismatch_stderr)
        .expect("read mismatch stderr");
    let mismatch_status = mismatch.wait().expect("wait for mismatch rejection");
    assert!(!mismatch_status.success());
    assert!(unexpected_receipt.is_empty());
    assert!(mismatch_stderr.contains("device_id"));

    let master =
        MasterKernel::open(directory.path().join("master.sqlite3")).expect("open pairing database");
    let health = master.health_snapshot().expect("pairing health");
    assert_eq!(health.registered_devices, 1);
    assert_eq!(health.unconsumed_enrollment_grants, 1);
    drop(master);

    let fixture_rotation = Command::new(binary)
        .args([
            "--data-dir",
            &data_dir,
            "enrollment",
            "rotate-pair",
            "--device-id",
            issued["device_id"].as_str().expect("paired device id"),
            "--master-endpoint",
            "100.64.23.14:7792",
            "--confirm",
        ])
        .output()
        .expect("reject fixture rotation");
    assert!(!fixture_rotation.status.success());
    assert!(fixture_rotation.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&fixture_rotation.stderr).contains("grant_secret"));

    let target_path = directory.path().join("mlx-capabilities.json");
    std::fs::write(
        &target_path,
        serde_json::to_vec(&spec().capabilities).expect("MLX capability JSON"),
    )
    .expect("write MLX capability fixture");
    let target_path = target_path.to_string_lossy().into_owned();
    let device_id = issued["device_id"].as_str().expect("paired device id");
    let mut rebind = Command::new(binary)
        .args([
            "--data-dir",
            &data_dir,
            "enrollment",
            "rebind-pair",
            "--device-id",
            device_id,
            "--capabilities-file",
            &target_path,
            "--master-endpoint",
            "100.64.23.14:7792",
            "--confirm",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn capability rebind pair");
    let mut rebind_stdout = BufReader::new(rebind.stdout.take().expect("rebind stdout"));
    let mut rebind_invitation_line = String::new();
    rebind_stdout
        .read_line(&mut rebind_invitation_line)
        .expect("read rebind invitation");
    assert!(!rebind_invitation_line.contains("grant_secret"));
    let rebind_invitation = EnrollmentInvitation::decode_frame(rebind_invitation_line.as_bytes())
        .expect("decode rebind invitation");
    assert_eq!(rebind_invitation.device_id.0.to_string(), device_id);
    assert_eq!(rebind_invitation.registry_revision, 2);
    let (rebind_csr, rebind_key) = csr_and_key("rebind-replacement");
    rebind
        .stdin
        .take()
        .expect("rebind stdin")
        .write_all(
            &serde_json::to_vec(&EnrollmentCsrReply {
                schema_version: 1,
                status: "enrollment_csr_ready".to_string(),
                grant_id: rebind_invitation.grant_id,
                device_id: rebind_invitation.device_id,
                csr_pem: rebind_csr,
            })
            .expect("rebind CSR"),
        )
        .expect("write rebind CSR");
    let mut pending_line = String::new();
    rebind_stdout
        .read_to_string(&mut pending_line)
        .expect("read pending rebind certificate");
    assert!(rebind.wait().expect("wait for rebind pair").success());
    let pending: serde_json::Value =
        serde_json::from_str(&pending_line).expect("pending rebind receipt");
    assert_eq!(pending["status"], "capability_rebind_certificate_pending");
    let pending_probe_at = pending["issued_at_ms"]
        .as_u64()
        .expect("pending issued time")
        + 1;
    let pending_device = assemblywright_protocol::DeviceId::new(
        uuid::Uuid::parse_str(device_id).expect("paired device UUID"),
    );
    let master =
        MasterKernel::open(directory.path().join("master.sqlite3")).expect("open pending registry");
    assert!(master
        .certificate_is_active(
            pending_device,
            issued["serial_hex"].as_str().expect("old serial"),
            pending_probe_at,
        )
        .expect("old certificate stays active before activation"));
    assert!(!master
        .certificate_is_active(
            pending_device,
            pending["serial_hex"].as_str().expect("pending serial"),
            pending_probe_at,
        )
        .expect("pending certificate is not authenticating"));
    drop(master);
    let mut acknowledgement = CapabilityRebindAcknowledgement {
        status: "capability_rebind_certificate_staged".to_string(),
        grant_id: uuid::Uuid::parse_str(pending["grant_id"].as_str().expect("pending grant id"))
            .expect("pending grant UUID"),
        device_id: pending_device,
        registry_revision: pending["registry_revision"]
            .as_u64()
            .expect("pending revision"),
        serial_hex: pending["serial_hex"]
            .as_str()
            .expect("pending serial")
            .to_string(),
        certificate_sha256: pending["certificate_sha256"]
            .as_str()
            .expect("pending certificate digest")
            .to_string(),
        signature_algorithm: "ecdsa_p256_sha256_der".to_string(),
        signature_base64: String::new(),
    };
    sign_rebind_acknowledgement(&mut acknowledgement, &rebind_key);
    let mut activate = Command::new(binary)
        .args([
            "--data-dir",
            &data_dir,
            "enrollment",
            "rebind-activate",
            "--acknowledgement-stdin",
            "--confirm",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn rebind activation");
    activate
        .stdin
        .take()
        .expect("activation stdin")
        .write_all(&serde_json::to_vec(&acknowledgement).expect("acknowledgement JSON"))
        .expect("write staged-certificate acknowledgement");
    let activated = activate.wait_with_output().expect("wait for activation");
    assert!(
        activated.status.success(),
        "activation failed: {}",
        String::from_utf8_lossy(&activated.stderr)
    );
    let activation: serde_json::Value =
        serde_json::from_slice(&activated.stdout).expect("activation receipt");
    assert_eq!(activation["status"], "capability_rebind_activated");
    assert_eq!(activation["registry_revision"], 2);
    let activated_at = activation["activated_at_ms"]
        .as_u64()
        .expect("activation time");
    let master = MasterKernel::open(directory.path().join("master.sqlite3"))
        .expect("open activated registry");
    assert!(!master
        .certificate_is_active(
            pending_device,
            issued["serial_hex"].as_str().expect("old serial"),
            activated_at + 1,
        )
        .expect("old certificate revoked"));
    assert!(master
        .certificate_is_active(
            pending_device,
            pending["serial_hex"].as_str().expect("replacement serial"),
            activated_at + 1,
        )
        .expect("replacement certificate active"));
    drop(master);

    let mut rotate = Command::new(binary)
        .args([
            "--data-dir",
            &data_dir,
            "enrollment",
            "rotate-pair",
            "--device-id",
            device_id,
            "--master-endpoint",
            "100.64.23.14:7792",
            "--confirm",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn same-device rotation pair");
    let mut rotate_stdout = BufReader::new(rotate.stdout.take().expect("rotation stdout"));
    let mut rotation_invitation_line = String::new();
    rotate_stdout
        .read_line(&mut rotation_invitation_line)
        .expect("read rotation invitation");
    assert!(!rotation_invitation_line.contains("grant_secret"));
    let rotation_invitation =
        EnrollmentInvitation::decode_frame(rotation_invitation_line.as_bytes())
            .expect("decode rotation invitation");
    assert_eq!(rotation_invitation.device_id, pending_device);
    assert_eq!(rotation_invitation.registry_revision, 2);
    assert_eq!(rotation_invitation.capabilities, spec().capabilities);
    rotate
        .stdin
        .take()
        .expect("rotation stdin")
        .write_all(
            &serde_json::to_vec(&EnrollmentCsrReply {
                schema_version: 1,
                status: "enrollment_csr_ready".to_string(),
                grant_id: rotation_invitation.grant_id,
                device_id: rotation_invitation.device_id,
                csr_pem: csr_with_key("rotation-current-key", &rebind_key),
            })
            .expect("rotation CSR"),
        )
        .expect("write rotation CSR");
    let mut rotation_receipt_line = String::new();
    rotate_stdout
        .read_to_string(&mut rotation_receipt_line)
        .expect("read rotation receipt");
    let rotation_status = rotate.wait().expect("wait for rotation");
    let mut rotation_stderr = String::new();
    rotate
        .stderr
        .take()
        .expect("rotation stderr")
        .read_to_string(&mut rotation_stderr)
        .expect("read rotation stderr");
    assert!(
        rotation_status.success(),
        "rotation failed: {rotation_stderr}"
    );
    assert!(!rotation_receipt_line.contains("grant_secret"));
    let rotation: serde_json::Value =
        serde_json::from_str(&rotation_receipt_line).expect("rotation receipt");
    assert_eq!(rotation["operation"], "rotate");
    assert_eq!(
        rotation["grant_id"],
        rotation_invitation.grant_id.to_string()
    );
    assert_eq!(rotation["device_id"], device_id);
    assert_eq!(rotation["registry_revision"], 2);
    let rotation_probe_at = rotation["issued_at_ms"]
        .as_u64()
        .expect("rotation issued time")
        + 1;
    let master =
        MasterKernel::open(directory.path().join("master.sqlite3")).expect("open rotated registry");
    assert!(!master
        .certificate_is_active(
            pending_device,
            pending["serial_hex"].as_str().expect("pre-rotation serial"),
            rotation_probe_at,
        )
        .expect("pre-rotation certificate inactive"));
    assert!(master
        .certificate_is_active(
            pending_device,
            rotation["serial_hex"].as_str().expect("rotation serial"),
            rotation_probe_at,
        )
        .expect("rotated certificate active"));
    drop(master);

    let direct_recovery = Command::new(binary)
        .args([
            "--data-dir",
            &data_dir,
            "enrollment",
            "rotate-recover",
            "--grant-id",
            &rotation_invitation.grant_id.to_string(),
            "--confirm",
        ])
        .output()
        .expect("recover directly delivered rotation receipt");
    assert!(direct_recovery.status.success());
    assert_eq!(direct_recovery.stdout, rotation_receipt_line.as_bytes());
    let direct_acknowledgement = Command::new(binary)
        .args([
            "--data-dir",
            &data_dir,
            "enrollment",
            "rotate-recover-acknowledge",
            "--grant-id",
            &rotation_invitation.grant_id.to_string(),
            "--confirm",
        ])
        .output()
        .expect("acknowledge directly delivered rotation receipt");
    assert!(direct_acknowledgement.status.success());

    // Closing the receipt pipe after accepting the public invitation simulates
    // the ambiguous post-commit failure boundary. The owner-private journal is
    // the only supported way to recover the exact committed receipt.
    let mut interrupted = Command::new(binary)
        .args([
            "--data-dir",
            &data_dir,
            "enrollment",
            "rotate-pair",
            "--device-id",
            device_id,
            "--master-endpoint",
            "100.64.23.14:7792",
            "--confirm",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn interrupted same-device rotation pair");
    let mut interrupted_stdout =
        BufReader::new(interrupted.stdout.take().expect("interrupted stdout"));
    let mut interrupted_invitation_line = String::new();
    interrupted_stdout
        .read_line(&mut interrupted_invitation_line)
        .expect("read interrupted rotation invitation");
    assert!(!interrupted_invitation_line.contains("grant_secret"));
    let interrupted_invitation =
        EnrollmentInvitation::decode_frame(interrupted_invitation_line.as_bytes())
            .expect("decode interrupted rotation invitation");
    drop(interrupted_stdout);
    interrupted
        .stdin
        .take()
        .expect("interrupted rotation stdin")
        .write_all(
            &serde_json::to_vec(&EnrollmentCsrReply {
                schema_version: 1,
                status: "enrollment_csr_ready".to_string(),
                grant_id: interrupted_invitation.grant_id,
                device_id: interrupted_invitation.device_id,
                csr_pem: csr_with_key("rotation-recovery-current-key", &rebind_key),
            })
            .expect("interrupted rotation CSR"),
        )
        .expect("write interrupted rotation CSR");
    let interrupted_output = interrupted
        .wait_with_output()
        .expect("wait for interrupted rotation");
    assert!(!interrupted_output.status.success());
    assert!(
        String::from_utf8_lossy(&interrupted_output.stderr).contains("confirmed rotate-recover")
    );
    assert!(!String::from_utf8_lossy(&interrupted_output.stderr).contains("grant_secret"));

    let unconfirmed_recovery = Command::new(binary)
        .args([
            "--data-dir",
            &data_dir,
            "enrollment",
            "rotate-recover",
            "--grant-id",
            &interrupted_invitation.grant_id.to_string(),
        ])
        .output()
        .expect("run unconfirmed rotation recovery");
    assert!(!unconfirmed_recovery.status.success());
    assert!(unconfirmed_recovery.stdout.is_empty());
    assert!(String::from_utf8_lossy(&unconfirmed_recovery.stderr).contains("--confirm"));

    let recovered = Command::new(binary)
        .args([
            "--data-dir",
            &data_dir,
            "enrollment",
            "rotate-recover",
            "--grant-id",
            &interrupted_invitation.grant_id.to_string(),
            "--confirm",
        ])
        .output()
        .expect("recover committed rotation receipt");
    assert!(
        recovered.status.success(),
        "rotation recovery failed: {}",
        String::from_utf8_lossy(&recovered.stderr)
    );
    assert!(!String::from_utf8_lossy(&recovered.stdout).contains("grant_secret"));
    let recovered_receipt: serde_json::Value =
        serde_json::from_slice(&recovered.stdout).expect("recovered rotation receipt");
    assert_eq!(recovered_receipt["operation"], "rotate");
    assert_eq!(
        recovered_receipt["grant_id"],
        interrupted_invitation.grant_id.to_string()
    );
    assert_eq!(recovered_receipt["device_id"], device_id);

    let recovered_again = Command::new(binary)
        .args([
            "--data-dir",
            &data_dir,
            "enrollment",
            "rotate-recover",
            "--grant-id",
            &interrupted_invitation.grant_id.to_string(),
            "--confirm",
        ])
        .output()
        .expect("repeat committed rotation receipt recovery");
    assert!(recovered_again.status.success());
    assert_eq!(
        recovered_again.stdout, recovered.stdout,
        "recovery remains byte-identical until explicit acknowledgement"
    );

    let recovered_at = recovered_receipt["issued_at_ms"]
        .as_u64()
        .expect("recovered issued time")
        + 1;
    let master = MasterKernel::open(directory.path().join("master.sqlite3"))
        .expect("open recovered registry");
    assert!(!master
        .certificate_is_active(
            pending_device,
            rotation["serial_hex"]
                .as_str()
                .expect("previous rotation serial"),
            recovered_at,
        )
        .expect("previous rotation certificate inactive"));
    assert!(master
        .certificate_is_active(
            pending_device,
            recovered_receipt["serial_hex"]
                .as_str()
                .expect("recovered rotation serial"),
            recovered_at,
        )
        .expect("recovered rotation certificate active"));
    drop(master);

    let unconfirmed_acknowledgement = Command::new(binary)
        .args([
            "--data-dir",
            &data_dir,
            "enrollment",
            "rotate-recover-acknowledge",
            "--grant-id",
            &interrupted_invitation.grant_id.to_string(),
        ])
        .output()
        .expect("run unconfirmed recovery acknowledgement");
    assert!(!unconfirmed_acknowledgement.status.success());
    assert!(unconfirmed_acknowledgement.stdout.is_empty());
    assert!(String::from_utf8_lossy(&unconfirmed_acknowledgement.stderr).contains("--confirm"));

    let acknowledged = Command::new(binary)
        .args([
            "--data-dir",
            &data_dir,
            "enrollment",
            "rotate-recover-acknowledge",
            "--grant-id",
            &interrupted_invitation.grant_id.to_string(),
            "--confirm",
        ])
        .output()
        .expect("acknowledge recovered rotation receipt");
    assert!(
        acknowledged.status.success(),
        "rotation recovery acknowledgement failed: {}",
        String::from_utf8_lossy(&acknowledged.stderr)
    );
    let acknowledgement: serde_json::Value =
        serde_json::from_slice(&acknowledged.stdout).expect("recovery acknowledgement receipt");
    assert_eq!(acknowledgement["status"], "rotation_recovery_acknowledged");
    assert_eq!(
        acknowledgement["grant_id"],
        interrupted_invitation.grant_id.to_string()
    );

    let consumed_recovery = Command::new(binary)
        .args([
            "--data-dir",
            &data_dir,
            "enrollment",
            "rotate-recover",
            "--grant-id",
            &interrupted_invitation.grant_id.to_string(),
            "--confirm",
        ])
        .output()
        .expect("retry consumed rotation recovery");
    assert!(!consumed_recovery.status.success());
    assert!(consumed_recovery.stdout.is_empty());
}
