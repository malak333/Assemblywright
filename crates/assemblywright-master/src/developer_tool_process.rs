//! Developer-only process ownership. Windows Jobs are the deployed boundary;
//! Unix process groups support native fixtures, not hostile descendant containment.
use anyhow::{bail, Context, Result};
use std::time::Duration;
use tokio::process::{Child, Command};

pub(super) fn prepare_process_tree(command: &mut Command) {
    #[cfg(unix)]
    command.process_group(0);
    #[cfg(not(unix))]
    let _ = command;
}

#[cfg(windows)]
pub(super) struct ProcessTree {
    handle: windows_sys::Win32::Foundation::HANDLE,
    stopped: bool,
}

// A Job handle owns a kernel object, is not thread-affine, and is never exposed
// outside this guard. Mutation/closure requires exclusive ownership of the guard.
#[cfg(windows)]
unsafe impl Send for ProcessTree {}

#[cfg(windows)]
pub(super) fn attach_process_tree(child: &Child) -> Result<ProcessTree> {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::{
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
                SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
                JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            },
            Threading::{
                OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_QUOTA,
                PROCESS_TERMINATE,
            },
        },
    };
    let pid = child.id().context("OpenCode process has no ID")?;
    let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
    if handle.is_null() {
        bail!("Could not create the OpenCode process Job");
    }
    let guard = ProcessTree {
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
        bail!("Could not configure the OpenCode process Job");
    }
    let process = unsafe {
        OpenProcess(
            PROCESS_SET_QUOTA | PROCESS_TERMINATE | PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            pid,
        )
    };
    if process.is_null() {
        bail!("Could not open the OpenCode process for Job assignment");
    }
    let assigned = unsafe { AssignProcessToJobObject(handle, process) };
    unsafe { CloseHandle(process) };
    if assigned == 0 {
        bail!("Could not assign OpenCode to its kill-on-close Job");
    }
    Ok(guard)
}

#[cfg(windows)]
impl ProcessTree {
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
            bail!("Could not terminate the OpenCode process Job; execution needs attention");
        }
        let deadline = tokio::time::Instant::now() + timeout;
        tokio::time::timeout_at(deadline, child.wait())
            .await
            .context("OpenCode parent termination timed out")??;
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
                bail!(
                    "Could not confirm OpenCode descendant termination; execution needs attention"
                );
            }
            if accounting.ActiveProcesses == 0 {
                self.stopped = true;
                return Ok(());
            }
            if tokio::time::Instant::now() >= deadline {
                bail!("OpenCode descendants did not stop before the deadline; execution needs attention");
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

#[cfg(unix)]
pub(super) struct ProcessTree {
    group: i32,
    stopped: bool,
}

#[cfg(unix)]
pub(super) fn attach_process_tree(child: &Child) -> Result<ProcessTree> {
    Ok(ProcessTree {
        group: child
            .id()
            .context("OpenCode process has no ID")?
            .try_into()?,
        stopped: false,
    })
}

#[cfg(unix)]
impl ProcessTree {
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
            bail!("Could not terminate the OpenCode fixture process group");
        }
        let deadline = tokio::time::Instant::now() + timeout;
        tokio::time::timeout_at(deadline, child.wait())
            .await
            .context("OpenCode parent termination timed out")??;
        loop {
            if unsafe { libc::kill(-self.group, 0) } != 0 {
                if std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
                    self.stopped = true;
                    return Ok(());
                }
                bail!("Could not inspect the OpenCode fixture process group");
            }
            if tokio::time::Instant::now() >= deadline {
                bail!("OpenCode fixture process group termination is unconfirmed");
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
