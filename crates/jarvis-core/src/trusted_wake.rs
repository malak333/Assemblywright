use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::{DateTime, Duration, Utc};
use p256::ecdsa::{signature::Verifier, Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{JarvisError, JarvisResult};

pub const TRUSTED_WAKE_SCHEMA_VERSION: u16 = 1;
pub const TRUSTED_WAKE_RULE_ID: Uuid = Uuid::from_u128(0x4a617276_6973_4000_8000_000000000010);
pub const MAX_WAKE_PAYLOAD_BYTES: usize = 4 * 1024;
pub const MAX_WAKE_SIGNATURE_BYTES: usize = 128;
pub const MAX_WAKE_PUBLIC_KEY_BYTES: usize = 65;
pub const MAX_WAKE_NONCE_BYTES: usize = 64;
pub const MAX_WAKE_COMMAND_BYTES: usize = 2 * 1024;
pub const MAX_WAKE_CLOCK_SKEW_SECONDS: i64 = 120;
pub const TRUSTED_WAKE_KEY_CONTROL_DOMAIN: &str = "jarvis.trusted-wake.key-control.v1";
pub const TRUSTED_WAKE_ROTATE_CONFIRMATION: &str = "ROTATE TRUSTED WAKE KEY";
pub const TRUSTED_WAKE_RECOVER_CONFIRMATION: &str =
    "RECOVER LOST TRUSTED WAKE KEY AND BLOCK PENDING WORK";
pub const TRUSTED_WAKE_CANCEL_CONFIRMATION: &str = "CANCEL TRUSTED WAKE KEY CHANGE";
pub const TRUSTED_WAKE_KEY_GRANT_TTL_SECONDS: i64 = 300;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TrustedWakePayload {
    pub schema_version: u16,
    pub rule_id: Uuid,
    pub rule_generation: u64,
    pub session_id: Uuid,
    pub challenge: String,
    pub counter: u64,
    pub occurred_at: DateTime<Utc>,
    pub nonce: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TrustedWakeEnvelope {
    pub payload_b64: String,
    pub signature_der_b64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustedWakeRule {
    pub id: Uuid,
    pub enabled: bool,
    pub key_fingerprint: String,
    pub generation: u64,
    pub highest_counter: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TrustedWakeRuleEnrollment {
    pub rule_id: Uuid,
    pub public_key_x963_b64: String,
    pub command: String,
    #[serde(default)]
    pub allow_rotation: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustedWakeKeyControlMode {
    Rotate,
    Recover,
}

impl TrustedWakeKeyControlMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rotate => "rotate",
            Self::Recover => "recover",
        }
    }

    pub fn required_confirmation(self) -> &'static str {
        match self {
            Self::Rotate => TRUSTED_WAKE_ROTATE_CONFIRMATION,
            Self::Recover => TRUSTED_WAKE_RECOVER_CONFIRMATION,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TrustedWakeKeyControlProofPayload {
    pub domain: String,
    pub schema_version: u16,
    pub operation: TrustedWakeKeyControlMode,
    pub rule_id: Uuid,
    pub expected_generation: u64,
    pub expected_fingerprint: String,
    pub new_fingerprint: String,
    pub session_id: Uuid,
    pub challenge: String,
    pub confirmation: String,
    pub occurred_at: DateTime<Utc>,
    pub nonce: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TrustedWakeKeyControlProof {
    pub payload_b64: String,
    pub signature_der_b64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TrustedWakeKeyControlPrepareRequest {
    pub operation: TrustedWakeKeyControlMode,
    pub expected_generation: u64,
    pub expected_fingerprint: String,
    pub new_public_key_x963_b64: String,
    pub confirmation: String,
    pub proof: Option<TrustedWakeKeyControlProof>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustedWakePendingKeyControl {
    pub operation: TrustedWakeKeyControlMode,
    pub source_generation: u64,
    pub target_generation: u64,
    pub old_fingerprint: String,
    pub new_fingerprint: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustedWakeKeyControlPrepareResponse {
    pub pending: TrustedWakePendingKeyControl,
    pub grant_token: String,
    pub blocked_accepted_count: usize,
    pub proof_boundary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TrustedWakeKeyControlCancelRequest {
    pub expected_generation: u64,
    pub expected_fingerprint: String,
    pub confirmation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TrustedWakeKeyControlInstallDocument {
    pub rule_id: Uuid,
    pub target_generation: u64,
    pub new_public_key_x963_b64: String,
    pub grant_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TrustedWakeRuleEnablement {
    pub enabled: bool,
    pub expected_generation: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustedWakeSessionStatus {
    pub schema_version: u16,
    pub session_id: Uuid,
    pub challenge: String,
    pub rule: Option<TrustedWakeRule>,
    pub attention_required: bool,
    pub ambiguous_dispatch_count: usize,
    pub pending_key_control: Option<TrustedWakePendingKeyControl>,
    pub proof_boundary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustedWakeAttentionItem {
    pub event_id: Uuid,
    pub scheduler_job_id: Uuid,
    pub rule_generation: u64,
    pub state: TrustedWakeDispatchState,
    pub received_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct TrustedWakeResolutionRequest {
    pub expected_generation: u64,
    pub expected_state: TrustedWakeDispatchState,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustedWakeDispatchState {
    Accepted,
    DispatchStarted,
    Completed,
    Blocked,
    Failed,
}

impl TrustedWakeDispatchState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::DispatchStarted => "dispatch_started",
            Self::Completed => "completed",
            Self::Blocked => "blocked",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrustedWakeAcceptedEvent {
    pub id: Uuid,
    pub rule_id: Uuid,
    pub counter: u64,
    #[serde(skip_serializing)]
    pub payload_sha256: String,
    pub state: TrustedWakeDispatchState,
    pub task_id: Option<Uuid>,
    pub scheduler_job_id: Uuid,
    pub received_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct VerifiedTrustedWakeEnvelope {
    pub payload: TrustedWakePayload,
    pub payload_sha256: String,
}

pub fn decode_public_key(public_key_b64: &str) -> JarvisResult<(Vec<u8>, String)> {
    if public_key_b64.len() > encoded_limit(MAX_WAKE_PUBLIC_KEY_BYTES) {
        return Err(JarvisError::Validation(
            "trusted wake public key exceeds bounded input limits".to_string(),
        ));
    }
    let public_key = STANDARD.decode(public_key_b64).map_err(|_| {
        JarvisError::Validation("trusted wake public key must be valid base64".to_string())
    })?;
    if public_key.len() != MAX_WAKE_PUBLIC_KEY_BYTES || public_key.first() != Some(&4) {
        return Err(JarvisError::Validation(
            "trusted wake public key must be an uncompressed P-256 X9.63 key".to_string(),
        ));
    }
    VerifyingKey::from_sec1_bytes(&public_key).map_err(|_| {
        JarvisError::Validation("trusted wake public key is not valid P-256".to_string())
    })?;
    let fingerprint = hex_sha256(&public_key);
    Ok((public_key, fingerprint))
}

pub fn verify_envelope(
    envelope: &TrustedWakeEnvelope,
    public_key: &[u8],
    expected_session_id: Uuid,
    expected_challenge: &str,
    now: DateTime<Utc>,
) -> JarvisResult<VerifiedTrustedWakeEnvelope> {
    if envelope.payload_b64.len() > encoded_limit(MAX_WAKE_PAYLOAD_BYTES)
        || envelope.signature_der_b64.len() > encoded_limit(MAX_WAKE_SIGNATURE_BYTES)
    {
        return Err(JarvisError::Validation(
            "trusted wake envelope exceeds bounded input limits".to_string(),
        ));
    }
    let payload_bytes = STANDARD.decode(&envelope.payload_b64).map_err(|_| {
        JarvisError::Validation("trusted wake payload must be valid base64".to_string())
    })?;
    if payload_bytes.is_empty() || payload_bytes.len() > MAX_WAKE_PAYLOAD_BYTES {
        return Err(JarvisError::Validation(
            "trusted wake payload exceeds bounded input limits".to_string(),
        ));
    }
    let signature_bytes = STANDARD.decode(&envelope.signature_der_b64).map_err(|_| {
        JarvisError::Validation("trusted wake signature must be valid base64".to_string())
    })?;
    if signature_bytes.is_empty() || signature_bytes.len() > MAX_WAKE_SIGNATURE_BYTES {
        return Err(JarvisError::Validation(
            "trusted wake signature exceeds bounded input limits".to_string(),
        ));
    }
    let verifying_key = VerifyingKey::from_sec1_bytes(public_key).map_err(|_| {
        JarvisError::Validation("stored trusted wake public key is invalid".to_string())
    })?;
    let signature = Signature::from_der(&signature_bytes).map_err(|_| {
        JarvisError::Validation("trusted wake signature is not DER-encoded P-256".to_string())
    })?;
    verifying_key
        .verify(&payload_bytes, &signature)
        .map_err(|_| {
            JarvisError::Validation("trusted wake signature verification failed".to_string())
        })?;
    let payload: TrustedWakePayload = serde_json::from_slice(&payload_bytes).map_err(|_| {
        JarvisError::Validation("trusted wake signed payload is invalid JSON".to_string())
    })?;
    if payload.schema_version != TRUSTED_WAKE_SCHEMA_VERSION {
        return Err(JarvisError::Validation(
            "trusted wake signed payload schema is unsupported".to_string(),
        ));
    }
    if payload.session_id != expected_session_id || payload.challenge != expected_challenge {
        return Err(JarvisError::Validation(
            "trusted wake payload is not bound to the active core session".to_string(),
        ));
    }
    if payload.counter == 0 {
        return Err(JarvisError::Validation(
            "trusted wake counter must be greater than zero".to_string(),
        ));
    }
    if payload.nonce.len() > MAX_WAKE_NONCE_BYTES || Uuid::parse_str(&payload.nonce).is_err() {
        return Err(JarvisError::Validation(
            "trusted wake nonce must be a UUID inside bounded limits".to_string(),
        ));
    }
    if payload.occurred_at < now - Duration::seconds(MAX_WAKE_CLOCK_SKEW_SECONDS)
        || payload.occurred_at > now + Duration::seconds(MAX_WAKE_CLOCK_SKEW_SECONDS)
    {
        return Err(JarvisError::Validation(
            "trusted wake payload timestamp is outside the accepted clock-skew window".to_string(),
        ));
    }
    Ok(VerifiedTrustedWakeEnvelope {
        payload,
        payload_sha256: hex_sha256(&payload_bytes),
    })
}

pub fn verify_key_control_proof(
    proof: &TrustedWakeKeyControlProof,
    public_key: &[u8],
    request: &TrustedWakeKeyControlPrepareRequest,
    new_fingerprint: &str,
    expected_session_id: Uuid,
    expected_challenge: &str,
    now: DateTime<Utc>,
) -> JarvisResult<()> {
    if proof.payload_b64.len() > encoded_limit(MAX_WAKE_PAYLOAD_BYTES)
        || proof.signature_der_b64.len() > encoded_limit(MAX_WAKE_SIGNATURE_BYTES)
    {
        return Err(JarvisError::Validation(
            "trusted wake key-control proof exceeds bounded input limits".to_string(),
        ));
    }
    let payload_bytes = STANDARD.decode(&proof.payload_b64).map_err(|_| {
        JarvisError::Validation(
            "trusted wake key-control proof payload must be valid base64".to_string(),
        )
    })?;
    if payload_bytes.is_empty() || payload_bytes.len() > MAX_WAKE_PAYLOAD_BYTES {
        return Err(JarvisError::Validation(
            "trusted wake key-control proof payload exceeds bounded input limits".to_string(),
        ));
    }
    let signature_bytes = STANDARD.decode(&proof.signature_der_b64).map_err(|_| {
        JarvisError::Validation(
            "trusted wake key-control signature must be valid base64".to_string(),
        )
    })?;
    let signature = Signature::from_der(&signature_bytes).map_err(|_| {
        JarvisError::Validation(
            "trusted wake key-control signature is not DER-encoded P-256".to_string(),
        )
    })?;
    VerifyingKey::from_sec1_bytes(public_key)
        .map_err(|_| {
            JarvisError::Validation("stored trusted wake public key is invalid".to_string())
        })?
        .verify(&payload_bytes, &signature)
        .map_err(|_| {
            JarvisError::Validation(
                "trusted wake key-control signature verification failed".to_string(),
            )
        })?;
    let payload: TrustedWakeKeyControlProofPayload = serde_json::from_slice(&payload_bytes)
        .map_err(|_| {
            JarvisError::Validation(
                "trusted wake key-control signed payload is invalid JSON".to_string(),
            )
        })?;
    if payload.domain != TRUSTED_WAKE_KEY_CONTROL_DOMAIN
        || payload.schema_version != TRUSTED_WAKE_SCHEMA_VERSION
        || payload.operation != TrustedWakeKeyControlMode::Rotate
        || request.operation != TrustedWakeKeyControlMode::Rotate
        || payload.rule_id != TRUSTED_WAKE_RULE_ID
        || payload.expected_generation != request.expected_generation
        || payload.expected_fingerprint != request.expected_fingerprint
        || payload.new_fingerprint != new_fingerprint
        || payload.session_id != expected_session_id
        || payload.challenge != expected_challenge
        || payload.confirmation != request.confirmation
    {
        return Err(JarvisError::Validation(
            "trusted wake key-control proof bindings do not match the active request".to_string(),
        ));
    }
    if payload.nonce.len() > MAX_WAKE_NONCE_BYTES || Uuid::parse_str(&payload.nonce).is_err() {
        return Err(JarvisError::Validation(
            "trusted wake key-control nonce must be a UUID inside bounded limits".to_string(),
        ));
    }
    if payload.occurred_at < now - Duration::seconds(MAX_WAKE_CLOCK_SKEW_SECONDS)
        || payload.occurred_at > now + Duration::seconds(MAX_WAKE_CLOCK_SKEW_SECONDS)
    {
        return Err(JarvisError::Validation(
            "trusted wake key-control proof timestamp is outside the accepted clock-skew window"
                .to_string(),
        ));
    }
    Ok(())
}

pub fn hash_grant_token(token: &str) -> JarvisResult<String> {
    if token.len() < 32
        || token.len() > 256
        || !token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(JarvisError::Validation(
            "trusted wake key-control grant token is outside bounded syntax".to_string(),
        ));
    }
    Ok(hex_sha256(token.as_bytes()))
}

pub fn validate_command(command: &str) -> JarvisResult<String> {
    let command = command.trim();
    if command.is_empty() || command.len() > MAX_WAKE_COMMAND_BYTES {
        return Err(JarvisError::Validation(
            "trusted wake rule command is outside bounded limits".to_string(),
        ));
    }
    Ok(command.to_string())
}

fn encoded_limit(bytes: usize) -> usize {
    bytes.div_ceil(3) * 4
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::{signature::Signer, SigningKey};

    #[test]
    fn verifies_bound_p256_envelope_and_rejects_bad_session() {
        let signing_key = SigningKey::from_bytes((&[7_u8; 32]).into()).unwrap();
        let public_key = signing_key.verifying_key().to_encoded_point(false);
        let session_id = Uuid::new_v4();
        let payload = TrustedWakePayload {
            schema_version: 1,
            rule_id: Uuid::new_v4(),
            rule_generation: 1,
            session_id,
            challenge: "challenge".to_string(),
            counter: 1,
            occurred_at: Utc::now(),
            nonce: Uuid::new_v4().to_string(),
        };
        let bytes = serde_json::to_vec(&payload).unwrap();
        let signature: Signature = signing_key.sign(&bytes);
        let envelope = TrustedWakeEnvelope {
            payload_b64: STANDARD.encode(&bytes),
            signature_der_b64: STANDARD.encode(signature.to_der().as_bytes()),
        };
        verify_envelope(
            &envelope,
            public_key.as_bytes(),
            session_id,
            "challenge",
            Utc::now(),
        )
        .unwrap();
        assert!(verify_envelope(
            &envelope,
            public_key.as_bytes(),
            Uuid::new_v4(),
            "challenge",
            Utc::now(),
        )
        .is_err());
    }

    #[test]
    fn rejects_unknown_fields_bad_der_stale_timestamp_and_oversized_inputs() {
        let signing_key = SigningKey::from_bytes((&[9_u8; 32]).into()).unwrap();
        let public_key = signing_key.verifying_key().to_encoded_point(false);
        let session_id = Uuid::new_v4();
        let now = Utc::now();
        let mut value = serde_json::json!({
            "schema_version": 1,
            "rule_id": Uuid::new_v4(),
            "rule_generation": 1,
            "session_id": session_id,
            "challenge": "challenge",
            "counter": 2,
            "occurred_at": now,
            "nonce": Uuid::new_v4(),
            "unexpected": true,
        });
        let signed = |value: &serde_json::Value| {
            let bytes = serde_json::to_vec(value).unwrap();
            let signature: Signature = signing_key.sign(&bytes);
            TrustedWakeEnvelope {
                payload_b64: STANDARD.encode(bytes),
                signature_der_b64: STANDARD.encode(signature.to_der().as_bytes()),
            }
        };
        assert!(verify_envelope(
            &signed(&value),
            public_key.as_bytes(),
            session_id,
            "challenge",
            now,
        )
        .is_err());

        value.as_object_mut().unwrap().remove("unexpected");
        value["occurred_at"] = serde_json::json!(now - Duration::seconds(121));
        assert!(verify_envelope(
            &signed(&value),
            public_key.as_bytes(),
            session_id,
            "challenge",
            now,
        )
        .is_err());

        let mut bad_der = signed(&serde_json::json!({
            "schema_version": 1,
            "rule_id": Uuid::new_v4(),
            "rule_generation": 1,
            "session_id": session_id,
            "challenge": "challenge",
            "counter": 3,
            "occurred_at": now,
            "nonce": Uuid::new_v4(),
        }));
        bad_der.signature_der_b64 = STANDARD.encode([0_u8; 64]);
        assert!(verify_envelope(
            &bad_der,
            public_key.as_bytes(),
            session_id,
            "challenge",
            now,
        )
        .is_err());

        let oversized = TrustedWakeEnvelope {
            payload_b64: "A".repeat(encoded_limit(MAX_WAKE_PAYLOAD_BYTES) + 1),
            signature_der_b64: STANDARD.encode([1_u8; 8]),
        };
        assert!(verify_envelope(
            &oversized,
            public_key.as_bytes(),
            session_id,
            "challenge",
            now,
        )
        .is_err());
    }

    #[test]
    fn key_control_proof_rejects_signature_domain_session_time_and_binding_substitution() {
        use p256::ecdsa::{signature::Signer, SigningKey};

        let old_key = SigningKey::from_bytes((&[41_u8; 32]).into()).unwrap();
        let wrong_key = SigningKey::from_bytes((&[42_u8; 32]).into()).unwrap();
        let new_key = SigningKey::from_bytes((&[43_u8; 32]).into()).unwrap();
        let old_public = old_key.verifying_key().to_encoded_point(false);
        let new_public_b64 = STANDARD.encode(new_key.verifying_key().to_encoded_point(false));
        let new_fingerprint = decode_public_key(&new_public_b64).unwrap().1;
        let session_id = Uuid::new_v4();
        let now = Utc::now();
        let request = TrustedWakeKeyControlPrepareRequest {
            operation: TrustedWakeKeyControlMode::Rotate,
            expected_generation: 7,
            expected_fingerprint: "old-fingerprint".to_string(),
            new_public_key_x963_b64: new_public_b64,
            confirmation: TRUSTED_WAKE_ROTATE_CONFIRMATION.to_string(),
            proof: None,
        };
        let payload = TrustedWakeKeyControlProofPayload {
            domain: TRUSTED_WAKE_KEY_CONTROL_DOMAIN.to_string(),
            schema_version: TRUSTED_WAKE_SCHEMA_VERSION,
            operation: TrustedWakeKeyControlMode::Rotate,
            rule_id: TRUSTED_WAKE_RULE_ID,
            expected_generation: request.expected_generation,
            expected_fingerprint: request.expected_fingerprint.clone(),
            new_fingerprint: new_fingerprint.clone(),
            session_id,
            challenge: "active-challenge".to_string(),
            confirmation: request.confirmation.clone(),
            occurred_at: now,
            nonce: Uuid::new_v4().to_string(),
        };
        let signed = |payload: &TrustedWakeKeyControlProofPayload, key: &SigningKey| {
            let bytes = serde_json::to_vec(payload).unwrap();
            let signature: Signature = key.sign(&bytes);
            TrustedWakeKeyControlProof {
                payload_b64: STANDARD.encode(bytes),
                signature_der_b64: STANDARD.encode(signature.to_der().as_bytes()),
            }
        };
        assert!(verify_key_control_proof(
            &signed(&payload, &old_key),
            old_public.as_bytes(),
            &request,
            &new_fingerprint,
            session_id,
            "active-challenge",
            now,
        )
        .is_ok());
        assert!(verify_key_control_proof(
            &signed(&payload, &wrong_key),
            old_public.as_bytes(),
            &request,
            &new_fingerprint,
            session_id,
            "active-challenge",
            now,
        )
        .is_err());

        for mutated in [
            TrustedWakeKeyControlProofPayload {
                domain: "jarvis.trusted-wake.event.v1".to_string(),
                ..payload.clone()
            },
            TrustedWakeKeyControlProofPayload {
                session_id: Uuid::new_v4(),
                ..payload.clone()
            },
            TrustedWakeKeyControlProofPayload {
                challenge: "wrong-challenge".to_string(),
                ..payload.clone()
            },
            TrustedWakeKeyControlProofPayload {
                occurred_at: now - Duration::seconds(MAX_WAKE_CLOCK_SKEW_SECONDS + 1),
                ..payload.clone()
            },
            TrustedWakeKeyControlProofPayload {
                expected_generation: request.expected_generation + 1,
                ..payload.clone()
            },
            TrustedWakeKeyControlProofPayload {
                expected_fingerprint: "different-old-fingerprint".to_string(),
                ..payload.clone()
            },
            TrustedWakeKeyControlProofPayload {
                new_fingerprint: "different-new-fingerprint".to_string(),
                ..payload.clone()
            },
        ] {
            assert!(verify_key_control_proof(
                &signed(&mutated, &old_key),
                old_public.as_bytes(),
                &request,
                &new_fingerprint,
                session_id,
                "active-challenge",
                now,
            )
            .is_err());
        }

        let mut malformed = signed(&payload, &old_key);
        malformed.signature_der_b64 = STANDARD.encode([0_u8; 64]);
        assert!(verify_key_control_proof(
            &malformed,
            old_public.as_bytes(),
            &request,
            &new_fingerprint,
            session_id,
            "active-challenge",
            now,
        )
        .is_err());
        let oversized = TrustedWakeKeyControlProof {
            payload_b64: "A".repeat(encoded_limit(MAX_WAKE_PAYLOAD_BYTES) + 1),
            signature_der_b64: STANDARD.encode([1_u8; 8]),
        };
        assert!(verify_key_control_proof(
            &oversized,
            old_public.as_bytes(),
            &request,
            &new_fingerprint,
            session_id,
            "active-challenge",
            now,
        )
        .is_err());
    }
}
