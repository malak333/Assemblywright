use assemblywright_protocol::execution_ipc_state::{
    load_service_signing_seed, DurableIpcAdmission, DurableIpcError, DurableIpcLedger,
};
use assemblywright_protocol::{
    ProtocolError, WindowsExecutionAck, WindowsExecutionAckStatus, WindowsExecutionControlFrame,
    WindowsExecutionControlKind, WindowsExecutionIpcEndpoint, WINDOWS_EXECUTION_IPC_SCHEMA_VERSION,
};
use ed25519_dalek::{SigningKey, VerifyingKey};
use std::path::Path;
use uuid::Uuid;
use zeroize::Zeroizing;

#[derive(Debug, thiserror::Error)]
pub enum BrokerIpcError {
    #[error("broker IPC rejected and quarantined")]
    Rejected,
    #[error("broker IPC durable state failed")]
    Durable(#[from] DurableIpcError),
    #[error("broker IPC contract failed")]
    Contract(#[from] ProtocolError),
}

#[derive(Debug)]
pub struct BrokerIpcAccepted {
    pub ack: WindowsExecutionAck,
    pub forwarded_executor_frame: Option<Vec<u8>>,
}

pub struct InertBrokerIpc {
    service_id: Uuid,
    authority_key_id: String,
    authority_key: VerifyingKey,
    ack_key_id: String,
    ack_key: SigningKey,
    ledger: DurableIpcLedger,
}

impl InertBrokerIpc {
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        state_path: impl AsRef<Path>,
        service_id: Uuid,
        authority_revision: u64,
        initial_sequence: u64,
        authority_key_id: String,
        authority_key: VerifyingKey,
        ack_key_id: String,
        ack_signing_seed: &[u8; 32],
    ) -> Result<Self, BrokerIpcError> {
        if authority_key_id.is_empty() || ack_key_id.is_empty() || *ack_signing_seed == [0; 32] {
            return Err(BrokerIpcError::Rejected);
        }
        Ok(Self {
            service_id,
            authority_key_id,
            authority_key,
            ack_key_id,
            ack_key: SigningKey::from_bytes(ack_signing_seed),
            ledger: DurableIpcLedger::open(
                state_path,
                WindowsExecutionIpcEndpoint::MasterToBroker,
                service_id,
                authority_revision,
                initial_sequence,
            )?,
        })
    }

    pub fn handle(
        &mut self,
        bytes: &[u8],
        now_ms: u64,
    ) -> Result<BrokerIpcAccepted, BrokerIpcError> {
        let frame = match WindowsExecutionControlFrame::decode_frame(bytes).and_then(|frame| {
            frame.verify_signature(&self.authority_key)?;
            Ok(frame)
        }) {
            Ok(frame) => frame,
            Err(_) => return self.reject(),
        };
        if frame.endpoint != WindowsExecutionIpcEndpoint::MasterToBroker
            || frame.service_id != self.service_id
            || frame.signer_key_id != self.authority_key_id
        {
            return self.reject();
        }
        if let Some(ack) = self.ledger.completed_replay(&frame)? {
            let forwarded_executor_frame = (frame.kind
                == WindowsExecutionControlKind::ValidateDispatch)
                .then(|| frame.forwarded_executor_frame.clone());
            return Ok(BrokerIpcAccepted {
                ack,
                forwarded_executor_frame,
            });
        }
        if frame.validate_at(now_ms).is_err() {
            return self.reject();
        }
        match self.ledger.admit(&frame)? {
            DurableIpcAdmission::Replay(ack) => {
                let forwarded_executor_frame = (frame.kind
                    == WindowsExecutionControlKind::ValidateDispatch)
                    .then(|| frame.forwarded_executor_frame.clone());
                return Ok(BrokerIpcAccepted {
                    ack,
                    forwarded_executor_frame,
                });
            }
            DurableIpcAdmission::New | DurableIpcAdmission::RecoverPending => {}
        }
        let (status, forwarded_executor_frame) = match frame.kind {
            WindowsExecutionControlKind::Health => {
                (WindowsExecutionAckStatus::HealthyEffectDisabled, None)
            }
            WindowsExecutionControlKind::ValidateDispatch => {
                let executor = match frame.forwarded_executor() {
                    Ok(Some(executor)) => executor,
                    Ok(None) | Err(_) => return self.reject(),
                };
                if executor.signer_key_id != self.authority_key_id
                    || executor.verify_signature(&self.authority_key).is_err()
                {
                    return self.reject();
                }
                (
                    WindowsExecutionAckStatus::DispatchValidatedEffectDisabled,
                    Some(frame.forwarded_executor_frame.clone()),
                )
            }
        };
        let mut ack = WindowsExecutionAck {
            schema_version: WINDOWS_EXECUTION_IPC_SCHEMA_VERSION,
            endpoint: WindowsExecutionIpcEndpoint::MasterToBroker,
            ack_id: Uuid::new_v4(),
            frame_id: frame.frame_id,
            request_sequence: frame.request_sequence,
            authority_revision: frame.authority_revision,
            frame_sha256: frame.canonical_sha256()?,
            status,
            effects_applied: 0,
            signer_key_id: self.ack_key_id.clone(),
            signature: Vec::new(),
        };
        ack.sign(&self.ack_key)?;
        self.ledger.complete(ack.clone())?;
        Ok(BrokerIpcAccepted {
            ack,
            forwarded_executor_frame,
        })
    }

    fn reject<T>(&mut self) -> Result<T, BrokerIpcError> {
        let _ = self.ledger.quarantine::<()>();
        Err(BrokerIpcError::Rejected)
    }
}

pub fn load_ack_seed(path: &Path) -> Result<Zeroizing<[u8; 32]>, BrokerIpcError> {
    load_service_signing_seed(path).map_err(|_| BrokerIpcError::Rejected)
}
