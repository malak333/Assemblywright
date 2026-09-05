use super::MAX_WINDOWS_EXECUTION_IPC_FRAME_BYTES;
use std::ffi::c_void;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::path::Path;
use std::time::Duration;
use windows_sys::Win32::Foundation::{
    CloseHandle, LocalFree, ERROR_BROKEN_PIPE, ERROR_INSUFFICIENT_BUFFER, ERROR_PIPE_CONNECTED,
    ERROR_PIPE_NOT_CONNECTED, ERROR_SEM_TIMEOUT, GENERIC_READ, GENERIC_WRITE, HANDLE,
    INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows_sys::Win32::Security::{
    EqualSid, GetTokenInformation, SecurityIdentification, TokenGroups, TokenImpersonationLevel,
    SECURITY_ATTRIBUTES, TOKEN_GROUPS, TOKEN_QUERY,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_SHARE_NONE, PIPE_ACCESS_DUPLEX, SECURITY_IDENTIFICATION,
    SECURITY_SQOS_PRESENT,
};
use windows_sys::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeServerProcessId,
    ImpersonateNamedPipeClient, PeekNamedPipe, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
    PIPE_TYPE_BYTE, PIPE_WAIT,
};
use windows_sys::Win32::System::SystemServices::{SE_GROUP_ENABLED, SE_GROUP_OWNER};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};

#[derive(Debug, thiserror::Error)]
pub enum WindowsExecutionPipeError {
    #[error("Windows execution pipe name or identity is invalid")]
    InvalidBinding,
    #[error("Windows execution pipe I/O failed")]
    Io,
    #[error("Windows execution pipe {operation} failed with native code {native_code:?}")]
    IoOperation {
        operation: &'static str,
        native_code: Option<i32>,
    },
    #[error("Windows execution pipe peer service SID did not match")]
    WrongPeer,
    #[error("Windows execution pipe frame is invalid")]
    InvalidFrame,
}

const RESPONSE_DELIVERY_TIMEOUT: Duration = Duration::from_secs(5);

impl WindowsExecutionPipeError {
    /// Returns a path-free diagnostic suitable for a Windows service-specific
    /// exit code. The high byte identifies the pipe operation and the low
    /// 24 bits retain ordinary Win32 error codes without exposing IPC data.
    #[doc(hidden)]
    pub fn service_diagnostic_code(&self) -> u32 {
        let (operation, native_code) = match self {
            Self::InvalidBinding => (0, 1),
            Self::Io => (0, 2),
            Self::WrongPeer => (0, 3),
            Self::InvalidFrame => (0, 4),
            Self::IoOperation {
                operation,
                native_code,
            } => (
                match *operation {
                    "server_create" => 1,
                    "server_connect" => 2,
                    "server_read" => 3,
                    "server_write" => 4,
                    "client_open" => 5,
                    "client_write" => 6,
                    "client_read" => 7,
                    "server_delivery_wait" => 8,
                    _ => 15,
                },
                native_code
                    .and_then(|code| u32::try_from(code).ok())
                    .filter(|code| *code <= 0x00FF_FFFF)
                    .unwrap_or(0x00FF_FFFF),
            ),
        };
        0xA000_0000 | (operation << 24) | native_code
    }
}

pub fn validate_local_pipe_name(name: &str) -> Result<(), WindowsExecutionPipeError> {
    let Some(suffix) = name.strip_prefix(r"\\.\pipe\Assemblywright.") else {
        return Err(WindowsExecutionPipeError::InvalidBinding);
    };
    if suffix.is_empty()
        || suffix.len() > 96
        || !suffix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'))
    {
        return Err(WindowsExecutionPipeError::InvalidBinding);
    }
    Ok(())
}

/// Creates one local-only, single-instance pipe bound to the current service
/// SID and services exactly one bounded frame from the expected client SID.
pub fn serve_once(
    pipe_name: &str,
    server_service_sid: &str,
    expected_client_service_sid: &str,
    handler: impl FnOnce(&[u8]) -> Result<Vec<u8>, WindowsExecutionPipeError>,
) -> Result<(), WindowsExecutionPipeError> {
    validate_local_pipe_name(pipe_name)?;
    validate_service_sid_text(server_service_sid)?;
    validate_service_sid_text(expected_client_service_sid)?;
    if !process_has_sid_with_attributes(
        unsafe { GetCurrentProcess() },
        server_service_sid,
        (SE_GROUP_ENABLED | SE_GROUP_OWNER) as u32,
    )? {
        return Err(WindowsExecutionPipeError::WrongPeer);
    }
    let pipe_name = wide(pipe_name);
    let sddl = wide(&format!(
        "O:{server_service_sid}D:P(A;;GA;;;SY)(A;;GA;;;{server_service_sid})(A;;GA;;;{expected_client_service_sid})"
    ));
    let mut descriptor = std::ptr::null_mut();
    if unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            sddl.as_ptr(),
            SDDL_REVISION_1,
            &mut descriptor,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(WindowsExecutionPipeError::InvalidBinding);
    }
    let security = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: descriptor,
        bInheritHandle: 0,
    };
    let raw = unsafe {
        CreateNamedPipeW(
            pipe_name.as_ptr(),
            PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            MAX_WINDOWS_EXECUTION_IPC_FRAME_BYTES as u32,
            MAX_WINDOWS_EXECUTION_IPC_FRAME_BYTES as u32,
            5_000,
            &security,
        )
    };
    let create_error = (raw == INVALID_HANDLE_VALUE).then(std::io::Error::last_os_error);
    unsafe { LocalFree(descriptor) };
    if let Some(error) = create_error {
        return Err(io_error("server_create", &error));
    }
    let mut pipe = unsafe { File::from_raw_handle(raw as _) };
    let connect_result = unsafe { ConnectNamedPipe(raw, std::ptr::null_mut()) };
    let connect_error = (connect_result == 0).then(std::io::Error::last_os_error);
    let connected = connect_result != 0
        || connect_error
            .as_ref()
            .and_then(std::io::Error::raw_os_error)
            == Some(ERROR_PIPE_CONNECTED as i32);
    if !connected {
        return Err(io_error(
            "server_connect",
            connect_error.as_ref().expect("failed connection error"),
        ));
    }
    let request = read_frame(&mut pipe, "server_read")?;
    if !impersonated_client_has_sid(raw, expected_client_service_sid)? {
        return Err(WindowsExecutionPipeError::WrongPeer);
    }
    let response = handler(&request)?;
    write_frame(&mut pipe, &response, "server_write")?;
    await_response_reader_close(raw, RESPONSE_DELIVERY_TIMEOUT)?;
    unsafe { DisconnectNamedPipe(raw) };
    Ok(())
}

/// Opens a local pipe, proves the server process token contains the exact
/// pinned service SID, and exchanges one bounded request/response frame.
pub fn transact(
    pipe_name: &str,
    expected_server_service_sid: &str,
    request: &[u8],
) -> Result<Vec<u8>, WindowsExecutionPipeError> {
    transact_inner(
        pipe_name,
        expected_server_service_sid,
        request,
        Duration::ZERO,
        Duration::ZERO,
    )
}

/// Native service-test seam for proving that server authentication is bound
/// to the client's message rather than racing pipe connection establishment.
/// Product callers use `transact`, which supplies no delay.
#[doc(hidden)]
pub fn transact_with_write_delay_for_native_test(
    pipe_name: &str,
    expected_server_service_sid: &str,
    request: &[u8],
    write_delay: Duration,
) -> Result<Vec<u8>, WindowsExecutionPipeError> {
    if write_delay > Duration::from_secs(5) {
        return Err(WindowsExecutionPipeError::InvalidBinding);
    }
    transact_inner(
        pipe_name,
        expected_server_service_sid,
        request,
        write_delay,
        Duration::ZERO,
    )
}

/// Native service-test seam for proving the server retains a completed
/// response until a delayed client has read the entire frame.
#[doc(hidden)]
pub fn transact_with_response_read_delay_for_native_test(
    pipe_name: &str,
    expected_server_service_sid: &str,
    request: &[u8],
    response_read_delay: Duration,
) -> Result<Vec<u8>, WindowsExecutionPipeError> {
    if response_read_delay > Duration::from_secs(10) {
        return Err(WindowsExecutionPipeError::InvalidBinding);
    }
    transact_inner(
        pipe_name,
        expected_server_service_sid,
        request,
        Duration::ZERO,
        response_read_delay,
    )
}

fn transact_inner(
    pipe_name: &str,
    expected_server_service_sid: &str,
    request: &[u8],
    write_delay: Duration,
    response_read_delay: Duration,
) -> Result<Vec<u8>, WindowsExecutionPipeError> {
    validate_local_pipe_name(pipe_name)?;
    validate_service_sid_text(expected_server_service_sid)?;
    if request.is_empty() || request.len() > MAX_WINDOWS_EXECUTION_IPC_FRAME_BYTES {
        return Err(WindowsExecutionPipeError::InvalidFrame);
    }
    let mut pipe = OpenOptions::new()
        .access_mode(GENERIC_READ | GENERIC_WRITE)
        .share_mode(FILE_SHARE_NONE)
        .custom_flags(SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION)
        .open(Path::new(pipe_name))
        .map_err(|error| io_error("client_open", &error))?;
    let raw = pipe.as_raw_handle() as HANDLE;
    let mut server_pid = 0_u32;
    if unsafe { GetNamedPipeServerProcessId(raw, &mut server_pid) } == 0 || server_pid == 0 {
        return Err(WindowsExecutionPipeError::WrongPeer);
    }
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, server_pid) };
    if process.is_null() {
        return Err(WindowsExecutionPipeError::WrongPeer);
    }
    let matches = process_has_sid(process, expected_server_service_sid);
    unsafe { CloseHandle(process) };
    if !matches? {
        return Err(WindowsExecutionPipeError::WrongPeer);
    }
    if !write_delay.is_zero() {
        std::thread::sleep(write_delay);
    }
    write_frame(&mut pipe, request, "client_write")?;
    if !response_read_delay.is_zero() {
        std::thread::sleep(response_read_delay);
    }
    read_frame(&mut pipe, "client_read")
}

fn await_response_reader_close(
    pipe: HANDLE,
    timeout: Duration,
) -> Result<(), WindowsExecutionPipeError> {
    let deadline = std::time::Instant::now() + timeout;
    loop {
        if unsafe {
            PeekNamedPipe(
                pipe,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        } == 0
        {
            let error = std::io::Error::last_os_error();
            return match error.raw_os_error() {
                Some(code)
                    if code == ERROR_BROKEN_PIPE as i32
                        || code == ERROR_PIPE_NOT_CONNECTED as i32 =>
                {
                    Ok(())
                }
                _ => Err(io_error("server_delivery_wait", &error)),
            };
        }
        if std::time::Instant::now() >= deadline {
            return Err(WindowsExecutionPipeError::IoOperation {
                operation: "server_delivery_wait",
                native_code: Some(ERROR_SEM_TIMEOUT as i32),
            });
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn impersonated_client_has_sid(
    pipe: HANDLE,
    expected_sid: &str,
) -> Result<bool, WindowsExecutionPipeError> {
    if unsafe { ImpersonateNamedPipeClient(pipe) } == 0 {
        return Err(WindowsExecutionPipeError::WrongPeer);
    }
    let mut token = HANDLE::default();
    let opened = unsafe {
        windows_sys::Win32::System::Threading::OpenThreadToken(
            windows_sys::Win32::System::Threading::GetCurrentThread(),
            TOKEN_QUERY,
            1,
            &mut token,
        )
    } != 0;
    let result = if opened {
        token_is_identification_only(token).and_then(|identification_only| {
            if identification_only {
                token_has_sid(token, expected_sid)
            } else {
                Err(WindowsExecutionPipeError::WrongPeer)
            }
        })
    } else {
        Err(WindowsExecutionPipeError::WrongPeer)
    };
    if !token.is_null() {
        unsafe { CloseHandle(token) };
    }
    if unsafe { windows_sys::Win32::Security::RevertToSelf() } == 0 {
        return Err(WindowsExecutionPipeError::WrongPeer);
    }
    result
}

fn token_is_identification_only(token: HANDLE) -> Result<bool, WindowsExecutionPipeError> {
    let mut level = 0_i32;
    let mut returned = 0_u32;
    if unsafe {
        GetTokenInformation(
            token,
            TokenImpersonationLevel,
            (&mut level as *mut i32).cast(),
            std::mem::size_of::<i32>() as u32,
            &mut returned,
        )
    } == 0
        || returned != std::mem::size_of::<i32>() as u32
    {
        return Err(WindowsExecutionPipeError::WrongPeer);
    }
    Ok(level == SecurityIdentification)
}

fn process_has_sid(process: HANDLE, expected_sid: &str) -> Result<bool, WindowsExecutionPipeError> {
    process_has_sid_with_attributes(process, expected_sid, SE_GROUP_ENABLED as u32)
}

fn process_has_sid_with_attributes(
    process: HANDLE,
    expected_sid: &str,
    required_attributes: u32,
) -> Result<bool, WindowsExecutionPipeError> {
    let mut token = HANDLE::default();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return Err(WindowsExecutionPipeError::WrongPeer);
    }
    let result = token_has_sid_with_attributes(token, expected_sid, required_attributes);
    unsafe { CloseHandle(token) };
    result
}

fn token_has_sid(token: HANDLE, expected_sid: &str) -> Result<bool, WindowsExecutionPipeError> {
    token_has_sid_with_attributes(token, expected_sid, SE_GROUP_ENABLED as u32)
}

fn token_has_sid_with_attributes(
    token: HANDLE,
    expected_sid: &str,
    required_attributes: u32,
) -> Result<bool, WindowsExecutionPipeError> {
    let sid = sid_from_text(expected_sid)?;
    let mut needed = 0_u32;
    unsafe { GetTokenInformation(token, TokenGroups, std::ptr::null_mut(), 0, &mut needed) };
    if needed == 0
        || std::io::Error::last_os_error().raw_os_error() != Some(ERROR_INSUFFICIENT_BUFFER as i32)
    {
        unsafe { LocalFree(sid) };
        return Err(WindowsExecutionPipeError::WrongPeer);
    }
    let mut buffer = vec![0_u8; needed as usize];
    if unsafe {
        GetTokenInformation(
            token,
            TokenGroups,
            buffer.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    } == 0
    {
        unsafe { LocalFree(sid) };
        return Err(WindowsExecutionPipeError::WrongPeer);
    }
    let groups = unsafe { &*(buffer.as_ptr() as *const TOKEN_GROUPS) };
    let first = groups.Groups.as_ptr();
    let matched = (0..groups.GroupCount as usize).any(|index| {
        let group = unsafe { &*first.add(index) };
        group.Attributes & required_attributes == required_attributes
            && unsafe { EqualSid(group.Sid, sid) } != 0
    });
    unsafe { LocalFree(sid) };
    Ok(matched)
}

fn sid_from_text(value: &str) -> Result<*mut c_void, WindowsExecutionPipeError> {
    let value = wide(value);
    let mut sid = std::ptr::null_mut();
    if unsafe {
        windows_sys::Win32::Security::Authorization::ConvertStringSidToSidW(
            value.as_ptr(),
            &mut sid,
        )
    } == 0
        || sid.is_null()
    {
        return Err(WindowsExecutionPipeError::InvalidBinding);
    }
    Ok(sid)
}

fn validate_service_sid_text(value: &str) -> Result<(), WindowsExecutionPipeError> {
    let mut parts = value.split('-');
    if parts.next() != Some("S")
        || parts.next() != Some("1")
        || parts.next() != Some("5")
        || parts.next() != Some("80")
    {
        return Err(WindowsExecutionPipeError::InvalidBinding);
    }
    let mut nonzero = false;
    for _ in 0..5 {
        let Some(part) = parts.next() else {
            return Err(WindowsExecutionPipeError::InvalidBinding);
        };
        let parsed = part
            .parse::<u32>()
            .map_err(|_| WindowsExecutionPipeError::InvalidBinding)?;
        if parsed.to_string() != part {
            return Err(WindowsExecutionPipeError::InvalidBinding);
        }
        nonzero |= parsed != 0;
    }
    if parts.next().is_some() || !nonzero {
        return Err(WindowsExecutionPipeError::InvalidBinding);
    }
    let sid = sid_from_text(value)?;
    unsafe { LocalFree(sid) };
    Ok(())
}

fn read_frame(
    reader: &mut impl Read,
    operation: &'static str,
) -> Result<Vec<u8>, WindowsExecutionPipeError> {
    let mut length = [0_u8; 4];
    reader
        .read_exact(&mut length)
        .map_err(|error| io_error(operation, &error))?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_WINDOWS_EXECUTION_IPC_FRAME_BYTES {
        return Err(WindowsExecutionPipeError::InvalidFrame);
    }
    let mut frame = vec![0_u8; length];
    reader
        .read_exact(&mut frame)
        .map_err(|error| io_error(operation, &error))?;
    Ok(frame)
}

fn write_frame(
    writer: &mut impl Write,
    frame: &[u8],
    operation: &'static str,
) -> Result<(), WindowsExecutionPipeError> {
    if frame.is_empty() || frame.len() > MAX_WINDOWS_EXECUTION_IPC_FRAME_BYTES {
        return Err(WindowsExecutionPipeError::InvalidFrame);
    }
    writer
        .write_all(&(frame.len() as u32).to_be_bytes())
        .and_then(|_| writer.write_all(frame))
        .and_then(|_| writer.flush())
        .map_err(|error| io_error(operation, &error))
}

fn io_error(operation: &'static str, error: &std::io::Error) -> WindowsExecutionPipeError {
    WindowsExecutionPipeError::IoOperation {
        operation,
        native_code: error.raw_os_error(),
    }
}

fn wide(value: &str) -> Vec<u16> {
    std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{validate_service_sid_text, WindowsExecutionPipeError};

    #[test]
    fn accepts_only_canonical_service_sid_shape() {
        assert!(validate_service_sid_text("S-1-5-80-123456789-2-3-4-4294967295").is_ok());
        for hostile in [
            "S-1-1-0",
            "S-1-5-18",
            "S-1-5-32-544",
            "S-1-5-80-1-2-3-4",
            "S-1-5-80-1-2-3-4-5-6",
            "S-1-5-80-01-2-3-4-5",
            "S-1-5-80-0-0-0-0-0",
        ] {
            assert!(validate_service_sid_text(hostile).is_err(), "{hostile}");
        }
    }

    #[test]
    fn service_diagnostic_codes_are_path_free_and_operation_exact() {
        assert_eq!(
            WindowsExecutionPipeError::IoOperation {
                operation: "client_read",
                native_code: Some(233),
            }
            .service_diagnostic_code(),
            0xA700_00E9
        );
        assert_eq!(
            WindowsExecutionPipeError::IoOperation {
                operation: "server_delivery_wait",
                native_code: Some(121),
            }
            .service_diagnostic_code(),
            0xA800_0079
        );
    }
}
