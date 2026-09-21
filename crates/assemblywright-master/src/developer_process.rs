//! Process-tree ownership for supervised developer validation.
//!
//! The Windows runner creates the command interpreter suspended, assigns it to a
//! kill-on-close Job, and only then resumes its primary thread. Keeping the Job
//! handle in [`ValidationChild`] makes runner exit or crash terminate the entire
//! validation tree. This is a developer-build lifecycle guarantee, not hostile
//! process containment or production execution evidence.

use anyhow::{Context, Result};
use std::{
    ffi::{OsStr, OsString},
    fs::File,
    path::Path,
    process::ExitStatus,
};

#[cfg(windows)]
use anyhow::bail;

#[cfg(windows)]
use std::time::{Duration, Instant};

#[cfg(windows)]
const TERMINATED_EXIT_CODE: u32 = 1;

#[derive(Debug, thiserror::Error)]
#[error("validation spawn cleanup could not be confirmed: {0}")]
pub struct CleanupUnconfirmed(pub String);

/// An owned, closed snapshot of the validation child's environment.
///
/// Capturing before spawn prevents a concurrent ambient environment mutation
/// from changing the child between policy validation and process creation.
#[derive(Debug, Clone)]
pub struct ValidationEnvironment {
    entries: Vec<(OsString, OsString)>,
}

impl ValidationEnvironment {
    pub fn capture() -> Result<Self> {
        Self::capture_entries(std::env::vars_os())
    }

    fn capture_entries(entries: impl IntoIterator<Item = (OsString, OsString)>) -> Result<Self> {
        let mut environment = Self {
            entries: Vec::new(),
        };
        for (name, value) in entries {
            #[cfg(windows)]
            if windows_reserved_environment_entry(&name) {
                continue;
            }
            environment.set(name, value)?;
        }
        Ok(environment)
    }

    pub fn set(&mut self, name: impl Into<OsString>, value: impl Into<OsString>) -> Result<()> {
        let name = name.into();
        let value = value.into();
        #[cfg(windows)]
        let name_key = windows_environment_name_key(&name)?;
        self.entries.retain(|(candidate, _)| {
            #[cfg(windows)]
            {
                windows_environment_name_key(candidate)
                    .map(|candidate| candidate != name_key)
                    .unwrap_or(false)
            }
            #[cfg(not(windows))]
            {
                candidate != &name
            }
        });
        self.entries.push((name, value));
        Ok(())
    }

    pub fn get(&self, name: &OsStr) -> Option<&OsStr> {
        self.entries.iter().find_map(|(candidate, value)| {
            #[cfg(windows)]
            let matches = windows_environment_name_key(candidate).ok()
                == windows_environment_name_key(name).ok();
            #[cfg(not(windows))]
            let matches = candidate == name;
            matches.then_some(value.as_os_str())
        })
    }
}

#[cfg(windows)]
fn windows_environment_name_key(name: &OsStr) -> Result<String> {
    let name = name
        .to_str()
        .context("Windows environment variable name is not Unicode")?;
    let hidden_drive = windows_hidden_drive_environment_entry(name);
    if name.is_empty() || name.contains('\0') || name.contains('=') && !hidden_drive {
        bail!("Windows environment variable name is invalid");
    }
    Ok(name.to_uppercase())
}

#[cfg(windows)]
fn windows_reserved_environment_entry(name: &OsStr) -> bool {
    name.to_str()
        .is_some_and(|name| name.starts_with('=') && !windows_hidden_drive_environment_entry(name))
}

#[cfg(windows)]
fn windows_hidden_drive_environment_entry(name: &str) -> bool {
    name.len() == 3
        && name.as_bytes()[0] == b'='
        && name.as_bytes()[1].is_ascii_alphabetic()
        && name.as_bytes()[2] == b':'
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::{
        ffi::c_void,
        ffi::OsString,
        mem::{size_of, size_of_val, zeroed},
        os::windows::{
            ffi::{OsStrExt, OsStringExt},
            io::AsRawHandle,
            process::ExitStatusExt,
        },
        ptr::{null, null_mut},
    };
    use windows_sys::Win32::{
        Foundation::{
            CloseHandle, DuplicateHandle, DUPLICATE_SAME_ACCESS, ERROR_INSUFFICIENT_BUFFER,
            GENERIC_READ, HANDLE, INVALID_HANDLE_VALUE, WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
        },
        Security::SECURITY_ATTRIBUTES,
        Storage::FileSystem::{
            CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        },
        System::{
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicAccountingInformation,
                JobObjectExtendedLimitInformation, QueryInformationJobObject,
                SetInformationJobObject, TerminateJobObject,
                JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            },
            SystemInformation::GetSystemDirectoryW,
            Threading::{
                CreateProcessW, DeleteProcThreadAttributeList, GetCurrentProcess,
                GetExitCodeProcess, InitializeProcThreadAttributeList, ResumeThread,
                TerminateProcess, UpdateProcThreadAttribute, WaitForSingleObject, CREATE_SUSPENDED,
                CREATE_UNICODE_ENVIRONMENT, EXTENDED_STARTUPINFO_PRESENT, PROCESS_INFORMATION,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST, STARTF_USESTDHANDLES, STARTUPINFOEXW,
            },
        },
    };

    const TERMINATION_WAIT: Duration = Duration::from_secs(30);

    struct OwnedHandle(HANDLE);

    // A Windows kernel handle can be used and closed from any thread. This type
    // uniquely owns its handle and exposes no operation that aliases ownership.
    unsafe impl Send for OwnedHandle {}

    impl OwnedHandle {
        fn new(handle: HANDLE, operation: &str) -> Result<Self> {
            if handle.is_null() || handle == INVALID_HANDLE_VALUE {
                return Err(std::io::Error::last_os_error()).context(operation.to_owned());
            }
            Ok(Self(handle))
        }

        fn raw(&self) -> HANDLE {
            self.0
        }
    }

    impl Drop for OwnedHandle {
        fn drop(&mut self) {
            if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
    }

    struct AttributeList {
        storage: Vec<usize>,
        ptr: windows_sys::Win32::System::Threading::LPPROC_THREAD_ATTRIBUTE_LIST,
    }

    impl AttributeList {
        fn new(count: u32) -> Result<Self> {
            let mut bytes = 0usize;
            unsafe { InitializeProcThreadAttributeList(null_mut(), count, 0, &mut bytes) };
            if bytes == 0
                || unsafe { windows_sys::Win32::Foundation::GetLastError() }
                    != ERROR_INSUFFICIENT_BUFFER
            {
                return Err(std::io::Error::last_os_error())
                    .context("measure validation process attribute list");
            }
            let mut storage = vec![0usize; bytes.div_ceil(size_of::<usize>())];
            let ptr = storage.as_mut_ptr().cast();
            if unsafe { InitializeProcThreadAttributeList(ptr, count, 0, &mut bytes) } == 0 {
                return Err(std::io::Error::last_os_error())
                    .context("initialize validation process attribute list");
            }
            Ok(Self { storage, ptr })
        }

        fn update(&mut self, attribute: usize, value: *const c_void, bytes: usize) -> Result<()> {
            if unsafe {
                UpdateProcThreadAttribute(self.ptr, 0, attribute, value, bytes, null_mut(), null())
            } == 0
            {
                return Err(std::io::Error::last_os_error())
                    .context("set validation inherited handle allowlist");
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

    pub struct ValidationChild {
        // Field order is deliberate: closing the Job first applies kill-on-close
        // before the root process handle is released.
        job: OwnedHandle,
        process: OwnedHandle,
    }

    impl ValidationChild {
        pub fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
            match unsafe { WaitForSingleObject(self.process.raw(), 0) } {
                WAIT_TIMEOUT => Ok(None),
                WAIT_OBJECT_0 => {
                    let mut exit_code = 0;
                    if unsafe { GetExitCodeProcess(self.process.raw(), &mut exit_code) } == 0 {
                        return Err(std::io::Error::last_os_error())
                            .context("query validation exit code");
                    }
                    Ok(Some(ExitStatus::from_raw(exit_code)))
                }
                WAIT_FAILED => {
                    Err(std::io::Error::last_os_error()).context("poll validation process")
                }
                result => bail!("unexpected validation wait result {result}"),
            }
        }

        pub async fn terminate(&mut self) -> Result<()> {
            if job_is_empty(self.job.raw())? {
                return match unsafe { WaitForSingleObject(self.process.raw(), 0) } {
                    WAIT_OBJECT_0 => Ok(()),
                    WAIT_TIMEOUT => bail!("validation Job is empty while its root is still active"),
                    WAIT_FAILED => Err(std::io::Error::last_os_error())
                        .context("wait for completed validation process"),
                    result => bail!("unexpected completed validation wait result {result}"),
                };
            }
            if unsafe { TerminateJobObject(self.job.raw(), TERMINATED_EXIT_CODE) } == 0 {
                return Err(std::io::Error::last_os_error())
                    .context("terminate validation process tree");
            }

            let deadline = Instant::now() + TERMINATION_WAIT;
            loop {
                let root_exited = match unsafe { WaitForSingleObject(self.process.raw(), 0) } {
                    WAIT_OBJECT_0 => true,
                    WAIT_TIMEOUT => false,
                    WAIT_FAILED => {
                        return Err(std::io::Error::last_os_error())
                            .context("wait for terminated validation process")
                    }
                    result => bail!("unexpected validation termination wait result {result}"),
                };
                let job_empty = job_is_empty(self.job.raw())?;
                if root_exited && job_empty {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    bail!("validation process tree did not terminate within 30 seconds");
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }
    }

    pub fn spawn(
        validation: &str,
        project: &Path,
        log: File,
        environment: &ValidationEnvironment,
    ) -> Result<ValidationChild> {
        if validation.contains('\0') {
            bail!("validation command contains a NUL character");
        }

        let job = create_kill_on_close_job()?;
        let inherited_log = duplicate_inheritable(log.as_raw_handle() as HANDLE)
            .context("duplicate validation log handle")?;
        let inherited_stdin = open_inheritable_null()?;
        let application = system_cmd_path()?;
        let current_directory = command_working_directory(project)?;
        let environment = environment_block(environment)?;
        let mut inherited = [inherited_stdin.raw(), inherited_log.raw()];
        let mut attributes = AttributeList::new(1)?;
        attributes.update(
            PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
            inherited.as_mut_ptr().cast(),
            size_of_val(&inherited),
        )?;
        let mut command_line: Vec<u16> =
            OsString::from(format!("\"cmd.exe\" /d /s /c \"{validation}\""))
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();

        let mut startup: STARTUPINFOEXW = unsafe { zeroed() };
        startup.StartupInfo.cb = size_of::<STARTUPINFOEXW>() as u32;
        startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
        startup.StartupInfo.hStdInput = inherited_stdin.raw();
        startup.StartupInfo.hStdOutput = inherited_log.raw();
        startup.StartupInfo.hStdError = inherited_log.raw();
        startup.lpAttributeList = attributes.ptr;
        let mut information: PROCESS_INFORMATION = unsafe { zeroed() };

        if unsafe {
            CreateProcessW(
                application.as_ptr(),
                command_line.as_mut_ptr(),
                null(),
                null(),
                1,
                CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | EXTENDED_STARTUPINFO_PRESENT,
                environment.as_ptr().cast(),
                current_directory.as_ptr(),
                &startup.StartupInfo,
                &mut information,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error()).context("spawn suspended validation");
        }

        let process = OwnedHandle::new(information.hProcess, "own validation process handle")?;
        let thread = OwnedHandle::new(information.hThread, "own validation thread handle")?;

        if unsafe { AssignProcessToJobObject(job.raw(), process.raw()) } == 0 {
            let assignment_error = std::io::Error::last_os_error();
            terminate_suspended_process(process.raw())
                .map_err(|error| CleanupUnconfirmed(format!("{error:#}")))
                .with_context(|| {
                    format!("clean up validation after Job assignment failed: {assignment_error}")
                })?;
            return Err(assignment_error).context("assign suspended validation to Job");
        }

        let previous_suspend_count = unsafe { ResumeThread(thread.raw()) };
        if previous_suspend_count != 1 {
            let resume_error = if previous_suspend_count == u32::MAX {
                std::io::Error::last_os_error().to_string()
            } else {
                format!("unexpected previous suspend count {previous_suspend_count}")
            };
            terminate_assigned_process(job.raw(), process.raw())
                .map_err(|error| CleanupUnconfirmed(format!("{error:#}")))
                .with_context(|| {
                    format!("clean up validation after resume failed: {resume_error}")
                })?;
            bail!("resume assigned validation: {resume_error}");
        }

        drop(thread);
        Ok(ValidationChild { job, process })
    }

    fn environment_block(environment: &ValidationEnvironment) -> Result<Vec<u16>> {
        let mut entries = environment
            .entries
            .iter()
            .map(|(name, value)| {
                let key = windows_environment_name_key(name)?;
                let mut encoded: Vec<u16> = name.encode_wide().collect();
                if encoded.contains(&0) {
                    bail!("Windows environment variable name contains a NUL character");
                }
                encoded.push(b'=' as u16);
                let value: Vec<u16> = value.encode_wide().collect();
                if value.contains(&0) {
                    bail!("Windows environment variable value contains a NUL character");
                }
                encoded.extend(value);
                Ok((key, encoded))
            })
            .collect::<Result<Vec<_>>>()?;
        entries.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
        let mut block = Vec::new();
        for (_, entry) in entries {
            block.extend(entry);
            block.push(0);
        }
        if block.is_empty() {
            block.push(0);
        }
        block.push(0);
        Ok(block)
    }

    fn create_kill_on_close_job() -> Result<OwnedHandle> {
        let job = OwnedHandle::new(
            unsafe { CreateJobObjectW(null(), null()) },
            "create validation Job",
        )?;
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                job.raw(),
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error())
                .context("set validation Job kill-on-close policy");
        }
        Ok(job)
    }

    fn duplicate_inheritable(source: HANDLE) -> Result<OwnedHandle> {
        let current_process = unsafe { GetCurrentProcess() };
        let mut duplicate = null_mut();
        if unsafe {
            DuplicateHandle(
                current_process,
                source,
                current_process,
                &mut duplicate,
                0,
                1,
                DUPLICATE_SAME_ACCESS,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error()).context("duplicate inheritable handle");
        }
        OwnedHandle::new(duplicate, "own duplicated handle")
    }

    fn open_inheritable_null() -> Result<OwnedHandle> {
        let attributes = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: null_mut(),
            bInheritHandle: 1,
        };
        let name: Vec<u16> = "NUL\0".encode_utf16().collect();
        OwnedHandle::new(
            unsafe {
                CreateFileW(
                    name.as_ptr(),
                    GENERIC_READ,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    &attributes,
                    OPEN_EXISTING,
                    FILE_ATTRIBUTE_NORMAL,
                    null_mut(),
                )
            },
            "open validation NUL input",
        )
    }

    fn system_cmd_path() -> Result<Vec<u16>> {
        let mut buffer = vec![0u16; 260];
        loop {
            let length = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) };
            if length == 0 {
                return Err(std::io::Error::last_os_error())
                    .context("resolve Windows system directory");
            }
            if (length as usize) < buffer.len() {
                buffer.truncate(length as usize);
                break;
            }
            buffer.resize(length as usize + 1, 0);
        }
        let mut path = OsString::from_wide(&buffer);
        path.push("\\cmd.exe");
        wide_nul(path.as_os_str()).context("encode Windows command interpreter path")
    }

    fn wide_nul(value: &std::ffi::OsStr) -> Result<Vec<u16>> {
        let mut encoded: Vec<u16> = value.encode_wide().collect();
        if encoded.contains(&0) {
            bail!("Windows path contains a NUL character");
        }
        encoded.push(0);
        Ok(encoded)
    }

    fn command_working_directory(project: &Path) -> Result<Vec<u16>> {
        let encoded: Vec<u16> = project.as_os_str().encode_wide().collect();
        let verbatim_disk_prefix: Vec<u16> = "\\\\?\\".encode_utf16().collect();
        let unc_prefix: Vec<u16> = "\\\\".encode_utf16().collect();
        let normalized = encoded
            .strip_prefix(verbatim_disk_prefix.as_slice())
            .unwrap_or(&encoded);
        if normalized.starts_with(unc_prefix.as_slice())
            || normalized.get(1) != Some(&(b':' as u16))
        {
            bail!("validation project must use a local Windows drive path");
        }
        if normalized.contains(&0) {
            bail!("Windows path contains a NUL character");
        }
        let mut current_directory = normalized.to_vec();
        current_directory.push(0);
        Ok(current_directory)
    }

    fn terminate_suspended_process(process: HANDLE) -> Result<()> {
        if unsafe { TerminateProcess(process, TERMINATED_EXIT_CODE) } == 0 {
            return Err(std::io::Error::last_os_error())
                .context("terminate unassigned suspended validation");
        }
        wait_for_process(process, TERMINATION_WAIT)
    }

    fn terminate_assigned_process(job: HANDLE, process: HANDLE) -> Result<()> {
        if unsafe { TerminateJobObject(job, TERMINATED_EXIT_CODE) } == 0 {
            return Err(std::io::Error::last_os_error())
                .context("terminate assigned suspended validation");
        }
        wait_for_process(process, TERMINATION_WAIT)?;
        let deadline = Instant::now() + TERMINATION_WAIT;
        while !job_is_empty(job)? {
            if Instant::now() >= deadline {
                bail!("assigned suspended validation Job did not become empty");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok(())
    }

    fn wait_for_process(process: HANDLE, timeout: Duration) -> Result<()> {
        let timeout_ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX - 1);
        match unsafe { WaitForSingleObject(process, timeout_ms) } {
            WAIT_OBJECT_0 => Ok(()),
            WAIT_TIMEOUT => bail!("timed out waiting for validation process termination"),
            WAIT_FAILED => Err(std::io::Error::last_os_error())
                .context("wait for validation process termination"),
            result => bail!("unexpected validation cleanup wait result {result}"),
        }
    }

    fn job_is_empty(job: HANDLE) -> Result<bool> {
        let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
        if unsafe {
            QueryInformationJobObject(
                job,
                JobObjectBasicAccountingInformation,
                (&mut accounting as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                null_mut(),
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error())
                .context("query validation Job process count");
        }
        Ok(accounting.ActiveProcesses == 0)
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use windows_sys::Win32::{
            Foundation::{CompareObjectHandles, ERROR_INVALID_HANDLE},
            Storage::FileSystem::FILE_SHARE_DELETE,
        };

        fn open_unrelated_inheritable_file(path: &Path) -> Result<OwnedHandle> {
            let attributes = SECURITY_ATTRIBUTES {
                nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: null_mut(),
                bInheritHandle: 1,
            };
            let path = wide_nul(path.as_os_str())?;
            OwnedHandle::new(
                unsafe {
                    CreateFileW(
                        path.as_ptr(),
                        GENERIC_READ | windows_sys::Win32::Foundation::GENERIC_WRITE,
                        FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                        &attributes,
                        windows_sys::Win32::Storage::FileSystem::CREATE_ALWAYS,
                        FILE_ATTRIBUTE_NORMAL,
                        null_mut(),
                    )
                },
                "open unrelated inheritable file",
            )
        }

        #[tokio::test]
        async fn invalid_process_query_cannot_report_confirmed_termination() {
            let root = tempfile::tempdir().unwrap();
            let log = File::create(root.path().join("validation.log")).unwrap();
            let environment = ValidationEnvironment::capture().unwrap();
            let mut child =
                spawn("ping -n 30 127.0.0.1 >NUL", root.path(), log, &environment).unwrap();
            // Force a real kernel query failure without altering the owned Job.
            let process = std::mem::replace(&mut child.process, OwnedHandle(null_mut()));
            assert!(child.try_wait().is_err());
            assert!(child.terminate().await.is_err());
            // The Job still supplies cleanup when observation cannot prove it.
            drop(child);
            assert_eq!(
                unsafe { WaitForSingleObject(process.raw(), 5000) },
                WAIT_OBJECT_0
            );
        }

        #[tokio::test]
        async fn spawn_excludes_unrelated_inheritable_handles() {
            let root = tempfile::tempdir().unwrap();
            let log = File::create(root.path().join("validation.log")).unwrap();
            let unrelated_path = root.path().join("unrelated-inheritable-handle.txt");
            let unrelated = open_unrelated_inheritable_file(&unrelated_path).unwrap();
            let environment = ValidationEnvironment::capture().unwrap();
            let mut child =
                spawn("ping -n 30 127.0.0.1 >NUL", root.path(), log, &environment).unwrap();

            let mut duplicated = null_mut();
            let duplicate_result = unsafe {
                DuplicateHandle(
                    child.process.raw(),
                    unrelated.raw(),
                    GetCurrentProcess(),
                    &mut duplicated,
                    0,
                    0,
                    DUPLICATE_SAME_ACCESS,
                )
            };
            let inherited_same_object = if duplicate_result == 0 {
                let code = unsafe { windows_sys::Win32::Foundation::GetLastError() };
                assert_eq!(
                    code, ERROR_INVALID_HANDLE,
                    "querying the validation child's unrelated handle slot failed unexpectedly"
                );
                false
            } else {
                let duplicated = OwnedHandle::new(duplicated, "own child handle probe").unwrap();
                unsafe { CompareObjectHandles(duplicated.raw(), unrelated.raw()) != 0 }
            };

            drop(unrelated);
            let termination = child.terminate().await;

            assert!(
                !inherited_same_object,
                "validation child inherited the unrelated file object"
            );
            termination.expect("validation process tree did not terminate cleanly");
            std::fs::remove_file(&unrelated_path).unwrap();
        }

        #[test]
        fn environment_block_is_casefolded_sorted_utf16_and_double_nul_terminated() {
            let mut environment = ValidationEnvironment {
                entries: Vec::new(),
            };
            environment.set("z_value", "last").unwrap();
            environment.set("Path", "old").unwrap();
            environment.set("alpha", "first").unwrap();
            environment.set("=c:", r"C:\old").unwrap();
            environment.set("PATH", "new-\u{2603}").unwrap();
            environment.set("=C:", r"C:\current").unwrap();
            let block = environment_block(&environment).unwrap();
            assert!(block.ends_with(&[0, 0]));
            let decoded = block[..block.len() - 1]
                .split(|value| *value == 0)
                .filter(|entry| !entry.is_empty())
                .map(String::from_utf16)
                .collect::<std::result::Result<Vec<_>, _>>()
                .unwrap();
            assert_eq!(
                decoded,
                [
                    r"=C:=C:\current",
                    "alpha=first",
                    "PATH=new-\u{2603}",
                    "z_value=last"
                ]
            );
        }

        #[test]
        fn environment_rejects_ambiguous_names_but_preserves_hidden_drive_entries() {
            let mut environment = ValidationEnvironment {
                entries: Vec::new(),
            };
            assert!(environment.set("", "value").is_err());
            assert!(environment.set("NAME=ALIAS", "value").is_err());
            assert!(environment.set("=CC:", r"C:\ambiguous").is_err());
            environment.set("=d:", r"D:\workspace").unwrap();
            assert_eq!(
                environment.get(OsStr::new("=D:")),
                Some(OsStr::new(r"D:\workspace"))
            );
        }

        #[test]
        fn environment_capture_omits_reserved_pseudo_entries() {
            let mut environment = ValidationEnvironment::capture_entries([
                (OsString::from("PATH"), OsString::from(r"C:\Windows")),
                (OsString::from("=C:"), OsString::from(r"C:\workspace")),
                (OsString::from("=ExitCode"), OsString::from("00000000")),
                (OsString::from("=ExitCodeAscii"), OsString::from("00000000")),
            ])
            .unwrap();

            assert_eq!(
                environment.get(OsStr::new("PATH")),
                Some(OsStr::new(r"C:\Windows"))
            );
            assert_eq!(
                environment.get(OsStr::new("=C:")),
                Some(OsStr::new(r"C:\workspace"))
            );
            assert_eq!(environment.get(OsStr::new("=ExitCode")), None);
            assert_eq!(environment.get(OsStr::new("=ExitCodeAscii")), None);
            assert!(environment.set("=ExitCode", "00000000").is_err());
        }
    }
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::{os::unix::process::CommandExt, process::Stdio, time::Duration};

    pub struct ValidationChild {
        child: tokio::process::Child,
        pid: u32,
    }

    impl ValidationChild {
        pub fn try_wait(&mut self) -> Result<Option<ExitStatus>> {
            self.child.try_wait().context("poll validation process")
        }

        pub async fn terminate(&mut self) -> Result<()> {
            let process_group = i32::try_from(self.pid).context("validation PID exceeds i32")?;
            let kill_result = unsafe { libc::kill(-process_group, libc::SIGKILL) };
            let kill_error = (kill_result == -1).then(std::io::Error::last_os_error);
            if let Some(error) = kill_error {
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return Err(error).context("terminate validation process group");
                }
            }
            tokio::time::timeout(Duration::from_secs(30), self.child.wait())
                .await
                .context("timed out waiting for terminated validation process")?
                .context("wait for terminated validation process")?;
            Ok(())
        }
    }

    pub fn spawn(
        validation: &str,
        project: &Path,
        log: File,
        environment: &ValidationEnvironment,
    ) -> Result<ValidationChild> {
        let mut command = tokio::process::Command::new("/bin/sh");
        command.args(["-c", validation]);
        command.as_std_mut().process_group(0);
        command.env_clear();
        command.envs(
            environment
                .entries
                .iter()
                .map(|(name, value)| (name, value)),
        );
        let child = command
            .current_dir(project)
            .stdin(Stdio::null())
            .stdout(log.try_clone().context("clone validation log")?)
            .stderr(log)
            .kill_on_drop(true)
            .spawn()
            .context("spawn validation")?;
        let pid = child.id().context("validation process has no ID")?;
        Ok(ValidationChild { child, pid })
    }
}

#[cfg(not(any(unix, windows)))]
compile_error!("developer validation processes require Windows or Unix");

pub use platform::spawn;
