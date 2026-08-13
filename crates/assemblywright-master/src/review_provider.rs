use assemblywright_protocol::{
    FeatureConveyorReviewGatewayRequest, FeatureConveyorReviewPacket,
    FeatureConveyorReviewProviderOutput, MAX_FEATURE_CONVEYOR_REVIEW_OUTPUT_BYTES,
    MAX_FEATURE_CONVEYOR_REVIEW_PACKET_BYTES,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const REVIEW_PROVIDER_CONFIG_FILE: &str = "provider.json";
const REVIEW_PROVIDER_TIMEOUT: Duration = Duration::from_secs(120);
#[cfg(windows)]
const REVIEW_PROVIDER_LAUNCHER_MARKER: &str = "__assemblywright_review_provider_launcher_v1";
#[cfg(windows)]
const REVIEW_PROVIDER_LAUNCH_GATE: u8 = 0xa7;

#[cfg(windows)]
fn hex_digest(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

#[cfg(windows)]
fn hex_sha256(bytes: &[u8]) -> String {
    hex_digest(&Sha256::digest(bytes))
}

struct VerifiedReviewExecutable {
    _file: File,
}

#[cfg(unix)]
struct ReviewChildContainment {
    process_group: i32,
}

#[cfg(unix)]
fn spawn_contained_review_child(
    command: &mut Command,
) -> Result<(std::process::Child, ReviewChildContainment), ReviewProviderTransportError> {
    use std::os::unix::process::CommandExt;
    command.process_group(0);
    let child = command
        .spawn()
        .map_err(|_| ReviewProviderTransportError::Outage)?;
    let process_group =
        i32::try_from(child.id()).map_err(|_| ReviewProviderTransportError::Outage)?;
    Ok((child, ReviewChildContainment { process_group }))
}

#[cfg(unix)]
impl ReviewChildContainment {
    fn terminate(&self) {
        unsafe {
            libc::kill(-self.process_group, libc::SIGKILL);
        }
    }
}

#[cfg(windows)]
pub fn review_provider_launcher_exit_code() -> Option<i32> {
    use std::ffi::OsStr;
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
    use windows_sys::Win32::Storage::FileSystem::ReadFile;
    use windows_sys::Win32::System::Console::{GetStdHandle, STD_INPUT_HANDLE};

    let mut arguments = std::env::args_os().skip(1);
    if arguments.next().as_deref() != Some(OsStr::new(REVIEW_PROVIDER_LAUNCHER_MARKER)) {
        return None;
    }
    let executable = match arguments.next() {
        Some(executable) => PathBuf::from(executable),
        None => return Some(1),
    };
    let mode = match arguments.next() {
        Some(mode) => mode,
        None => return Some(1),
    };
    let expected_sha256 = match arguments.next().and_then(|value| value.into_string().ok()) {
        Some(value) => value,
        None => return Some(1),
    };
    let expected_length = match arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .and_then(|value| value.parse::<u64>().ok())
    {
        Some(value) => value,
        None => return Some(1),
    };
    if arguments.next().is_some()
        || executable.file_name() != Some(OsStr::new("review-provider.exe"))
        || executable.parent().and_then(Path::file_name) != Some(OsStr::new("review-provider"))
        || !matches!(mode.to_str(), Some("review" | "count_tokens"))
    {
        return Some(1);
    }
    let mut verified_provider = {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
        };
        let mut file = match OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(&executable)
        {
            Ok(file) => file,
            Err(_) => return Some(1),
        };
        let metadata = match file.metadata() {
            Ok(metadata) => metadata,
            Err(_) => return Some(1),
        };
        use std::os::windows::fs::MetadataExt;
        if !metadata.is_file()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || metadata.len() != expected_length
        {
            return Some(1);
        }
        let mut bytes = Vec::with_capacity(match usize::try_from(expected_length) {
            Ok(length) => length,
            Err(_) => return Some(1),
        });
        if file.read_to_end(&mut bytes).is_err()
            || hex_sha256(&bytes) != expected_sha256
            || file.seek(SeekFrom::Start(0)).is_err()
        {
            return Some(1);
        }
        file
    };
    let stdin = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
    if stdin.is_null() || stdin == INVALID_HANDLE_VALUE {
        return Some(1);
    }
    let mut gate = 0_u8;
    let mut read = 0_u32;
    if unsafe {
        ReadFile(
            stdin,
            (&mut gate as *mut u8).cast(),
            1,
            &mut read,
            null_mut(),
        )
    } == 0
        || read != 1
        || gate != REVIEW_PROVIDER_LAUNCH_GATE
    {
        return Some(1);
    }
    let mut command = Command::new(&executable);
    if mode == OsStr::new("count_tokens") {
        command.arg("--count-tokens");
    }
    command
        .current_dir(executable.parent().expect("validated provider parent"))
        .env_clear()
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::null());
    let _verified_provider_guard = &mut verified_provider;
    Some(match command.spawn().and_then(|mut child| child.wait()) {
        Ok(status) if status.success() => 0,
        _ => 1,
    })
}

#[cfg(windows)]
struct ReviewChildContainment {
    job: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
fn spawn_contained_review_child(
    command: &mut Command,
) -> Result<(std::process::Child, ReviewChildContainment), ReviewProviderTransportError> {
    use std::mem::{size_of, zeroed};
    use std::os::windows::io::AsRawHandle;
    use std::ptr::null;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };
    let job = unsafe { CreateJobObjectW(null(), null()) };
    if job.is_null() {
        return Err(ReviewProviderTransportError::Outage);
    }
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if unsafe {
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    } == 0
    {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(job) };
        return Err(ReviewProviderTransportError::Outage);
    }
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => {
            unsafe { windows_sys::Win32::Foundation::CloseHandle(job) };
            return Err(ReviewProviderTransportError::Outage);
        }
    };
    if unsafe {
        AssignProcessToJobObject(
            job,
            child.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE,
        )
    } == 0
    {
        let _ = child.kill();
        let _ = child.wait();
        unsafe { windows_sys::Win32::Foundation::CloseHandle(job) };
        return Err(ReviewProviderTransportError::Outage);
    }
    if child
        .stdin
        .as_mut()
        .is_none_or(|stdin| stdin.write_all(&[REVIEW_PROVIDER_LAUNCH_GATE]).is_err())
    {
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(job, 1);
        }
        let _ = child.kill();
        let _ = child.wait();
        unsafe { windows_sys::Win32::Foundation::CloseHandle(job) };
        return Err(ReviewProviderTransportError::Outage);
    }
    Ok((child, ReviewChildContainment { job }))
}

#[cfg(windows)]
impl ReviewChildContainment {
    fn terminate(&self) {
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.job, 1);
        }
    }
}

#[cfg(windows)]
impl Drop for ReviewChildContainment {
    fn drop(&mut self) {
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.job);
        }
    }
}

#[cfg(not(any(unix, windows)))]
struct ReviewChildContainment;

#[cfg(not(any(unix, windows)))]
fn spawn_contained_review_child(
    command: &mut Command,
) -> Result<(std::process::Child, ReviewChildContainment), ReviewProviderTransportError> {
    command
        .spawn()
        .map(|child| (child, ReviewChildContainment))
        .map_err(|_| ReviewProviderTransportError::Outage)
}

#[cfg(not(any(unix, windows)))]
impl ReviewChildContainment {
    fn terminate(&self) {}
}

pub const MAX_FEATURE_CONVEYOR_REVIEW_INPUT_TOKENS: u64 = 64_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewProviderCapabilities {
    pub provider_id: String,
    pub model_id: String,
    pub max_input_bytes: usize,
    pub max_input_tokens: u64,
    pub max_output_bytes: usize,
    pub strict_structured_output: bool,
    pub response_only: bool,
    pub fresh_session_per_call: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewProviderTransportError {
    Outage,
    IncompleteTransport,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReviewProviderTokenCountError;

#[derive(Debug, thiserror::Error)]
pub enum ReviewProviderConfigError {
    #[error("review provider configuration is incomplete or invalid")]
    Invalid,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProcessReviewProviderConfig {
    schema_version: u16,
    provider_id: String,
    model_id: String,
    max_input_tokens: u64,
}

#[derive(Debug)]
pub struct ProcessReviewProvider {
    executable: PathBuf,
    #[cfg(windows)]
    launcher_executable: PathBuf,
    working_directory: PathBuf,
    executable_identity: ReviewExecutableIdentity,
    capabilities: ReviewProviderCapabilities,
}

#[derive(Debug)]
struct ReviewExecutableIdentity {
    sha256: [u8; 32],
    length: u64,
    modified: std::time::SystemTime,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
}

impl ProcessReviewProvider {
    pub fn load(data_dir: &Path) -> Result<Option<Self>, ReviewProviderConfigError> {
        let launcher_executable = std::env::current_exe()?;
        Self::load_with_launcher(data_dir, launcher_executable)
    }

    #[cfg(all(windows, debug_assertions))]
    #[doc(hidden)]
    pub fn load_with_launcher_for_test(
        data_dir: &Path,
        launcher_executable: &Path,
    ) -> Result<Option<Self>, ReviewProviderConfigError> {
        Self::load_with_launcher(data_dir, launcher_executable.to_path_buf())
    }

    fn load_with_launcher(
        data_dir: &Path,
        launcher_executable: PathBuf,
    ) -> Result<Option<Self>, ReviewProviderConfigError> {
        #[cfg(not(windows))]
        let _ = launcher_executable;
        let root = data_dir.join("review-provider");
        let config_path = root.join(REVIEW_PROVIDER_CONFIG_FILE);
        let executable_path = root.join(if cfg!(windows) {
            "review-provider.exe"
        } else {
            "review-provider"
        });
        if !root.exists() && !config_path.exists() && !executable_path.exists() {
            return Ok(None);
        }
        if fs::symlink_metadata(&root)?.file_type().is_symlink()
            || fs::symlink_metadata(&config_path)?.file_type().is_symlink()
            || fs::symlink_metadata(&executable_path)?
                .file_type()
                .is_symlink()
        {
            return Err(ReviewProviderConfigError::Invalid);
        }
        let root = fs::canonicalize(&root)?;
        let config_path = fs::canonicalize(config_path)?;
        let executable = fs::canonicalize(executable_path)?;
        if config_path.parent() != Some(root.as_path())
            || executable.parent() != Some(root.as_path())
            || !fs::metadata(&config_path)?.is_file()
            || !fs::metadata(&executable)?.is_file()
        {
            return Err(ReviewProviderConfigError::Invalid);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            let root_metadata = fs::metadata(&root)?;
            let config_metadata = fs::metadata(&config_path)?;
            let executable_metadata = fs::metadata(&executable)?;
            let effective_uid = unsafe { libc::geteuid() };
            if root_metadata.uid() != effective_uid
                || config_metadata.uid() != effective_uid
                || executable_metadata.uid() != effective_uid
                || root_metadata.permissions().mode() & 0o077 != 0
                || config_metadata.permissions().mode() & 0o077 != 0
                || executable_metadata.permissions().mode() & 0o022 != 0
                || executable_metadata.permissions().mode() & 0o111 == 0
                || executable_metadata.nlink() != 1
            {
                return Err(ReviewProviderConfigError::Invalid);
            }
        }
        let bytes = fs::read(config_path)?;
        if bytes.is_empty() || bytes.len() > 4096 {
            return Err(ReviewProviderConfigError::Invalid);
        }
        let config: ProcessReviewProviderConfig = serde_json::from_slice(&bytes)?;
        if config.schema_version != 1
            || config.provider_id.is_empty()
            || config.provider_id.len() > 128
            || config.model_id.is_empty()
            || config.model_id.len() > 128
            || config.max_input_tokens < MAX_FEATURE_CONVEYOR_REVIEW_INPUT_TOKENS
        {
            return Err(ReviewProviderConfigError::Invalid);
        }
        let executable_bytes = fs::read(&executable)?;
        if executable_bytes.is_empty() || executable_bytes.len() > 32 * 1024 * 1024 {
            return Err(ReviewProviderConfigError::Invalid);
        }
        let executable_metadata = fs::metadata(&executable)?;
        let executable_identity = ReviewExecutableIdentity {
            sha256: Sha256::digest(&executable_bytes).into(),
            length: executable_metadata.len(),
            modified: executable_metadata.modified()?,
            #[cfg(unix)]
            device: {
                use std::os::unix::fs::MetadataExt;
                executable_metadata.dev()
            },
            #[cfg(unix)]
            inode: {
                use std::os::unix::fs::MetadataExt;
                executable_metadata.ino()
            },
        };
        Ok(Some(Self {
            executable,
            #[cfg(windows)]
            launcher_executable,
            working_directory: root,
            executable_identity,
            capabilities: ReviewProviderCapabilities {
                provider_id: config.provider_id,
                model_id: config.model_id,
                max_input_bytes: MAX_FEATURE_CONVEYOR_REVIEW_PACKET_BYTES,
                max_input_tokens: config.max_input_tokens,
                max_output_bytes: MAX_FEATURE_CONVEYOR_REVIEW_OUTPUT_BYTES,
                strict_structured_output: true,
                response_only: true,
                fresh_session_per_call: true,
            },
        }))
    }

    fn open_verified_executable(
        &self,
    ) -> Result<VerifiedReviewExecutable, ReviewProviderTransportError> {
        let path_metadata = fs::symlink_metadata(&self.executable)
            .map_err(|_| ReviewProviderTransportError::Outage)?;
        if path_metadata.file_type().is_symlink() {
            return Err(ReviewProviderTransportError::Outage);
        }
        #[cfg(unix)]
        let mut file = {
            use std::os::unix::fs::OpenOptionsExt;
            OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&self.executable)
                .map_err(|_| ReviewProviderTransportError::Outage)?
        };
        #[cfg(windows)]
        let mut file = {
            use std::os::windows::fs::OpenOptionsExt;
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
            };
            OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(&self.executable)
                .map_err(|_| ReviewProviderTransportError::Outage)?
        };
        #[cfg(not(any(unix, windows)))]
        let mut file =
            File::open(&self.executable).map_err(|_| ReviewProviderTransportError::Outage)?;
        let metadata = file
            .metadata()
            .map_err(|_| ReviewProviderTransportError::Outage)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_file()
            || metadata.len() != self.executable_identity.length
            || metadata.modified().ok() != Some(self.executable_identity.modified)
        {
            return Err(ReviewProviderTransportError::Outage);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if metadata.dev() != self.executable_identity.device
                || metadata.ino() != self.executable_identity.inode
                || metadata.uid() != unsafe { libc::geteuid() }
                || metadata.mode() & 0o022 != 0
                || metadata.nlink() != 1
            {
                return Err(ReviewProviderTransportError::Outage);
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
            if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                return Err(ReviewProviderTransportError::Outage);
            }
        }
        let mut bytes = Vec::with_capacity(
            usize::try_from(self.executable_identity.length)
                .map_err(|_| ReviewProviderTransportError::Outage)?,
        );
        file.read_to_end(&mut bytes)
            .map_err(|_| ReviewProviderTransportError::Outage)?;
        if <[u8; 32]>::from(Sha256::digest(&bytes)) != self.executable_identity.sha256 {
            return Err(ReviewProviderTransportError::Outage);
        }
        file.seek(SeekFrom::Start(0))
            .map_err(|_| ReviewProviderTransportError::Outage)?;
        Ok(VerifiedReviewExecutable { _file: file })
    }

    fn command_for_verified_executable(
        &self,
        _verified: &VerifiedReviewExecutable,
        arguments: &[&str],
    ) -> Result<Command, ReviewProviderTransportError> {
        #[cfg(unix)]
        let mut command = Command::new(&self.executable);
        #[cfg(windows)]
        let mut command = {
            let mut command = Command::new(&self.launcher_executable);
            command
                .arg(REVIEW_PROVIDER_LAUNCHER_MARKER)
                .arg(&self.executable);
            match arguments {
                [] => command.arg("review"),
                ["--count-tokens"] => command.arg("count_tokens"),
                _ => return Err(ReviewProviderTransportError::Outage),
            };
            command
                .arg(hex_digest(&self.executable_identity.sha256))
                .arg(self.executable_identity.length.to_string());
            command
        };
        #[cfg(not(any(unix, windows)))]
        let mut command = Command::new(&self.executable);
        #[cfg(not(windows))]
        command.args(arguments);
        command
            .current_dir(&self.working_directory)
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped());
        command.stderr(Stdio::null());
        Ok(command)
    }

    fn invoke_process(
        &self,
        arguments: &[&str],
        input_bytes: &[u8],
        output_limit: usize,
        cancelled: &AtomicBool,
    ) -> Result<Vec<u8>, ReviewProviderTransportError> {
        let verified = self.open_verified_executable()?;
        let mut command = self.command_for_verified_executable(&verified, arguments)?;
        let _executable_guard = verified;
        let (mut child, containment) = spawn_contained_review_child(&mut command)?;
        #[cfg(not(windows))]
        let _post_spawn_guard = match self.open_verified_executable() {
            Ok(guard) => guard,
            Err(error) => {
                containment.terminate();
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        let mut stdin = match child.stdin.take() {
            Some(stdin) => stdin,
            None => {
                containment.terminate();
                let _ = child.kill();
                let _ = child.wait();
                return Err(ReviewProviderTransportError::IncompleteTransport);
            }
        };
        let mut stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                containment.terminate();
                let _ = child.kill();
                let _ = child.wait();
                return Err(ReviewProviderTransportError::IncompleteTransport);
            }
        };
        let input = input_bytes.to_vec();
        let writer = std::thread::spawn(move || stdin.write_all(&input));
        let reader = std::thread::spawn(move || {
            let mut retained = Vec::new();
            let mut buffer = [0_u8; 8192];
            let mut oversized = false;
            loop {
                let read = stdout.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                let remaining = (output_limit + 1).saturating_sub(retained.len());
                retained.extend_from_slice(&buffer[..read.min(remaining)]);
                oversized |= read > remaining;
            }
            Ok::<_, std::io::Error>((retained, oversized))
        });
        let started = Instant::now();
        let status = loop {
            if cancelled.load(Ordering::Acquire) || started.elapsed() >= REVIEW_PROVIDER_TIMEOUT {
                containment.terminate();
                let _ = child.kill();
                let _ = child.wait();
                let _ = writer.join();
                let _ = reader.join();
                return if cancelled.load(Ordering::Acquire) {
                    Err(ReviewProviderTransportError::Cancelled)
                } else {
                    Err(ReviewProviderTransportError::IncompleteTransport)
                };
            }
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) => std::thread::sleep(Duration::from_millis(25)),
                Err(_) => {
                    containment.terminate();
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = writer.join();
                    let _ = reader.join();
                    return Err(ReviewProviderTransportError::IncompleteTransport);
                }
            }
        };
        containment.terminate();
        writer
            .join()
            .map_err(|_| ReviewProviderTransportError::IncompleteTransport)?
            .map_err(|_| ReviewProviderTransportError::IncompleteTransport)?;
        let (output, oversized) = reader
            .join()
            .map_err(|_| ReviewProviderTransportError::IncompleteTransport)?
            .map_err(|_| ReviewProviderTransportError::IncompleteTransport)?;
        if !status.success() || oversized || output.is_empty() {
            return Err(ReviewProviderTransportError::IncompleteTransport);
        }
        Ok(output)
    }
}

impl ReviewProvider for ProcessReviewProvider {
    fn capabilities(&self) -> Option<ReviewProviderCapabilities> {
        Some(self.capabilities.clone())
    }

    fn count_input_tokens(
        &self,
        canonical_packet: &[u8],
    ) -> Result<u64, ReviewProviderTokenCountError> {
        let output = self
            .invoke_process(
                &["--count-tokens"],
                canonical_packet,
                32,
                &AtomicBool::new(false),
            )
            .map_err(|_| ReviewProviderTokenCountError)?;
        let count = std::str::from_utf8(&output)
            .map_err(|_| ReviewProviderTokenCountError)?
            .trim()
            .parse::<u64>()
            .map_err(|_| ReviewProviderTokenCountError)?;
        if count == 0 {
            return Err(ReviewProviderTokenCountError);
        }
        Ok(count)
    }

    fn review_response_only(
        &self,
        _request: &FeatureConveyorReviewGatewayRequest,
        canonical_packet: &[u8],
        cancelled: &AtomicBool,
    ) -> Result<Vec<u8>, ReviewProviderTransportError> {
        self.invoke_process(
            &[],
            canonical_packet,
            MAX_FEATURE_CONVEYOR_REVIEW_OUTPUT_BYTES,
            cancelled,
        )
    }
}

pub trait ReviewProvider: Send + Sync {
    /// `None` means production configuration is unavailable. Callers must
    /// reject before opening a durable provider-call intent.
    fn capabilities(&self) -> Option<ReviewProviderCapabilities>;

    fn count_input_tokens(
        &self,
        canonical_packet: &[u8],
    ) -> Result<u64, ReviewProviderTokenCountError>;

    /// One fresh response-only request. No conversation/session handle is
    /// accepted or returned, so an adapter cannot silently reuse context.
    fn review_response_only(
        &self,
        request: &FeatureConveyorReviewGatewayRequest,
        canonical_packet: &[u8],
        cancelled: &AtomicBool,
    ) -> Result<Vec<u8>, ReviewProviderTransportError>;
}

#[derive(Debug, Default)]
pub struct UnavailableReviewProvider;

impl ReviewProvider for UnavailableReviewProvider {
    fn capabilities(&self) -> Option<ReviewProviderCapabilities> {
        None
    }

    fn count_input_tokens(
        &self,
        _canonical_packet: &[u8],
    ) -> Result<u64, ReviewProviderTokenCountError> {
        Err(ReviewProviderTokenCountError)
    }

    fn review_response_only(
        &self,
        _request: &FeatureConveyorReviewGatewayRequest,
        _canonical_packet: &[u8],
        _cancelled: &AtomicBool,
    ) -> Result<Vec<u8>, ReviewProviderTransportError> {
        Err(ReviewProviderTransportError::Outage)
    }
}

#[derive(Debug, Clone)]
pub struct PreparedReviewProviderCall {
    canonical_packet: Vec<u8>,
}

impl PreparedReviewProviderCall {
    pub fn canonical_packet(&self) -> &[u8] {
        &self.canonical_packet
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewProviderInvocationError {
    Unavailable,
    Outage,
    IncompleteTransport,
    MalformedOutput,
    Cancelled,
}

/// Mechanical admission happens before durable mutation. Provider/model drift,
/// missing structured-output support, an undersized context, and tokenization
/// failure all produce the same default-unavailable boundary.
pub fn prepare_review_provider_call(
    provider: &dyn ReviewProvider,
    request: &FeatureConveyorReviewGatewayRequest,
    packet: &FeatureConveyorReviewPacket,
) -> Result<PreparedReviewProviderCall, ReviewProviderInvocationError> {
    packet
        .validate()
        .map_err(|_| ReviewProviderInvocationError::Unavailable)?;
    if packet.sha256().ok() != Some(request.review_packet_sha256) {
        return Err(ReviewProviderInvocationError::Unavailable);
    }
    let canonical_packet = packet
        .canonical_bytes()
        .map_err(|_| ReviewProviderInvocationError::Unavailable)?;
    let capabilities = provider
        .capabilities()
        .ok_or(ReviewProviderInvocationError::Unavailable)?;
    if capabilities.provider_id != request.provider_id
        || capabilities.model_id != request.model_id
        || capabilities.max_input_bytes < MAX_FEATURE_CONVEYOR_REVIEW_PACKET_BYTES
        || capabilities.max_input_tokens < MAX_FEATURE_CONVEYOR_REVIEW_INPUT_TOKENS
        || capabilities.max_output_bytes < MAX_FEATURE_CONVEYOR_REVIEW_OUTPUT_BYTES
        || !capabilities.strict_structured_output
        || !capabilities.response_only
        || !capabilities.fresh_session_per_call
        || canonical_packet.len() > MAX_FEATURE_CONVEYOR_REVIEW_PACKET_BYTES
        || provider
            .count_input_tokens(&canonical_packet)
            .map_err(|_| ReviewProviderInvocationError::Unavailable)?
            > MAX_FEATURE_CONVEYOR_REVIEW_INPUT_TOKENS
    {
        return Err(ReviewProviderInvocationError::Unavailable);
    }
    Ok(PreparedReviewProviderCall { canonical_packet })
}

pub fn invoke_review_provider(
    provider: &dyn ReviewProvider,
    request: &FeatureConveyorReviewGatewayRequest,
    prepared: &PreparedReviewProviderCall,
    cancelled: &AtomicBool,
) -> Result<FeatureConveyorReviewProviderOutput, ReviewProviderInvocationError> {
    if cancelled.load(Ordering::Acquire) {
        return Err(ReviewProviderInvocationError::Cancelled);
    }
    let output = provider
        .review_response_only(request, prepared.canonical_packet(), cancelled)
        .map_err(|error| match error {
            ReviewProviderTransportError::Outage => ReviewProviderInvocationError::Outage,
            ReviewProviderTransportError::IncompleteTransport => {
                ReviewProviderInvocationError::IncompleteTransport
            }
            ReviewProviderTransportError::Cancelled => ReviewProviderInvocationError::Cancelled,
        })?;
    if cancelled.load(Ordering::Acquire) {
        return Err(ReviewProviderInvocationError::Cancelled);
    }
    FeatureConveyorReviewProviderOutput::decode_frame(&output)
        .map_err(|_| ReviewProviderInvocationError::MalformedOutput)
}
