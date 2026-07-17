use crate::{i64_to_u64, parse_uuid, u64_to_i64, DeviceRegistration, MasterError, MasterKernel};
use base64::Engine as _;
use jarvis_protocol::{
    CapabilityDescriptor, DeviceId, DeviceRole, HandshakeRequest, PROTOCOL_VERSION,
};
use rcgen::{
    BasicConstraints, CertificateParams, CertificateSigningRequestParams, CertifiedIssuer,
    DistinguishedName, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair, KeyUsagePurpose,
    SanType, SerialNumber, PKCS_ECDSA_P256_SHA256,
};
use rusqlite::{params, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use time::OffsetDateTime;
use uuid::Uuid;
use zeroize::Zeroizing;

pub const ENROLLMENT_GRANT_TTL_MS: u64 = 10 * 60 * 1_000;
pub const DEVICE_CERTIFICATE_LIFETIME_MS: u64 = 30 * 24 * 60 * 60 * 1_000;
pub const MAX_ENROLLED_DEVICES: u64 = 16;
const MAX_OUTSTANDING_ENROLLMENT_GRANTS: u64 = 32;
const MAX_CSR_PEM_BYTES: usize = 64 * 1_024;
const MAX_REVOCATION_REASON_BYTES: usize = 256;
const CA_LIFETIME_MS: u64 = 10 * 365 * 24 * 60 * 60 * 1_000;
const CLOCK_SKEW_MS: u64 = 5 * 60 * 1_000;
const IDENTITY_DIRECTORY: &str = "identity";
const CA_CERTIFICATE_FILE: &str = "ca.pem";
const PROTECTED_CA_KEY_FILE: &str = "ca-key.protected";
const AUTHORITY_METADATA_FILE: &str = "authority.json";

#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    #[error("identity filesystem error: {0}")]
    Io(#[from] std::io::Error),
    #[error("identity certificate error: {0}")]
    Certificate(#[from] rcgen::Error),
    #[error("identity metadata JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("identity key protection is unavailable: {0}")]
    ProtectionUnavailable(String),
    #[error("identity key protection failed: {0}")]
    ProtectionFailed(String),
    #[error("identity authority files are incomplete; refuse silent CA regeneration")]
    PartialAuthority,
    #[error("identity authority metadata does not match its certificate or protector")]
    AuthorityMismatch,
    #[error("identity authority is expired")]
    AuthorityExpired,
    #[error("identity timestamp is outside the supported certificate range")]
    InvalidTimestamp,
    #[error("identity certificate serial generation failed: {0}")]
    Random(String),
    #[error("certificate request must be valid signed PEM no larger than 64 KiB")]
    InvalidCertificateRequest,
    #[error("revocation reason must contain 1-256 non-control UTF-8 bytes")]
    InvalidRevocationReason,
}

pub trait SecretProtector {
    fn scheme(&self) -> &'static str;
    fn protect(&self, plaintext: &[u8]) -> Result<Vec<u8>, IdentityError>;
    fn unprotect(&self, ciphertext: &[u8]) -> Result<Vec<u8>, IdentityError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PlatformSecretProtector;

#[cfg(windows)]
impl SecretProtector for PlatformSecretProtector {
    fn scheme(&self) -> &'static str {
        "windows_dpapi_current_user"
    }

    fn protect(&self, plaintext: &[u8]) -> Result<Vec<u8>, IdentityError> {
        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::Security::Cryptography::{
            CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        };

        let input_length = u32::try_from(plaintext.len()).map_err(|_| {
            IdentityError::ProtectionFailed("plaintext exceeds DPAPI input range".to_string())
        })?;
        let input = CRYPT_INTEGER_BLOB {
            cbData: input_length,
            pbData: plaintext.as_ptr().cast_mut(),
        };
        let mut output = CRYPT_INTEGER_BLOB::default();
        let succeeded = unsafe {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if succeeded == 0 {
            return Err(IdentityError::ProtectionFailed(
                std::io::Error::last_os_error().to_string(),
            ));
        }
        let protected = unsafe {
            let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
            LocalFree(output.pbData.cast());
            bytes
        };
        Ok(protected)
    }

    fn unprotect(&self, ciphertext: &[u8]) -> Result<Vec<u8>, IdentityError> {
        use windows_sys::Win32::Foundation::LocalFree;
        use windows_sys::Win32::Security::Cryptography::{
            CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        };

        let input_length = u32::try_from(ciphertext.len()).map_err(|_| {
            IdentityError::ProtectionFailed("ciphertext exceeds DPAPI input range".to_string())
        })?;
        let input = CRYPT_INTEGER_BLOB {
            cbData: input_length,
            pbData: ciphertext.as_ptr().cast_mut(),
        };
        let mut output = CRYPT_INTEGER_BLOB::default();
        let succeeded = unsafe {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        if succeeded == 0 {
            return Err(IdentityError::ProtectionFailed(
                std::io::Error::last_os_error().to_string(),
            ));
        }
        let plaintext = unsafe {
            let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
            LocalFree(output.pbData.cast());
            bytes
        };
        Ok(plaintext)
    }
}

#[cfg(not(windows))]
impl SecretProtector for PlatformSecretProtector {
    fn scheme(&self) -> &'static str {
        "unsupported_non_windows"
    }

    fn protect(&self, _plaintext: &[u8]) -> Result<Vec<u8>, IdentityError> {
        Err(IdentityError::ProtectionUnavailable(
            "the master CA requires Windows DPAPI current-user protection".to_string(),
        ))
    }

    fn unprotect(&self, _ciphertext: &[u8]) -> Result<Vec<u8>, IdentityError> {
        Err(IdentityError::ProtectionUnavailable(
            "the master CA requires Windows DPAPI current-user protection".to_string(),
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IdentityAuthorityReceipt {
    pub status: String,
    pub ca_fingerprint_sha256: String,
    pub created_at_ms: u64,
    pub not_after_ms: u64,
    pub key_protection: String,
    pub ca_certificate_path: PathBuf,
    pub protected_ca_key_path: PathBuf,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EnrollmentOperation {
    Enroll,
    Rotate,
}

impl EnrollmentOperation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Enroll => "enroll",
            Self::Rotate => "rotate",
        }
    }

    fn parse(value: &str) -> Result<Self, MasterError> {
        match value {
            "enroll" => Ok(Self::Enroll),
            "rotate" => Ok(Self::Rotate),
            other => Err(MasterError::InvalidEnrollmentGrant(format!(
                "unknown operation {other}"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentGrantSpec {
    pub device_name: String,
    pub role: DeviceRole,
    pub capabilities: Vec<CapabilityDescriptor>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentGrantReceipt {
    pub status: String,
    pub operation: EnrollmentOperation,
    pub grant_id: Uuid,
    pub grant_secret: String,
    pub device_id: DeviceId,
    pub device_name: String,
    pub role: DeviceRole,
    pub registry_revision: u64,
    pub expires_at_ms: u64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnrollmentRequest {
    pub grant_id: Uuid,
    pub grant_secret: String,
    pub csr_pem: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct IssuedDeviceCertificate {
    pub status: String,
    pub operation: EnrollmentOperation,
    pub device_id: DeviceId,
    pub device_name: String,
    pub role: DeviceRole,
    pub registry_revision: u64,
    pub serial_hex: String,
    pub issued_at_ms: u64,
    pub not_after_ms: u64,
    pub certificate_sha256: String,
    pub certificate_pem: String,
    pub ca_certificate_pem: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct AuthorityMetadata {
    ca_fingerprint_sha256: String,
    created_at_ms: u64,
    not_after_ms: u64,
    key_protection: String,
}

pub struct IdentityAuthority {
    issuer: Issuer<'static, KeyPair>,
    ca_certificate_pem: String,
    receipt: IdentityAuthorityReceipt,
}

impl IdentityAuthority {
    pub fn open_existing(
        data_dir: &Path,
        protector: &impl SecretProtector,
        now_ms: u64,
    ) -> Result<Self, IdentityError> {
        let identity_dir = data_dir.join(IDENTITY_DIRECTORY);
        let certificate_path = identity_dir.join(CA_CERTIFICATE_FILE);
        let protected_key_path = identity_dir.join(PROTECTED_CA_KEY_FILE);
        let metadata_path = identity_dir.join(AUTHORITY_METADATA_FILE);
        if !certificate_path.exists() || !protected_key_path.exists() || !metadata_path.exists() {
            return Err(IdentityError::PartialAuthority);
        }
        let authority = Self::load(
            protector,
            certificate_path,
            protected_key_path,
            metadata_path,
        )?;
        authority.require_current(now_ms)?;
        Ok(authority)
    }

    pub fn open_or_initialize(
        data_dir: &Path,
        protector: &impl SecretProtector,
        now_ms: u64,
    ) -> Result<Self, IdentityError> {
        let identity_dir = data_dir.join(IDENTITY_DIRECTORY);
        let certificate_path = identity_dir.join(CA_CERTIFICATE_FILE);
        let protected_key_path = identity_dir.join(PROTECTED_CA_KEY_FILE);
        let metadata_path = identity_dir.join(AUTHORITY_METADATA_FILE);
        let present = [
            certificate_path.exists(),
            protected_key_path.exists(),
            metadata_path.exists(),
        ];
        if present.iter().any(|value| *value) && !present.iter().all(|value| *value) {
            return Err(IdentityError::PartialAuthority);
        }
        if present.iter().all(|value| *value) {
            let authority = Self::load(
                protector,
                certificate_path,
                protected_key_path,
                metadata_path,
            )?;
            authority.require_current(now_ms)?;
            return Ok(authority);
        }

        fs::create_dir_all(&identity_dir)?;
        let created_at_ms = now_ms;
        let not_after_ms = now_ms
            .checked_add(CA_LIFETIME_MS)
            .ok_or(IdentityError::InvalidTimestamp)?;
        let signing_key = KeyPair::generate_for(&PKCS_ECDSA_P256_SHA256)?;
        let mut params = CertificateParams::default();
        params.not_before = timestamp(now_ms.saturating_sub(CLOCK_SKEW_MS))?;
        params.not_after = timestamp(not_after_ms)?;
        params.serial_number = Some(random_serial()?.into());
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        params.key_usages = vec![
            KeyUsagePurpose::DigitalSignature,
            KeyUsagePurpose::KeyCertSign,
            KeyUsagePurpose::CrlSign,
        ];
        params.distinguished_name = distinguished_name("Jarvis Windows Master Enrollment CA");
        let private_key = Zeroizing::new(signing_key.serialize_der());
        let certified = CertifiedIssuer::self_signed(params, signing_key)?;
        let ca_certificate_pem = certified.pem();
        let fingerprint = Sha256::digest(certified.der().as_ref());
        let fingerprint_hex = hex(&fingerprint);
        let protected_key = protector.protect(private_key.as_slice())?;
        let metadata = AuthorityMetadata {
            ca_fingerprint_sha256: fingerprint_hex.clone(),
            created_at_ms,
            not_after_ms,
            key_protection: protector.scheme().to_string(),
        };
        write_new(&protected_key_path, &protected_key)?;
        write_new(&certificate_path, ca_certificate_pem.as_bytes())?;
        write_new(&metadata_path, &serde_json::to_vec_pretty(&metadata)?)?;
        drop(certified);

        let authority = Self::load(
            protector,
            certificate_path,
            protected_key_path,
            metadata_path,
        )?;
        authority.require_current(now_ms)?;
        Ok(authority)
    }

    fn load(
        protector: &impl SecretProtector,
        certificate_path: PathBuf,
        protected_key_path: PathBuf,
        metadata_path: PathBuf,
    ) -> Result<Self, IdentityError> {
        let ca_certificate_pem = fs::read_to_string(&certificate_path)?;
        let metadata: AuthorityMetadata = serde_json::from_slice(&fs::read(&metadata_path)?)?;
        let certificate_der = pem_certificate_der(&ca_certificate_pem)?;
        if hex(&Sha256::digest(&certificate_der)) != metadata.ca_fingerprint_sha256
            || metadata.key_protection != protector.scheme()
        {
            return Err(IdentityError::AuthorityMismatch);
        }
        let protected_key = fs::read(&protected_key_path)?;
        let private_key = Zeroizing::new(protector.unprotect(&protected_key)?);
        let signing_key = KeyPair::try_from(private_key.as_slice())?;
        let (_, certificate) = x509_parser::parse_x509_certificate(&certificate_der)
            .map_err(|_| IdentityError::AuthorityMismatch)?;
        if certificate
            .tbs_certificate
            .subject_pki
            .subject_public_key
            .data
            .as_ref()
            != signing_key.public_key_raw()
        {
            return Err(IdentityError::AuthorityMismatch);
        }
        let issuer = Issuer::from_ca_cert_pem(&ca_certificate_pem, signing_key)?;
        Ok(Self {
            issuer,
            ca_certificate_pem,
            receipt: IdentityAuthorityReceipt {
                status: "identity_authority_ready".to_string(),
                ca_fingerprint_sha256: metadata.ca_fingerprint_sha256,
                created_at_ms: metadata.created_at_ms,
                not_after_ms: metadata.not_after_ms,
                key_protection: metadata.key_protection,
                ca_certificate_path: certificate_path,
                protected_ca_key_path: protected_key_path,
            },
        })
    }

    pub fn receipt(&self) -> &IdentityAuthorityReceipt {
        &self.receipt
    }

    fn require_current(&self, now_ms: u64) -> Result<(), IdentityError> {
        if now_ms >= self.receipt.not_after_ms {
            return Err(IdentityError::AuthorityExpired);
        }
        Ok(())
    }

    fn issue(
        &self,
        device_id: DeviceId,
        device_name: &str,
        now_ms: u64,
        csr_pem: &str,
    ) -> Result<IssuedMaterial, IdentityError> {
        self.require_current(now_ms)?;
        if csr_pem.is_empty() || csr_pem.len() > MAX_CSR_PEM_BYTES {
            return Err(IdentityError::InvalidCertificateRequest);
        }
        let csr = CertificateSigningRequestParams::from_pem(csr_pem)
            .map_err(|_| IdentityError::InvalidCertificateRequest)?;
        let issued_at_ms = now_ms;
        let requested_not_after_ms = now_ms
            .checked_add(DEVICE_CERTIFICATE_LIFETIME_MS)
            .ok_or(IdentityError::InvalidTimestamp)?;
        let not_after_ms = requested_not_after_ms.min(self.receipt.not_after_ms);
        if not_after_ms <= now_ms {
            return Err(IdentityError::AuthorityExpired);
        }
        let serial = random_serial()?;
        let serial_hex = hex(&serial);
        let mut params = CertificateParams::default();
        params.not_before = timestamp(now_ms.saturating_sub(CLOCK_SKEW_MS))?;
        params.not_after = timestamp(not_after_ms)?;
        params.serial_number = Some(SerialNumber::from(serial));
        params.distinguished_name = distinguished_name(device_name);
        params.subject_alt_names = vec![SanType::URI(
            format!("urn:jarvis:device:{}", device_id.0)
                .try_into()
                .map_err(|_| IdentityError::InvalidCertificateRequest)?,
        )];
        params.is_ca = IsCa::NoCa;
        params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
        params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
        params.use_authority_key_identifier_extension = true;
        let certificate = params.signed_by(&csr.public_key, &self.issuer)?;
        let certificate_der = certificate.der();
        Ok(IssuedMaterial {
            serial_hex,
            issued_at_ms,
            not_after_ms,
            certificate_sha256: Sha256::digest(certificate_der.as_ref()).into(),
            certificate_pem: certificate.pem(),
        })
    }
}

struct IssuedMaterial {
    serial_hex: String,
    issued_at_ms: u64,
    not_after_ms: u64,
    certificate_sha256: [u8; 32],
    certificate_pem: String,
}

struct StoredGrant {
    operation: EnrollmentOperation,
    secret_sha256: [u8; 32],
    registration: DeviceRegistration,
    expires_at_ms: u64,
    consumed_at_ms: Option<u64>,
}

impl MasterKernel {
    pub fn identity_authority_recorded(&self) -> Result<bool, MasterError> {
        self.connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM master_identity_authority WHERE authority_id = 1)",
                [],
                |row| row.get(0),
            )
            .map_err(MasterError::from)
    }

    pub fn record_identity_authority(
        &mut self,
        receipt: &IdentityAuthorityReceipt,
    ) -> Result<(), MasterError> {
        let fingerprint = decode_hex_digest(&receipt.ca_fingerprint_sha256)?;
        let stored = self
            .connection
            .query_row(
                "SELECT ca_fingerprint_sha256, created_at_ms, not_after_ms, key_protection\n                 FROM master_identity_authority WHERE authority_id = 1",
                [],
                |row| {
                    Ok((
                        row.get::<_, Vec<u8>>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                    ))
                },
            )
            .optional()?;
        if let Some((stored_fingerprint, created_at_ms, not_after_ms, key_protection)) = stored {
            if stored_fingerprint != fingerprint
                || i64_to_u64(created_at_ms)? != receipt.created_at_ms
                || i64_to_u64(not_after_ms)? != receipt.not_after_ms
                || key_protection != receipt.key_protection
            {
                return Err(IdentityError::AuthorityMismatch.into());
            }
            return Ok(());
        }
        self.connection.execute(
            "INSERT INTO master_identity_authority\n             (authority_id, ca_fingerprint_sha256, created_at_ms, not_after_ms, key_protection)\n             VALUES (1, ?1, ?2, ?3, ?4)",
            params![
                fingerprint.as_slice(),
                u64_to_i64(receipt.created_at_ms)?,
                u64_to_i64(receipt.not_after_ms)?,
                receipt.key_protection,
            ],
        )?;
        Ok(())
    }

    pub fn create_enrollment_grant(
        &mut self,
        spec: EnrollmentGrantSpec,
        now_ms: u64,
    ) -> Result<EnrollmentGrantReceipt, MasterError> {
        let device_id = DeviceId::new(Uuid::new_v4());
        let registration = DeviceRegistration {
            device_id,
            device_name: spec.device_name,
            role: spec.role,
            registry_revision: 1,
            capabilities: spec.capabilities,
        };
        validate_registration(&registration)?;
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let enrolled: i64 =
            tx.query_row("SELECT COUNT(*) FROM master_devices", [], |row| row.get(0))?;
        let reserved: i64 = tx.query_row(
            "SELECT COUNT(*) FROM master_enrollment_grants\n             WHERE operation = 'enroll' AND consumed_at_ms IS NULL AND expires_at_ms > ?1",
            [u64_to_i64(now_ms)?],
            |row| row.get(0),
        )?;
        if i64_to_u64(enrolled)?
            .checked_add(i64_to_u64(reserved)?)
            .ok_or(MasterError::IntegerOutOfRange)?
            >= MAX_ENROLLED_DEVICES
        {
            return Err(MasterError::EnrolledDeviceLimit);
        }
        let receipt = insert_grant(&tx, EnrollmentOperation::Enroll, registration, now_ms)?;
        tx.commit()?;
        Ok(receipt)
    }

    pub fn create_rotation_grant(
        &mut self,
        device_id: DeviceId,
        now_ms: u64,
    ) -> Result<EnrollmentGrantReceipt, MasterError> {
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let registration = load_registration(&tx, device_id, false)?;
        let receipt = insert_grant(&tx, EnrollmentOperation::Rotate, registration, now_ms)?;
        tx.commit()?;
        Ok(receipt)
    }

    pub fn issue_device_certificate(
        &mut self,
        authority: &IdentityAuthority,
        request: &EnrollmentRequest,
        now_ms: u64,
    ) -> Result<IssuedDeviceCertificate, MasterError> {
        if request.grant_id.is_nil() || !valid_grant_secret(&request.grant_secret) {
            return Err(MasterError::InvalidEnrollmentGrantSecret);
        }
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let grant = load_grant(&tx, request.grant_id)?;
        if grant.consumed_at_ms.is_some() {
            return Err(MasterError::EnrollmentGrantConsumed);
        }
        if now_ms >= grant.expires_at_ms {
            return Err(MasterError::EnrollmentGrantExpired);
        }
        let candidate_digest: [u8; 32] = Sha256::digest(request.grant_secret.as_bytes()).into();
        if !constant_time_equal(&candidate_digest, &grant.secret_sha256) {
            return Err(MasterError::InvalidEnrollmentGrantSecret);
        }
        let material = authority.issue(
            grant.registration.device_id,
            &grant.registration.device_name,
            now_ms,
            &request.csr_pem,
        )?;
        match grant.operation {
            EnrollmentOperation::Enroll => {
                let enrolled: i64 =
                    tx.query_row("SELECT COUNT(*) FROM master_devices", [], |row| row.get(0))?;
                if i64_to_u64(enrolled)? >= MAX_ENROLLED_DEVICES {
                    return Err(MasterError::EnrolledDeviceLimit);
                }
                insert_registration(&tx, &grant.registration)?;
            }
            EnrollmentOperation::Rotate => {
                let current = load_registration(&tx, grant.registration.device_id, false)?;
                if current != grant.registration {
                    return Err(MasterError::InvalidEnrollmentGrant(
                        "device registry changed after rotation grant creation".to_string(),
                    ));
                }
            }
        }
        tx.execute(
            "INSERT INTO master_device_certificates\n             (serial_hex, device_id, certificate_sha256, issued_at_ms, not_after_ms,\n              revoked_at_ms, revocation_reason, replaced_by_serial_hex)\n             VALUES (?1, ?2, ?3, ?4, ?5, NULL, NULL, NULL)",
            params![
                material.serial_hex,
                grant.registration.device_id.0.to_string(),
                material.certificate_sha256.as_slice(),
                u64_to_i64(material.issued_at_ms)?,
                u64_to_i64(material.not_after_ms)?,
            ],
        )?;
        if grant.operation == EnrollmentOperation::Rotate {
            tx.execute(
                "UPDATE master_device_certificates\n                 SET revoked_at_ms = ?1, revocation_reason = 'rotated',\n                     replaced_by_serial_hex = ?2\n                 WHERE device_id = ?3 AND serial_hex <> ?2 AND revoked_at_ms IS NULL",
                params![
                    u64_to_i64(now_ms)?,
                    material.serial_hex,
                    grant.registration.device_id.0.to_string(),
                ],
            )?;
        }
        let consumed = tx.execute(
            "UPDATE master_enrollment_grants SET consumed_at_ms = ?1\n             WHERE grant_id = ?2 AND consumed_at_ms IS NULL",
            params![u64_to_i64(now_ms)?, request.grant_id.to_string()],
        )?;
        if consumed != 1 {
            return Err(MasterError::EnrollmentGrantConsumed);
        }
        tx.commit()?;
        Ok(IssuedDeviceCertificate {
            status: "device_certificate_issued".to_string(),
            operation: grant.operation,
            device_id: grant.registration.device_id,
            device_name: grant.registration.device_name,
            role: grant.registration.role,
            registry_revision: grant.registration.registry_revision,
            serial_hex: material.serial_hex,
            issued_at_ms: material.issued_at_ms,
            not_after_ms: material.not_after_ms,
            certificate_sha256: hex(&material.certificate_sha256),
            certificate_pem: material.certificate_pem,
            ca_certificate_pem: authority.ca_certificate_pem.clone(),
        })
    }

    pub fn certificate_is_active(
        &self,
        device_id: DeviceId,
        serial_hex: &str,
        now_ms: u64,
    ) -> Result<bool, MasterError> {
        let active = self.connection.query_row(
            "SELECT EXISTS(\n               SELECT 1 FROM master_device_certificates c\n               JOIN master_devices d ON d.device_id = c.device_id\n               WHERE c.device_id = ?1 AND c.serial_hex = ?2 AND c.revoked_at_ms IS NULL\n                 AND c.not_after_ms > ?3 AND d.revoked = 0\n             )",
            params![
                device_id.0.to_string(),
                serial_hex,
                u64_to_i64(now_ms)?,
            ],
            |row| row.get(0),
        )?;
        Ok(active)
    }
}

fn insert_grant(
    tx: &rusqlite::Transaction<'_>,
    operation: EnrollmentOperation,
    registration: DeviceRegistration,
    now_ms: u64,
) -> Result<EnrollmentGrantReceipt, MasterError> {
    let outstanding: i64 = tx.query_row(
        "SELECT COUNT(*) FROM master_enrollment_grants\n         WHERE consumed_at_ms IS NULL AND expires_at_ms > ?1",
        [u64_to_i64(now_ms)?],
        |row| row.get(0),
    )?;
    if i64_to_u64(outstanding)? >= MAX_OUTSTANDING_ENROLLMENT_GRANTS {
        return Err(MasterError::EnrollmentGrantLimit);
    }
    let grant_id = Uuid::new_v4();
    let grant_secret = random_secret()?;
    let secret_sha256: [u8; 32] = Sha256::digest(grant_secret.as_bytes()).into();
    let expires_at_ms = now_ms
        .checked_add(ENROLLMENT_GRANT_TTL_MS)
        .ok_or(MasterError::IntegerOutOfRange)?;
    tx.execute(
        "INSERT INTO master_enrollment_grants\n         (grant_id, operation, secret_sha256, device_id, device_name, role_json,\n          registry_revision, capabilities_json, created_at_ms, expires_at_ms, consumed_at_ms)\n         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL)",
        params![
            grant_id.to_string(),
            operation.as_str(),
            secret_sha256.as_slice(),
            registration.device_id.0.to_string(),
            registration.device_name,
            serde_json::to_string(&registration.role)?,
            u64_to_i64(registration.registry_revision)?,
            serde_json::to_string(&registration.capabilities)?,
            u64_to_i64(now_ms)?,
            u64_to_i64(expires_at_ms)?,
        ],
    )?;
    Ok(EnrollmentGrantReceipt {
        status: "enrollment_grant_created".to_string(),
        operation,
        grant_id,
        grant_secret,
        device_id: registration.device_id,
        device_name: registration.device_name,
        role: registration.role,
        registry_revision: registration.registry_revision,
        expires_at_ms,
    })
}

fn load_grant(tx: &rusqlite::Transaction<'_>, grant_id: Uuid) -> Result<StoredGrant, MasterError> {
    let stored = tx
        .query_row(
            "SELECT operation, secret_sha256, device_id, device_name, role_json,\n                    registry_revision, capabilities_json, expires_at_ms, consumed_at_ms\n             FROM master_enrollment_grants WHERE grant_id = ?1",
            [grant_id.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Vec<u8>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, i64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, Option<i64>>(8)?,
                ))
            },
        )
        .optional()?
        .ok_or(MasterError::EnrollmentGrantNotFound)?;
    Ok(StoredGrant {
        operation: EnrollmentOperation::parse(&stored.0)?,
        secret_sha256: digest_array(&stored.1)?,
        registration: DeviceRegistration {
            device_id: DeviceId::new(parse_uuid(&stored.2)?),
            device_name: stored.3,
            role: serde_json::from_str(&stored.4)?,
            registry_revision: i64_to_u64(stored.5)?,
            capabilities: serde_json::from_str(&stored.6)?,
        },
        expires_at_ms: i64_to_u64(stored.7)?,
        consumed_at_ms: stored.8.map(i64_to_u64).transpose()?,
    })
}

fn insert_registration(
    tx: &rusqlite::Transaction<'_>,
    registration: &DeviceRegistration,
) -> Result<(), MasterError> {
    let result = tx.execute(
        "INSERT INTO master_devices\n         (device_id, device_name, role_json, registry_revision, capabilities_json, revoked)\n         VALUES (?1, ?2, ?3, ?4, ?5, 0)",
        params![
            registration.device_id.0.to_string(),
            registration.device_name,
            serde_json::to_string(&registration.role)?,
            u64_to_i64(registration.registry_revision)?,
            serde_json::to_string(&registration.capabilities)?,
        ],
    );
    match result {
        Ok(_) => Ok(()),
        Err(error) if crate::is_constraint_violation(&error) => {
            Err(MasterError::DeviceAlreadyRegistered)
        }
        Err(error) => Err(error.into()),
    }
}

fn load_registration(
    tx: &rusqlite::Transaction<'_>,
    device_id: DeviceId,
    allow_revoked: bool,
) -> Result<DeviceRegistration, MasterError> {
    let stored = tx
        .query_row(
            "SELECT device_name, role_json, registry_revision, capabilities_json, revoked\n             FROM master_devices WHERE device_id = ?1",
            [device_id.0.to_string()],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, i64>(4)?,
                ))
            },
        )
        .optional()?
        .ok_or(MasterError::DeviceNotRegistered)?;
    if stored.4 != 0 && !allow_revoked {
        return Err(MasterError::InvalidEnrollmentGrant(
            "device is revoked".to_string(),
        ));
    }
    Ok(DeviceRegistration {
        device_id,
        device_name: stored.0,
        role: serde_json::from_str(&stored.1)?,
        registry_revision: i64_to_u64(stored.2)?,
        capabilities: serde_json::from_str(&stored.3)?,
    })
}

fn validate_registration(registration: &DeviceRegistration) -> Result<(), MasterError> {
    HandshakeRequest {
        protocol_version: PROTOCOL_VERSION,
        device_id: registration.device_id,
        device_name: registration.device_name.clone(),
        role: registration.role,
        registry_revision: registration.registry_revision,
        capabilities: registration.capabilities.clone(),
    }
    .validate()?;
    Ok(())
}

fn random_secret() -> Result<String, IdentityError> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes).map_err(|error| IdentityError::Random(error.to_string()))?;
    Ok(hex(&bytes))
}

fn random_serial() -> Result<Vec<u8>, IdentityError> {
    let mut bytes = vec![0_u8; 20];
    getrandom::fill(&mut bytes).map_err(|error| IdentityError::Random(error.to_string()))?;
    bytes[0] &= 0x7f;
    bytes[0] |= 0x01;
    Ok(bytes)
}

fn timestamp(milliseconds: u64) -> Result<OffsetDateTime, IdentityError> {
    let seconds =
        i64::try_from(milliseconds / 1_000).map_err(|_| IdentityError::InvalidTimestamp)?;
    OffsetDateTime::from_unix_timestamp(seconds).map_err(|_| IdentityError::InvalidTimestamp)
}

fn distinguished_name(common_name: &str) -> DistinguishedName {
    let mut name = DistinguishedName::new();
    name.push(DnType::OrganizationName, "Jarvis Developer Mode");
    name.push(DnType::CommonName, common_name);
    name
}

fn write_new(path: &Path, contents: &[u8]) -> Result<(), IdentityError> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(contents)?;
    file.sync_all()?;
    Ok(())
}

fn pem_certificate_der(pem: &str) -> Result<Vec<u8>, IdentityError> {
    let start = "-----BEGIN CERTIFICATE-----";
    let end = "-----END CERTIFICATE-----";
    let trimmed = pem.trim();
    let body = trimmed
        .strip_prefix(start)
        .and_then(|value| value.strip_suffix(end))
        .ok_or(IdentityError::AuthorityMismatch)?;
    let compact = body.lines().collect::<String>();
    base64::engine::general_purpose::STANDARD
        .decode(compact)
        .map_err(|_| IdentityError::AuthorityMismatch)
}

fn decode_hex_digest(value: &str) -> Result<[u8; 32], MasterError> {
    if value.len() != 64 {
        return Err(IdentityError::AuthorityMismatch.into());
    }
    let mut output = [0_u8; 32];
    for (index, pair) in value.as_bytes().chunks_exact(2).enumerate() {
        let high = hex_nibble(pair[0]).ok_or(IdentityError::AuthorityMismatch)?;
        let low = hex_nibble(pair[1]).ok_or(IdentityError::AuthorityMismatch)?;
        output[index] = (high << 4) | low;
    }
    Ok(output)
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn valid_grant_secret(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn digest_array(value: &[u8]) -> Result<[u8; 32], MasterError> {
    value.try_into().map_err(|_| {
        MasterError::InvalidEnrollmentGrant("secret digest length is invalid".to_string())
    })
}

fn constant_time_equal(left: &[u8; 32], right: &[u8; 32]) -> bool {
    left.iter()
        .zip(right.iter())
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

pub fn validate_revocation_reason(reason: &str) -> Result<(), IdentityError> {
    if reason.is_empty()
        || reason.len() > MAX_REVOCATION_REASON_BYTES
        || reason.chars().any(char::is_control)
    {
        return Err(IdentityError::InvalidRevocationReason);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grant_secret_validation_is_exact() {
        assert!(valid_grant_secret(&"a".repeat(64)));
        assert!(!valid_grant_secret(&"a".repeat(63)));
        assert!(!valid_grant_secret(&"z".repeat(64)));
    }

    #[test]
    fn pem_decoder_rejects_non_certificate_text() {
        assert!(pem_certificate_der("not pem").is_err());
    }
}
