use jarvis_master::{
    EnrollmentGrantSpec, EnrollmentRequest, IdentityAuthority, IdentityError, MasterError,
    MasterKernel, SecretProtector, DEVICE_CERTIFICATE_LIFETIME_MS, ENROLLMENT_GRANT_TTL_MS,
};
use jarvis_protocol::{
    CapabilityDescriptor, CapabilityKind, DeviceRole, DistributedEventBatchRequest,
    MAX_JOB_CONTEXT_BYTES, MAX_JOB_RESULT_BYTES, PROTOCOL_VERSION,
};
use rcgen::{CertificateParams, DistinguishedName, DnType, IsCa, KeyPair};
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

fn csr(common_name: &str) -> String {
    let key = KeyPair::generate().expect("generate client key");
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
    assert!(san.value.general_names.iter().any(|name| matches!(
        name,
        GeneralName::URI(uri) if *uri == format!("urn:jarvis:device:{}", first.device_id.0)
    )));
    assert!(master
        .certificate_is_active(first.device_id, &first.serial_hex, 2_000_003)
        .expect("inspect active certificate"));
    assert!(matches!(
        master.issue_device_certificate(&authority, &request, 2_000_004),
        Err(MasterError::EnrollmentGrantConsumed)
    ));

    let rotation = master
        .create_rotation_grant(first.device_id, 3_000_000)
        .expect("create rotation grant");
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
fn schema_v1_migrates_transactionally_to_enrollment_identity_and_event_cursor_v3() {
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

    let master = MasterKernel::open(&database).expect("migrate v1 database");
    assert_eq!(master.schema_version().expect("schema version"), 3);
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
    use jarvis_master::PlatformSecretProtector;

    let protector = PlatformSecretProtector;
    let plaintext = b"jarvis enrollment authority test key";
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

    let binary = env!("CARGO_BIN_EXE_jarvis-master");
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
fn windows_pair_cli_emits_no_secret_and_preserves_unconsumed_grant_on_mismatch() {
    use jarvis_protocol::{EnrollmentCsrReply, EnrollmentInvitation};
    use std::io::{BufRead as _, BufReader, Read as _, Write as _};
    use std::process::{Command, Stdio};

    let binary = env!("CARGO_BIN_EXE_jarvis-master");
    let directory = tempfile::tempdir().expect("pairing identity directory");
    let data_dir = directory.path().to_string_lossy().into_owned();
    let capabilities_path = directory.path().join("capabilities.json");
    std::fs::write(
        &capabilities_path,
        serde_json::to_vec(&spec().capabilities).expect("capability JSON"),
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
        device_id: jarvis_protocol::DeviceId::new(uuid::Uuid::new_v4()),
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
}
