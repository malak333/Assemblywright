use assemblywright_protocol::{
    FeatureConveyorGrantRevisions, FeatureConveyorReviewDecision,
    FeatureConveyorReviewGatewayRequest, FeatureConveyorReviewPacket,
    FeatureConveyorReviewProviderOutput, FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION,
    MAX_FEATURE_CONVEYOR_REVIEW_OUTPUT_BYTES, MAX_FEATURE_CONVEYOR_REVIEW_PACKET_BYTES,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const REVIEW_PROVIDER_CONFIG_FILE: &str = "provider.json";
const REVIEW_PROVIDER_TIMEOUT: Duration = Duration::from_secs(120);
const REVIEW_PROVIDER_CODEX_EXECUTABLE_ENV: &str = "ASSEMBLYWRIGHT_REVIEW_CODEX_EXECUTABLE";
const REVIEW_PROVIDER_CODEX_HOME_ENV: &str = "ASSEMBLYWRIGHT_REVIEW_CODEX_HOME";
const REVIEW_PROVIDER_MODEL_ID_ENV: &str = "ASSEMBLYWRIGHT_REVIEW_MODEL_ID";
const REVIEW_PROVIDER_OUTPUT_SCHEMA_ENV: &str = "ASSEMBLYWRIGHT_REVIEW_OUTPUT_SCHEMA";
const REVIEW_PROVIDER_OUTPUT_SCHEMA_FILE: &str = "review-output-schema.json";
const REVIEW_PROVIDER_CODEX_ADAPTER_KIND: &str = "codex_exec_v1";
const REVIEW_PROVIDER_CODEX_PROVIDER_ID: &str = "openai.codex";
const REVIEW_PROVIDER_CODEX_MODEL_ID: &str = "gpt-5.6-sol";
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
    _assets: Vec<File>,
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
        .env_clear();
    for name in [
        REVIEW_PROVIDER_CODEX_EXECUTABLE_ENV,
        REVIEW_PROVIDER_CODEX_HOME_ENV,
        REVIEW_PROVIDER_MODEL_ID_ENV,
        REVIEW_PROVIDER_OUTPUT_SCHEMA_ENV,
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    command
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
    #[serde(default)]
    review_provider_executable_sha256: Option<String>,
    #[serde(default)]
    codex_adapter: Option<CodexExecAdapterConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CodexExecAdapterConfig {
    kind: String,
    codex_home: PathBuf,
    codex_executable_sha256: String,
    output_schema_sha256: String,
}

#[derive(Debug)]
pub struct ProcessReviewProvider {
    executable: PathBuf,
    #[cfg(windows)]
    launcher_executable: PathBuf,
    working_directory: PathBuf,
    executable_identity: ReviewExecutableIdentity,
    codex_adapter: Option<CodexExecAdapter>,
    capabilities: ReviewProviderCapabilities,
}

#[derive(Debug)]
struct CodexExecAdapter {
    codex_home: PathBuf,
    codex_executable: PathBuf,
    codex_executable_identity: ReviewExecutableIdentity,
    output_schema: PathBuf,
    output_schema_identity: ReviewExecutableIdentity,
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

fn is_lowercase_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .as_bytes()
            .iter()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn is_symlink_or_reparse(metadata: &fs::Metadata) -> bool {
    if metadata.file_type().is_symlink() {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        return metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    }
    #[cfg(not(windows))]
    false
}

fn load_asset_identity(
    path: &Path,
    maximum_length: u64,
    require_executable: bool,
    expected_sha256: &str,
) -> Result<ReviewExecutableIdentity, ReviewProviderConfigError> {
    if is_symlink_or_reparse(&fs::symlink_metadata(path)?) {
        return Err(ReviewProviderConfigError::Invalid);
    }
    let canonical = fs::canonicalize(path)?;
    if canonical != path || canonical.parent() != path.parent() {
        return Err(ReviewProviderConfigError::Invalid);
    }
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > maximum_length {
        return Err(ReviewProviderConfigError::Invalid);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.uid() != unsafe { libc::geteuid() }
            || metadata.permissions().mode() & 0o022 != 0
            || metadata.nlink() != 1
            || (require_executable && metadata.permissions().mode() & 0o111 == 0)
        {
            return Err(ReviewProviderConfigError::Invalid);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(ReviewProviderConfigError::Invalid);
        }
        let _ = require_executable;
    }
    let bytes = fs::read(path)?;
    let sha256: [u8; 32] = Sha256::digest(&bytes).into();
    if hex_bytes(&sha256) != expected_sha256 {
        return Err(ReviewProviderConfigError::Invalid);
    }
    Ok(ReviewExecutableIdentity {
        sha256,
        length: metadata.len(),
        modified: metadata.modified()?,
        #[cfg(unix)]
        device: {
            use std::os::unix::fs::MetadataExt;
            metadata.dev()
        },
        #[cfg(unix)]
        inode: {
            use std::os::unix::fs::MetadataExt;
            metadata.ino()
        },
    })
}

fn validate_codex_home(codex_home: &Path) -> Result<(), ReviewProviderConfigError> {
    let home_metadata = fs::metadata(codex_home)?;
    let auth_path = codex_home.join("auth.json");
    if is_symlink_or_reparse(&fs::symlink_metadata(codex_home)?)
        || is_symlink_or_reparse(&fs::symlink_metadata(&auth_path)?)
        || !home_metadata.is_dir()
    {
        return Err(ReviewProviderConfigError::Invalid);
    }
    let auth_metadata = fs::metadata(&auth_path)?;
    if !auth_metadata.is_file() || auth_metadata.len() == 0 {
        return Err(ReviewProviderConfigError::Invalid);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let effective_uid = unsafe { libc::geteuid() };
        if home_metadata.uid() != effective_uid
            || auth_metadata.uid() != effective_uid
            || home_metadata.permissions().mode() & 0o077 != 0
            || auth_metadata.permissions().mode() & 0o077 != 0
            || auth_metadata.nlink() != 1
        {
            return Err(ReviewProviderConfigError::Invalid);
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        if home_metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || auth_metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        {
            return Err(ReviewProviderConfigError::Invalid);
        }
        validate_private_windows_acl(codex_home)?;
        validate_private_windows_acl(&auth_path)?;
    }
    Ok(())
}

#[cfg(windows)]
fn validate_private_windows_acl(path: &Path) -> Result<(), ReviewProviderConfigError> {
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::{addr_of, null_mut};
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree};
    use windows_sys::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
    use windows_sys::Win32::Security::{
        AclSizeInformation, CreateWellKnownSid, EqualSid, GetAce, GetAclInformation,
        GetSecurityDescriptorControl, GetTokenInformation, TokenUser, WinLocalSystemSid,
        ACCESS_ALLOWED_ACE, ACL_SIZE_INFORMATION, DACL_SECURITY_INFORMATION, INHERITED_ACE,
        OWNER_SECURITY_INFORMATION, PSID, SECURITY_MAX_SID_SIZE, SE_DACL_PROTECTED, TOKEN_QUERY,
        TOKEN_USER,
    };
    use windows_sys::Win32::Storage::FileSystem::FILE_ALL_ACCESS;
    use windows_sys::Win32::System::SystemServices::ACCESS_ALLOWED_ACE_TYPE;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
    wide.push(0);
    let mut owner: PSID = null_mut();
    let mut dacl = null_mut();
    let mut descriptor = null_mut();
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut dacl,
            null_mut(),
            &mut descriptor,
        )
    };
    if status != 0 || owner.is_null() || dacl.is_null() || descriptor.is_null() {
        if !descriptor.is_null() {
            unsafe { LocalFree(descriptor) };
        }
        return Err(ReviewProviderConfigError::Invalid);
    }
    let result = (|| {
        let mut token = std::ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err(ReviewProviderConfigError::Invalid);
        }
        let token_result = (|| {
            let mut token_bytes = 0_u32;
            unsafe { GetTokenInformation(token, TokenUser, null_mut(), 0, &mut token_bytes) };
            if token_bytes < size_of::<TOKEN_USER>() as u32 {
                return Err(ReviewProviderConfigError::Invalid);
            }
            let words = (token_bytes as usize).div_ceil(size_of::<usize>());
            let mut token_buffer = vec![0_usize; words];
            if unsafe {
                GetTokenInformation(
                    token,
                    TokenUser,
                    token_buffer.as_mut_ptr().cast(),
                    token_bytes,
                    &mut token_bytes,
                )
            } == 0
            {
                return Err(ReviewProviderConfigError::Invalid);
            }
            let user = unsafe { &*(token_buffer.as_ptr().cast::<TOKEN_USER>()) };
            if unsafe { EqualSid(owner, user.User.Sid) } == 0 {
                return Err(ReviewProviderConfigError::Invalid);
            }
            let mut control = 0_u16;
            let mut revision = 0_u32;
            if unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } == 0
                || control & SE_DACL_PROTECTED == 0
            {
                return Err(ReviewProviderConfigError::Invalid);
            }
            let mut acl_info: ACL_SIZE_INFORMATION = unsafe { zeroed() };
            if unsafe {
                GetAclInformation(
                    dacl,
                    (&mut acl_info as *mut ACL_SIZE_INFORMATION).cast(),
                    size_of::<ACL_SIZE_INFORMATION>() as u32,
                    AclSizeInformation,
                )
            } == 0
                || acl_info.AceCount != 2
            {
                return Err(ReviewProviderConfigError::Invalid);
            }
            let mut system_sid = [0_u8; SECURITY_MAX_SID_SIZE as usize];
            let mut system_sid_bytes = SECURITY_MAX_SID_SIZE;
            if unsafe {
                CreateWellKnownSid(
                    WinLocalSystemSid,
                    null_mut(),
                    system_sid.as_mut_ptr().cast(),
                    &mut system_sid_bytes,
                )
            } == 0
            {
                return Err(ReviewProviderConfigError::Invalid);
            }
            let mut saw_user = false;
            let mut saw_system = false;
            for index in 0..acl_info.AceCount {
                let mut raw_ace: *mut c_void = null_mut();
                if unsafe { GetAce(dacl, index, &mut raw_ace) } == 0 || raw_ace.is_null() {
                    return Err(ReviewProviderConfigError::Invalid);
                }
                let ace = unsafe { &*(raw_ace.cast::<ACCESS_ALLOWED_ACE>()) };
                if u32::from(ace.Header.AceType) != ACCESS_ALLOWED_ACE_TYPE
                    || u32::from(ace.Header.AceFlags) & INHERITED_ACE != 0
                    || ace.Mask & FILE_ALL_ACCESS != FILE_ALL_ACCESS
                {
                    return Err(ReviewProviderConfigError::Invalid);
                }
                let sid = addr_of!(ace.SidStart) as PSID;
                if unsafe { EqualSid(sid, user.User.Sid) } != 0 && !saw_user {
                    saw_user = true;
                } else if unsafe { EqualSid(sid, system_sid.as_mut_ptr().cast()) } != 0
                    && !saw_system
                {
                    saw_system = true;
                } else {
                    return Err(ReviewProviderConfigError::Invalid);
                }
            }
            if !saw_user || !saw_system {
                return Err(ReviewProviderConfigError::Invalid);
            }
            Ok(())
        })();
        unsafe { CloseHandle(token) };
        token_result
    })();
    unsafe { LocalFree(descriptor) };
    result
}

fn hex_bytes(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut encoded, "{byte:02x}").expect("writing to a String cannot fail");
    }
    encoded
}

fn open_verified_asset(
    path: &Path,
    identity: &ReviewExecutableIdentity,
) -> Result<File, ReviewProviderTransportError> {
    let path_metadata =
        fs::symlink_metadata(path).map_err(|_| ReviewProviderTransportError::Outage)?;
    if is_symlink_or_reparse(&path_metadata) {
        return Err(ReviewProviderTransportError::Outage);
    }
    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
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
            .open(path)
            .map_err(|_| ReviewProviderTransportError::Outage)?
    };
    #[cfg(not(any(unix, windows)))]
    let mut file = File::open(path).map_err(|_| ReviewProviderTransportError::Outage)?;
    let metadata = file
        .metadata()
        .map_err(|_| ReviewProviderTransportError::Outage)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() != identity.length
        || metadata.modified().ok() != Some(identity.modified)
    {
        return Err(ReviewProviderTransportError::Outage);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.dev() != identity.device
            || metadata.ino() != identity.inode
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
        usize::try_from(identity.length).map_err(|_| ReviewProviderTransportError::Outage)?,
    );
    file.read_to_end(&mut bytes)
        .map_err(|_| ReviewProviderTransportError::Outage)?;
    if <[u8; 32]>::from(Sha256::digest(&bytes)) != identity.sha256 {
        return Err(ReviewProviderTransportError::Outage);
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| ReviewProviderTransportError::Outage)?;
    Ok(file)
}

impl ProcessReviewProvider {
    pub fn is_pinned_codex_adapter(&self) -> bool {
        self.codex_adapter.is_some()
            && self.capabilities.provider_id == REVIEW_PROVIDER_CODEX_PROVIDER_ID
            && self.capabilities.model_id == REVIEW_PROVIDER_CODEX_MODEL_ID
    }

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
        if is_symlink_or_reparse(&fs::symlink_metadata(&root)?)
            || is_symlink_or_reparse(&fs::symlink_metadata(&config_path)?)
            || is_symlink_or_reparse(&fs::symlink_metadata(&executable_path)?)
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
        if !matches!(config.schema_version, 1 | 2)
            || config.provider_id.is_empty()
            || config.provider_id.len() > 128
            || config.model_id.is_empty()
            || config.model_id.len() > 128
            || config.max_input_tokens < MAX_FEATURE_CONVEYOR_REVIEW_INPUT_TOKENS
        {
            return Err(ReviewProviderConfigError::Invalid);
        }
        let executable_identity = match (
            config.schema_version,
            config.review_provider_executable_sha256.as_deref(),
        ) {
            (1, None) => {
                let executable_bytes = fs::read(&executable)?;
                if executable_bytes.is_empty() || executable_bytes.len() > 32 * 1024 * 1024 {
                    return Err(ReviewProviderConfigError::Invalid);
                }
                let metadata = fs::metadata(&executable)?;
                ReviewExecutableIdentity {
                    sha256: Sha256::digest(&executable_bytes).into(),
                    length: metadata.len(),
                    modified: metadata.modified()?,
                    #[cfg(unix)]
                    device: {
                        use std::os::unix::fs::MetadataExt;
                        metadata.dev()
                    },
                    #[cfg(unix)]
                    inode: {
                        use std::os::unix::fs::MetadataExt;
                        metadata.ino()
                    },
                }
            }
            (2, Some(expected)) if is_lowercase_sha256(expected) => {
                load_asset_identity(&executable, 32 * 1024 * 1024, true, expected)?
            }
            _ => return Err(ReviewProviderConfigError::Invalid),
        };
        let codex_adapter = match (config.schema_version, config.codex_adapter) {
            (1, None) => None,
            (2, Some(adapter))
                if config.provider_id == REVIEW_PROVIDER_CODEX_PROVIDER_ID
                    && config.model_id == REVIEW_PROVIDER_CODEX_MODEL_ID
                    && adapter.kind == REVIEW_PROVIDER_CODEX_ADAPTER_KIND
                    && is_lowercase_sha256(&adapter.codex_executable_sha256)
                    && is_lowercase_sha256(&adapter.output_schema_sha256) =>
            {
                if !adapter.codex_home.is_absolute() {
                    return Err(ReviewProviderConfigError::Invalid);
                }
                #[cfg(windows)]
                if adapter.codex_home != Path::new(r"C:\Users\mike\.codex") {
                    return Err(ReviewProviderConfigError::Invalid);
                }
                let codex_home = fs::canonicalize(&adapter.codex_home)?;
                #[cfg(not(windows))]
                if codex_home != adapter.codex_home {
                    return Err(ReviewProviderConfigError::Invalid);
                }
                validate_codex_home(&codex_home)?;
                let codex_executable = root.join(if cfg!(windows) { "codex.exe" } else { "codex" });
                let output_schema = root.join(REVIEW_PROVIDER_OUTPUT_SCHEMA_FILE);
                let codex_executable_identity = load_asset_identity(
                    &codex_executable,
                    384 * 1024 * 1024,
                    true,
                    &adapter.codex_executable_sha256,
                )?;
                let output_schema_identity = load_asset_identity(
                    &output_schema,
                    256 * 1024,
                    false,
                    &adapter.output_schema_sha256,
                )?;
                Some(CodexExecAdapter {
                    codex_home,
                    codex_executable,
                    codex_executable_identity,
                    output_schema,
                    output_schema_identity,
                })
            }
            _ => return Err(ReviewProviderConfigError::Invalid),
        };
        Ok(Some(Self {
            executable,
            #[cfg(windows)]
            launcher_executable,
            working_directory: root,
            executable_identity,
            codex_adapter,
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
        let file = open_verified_asset(&self.executable, &self.executable_identity)?;
        let mut assets = Vec::new();
        if let Some(adapter) = &self.codex_adapter {
            assets.push(open_verified_asset(
                &adapter.codex_executable,
                &adapter.codex_executable_identity,
            )?);
            assets.push(open_verified_asset(
                &adapter.output_schema,
                &adapter.output_schema_identity,
            )?);
        }
        Ok(VerifiedReviewExecutable {
            _file: file,
            _assets: assets,
        })
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
        command.current_dir(&self.working_directory).env_clear();
        if let Some(adapter) = &self.codex_adapter {
            command
                .env(REVIEW_PROVIDER_CODEX_HOME_ENV, &adapter.codex_home)
                .env(
                    REVIEW_PROVIDER_CODEX_EXECUTABLE_ENV,
                    &adapter.codex_executable,
                )
                .env(REVIEW_PROVIDER_OUTPUT_SCHEMA_ENV, &adapter.output_schema)
                .env(REVIEW_PROVIDER_MODEL_ID_ENV, &self.capabilities.model_id);
        }
        command.stdin(Stdio::piped()).stdout(Stdio::piped());
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReviewProviderLiveProofReceipt {
    pub schema_version: u16,
    pub status: &'static str,
    pub provider_id: String,
    pub model_id: String,
    pub approval_packet_sha256: String,
    pub approval_output_sha256: String,
    pub rejection_packet_sha256: String,
    pub rejection_output_sha256: String,
    pub observed_at_ms: u64,
}

pub fn execute_review_provider_live_proof(
    provider: &dyn ReviewProvider,
    observed_at_ms: u64,
) -> Result<ReviewProviderLiveProofReceipt, ReviewProviderInvocationError> {
    let capabilities = provider
        .capabilities()
        .ok_or(ReviewProviderInvocationError::Unavailable)?;
    let approval = live_proof_packet(
        &capabilities,
        true,
        UuidSeed::Approval,
        "review-provider-live=approved",
    )?;
    let rejection = live_proof_packet(
        &capabilities,
        false,
        UuidSeed::Rejection,
        "review-provider-live=incorrect",
    )?;
    let approval_output = execute_live_proof_case(provider, &approval, true)?;
    let rejection_output = execute_live_proof_case(provider, &rejection, false)?;
    Ok(ReviewProviderLiveProofReceipt {
        schema_version: 1,
        status: "review_provider_live_proof_passed",
        provider_id: capabilities.provider_id,
        model_id: capabilities.model_id,
        approval_packet_sha256: hex_bytes(
            &approval
                .0
                .sha256()
                .map_err(|_| ReviewProviderInvocationError::MalformedOutput)?,
        ),
        approval_output_sha256: hex_bytes(&Sha256::digest(
            serde_json::to_vec(&approval_output)
                .map_err(|_| ReviewProviderInvocationError::MalformedOutput)?,
        )),
        rejection_packet_sha256: hex_bytes(
            &rejection
                .0
                .sha256()
                .map_err(|_| ReviewProviderInvocationError::MalformedOutput)?,
        ),
        rejection_output_sha256: hex_bytes(&Sha256::digest(
            serde_json::to_vec(&rejection_output)
                .map_err(|_| ReviewProviderInvocationError::MalformedOutput)?,
        )),
        observed_at_ms,
    })
}

enum UuidSeed {
    Approval,
    Rejection,
}

fn live_proof_packet(
    capabilities: &ReviewProviderCapabilities,
    should_approve: bool,
    seed: UuidSeed,
    candidate_value: &str,
) -> Result<
    (
        FeatureConveyorReviewPacket,
        FeatureConveyorReviewGatewayRequest,
    ),
    ReviewProviderInvocationError,
> {
    let requirement_id = if should_approve {
        "review-provider-live-approval"
    } else {
        "review-provider-live-rejection"
    };
    let specification = json!({
        "acceptance": [requirement_id],
        "allowed_paths": ["proof.txt"],
        "non_goals": ["repository mutation", "publication"],
        "outcome": "proof.txt must contain exactly review-provider-live=approved followed by one newline"
    });
    let specification_bytes = serde_json::to_vec(&specification)
        .map_err(|_| ReviewProviderInvocationError::MalformedOutput)?;
    let candidate_diff = format!(
        "diff --git a/proof.txt b/proof.txt\nnew file mode 100644\n--- /dev/null\n+++ b/proof.txt\n@@ -0,0 +1 @@\n+{candidate_value}\n"
    );
    let evidence_digests = vec![
        Sha256::digest(
            format!("assemblywright.review-provider-live.{requirement_id}.validation").as_bytes(),
        )
        .into(),
        Sha256::digest(
            format!("assemblywright.review-provider-live.{requirement_id}.knowledge").as_bytes(),
        )
        .into(),
    ];
    let seed = match seed {
        UuidSeed::Approval => 0x100_u128,
        UuidSeed::Rejection => 0x200_u128,
    };
    let packet = FeatureConveyorReviewPacket {
        schema_version: FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION,
        feature_id: uuid::Uuid::from_u128(seed + 1),
        specification_revision: 1,
        approved_specification: specification,
        approved_specification_sha256: Sha256::digest(&specification_bytes).into(),
        candidate_commit: if should_approve { "1" } else { "4" }.repeat(40),
        candidate_tree: if should_approve { "2" } else { "5" }.repeat(40),
        base_commit: if should_approve { "3" } else { "6" }.repeat(40),
        candidate_diff_sha256: Sha256::digest(candidate_diff.as_bytes()).into(),
        candidate_diff,
        evidence_manifest_sha256: evidence_digests[0],
        evidence_digests,
        requirements_sha256: Sha256::digest(requirement_id.as_bytes()).into(),
        requirement_ids: vec![requirement_id.to_string()],
        provider_id: capabilities.provider_id.clone(),
        model_id: capabilities.model_id.clone(),
        grants: FeatureConveyorGrantRevisions {
            registration: 1,
            cloud_disclosure: 1,
            autonomous_publication: 1,
        },
    };
    let request = FeatureConveyorReviewGatewayRequest {
        schema_version: FEATURE_CONVEYOR_REVIEW_GATEWAY_SCHEMA_VERSION,
        review_call_id: uuid::Uuid::from_u128(seed + 2),
        feature_id: packet.feature_id,
        specification_revision: 1,
        expected_lifecycle_revision: 1,
        feature_lease_id: uuid::Uuid::from_u128(seed + 3),
        integration_id: uuid::Uuid::from_u128(seed + 4),
        validation_id: uuid::Uuid::from_u128(seed + 5),
        candidate_commit: packet.candidate_commit.clone(),
        candidate_tree: packet.candidate_tree.clone(),
        base_commit: packet.base_commit.clone(),
        candidate_diff_sha256: packet.candidate_diff_sha256,
        evidence_manifest_sha256: packet.evidence_manifest_sha256,
        review_packet_sha256: packet
            .sha256()
            .map_err(|_| ReviewProviderInvocationError::MalformedOutput)?,
        provider_id: packet.provider_id.clone(),
        model_id: packet.model_id.clone(),
        expected_queue_revision: 1,
        expected_emergency_pause_revision: 0,
        grants: packet.grants,
    };
    Ok((packet, request))
}

fn execute_live_proof_case(
    provider: &dyn ReviewProvider,
    case: &(
        FeatureConveyorReviewPacket,
        FeatureConveyorReviewGatewayRequest,
    ),
    should_approve: bool,
) -> Result<FeatureConveyorReviewProviderOutput, ReviewProviderInvocationError> {
    let prepared = prepare_review_provider_call(provider, &case.1, &case.0)?;
    let output = invoke_review_provider(provider, &case.1, &prepared, &AtomicBool::new(false))?;
    let expected = if should_approve {
        FeatureConveyorReviewDecision::Approved
    } else {
        FeatureConveyorReviewDecision::Rejected
    };
    if output.decision != expected {
        return Err(ReviewProviderInvocationError::MalformedOutput);
    }
    Ok(output)
}
