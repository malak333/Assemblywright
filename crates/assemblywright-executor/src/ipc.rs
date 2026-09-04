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
pub enum ExecutorIpcError {
    #[error("executor IPC rejected and quarantined")]
    Rejected,
    #[error("executor IPC durable state failed")]
    Durable(#[from] DurableIpcError),
    #[error("executor IPC contract failed")]
    Contract(#[from] ProtocolError),
}

pub struct InertExecutorIpc {
    service_id: Uuid,
    authority_key_id: String,
    authority_key: VerifyingKey,
    ack_key_id: String,
    ack_key: SigningKey,
    ledger: DurableIpcLedger,
}

impl InertExecutorIpc {
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
    ) -> Result<Self, ExecutorIpcError> {
        if authority_key_id.is_empty() || ack_key_id.is_empty() || *ack_signing_seed == [0; 32] {
            return Err(ExecutorIpcError::Rejected);
        }
        Ok(Self {
            service_id,
            authority_key_id,
            authority_key,
            ack_key_id,
            ack_key: SigningKey::from_bytes(ack_signing_seed),
            ledger: DurableIpcLedger::open(
                state_path,
                WindowsExecutionIpcEndpoint::MasterToExecutor,
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
    ) -> Result<WindowsExecutionAck, ExecutorIpcError> {
        let frame = match WindowsExecutionControlFrame::decode_frame(bytes).and_then(|frame| {
            frame.verify_signature(&self.authority_key)?;
            Ok(frame)
        }) {
            Ok(frame) => frame,
            Err(_) => return self.reject(),
        };
        if frame.endpoint != WindowsExecutionIpcEndpoint::MasterToExecutor
            || frame.service_id != self.service_id
            || frame.signer_key_id != self.authority_key_id
            || !frame.forwarded_executor_frame.is_empty()
        {
            return self.reject();
        }
        if let Some(ack) = self.ledger.completed_replay(&frame)? {
            return Ok(ack);
        }
        if frame.validate_at(now_ms).is_err() {
            return self.reject();
        }
        match self.ledger.admit(&frame)? {
            DurableIpcAdmission::Replay(ack) => return Ok(ack),
            DurableIpcAdmission::New | DurableIpcAdmission::RecoverPending => {}
        }
        let status = match frame.kind {
            WindowsExecutionControlKind::Health => WindowsExecutionAckStatus::HealthyEffectDisabled,
            WindowsExecutionControlKind::ValidateDispatch => {
                WindowsExecutionAckStatus::DispatchValidatedEffectDisabled
            }
        };
        let mut ack = WindowsExecutionAck {
            schema_version: WINDOWS_EXECUTION_IPC_SCHEMA_VERSION,
            endpoint: WindowsExecutionIpcEndpoint::MasterToExecutor,
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
        Ok(ack)
    }

    fn reject<T>(&mut self) -> Result<T, ExecutorIpcError> {
        let _ = self.ledger.quarantine::<()>();
        Err(ExecutorIpcError::Rejected)
    }
}

pub fn load_ack_seed(path: &Path) -> Result<Zeroizing<[u8; 32]>, ExecutorIpcError> {
    load_service_signing_seed(path).map_err(|_| ExecutorIpcError::Rejected)
}
