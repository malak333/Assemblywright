use super::{
    bounded_reader, verify_executable, CommandError, CommandInvocation, CommandStderrMode,
    Executable, PlanningEffectControl,
};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::ffi::{c_void, OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::mem::{size_of, size_of_val, zeroed};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
use std::os::windows::io::{AsRawHandle, FromRawHandle, RawHandle};
use std::path::{Path, PathBuf};
use std::ptr::{addr_of, null, null_mut};
use std::sync::{Arc, Mutex};
use uuid::Uuid;
use windows_service::service::{
    ServiceAccess, ServiceConfig, ServiceErrorControl, ServiceStartType, ServiceState, ServiceType,
};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, LocalFree, ERROR_INSUFFICIENT_BUFFER, HANDLE, HANDLE_FLAG_INHERIT,
    INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
    ConvertStringSidToSidW, GetEffectiveRightsFromAclW, GetNamedSecurityInfoW, GetSecurityInfo,
    SetSecurityInfo, NO_MULTIPLE_TRUSTEE, SDDL_REVISION_1, SE_FILE_OBJECT, TRUSTEE_IS_SID,
    TRUSTEE_IS_UNKNOWN, TRUSTEE_W,
};
#[cfg(test)]
use windows_sys::Win32::Security::Isolation::DeleteAppContainerProfile;
use windows_sys::Win32::Security::Isolation::{
    CreateAppContainerProfile, DeriveAppContainerSidFromAppContainerName,
};
use windows_sys::Win32::Security::{
    AclSizeInformation, CreateRestrictedToken, CreateWellKnownSid, EqualSid, FreeSid, GetAce,
    GetAclInformation, GetSecurityDescriptorControl, GetSecurityDescriptorSacl,
    GetTokenInformation, IsValidSid, LookupAccountNameW, SetTokenInformation, TokenOwner,
    TokenUser, WinLocalSystemSid, ACCESS_ALLOWED_ACE, ACL_SIZE_INFORMATION, CONTAINER_INHERIT_ACE,
    DACL_SECURITY_INFORMATION, DISABLE_MAX_PRIVILEGE, INHERITED_ACE, LABEL_SECURITY_INFORMATION,
    OBJECT_INHERIT_ACE, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR, PSID,
    SECURITY_ATTRIBUTES, SECURITY_CAPABILITIES, SECURITY_MAX_SID_SIZE, SE_DACL_PROTECTED,
    SE_SACL_PROTECTED, SID_AND_ATTRIBUTES, SID_NAME_USE, SYSTEM_MANDATORY_LABEL_ACE,
    TOKEN_ADJUST_DEFAULT, TOKEN_ASSIGN_PRIMARY, TOKEN_DUPLICATE, TOKEN_OWNER, TOKEN_QUERY,
    TOKEN_USER,
};
use windows_sys::Win32::Storage::FileSystem::{
    GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION, DELETE, FILE_ALL_ACCESS,
    FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
    FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TRAVERSE, READ_CONTROL, SYNCHRONIZE,
    WRITE_OWNER,
};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
    SetInformationJobObject, TerminateJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JOB_OBJECT_LIMIT_ACTIVE_PROCESS, JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION,
    JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Pipes::CreatePipe;
use windows_sys::Win32::System::StationsAndDesktops::{
    CloseDesktop, CloseWindowStation, CreateDesktopW, CreateWindowStationW,
    GetProcessWindowStation, SetProcessWindowStation, HDESK, HWINSTA,
};
use windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW;
use windows_sys::Win32::System::SystemServices::{
    ACCESS_ALLOWED_ACE_TYPE, SE_GROUP_ENABLED, SYSTEM_MANDATORY_LABEL_ACE_TYPE,
    SYSTEM_MANDATORY_LABEL_NO_WRITE_UP,
};
use windows_sys::Win32::System::Threading::{
    CreateProcessAsUserW, DeleteProcThreadAttributeList, GetCurrentProcess, GetExitCodeProcess,
    InitializeProcThreadAttributeList, OpenProcessToken, ResumeThread, TerminateProcess,
    UpdateProcThreadAttribute, WaitForSingleObject, CREATE_NO_WINDOW, CREATE_SUSPENDED,
    CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT, PROCESS_INFORMATION,
    PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES,
    STARTF_USESTDHANDLES, STARTUPINFOEXW,
};
use windows_sys::Win32::System::WindowsProgramming::GetComputerNameW;
use windows_sys::Win32::UI::Shell::{
    CommandLineToArgvW, FOLDERID_ProgramData, SHGetKnownFolderPath, KF_FLAG_DEFAULT,
};
use zeroize::Zeroize;

const PROVIDER_PROFILE: &str = "Assemblywright.Planning.Provider.v1";
const GITHUB_PROFILE: &str = "Assemblywright.Planning.Github.v1";
const INTERNET_CLIENT_SID: &str = "S-1-15-3-1";
const TERMINATED_EXIT_CODE: u32 = 0xA55E_1001;
const MAX_PROFILE_NAME_BYTES: usize = 64;
const HRESULT_PROFILE_ALREADY_EXISTS: i32 = 0x8007_00B7u32 as i32;
const MAX_USER_ENVIRONMENT_UNITS: usize = 32_768;
const MAX_USER_ENVIRONMENT_ENTRIES: usize = 512;
const PRIVATE_WINDOW_STATION_CREATE_ONLY: u32 = 1;
const PRIVATE_DESKTOP_NAME: &str = "Planning";
const RUNTIME_VENDOR_DIRECTORY: &str = "Assemblywright";
const RUNTIME_NAMESPACE_DIRECTORY: &str = "planning-runtime";
const WINDOW_STATION_OWNER_ACCESS: u32 = 0x000f_037f;
const WINDOW_STATION_PROFILE_ACCESS: u32 = 0x000f_006e;
const DESKTOP_OWNER_ACCESS: u32 = 0x000f_01ff;
const DESKTOP_PROFILE_ACCESS: u32 = 0x000f_00cf;
static WINDOW_STATION_CREATION: Mutex<()> = Mutex::new(());

pub(super) struct Provisioning<'a> {
    pub data_root: &'a Path,
    pub locator_root: &'a Path,
    pub planning_root: &'a Path,
    pub config_path: &'a Path,
    pub provider_root: &'a Path,
    pub provider_paths: &'a [&'a Path],
    pub github_root: &'a Path,
    pub github_paths: &'a [&'a Path],
    pub provider_profile_name: &'a str,
    pub provider_profile_sid: &'a str,
    pub github_profile_name: &'a str,
    pub github_profile_sid: &'a str,
    pub provisioning_owner_sid: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum ProvisioningRejection {
    ProfileIdentity,
    OwnerIdentity,
    DataAcl,
    RuntimeLocation,
    MasterTreeAcl,
    PlanningAcl,
    PlanningAllowlist,
    MasterCheck,
    ConfigAcl,
    ProviderScope,
    GithubScope,
    Revalidation,
}

impl ProvisioningRejection {
    pub(super) const fn code(self) -> &'static str {
        match self {
            Self::ProfileIdentity => "profile_identity",
            Self::OwnerIdentity => "provisioning_owner_identity",
            Self::DataAcl => "data_acl",
            Self::RuntimeLocation => "runtime_location",
            Self::MasterTreeAcl => "master_tree_acl",
            Self::PlanningAcl => "planning_acl",
            Self::PlanningAllowlist => "planning_allowlist",
            Self::MasterCheck => "master_check",
            Self::ConfigAcl => "config_acl",
            Self::ProviderScope => "provider_scope",
            Self::GithubScope => "github_scope",
            Self::Revalidation => "revalidation",
        }
    }
}

#[derive(Clone)]
pub(super) struct ProfileBinding {
    name: String,
    sid: String,
    provisioning_owner_sid: String,
    scope_root: PathBuf,
    peer_root: PathBuf,
    planning_root: PathBuf,
    master_check_root: PathBuf,
    data_root: PathBuf,
    locator_root: PathBuf,
    runtime_ancestors: Vec<DirectoryBinding>,
    protected_paths: Vec<PathBuf>,
    scope_paths: Vec<(PathBuf, bool)>,
}

impl ProfileBinding {
    pub(super) fn name(&self) -> &str {
        &self.name
    }

    pub(super) fn sid(&self) -> &str {
        &self.sid
    }

    pub(super) fn provisioning_owner_sid(&self) -> &str {
        &self.provisioning_owner_sid
    }
}

pub(super) struct NativeProbeAuthority {
    service_name: String,
    config: ServiceConfig,
    current_token_sid: String,
    bindings: NativeProbeBindings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NativeProbeOpenRejection {
    ServiceConfiguration,
    ExecutableCommandBinding,
    AccountIdentity,
    ProfileRevalidation,
    StoppedState,
    BindingDigest,
}

impl NativeProbeOpenRejection {
    pub(super) const fn code(self) -> &'static str {
        match self {
            Self::ServiceConfiguration => "native_probe_service_configuration",
            Self::ExecutableCommandBinding => "native_probe_executable_command_binding",
            Self::AccountIdentity => "native_probe_account_identity",
            Self::ProfileRevalidation => "native_probe_profile_revalidation",
            Self::StoppedState => "native_probe_stopped_state",
            Self::BindingDigest => "native_probe_binding_digest",
        }
    }
}

#[derive(Clone)]
pub(super) struct NativeProbeBindings {
    pub service_name_sha256: [u8; 32],
    pub service_runtime_binding_sha256: [u8; 32],
    pub service_account_sid: String,
    pub current_token_sid: String,
    pub provisioning_owner_sid: String,
    pub health_endpoint: std::net::SocketAddr,
}

impl NativeProbeAuthority {
    pub(super) fn open(
        service_name: &str,
        data_dir: &Path,
        profile: &ProfileBinding,
    ) -> Result<Self, NativeProbeOpenRejection> {
        if service_name.is_empty()
            || service_name.len() > 64
            || !service_name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(NativeProbeOpenRejection::ServiceConfiguration);
        }
        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
            .map_err(|_| NativeProbeOpenRejection::ServiceConfiguration)?;
        let service = manager
            .open_service(
                service_name,
                ServiceAccess::QUERY_STATUS | ServiceAccess::QUERY_CONFIG,
            )
            .map_err(|_| NativeProbeOpenRejection::ServiceConfiguration)?;
        let config = service
            .query_config()
            .map_err(|_| NativeProbeOpenRejection::ServiceConfiguration)?;
        let current_executable = std::env::current_exe()
            .and_then(fs::canonicalize)
            .map_err(|_| NativeProbeOpenRejection::ExecutableCommandBinding)?;
        let data_dir = fs::canonicalize(data_dir)
            .map_err(|_| NativeProbeOpenRejection::ExecutableCommandBinding)?;
        let command = parse_service_launch_command(config.executable_path.as_os_str())
            .map_err(|_| NativeProbeOpenRejection::ExecutableCommandBinding)?;
        let health_endpoint = validate_service_launch_command(
            service_name,
            &data_dir,
            &current_executable,
            &config,
            &command,
        )
        .map_err(|_| NativeProbeOpenRejection::ExecutableCommandBinding)?;
        let account_name = config
            .account_name
            .as_deref()
            .ok_or(NativeProbeOpenRejection::AccountIdentity)?;
        let service_account_sid = lookup_account_sid_text(account_name)
            .map_err(|_| NativeProbeOpenRejection::AccountIdentity)?;
        let command_account_sid = lookup_account_sid_text(&command[9])
            .map_err(|_| NativeProbeOpenRejection::AccountIdentity)?;
        let current_token_sid =
            current_token_sid_text().map_err(|_| NativeProbeOpenRejection::AccountIdentity)?;
        let provisioning_owner_sid = profile.provisioning_owner_sid().to_string();
        if service_account_sid != command_account_sid
            || service_account_sid != current_token_sid
            || current_token_sid != provisioning_owner_sid
        {
            return Err(NativeProbeOpenRejection::AccountIdentity);
        }
        profile
            .revalidate()
            .map_err(|_| NativeProbeOpenRejection::ProfileRevalidation)?;
        require_service_stopped(&service).map_err(|_| NativeProbeOpenRejection::StoppedState)?;
        let service_name_sha256: [u8; 32] = Sha256::digest(
            [
                b"assemblywright.service-name.v1\0".as_slice(),
                service_name.as_bytes(),
            ]
            .concat(),
        )
        .into();
        let service_runtime_binding_sha256 = service_runtime_binding_sha256(
            service_name,
            &data_dir,
            &current_executable,
            &command,
            &service_account_sid,
            &current_token_sid,
            &provisioning_owner_sid,
            profile.name(),
            profile.sid(),
        )
        .map_err(|_| NativeProbeOpenRejection::BindingDigest)?;
        let bindings = NativeProbeBindings {
            service_name_sha256,
            service_runtime_binding_sha256,
            service_account_sid,
            current_token_sid: current_token_sid.clone(),
            provisioning_owner_sid,
            health_endpoint,
        };
        Ok(Self {
            service_name: service_name.to_string(),
            config,
            current_token_sid,
            bindings,
        })
    }

    pub(super) fn bindings(&self) -> NativeProbeBindings {
        self.bindings.clone()
    }

    pub(super) fn revalidate_service(&self) -> Result<(), u8> {
        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
            .map_err(|_| 1)?;
        let service = manager
            .open_service(
                &self.service_name,
                ServiceAccess::QUERY_STATUS | ServiceAccess::QUERY_CONFIG,
            )
            .map_err(|_| 2)?;
        require_service_stopped(&service).map_err(|_| 3)?;
        let current_config = service.query_config().map_err(|_| 4)?;
        let current_token_sid = current_token_sid_text().map_err(|_| 5)?;
        if native_probe_authority_drifted(
            &self.config,
            &current_config,
            &self.current_token_sid,
            &current_token_sid,
        ) {
            return Err(6);
        }
        Ok(())
    }
}

pub(super) fn codex_windows_environment_path(path: &Path) -> OsString {
    let units = path.as_os_str().encode_wide().collect::<Vec<_>>();
    let drive = units.get(4).copied().unwrap_or_default();
    let local_drive = units.len() >= 7
        && units[..4] == [b'\\' as u16, b'\\' as u16, b'?' as u16, b'\\' as u16]
        && ((b'A' as u16..=b'Z' as u16).contains(&drive)
            || (b'a' as u16..=b'z' as u16).contains(&drive))
        && units[5] == b':' as u16
        && units[6] == b'\\' as u16;
    if local_drive {
        OsString::from_wide(&units[4..])
    } else {
        path.as_os_str().to_owned()
    }
}

fn child_visible_current_directory(path: &Path) -> OsString {
    codex_windows_environment_path(path)
}

fn native_probe_authority_drifted(
    expected_config: &ServiceConfig,
    current_config: &ServiceConfig,
    expected_token_sid: &str,
    current_token_sid: &str,
) -> bool {
    current_config != expected_config || current_token_sid != expected_token_sid
}

fn require_service_stopped(
    service: &windows_service::service::Service,
) -> Result<(), CommandError> {
    let status = service.query_status().map_err(|_| CommandError::Failed)?;
    if status.current_state != ServiceState::Stopped
        || status.process_id.is_some_and(|process_id| process_id != 0)
    {
        return Err(CommandError::Failed);
    }
    Ok(())
}

fn parse_service_launch_command(command: &OsStr) -> Result<Vec<OsString>, CommandError> {
    let command = wide(command);
    let mut count = 0;
    let arguments = unsafe { CommandLineToArgvW(command.as_ptr(), &mut count) };
    if arguments.is_null() {
        return Err(CommandError::Failed);
    }
    if !(1..=32).contains(&count) {
        unsafe { LocalFree(arguments.cast()) };
        return Err(CommandError::Failed);
    }
    let result = (0..count as usize)
        .map(|index| {
            let argument = unsafe { *arguments.add(index) };
            if argument.is_null() {
                return Err(CommandError::Failed);
            }
            let length = (0..32_768)
                .find(|offset| unsafe { *argument.add(*offset) } == 0)
                .ok_or(CommandError::Failed)?;
            Ok(OsString::from_wide(unsafe {
                std::slice::from_raw_parts(argument, length)
            }))
        })
        .collect();
    unsafe { LocalFree(arguments.cast()) };
    result
}

fn validate_service_launch_command(
    service_name: &str,
    data_dir: &Path,
    current_executable: &Path,
    config: &ServiceConfig,
    command: &[OsString],
) -> Result<std::net::SocketAddr, CommandError> {
    if config.service_type != ServiceType::OWN_PROCESS
        || config.start_type != ServiceStartType::AutoStart
        || config.error_control != ServiceErrorControl::Normal
        || config.load_order_group.is_some()
        || config.tag_id != 0
        || !config.dependencies.is_empty()
        || config.display_name != OsStr::new("Assemblywright Developer Mode Master")
        || (command.len() != 10 && command.len() != 12)
    {
        return Err(CommandError::Failed);
    }
    let installed_executable = fs::canonicalize(&command[0]).map_err(|_| CommandError::Failed)?;
    let installed_data_dir = fs::canonicalize(&command[2]).map_err(|_| CommandError::Failed)?;
    if installed_executable != current_executable
        || installed_data_dir != data_dir
        || command[1] != OsStr::new("--data-dir")
        || command[3] != OsStr::new("service-run")
        || command[4] != OsStr::new("--service-name")
        || command[5] != OsStr::new(service_name)
        || command[6] != OsStr::new("--bind")
        || command[8] != OsStr::new("--service-identity")
        || command[9].is_empty()
    {
        return Err(CommandError::Failed);
    }
    let bind = command[7]
        .to_string_lossy()
        .parse::<std::net::SocketAddr>()
        .map_err(|_| CommandError::Failed)?;
    if !bind.ip().is_loopback() || bind.port() == 0 {
        return Err(CommandError::Failed);
    }
    match command.len() {
        10 => {}
        12 if command[10] == OsStr::new("--remote-bind")
            && command[11]
                .to_string_lossy()
                .parse::<std::net::SocketAddr>()
                .is_ok_and(|remote| !remote.ip().is_unspecified() && remote.port() != 0) => {}
        _ => return Err(CommandError::Failed),
    }
    Ok(bind)
}

fn current_token_sid_text() -> Result<String, CommandError> {
    let mut token = null_mut();
    if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
        return Err(CommandError::Failed);
    }
    let token = OwnedHandle::new(token)?;
    let mut bytes = 0;
    unsafe { GetTokenInformation(token.raw(), TokenUser, null_mut(), 0, &mut bytes) };
    if bytes < size_of::<TOKEN_USER>() as u32 {
        return Err(CommandError::Failed);
    }
    let mut buffer = vec![0usize; (bytes as usize).div_ceil(size_of::<usize>())];
    if unsafe {
        GetTokenInformation(
            token.raw(),
            TokenUser,
            buffer.as_mut_ptr().cast(),
            bytes,
            &mut bytes,
        )
    } == 0
    {
        return Err(CommandError::Failed);
    }
    let user = unsafe { &*(buffer.as_ptr().cast::<TOKEN_USER>()) };
    sid_to_text(user.User.Sid)
}

fn current_process_is_local_system() -> Result<bool, ()> {
    let current = current_token_sid_text().map_err(|_| ())?;
    let current = StringSid::parse_canonical(&current).map_err(|_| ())?;
    let system = system_sid()?;
    Ok(unsafe { EqualSid(current.raw(), system.raw()) } != 0)
}

fn lookup_account_sid_text(account_name: &OsStr) -> Result<String, CommandError> {
    let account_name = canonicalize_lookup_account_name(account_name)?;
    let account_name = wide(&account_name);
    let mut sid_bytes = 0;
    let mut domain_units = 0;
    let mut account_kind: SID_NAME_USE = 0;
    unsafe {
        LookupAccountNameW(
            null(),
            account_name.as_ptr(),
            null_mut(),
            &mut sid_bytes,
            null_mut(),
            &mut domain_units,
            &mut account_kind,
        )
    };
    if unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER || sid_bytes == 0 {
        return Err(CommandError::Failed);
    }
    let mut sid = vec![0u8; sid_bytes as usize];
    let mut domain = vec![0u16; domain_units as usize];
    if unsafe {
        LookupAccountNameW(
            null(),
            account_name.as_ptr(),
            sid.as_mut_ptr().cast(),
            &mut sid_bytes,
            domain.as_mut_ptr(),
            &mut domain_units,
            &mut account_kind,
        )
    } == 0
    {
        return Err(CommandError::Failed);
    }
    sid_to_text(sid.as_mut_ptr().cast())
}

fn canonicalize_lookup_account_name(account_name: &OsStr) -> Result<OsString, CommandError> {
    let units = account_name.encode_wide().collect::<Vec<_>>();
    if units.is_empty() || units.contains(&0) {
        return Err(CommandError::Failed);
    }
    if units.starts_with(&[b'.' as u16, b'\\' as u16]) {
        let mut computer = vec![0u16; 256];
        let mut length = computer.len() as u32;
        if unsafe { GetComputerNameW(computer.as_mut_ptr(), &mut length) } == 0
            || length == 0
            || length as usize >= computer.len()
        {
            return Err(CommandError::Failed);
        }
        computer.truncate(length as usize);
        return canonicalize_local_account_name(&units, &computer);
    }
    if units.first() == Some(&(b'.' as u16)) {
        return Err(CommandError::Failed);
    }
    Ok(account_name.to_os_string())
}

fn canonicalize_local_account_name(
    account_name: &[u16],
    computer_name: &[u16],
) -> Result<OsString, CommandError> {
    let prefix = [b'.' as u16, b'\\' as u16];
    if !account_name.starts_with(&prefix)
        || computer_name.is_empty()
        || computer_name
            .iter()
            .any(|unit| *unit == 0 || *unit == b'\\' as u16 || *unit == b'/' as u16)
    {
        return Err(CommandError::Failed);
    }
    let name = &account_name[prefix.len()..];
    if name.is_empty()
        || name.len() > 256
        || name == [b'.' as u16]
        || name == [b'.' as u16, b'.' as u16]
        || name
            .last()
            .is_some_and(|unit| *unit == b' ' as u16 || *unit == b'.' as u16)
        || name.iter().copied().any(invalid_local_account_unit)
    {
        return Err(CommandError::Failed);
    }
    let mut canonical = Vec::with_capacity(computer_name.len() + 1 + name.len());
    canonical.extend_from_slice(computer_name);
    canonical.push(b'\\' as u16);
    canonical.extend_from_slice(name);
    Ok(OsString::from_wide(&canonical))
}

fn invalid_local_account_unit(unit: u16) -> bool {
    unit < 0x20
        || [
            b'"', b'/', b'\\', b'[', b']', b':', b';', b'|', b'=', b',', b'+', b'*', b'?', b'<',
            b'>',
        ]
        .into_iter()
        .map(u16::from)
        .any(|invalid| unit == invalid)
}

fn sid_to_text(sid: PSID) -> Result<String, CommandError> {
    if sid.is_null() {
        return Err(CommandError::Failed);
    }
    let mut text = null_mut();
    if unsafe { ConvertSidToStringSidW(sid, &mut text) } == 0 || text.is_null() {
        return Err(CommandError::Failed);
    }
    let length = (0..256)
        .find(|index| unsafe { *text.add(*index) } == 0)
        .ok_or(CommandError::Failed);
    let value = length.and_then(|length| {
        String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) })
            .map_err(|_| CommandError::Failed)
    });
    unsafe { LocalFree(text.cast()) };
    value
}

#[allow(clippy::too_many_arguments)]
fn service_runtime_binding_sha256(
    service_name: &str,
    data_dir: &Path,
    executable_path: &Path,
    command: &[OsString],
    service_account_sid: &str,
    current_token_sid: &str,
    provisioning_owner_sid: &str,
    profile_name: &str,
    profile_sid: &str,
) -> Result<[u8; 32], CommandError> {
    let executable_sha256 =
        Sha256::digest(fs::read(executable_path).map_err(|_| CommandError::Failed)?);
    let mut digest = Sha256::new();
    digest.update(b"assemblywright.stopped-exact-service-runtime.v2\0");
    for value in [
        service_name,
        service_account_sid,
        current_token_sid,
        provisioning_owner_sid,
        profile_name,
        profile_sid,
    ] {
        digest.update(value.as_bytes());
        digest.update([0]);
    }
    digest.update(executable_sha256);
    for value in
        std::iter::once(data_dir.as_os_str()).chain(command.iter().map(OsString::as_os_str))
    {
        for unit in value.encode_wide() {
            digest.update(unit.to_le_bytes());
        }
        digest.update([0, 0]);
    }
    Ok(digest.finalize().into())
}

#[derive(Clone, Debug)]
struct DirectoryBinding {
    path: PathBuf,
    volume: u64,
    index: u64,
    acl: RuntimeDirectoryAcl,
    handle: Arc<File>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RuntimeDirectoryAcl {
    #[cfg(test)]
    KnownFolder,
    SharedTraverse,
    InstanceRoot,
}

pub(super) fn canonical_runtime_root(
    runtime_instance: &str,
) -> Result<PathBuf, ProvisioningRejection> {
    if !valid_runtime_instance(runtime_instance) {
        return Err(ProvisioningRejection::RuntimeLocation);
    }
    let program_data =
        canonical_program_data().map_err(|_| ProvisioningRejection::RuntimeLocation)?;
    let expected = program_data
        .join(RUNTIME_VENDOR_DIRECTORY)
        .join(RUNTIME_NAMESPACE_DIRECTORY)
        .join(runtime_instance);
    for path in [
        program_data.clone(),
        program_data.join(RUNTIME_VENDOR_DIRECTORY),
        program_data
            .join(RUNTIME_VENDOR_DIRECTORY)
            .join(RUNTIME_NAMESPACE_DIRECTORY),
        expected.clone(),
    ] {
        reject_directory_link(&path).map_err(|_| ProvisioningRejection::RuntimeLocation)?;
        if fs::canonicalize(&path).map_err(|_| ProvisioningRejection::RuntimeLocation)? != path {
            return Err(ProvisioningRejection::RuntimeLocation);
        }
    }
    Ok(expected)
}

fn valid_runtime_instance(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.is_ascii()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-'))
}

impl super::ProcessIsolation {
    pub(super) fn revalidate(&self) -> Result<(), CommandError> {
        self.profile.revalidate().map_err(|_| CommandError::Failed)
    }
}

pub(super) fn validate_provisioning(
    provisioning: Provisioning<'_>,
) -> Result<(super::ProcessIsolation, super::ProcessIsolation), ProvisioningRejection> {
    if provisioning.provider_profile_name != PROVIDER_PROFILE
        || provisioning.github_profile_name != GITHUB_PROFILE
        || provisioning.provider_profile_sid == provisioning.github_profile_sid
    {
        return Err(ProvisioningRejection::ProfileIdentity);
    }
    let provider_sid = ProfileSid::derive_and_match(
        provisioning.provider_profile_name,
        provisioning.provider_profile_sid,
    )
    .map_err(|_| ProvisioningRejection::ProfileIdentity)?;
    let github_sid = ProfileSid::derive_and_match(
        provisioning.github_profile_name,
        provisioning.github_profile_sid,
    )
    .map_err(|_| ProvisioningRejection::ProfileIdentity)?;
    let provisioning_owner = StringSid::parse_canonical(provisioning.provisioning_owner_sid)
        .map_err(|_| ProvisioningRejection::OwnerIdentity)?;
    let system = system_sid().map_err(|_| ProvisioningRejection::OwnerIdentity)?;
    if unsafe { EqualSid(provisioning_owner.raw(), system.raw()) } != 0 {
        return Err(ProvisioningRejection::OwnerIdentity);
    }
    validate_path(
        provisioning.data_root,
        true,
        AclScope::Master,
        provisioning_owner.raw(),
        None,
        None,
    )
    .map_err(|_| ProvisioningRejection::DataAcl)?;
    validate_master_tree(
        provisioning.data_root,
        provisioning_owner.raw(),
        provider_sid.raw(),
        github_sid.raw(),
    )
    .map_err(|_| ProvisioningRejection::MasterTreeAcl)?;
    validate_path(
        provisioning.locator_root,
        true,
        AclScope::Master,
        provisioning_owner.raw(),
        None,
        None,
    )
    .map_err(|_| ProvisioningRejection::MasterTreeAcl)?;
    let runtime_ancestors = validate_runtime_ancestors(
        provisioning.planning_root,
        provisioning_owner.raw(),
        provider_sid.raw(),
        github_sid.raw(),
    )
    .map_err(|_| ProvisioningRejection::RuntimeLocation)?;
    validate_path(
        provisioning.planning_root,
        true,
        AclScope::TraverseOnly,
        provisioning_owner.raw(),
        Some(provider_sid.raw()),
        Some(github_sid.raw()),
    )
    .map_err(|_| ProvisioningRejection::PlanningAcl)?;
    let master_check_root = provisioning.planning_root.join("master-check");
    validate_named_allowlist(
        provisioning.planning_root,
        &["provider", "github", "master-check"],
    )
    .map_err(|_| ProvisioningRejection::PlanningAllowlist)?;
    validate_path(
        &master_check_root,
        true,
        AclScope::Master,
        provisioning_owner.raw(),
        Some(provider_sid.raw()),
        Some(github_sid.raw()),
    )
    .map_err(|_| ProvisioningRejection::MasterCheck)?;
    validate_named_allowlist(&master_check_root, &["assemblywright-master.exe"])
        .map_err(|_| ProvisioningRejection::MasterCheck)?;
    validate_master_entry(
        &master_check_root.join("assemblywright-master.exe"),
        provisioning_owner.raw(),
        provider_sid.raw(),
        github_sid.raw(),
    )
    .map_err(|_| ProvisioningRejection::MasterCheck)?;
    validate_path(
        provisioning.config_path,
        false,
        AclScope::Master,
        provisioning_owner.raw(),
        None,
        None,
    )
    .map_err(|_| ProvisioningRejection::ConfigAcl)?;
    let provider_scope_paths = provisioning
        .provider_paths
        .iter()
        .enumerate()
        .map(|(index, path)| ((*path).to_path_buf(), index >= 4))
        .collect::<Vec<_>>();
    let github_scope_paths = provisioning
        .github_paths
        .iter()
        .enumerate()
        .map(|(index, path)| ((*path).to_path_buf(), index >= 1))
        .collect::<Vec<_>>();
    validate_scope(
        provisioning.provider_root,
        &provider_scope_paths,
        provisioning_owner.raw(),
        provider_sid.raw(),
        github_sid.raw(),
    )
    .map_err(|_| ProvisioningRejection::ProviderScope)?;
    validate_scope(
        provisioning.github_root,
        &github_scope_paths,
        provisioning_owner.raw(),
        github_sid.raw(),
        provider_sid.raw(),
    )
    .map_err(|_| ProvisioningRejection::GithubScope)?;
    install_provider_low_integrity_labels(provisioning.provider_root, &provider_scope_paths)
        .map_err(|_| ProvisioningRejection::ProviderScope)?;
    validate_provider_integrity_scope(provisioning.provider_root, &provider_scope_paths)
        .map_err(|_| ProvisioningRejection::ProviderScope)?;
    let protected_paths = vec![provisioning.config_path.to_path_buf()];
    let provider = ProfileBinding {
        name: provisioning.provider_profile_name.to_string(),
        sid: provisioning.provider_profile_sid.to_string(),
        provisioning_owner_sid: provisioning.provisioning_owner_sid.to_string(),
        scope_root: provisioning.provider_root.to_path_buf(),
        peer_root: provisioning.github_root.to_path_buf(),
        planning_root: provisioning.planning_root.to_path_buf(),
        master_check_root: master_check_root.clone(),
        data_root: provisioning.data_root.to_path_buf(),
        locator_root: provisioning.locator_root.to_path_buf(),
        runtime_ancestors: runtime_ancestors.clone(),
        protected_paths: protected_paths.clone(),
        scope_paths: provider_scope_paths,
    };
    let github = ProfileBinding {
        name: provisioning.github_profile_name.to_string(),
        sid: provisioning.github_profile_sid.to_string(),
        provisioning_owner_sid: provisioning.provisioning_owner_sid.to_string(),
        scope_root: provisioning.github_root.to_path_buf(),
        peer_root: provisioning.provider_root.to_path_buf(),
        planning_root: provisioning.planning_root.to_path_buf(),
        master_check_root,
        data_root: provisioning.data_root.to_path_buf(),
        locator_root: provisioning.locator_root.to_path_buf(),
        runtime_ancestors,
        protected_paths,
        scope_paths: github_scope_paths,
    };
    provider
        .revalidate()
        .map_err(|_| ProvisioningRejection::Revalidation)?;
    github
        .revalidate()
        .map_err(|_| ProvisioningRejection::Revalidation)?;
    Ok((
        super::ProcessIsolation { profile: provider },
        super::ProcessIsolation { profile: github },
    ))
}

impl ProfileBinding {
    fn revalidate(&self) -> Result<(), ()> {
        let provisioning_owner =
            StringSid::parse_canonical(&self.provisioning_owner_sid).map_err(|_| ())?;
        let system = system_sid()?;
        if unsafe { EqualSid(provisioning_owner.raw(), system.raw()) } != 0 {
            return Err(());
        }
        let own = ProfileSid::derive_and_match(&self.name, &self.sid)?;
        let peer_name = if self.name == PROVIDER_PROFILE {
            GITHUB_PROFILE
        } else {
            PROVIDER_PROFILE
        };
        let peer = ProfileSid::derive(peer_name)?;
        validate_path(
            &self.data_root,
            true,
            AclScope::Master,
            provisioning_owner.raw(),
            None,
            None,
        )?;
        validate_master_tree(
            &self.data_root,
            provisioning_owner.raw(),
            own.raw(),
            peer.raw(),
        )?;
        validate_path(
            &self.locator_root,
            true,
            AclScope::Master,
            provisioning_owner.raw(),
            None,
            None,
        )?;
        revalidate_runtime_ancestors(
            &self.runtime_ancestors,
            provisioning_owner.raw(),
            own.raw(),
            peer.raw(),
        )?;
        validate_path(
            &self.planning_root,
            true,
            AclScope::TraverseOnly,
            provisioning_owner.raw(),
            Some(own.raw()),
            Some(peer.raw()),
        )?;
        validate_named_allowlist(&self.planning_root, &["provider", "github", "master-check"])?;
        validate_path(
            &self.master_check_root,
            true,
            AclScope::Master,
            provisioning_owner.raw(),
            Some(own.raw()),
            Some(peer.raw()),
        )?;
        validate_named_allowlist(&self.master_check_root, &["assemblywright-master.exe"])?;
        validate_master_entry(
            &self.master_check_root.join("assemblywright-master.exe"),
            provisioning_owner.raw(),
            own.raw(),
            peer.raw(),
        )?;
        validate_scope(
            &self.scope_root,
            &self.scope_paths,
            provisioning_owner.raw(),
            own.raw(),
            peer.raw(),
        )?;
        if self.name == PROVIDER_PROFILE {
            validate_provider_integrity_scope(&self.scope_root, &self.scope_paths)?;
        }
        validate_path(
            &self.peer_root,
            true,
            AclScope::ProfileRead,
            provisioning_owner.raw(),
            Some(peer.raw()),
            Some(own.raw()),
        )?;
        for path in &self.protected_paths {
            validate_path(
                path,
                false,
                AclScope::Master,
                provisioning_owner.raw(),
                None,
                None,
            )?;
        }
        Ok(())
    }
}

fn canonical_program_data() -> Result<PathBuf, ()> {
    let mut raw = null_mut();
    let status = unsafe {
        SHGetKnownFolderPath(
            &FOLDERID_ProgramData,
            KF_FLAG_DEFAULT as u32,
            null_mut(),
            &mut raw,
        )
    };
    if status < 0 || raw.is_null() {
        return Err(());
    }
    let result = (|| {
        let mut length = 0_usize;
        while length < 32_768 && unsafe { *raw.add(length) } != 0 {
            length += 1;
        }
        if length == 0 || length == 32_768 {
            return Err(());
        }
        let path = PathBuf::from(std::ffi::OsString::from_wide(unsafe {
            std::slice::from_raw_parts(raw, length)
        }));
        reject_directory_link(&path)?;
        fs::canonicalize(path).map_err(|_| ())
    })();
    unsafe { CoTaskMemFree(raw.cast()) };
    result
}

fn reject_directory_link(path: &Path) -> Result<(), ()> {
    super::reject_link(path).map_err(|_| ())?;
    let metadata = fs::symlink_metadata(path).map_err(|_| ())?;
    if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(());
    }
    Ok(())
}

fn validate_runtime_ancestors(
    planning_root: &Path,
    provisioning_owner: PSID,
    provider: PSID,
    github: PSID,
) -> Result<Vec<DirectoryBinding>, ()> {
    let program_data = canonical_program_data()?;
    let vendor = program_data.join(RUNTIME_VENDOR_DIRECTORY);
    let namespace = vendor.join(RUNTIME_NAMESPACE_DIRECTORY);
    if planning_root.parent() != Some(namespace.as_path())
        || namespace.parent() != Some(vendor.as_path())
        || vendor.parent() != Some(program_data.as_path())
    {
        return Err(());
    }
    let mut bindings = Vec::with_capacity(4);
    for (path, acl) in [
        (program_data, RuntimeDirectoryAcl::SharedTraverse),
        (vendor, RuntimeDirectoryAcl::SharedTraverse),
        (namespace, RuntimeDirectoryAcl::SharedTraverse),
        (
            planning_root.to_path_buf(),
            RuntimeDirectoryAcl::InstanceRoot,
        ),
    ] {
        reject_directory_link(&path)?;
        if fs::canonicalize(&path).map_err(|_| ())? != path {
            return Err(());
        }
        let binding = directory_binding(path, acl)?;
        validate_runtime_directory_acl(&binding, provisioning_owner, provider, github)?;
        bindings.push(binding);
    }
    Ok(bindings)
}

fn revalidate_runtime_ancestors(
    expected: &[DirectoryBinding],
    provisioning_owner: PSID,
    provider: PSID,
    github: PSID,
) -> Result<(), ()> {
    if expected.len() != 4 {
        return Err(());
    }
    for binding in expected {
        reject_directory_link(&binding.path)?;
        if fs::canonicalize(&binding.path).map_err(|_| ())? != binding.path {
            return Err(());
        }
        let identity = file_identity(&binding.handle).map_err(|_| ())?;
        if (identity.0, identity.1) != (binding.volume, binding.index) {
            return Err(());
        }
        let current = open_bound_directory(&binding.path)?;
        let current_identity = file_identity(&current).map_err(|_| ())?;
        if (current_identity.0, current_identity.1) != (binding.volume, binding.index) {
            return Err(());
        }
        validate_runtime_directory_acl(binding, provisioning_owner, provider, github)?;
    }
    Ok(())
}

fn directory_binding(path: PathBuf, acl: RuntimeDirectoryAcl) -> Result<DirectoryBinding, ()> {
    reject_directory_link(&path)?;
    let directory = Arc::new(open_bound_directory(&path)?);
    let (volume, index, _) = file_identity(&directory).map_err(|_| ())?;
    Ok(DirectoryBinding {
        path,
        volume,
        index,
        acl,
        handle: directory,
    })
}

fn open_bound_directory(path: &Path) -> Result<File, ()> {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    options.open(path).map_err(|_| ())
}

fn validate_runtime_directory_acl(
    binding: &DirectoryBinding,
    provisioning_owner: PSID,
    provider: PSID,
    github: PSID,
) -> Result<(), ()> {
    match binding.acl {
        #[cfg(test)]
        RuntimeDirectoryAcl::KnownFolder => Ok(()),
        RuntimeDirectoryAcl::SharedTraverse => {
            validate_shared_traverse_acl_handle(&binding.handle, provider, github)
        }
        RuntimeDirectoryAcl::InstanceRoot => validate_acl_handle(
            &binding.handle,
            AclScope::TraverseOnly,
            provisioning_owner,
            Some(provider),
            Some(github),
        ),
    }
}

fn validate_shared_traverse_acl_handle(
    directory: &File,
    provider: PSID,
    github: PSID,
) -> Result<(), ()> {
    let mut dacl = null_mut();
    let mut descriptor = null_mut();
    let status = unsafe {
        GetSecurityInfo(
            directory.as_raw_handle() as HANDLE,
            SE_FILE_OBJECT,
            DACL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            &mut dacl,
            null_mut(),
            &mut descriptor,
        )
    };
    if status != 0 || dacl.is_null() || descriptor.is_null() {
        if !descriptor.is_null() {
            unsafe { LocalFree(descriptor) };
        }
        return Err(());
    }
    let result = (|| {
        for sid in [provider, github] {
            if effective_trustee_rights(dacl, sid)? != FILE_TRAVERSE {
                return Err(());
            }
        }
        let mut info: ACL_SIZE_INFORMATION = unsafe { zeroed() };
        if unsafe {
            GetAclInformation(
                dacl,
                (&mut info as *mut ACL_SIZE_INFORMATION).cast(),
                size_of::<ACL_SIZE_INFORMATION>() as u32,
                AclSizeInformation,
            )
        } == 0
        {
            return Err(());
        }
        let mut provider_count = 0_u8;
        let mut github_count = 0_u8;
        for index in 0..info.AceCount {
            let mut raw: *mut c_void = null_mut();
            if unsafe { GetAce(dacl, index, &mut raw) } == 0 || raw.is_null() {
                return Err(());
            }
            let ace = unsafe { &*(raw.cast::<ACCESS_ALLOWED_ACE>()) };
            if u32::from(ace.Header.AceType) != ACCESS_ALLOWED_ACE_TYPE {
                continue;
            }
            let sid = addr_of!(ace.SidStart) as PSID;
            let count = if unsafe { EqualSid(sid, provider) } != 0 {
                &mut provider_count
            } else if unsafe { EqualSid(sid, github) } != 0 {
                &mut github_count
            } else {
                continue;
            };
            if !valid_shared_traverse_ace(ace.Mask, ace.Header.AceFlags) || *count != 0 {
                return Err(());
            }
            *count += 1;
        }
        (provider_count == 1 && github_count == 1)
            .then_some(())
            .ok_or(())
    })();
    unsafe { LocalFree(descriptor) };
    result
}

fn valid_shared_traverse_ace(mask: u32, flags: u8) -> bool {
    mask == FILE_TRAVERSE && flags == 0
}

fn effective_trustee_rights(
    dacl: *mut windows_sys::Win32::Security::ACL,
    sid: PSID,
) -> Result<u32, ()> {
    let trustee = TRUSTEE_W {
        pMultipleTrustee: null_mut(),
        MultipleTrusteeOperation: NO_MULTIPLE_TRUSTEE,
        TrusteeForm: TRUSTEE_IS_SID,
        TrusteeType: TRUSTEE_IS_UNKNOWN,
        ptstrName: sid.cast(),
    };
    let mut rights = 0_u32;
    (unsafe { GetEffectiveRightsFromAclW(dacl, &trustee, &mut rights) } == 0)
        .then_some(rights)
        .ok_or(())
}

fn validate_master_tree(
    data_root: &Path,
    provisioning_owner: PSID,
    provider: PSID,
    github: PSID,
) -> Result<(), ()> {
    for entry in fs::read_dir(data_root).map_err(|_| ())? {
        let path = entry.map_err(|_| ())?.path();
        validate_master_entry(&path, provisioning_owner, provider, github)?;
    }
    Ok(())
}

fn validate_master_entry(
    path: &Path,
    provisioning_owner: PSID,
    provider: PSID,
    github: PSID,
) -> Result<(), ()> {
    let metadata = fs::metadata(path).map_err(|_| ())?;
    validate_path_with_file_share(
        path,
        metadata.is_dir(),
        AclScope::Master,
        provisioning_owner,
        Some(provider),
        Some(github),
        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
    )?;
    if metadata.is_dir() {
        for entry in fs::read_dir(path).map_err(|_| ())? {
            validate_master_entry(
                &entry.map_err(|_| ())?.path(),
                provisioning_owner,
                provider,
                github,
            )?;
        }
    }
    Ok(())
}

fn validate_scope(
    root: &Path,
    paths: &[(PathBuf, bool)],
    provisioning_owner: PSID,
    own: PSID,
    peer: PSID,
) -> Result<(), ()> {
    let allow_system_owned_children = current_process_is_local_system()?;
    validate_root_allowlist(root, paths)?;
    validate_path(
        root,
        true,
        AclScope::ProfileRead,
        provisioning_owner,
        Some(own),
        Some(peer),
    )?;
    for (path, writable) in paths {
        let directory = fs::metadata(path).map_err(|_| ())?.is_dir();
        validate_path(
            path,
            directory,
            if *writable {
                if directory {
                    AclScope::ProfileWriteRoot
                } else {
                    AclScope::ProfileWriteFile
                }
            } else {
                AclScope::ProfileRead
            },
            provisioning_owner,
            Some(own),
            Some(peer),
        )?;
        if *writable && directory {
            validate_writable_tree(
                path,
                provisioning_owner,
                own,
                peer,
                allow_system_owned_children,
            )?;
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum IntegrityLabelScope {
    Unlabeled,
    WritableRoot,
    WritableChild(bool),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProviderIntegrityState {
    Unlabeled,
    Incomplete,
    Exact,
}

fn install_provider_low_integrity_labels(root: &Path, paths: &[(PathBuf, bool)]) -> Result<(), ()> {
    match provider_integrity_state(root, paths)? {
        ProviderIntegrityState::Exact => return Ok(()),
        ProviderIntegrityState::Unlabeled | ProviderIntegrityState::Incomplete => {}
    }
    let root_guard = IntegrityObject::open(root, true, false)?;
    validate_integrity_object(&root_guard, IntegrityLabelScope::Unlabeled)?;
    for (path, writable) in paths {
        if path.parent() != Some(root) {
            return Err(());
        }
        if *writable {
            install_writable_integrity_tree(path, true)?;
        }
    }
    root_guard.revalidate_path()?;
    validate_provider_integrity_scope(root, paths)
}

fn validate_provider_integrity_scope(root: &Path, paths: &[(PathBuf, bool)]) -> Result<(), ()> {
    let root_guard = IntegrityObject::open(root, true, false)?;
    validate_integrity_object(&root_guard, IntegrityLabelScope::Unlabeled)?;
    for (path, writable) in paths {
        if path.parent() != Some(root) {
            return Err(());
        }
        if *writable {
            validate_writable_integrity_tree(path)?;
        } else {
            let object = IntegrityObject::open_unknown(path, false)?;
            validate_integrity_object(&object, IntegrityLabelScope::Unlabeled)?;
            object.revalidate_path()?;
        }
    }
    root_guard.revalidate_path()?;
    Ok(())
}

fn provider_integrity_state(
    root: &Path,
    paths: &[(PathBuf, bool)],
) -> Result<ProviderIntegrityState, ()> {
    let root_guard = IntegrityObject::open(root, true, false)?;
    validate_integrity_object(&root_guard, IntegrityLabelScope::Unlabeled)?;
    let mut observed = None;
    for (path, writable) in paths {
        if path.parent() != Some(root) {
            return Err(());
        }
        if !*writable {
            let object = IntegrityObject::open_unknown(path, false)?;
            validate_integrity_object(&object, IntegrityLabelScope::Unlabeled)?;
            object.revalidate_path()?;
            continue;
        }
        observe_writable_integrity_tree(path, &mut observed)?;
    }
    root_guard.revalidate_path()?;
    observed.ok_or(())
}

fn accumulate_integrity_state(state: &mut Option<ProviderIntegrityState>, labeled: bool) {
    let current = if labeled {
        ProviderIntegrityState::Exact
    } else {
        ProviderIntegrityState::Unlabeled
    };
    *state = Some(match *state {
        Some(ProviderIntegrityState::Incomplete) => ProviderIntegrityState::Incomplete,
        Some(expected) if expected != current => ProviderIntegrityState::Incomplete,
        Some(expected) => expected,
        None => current,
    });
}

fn observe_writable_integrity_tree(
    path: &Path,
    state: &mut Option<ProviderIntegrityState>,
) -> Result<(), ()> {
    let root = IntegrityObject::open(path, true, false)?;
    observe_writable_integrity_object(root, true, state)
}

fn observe_writable_integrity_object(
    object: IntegrityObject,
    root: bool,
    state: &mut Option<ProviderIntegrityState>,
) -> Result<(), ()> {
    let scope = if root {
        IntegrityLabelScope::WritableRoot
    } else {
        IntegrityLabelScope::WritableChild(true)
    };
    accumulate_integrity_state(state, observe_integrity_object(&object, scope)?);
    for entry in fs::read_dir(&object.path).map_err(|_| ())? {
        let path = entry.map_err(|_| ())?.path();
        let child = IntegrityObject::open_unknown(&path, false)?;
        let directory = child.directory;
        accumulate_integrity_state(
            state,
            observe_integrity_object(&child, IntegrityLabelScope::WritableChild(directory))?,
        );
        if directory {
            observe_writable_integrity_object(child, false, state)?;
        } else {
            child.revalidate_path()?;
        }
    }
    object.revalidate_path()?;
    Ok(())
}

fn install_writable_integrity_tree(path: &Path, root: bool) -> Result<(), ()> {
    let tree = IntegrityTree::collect(path)?;
    if root && !tree.object.directory {
        return Err(());
    }
    tree.revalidate_paths()?;
    tree.install_labels()?;
    tree.validate_labels(root)?;
    tree.revalidate_paths()?;
    Ok(())
}

fn validate_writable_integrity_tree(path: &Path) -> Result<(), ()> {
    let object = IntegrityObject::open(path, true, false)?;
    validate_writable_integrity_object(object, true)
}

fn validate_writable_integrity_object(object: IntegrityObject, root: bool) -> Result<(), ()> {
    validate_integrity_object(
        &object,
        if root {
            IntegrityLabelScope::WritableRoot
        } else {
            IntegrityLabelScope::WritableChild(true)
        },
    )?;
    for entry in fs::read_dir(&object.path).map_err(|_| ())? {
        let path = entry.map_err(|_| ())?.path();
        let child = IntegrityObject::open_unknown(&path, false)?;
        if child.directory {
            validate_writable_integrity_object(child, false)?;
        } else {
            validate_integrity_object(&child, IntegrityLabelScope::WritableChild(false))?;
            child.revalidate_path()?;
        }
    }
    object.revalidate_path()?;
    Ok(())
}

fn install_low_integrity_label(object: &IntegrityObject) -> Result<(), ()> {
    let sddl = if object.directory {
        "S:(ML;OICI;NW;;;LW)"
    } else {
        "S:(ML;;NW;;;LW)"
    };
    let descriptor = LocalSecurityDescriptor::from_sddl(sddl).map_err(|_| ())?;
    let sacl = descriptor.sacl().map_err(|_| ())?;
    let status = unsafe {
        SetSecurityInfo(
            object.file.as_raw_handle() as HANDLE,
            SE_FILE_OBJECT,
            LABEL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            null(),
            sacl,
        )
    };
    if status != 0 {
        return Err(());
    }
    Ok(())
}

fn validate_integrity_object(
    object: &IntegrityObject,
    scope: IntegrityLabelScope,
) -> Result<(), ()> {
    let observed = observe_integrity_object(object, scope)?;
    match scope {
        IntegrityLabelScope::Unlabeled if !observed => Ok(()),
        IntegrityLabelScope::WritableRoot | IntegrityLabelScope::WritableChild(_) if observed => {
            Ok(())
        }
        _ => Err(()),
    }
}

fn observe_integrity_object(
    object: &IntegrityObject,
    scope: IntegrityLabelScope,
) -> Result<bool, ()> {
    let mut sacl = null_mut();
    let mut descriptor = null_mut();
    let status = unsafe {
        GetSecurityInfo(
            object.file.as_raw_handle() as HANDLE,
            SE_FILE_OBJECT,
            LABEL_SECURITY_INFORMATION,
            null_mut(),
            null_mut(),
            null_mut(),
            &mut sacl,
            &mut descriptor,
        )
    };
    if status != 0 || descriptor.is_null() {
        if !descriptor.is_null() {
            unsafe { LocalFree(descriptor) };
        }
        return Err(());
    }
    let result = validate_integrity_label_inner(sacl, descriptor, scope);
    unsafe { LocalFree(descriptor) };
    result
}

fn validate_integrity_label_inner(
    sacl: *mut windows_sys::Win32::Security::ACL,
    descriptor: *mut c_void,
    scope: IntegrityLabelScope,
) -> Result<bool, ()> {
    if sacl.is_null() {
        return Ok(false);
    }
    let mut control = 0_u16;
    let mut revision = 0_u32;
    if unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } == 0
        || control & SE_SACL_PROTECTED != 0
    {
        return Err(());
    }
    let mut info: ACL_SIZE_INFORMATION = unsafe { zeroed() };
    if unsafe {
        GetAclInformation(
            sacl,
            (&mut info as *mut ACL_SIZE_INFORMATION).cast(),
            size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        )
    } == 0
    {
        return Err(());
    }
    if info.AceCount == 0 {
        return Ok(false);
    }
    if info.AceCount != 1 {
        return Err(());
    }
    let mut raw: *mut c_void = null_mut();
    if unsafe { GetAce(sacl, 0, &mut raw) } == 0 || raw.is_null() {
        return Err(());
    }
    let ace = unsafe { &*(raw.cast::<SYSTEM_MANDATORY_LABEL_ACE>()) };
    if u32::from(ace.Header.AceType) != SYSTEM_MANDATORY_LABEL_ACE_TYPE
        || ace.Mask != SYSTEM_MANDATORY_LABEL_NO_WRITE_UP
    {
        return Err(());
    }
    let low = StringSid::parse_canonical("S-1-16-4096").map_err(|_| ())?;
    let sid = addr_of!(ace.SidStart) as PSID;
    if unsafe { IsValidSid(sid) } == 0 || unsafe { EqualSid(sid, low.raw()) } == 0 {
        return Err(());
    }
    let flags = u32::from(ace.Header.AceFlags);
    let inheritance = OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE;
    let valid_flags = match scope {
        IntegrityLabelScope::Unlabeled => false,
        IntegrityLabelScope::WritableRoot => flags == inheritance,
        IntegrityLabelScope::WritableChild(true) => {
            flags == inheritance || flags == inheritance | INHERITED_ACE
        }
        IntegrityLabelScope::WritableChild(false) => flags == 0 || flags == INHERITED_ACE,
    };
    valid_flags.then_some(true).ok_or(())
}

struct IntegrityObject {
    path: PathBuf,
    directory: bool,
    identity: (u64, u64, u32),
    file: File,
}

struct IntegrityTree {
    object: IntegrityObject,
    children: Vec<IntegrityTree>,
}

impl IntegrityTree {
    fn collect(path: &Path) -> Result<Self, ()> {
        let object = IntegrityObject::open_unknown(path, true)?;
        let mut children = Vec::new();
        if object.directory {
            for entry in fs::read_dir(&object.path).map_err(|_| ())? {
                children.push(Self::collect(&entry.map_err(|_| ())?.path())?);
            }
        }
        object.revalidate_path()?;
        Ok(Self { object, children })
    }

    fn install_labels(&self) -> Result<(), ()> {
        install_low_integrity_label(&self.object)?;
        for child in &self.children {
            child.install_labels()?;
        }
        Ok(())
    }

    fn validate_labels(&self, root: bool) -> Result<(), ()> {
        let scope = if root {
            IntegrityLabelScope::WritableRoot
        } else {
            IntegrityLabelScope::WritableChild(self.object.directory)
        };
        validate_integrity_object(&self.object, scope)?;
        for child in &self.children {
            child.validate_labels(false)?;
        }
        Ok(())
    }

    fn revalidate_paths(&self) -> Result<(), ()> {
        self.object.revalidate_path()?;
        for child in &self.children {
            child.revalidate_paths()?;
        }
        Ok(())
    }
}

impl IntegrityObject {
    fn open(path: &Path, directory: bool, write: bool) -> Result<Self, ()> {
        let object = Self::open_unknown(path, write)?;
        (object.directory == directory).then_some(object).ok_or(())
    }

    fn open_unknown(path: &Path, write: bool) -> Result<Self, ()> {
        super::reject_link(path).map_err(|_| ())?;
        let file = open_integrity_file(path, write)?;
        let information = file_information(&file).map_err(|_| ())?;
        if information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(());
        }
        let directory = information.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
        let identity = file_identity_from_information(&information);
        let object = Self {
            path: path.to_path_buf(),
            directory,
            identity,
            file,
        };
        object.revalidate_path()?;
        Ok(object)
    }

    fn revalidate_path(&self) -> Result<(), ()> {
        super::reject_link(&self.path).map_err(|_| ())?;
        let current = open_integrity_file(&self.path, false)?;
        let information = file_information(&current).map_err(|_| ())?;
        let directory = information.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
        if information.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || directory != self.directory
            || file_identity_from_information(&information) != self.identity
        {
            return Err(());
        }
        Ok(())
    }
}

fn open_integrity_file(path: &Path, write: bool) -> Result<File, ()> {
    let access = READ_CONTROL | if write { WRITE_OWNER } else { 0 };
    let mut options = OpenOptions::new();
    options
        .access_mode(access)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS);
    options.open(path).map_err(|_| ())
}

fn validate_root_allowlist(root: &Path, paths: &[(PathBuf, bool)]) -> Result<(), ()> {
    let mut expected = HashSet::new();
    for (path, _) in paths {
        let relative = path.strip_prefix(root).map_err(|_| ())?;
        let name = relative.components().next().ok_or(())?.as_os_str();
        if name.is_empty() {
            return Err(());
        }
        expected.insert(name.to_str().ok_or(())?.to_owned());
    }
    validate_name_set(root, expected.into_iter().collect())
}

fn validate_named_allowlist(root: &Path, names: &[&str]) -> Result<(), ()> {
    validate_name_set(root, names.iter().map(|name| (*name).to_owned()).collect())
}

fn validate_name_set(root: &Path, expected: Vec<String>) -> Result<(), ()> {
    let observed = fs::read_dir(root)
        .map_err(|_| ())?
        .map(|entry| {
            entry
                .map_err(|_| ())
                .and_then(|entry| entry.file_name().into_string().map_err(|_| ()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    exact_name_set_matches(&observed, &expected)
        .then_some(())
        .ok_or(())
}

fn exact_name_set_matches(observed: &[String], expected: &[String]) -> bool {
    if observed.len() != expected.len()
        || !observed.iter().chain(expected).all(|name| name.is_ascii())
    {
        return false;
    }
    let observed_exact = observed.iter().map(String::as_str).collect::<HashSet<_>>();
    let expected_exact = expected.iter().map(String::as_str).collect::<HashSet<_>>();
    if observed_exact.len() != observed.len()
        || expected_exact.len() != expected.len()
        || observed_exact != expected_exact
    {
        return false;
    }
    let observed_folded = observed
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    let expected_folded = expected
        .iter()
        .map(|name| name.to_ascii_lowercase())
        .collect::<HashSet<_>>();
    observed_folded.len() == observed.len()
        && expected_folded.len() == expected.len()
        && observed_folded == expected_folded
}

fn validate_writable_tree(
    path: &Path,
    provisioning_owner: PSID,
    own: PSID,
    peer: PSID,
    allow_system_owned_children: bool,
) -> Result<(), ()> {
    for entry in fs::read_dir(path).map_err(|_| ())? {
        let path = entry.map_err(|_| ())?.path();
        let directory = fs::metadata(&path).map_err(|_| ())?.is_dir();
        validate_path(
            &path,
            directory,
            AclScope::ProfileWriteChild(directory, allow_system_owned_children),
            provisioning_owner,
            Some(own),
            Some(peer),
        )?;
        if directory {
            validate_writable_tree(
                &path,
                provisioning_owner,
                own,
                peer,
                allow_system_owned_children,
            )?;
        }
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum AclScope {
    Master,
    TraverseOnly,
    ProfileRead,
    ProfileWriteRoot,
    ProfileWriteFile,
    ProfileWriteChild(bool, bool),
}

fn validate_path(
    path: &Path,
    directory: bool,
    scope: AclScope,
    provisioning_owner: PSID,
    profile: Option<PSID>,
    peer: Option<PSID>,
) -> Result<(), ()> {
    validate_path_with_file_share(
        path,
        directory,
        scope,
        provisioning_owner,
        profile,
        peer,
        FILE_SHARE_READ,
    )
}

fn validate_path_with_file_share(
    path: &Path,
    directory: bool,
    scope: AclScope,
    provisioning_owner: PSID,
    profile: Option<PSID>,
    peer: Option<PSID>,
    file_share_mode: u32,
) -> Result<(), ()> {
    super::reject_link(path).map_err(|_| ())?;
    let metadata = fs::metadata(path).map_err(|_| ())?;
    if metadata.is_dir() != directory
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    {
        return Err(());
    }
    if !directory {
        let mut options = OpenOptions::new();
        options
            .read(true)
            .share_mode(file_share_mode)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        let file = options.open(path).map_err(|_| ())?;
        if file_identity(&file).map_err(|_| ())?.2 != 1 {
            return Err(());
        }
    }
    validate_acl(path, scope, provisioning_owner, profile, peer)
}

fn validate_acl(
    path: &Path,
    scope: AclScope,
    provisioning_owner: PSID,
    profile: Option<PSID>,
    peer: Option<PSID>,
) -> Result<(), ()> {
    let mut wide = wide(path.as_os_str());
    let mut owner: PSID = null_mut();
    let mut dacl = null_mut();
    let mut descriptor = null_mut();
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide.as_mut_ptr(),
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
        return Err(());
    }
    let result = validate_acl_inner(
        owner,
        dacl,
        descriptor,
        scope,
        provisioning_owner,
        profile,
        peer,
    );
    unsafe { LocalFree(descriptor) };
    result
}

fn validate_acl_handle(
    file: &File,
    scope: AclScope,
    provisioning_owner: PSID,
    profile: Option<PSID>,
    peer: Option<PSID>,
) -> Result<(), ()> {
    let mut owner: PSID = null_mut();
    let mut dacl = null_mut();
    let mut descriptor = null_mut();
    let status = unsafe {
        GetSecurityInfo(
            file.as_raw_handle() as HANDLE,
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
        return Err(());
    }
    let result = validate_acl_inner(
        owner,
        dacl,
        descriptor,
        scope,
        provisioning_owner,
        profile,
        peer,
    );
    unsafe { LocalFree(descriptor) };
    result
}

fn validate_acl_inner(
    owner: PSID,
    dacl: *mut windows_sys::Win32::Security::ACL,
    descriptor: *mut c_void,
    scope: AclScope,
    provisioning_owner: PSID,
    profile: Option<PSID>,
    peer: Option<PSID>,
) -> Result<(), ()> {
    let system = system_sid()?;
    if !exact_owner_matches(scope, owner, provisioning_owner, system.raw()) {
        return Err(());
    }
    let mut control = 0_u16;
    let mut revision = 0_u32;
    if unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } == 0 {
        return Err(());
    }
    let protected = control & SE_DACL_PROTECTED != 0;
    match scope {
        AclScope::ProfileWriteChild(_, _) => {}
        _ if !protected => return Err(()),
        _ => {}
    }
    let mut info: ACL_SIZE_INFORMATION = unsafe { zeroed() };
    if unsafe {
        GetAclInformation(
            dacl,
            (&mut info as *mut ACL_SIZE_INFORMATION).cast(),
            size_of::<ACL_SIZE_INFORMATION>() as u32,
            AclSizeInformation,
        )
    } == 0
    {
        return Err(());
    }
    let mut principals = PrincipalPresence::default();
    for index in 0..info.AceCount {
        let mut raw: *mut c_void = null_mut();
        if unsafe { GetAce(dacl, index, &mut raw) } == 0 || raw.is_null() {
            return Err(());
        }
        let ace = unsafe { &*(raw.cast::<ACCESS_ALLOWED_ACE>()) };
        if u32::from(ace.Header.AceType) != ACCESS_ALLOWED_ACE_TYPE {
            return Err(());
        }
        if !valid_write_ace_flags(scope, protected, u32::from(ace.Header.AceFlags)) {
            return Err(());
        }
        let sid = addr_of!(ace.SidStart) as PSID;
        match classify_principal(sid, provisioning_owner, system.raw(), profile, peer).ok_or(())? {
            AclPrincipal::Owner => {
                claim_once(&mut principals.owner)?;
                if ace.Mask & FILE_ALL_ACCESS != FILE_ALL_ACCESS {
                    return Err(());
                }
            }
            AclPrincipal::System => {
                claim_once(&mut principals.system)?;
                if ace.Mask & FILE_ALL_ACCESS != FILE_ALL_ACCESS {
                    return Err(());
                }
            }
            AclPrincipal::Profile => {
                claim_once(&mut principals.profile)?;
                if !valid_profile_mask(scope, ace.Mask) {
                    return Err(());
                }
            }
            AclPrincipal::Peer => {
                claim_once(&mut principals.peer)?;
                if !matches!(scope, AclScope::TraverseOnly) || !valid_profile_mask(scope, ace.Mask)
                {
                    return Err(());
                }
            }
        }
    }
    validate_principal_presence(scope, principals)
}

fn exact_owner_matches(scope: AclScope, observed: PSID, expected: PSID, system: PSID) -> bool {
    if unsafe { EqualSid(expected, system) } != 0 {
        return false;
    }
    (unsafe { EqualSid(observed, expected) } != 0)
        || (matches!(scope, AclScope::ProfileWriteChild(_, true))
            && unsafe { EqualSid(observed, system) } != 0)
}

#[derive(Clone, Copy)]
enum AclPrincipal {
    Owner,
    System,
    Profile,
    Peer,
}

fn classify_principal(
    observed: PSID,
    owner: PSID,
    system: PSID,
    profile: Option<PSID>,
    peer: Option<PSID>,
) -> Option<AclPrincipal> {
    if unsafe { EqualSid(observed, owner) } != 0 {
        Some(AclPrincipal::Owner)
    } else if unsafe { EqualSid(observed, system) } != 0 {
        Some(AclPrincipal::System)
    } else if profile.is_some_and(|expected| unsafe { EqualSid(observed, expected) } != 0) {
        Some(AclPrincipal::Profile)
    } else if peer.is_some_and(|expected| unsafe { EqualSid(observed, expected) } != 0) {
        Some(AclPrincipal::Peer)
    } else {
        None
    }
}

#[derive(Clone, Copy, Default)]
struct PrincipalPresence {
    owner: bool,
    system: bool,
    profile: bool,
    peer: bool,
}

fn claim_once(observed: &mut bool) -> Result<(), ()> {
    if *observed {
        return Err(());
    }
    *observed = true;
    Ok(())
}

fn validate_principal_presence(scope: AclScope, principals: PrincipalPresence) -> Result<(), ()> {
    if !principals.owner || !principals.system {
        return Err(());
    }
    match scope {
        AclScope::Master if principals.profile || principals.peer => Err(()),
        AclScope::TraverseOnly if !principals.profile || !principals.peer => Err(()),
        AclScope::ProfileRead
        | AclScope::ProfileWriteRoot
        | AclScope::ProfileWriteFile
        | AclScope::ProfileWriteChild(_, _)
            if !principals.profile || principals.peer =>
        {
            Err(())
        }
        _ => Ok(()),
    }
}

fn valid_profile_mask(scope: AclScope, mask: u32) -> bool {
    match scope {
        AclScope::TraverseOnly => {
            mask & FILE_TRAVERSE != 0 && mask & !(FILE_TRAVERSE | SYNCHRONIZE) == 0
        }
        AclScope::ProfileRead => mask == (FILE_GENERIC_READ | FILE_GENERIC_EXECUTE),
        AclScope::ProfileWriteRoot
        | AclScope::ProfileWriteFile
        | AclScope::ProfileWriteChild(_, _) => {
            mask == (FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_GENERIC_EXECUTE | DELETE)
        }
        AclScope::Master => false,
    }
}

fn valid_write_ace_flags(scope: AclScope, protected: bool, flags: u32) -> bool {
    match scope {
        AclScope::ProfileWriteRoot => {
            protected && flags == OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE
        }
        AclScope::ProfileWriteFile => protected && flags == 0,
        AclScope::ProfileWriteChild(directory, _) if protected => {
            flags
                == if directory {
                    OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE
                } else {
                    0
                }
        }
        AclScope::ProfileWriteChild(directory, _) => {
            flags
                == if directory {
                    OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE | INHERITED_ACE
                } else {
                    INHERITED_ACE
                }
        }
        _ => true,
    }
}

struct LocalSecurityDescriptor(PSECURITY_DESCRIPTOR);

impl LocalSecurityDescriptor {
    fn from_sddl(sddl: &str) -> Result<Self, CommandError> {
        if sddl.is_empty() || !sddl.is_ascii() || sddl.len() > 512 {
            return Err(CommandError::Failed);
        }
        let sddl = wide(OsStr::new(sddl));
        let mut descriptor = null_mut();
        if unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                sddl.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor,
                null_mut(),
            )
        } == 0
            || descriptor.is_null()
        {
            return Err(CommandError::Failed);
        }
        Ok(Self(descriptor))
    }

    fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0,
            bInheritHandle: 0,
        }
    }

    fn sacl(&self) -> Result<*mut windows_sys::Win32::Security::ACL, CommandError> {
        let mut present = 0;
        let mut defaulted = 0;
        let mut sacl = null_mut();
        if unsafe { GetSecurityDescriptorSacl(self.0, &mut present, &mut sacl, &mut defaulted) }
            == 0
            || present == 0
            || sacl.is_null()
        {
            return Err(CommandError::Failed);
        }
        Ok(sacl)
    }
}

impl Drop for LocalSecurityDescriptor {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { LocalFree(self.0.cast()) };
        }
    }
}

struct OwnedWindowStation(HWINSTA);

impl OwnedWindowStation {
    fn raw(&self) -> HWINSTA {
        self.0
    }
}

impl Drop for OwnedWindowStation {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                CloseWindowStation(self.0);
            }
        }
    }
}

struct OwnedDesktop(HDESK);

impl Drop for OwnedDesktop {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                CloseDesktop(self.0);
            }
        }
    }
}

struct WindowStationAssociationGuard {
    original: HWINSTA,
    restored: bool,
}

impl WindowStationAssociationGuard {
    fn new(original: HWINSTA) -> Self {
        Self {
            original,
            restored: false,
        }
    }

    fn restore(&mut self) -> Result<(), CommandError> {
        if unsafe { SetProcessWindowStation(self.original) } == 0 {
            return Err(CommandError::Failed);
        }
        self.restored = true;
        Ok(())
    }
}

impl Drop for WindowStationAssociationGuard {
    fn drop(&mut self) {
        if !self.restored && unsafe { SetProcessWindowStation(self.original) } == 0 {
            // Continuing with the master attached to a per-effect station would make
            // later launch behavior ambiguous. Terminate instead of silently widening
            // or reusing the private GUI-object boundary.
            std::process::abort();
        }
    }
}

struct PrivateDesktop {
    // Drop the desktop before the containing station.
    _desktop: OwnedDesktop,
    _station: OwnedWindowStation,
    startup_name: Vec<u16>,
}

impl PrivateDesktop {
    fn create(profile: &ProfileBinding) -> Result<Self, CommandError> {
        let station_sddl = private_object_sddl(
            &profile.provisioning_owner_sid,
            &profile.sid,
            WINDOW_STATION_OWNER_ACCESS,
            WINDOW_STATION_PROFILE_ACCESS,
        )?;
        let desktop_sddl = private_object_sddl(
            &profile.provisioning_owner_sid,
            &profile.sid,
            DESKTOP_OWNER_ACCESS,
            DESKTOP_PROFILE_ACCESS,
        )?;
        let station_descriptor = LocalSecurityDescriptor::from_sddl(&station_sddl)?;
        let desktop_descriptor = LocalSecurityDescriptor::from_sddl(&desktop_sddl)?;
        let station_attributes = station_descriptor.attributes();
        let desktop_attributes = desktop_descriptor.attributes();
        let station_name = format!("AssemblywrightPlanning-{}", Uuid::new_v4().simple());
        let station_name_wide = wide(OsStr::new(&station_name));
        let desktop_name_wide = wide(OsStr::new(PRIVATE_DESKTOP_NAME));
        let startup_name = wide(OsStr::new(&format!(
            "{station_name}\\{PRIVATE_DESKTOP_NAME}"
        )));

        // SetProcessWindowStation changes process-global state. Serialize the short
        // create/restore interval and never create a process until restoration succeeds.
        let _creation = WINDOW_STATION_CREATION
            .lock()
            .map_err(|_| CommandError::Failed)?;
        let original = unsafe { GetProcessWindowStation() };
        if original.is_null() {
            return Err(CommandError::Failed);
        }
        let station = unsafe {
            CreateWindowStationW(
                station_name_wide.as_ptr(),
                PRIVATE_WINDOW_STATION_CREATE_ONLY,
                WINDOW_STATION_OWNER_ACCESS,
                &station_attributes,
            )
        };
        if station.is_null() {
            return Err(CommandError::Failed);
        }
        let station = OwnedWindowStation(station);
        let mut association = WindowStationAssociationGuard::new(original);
        if unsafe { SetProcessWindowStation(station.raw()) } == 0 {
            return Err(CommandError::Failed);
        }
        let desktop = unsafe {
            CreateDesktopW(
                desktop_name_wide.as_ptr(),
                null(),
                null_mut(),
                0,
                DESKTOP_OWNER_ACCESS,
                &desktop_attributes,
            )
        };
        if desktop.is_null() {
            return Err(CommandError::Failed);
        }
        let desktop = OwnedDesktop(desktop);
        association.restore()?;
        Ok(Self {
            _desktop: desktop,
            _station: station,
            startup_name,
        })
    }
}

fn private_object_sddl(
    provisioning_owner_sid: &str,
    profile_sid: &str,
    owner_access: u32,
    profile_access: u32,
) -> Result<String, CommandError> {
    let _owner = StringSid::parse_canonical(provisioning_owner_sid)?;
    let _profile = StringSid::parse_canonical(profile_sid)?;
    if provisioning_owner_sid == "S-1-5-18" || provisioning_owner_sid == profile_sid {
        return Err(CommandError::Failed);
    }
    Ok(format!(
        "D:P(A;;0x{owner_access:08x};;;S-1-5-18)(A;;0x{owner_access:08x};;;{provisioning_owner_sid})(A;;0x{profile_access:08x};;;{profile_sid})"
    ))
}

pub(super) fn run_command(
    executable: &Executable,
    control: &PlanningEffectControl,
    invocation: &CommandInvocation<'_>,
    profile: &ProfileBinding,
) -> Result<Vec<u8>, CommandError> {
    let containment_failure = |stage, status| {
        invocation.stderr.record_containment_failure(stage, status);
        CommandError::Failed
    };
    // Keep independent references to every no-delete ancestry handle until all return paths have
    // terminated/reaped the Job. This prevents a delete-child/rename adversary from replacing a
    // directory after its handle-bound identity and ACL were checked.
    let _runtime_directory_guards = profile
        .runtime_ancestors
        .iter()
        .map(|binding| Arc::clone(&binding.handle))
        .collect::<Vec<_>>();
    if !control.poll() {
        return Err(CommandError::Cancelled);
    }
    profile
        .revalidate()
        .map_err(|_| containment_failure(1, 0))?;
    let executable_guard = open_image_guard(executable).map_err(|_| containment_failure(2, 0))?;
    let token = restricted_token(profile.provisioning_owner_sid())
        .map_err(|_| containment_failure(3, 0))?;
    let appcontainer = ProfileSid::derive_and_match(&profile.name, &profile.sid)
        .map_err(|_| containment_failure(4, 0))?;
    let internet = StringSid::parse(INTERNET_CLIENT_SID).map_err(|_| containment_failure(5, 0))?;
    if !control.poll() {
        return Err(CommandError::Cancelled);
    }
    let private_desktop = PrivateDesktop::create(profile).map_err(|_| containment_failure(6, 0))?;
    if !control.poll() {
        return Err(CommandError::Cancelled);
    }
    profile
        .revalidate()
        .map_err(|_| containment_failure(7, 0))?;
    let (stdin_read, stdin_write) = stdin_pipe().map_err(|_| containment_failure(8, 0))?;
    let (stdout_read, stdout_write) = stdout_pipe().map_err(|_| containment_failure(9, 0))?;
    let (stderr_read, stderr_write) = stdout_pipe().map_err(|_| containment_failure(10, 0))?;
    let mut inherited = [stdin_read.raw(), stdout_write.raw(), stderr_write.raw()];
    let mut attributes = AttributeList::new(2).map_err(|_| containment_failure(11, 0))?;
    let mut capability = SID_AND_ATTRIBUTES {
        Sid: internet.raw(),
        Attributes: SE_GROUP_ENABLED as u32,
    };
    let mut security = SECURITY_CAPABILITIES {
        AppContainerSid: appcontainer.raw(),
        Capabilities: &mut capability,
        CapabilityCount: 1,
        Reserved: 0,
    };
    attributes
        .update(
            PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES as usize,
            (&mut security as *mut SECURITY_CAPABILITIES).cast(),
            size_of::<SECURITY_CAPABILITIES>(),
        )
        .map_err(|_| containment_failure(12, 0))?;
    attributes
        .update(
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            inherited.as_mut_ptr().cast(),
            size_of_val(&inherited),
        )
        .map_err(|_| containment_failure(13, 0))?;
    let mut startup: STARTUPINFOEXW = unsafe { zeroed() };
    startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = stdin_read.raw();
    startup.StartupInfo.hStdOutput = stdout_write.raw();
    startup.StartupInfo.hStdError = stderr_write.raw();
    startup.StartupInfo.lpDesktop = private_desktop.startup_name.as_ptr().cast_mut();
    startup.lpAttributeList = attributes.ptr;
    let mut command_line =
        command_line(&executable.path, invocation.args).map_err(|_| containment_failure(14, 0))?;
    let executable_w = wide(executable.path.as_os_str());
    let child_visible_current_dir = child_visible_current_directory(invocation.current_dir);
    let current_dir_w = wide(child_visible_current_dir.as_os_str());
    let environment = environment_block(token.raw(), invocation.environment)
        .map_err(|_| containment_failure(15, 0))?;
    let mut process: PROCESS_INFORMATION = unsafe { zeroed() };
    if unsafe {
        CreateProcessAsUserW(
            token.raw(),
            executable_w.as_ptr(),
            command_line.as_mut_ptr(),
            null(),
            null(),
            1,
            CREATE_SUSPENDED
                | CREATE_NO_WINDOW
                | CREATE_UNICODE_ENVIRONMENT
                | EXTENDED_STARTUPINFO_PRESENT,
            environment.as_ptr().cast(),
            current_dir_w.as_ptr(),
            &startup.StartupInfo,
            &mut process,
        )
    } == 0
    {
        return Err(containment_failure(16, unsafe { GetLastError() }));
    }
    let mut suspended =
        SuspendedProcessGuard::new(process).map_err(|_| containment_failure(17, 0))?;
    drop(stdin_read);
    drop(stdout_write);
    drop(stderr_write);
    profile
        .revalidate()
        .map_err(|_| containment_failure(18, 0))?;
    revalidate_image_guard(executable, &executable_guard)
        .map_err(|_| containment_failure(19, 0))?;
    let job = create_job().map_err(|_| containment_failure(20, 0))?;
    if unsafe { AssignProcessToJobObject(job.raw(), suspended.process()) } == 0 {
        invocation
            .stderr
            .record_containment_failure(21, unsafe { GetLastError() });
        return Err(CommandError::Failed);
    }
    suspended.mark_job_assigned();
    if !control.poll() {
        terminate_job(&job, suspended.process());
        return Err(CommandError::Cancelled);
    }
    if profile.revalidate().is_err() {
        invocation.stderr.record_containment_failure(22, 0);
        terminate_job(&job, suspended.process());
        return Err(CommandError::Failed);
    }
    if !control.poll() {
        terminate_job(&job, suspended.process());
        return Err(CommandError::Cancelled);
    }
    if unsafe { ResumeThread(suspended.thread()) } == u32::MAX {
        invocation
            .stderr
            .record_containment_failure(23, unsafe { GetLastError() });
        terminate_job(&job, suspended.process());
        return Err(CommandError::Failed);
    }
    let mut stdin_file = unsafe { File::from_raw_handle(stdin_write.into_raw() as RawHandle) };
    stdin_file
        .write_all(invocation.input)
        .map_err(|_| containment_failure(24, 0))?;
    drop(stdin_file);
    let stdout_file = unsafe { File::from_raw_handle(stdout_read.into_raw() as RawHandle) };
    let output_thread = bounded_reader(
        stdout_file,
        invocation.max_output,
        invocation.stderr.allows_empty_stdout(invocation.args),
    );
    let stderr_file = unsafe { File::from_raw_handle(stderr_read.into_raw() as RawHandle) };
    let stderr_thread = stderr_reader(stderr_file, &invocation.stderr);
    loop {
        if !control.poll() {
            terminate_job(&job, suspended.process());
            let _ = output_thread.join();
            let _ = stderr_thread.join();
            return Err(CommandError::Cancelled);
        }
        match unsafe { WaitForSingleObject(suspended.process(), 25) } {
            WAIT_OBJECT_0 => {
                return complete_signaled_process(
                    job,
                    suspended.process(),
                    output_thread,
                    stderr_thread,
                    profile,
                    &invocation.stderr,
                );
            }
            WAIT_TIMEOUT => {}
            _ => {
                invocation.stderr.record_containment_failure(25, 0);
                terminate_job(&job, suspended.process());
                let _ = output_thread.join();
                let _ = stderr_thread.join();
                return Err(CommandError::Failed);
            }
        }
    }
}

fn complete_signaled_process(
    job: OwnedHandle,
    process: HANDLE,
    output_thread: std::thread::JoinHandle<Result<Vec<u8>, CommandError>>,
    stderr_thread: StderrDrain,
    profile: &ProfileBinding,
    diagnostic: &CommandStderrMode,
) -> Result<Vec<u8>, CommandError> {
    let mut exit_code = 0;
    let exit_observed = unsafe { GetExitCodeProcess(process, &mut exit_code) } != 0;
    unsafe {
        TerminateJobObject(job.raw(), TERMINATED_EXIT_CODE);
    }
    drop(job);
    let reaped = unsafe { WaitForSingleObject(process, 5_000) } == WAIT_OBJECT_0;
    let mut output = output_thread.join().unwrap_or(Err(CommandError::Failed));
    let stderr_drained = stderr_thread.join();
    if !exit_observed || !reaped {
        if let Ok(bytes) = &mut output {
            bytes.zeroize();
        }
        diagnostic.record_containment_failure(26, 0);
        return Err(CommandError::Failed);
    }
    if stderr_drained.is_err() {
        if let Ok(bytes) = &mut output {
            bytes.zeroize();
        }
        diagnostic.record_containment_failure(29, 0);
        return Err(CommandError::Failed);
    }
    if profile.revalidate().is_err() {
        if let Ok(bytes) = &mut output {
            bytes.zeroize();
        }
        diagnostic.record_containment_failure(27, 0);
        return Err(CommandError::Failed);
    }
    if output.is_err() {
        diagnostic.record_containment_failure(28, 0);
    }
    classify_completed_output(exit_code, output)
}

enum StderrDrain {
    Discard(std::thread::JoinHandle<Result<(), CommandError>>),
    Capture {
        thread: std::thread::JoinHandle<Result<Vec<u8>, CommandError>>,
        output: Arc<Mutex<Vec<u8>>>,
    },
}

impl StderrDrain {
    fn join(self) -> Result<(), CommandError> {
        match self {
            Self::Discard(thread) => thread.join().unwrap_or(Err(CommandError::Failed)),
            Self::Capture { thread, output } => {
                let captured = thread.join().unwrap_or(Err(CommandError::Failed))?;
                let mut guard = output
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                guard.zeroize();
                *guard = captured;
                Ok(())
            }
        }
    }
}

fn stderr_reader(reader: impl Read + Send + 'static, mode: &CommandStderrMode) -> StderrDrain {
    match mode {
        CommandStderrMode::Discard => StderrDrain::Discard(discard_reader(reader)),
        CommandStderrMode::CaptureBounded { max_bytes, output } => StderrDrain::Capture {
            thread: bounded_diagnostic_reader(reader, *max_bytes),
            output: Arc::clone(output),
        },
    }
}

fn bounded_diagnostic_reader(
    mut reader: impl Read + Send + 'static,
    max_bytes: usize,
) -> std::thread::JoinHandle<Result<Vec<u8>, CommandError>> {
    std::thread::spawn(move || {
        let mut retained = Vec::with_capacity(max_bytes);
        let mut buffer = [0_u8; 8192];
        loop {
            let count = match reader.read(&mut buffer) {
                Ok(count) => count,
                Err(_) => {
                    buffer.zeroize();
                    retained.zeroize();
                    return Err(CommandError::Failed);
                }
            };
            if count == 0 {
                buffer.zeroize();
                return Ok(retained);
            }
            let remaining = max_bytes.saturating_sub(retained.len());
            retained.extend_from_slice(&buffer[..count.min(remaining)]);
            buffer[..count].zeroize();
        }
    })
}

fn discard_reader(
    mut reader: impl Read + Send + 'static,
) -> std::thread::JoinHandle<Result<(), CommandError>> {
    std::thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        loop {
            match reader.read(&mut buffer) {
                Ok(0) => return Ok(()),
                Ok(_) => {}
                Err(_) => return Err(CommandError::Failed),
            }
        }
    })
}

fn classify_completed_output(
    exit_code: u32,
    output: Result<Vec<u8>, CommandError>,
) -> Result<Vec<u8>, CommandError> {
    if exit_code != 0 {
        Err(CommandError::Exited(exit_code))
    } else {
        output
    }
}

fn open_image_guard(executable: &Executable) -> Result<File, CommandError> {
    let mut options = OpenOptions::new();
    options.read(true).share_mode(FILE_SHARE_READ);
    let file = options
        .open(&executable.path)
        .map_err(|_| CommandError::Failed)?;
    revalidate_image_guard(executable, &file)?;
    Ok(file)
}

fn revalidate_image_guard(executable: &Executable, file: &File) -> Result<(), CommandError> {
    let mut options = OpenOptions::new();
    options.read(true).share_mode(FILE_SHARE_READ);
    let current = options
        .open(&executable.path)
        .map_err(|_| CommandError::Failed)?;
    let guarded_identity = file_identity(file)?;
    if guarded_identity != file_identity(&current)? || guarded_identity.2 != 1 {
        return Err(CommandError::Failed);
    }
    let _ = verify_executable(executable)?;
    Ok(())
}

fn file_identity(file: &File) -> Result<(u64, u64, u32), CommandError> {
    Ok(file_identity_from_information(&file_information(file)?))
}

fn file_information(file: &File) -> Result<BY_HANDLE_FILE_INFORMATION, CommandError> {
    let mut information: BY_HANDLE_FILE_INFORMATION = unsafe { zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle() as HANDLE, &mut information) } == 0
    {
        return Err(CommandError::Failed);
    }
    Ok(information)
}

fn file_identity_from_information(information: &BY_HANDLE_FILE_INFORMATION) -> (u64, u64, u32) {
    (
        u64::from(information.dwVolumeSerialNumber),
        (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow),
        information.nNumberOfLinks,
    )
}

fn terminate_job(job: &OwnedHandle, process: HANDLE) {
    unsafe {
        TerminateJobObject(job.raw(), TERMINATED_EXIT_CODE);
        WaitForSingleObject(process, 5_000);
    }
}

fn create_job() -> Result<OwnedHandle, CommandError> {
    let job = OwnedHandle::new(unsafe { CreateJobObjectW(null(), null()) })?;
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        | JOB_OBJECT_LIMIT_ACTIVE_PROCESS
        | JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION;
    limits.BasicLimitInformation.ActiveProcessLimit = 8;
    if unsafe {
        SetInformationJobObject(
            job.raw(),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    } == 0
    {
        return Err(CommandError::Failed);
    }
    Ok(job)
}

fn restricted_token(provisioning_owner_sid: &str) -> Result<OwnedHandle, CommandError> {
    let restricted = create_restricted_token()?;
    let _policy = apply_token_default_owner_policy(&restricted, provisioning_owner_sid)?;
    Ok(restricted)
}

fn create_restricted_token() -> Result<OwnedHandle, CommandError> {
    let mut current: HANDLE = null_mut();
    if unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_DEFAULT | TOKEN_ASSIGN_PRIMARY | TOKEN_DUPLICATE | TOKEN_QUERY,
            &mut current,
        )
    } == 0
    {
        return Err(CommandError::Failed);
    }
    let current = OwnedHandle::new(current)?;
    let mut restricted = null_mut();
    if unsafe {
        CreateRestrictedToken(
            current.raw(),
            DISABLE_MAX_PRIVILEGE,
            0,
            null(),
            0,
            null(),
            0,
            null(),
            &mut restricted,
        )
    } == 0
    {
        return Err(CommandError::Failed);
    }
    OwnedHandle::new(restricted)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenDefaultOwnerPolicy {
    ProvisioningOwnerApplied,
    LocalSystemOwnerApplied,
}

fn apply_token_default_owner_policy(
    token: &OwnedHandle,
    provisioning_owner_sid: &str,
) -> Result<TokenDefaultOwnerPolicy, CommandError> {
    let owner = StringSid::parse_canonical(provisioning_owner_sid)?;
    let system = system_sid().map_err(|_| CommandError::Failed)?;
    let token_user_is_owner = token_information_sid_matches(token.raw(), TokenUser, owner.raw())?;
    let token_user_is_system = token_information_sid_matches(token.raw(), TokenUser, system.raw())?;
    let owner_is_system = unsafe { EqualSid(owner.raw(), system.raw()) } != 0;
    let policy = select_token_default_owner_policy(
        token_user_is_owner,
        token_user_is_system,
        owner_is_system,
    )?;
    let selected_owner = match policy {
        TokenDefaultOwnerPolicy::ProvisioningOwnerApplied => owner.raw(),
        TokenDefaultOwnerPolicy::LocalSystemOwnerApplied => system.raw(),
    };
    let token_owner = TOKEN_OWNER {
        Owner: selected_owner,
    };
    if unsafe {
        SetTokenInformation(
            token.raw(),
            TokenOwner,
            (&token_owner as *const TOKEN_OWNER).cast(),
            size_of::<TOKEN_OWNER>() as u32,
        )
    } == 0
    {
        return Err(CommandError::Failed);
    }
    if !token_information_sid_matches(token.raw(), TokenOwner, selected_owner)? {
        return Err(CommandError::Failed);
    }
    Ok(policy)
}

fn select_token_default_owner_policy(
    token_user_is_owner: bool,
    token_user_is_system: bool,
    provisioning_owner_is_system: bool,
) -> Result<TokenDefaultOwnerPolicy, CommandError> {
    if provisioning_owner_is_system || (token_user_is_owner && token_user_is_system) {
        return Err(CommandError::Failed);
    }
    if token_user_is_owner {
        Ok(TokenDefaultOwnerPolicy::ProvisioningOwnerApplied)
    } else if token_user_is_system {
        Ok(TokenDefaultOwnerPolicy::LocalSystemOwnerApplied)
    } else {
        Err(CommandError::Failed)
    }
}

fn token_information_sid_matches(
    token: HANDLE,
    information_class: windows_sys::Win32::Security::TOKEN_INFORMATION_CLASS,
    expected: PSID,
) -> Result<bool, CommandError> {
    let minimum_bytes = match information_class {
        TokenUser => size_of::<TOKEN_USER>() as u32,
        TokenOwner => size_of::<TOKEN_OWNER>() as u32,
        _ => return Err(CommandError::Failed),
    };
    let mut bytes = 0;
    unsafe { GetTokenInformation(token, information_class, null_mut(), 0, &mut bytes) };
    if bytes < minimum_bytes {
        return Err(CommandError::Failed);
    }
    let mut buffer = vec![0usize; (bytes as usize).div_ceil(size_of::<usize>())];
    if unsafe {
        GetTokenInformation(
            token,
            information_class,
            buffer.as_mut_ptr().cast(),
            bytes,
            &mut bytes,
        )
    } == 0
    {
        return Err(CommandError::Failed);
    }
    let observed = match information_class {
        TokenUser => unsafe { &*(buffer.as_ptr().cast::<TOKEN_USER>()) }.User.Sid,
        TokenOwner => unsafe { &*(buffer.as_ptr().cast::<TOKEN_OWNER>()) }.Owner,
        _ => unreachable!("information class was validated before querying the token"),
    };
    if observed.is_null() || expected.is_null() {
        return Err(CommandError::Failed);
    }
    Ok(unsafe { EqualSid(observed, expected) } != 0)
}

fn raw_pipe() -> Result<(OwnedHandle, OwnedHandle), CommandError> {
    let attributes = windows_sys::Win32::Security::SECURITY_ATTRIBUTES {
        nLength: size_of::<windows_sys::Win32::Security::SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: null_mut(),
        bInheritHandle: 1,
    };
    let mut read = null_mut();
    let mut write = null_mut();
    if unsafe { CreatePipe(&mut read, &mut write, &attributes, 0) } == 0 {
        return Err(CommandError::Failed);
    }
    Ok((OwnedHandle::new(read)?, OwnedHandle::new(write)?))
}

fn stdin_pipe() -> Result<(OwnedHandle, OwnedHandle), CommandError> {
    let (child_read, parent_write) = raw_pipe()?;
    if unsafe {
        windows_sys::Win32::Foundation::SetHandleInformation(
            parent_write.raw(),
            HANDLE_FLAG_INHERIT,
            0,
        )
    } == 0
    {
        return Err(CommandError::Failed);
    }
    Ok((child_read, parent_write))
}

fn stdout_pipe() -> Result<(OwnedHandle, OwnedHandle), CommandError> {
    let (parent_read, child_write) = raw_pipe()?;
    if unsafe {
        windows_sys::Win32::Foundation::SetHandleInformation(
            parent_read.raw(),
            HANDLE_FLAG_INHERIT,
            0,
        )
    } == 0
    {
        return Err(CommandError::Failed);
    }
    Ok((parent_read, child_write))
}

fn command_line(executable: &Path, args: &[&str]) -> Result<Vec<u16>, CommandError> {
    let mut values = Vec::with_capacity(args.len() + 1);
    values.push(executable.as_os_str().to_string_lossy().into_owned());
    values.extend(args.iter().map(|value| (*value).to_string()));
    let mut output = String::new();
    for (index, value) in values.iter().enumerate() {
        if value.contains('\0') {
            return Err(CommandError::Failed);
        }
        if index != 0 {
            output.push(' ');
        }
        output.push_str(&quote_argument(value));
    }
    Ok(wide(OsStr::new(&output)))
}

fn quote_argument(value: &str) -> String {
    let mut output = String::from("\"");
    let mut backslashes = 0;
    for character in value.chars() {
        match character {
            '\\' => backslashes += 1,
            '"' => {
                output.push_str(&"\\".repeat(backslashes * 2 + 1));
                output.push('"');
                backslashes = 0;
            }
            _ => {
                output.push_str(&"\\".repeat(backslashes));
                backslashes = 0;
                output.push(character);
            }
        }
    }
    output.push_str(&"\\".repeat(backslashes * 2));
    output.push('"');
    output
}

fn environment_block(
    token: HANDLE,
    supplied: &[(&OsStr, &OsStr)],
) -> Result<Vec<u16>, CommandError> {
    let mut inherited = copy_user_environment(token)?;
    let local_app_data = extract_local_app_data(&inherited);
    inherited.zeroize();
    let local_app_data = local_app_data?;
    build_environment_block(&local_app_data, &system_root()?, supplied)
}

fn copy_user_environment(token: HANDLE) -> Result<Vec<u16>, CommandError> {
    let mut raw = null_mut();
    if unsafe { CreateEnvironmentBlock(&mut raw, token, 0) } == 0 || raw.is_null() {
        return Err(CommandError::Failed);
    }
    let environment = UserEnvironmentBlock(raw);
    let source = raw.cast::<u16>();
    let mut copied = Vec::with_capacity(256);
    let mut previous_was_nul = false;
    for index in 0..MAX_USER_ENVIRONMENT_UNITS {
        let unit = unsafe { *source.add(index) };
        copied.push(unit);
        if unit == 0 && previous_was_nul {
            environment.destroy()?;
            return Ok(copied);
        }
        previous_was_nul = unit == 0;
    }
    Err(CommandError::Failed)
}

fn extract_local_app_data(block: &[u16]) -> Result<Vec<u16>, CommandError> {
    if block.len() < 2 || block.len() > MAX_USER_ENVIRONMENT_UNITS || !block.ends_with(&[0, 0]) {
        return Err(CommandError::Failed);
    }
    let mut cursor = 0;
    let mut entries = 0;
    let mut local_app_data = None;
    while cursor < block.len() - 1 {
        let relative_end = block[cursor..]
            .iter()
            .position(|unit| *unit == 0)
            .ok_or(CommandError::Failed)?;
        if relative_end == 0 {
            return Err(CommandError::Failed);
        }
        let entry = &block[cursor..cursor + relative_end];
        entries += 1;
        if entries > MAX_USER_ENVIRONMENT_ENTRIES {
            return Err(CommandError::Failed);
        }
        let separator = if entry[0] == b'=' as u16 {
            let separator = entry[1..]
                .iter()
                .position(|unit| *unit == b'=' as u16)
                .map(|index| index + 1)
                .ok_or(CommandError::Failed)?;
            let hidden_name = &entry[..separator];
            let valid_hidden_name = hidden_name.len() == 3
                && hidden_name[0] == b'=' as u16
                && u8::try_from(hidden_name[1]).is_ok_and(|unit| unit.is_ascii_alphabetic())
                && hidden_name[2] == b':' as u16;
            if !valid_hidden_name {
                return Err(CommandError::Failed);
            }
            separator
        } else {
            entry
                .iter()
                .position(|unit| *unit == b'=' as u16)
                .ok_or(CommandError::Failed)?
        };
        if separator == 0 || separator + 1 >= entry.len() {
            return Err(CommandError::Failed);
        }
        let name = String::from_utf16(&entry[..separator]).map_err(|_| CommandError::Failed)?;
        let value = &entry[separator + 1..];
        String::from_utf16(value).map_err(|_| CommandError::Failed)?;
        if name.eq_ignore_ascii_case("LOCALAPPDATA")
            && local_app_data.replace(value.to_vec()).is_some()
        {
            return Err(CommandError::Failed);
        }
        cursor += relative_end + 1;
    }
    local_app_data.ok_or(CommandError::Failed)
}

fn system_root() -> Result<Vec<u16>, CommandError> {
    let mut buffer = vec![0; 32_768];
    let length = unsafe { GetWindowsDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    if length == 0 || length >= buffer.len() {
        return Err(CommandError::Failed);
    }
    buffer.truncate(length);
    if buffer.contains(&0) {
        return Err(CommandError::Failed);
    }
    Ok(buffer)
}

fn build_environment_block(
    local_app_data: &[u16],
    system_root: &[u16],
    supplied: &[(&OsStr, &OsStr)],
) -> Result<Vec<u16>, CommandError> {
    if local_app_data.is_empty()
        || local_app_data.contains(&0)
        || system_root.is_empty()
        || system_root.contains(&0)
    {
        return Err(CommandError::Failed);
    }
    let mut entries = vec![("SystemRoot", system_root.to_vec())];
    match supplied {
        [] => entries.push(("LOCALAPPDATA", local_app_data.to_vec())),
        [(name, value)] if *name == OsStr::new("GH_CONFIG_DIR") => {
            entries.push(("LOCALAPPDATA", local_app_data.to_vec()));
            entries.push(("GH_CONFIG_DIR", environment_value(value)?));
        }
        [codex_home, local_app_data, temporary, temporary_alias]
            if codex_home.0 == OsStr::new("CODEX_HOME")
                && local_app_data.0 == OsStr::new("LOCALAPPDATA")
                && temporary.0 == OsStr::new("TEMP")
                && temporary_alias.0 == OsStr::new("TMP")
                && temporary.1 == temporary_alias.1 =>
        {
            entries.push(("CODEX_HOME", environment_value(codex_home.1)?));
            entries.push(("LOCALAPPDATA", environment_value(local_app_data.1)?));
            entries.push(("TEMP", environment_value(temporary.1)?));
            entries.push(("TMP", environment_value(temporary_alias.1)?));
        }
        _ => return Err(CommandError::Failed),
    }
    entries.sort_by(|left, right| {
        left.0
            .to_ascii_uppercase()
            .cmp(&right.0.to_ascii_uppercase())
    });
    let mut block = Vec::new();
    for (name, value) in entries {
        block.extend(name.encode_utf16());
        block.push(b'=' as u16);
        block.extend(value);
        block.push(0);
    }
    block.push(0);
    if block.len() > MAX_USER_ENVIRONMENT_UNITS {
        return Err(CommandError::Failed);
    }
    Ok(block)
}

fn environment_value(value: &OsStr) -> Result<Vec<u16>, CommandError> {
    let value: Vec<u16> = value.encode_wide().collect();
    if value.is_empty() || value.contains(&0) {
        return Err(CommandError::Failed);
    }
    Ok(value)
}

struct UserEnvironmentBlock(*mut c_void);

impl UserEnvironmentBlock {
    fn destroy(mut self) -> Result<(), CommandError> {
        if unsafe { DestroyEnvironmentBlock(self.0) } == 0 {
            return Err(CommandError::Failed);
        }
        self.0 = null_mut();
        Ok(())
    }
}

impl Drop for UserEnvironmentBlock {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                DestroyEnvironmentBlock(self.0);
            }
        }
    }
}

fn wide(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}

struct OwnedHandle(HANDLE);

struct SuspendedProcessGuard {
    process: OwnedHandle,
    thread: OwnedHandle,
    job_assigned: bool,
}

impl SuspendedProcessGuard {
    fn new(information: PROCESS_INFORMATION) -> Result<Self, CommandError> {
        let process_valid = valid_handle(information.hProcess);
        let thread_valid = valid_handle(information.hThread);
        if !process_valid || !thread_valid {
            if process_valid {
                unsafe {
                    TerminateProcess(information.hProcess, TERMINATED_EXIT_CODE);
                    WaitForSingleObject(information.hProcess, 5_000);
                }
            }
            if process_valid {
                unsafe { CloseHandle(information.hProcess) };
            }
            if thread_valid {
                unsafe { CloseHandle(information.hThread) };
            }
            return Err(CommandError::Failed);
        }
        let process = OwnedHandle(information.hProcess);
        let thread = OwnedHandle(information.hThread);
        Ok(Self {
            process,
            thread,
            job_assigned: false,
        })
    }

    fn process(&self) -> HANDLE {
        self.process.raw()
    }

    fn thread(&self) -> HANDLE {
        self.thread.raw()
    }

    fn mark_job_assigned(&mut self) {
        self.job_assigned = true;
    }
}

impl Drop for SuspendedProcessGuard {
    fn drop(&mut self) {
        if !self.job_assigned {
            unsafe {
                TerminateProcess(self.process.raw(), TERMINATED_EXIT_CODE);
                WaitForSingleObject(self.process.raw(), 5_000);
            }
        }
    }
}

fn valid_handle(handle: HANDLE) -> bool {
    !handle.is_null() && handle != INVALID_HANDLE_VALUE
}

impl OwnedHandle {
    fn new(handle: HANDLE) -> Result<Self, CommandError> {
        if !valid_handle(handle) {
            Err(CommandError::Failed)
        } else {
            Ok(Self(handle))
        }
    }
    fn raw(&self) -> HANDLE {
        self.0
    }
    fn into_raw(mut self) -> HANDLE {
        let raw = self.0;
        self.0 = null_mut();
        raw
    }
}

unsafe impl Send for OwnedHandle {}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            unsafe { CloseHandle(self.0) };
        }
    }
}

struct AttributeList {
    storage: Vec<usize>,
    ptr: windows_sys::Win32::System::Threading::LPPROC_THREAD_ATTRIBUTE_LIST,
}

impl AttributeList {
    fn new(count: u32) -> Result<Self, CommandError> {
        let mut bytes: usize = 0;
        unsafe { InitializeProcThreadAttributeList(null_mut(), count, 0, &mut bytes) };
        if bytes == 0 || unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
            return Err(CommandError::Failed);
        }
        let mut storage = vec![0usize; bytes.div_ceil(size_of::<usize>())];
        let ptr = storage.as_mut_ptr().cast();
        if unsafe { InitializeProcThreadAttributeList(ptr, count, 0, &mut bytes) } == 0 {
            return Err(CommandError::Failed);
        }
        Ok(Self { storage, ptr })
    }

    fn update(
        &mut self,
        attribute: usize,
        value: *const c_void,
        bytes: usize,
    ) -> Result<(), CommandError> {
        if unsafe {
            UpdateProcThreadAttribute(self.ptr, 0, attribute, value, bytes, null_mut(), null())
        } == 0
        {
            return Err(CommandError::Failed);
        }
        Ok(())
    }
}

impl Drop for AttributeList {
    fn drop(&mut self) {
        unsafe { DeleteProcThreadAttributeList(self.ptr) };
        let _ = self.storage.len();
    }
}

struct ProfileSid(PSID);

impl ProfileSid {
    fn derive(name: &str) -> Result<Self, ()> {
        if name.is_empty() || name.len() > MAX_PROFILE_NAME_BYTES || !name.is_ascii() {
            return Err(());
        }
        let name = wide(OsStr::new(name));
        let mut sid = null_mut();
        if unsafe { DeriveAppContainerSidFromAppContainerName(name.as_ptr(), &mut sid) } < 0
            || sid.is_null()
        {
            return Err(());
        }
        Ok(Self(sid))
    }

    fn derive_and_match(name: &str, expected: &str) -> Result<Self, ()> {
        let derived = Self::derive(name)?;
        let expected = StringSid::parse(expected).map_err(|_| ())?;
        if unsafe { EqualSid(derived.raw(), expected.raw()) } == 0 {
            return Err(());
        }
        drop(derived);

        // AppContainer profile registration is scoped to the calling Windows identity.
        // Provisioning registers the fixed profiles for the owner, while the durable master
        // normally runs as LocalSystem. Register the already-validated fixed profile in the
        // current identity namespace before launching the contained process.
        let name = wide(OsStr::new(name));
        let mut sid = null_mut();
        let created = unsafe {
            CreateAppContainerProfile(
                name.as_ptr(),
                name.as_ptr(),
                name.as_ptr(),
                null(),
                0,
                &mut sid,
            )
        };
        if created == HRESULT_PROFILE_ALREADY_EXISTS {
            if unsafe { DeriveAppContainerSidFromAppContainerName(name.as_ptr(), &mut sid) } < 0 {
                return Err(());
            }
        } else if created < 0 {
            return Err(());
        }
        if sid.is_null() {
            return Err(());
        }
        let registered = Self(sid);
        if unsafe { EqualSid(registered.raw(), expected.raw()) } == 0 {
            return Err(());
        }
        Ok(registered)
    }

    fn raw(&self) -> PSID {
        self.0
    }
}

impl Drop for ProfileSid {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { FreeSid(self.0) };
        }
    }
}

struct StringSid(PSID);

impl StringSid {
    fn parse(value: &str) -> Result<Self, CommandError> {
        let value = wide(OsStr::new(value));
        let mut sid = null_mut();
        if unsafe { ConvertStringSidToSidW(value.as_ptr(), &mut sid) } == 0 || sid.is_null() {
            return Err(CommandError::Failed);
        }
        Ok(Self(sid))
    }

    fn parse_canonical(value: &str) -> Result<Self, CommandError> {
        if value.is_empty() || !value.is_ascii() {
            return Err(CommandError::Failed);
        }
        let sid = Self::parse(value)?;
        let mut canonical = null_mut();
        if unsafe { ConvertSidToStringSidW(sid.raw(), &mut canonical) } == 0 || canonical.is_null()
        {
            return Err(CommandError::Failed);
        }
        let length = (0..256)
            .find(|index| unsafe { *canonical.add(*index) } == 0)
            .ok_or(CommandError::Failed);
        let observed = length.and_then(|length| {
            String::from_utf16(unsafe { std::slice::from_raw_parts(canonical, length) })
                .map_err(|_| CommandError::Failed)
        });
        unsafe { LocalFree(canonical.cast()) };
        if observed? != value {
            return Err(CommandError::Failed);
        }
        Ok(sid)
    }

    fn raw(&self) -> PSID {
        self.0
    }
}

impl Drop for StringSid {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { LocalFree(self.0) };
        }
    }
}

struct BufferedSid(Vec<usize>);

impl BufferedSid {
    fn raw(&self) -> PSID {
        self.0.as_ptr().cast_mut().cast()
    }
}

fn system_sid() -> Result<BufferedSid, ()> {
    let mut sid = vec![0usize; (SECURITY_MAX_SID_SIZE as usize).div_ceil(size_of::<usize>())];
    let mut bytes = SECURITY_MAX_SID_SIZE;
    if unsafe {
        CreateWellKnownSid(
            WinLocalSystemSid,
            null_mut(),
            sid.as_mut_ptr().cast(),
            &mut bytes,
        )
    } == 0
    {
        return Err(());
    }
    Ok(BufferedSid(sid))
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::Foundation::{
        DuplicateHandle, GetHandleInformation, DUPLICATE_SAME_ACCESS,
    };
    use windows_sys::Win32::Storage::FileSystem::{WRITE_DAC, WRITE_OWNER};
    use windows_sys::Win32::System::Threading::{CreateProcessW, STARTUPINFOW};

    fn inherit_flag(handle: HANDLE) -> u32 {
        let mut flags = 0;
        assert_ne!(unsafe { GetHandleInformation(handle, &mut flags) }, 0);
        flags & HANDLE_FLAG_INHERIT
    }

    fn environment_names(block: &[u16]) -> Vec<String> {
        block
            .split(|unit| *unit == 0)
            .take_while(|value| !value.is_empty())
            .map(|value| String::from_utf16(value).unwrap())
            .map(|value| value.split_once('=').unwrap().0.to_string())
            .collect()
    }

    fn test_environment(entries: &[&str]) -> Vec<u16> {
        let mut block = Vec::new();
        for entry in entries {
            block.extend(entry.encode_utf16());
            block.push(0);
        }
        block.push(0);
        block
    }

    fn sid_text(sid: PSID) -> String {
        let mut text = null_mut();
        assert_ne!(unsafe { ConvertSidToStringSidW(sid, &mut text) }, 0);
        assert!(!text.is_null());
        let length = (0..256)
            .find(|index| unsafe { *text.add(*index) } == 0)
            .unwrap();
        let value =
            String::from_utf16(unsafe { std::slice::from_raw_parts(text, length) }).unwrap();
        unsafe { LocalFree(text.cast()) };
        value
    }

    struct TestAppContainerProfile(Vec<u16>);

    impl Drop for TestAppContainerProfile {
        fn drop(&mut self) {
            assert!(unsafe { DeleteAppContainerProfile(self.0.as_ptr()) } >= 0);
        }
    }

    #[test]
    fn profile_registration_is_idempotent_for_the_current_windows_identity() {
        let name = format!("Assemblywright.Test.{}", Uuid::new_v4().simple());
        let expected = sid_text(ProfileSid::derive(&name).unwrap().raw());
        let cleanup = TestAppContainerProfile(wide(OsStr::new(&name)));

        let registered = ProfileSid::derive_and_match(&name, &expected).unwrap();
        assert_eq!(sid_text(registered.raw()), expected);
        drop(registered);

        let existing = ProfileSid::derive_and_match(&name, &expected).unwrap();
        assert_eq!(sid_text(existing.raw()), expected);
        assert!(ProfileSid::derive_and_match(&name, "S-1-15-2-1").is_err());
        drop(existing);
        drop(cleanup);
    }

    #[test]
    fn explicit_pipe_contract_inherits_only_child_ends() {
        let (stdin_child, stdin_parent) = stdin_pipe().unwrap();
        let (stdout_parent, stdout_child) = stdout_pipe().unwrap();
        assert_eq!(inherit_flag(stdin_child.raw()), HANDLE_FLAG_INHERIT);
        assert_eq!(inherit_flag(stdin_parent.raw()), 0);
        assert_eq!(inherit_flag(stdout_child.raw()), HANDLE_FLAG_INHERIT);
        assert_eq!(inherit_flag(stdout_parent.raw()), 0);
    }

    #[test]
    fn private_gui_object_descriptors_are_protected_profile_specific_and_parseable() {
        let owner = "S-1-5-21-1-2-3-1001";
        let profile = "S-1-15-2-1";
        let station = private_object_sddl(
            owner,
            profile,
            WINDOW_STATION_OWNER_ACCESS,
            WINDOW_STATION_PROFILE_ACCESS,
        )
        .unwrap();
        let desktop =
            private_object_sddl(owner, profile, DESKTOP_OWNER_ACCESS, DESKTOP_PROFILE_ACCESS)
                .unwrap();
        assert_eq!(
            station,
            "D:P(A;;0x000f037f;;;S-1-5-18)(A;;0x000f037f;;;S-1-5-21-1-2-3-1001)(A;;0x000f006e;;;S-1-15-2-1)"
        );
        assert_eq!(
            desktop,
            "D:P(A;;0x000f01ff;;;S-1-5-18)(A;;0x000f01ff;;;S-1-5-21-1-2-3-1001)(A;;0x000f00cf;;;S-1-15-2-1)"
        );
        assert!(LocalSecurityDescriptor::from_sddl(&station).is_ok());
        assert!(LocalSecurityDescriptor::from_sddl(&desktop).is_ok());
        for forbidden in [
            [";;;", "WD)"].concat(),
            [";;;", "BA)"].concat(),
            [";;;", "AC)"].concat(),
            ["S-1-15-2-", "2"].concat(),
        ] {
            assert!(!station.contains(&forbidden));
            assert!(!desktop.contains(&forbidden));
        }
        assert!(private_object_sddl(
            "S-1-5-18",
            profile,
            WINDOW_STATION_OWNER_ACCESS,
            WINDOW_STATION_PROFILE_ACCESS,
        )
        .is_err());
        assert!(private_object_sddl(
            profile,
            profile,
            WINDOW_STATION_OWNER_ACCESS,
            WINDOW_STATION_PROFILE_ACCESS,
        )
        .is_err());
    }

    #[test]
    fn private_station_is_restored_before_suspended_process_creation() {
        let source = include_str!("windows_containment.rs");
        for required in [
            "PRIVATE_WINDOW_STATION_CREATE_ONLY: u32 = 1",
            "bInheritHandle: 0",
            "association.restore()?;",
            "startup.StartupInfo.lpDesktop = private_desktop.startup_name.as_ptr().cast_mut();",
            "let private_desktop = PrivateDesktop::create(profile)?;",
            "std::process::abort();",
        ] {
            assert!(source.contains(required));
        }
        let restore = source.find("association.restore()?;").unwrap();
        let create_process = source.find("CreateProcessAsUserW(").unwrap();
        assert!(restore < create_process);
        let interactive_station = ["Win", "sta0"].concat();
        assert!(!source.contains(&interactive_station));
        let inherited = source
            .find("let mut inherited = [stdin_read.raw(), stdout_write.raw(), stderr_write.raw()];")
            .unwrap();
        let attributes = source
            .find("PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize")
            .unwrap();
        assert!(inherited < attributes);
        assert!(source.contains("startup.StartupInfo.hStdError = stderr_write.raw();"));
    }

    #[test]
    #[ignore = "requires elevated native Windows staging"]
    fn private_window_station_native_lifecycle_restores_original_association() {
        let profile_sid = ProfileSid::derive(PROVIDER_PROFILE).unwrap();
        let original = unsafe { GetProcessWindowStation() };
        assert!(!original.is_null());
        let binding = ProfileBinding {
            name: PROVIDER_PROFILE.to_string(),
            sid: sid_text(profile_sid.raw()),
            provisioning_owner_sid: "S-1-5-32-544".to_string(),
            scope_root: PathBuf::new(),
            peer_root: PathBuf::new(),
            planning_root: PathBuf::new(),
            master_check_root: PathBuf::new(),
            data_root: PathBuf::new(),
            locator_root: PathBuf::new(),
            runtime_ancestors: Vec::new(),
            protected_paths: Vec::new(),
            scope_paths: Vec::new(),
        };

        let private = PrivateDesktop::create(&binding).unwrap();
        assert_eq!(unsafe { GetProcessWindowStation() }, original);
        let name = String::from_utf16(
            private
                .startup_name
                .strip_suffix(&[0])
                .expect("startup desktop name is NUL terminated"),
        )
        .unwrap();
        assert!(name.starts_with("AssemblywrightPlanning-"));
        assert!(name.ends_with("\\Planning"));
        drop(private);
        assert_eq!(unsafe { GetProcessWindowStation() }, original);
    }

    #[test]
    fn nonzero_exit_precedes_empty_or_oversized_output_classification() {
        assert!(matches!(
            classify_completed_output(10, Err(CommandError::Malformed)),
            Err(CommandError::Exited(10))
        ));
        assert!(matches!(
            classify_completed_output(10, Err(CommandError::Failed)),
            Err(CommandError::Exited(10))
        ));
        assert!(matches!(
            classify_completed_output(0, Err(CommandError::Malformed)),
            Err(CommandError::Malformed)
        ));
        assert!(matches!(
            bounded_reader(std::io::Cursor::new(Vec::new()), 1, false)
                .join()
                .unwrap(),
            Err(CommandError::Malformed)
        ));
        assert!(matches!(
            bounded_reader(std::io::Cursor::new(vec![1, 2]), 1, false)
                .join()
                .unwrap(),
            Err(CommandError::Malformed)
        ));
    }

    #[test]
    fn stderr_discarder_drains_without_retaining_content_and_fails_on_read_error() {
        assert!(discard_reader(std::io::Cursor::new(Vec::<u8>::new()))
            .join()
            .unwrap()
            .is_ok());
        assert!(discard_reader(std::io::Cursor::new(vec![b'x'; 32 * 1024]))
            .join()
            .unwrap()
            .is_ok());

        struct FailingReader;
        impl std::io::Read for FailingReader {
            fn read(&mut self, _buffer: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::other("fixture read failure"))
            }
        }
        assert!(matches!(
            discard_reader(FailingReader).join().unwrap(),
            Err(CommandError::Failed)
        ));
    }

    #[test]
    fn diagnostic_stderr_capture_retains_only_the_fixed_prefix_and_drains_the_rest() {
        let output = Arc::new(Mutex::new(Vec::new()));
        let mode = CommandStderrMode::CaptureBounded {
            max_bytes: 4,
            output: Arc::clone(&output),
        };
        assert!(stderr_reader(std::io::Cursor::new(b"abcdefgh"), &mode)
            .join()
            .is_ok());
        let mut captured = output.lock().unwrap();
        assert_eq!(captured.as_slice(), b"abcd");
        captured.zeroize();
        captured.clear();
    }

    #[test]
    fn signaled_completion_observes_exit_before_cleanup_and_output_join() {
        let source = include_str!("windows_containment.rs");
        let completion = &source[source.find("fn complete_signaled_process(").unwrap()
            ..source.find("fn open_image_guard(").unwrap()];
        let exit = completion.find("GetExitCodeProcess").unwrap();
        let terminate = completion.find("TerminateJobObject").unwrap();
        let close = completion.find("drop(job)").unwrap();
        let reap = completion.find("WaitForSingleObject").unwrap();
        let join = completion.find(".join()").unwrap();
        let classify = completion.find("classify_completed_output").unwrap();
        assert!(exit < terminate);
        assert!(terminate < close);
        assert!(close < reap);
        assert!(reap < join);
        assert!(join < classify);
    }

    #[test]
    fn common_parent_profile_acl_is_exactly_traverse_only() {
        assert!(valid_shared_traverse_ace(FILE_TRAVERSE, 0));
        for mask in [
            FILE_TRAVERSE | SYNCHRONIZE,
            FILE_TRAVERSE | FILE_GENERIC_READ,
            FILE_TRAVERSE | FILE_GENERIC_WRITE,
        ] {
            assert!(!valid_shared_traverse_ace(mask, 0));
        }
        for flags in [INHERITED_ACE as u8, OBJECT_INHERIT_ACE as u8] {
            assert!(!valid_shared_traverse_ace(FILE_TRAVERSE, flags));
        }
        assert!(valid_runtime_instance("AssemblywrightMaster"));
        assert!(!valid_runtime_instance(""));
        assert!(!valid_runtime_instance("unsafe\\path"));
        assert!(!valid_runtime_instance("private data"));
    }

    #[test]
    fn provisioning_owner_is_canonical_distinct_and_exact() {
        let owner = StringSid::parse_canonical("S-1-5-21-1-2-3-1001").unwrap();
        let wrong = StringSid::parse_canonical("S-1-5-21-1-2-3-1002").unwrap();
        let system = system_sid().unwrap();
        assert!(exact_owner_matches(
            AclScope::Master,
            owner.raw(),
            owner.raw(),
            system.raw()
        ));
        assert!(!exact_owner_matches(
            AclScope::Master,
            wrong.raw(),
            owner.raw(),
            system.raw()
        ));
        assert!(!exact_owner_matches(
            AclScope::Master,
            system.raw(),
            system.raw(),
            system.raw()
        ));
        assert!(exact_owner_matches(
            AclScope::ProfileWriteChild(true, true),
            system.raw(),
            owner.raw(),
            system.raw()
        ));
        assert!(exact_owner_matches(
            AclScope::ProfileWriteChild(false, true),
            system.raw(),
            owner.raw(),
            system.raw()
        ));
        assert!(!exact_owner_matches(
            AclScope::ProfileWriteChild(true, false),
            system.raw(),
            owner.raw(),
            system.raw()
        ));
        for scope in [
            AclScope::Master,
            AclScope::TraverseOnly,
            AclScope::ProfileRead,
            AclScope::ProfileWriteRoot,
            AclScope::ProfileWriteFile,
        ] {
            assert!(!exact_owner_matches(
                scope,
                system.raw(),
                owner.raw(),
                system.raw()
            ));
        }
        assert!(!exact_owner_matches(
            AclScope::ProfileWriteChild(true, true),
            wrong.raw(),
            owner.raw(),
            system.raw()
        ));
        assert!(StringSid::parse_canonical("s-1-5-21-1-2-3-1001").is_err());
        assert!(StringSid::parse_canonical("S-1-5-21-01-2-3-1001").is_err());
    }

    #[test]
    fn acl_principal_contract_rejects_duplicate_missing_and_foreign_authorities() {
        let owner = StringSid::parse_canonical("S-1-5-21-1-2-3-1001").unwrap();
        let profile = StringSid::parse_canonical("S-1-5-21-1-2-3-1002").unwrap();
        let peer = StringSid::parse_canonical("S-1-5-21-1-2-3-1003").unwrap();
        let foreign = StringSid::parse_canonical("S-1-5-21-1-2-3-1004").unwrap();
        let system = system_sid().unwrap();
        assert!(matches!(
            classify_principal(
                owner.raw(),
                owner.raw(),
                system.raw(),
                Some(profile.raw()),
                Some(peer.raw())
            ),
            Some(AclPrincipal::Owner)
        ));
        assert!(classify_principal(
            foreign.raw(),
            owner.raw(),
            system.raw(),
            Some(profile.raw()),
            Some(peer.raw())
        )
        .is_none());
        let mut duplicate = false;
        assert!(claim_once(&mut duplicate).is_ok());
        assert!(claim_once(&mut duplicate).is_err());
        assert!(validate_principal_presence(
            AclScope::Master,
            PrincipalPresence {
                owner: true,
                system: true,
                ..PrincipalPresence::default()
            }
        )
        .is_ok());
        assert!(validate_principal_presence(
            AclScope::Master,
            PrincipalPresence {
                owner: true,
                ..PrincipalPresence::default()
            }
        )
        .is_err());
        assert!(validate_principal_presence(
            AclScope::TraverseOnly,
            PrincipalPresence {
                owner: true,
                system: true,
                profile: true,
                peer: true,
            }
        )
        .is_ok());
    }

    #[test]
    fn master_config_v4_binds_owner_and_program_data_instance() {
        assert!(super::super::valid_config_schema(4));
        assert!(!super::super::valid_config_schema(3));
        let runtime = include_str!("../planning_runtime.rs");
        let provision = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/provision-planning-runtime.ps1"
        ));
        for required in [
            "provisioning_owner_sid: Option<String>",
            "runtime_instance: Option<String>",
            "schema_version == if cfg!(windows) { 4 } else { 1 }",
            "provisioning_owner_sid: config",
        ] {
            assert!(runtime.contains(required));
        }
        assert!(provision.contains("$providerConfig = [ordered]@{schema_version=1"));
        assert!(provision.contains("$masterConfig = [ordered]@{schema_version=4"));
        assert!(provision.contains("provisioning_owner_sid=$ownerSid.Value"));
        assert!(provision.contains("runtime_instance=$ServiceName"));
        assert!(!provision.contains("$masterConfig = [ordered]@{schema_version=3"));
    }

    #[test]
    fn profile_acl_masks_require_the_exact_read_or_modify_contract() {
        let read = FILE_GENERIC_READ | FILE_GENERIC_EXECUTE;
        let modify = FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_GENERIC_EXECUTE | DELETE;
        assert!(valid_profile_mask(AclScope::ProfileRead, read));
        assert!(!valid_profile_mask(AclScope::ProfileRead, modify));
        assert!(!valid_profile_mask(AclScope::ProfileRead, read | WRITE_DAC));
        assert!(valid_profile_mask(AclScope::ProfileWriteRoot, modify));
        assert!(!valid_profile_mask(AclScope::ProfileWriteRoot, read));
        assert!(!valid_profile_mask(AclScope::ProfileWriteRoot, SYNCHRONIZE));
        assert!(!valid_profile_mask(
            AclScope::ProfileWriteRoot,
            modify | WRITE_OWNER
        ));
    }

    #[test]
    fn explicit_writable_file_requires_protected_zero_flag_modify_aces() {
        let modify = FILE_GENERIC_READ | FILE_GENERIC_WRITE | FILE_GENERIC_EXECUTE | DELETE;
        let inheritance = OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE;
        assert!(valid_profile_mask(AclScope::ProfileWriteFile, modify));
        assert!(valid_write_ace_flags(AclScope::ProfileWriteFile, true, 0));
        assert!(!valid_write_ace_flags(AclScope::ProfileWriteFile, false, 0));
        for rejected in [inheritance, INHERITED_ACE, inheritance | INHERITED_ACE] {
            assert!(!valid_write_ace_flags(
                AclScope::ProfileWriteFile,
                true,
                rejected
            ));
        }
    }

    #[test]
    fn preexisting_writable_directory_preserves_exact_appcontainer_child_acl_contract() {
        let inheritance = OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE;
        assert!(valid_write_ace_flags(
            AclScope::ProfileWriteRoot,
            true,
            inheritance
        ));
        assert!(!valid_write_ace_flags(AclScope::ProfileWriteRoot, true, 0));
        assert!(valid_write_ace_flags(
            AclScope::ProfileWriteChild(false, false),
            false,
            INHERITED_ACE
        ));
        assert!(valid_write_ace_flags(
            AclScope::ProfileWriteChild(true, false),
            false,
            inheritance | INHERITED_ACE
        ));
        assert!(!valid_write_ace_flags(
            AclScope::ProfileWriteChild(false, false),
            false,
            0
        ));
        assert!(valid_write_ace_flags(
            AclScope::ProfileWriteChild(true, false),
            true,
            inheritance
        ));
        // Provisioning protects a pre-existing nested directory with OI|CI. A child directory or
        // file created by the AppContainer then receives only inherited exact ACEs; validating it
        // again after close/reopen must accept those states without normalization.
        let created_directory_flags = inheritance | INHERITED_ACE;
        let created_file_flags = INHERITED_ACE;
        assert!(valid_write_ace_flags(
            AclScope::ProfileWriteChild(true, false),
            false,
            created_directory_flags
        ));
        assert!(valid_write_ace_flags(
            AclScope::ProfileWriteChild(false, false),
            false,
            created_file_flags
        ));
        assert!(valid_write_ace_flags(
            AclScope::ProfileWriteChild(false, false),
            true,
            0
        ));
        for rejected in [
            INHERITED_ACE,
            inheritance | INHERITED_ACE,
            OBJECT_INHERIT_ACE,
        ] {
            assert!(!valid_write_ace_flags(
                AclScope::ProfileWriteChild(true, false),
                true,
                rejected
            ));
        }
        for rejected in [INHERITED_ACE, inheritance, OBJECT_INHERIT_ACE] {
            assert!(!valid_write_ace_flags(
                AclScope::ProfileWriteChild(false, false),
                true,
                rejected
            ));
        }
    }

    #[test]
    fn provider_integrity_state_tracks_bounded_resumable_migration() {
        let mut state = None;
        accumulate_integrity_state(&mut state, false);
        assert_eq!(state, Some(ProviderIntegrityState::Unlabeled));
        accumulate_integrity_state(&mut state, true);
        assert_eq!(state, Some(ProviderIntegrityState::Incomplete));
        accumulate_integrity_state(&mut state, false);
        assert_eq!(state, Some(ProviderIntegrityState::Incomplete));

        let mut state = None;
        accumulate_integrity_state(&mut state, true);
        assert_eq!(state, Some(ProviderIntegrityState::Exact));
        accumulate_integrity_state(&mut state, false);
        assert_eq!(state, Some(ProviderIntegrityState::Incomplete));
    }

    #[test]
    fn low_integrity_label_is_confined_to_the_writable_tree() {
        let root = tempfile::tempdir().unwrap();
        let immutable = root.path().join("immutable.exe");
        let writable = root.path().join("writable");
        let nested = writable.join("nested");
        let state = nested.join("state.json");
        fs::write(&immutable, b"immutable").unwrap();
        fs::create_dir(&writable).unwrap();
        fs::create_dir(&nested).unwrap();
        fs::write(&state, b"state").unwrap();

        let paths = vec![(immutable.clone(), false), (writable.clone(), true)];
        let writable_guard = IntegrityObject::open(&writable, true, false).unwrap();
        let renamed = root.path().join("renamed");
        if fs::rename(&writable, &renamed).is_ok() {
            assert!(writable_guard.revalidate_path().is_err());
            fs::rename(&renamed, &writable).unwrap();
        }
        writable_guard.revalidate_path().unwrap();
        drop(writable_guard);
        assert_eq!(
            provider_integrity_state(root.path(), &paths).unwrap(),
            ProviderIntegrityState::Unlabeled
        );
        let state_guard = IntegrityObject::open(&state, false, true).unwrap();
        install_low_integrity_label(&state_guard).unwrap();
        drop(state_guard);
        assert_eq!(
            provider_integrity_state(root.path(), &paths).unwrap(),
            ProviderIntegrityState::Incomplete
        );
        install_provider_low_integrity_labels(root.path(), &paths).unwrap();
        assert_eq!(
            provider_integrity_state(root.path(), &paths).unwrap(),
            ProviderIntegrityState::Exact
        );
        validate_provider_integrity_scope(root.path(), &paths).unwrap();
        let immutable_guard = IntegrityObject::open(&immutable, false, false).unwrap();
        validate_integrity_object(&immutable_guard, IntegrityLabelScope::Unlabeled).unwrap();
        immutable_guard.revalidate_path().unwrap();
        drop(immutable_guard);

        let immutable_guard = IntegrityObject::open(&immutable, false, true).unwrap();
        install_low_integrity_label(&immutable_guard).unwrap();
        assert!(validate_provider_integrity_scope(root.path(), &paths).is_err());
    }

    #[test]
    fn provider_environment_contract_is_exactly_local_app_data_and_system_root() {
        let environment = build_environment_block(
            &"C:\\Users\\owner\\AppData\\Local"
                .encode_utf16()
                .collect::<Vec<_>>(),
            &"C:\\Windows".encode_utf16().collect::<Vec<_>>(),
            &[],
        )
        .unwrap();
        assert_eq!(
            environment_names(&environment),
            ["LOCALAPPDATA", "SystemRoot"]
        );
    }

    #[test]
    fn github_environment_contract_adds_only_config_root() {
        let environment = build_environment_block(
            &"C:\\Users\\owner\\AppData\\Local"
                .encode_utf16()
                .collect::<Vec<_>>(),
            &"C:\\Windows".encode_utf16().collect::<Vec<_>>(),
            &[(
                OsStr::new("GH_CONFIG_DIR"),
                OsStr::new(r"C:\private\github"),
            )],
        )
        .unwrap();
        assert_eq!(
            environment_names(&environment),
            ["GH_CONFIG_DIR", "LOCALAPPDATA", "SystemRoot"]
        );
    }

    #[test]
    fn native_codex_probe_environment_is_exactly_five_private_variables() {
        let environment = build_environment_block(
            &"C:\\Users\\owner\\AppData\\Local"
                .encode_utf16()
                .collect::<Vec<_>>(),
            &"C:\\Windows".encode_utf16().collect::<Vec<_>>(),
            &[
                (OsStr::new("CODEX_HOME"), OsStr::new(r"C:\private\codex")),
                (OsStr::new("LOCALAPPDATA"), OsStr::new(r"C:\private\local")),
                (OsStr::new("TEMP"), OsStr::new(r"C:\private\temp")),
                (OsStr::new("TMP"), OsStr::new(r"C:\private\temp")),
            ],
        )
        .unwrap();
        assert_eq!(
            environment_names(&environment),
            ["CODEX_HOME", "LOCALAPPDATA", "SystemRoot", "TEMP", "TMP"]
        );
        let text = String::from_utf16_lossy(&environment);
        assert!(!text.contains(r"C:\Users\owner\AppData\Local"));
    }

    #[test]
    fn native_codex_probe_environment_rejects_alias_drift_and_extra_names() {
        let local = "C:\\ambient".encode_utf16().collect::<Vec<_>>();
        let system = "C:\\Windows".encode_utf16().collect::<Vec<_>>();
        assert!(build_environment_block(
            &local,
            &system,
            &[
                (OsStr::new("CODEX_HOME"), OsStr::new(r"C:\private\codex")),
                (OsStr::new("LOCALAPPDATA"), OsStr::new(r"C:\private\local"),),
                (OsStr::new("TEMP"), OsStr::new(r"C:\private\temp")),
                (OsStr::new("TMP"), OsStr::new(r"C:\other")),
            ]
        )
        .is_err());
        assert!(build_environment_block(
            &local,
            &system,
            &[(OsStr::new("PATH"), OsStr::new(r"C:\forbidden"))]
        )
        .is_err());
    }

    #[test]
    fn native_probe_service_command_accepts_only_exact_pinned_shape_and_loopback_bind() {
        let directory = tempfile::tempdir().unwrap();
        let executable = directory.path().join("assemblywright-master.exe");
        fs::write(&executable, b"pinned master").unwrap();
        let data = directory.path().join("master");
        fs::create_dir(&data).unwrap();
        let executable = fs::canonicalize(executable).unwrap();
        let data = fs::canonicalize(data).unwrap();
        let config = ServiceConfig {
            service_type: ServiceType::OWN_PROCESS,
            start_type: ServiceStartType::AutoStart,
            error_control: ServiceErrorControl::Normal,
            executable_path: PathBuf::from("raw command line is parsed separately"),
            load_order_group: None,
            tag_id: 0,
            dependencies: Vec::new(),
            account_name: Some(OsString::from(r"MIKE-PC\mike")),
            display_name: OsString::from("Assemblywright Developer Mode Master"),
        };
        let command = [
            executable.as_os_str(),
            OsStr::new("--data-dir"),
            data.as_os_str(),
            OsStr::new("service-run"),
            OsStr::new("--service-name"),
            OsStr::new("AssemblywrightMaster"),
            OsStr::new("--bind"),
            OsStr::new("127.0.0.1:7791"),
            OsStr::new("--service-identity"),
            OsStr::new(r"MIKE-PC\mike"),
        ]
        .into_iter()
        .map(OsString::from)
        .collect::<Vec<_>>();
        assert_eq!(
            validate_service_launch_command(
                "AssemblywrightMaster",
                &data,
                &executable,
                &config,
                &command,
            )
            .unwrap(),
            "127.0.0.1:7791".parse().unwrap()
        );

        let mut public_bind = command.clone();
        public_bind[7] = OsString::from("0.0.0.0:7791");
        assert!(validate_service_launch_command(
            "AssemblywrightMaster",
            &data,
            &executable,
            &config,
            &public_bind,
        )
        .is_err());
        let mut drifted = config.clone();
        drifted.start_type = ServiceStartType::OnDemand;
        assert!(!native_probe_authority_drifted(
            &config,
            &config,
            "S-1-5-21-1",
            "S-1-5-21-1",
        ));
        assert!(native_probe_authority_drifted(
            &config,
            &drifted,
            "S-1-5-21-1",
            "S-1-5-21-1",
        ));
        assert!(native_probe_authority_drifted(
            &config,
            &config,
            "S-1-5-21-1",
            "S-1-5-21-2",
        ));
        assert!(validate_service_launch_command(
            "AssemblywrightMaster",
            &data,
            &executable,
            &drifted,
            &command,
        )
        .is_err());
        let mut extra = command.clone();
        extra.push(OsString::from("--unexpected"));
        assert!(validate_service_launch_command(
            "AssemblywrightMaster",
            &data,
            &executable,
            &config,
            &extra,
        )
        .is_err());
    }

    #[test]
    fn native_probe_uses_windows_command_line_parser_for_quoted_service_paths() {
        let parsed = parse_service_launch_command(OsStr::new(
            r#""C:\Program Files\Assemblywright\assemblywright-master.exe" --data-dir "C:\ProgramData\Assemblywright Master" service-run"#,
        ))
        .unwrap();
        assert_eq!(
            parsed,
            [
                r"C:\Program Files\Assemblywright\assemblywright-master.exe",
                "--data-dir",
                r"C:\ProgramData\Assemblywright Master",
                "service-run",
            ]
            .into_iter()
            .map(OsString::from)
            .collect::<Vec<_>>()
        );
    }

    #[test]
    fn native_probe_open_rejections_are_fixed_content_free_stage_codes() {
        assert_eq!(
            [
                NativeProbeOpenRejection::ServiceConfiguration,
                NativeProbeOpenRejection::ExecutableCommandBinding,
                NativeProbeOpenRejection::AccountIdentity,
                NativeProbeOpenRejection::ProfileRevalidation,
                NativeProbeOpenRejection::StoppedState,
                NativeProbeOpenRejection::BindingDigest,
            ]
            .map(NativeProbeOpenRejection::code),
            [
                "native_probe_service_configuration",
                "native_probe_executable_command_binding",
                "native_probe_account_identity",
                "native_probe_profile_revalidation",
                "native_probe_stopped_state",
                "native_probe_binding_digest",
            ]
        );
    }

    #[test]
    fn scm_local_account_form_is_canonicalized_to_the_computer_name() {
        let account = OsStr::new(r".\mike").encode_wide().collect::<Vec<_>>();
        let computer = OsStr::new("MIKE-PC").encode_wide().collect::<Vec<_>>();
        assert_eq!(
            canonicalize_local_account_name(&account, &computer).unwrap(),
            OsString::from(r"MIKE-PC\mike")
        );
        assert_eq!(
            canonicalize_lookup_account_name(OsStr::new(r"MIKE-PC\mike")).unwrap(),
            OsString::from(r"MIKE-PC\mike")
        );
    }

    #[test]
    fn scm_malformed_relative_account_forms_are_rejected() {
        let computer = OsStr::new("MIKE-PC").encode_wide().collect::<Vec<_>>();
        for account in [
            r".\",
            r".\.",
            r".\..",
            r".\mike\admin",
            r".\mike.",
            r".\mi/ke",
        ] {
            let account = OsStr::new(account).encode_wide().collect::<Vec<_>>();
            assert!(canonicalize_local_account_name(&account, &computer).is_err());
        }
        for account in ["", ".", "./mike", r"..\mike"] {
            assert!(canonicalize_lookup_account_name(OsStr::new(account)).is_err());
        }
    }

    #[test]
    fn native_probe_codex_environment_strips_only_local_drive_verbatim_prefix() {
        assert_eq!(
            codex_windows_environment_path(Path::new(
                r"\\?\C:\ProgramData\Assemblywright\planning-runtime\provider\codex-home"
            )),
            OsString::from(r"C:\ProgramData\Assemblywright\planning-runtime\provider\codex-home")
        );
        assert_eq!(
            codex_windows_environment_path(Path::new(
                r"C:\ProgramData\Assemblywright\planning-runtime\provider\temp"
            )),
            OsString::from(r"C:\ProgramData\Assemblywright\planning-runtime\provider\temp")
        );
        assert_eq!(
            codex_windows_environment_path(Path::new(r"\\?\UNC\server\share\state")),
            OsString::from(r"\\?\UNC\server\share\state")
        );
        assert_eq!(
            codex_windows_environment_path(Path::new(r"\\?\Volume{1234}\state")),
            OsString::from(r"\\?\Volume{1234}\state")
        );
    }

    #[test]
    fn child_visible_current_directory_strips_only_local_drive_verbatim_prefix() {
        let authoritative =
            PathBuf::from(r"\\?\C:\ProgramData\Assemblywright\planning-runtime\provider");
        let authoritative_before = authoritative.clone();
        assert_eq!(
            child_visible_current_directory(&authoritative),
            OsString::from(r"C:\ProgramData\Assemblywright\planning-runtime\provider")
        );
        assert_eq!(authoritative, authoritative_before);

        for unchanged in [
            r"C:\ProgramData\Assemblywright\planning-runtime\provider",
            r"\\?\UNC\server\share\state",
            r"\\?\Volume{1234}\state",
        ] {
            let authoritative = PathBuf::from(unchanged);
            let authoritative_before = authoritative.clone();
            assert_eq!(
                child_visible_current_directory(&authoritative),
                authoritative.as_os_str()
            );
            assert_eq!(authoritative, authoritative_before);
        }
    }

    #[test]
    fn restricted_child_token_uses_exact_token_user_as_default_owner() {
        let current_user = current_token_sid_text().unwrap();
        let token = create_restricted_token().unwrap();
        if current_user == "S-1-5-18" {
            let system = system_sid().unwrap();
            assert_eq!(
                apply_token_default_owner_policy(&token, "S-1-5-21-1-2-3-1001").unwrap(),
                TokenDefaultOwnerPolicy::LocalSystemOwnerApplied
            );
            assert!(token_information_sid_matches(token.raw(), TokenOwner, system.raw()).unwrap());
        } else {
            let expected = StringSid::parse_canonical(&current_user).unwrap();
            assert_eq!(
                apply_token_default_owner_policy(&token, &current_user).unwrap(),
                TokenDefaultOwnerPolicy::ProvisioningOwnerApplied
            );
            assert!(token_information_sid_matches(token.raw(), TokenUser, expected.raw()).unwrap());
            assert!(
                token_information_sid_matches(token.raw(), TokenOwner, expected.raw()).unwrap()
            );
            assert!(apply_token_default_owner_policy(&token, "S-1-5-32-544").is_err());
        }
        assert!(apply_token_default_owner_policy(&token, "not-a-sid").is_err());
    }

    #[test]
    fn token_default_owner_policy_rejects_every_unexpected_principal_shape() {
        assert_eq!(
            select_token_default_owner_policy(true, false, false).unwrap(),
            TokenDefaultOwnerPolicy::ProvisioningOwnerApplied
        );
        assert_eq!(
            select_token_default_owner_policy(false, true, false).unwrap(),
            TokenDefaultOwnerPolicy::LocalSystemOwnerApplied
        );
        for rejected in [
            (false, false, false),
            (true, false, true),
            (false, true, true),
            (true, true, false),
            (true, true, true),
        ] {
            assert!(select_token_default_owner_policy(rejected.0, rejected.1, rejected.2).is_err());
        }
    }

    #[test]
    fn environment_contract_rejects_unlisted_names() {
        assert!(build_environment_block(
            &"local".encode_utf16().collect::<Vec<_>>(),
            &"windows".encode_utf16().collect::<Vec<_>>(),
            &[(OsStr::new("PATH"), OsStr::new("secret"))]
        )
        .is_err());
    }

    #[test]
    fn user_environment_exposes_only_one_exact_local_app_data_value() {
        let source = test_environment(&[
            "=C:=C:\\Windows",
            "APPDATA=C:\\private\\roaming",
            "LOCALAPPDATA=C:\\private\\local",
            "PATH=C:\\must-not-cross",
            "SystemRoot=C:\\Windows",
        ]);
        let local = extract_local_app_data(&source).unwrap();
        assert_eq!(String::from_utf16(&local).unwrap(), r"C:\private\local");
        let child = build_environment_block(
            &local,
            &"C:\\Windows".encode_utf16().collect::<Vec<_>>(),
            &[],
        )
        .unwrap();
        assert_eq!(environment_names(&child), ["LOCALAPPDATA", "SystemRoot"]);
        assert!(!String::from_utf16_lossy(&child).contains("must-not-cross"));
    }

    #[test]
    fn user_environment_rejects_missing_duplicate_and_empty_local_app_data() {
        assert!(extract_local_app_data(&test_environment(&["SystemRoot=C:\\Windows"])).is_err());
        assert!(extract_local_app_data(&test_environment(&[
            "LOCALAPPDATA=C:\\first",
            "localappdata=C:\\second",
        ]))
        .is_err());
        assert!(extract_local_app_data(&test_environment(&["LOCALAPPDATA="])).is_err());
    }

    #[test]
    fn user_environment_rejects_malformed_invalid_utf16_and_oversize_blocks() {
        assert!(extract_local_app_data(&test_environment(&["MALFORMED"])).is_err());
        assert!(extract_local_app_data(&[b'L' as u16, b'=' as u16, 0xd800, 0, 0]).is_err());
        assert!(extract_local_app_data(&[b'L' as u16, b'=' as u16, 0]).is_err());
        let mut oversized = vec![b'A' as u16; MAX_USER_ENVIRONMENT_UNITS + 1];
        let end = oversized.len();
        oversized[end - 2] = 0;
        oversized[end - 1] = 0;
        assert!(extract_local_app_data(&oversized).is_err());
    }

    #[test]
    fn suspended_guard_rejects_invalid_handle_pairs() {
        let information = PROCESS_INFORMATION {
            hProcess: null_mut(),
            hThread: INVALID_HANDLE_VALUE,
            dwProcessId: 0,
            dwThreadId: 0,
        };
        assert!(SuspendedProcessGuard::new(information).is_err());
    }

    #[test]
    fn uncommitted_suspended_process_is_terminated_and_waited() {
        let system_root = std::env::var_os("SystemRoot").unwrap();
        let executable = PathBuf::from(system_root).join(r"System32\cmd.exe");
        let executable_w = wide(executable.as_os_str());
        let mut command_line = wide(OsStr::new("cmd.exe /d /c exit 0"));
        let mut startup: STARTUPINFOW = unsafe { zeroed() };
        startup.cb = size_of::<STARTUPINFOW>() as u32;
        let mut information: PROCESS_INFORMATION = unsafe { zeroed() };
        assert_ne!(
            unsafe {
                CreateProcessW(
                    executable_w.as_ptr(),
                    command_line.as_mut_ptr(),
                    null(),
                    null(),
                    0,
                    CREATE_SUSPENDED,
                    null(),
                    null(),
                    &startup,
                    &mut information,
                )
            },
            0
        );
        let mut observation = null_mut();
        assert_ne!(
            unsafe {
                DuplicateHandle(
                    GetCurrentProcess(),
                    information.hProcess,
                    GetCurrentProcess(),
                    &mut observation,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS,
                )
            },
            0
        );
        let observation = OwnedHandle::new(observation).unwrap();
        let guard = SuspendedProcessGuard::new(information).unwrap();
        drop(guard);
        assert_eq!(
            unsafe { WaitForSingleObject(observation.raw(), 5_000) },
            WAIT_OBJECT_0
        );
        let mut exit_code = 0;
        assert_ne!(
            unsafe { GetExitCodeProcess(observation.raw(), &mut exit_code) },
            0
        );
        assert_eq!(exit_code, TERMINATED_EXIT_CODE);
    }

    #[test]
    fn provisioning_preflight_and_recursive_acl_contract_is_ordered() {
        let script = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/provision-planning-runtime.ps1"
        ));
        let profile_mutation = script
            .find("[AssemblywrightPlanningProfilesV9]::Ensure($providerName)")
            .unwrap();
        for proof in [
            "$targetManifest = Get-ProvenTreeManifest $data $false",
            "$masterProof = Open-SourceFileProof $MasterExe",
            "$codexManifest = Get-ProvenTreeManifest $CodexHome $true",
            "$ghManifest = Get-ProvenTreeManifest $GhConfigDir $true",
            "$serviceArguments = [AssemblywrightPlanningPathProofV9]::ParseCommandLine",
            "Assert-TopLevelAllowlist $masterCheck",
        ] {
            assert!(script.find(proof).unwrap() < profile_mutation);
        }
        let final_source_proof = script
            .find("Assert-TopLevelAllowlist $masterCheck")
            .unwrap();
        for mutation in [
            "[AssemblywrightPlanningProfilesV9]::Ensure($providerName)",
            "$planningProof = Ensure-ProvenTargetDirectory $planning $runtimeNamespaceProof",
        ] {
            assert!(final_source_proof < script.find(mutation).unwrap());
        }
        assert!(!script.contains("Copy-Item"));
        assert!(!script.contains("Set-Acl"));
        assert!(!script.contains("Get-ChildItem -LiteralPath $Root -Force -Recurse"));
        assert!(script.contains("SetSecurityInfo(handle,SE_FILE_OBJECT,DACL_SECURITY_INFORMATION"));
        assert!(script.contains("SetEntriesInAclW((uint)entries.Length,entries,oldAcl"));
        assert!(script.contains("NtSetSecurityObject(handle,OWNER_SECURITY_INFORMATION|DACL_SECURITY_INFORMATION|PROTECTED_DACL_SECURITY_INFORMATION,security)"));
        assert!(script.contains("RtlNtStatusToDosError(status)"));
        assert!(script.contains("SE_DACL_PROTECTED=0x1000,SE_SELF_RELATIVE=0x8000"));
        assert!(script.contains("GENERIC_READ|WRITE_DAC|WRITE_OWNER"));
        assert!(script.contains(
            "Open(path,FILE_READ_ATTRIBUTES|READ_CONTROL|WRITE_DAC|WRITE_OWNER,FILE_SHARE_READ|FILE_SHARE_WRITE"
        ));
        assert!(script.contains("READ_CONTROL=0x00020000"));
        assert!(script.contains("Open(path,FILE_READ_ATTRIBUTES,FILE_SHARE_READ|FILE_SHARE_WRITE"));
        assert!(script.contains("$acl.SetOwner($ownerSid)"));
        assert!(!script.contains("GetSecurityDescriptorOwner"));
        assert!(script.contains("role=$Role kind=$kind status=$status"));
        assert!(script.contains("$Proof.ApplyProtectedAcl($descriptor)"));
        assert!(script.contains("Set-ManifestDescendantsAcl $targetManifest"));
        assert!(script.contains("Assert-ManifestUnchanged $manifest"));
        assert!(script.contains("Open(path,GENERIC_READ,0,FILE_FLAG_OPEN_REPARSE_POINT)"));
        assert!(script.contains("public readonly string Identity;"));
        assert!(script.contains("stream.CopyTo(output); output.Flush(true);"));
        assert!(script.contains("Every provisioning source file must have exactly one hard link."));
        assert!(script.contains("Every existing target file must have exactly one hard link."));
        assert!(script.contains("[ValidatePattern('^[0-9a-fA-F]{64}$')][string]$MasterExeSha256"));
        assert!(
            script.contains("The service executable does not match the held release executable.")
        );
        assert!(script.contains("Install-ProvenFile $masterProof"));
        assert!(script.contains("OpenExecutableGuard($stagedMaster)"));
        assert!(script.contains(
            "The planning data directory may not contain a Windows profile or shared-user root."
        ));
        assert!(script.contains("AssemblywrightPlanningPathProofV9]::Canonical"));
        assert!(script.contains("FILE_FLAG_OPEN_REPARSE_POINT"));
        assert!(script.contains("ParentIdentity=$parent.Proof.Identity"));
        assert!(script.contains("Existing private state bytes do not match the held source proof."));
        assert!(script.contains("if (-not ('AssemblywrightPlanningPathProofV9' -as [type]))"));
        assert!(script.contains("Provisioning paths must use a local drive."));
        assert!(script.contains("The planning data directory may not be a volume root."));
        assert!(
            script.contains("Provisioning sources and the destination data tree must not overlap.")
        );
        assert!(script.contains("A provisioning tree contains a reparse point."));
        assert!(script.contains("SetAccessRuleProtection($true, $false)"));
        assert!(script.contains("WellKnownSidType]::LocalSystemSid"));
        assert!(script.contains("[Security.Principal.IdentityReference]$rule.Identity"));
        assert!(script.contains("$providerAclSid.Value -cne $providerSid"));
        assert!(!script.contains("Identity='SYSTEM'"));
        for protected_tree in [
            "Set-ProtectedManifestAcl $providerManifest $providerAclSid $readExecute",
            "Set-ProtectedManifestAcl $providerCodexManifest $providerAclSid $modify",
            "Set-ProtectedManifestAcl $providerReconciliationManifest $providerAclSid $modify",
            "Set-ProtectedManifestAcl $githubTargetManifest $githubAclSid $readExecute",
            "Set-ProtectedManifestAcl $githubConfigManifest $githubAclSid $modify",
        ] {
            assert!(script.contains(protected_tree));
        }
        assert!(script.contains(
            "Set-ProtectedManifestAcl $providerReconciliationManifest $providerAclSid $modify $true"
        ));
        assert!(script.contains(
            "Windows therefore installs an exact child DACL atomically at CreateFile/CreateDirectory time"
        ));
        assert!(script.contains(
            "$entryRules = ScopeRules $sid $rights ($InheritNewChildren -and $entry.Directory)"
        ));
        assert!(script.contains(
            "A pre-existing writable directory must remain an atomic inheritance boundary"
        ));
        assert!(script.contains("[StringComparer]::OrdinalIgnoreCase"));
        assert!(script.contains("$children.Count -ne $foldedNames.Count"));
        assert!(script.contains("Assert-ManifestTopLevelExact $planningManifest"));
        assert!(!script.contains("New-Item -ItemType Directory -Force"));
        let held_hierarchy = [
            "$locatorProof = Ensure-ProvenTargetDirectory $locator $targetManifest.RootProof",
            "$runtimeVendorProof = Ensure-ProvenTargetDirectory $runtimeVendor $programDataProof",
            "$runtimeNamespaceProof = Ensure-ProvenTargetDirectory $runtimeNamespace $runtimeVendorProof",
            "$planningProof = Ensure-ProvenTargetDirectory $planning $runtimeNamespaceProof",
            "$providerDirectoryProof = Ensure-ProvenTargetDirectory $provider $planningProof",
            "$providerCodexHomeProof = Ensure-ProvenTargetDirectory $providerCodexHome $providerDirectoryProof",
            "$githubDirectoryProof = Ensure-ProvenTargetDirectory $github $planningProof",
            "$githubConfigProof = Ensure-ProvenTargetDirectory $githubConfig $githubDirectoryProof",
            "$masterCheckProof = Ensure-ProvenTargetDirectory $masterCheck $planningProof",
        ]
        .map(|text| script.find(text).unwrap());
        assert!(held_hierarchy.windows(2).all(|pair| pair[0] < pair[1]));
        assert!(script.contains(
            "throw 'A newly created planning directory escaped or changed its held parent identity.'"
        ));
    }

    #[test]
    fn standalone_check_is_digest_service_and_staged_image_bound() {
        let check = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/check-planning-runtime.ps1"
        ));
        let native = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/planning-runtime-native-proof.ps1"
        ));
        assert!(!check.contains("[string]$MasterExe,"));
        assert!(check.contains("[string]$MasterExeSha256"));
        assert!(check.contains("OpenServiceImage($arguments[0])"));
        assert!(check.contains("OpenStagedImage($staged)"));
        assert!(check.contains("$serviceConfig[0].State -ne 'Stopped'"));
        assert!(check.contains("& $staged --data-dir $data planning-runtime-check --confirm"));
        assert!(check.contains("if (-not ('AssemblywrightPlanningCheckProofV4' -as [type]))"));
        assert_eq!(
            native
                .matches("check-planning-runtime.ps1') -DataDir $DataDir")
                .count(),
            3
        );
        assert!(native.contains("-MasterExeSha256 $MasterExeSha256"));
        for lifecycle_contract in [
            "AssemblywrightPlanningAppContainerLifecycleProofV2",
            "TokenIsAppContainer=29,TokenAppContainerSid=31",
            "PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES",
            "PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES=0x00020009",
            "ConvertStringSidToSid(\"S-1-15-3-1\"",
            "Capabilities=capabilityMemory,CapabilityCount=1",
            "Attributes=0x00000004",
            "CreateEnvironmentBlock(out source,restrictedToken,false)",
            "DestroyEnvironmentBlock(source)",
            "String.Equals(name,\"LOCALAPPDATA\",StringComparison.OrdinalIgnoreCase)",
            "LOCALAPPDATA=\"+local+'\\0'+\"SystemRoot=\"+systemRoot+'\\0'+'\\0'",
            "block[block.Length-2]!='\\0'",
            "Marshal.Copy(block,0,exact,block.Length)",
            "_win32_\"+code",
            "CREATE_SUSPENDED",
            "CreateRestrictedToken(baseToken,DISABLE_MAX_PRIVILEGE",
            "CreateProcessAsUser(restrictedToken",
            "ref startup,out pi),16)",
            "Stage(TerminateProcess(pi.Process,0xA55E2001),26)",
            "WaitForSingleObject(pi.Process,5000)",
            "AssignProcessToJobObject(job,pi.Process)",
            "LimitFlags=0x00002000",
            "child token category mismatch",
            "child profile mismatch",
            "$provider = Join-Path $providerRoot 'brainstorming-provider.exe'",
            "throw \"native_child_exit_$exit\"",
            "planning_runtime_native_process_containment_proof_passed",
            "Writable create-close-reopen remains a real-provider live E2E.",
        ] {
            assert!(native.contains(lifecycle_contract));
        }
        assert!(!native.contains("PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES=0x00020005"));
        assert!(!native.contains("Marshal.StringToHGlobalUni(block)"));
        assert!(!native.contains("Environment.GetEnvironmentVariable("));
        assert!(!native.contains("ResumeThread(pi.Thread)"));
        assert!(!native.contains("powershell.exe"));
    }

    #[test]
    fn immutable_scope_root_requires_the_complete_allowlist() {
        let unique = format!(
            "assemblywright-planning-allowlist-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let root = std::env::temp_dir().join(unique);
        fs::create_dir(&root).unwrap();
        fs::write(root.join("provider.exe"), b"provider").unwrap();
        fs::create_dir(root.join("state")).unwrap();
        let paths = vec![
            (root.join("provider.exe"), false),
            (root.join("state"), true),
        ];
        assert!(validate_root_allowlist(&root, &paths).is_ok());
        fs::write(root.join("stale.dll"), b"outside allowlist").unwrap();
        assert!(validate_root_allowlist(&root, &paths).is_err());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn allowlist_rejects_case_variants_without_collapsing_raw_entries() {
        let expected = vec!["provider".to_owned(), "runtime.json".to_owned()];
        assert!(exact_name_set_matches(
            &["provider".to_owned(), "runtime.json".to_owned()],
            &expected
        ));
        assert!(!exact_name_set_matches(
            &[
                "provider".to_owned(),
                "Provider".to_owned(),
                "runtime.json".to_owned()
            ],
            &expected
        ));
        assert!(!exact_name_set_matches(
            &["Provider".to_owned(), "runtime.json".to_owned()],
            &expected
        ));
    }

    #[test]
    fn master_tree_has_no_effect_runtime_exclusion() {
        let source = include_str!("windows_containment.rs");
        let tree = &source[source.find("fn validate_master_tree(").unwrap()
            ..source.find("fn validate_master_entry(").unwrap()];
        assert!(tree.contains("validate_master_entry(&path"));
        assert!(!tree.contains("continue"));
        let removed_helper = ["fn is_exact_planning_root_", "entry("].concat();
        assert!(!source.contains(&removed_helper));
    }

    #[test]
    fn held_no_delete_runtime_directory_denies_rename_swap() {
        let unique = format!(
            "assemblywright-held-runtime-directory-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let parent = std::env::temp_dir().join(unique);
        let original = parent.join("original");
        let replacement = parent.join("replacement");
        fs::create_dir_all(&original).unwrap();
        fs::create_dir(&replacement).unwrap();
        let original = fs::canonicalize(original).unwrap();
        let moved = parent.join("moved");
        let binding =
            directory_binding(original.clone(), RuntimeDirectoryAcl::KnownFolder).unwrap();

        assert!(fs::rename(&original, &moved).is_err());
        assert!(original.is_dir());
        assert!(replacement.is_dir());
        assert_eq!(file_identity(&binding.handle).unwrap().0, binding.volume);

        drop(binding);
        fs::rename(&original, &moved).unwrap();
        fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn live_master_file_identity_check_allows_existing_writer_sharing() {
        let unique = format!(
            "assemblywright-live-master-file-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        let writer = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(&path)
            .unwrap();
        let mut restrictive = OpenOptions::new();
        restrictive
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        assert!(restrictive.open(&path).is_err());
        let mut compatible = OpenOptions::new();
        compatible
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
        let identity = compatible.open(&path).unwrap();
        assert_eq!(file_identity(&identity).unwrap().2, 1);
        drop(identity);
        drop(writer);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn resume_commit_has_a_final_control_poll() {
        let source = include_str!("windows_containment.rs");
        let assigned = source.find("suspended.mark_job_assigned();").unwrap();
        let final_poll = source[assigned..].find("if !control.poll()").unwrap() + assigned;
        let resume = source[assigned..]
            .find("ResumeThread(suspended.thread())")
            .unwrap()
            + assigned;
        assert!(assigned < final_poll && final_poll < resume);
        assert!(source[final_poll..resume].contains("terminate_job(&job, suspended.process())"));
    }
}
