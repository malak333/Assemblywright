#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::ffi::OsString;
    use std::fs;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_ACCESS_DENIED};
    use windows_sys::Win32::System::Services::{
        CloseServiceHandle, OpenSCManagerW, OpenServiceW, SC_MANAGER_CONNECT, SERVICE_CHANGE_CONFIG,
    };

    let arguments: Vec<OsString> = std::env::args_os().collect();
    if arguments.len() != 7 {
        return Err(
            "expected readable, sibling, protected, reserve, service, and marker paths".into(),
        );
    }

    let mut result = String::from("started");
    if matches!(
        fs::read_to_string(&arguments[1]),
        Ok(content) if content == "{\"schema_version\":1}"
    ) {
        result.push_str(";read");
    }
    if matches!(
        fs::read_to_string(&arguments[2]),
        Ok(content) if content == "sibling-secret"
    ) {
        result.push_str(";sibling-read");
    }

    let _ = fs::write(&arguments[1], b"hostile-overwrite");
    let _ = fs::write(&arguments[3], b"hostile-overwrite");
    let _ = fs::remove_file(&arguments[4]);

    let service_name: Vec<u16> = arguments[5].encode_wide().chain(Some(0)).collect();
    // SAFETY: null selects the local default SCM database, and service_name is
    // a live, NUL-terminated UTF-16 buffer for the duration of both calls.
    let manager = unsafe { OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT) };
    if manager.is_null() {
        return Err(std::io::Error::last_os_error().into());
    }
    // Opening with SERVICE_CHANGE_CONFIG itself must be rejected by the exact
    // protected service DACL, proving that the request reached SCM.
    let service = unsafe { OpenServiceW(manager, service_name.as_ptr(), SERVICE_CHANGE_CONFIG) };
    let open_error = unsafe { GetLastError() };
    if !service.is_null() {
        unsafe {
            CloseServiceHandle(service);
            CloseServiceHandle(manager);
        }
        return Err("restricted token unexpectedly opened the service for mutation".into());
    }
    unsafe { CloseServiceHandle(manager) };
    if open_error != ERROR_ACCESS_DENIED {
        return Err(format!("service mutation failed with unexpected error {open_error}").into());
    }

    result.push_str(";service-denied");
    result.push_str(";attempted");
    fs::write(&arguments[6], result.as_bytes())?;
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("the restricted-service hostile probe is Windows-only");
}
