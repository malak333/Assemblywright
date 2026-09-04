use assemblywright_protocol::{
    ProtocolError, WindowsExecutionAck, WindowsExecutionControlFrame, WindowsExecutionControlKind,
    WindowsExecutionIpcEndpoint, WINDOWS_EXECUTION_IPC_SCHEMA_VERSION,
};
use ed25519_dalek::{SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct WindowsExecutionIpcBinding {
    pub master_id: Uuid,
    pub broker_id: Uuid,
    pub executor_id: Uuid,
    pub session_id: Uuid,
    pub session_revision: u64,
    pub child_epoch_id: Uuid,
    pub child_epoch_revision: u64,
    pub feature_lifecycle_revision: u64,
    pub authority_revision: u64,
    pub authority_key_id: String,
    pub broker_ack_key_id: String,
    pub broker_ack_key: VerifyingKey,
    pub executor_ack_key_id: String,
    pub executor_ack_key: VerifyingKey,
}

pub struct InertWindowsExecutionIpcFoundation {
    binding: WindowsExecutionIpcBinding,
    authority_key: SigningKey,
}

impl InertWindowsExecutionIpcFoundation {
    pub fn new(
        binding: WindowsExecutionIpcBinding,
        authority_key: SigningKey,
    ) -> Result<Self, ProtocolError> {
        if binding.master_id.is_nil()
            || binding.broker_id.is_nil()
            || binding.executor_id.is_nil()
            || binding.master_id == binding.broker_id
            || binding.master_id == binding.executor_id
            || binding.broker_id == binding.executor_id
            || binding.session_id.is_nil()
            || binding.child_epoch_id.is_nil()
            || binding.session_revision == 0
            || binding.child_epoch_revision == 0
            || binding.feature_lifecycle_revision == 0
            || binding.authority_revision == 0
            || binding.authority_key_id.is_empty()
            || binding.broker_ack_key_id.is_empty()
            || binding.executor_ack_key_id.is_empty()
        {
            return Err(ProtocolError::InvalidFullMachineAssemblyLine);
        }
        Ok(Self {
            binding,
            authority_key,
        })
    }

    /// Produces the two independently signed, inert dispatch-validation hops.
    /// This method does not contact a service and cannot execute an adapter.
    pub fn sign_dispatch_validation(
        &self,
        broker_sequence: u64,
        executor_sequence: u64,
        issued_at_ms: u64,
        expires_at_ms: u64,
    ) -> Result<(WindowsExecutionControlFrame, WindowsExecutionControlFrame), ProtocolError> {
        let executor = self.sign_frame(
            WindowsExecutionIpcEndpoint::MasterToExecutor,
            self.binding.executor_id,
            executor_sequence,
            issued_at_ms,
            expires_at_ms,
            WindowsExecutionControlKind::ValidateDispatch,
            Vec::new(),
        )?;
        let executor_bytes = executor.encode_frame()?;
        let broker = self.sign_frame(
            WindowsExecutionIpcEndpoint::MasterToBroker,
            self.binding.broker_id,
            broker_sequence,
            issued_at_ms,
            expires_at_ms,
            WindowsExecutionControlKind::ValidateDispatch,
            executor_bytes,
        )?;
        Ok((broker, executor))
    }

    pub fn sign_health(
        &self,
        endpoint: WindowsExecutionIpcEndpoint,
        sequence: u64,
        issued_at_ms: u64,
        expires_at_ms: u64,
    ) -> Result<WindowsExecutionControlFrame, ProtocolError> {
        let service_id = match endpoint {
            WindowsExecutionIpcEndpoint::MasterToBroker => self.binding.broker_id,
            WindowsExecutionIpcEndpoint::MasterToExecutor => self.binding.executor_id,
        };
        self.sign_frame(
            endpoint,
            service_id,
            sequence,
            issued_at_ms,
            expires_at_ms,
            WindowsExecutionControlKind::Health,
            Vec::new(),
        )
    }

    pub fn verify_ack(
        &self,
        frame: &WindowsExecutionControlFrame,
        ack: &WindowsExecutionAck,
    ) -> Result<(), ProtocolError> {
        match frame.endpoint {
            WindowsExecutionIpcEndpoint::MasterToBroker => ack.verify_for(
                frame,
                &self.binding.broker_ack_key_id,
                &self.binding.broker_ack_key,
            ),
            WindowsExecutionIpcEndpoint::MasterToExecutor => ack.verify_for(
                frame,
                &self.binding.executor_ack_key_id,
                &self.binding.executor_ack_key,
            ),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn sign_frame(
        &self,
        endpoint: WindowsExecutionIpcEndpoint,
        service_id: Uuid,
        request_sequence: u64,
        issued_at_ms: u64,
        expires_at_ms: u64,
        kind: WindowsExecutionControlKind,
        forwarded_executor_frame: Vec<u8>,
    ) -> Result<WindowsExecutionControlFrame, ProtocolError> {
        let forwarded_executor_frame_sha256 = if forwarded_executor_frame.is_empty() {
            [0; 32]
        } else {
            Sha256::digest(&forwarded_executor_frame).into()
        };
        let mut frame = WindowsExecutionControlFrame {
            schema_version: WINDOWS_EXECUTION_IPC_SCHEMA_VERSION,
            endpoint,
            frame_id: Uuid::new_v4(),
            request_sequence,
            nonce: Uuid::new_v4(),
            master_id: self.binding.master_id,
            service_id,
            session_id: self.binding.session_id,
            session_revision: self.binding.session_revision,
            child_epoch_id: self.binding.child_epoch_id,
            child_epoch_revision: self.binding.child_epoch_revision,
            feature_lifecycle_revision: self.binding.feature_lifecycle_revision,
            authority_revision: self.binding.authority_revision,
            issued_at_ms,
            expires_at_ms,
            kind,
            forwarded_executor_frame_sha256,
            forwarded_executor_frame,
            signer_key_id: self.binding.authority_key_id.clone(),
            signature: Vec::new(),
        };
        frame.sign(&self.authority_key)?;
        Ok(frame)
    }
}
