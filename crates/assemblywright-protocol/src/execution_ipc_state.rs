use super::{
    ProtocolError, WindowsExecutionAck, WindowsExecutionControlFrame, WindowsExecutionIpcEndpoint,
    WINDOWS_EXECUTION_IPC_SCHEMA_VERSION,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;
use zeroize::Zeroizing;

const JOURNAL_SCHEMA_VERSION: u16 = 1;
const MAX_JOURNAL_BYTES: u64 = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum DurableIpcError {
    #[error("durable IPC state is invalid or ambiguous")]
    InvalidState,
    #[error("durable IPC state is quarantined")]
    Quarantined,
    #[error("durable IPC state I/O failed")]
    Io,
    #[error("IPC frame contract rejected")]
    Contract(#[from] ProtocolError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DurableIpcAdmission {
    New,
    RecoverPending,
    Replay(WindowsExecutionAck),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PendingFrame {
    frame_id: Uuid,
    sequence: u64,
    nonce: Uuid,
    frame_sha256: [u8; 32],
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LedgerState {
    endpoint: WindowsExecutionIpcEndpoint,
    service_id: Uuid,
    authority_revision: u64,
    next_sequence: u64,
    pending: Option<PendingFrame>,
    last_ack: Option<WindowsExecutionAck>,
    quarantined: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct JournalRecord {
    schema_version: u16,
    previous_record_sha256: [u8; 32],
    state: LedgerState,
}

/// Append-only intent/ack state for one local service endpoint. The caller
/// supplies a service-private protected file location. The journal contains no
/// key material, paths, payloads, or operation content.
pub struct DurableIpcLedger {
    path: PathBuf,
    file: File,
    #[cfg(windows)]
    _held_parent_ancestry: Vec<File>,
    state: LedgerState,
    last_record_sha256: [u8; 32],
}

impl DurableIpcLedger {
    pub fn open(
        path: impl AsRef<Path>,
        endpoint: WindowsExecutionIpcEndpoint,
        service_id: Uuid,
        authority_revision: u64,
        initial_sequence: u64,
    ) -> Result<Self, DurableIpcError> {
        if service_id.is_nil() || authority_revision == 0 || initial_sequence == 0 {
            return Err(DurableIpcError::InvalidState);
        }
        let path = path.as_ref().to_path_buf();
        if !path.is_absolute() || path.file_name().is_none() {
            return Err(DurableIpcError::InvalidState);
        }
        #[cfg(windows)]
        let held_parent_ancestry = hold_parent_ancestry(&path)?;
        let exists = path.symlink_metadata().is_ok();
        let mut options = OpenOptions::new();
        options.read(true).append(true);
        configure_no_follow(&mut options);
        if !exists {
            options.create_new(true);
        }
        let mut file = options.open(&path).map_err(|_| DurableIpcError::Io)?;
        if !exists {
            protect_new_file(&file)?;
        }
        validate_open_file(&file, MAX_JOURNAL_BYTES)?;
        let (state, last_record_sha256) = if exists {
            load_records(&mut file)?
        } else {
            (
                LedgerState {
                    endpoint,
                    service_id,
                    authority_revision,
                    next_sequence: initial_sequence,
                    pending: None,
                    last_ack: None,
                    quarantined: false,
                },
                [0; 32],
            )
        };
        if state.endpoint != endpoint
            || state.service_id != service_id
            || state.authority_revision != authority_revision
            || state.next_sequence == 0
        {
            return Err(DurableIpcError::InvalidState);
        }
        let mut ledger = Self {
            path,
            file,
            #[cfg(windows)]
            _held_parent_ancestry: held_parent_ancestry,
            state,
            last_record_sha256,
        };
        if !exists {
            ledger.persist()?;
        }
        if ledger.state.quarantined {
            return Err(DurableIpcError::Quarantined);
        }
        Ok(ledger)
    }

    pub fn admit(
        &mut self,
        frame: &WindowsExecutionControlFrame,
    ) -> Result<DurableIpcAdmission, DurableIpcError> {
        if self.state.quarantined {
            return Err(DurableIpcError::Quarantined);
        }
        let frame_sha256 = frame.canonical_sha256()?;
        if frame.endpoint != self.state.endpoint
            || frame.service_id != self.state.service_id
            || frame.authority_revision != self.state.authority_revision
        {
            return self.quarantine();
        }
        if let Some(ack) = &self.state.last_ack {
            if ack.frame_id == frame.frame_id && ack.frame_sha256 == frame_sha256 {
                return Ok(DurableIpcAdmission::Replay(ack.clone()));
            }
        }
        if let Some(pending) = &self.state.pending {
            if pending.frame_id == frame.frame_id
                && pending.sequence == frame.request_sequence
                && pending.nonce == frame.nonce
                && pending.frame_sha256 == frame_sha256
            {
                return Ok(DurableIpcAdmission::RecoverPending);
            }
            return self.quarantine();
        }
        if frame.request_sequence != self.state.next_sequence {
            return self.quarantine();
        }
        self.state.pending = Some(PendingFrame {
            frame_id: frame.frame_id,
            sequence: frame.request_sequence,
            nonce: frame.nonce,
            frame_sha256,
        });
        self.persist()?;
        Ok(DurableIpcAdmission::New)
    }

    /// Returns the original acknowledgement for one byte-exact completed
    /// request without admitting new work. Callers may use this before a
    /// freshness check because no handler is re-run and no new effect is
    /// possible; pending or otherwise changed requests still proceed through
    /// normal admission and freshness validation.
    pub fn completed_replay(
        &self,
        frame: &WindowsExecutionControlFrame,
    ) -> Result<Option<WindowsExecutionAck>, DurableIpcError> {
        if self.state.quarantined {
            return Err(DurableIpcError::Quarantined);
        }
        if frame.endpoint != self.state.endpoint
            || frame.service_id != self.state.service_id
            || frame.authority_revision != self.state.authority_revision
        {
            return Ok(None);
        }
        let frame_sha256 = frame.canonical_sha256()?;
        Ok(self.state.last_ack.as_ref().and_then(|ack| {
            (ack.frame_id == frame.frame_id && ack.frame_sha256 == frame_sha256)
                .then(|| ack.clone())
        }))
    }

    pub fn complete(&mut self, ack: WindowsExecutionAck) -> Result<(), DurableIpcError> {
        let pending = self
            .state
            .pending
            .as_ref()
            .ok_or(DurableIpcError::InvalidState)?;
        if ack.schema_version != WINDOWS_EXECUTION_IPC_SCHEMA_VERSION
            || ack.endpoint != self.state.endpoint
            || ack.frame_id != pending.frame_id
            || ack.request_sequence != pending.sequence
            || ack.authority_revision != self.state.authority_revision
            || ack.frame_sha256 != pending.frame_sha256
            || ack.effects_applied != 0
        {
            return self.quarantine();
        }
        self.state.next_sequence = self
            .state
            .next_sequence
            .checked_add(1)
            .ok_or(DurableIpcError::InvalidState)?;
        self.state.pending = None;
        self.state.last_ack = Some(ack);
        self.persist()
    }

    pub fn quarantine<T>(&mut self) -> Result<T, DurableIpcError> {
        self.state.quarantined = true;
        self.persist()?;
        Err(DurableIpcError::Quarantined)
    }

    pub fn state_path(&self) -> &Path {
        &self.path
    }

    fn persist(&mut self) -> Result<(), DurableIpcError> {
        let record = JournalRecord {
            schema_version: JOURNAL_SCHEMA_VERSION,
            previous_record_sha256: self.last_record_sha256,
            state: self.state.clone(),
        };
        let bytes = serde_json::to_vec(&record).map_err(|_| DurableIpcError::InvalidState)?;
        if bytes.len() > 64 * 1024 {
            return Err(DurableIpcError::InvalidState);
        }
        let current_len = self.file.metadata().map_err(|_| DurableIpcError::Io)?.len();
        if current_len
            .checked_add(bytes.len() as u64)
            .and_then(|length| length.checked_add(1))
            .is_none_or(|length| length > MAX_JOURNAL_BYTES)
        {
            return Err(DurableIpcError::InvalidState);
        }
        self.file
            .write_all(&bytes)
            .and_then(|_| self.file.write_all(b"\n"))
            .and_then(|_| self.file.sync_data())
            .map_err(|_| DurableIpcError::Io)?;
        self.last_record_sha256 = Sha256::digest(&bytes).into();
        Ok(())
    }
}

fn load_records(file: &mut File) -> Result<(LedgerState, [u8; 32]), DurableIpcError> {
    let metadata = file.metadata().map_err(|_| DurableIpcError::Io)?;
    if metadata.len() == 0 || metadata.len() > MAX_JOURNAL_BYTES {
        return Err(DurableIpcError::InvalidState);
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut bytes)
        .map_err(|_| DurableIpcError::Io)?;
    if bytes.last() != Some(&b'\n') {
        return Err(DurableIpcError::InvalidState);
    }
    let mut expected_previous = [0; 32];
    let mut state = None;
    for raw in bytes[..bytes.len() - 1].split(|byte| *byte == b'\n') {
        if raw.is_empty() {
            return Err(DurableIpcError::InvalidState);
        }
        let record: JournalRecord =
            serde_json::from_slice(raw).map_err(|_| DurableIpcError::InvalidState)?;
        if record.schema_version != JOURNAL_SCHEMA_VERSION
            || record.previous_record_sha256 != expected_previous
        {
            return Err(DurableIpcError::InvalidState);
        }
        expected_previous = Sha256::digest(raw).into();
        state = Some(record.state);
    }
    Ok((
        state.ok_or(DurableIpcError::InvalidState)?,
        expected_previous,
    ))
}

fn validate_file(path: &Path) -> Result<(), DurableIpcError> {
    let mut options = OpenOptions::new();
    options.read(true);
    configure_no_follow(&mut options);
    let file = options
        .open(path)
        .map_err(|_| DurableIpcError::InvalidState)?;
    validate_open_file(&file, MAX_JOURNAL_BYTES)
}

fn validate_open_file(file: &File, maximum: u64) -> Result<(), DurableIpcError> {
    let metadata = file.metadata().map_err(|_| DurableIpcError::InvalidState)?;
    if !metadata.file_type().is_file() || metadata.len() > maximum {
        return Err(DurableIpcError::InvalidState);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.nlink() != 1
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err(DurableIpcError::InvalidState);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
        };
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(DurableIpcError::InvalidState);
        }
        let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) } == 0
            || information.nNumberOfLinks != 1
        {
            return Err(DurableIpcError::InvalidState);
        }
    }
    Ok(())
}

#[cfg(unix)]
fn configure_no_follow(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt;
    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
}

#[cfg(windows)]
fn configure_no_follow(options: &mut OpenOptions) {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
    options.custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
}

#[cfg(not(any(unix, windows)))]
fn configure_no_follow(_options: &mut OpenOptions) {}

/// Loads one service-private signing seed from a separately ACL-protected,
/// ordinary, single-link leaf. The caller must clear the returned stack buffer
/// immediately after constructing its signing key.
pub fn load_service_signing_seed(path: &Path) -> Result<Zeroizing<[u8; 32]>, DurableIpcError> {
    if !path.is_absolute() {
        return Err(DurableIpcError::InvalidState);
    }
    #[cfg(windows)]
    let _held_parent_ancestry = hold_parent_ancestry(path)?;
    validate_file(path)?;
    let metadata = path.metadata().map_err(|_| DurableIpcError::InvalidState)?;
    if metadata.len() != 32 {
        return Err(DurableIpcError::InvalidState);
    }
    let mut options = OpenOptions::new();
    options.read(true);
    configure_no_follow(&mut options);
    let mut file = options
        .open(path)
        .map_err(|_| DurableIpcError::InvalidState)?;
    validate_open_file(&file, 32)?;
    let mut seed = Zeroizing::new([0_u8; 32]);
    file.read_exact(seed.as_mut())
        .map_err(|_| DurableIpcError::InvalidState)?;
    let mut extra = [0_u8; 1];
    if *seed == [0; 32] || file.read(&mut extra).map_err(|_| DurableIpcError::Io)? != 0 {
        return Err(DurableIpcError::InvalidState);
    }
    Ok(seed)
}

#[cfg(windows)]
fn hold_parent_ancestry(path: &Path) -> Result<Vec<File>, DurableIpcError> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        FILE_READ_ATTRIBUTES, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };

    let parent = path.parent().ok_or(DurableIpcError::InvalidState)?;
    let handle = OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(parent)
        .map_err(|_| DurableIpcError::InvalidState)?;
    let metadata = handle
        .metadata()
        .map_err(|_| DurableIpcError::InvalidState)?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(DurableIpcError::InvalidState);
    }
    Ok(vec![handle])
}

fn protect_new_file(_file: &File) -> Result<(), DurableIpcError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        _file
            .set_permissions(std::fs::Permissions::from_mode(0o600))
            .map_err(|_| DurableIpcError::Io)?;
    }
    Ok(())
}
