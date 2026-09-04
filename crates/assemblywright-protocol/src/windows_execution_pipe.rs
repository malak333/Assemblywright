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
    CloseHandle, LocalFree, ERROR_INSUFFICIENT_BUFFER, ERROR_PIPE_CONNECTED, GENERIC_READ,
    GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE,
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
    ImpersonateNamedPipeClient, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
    PIPE_WAIT,
};
use windows_sys::Win32::System::SystemServices::SE_GROUP_ENABLED;
use windows_sys::Win32::System::Threading::{
    OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};

#[derive(Debug, thiserror::Error)]
pub enum WindowsExecutionPipeError {
    #[error("Windows execution pipe name or identity is invalid")]
    InvalidBinding,
    #[error("Windows execution pipe I/O failed")]
    Io,
    #[error("Windows execution pipe peer service SID did not match")]
    WrongPeer,
    #[error("Windows execution pipe frame is invalid")]
    InvalidFrame,
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

/// Creates one local-only, single-instance pipe and services exactly one
/// bounded frame from the exact expected client service SID.
pub fn serve_once(
    pipe_name: &str,
    expected_client_service_sid: &str,
    handler: impl FnOnce(&[u8]) -> Result<Vec<u8>, WindowsExecutionPipeError>,
) -> Result<(), WindowsExecutionPipeError> {
    validate_local_pipe_name(pipe_name)?;
    validate_service_sid_text(expected_client_service_sid)?;
    let pipe_name = wide(pipe_name);
    let sddl = wide(&format!(
        "O:SYD:P(A;;GA;;;SY)(A;;GA;;;{expected_client_service_sid})"
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
    unsafe { LocalFree(descriptor) };
    if raw == INVALID_HANDLE_VALUE {
        return Err(WindowsExecutionPipeError::Io);
    }
    let mut pipe = unsafe { File::from_raw_handle(raw as _) };
    let connected = unsafe { ConnectNamedPipe(raw, std::ptr::null_mut()) } != 0
        || std::io::Error::last_os_error().raw_os_error() == Some(ERROR_PIPE_CONNECTED as i32);
    if !connected {
        return Err(WindowsExecutionPipeError::Io);
    }
    let request = read_frame(&mut pipe)?;
    if !impersonated_client_has_sid(raw, expected_client_service_sid)? {
        return Err(WindowsExecutionPipeError::WrongPeer);
    }
    let response = handler(&request)?;
    write_frame(&mut pipe, &response)?;
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
    transact_inner(pipe_name, expected_server_service_sid, request, write_delay)
}

fn transact_inner(
    pipe_name: &str,
    expected_server_service_sid: &str,
    request: &[u8],
    write_delay: Duration,
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
        .map_err(|_| WindowsExecutionPipeError::Io)?;
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
    write_frame(&mut pipe, request)?;
    read_frame(&mut pipe)
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
    let mut token = HANDLE::default();
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) } == 0 {
        return Err(WindowsExecutionPipeError::WrongPeer);
    }
    let result = token_has_sid(token, expected_sid);
    unsafe { CloseHandle(token) };
    result
}

fn token_has_sid(token: HANDLE, expected_sid: &str) -> Result<bool, WindowsExecutionPipeError> {
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
        group.Attributes & SE_GROUP_ENABLED as u32 != 0 && unsafe { EqualSid(group.Sid, sid) } != 0
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

fn read_frame(reader: &mut impl Read) -> Result<Vec<u8>, WindowsExecutionPipeError> {
    let mut length = [0_u8; 4];
    reader
        .read_exact(&mut length)
        .map_err(|_| WindowsExecutionPipeError::Io)?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_WINDOWS_EXECUTION_IPC_FRAME_BYTES {
        return Err(WindowsExecutionPipeError::InvalidFrame);
    }
    let mut frame = vec![0_u8; length];
    reader
        .read_exact(&mut frame)
        .map_err(|_| WindowsExecutionPipeError::Io)?;
    Ok(frame)
}

fn write_frame(writer: &mut impl Write, frame: &[u8]) -> Result<(), WindowsExecutionPipeError> {
    if frame.is_empty() || frame.len() > MAX_WINDOWS_EXECUTION_IPC_FRAME_BYTES {
        return Err(WindowsExecutionPipeError::InvalidFrame);
    }
    writer
        .write_all(&(frame.len() as u32).to_be_bytes())
        .and_then(|_| writer.write_all(frame))
        .and_then(|_| writer.flush())
        .map_err(|_| WindowsExecutionPipeError::Io)
}

fn wide(value: &str) -> Vec<u16> {
    std::ffi::OsStr::new(value)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::validate_service_sid_text;

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
}
