//! Private process-tree ownership for Developer GitHub publication commands.
use anyhow::{bail, Context, Result};
use std::time::Duration;
use tokio::process::{Child, Command};

pub(super) fn prepare_process_tree(command: &mut Command) {
    #[cfg(unix)]
    command.process_group(0);
    #[cfg(windows)]
    command.creation_flags(windows_sys::Win32::System::Threading::CREATE_SUSPENDED);
    #[cfg(not(any(unix, windows)))]
    let _ = command;
}

#[cfg(windows)]
pub(super) struct ProcessTree {
    handle: windows_sys::Win32::Foundation::HANDLE,
    stopped: bool,
}

#[cfg(windows)]
unsafe impl Send for ProcessTree {}

#[cfg(windows)]
pub(super) async fn attach_process_tree(child: &mut Child) -> Result<ProcessTree> {
    attach_process_tree_inner(child, false).await
}

#[cfg(windows)]
async fn attach_process_tree_inner(
    child: &mut Child,
    fail_after_assignment: bool,
) -> Result<ProcessTree> {
    use windows_sys::Win32::{
        Foundation::HANDLE,
        System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        },
    };
    let pid = child.id().context("Publication process has no ID")?;
    let process = child
        .raw_handle()
        .context("Publication process handle is unavailable")? as HANDLE;
    let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if handle.is_null() {
        return containment_setup_failed(child, "Could not create the publication process Job")
            .await;
    }
    let mut guard = ProcessTree {
        handle,
        stopped: false,
    };
    let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    if unsafe {
        SetInformationJobObject(
            handle,
            JobObjectExtendedLimitInformation,
            &limits as *const _ as *const _,
            std::mem::size_of_val(&limits) as u32,
        )
    } == 0
    {
        return containment_setup_failed(child, "Could not configure the publication process Job")
            .await;
    }
    let assigned = unsafe { AssignProcessToJobObject(handle, process) };
    if assigned == 0 {
        return containment_setup_failed(
            child,
            "Could not assign publication process to its kill-on-close Job",
        )
        .await;
    }
    if fail_after_assignment {
        return assigned_setup_failed(
            &mut guard,
            child,
            "Publication containment test stopped before resuming the process",
        )
        .await;
    }
    if let Err(error) = resume_contained_primary(child, pid) {
        return assigned_setup_failed(
            &mut guard,
            child,
            &format!("Could not resume the suspended publication process: {error:#}"),
        )
        .await;
    }
    Ok(guard)
}

#[cfg(windows)]
struct OwnedHandle {
    handle: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.handle) };
    }
}

#[cfg(windows)]
fn suspended_primary_thread(pid: u32) -> Result<OwnedHandle> {
    use windows_sys::Win32::{
        Foundation::{GetLastError, ERROR_NO_MORE_FILES, INVALID_HANDLE_VALUE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Thread32First, Thread32Next, TH32CS_SNAPTHREAD,
                THREADENTRY32,
            },
            Threading::{
                GetProcessIdOfThread, OpenThread, THREAD_QUERY_LIMITED_INFORMATION,
                THREAD_SUSPEND_RESUME,
            },
        },
    };
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        bail!("thread snapshot failed");
    }
    let snapshot = OwnedHandle { handle: snapshot };
    let mut entry: THREADENTRY32 = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<THREADENTRY32>() as u32;
    if unsafe { Thread32First(snapshot.handle, &mut entry) } == 0 {
        bail!("thread snapshot was empty");
    }
    let mut thread_id = None;
    loop {
        if entry.th32OwnerProcessID == pid && thread_id.replace(entry.th32ThreadID).is_some() {
            bail!("suspended publication process had more than one thread");
        }
        if unsafe { Thread32Next(snapshot.handle, &mut entry) } == 0 {
            let error = unsafe { GetLastError() };
            if error != ERROR_NO_MORE_FILES {
                bail!("thread snapshot traversal failed");
            }
            break;
        }
    }
    let thread_id = thread_id.context("suspended publication thread was not found")?;
    let thread = unsafe {
        OpenThread(
            THREAD_SUSPEND_RESUME | THREAD_QUERY_LIMITED_INFORMATION,
            0,
            thread_id,
        )
    };
    if thread.is_null() {
        bail!("suspended publication thread could not be opened");
    }
    let thread = OwnedHandle { handle: thread };
    if unsafe { GetProcessIdOfThread(thread.handle) } != pid {
        bail!("suspended publication thread ownership changed");
    }
    Ok(thread)
}

#[cfg(windows)]
fn resume_contained_primary(child: &mut Child, pid: u32) -> Result<()> {
    let thread = suspended_primary_thread(pid)
        .context("Could not identify the suspended publication thread")?;
    if child.try_wait()?.is_some() {
        bail!("Suspended publication process exited before containment completed");
    }
    let prior_suspend_count =
        unsafe { windows_sys::Win32::System::Threading::ResumeThread(thread.handle) };
    drop(thread);
    if prior_suspend_count != 1 {
        bail!("Could not exclusively resume the contained publication process");
    }
    Ok(())
}

#[cfg(windows)]
async fn containment_setup_failed(child: &mut Child, message: &str) -> Result<ProcessTree> {
    let kill = child.start_kill();
    let wait = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
    let cleanup = match wait {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(error)) => {
            Err(anyhow::Error::new(error).context("Could not reap suspended publication process"))
        }
        Err(error) => {
            Err(anyhow::Error::new(error)
                .context("Suspended publication process cleanup timed out"))
        }
    };
    match (kill, cleanup) {
        (_, Ok(())) => bail!(message.to_owned()),
        (Ok(()), Err(error)) => {
            bail!("{message}; suspended process cleanup failed: {error:#}")
        }
        (Err(kill_error), Err(wait_error)) => bail!(
            "{message}; suspended process termination failed: {kill_error}; reap failed: {wait_error:#}"
        ),
    }
}

#[cfg(windows)]
async fn force_reap_after_containment_error(child: &mut Child) -> Result<()> {
    let _ = child.start_kill();
    tokio::time::timeout(Duration::from_secs(5), child.wait())
        .await
        .context("Contained publication process fallback reap timed out")??;
    Ok(())
}

#[cfg(windows)]
async fn assigned_setup_failed(
    tree: &mut ProcessTree,
    child: &mut Child,
    message: &str,
) -> Result<ProcessTree> {
    let cleanup = tree.terminate_and_wait(child, Duration::from_secs(5)).await;
    if let Err(error) = cleanup {
        match force_reap_after_containment_error(child).await {
            Ok(()) => bail!("{message}; Job cleanup was not fully confirmed: {error:#}"),
            Err(fallback_error) => bail!(
                "{message}; contained cleanup failed: {error:#}; fallback reap failed: {fallback_error:#}"
            ),
        }
    }
    bail!(message.to_owned())
}

#[cfg(windows)]
impl ProcessTree {
    pub(super) fn disarm(&mut self) {
        self.stopped = true;
    }

    pub(super) async fn confirm_stopped(&mut self, timeout: Duration) -> Result<()> {
        use windows_sys::Win32::System::JobObjects::{
            JobObjectBasicAccountingInformation, QueryInformationJobObject,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
        };
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION =
                unsafe { std::mem::zeroed() };
            if unsafe {
                QueryInformationJobObject(
                    self.handle,
                    JobObjectBasicAccountingInformation,
                    &mut accounting as *mut _ as *mut _,
                    std::mem::size_of_val(&accounting) as u32,
                    std::ptr::null_mut(),
                )
            } == 0
            {
                bail!("Could not confirm publication process completion");
            }
            if accounting.ActiveProcesses == 0 {
                self.stopped = true;
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                bail!(
                    "Publication descendants retained process resources after command completion"
                );
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    pub(super) async fn terminate_and_wait(
        &mut self,
        child: &mut Child,
        timeout: Duration,
    ) -> Result<()> {
        use windows_sys::Win32::System::JobObjects::{
            JobObjectBasicAccountingInformation, QueryInformationJobObject, TerminateJobObject,
            JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
        };
        if self.stopped {
            return Ok(());
        }
        if unsafe { TerminateJobObject(self.handle, 1) } == 0 {
            bail!("Could not terminate the publication process Job; publication needs attention");
        }
        let deadline = tokio::time::Instant::now() + timeout;
        tokio::time::timeout_at(deadline, child.wait())
            .await
            .context("Publication parent termination timed out")??;
        loop {
            let mut accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION =
                unsafe { std::mem::zeroed() };
            if unsafe {
                QueryInformationJobObject(
                    self.handle,
                    JobObjectBasicAccountingInformation,
                    &mut accounting as *mut _ as *mut _,
                    std::mem::size_of_val(&accounting) as u32,
                    std::ptr::null_mut(),
                )
            } == 0
            {
                bail!("Could not confirm publication descendant termination; publication needs attention");
            }
            if accounting.ActiveProcesses == 0 {
                self.stopped = true;
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                bail!("Publication descendants did not stop before the deadline; publication needs attention");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

#[cfg(windows)]
impl Drop for ProcessTree {
    fn drop(&mut self) {
        unsafe {
            if !self.stopped {
                windows_sys::Win32::System::JobObjects::TerminateJobObject(self.handle, 1);
            }
            windows_sys::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;
    use std::{fs, path::PathBuf};

    const MARKER_ENV: &str = "ASSEMBLYWRIGHT_PUBLICATION_SUSPENDED_TEST_MARKER";

    fn fixture_command(marker: &PathBuf) -> Command {
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .arg("--ignored")
            .arg("publication_suspended_child_fixture")
            .env(MARKER_ENV, marker)
            .kill_on_drop(true);
        command
    }

    #[test]
    #[ignore]
    fn publication_suspended_child_fixture() {
        let Some(marker) = std::env::var_os(MARKER_ENV) else {
            return;
        };
        fs::write(marker, b"executed").unwrap();
    }

    #[tokio::test]
    async fn contained_process_executes_only_after_job_assignment_and_resume() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("executed.txt");
        let mut command = fixture_command(&marker);
        prepare_process_tree(&mut command);
        let mut child = command.spawn().unwrap();
        assert!(
            !marker.exists(),
            "suspended child executed before containment"
        );

        let mut tree = attach_process_tree(&mut child).await.unwrap();
        assert!(child.wait().await.unwrap().success());
        tree.confirm_stopped(Duration::from_secs(5)).await.unwrap();
        assert_eq!(fs::read(&marker).unwrap(), b"executed");
    }

    #[tokio::test]
    async fn containment_setup_failure_kills_and_reaps_without_running_child_code() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("must-not-exist.txt");
        let mut command = fixture_command(&marker);
        prepare_process_tree(&mut command);
        let mut child = command.spawn().unwrap();

        let error = attach_process_tree_inner(&mut child, true)
            .await
            .err()
            .expect("injected containment failure should be reported");
        assert!(error.to_string().contains("before resuming"));
        assert!(
            child.try_wait().unwrap().is_some(),
            "failed child was not reaped"
        );
        assert!(
            !marker.exists(),
            "uncontained child code executed during cleanup"
        );
    }
}

#[cfg(unix)]
pub(super) struct ProcessTree {
    group: i32,
    stopped: bool,
}

#[cfg(unix)]
pub(super) async fn attach_process_tree(child: &mut Child) -> Result<ProcessTree> {
    Ok(ProcessTree {
        group: child
            .id()
            .context("Publication process has no ID")?
            .try_into()?,
        stopped: false,
    })
}

#[cfg(unix)]
impl ProcessTree {
    pub(super) fn disarm(&mut self) {
        self.stopped = true;
    }

    pub(super) async fn confirm_stopped(&mut self, timeout: Duration) -> Result<()> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            if unsafe { libc::kill(-self.group, 0) } != 0 {
                if std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
                    self.stopped = true;
                    return Ok(());
                }
                bail!("Could not inspect the publication fixture process group");
            }
            if tokio::time::Instant::now() >= deadline {
                bail!(
                    "Publication descendants retained process resources after command completion"
                );
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    pub(super) async fn terminate_and_wait(
        &mut self,
        child: &mut Child,
        timeout: Duration,
    ) -> Result<()> {
        if self.stopped {
            return Ok(());
        }
        if unsafe { libc::kill(-self.group, libc::SIGKILL) } != 0
            && std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
        {
            bail!("Could not terminate the publication fixture process group");
        }
        let deadline = tokio::time::Instant::now() + timeout;
        tokio::time::timeout_at(deadline, child.wait())
            .await
            .context("Publication parent termination timed out")??;
        loop {
            if unsafe { libc::kill(-self.group, 0) } != 0 {
                if std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
                    self.stopped = true;
                    return Ok(());
                }
                bail!("Could not inspect the publication fixture process group");
            }
            if tokio::time::Instant::now() >= deadline {
                bail!("Publication fixture process group termination is unconfirmed");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}

#[cfg(unix)]
impl Drop for ProcessTree {
    fn drop(&mut self) {
        if !self.stopped {
            unsafe {
                libc::kill(-self.group, libc::SIGKILL);
            }
        }
    }
}
