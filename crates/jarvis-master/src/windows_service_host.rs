use anyhow::{bail, Context};
use serde_json::{json, Value};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use windows_service::service::{
    ServiceAccess, ServiceAction, ServiceActionType, ServiceControl, ServiceControlAccept,
    ServiceErrorControl, ServiceExitCode, ServiceFailureActions, ServiceFailureResetPeriod,
    ServiceInfo, ServiceStartType, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_dispatcher;
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};
use windows_service::{define_windows_service, Error as WindowsServiceError};

const SERVICE_DISPLAY_NAME: &str = "Jarvis Developer Mode Master";
const SERVICE_DESCRIPTION: &str =
    "Headless Jarvis Developer Mode Windows master with durable reconciliation.";
const SERVICE_WAIT_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone)]
pub struct ServiceRuntimeConfig {
    pub service_name: String,
    pub data_dir: PathBuf,
    pub bind: std::net::SocketAddr,
    pub remote_bind: Option<std::net::SocketAddr>,
    pub service_identity: String,
}

#[derive(Debug, Clone, Copy)]
pub enum ServiceRuntimeControl {
    Stop,
    EnterMaintenance,
    ExitMaintenance,
}

static SERVICE_RUNTIME_CONFIG: OnceLock<ServiceRuntimeConfig> = OnceLock::new();

define_windows_service!(ffi_service_main, service_main);

pub fn run_dispatcher(config: ServiceRuntimeConfig) -> anyhow::Result<()> {
    validate_service_name(&config.service_name)?;
    let service_name = config.service_name.clone();
    SERVICE_RUNTIME_CONFIG
        .set(config)
        .map_err(|_| anyhow::anyhow!("Windows service runtime configuration was already set"))?;
    service_dispatcher::start(service_name, ffi_service_main)
        .context("register Jarvis master with the Windows Service Control Manager")
}

fn service_main(_arguments: Vec<OsString>) {
    let Some(config) = SERVICE_RUNTIME_CONFIG.get().cloned() else {
        return;
    };
    let (control_tx, control_rx) = mpsc::unbounded_channel();
    let event_handler = move |control_event| -> ServiceControlHandlerResult {
        let signal = match control_event {
            ServiceControl::Stop | ServiceControl::Shutdown => Some(ServiceRuntimeControl::Stop),
            ServiceControl::Pause => Some(ServiceRuntimeControl::EnterMaintenance),
            ServiceControl::Continue => Some(ServiceRuntimeControl::ExitMaintenance),
            ServiceControl::Interrogate => return ServiceControlHandlerResult::NoError,
            _ => return ServiceControlHandlerResult::NotImplemented,
        };
        if signal.is_some_and(|signal| control_tx.send(signal).is_err()) {
            return ServiceControlHandlerResult::Other(1);
        }
        ServiceControlHandlerResult::NoError
    };

    let Ok(status_handle) = service_control_handler::register(&config.service_name, event_handler)
    else {
        return;
    };
    let _ = status_handle.set_service_status(service_status(
        ServiceState::StartPending,
        ServiceControlAccept::empty(),
        ServiceExitCode::NO_ERROR,
        1,
        Duration::from_secs(30),
    ));

    let result = super::run_windows_service_runtime(config, control_rx, status_handle);
    let exit_code = if result.is_ok() {
        ServiceExitCode::NO_ERROR
    } else {
        ServiceExitCode::ServiceSpecific(1)
    };
    let _ = status_handle.set_service_status(service_status(
        ServiceState::Stopped,
        ServiceControlAccept::empty(),
        exit_code,
        0,
        Duration::default(),
    ));
}

pub fn install(
    service_name: &str,
    data_dir: &Path,
    bind: std::net::SocketAddr,
    remote_bind: Option<std::net::SocketAddr>,
    account_name: Option<&str>,
    account_password: Option<&str>,
) -> anyhow::Result<Value> {
    validate_service_name(service_name)?;
    let executable_path = std::env::current_exe()
        .context("resolve the Jarvis master executable")?
        .canonicalize()
        .context("canonicalize the Jarvis master executable")?;
    if !executable_path.is_file() {
        bail!("Jarvis master service executable is not a regular file");
    }
    let data_dir = data_dir
        .canonicalize()
        .context("canonicalize the initialized Jarvis master data directory")?;
    let identity_label = account_name.unwrap_or("LocalSystem");
    let mut launch_arguments = vec![
        OsString::from("--data-dir"),
        data_dir.as_os_str().to_os_string(),
        OsString::from("service-run"),
        OsString::from("--service-name"),
        OsString::from(service_name),
        OsString::from("--bind"),
        OsString::from(bind.to_string()),
        OsString::from("--service-identity"),
        OsString::from(identity_label),
    ];
    if let Some(remote_bind) = remote_bind {
        launch_arguments.push(OsString::from("--remote-bind"));
        launch_arguments.push(OsString::from(remote_bind.to_string()));
    }
    let service_info = ServiceInfo {
        name: OsString::from(service_name),
        display_name: OsString::from(SERVICE_DISPLAY_NAME),
        service_type: ServiceType::OWN_PROCESS,
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: executable_path.clone(),
        launch_arguments,
        dependencies: vec![],
        account_name: account_name.map(OsString::from),
        account_password: account_password.map(OsString::from),
    };

    let manager = ServiceManager::local_computer(
        None::<&str>,
        ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE,
    )
    .context("open the local Windows Service Control Manager")?;
    let access = ServiceAccess::QUERY_CONFIG
        | ServiceAccess::QUERY_STATUS
        | ServiceAccess::CHANGE_CONFIG
        | ServiceAccess::START
        | ServiceAccess::STOP
        | ServiceAccess::PAUSE_CONTINUE
        | ServiceAccess::DELETE;
    let service = manager
        .create_service(&service_info, access)
        .context("install the Jarvis master Windows service")?;
    let configure = || -> anyhow::Result<()> {
        service
            .set_description(SERVICE_DESCRIPTION)
            .context("set the Jarvis master service description")?;
        service
            .update_failure_actions(ServiceFailureActions {
                reset_period: ServiceFailureResetPeriod::After(Duration::from_secs(24 * 60 * 60)),
                reboot_msg: None,
                command: None,
                actions: Some(vec![
                    ServiceAction {
                        action_type: ServiceActionType::Restart,
                        delay: Duration::from_secs(5),
                    },
                    ServiceAction {
                        action_type: ServiceActionType::Restart,
                        delay: Duration::from_secs(15),
                    },
                    ServiceAction {
                        action_type: ServiceActionType::Restart,
                        delay: Duration::from_secs(60),
                    },
                    ServiceAction {
                        action_type: ServiceActionType::None,
                        delay: Duration::default(),
                    },
                ]),
            })
            .context("configure bounded Jarvis master service recovery")?;
        service
            .set_failure_actions_on_non_crash_failures(true)
            .context("enable Jarvis master recovery for non-zero service exits")?;
        Ok(())
    };
    if let Err(error) = configure() {
        let _ = service.delete();
        return Err(error).context("roll back incomplete Jarvis master service installation");
    }

    Ok(json!({
        "status": "service_installed",
        "service_name": service_name,
        "display_name": SERVICE_DISPLAY_NAME,
        "start_type": "automatic",
        "service_identity": identity_label,
        "executable_path": executable_path,
        "data_dir": data_dir,
        "bind": bind,
        "remote_bind": remote_bind,
        "recovery": {
            "reset_after_seconds": 86400,
            "restart_delays_seconds": [5, 15, 60],
            "stop_after_bounded_retries": true
        }
    }))
}

pub fn start(service_name: &str) -> anyhow::Result<Value> {
    let service = open_service(
        service_name,
        ServiceAccess::START | ServiceAccess::QUERY_STATUS,
    )?;
    let current = service
        .query_status()
        .context("query service before start")?;
    if current.current_state == ServiceState::Stopped {
        service
            .start::<&OsStr>(&[])
            .context("start Jarvis master service")?;
    }
    let status = wait_for_any_state(
        &service,
        &[ServiceState::Running, ServiceState::Paused],
        SERVICE_WAIT_TIMEOUT,
    )?;
    Ok(status_receipt(
        "service_started",
        service_name,
        &status,
        None,
    ))
}

pub fn stop(service_name: &str) -> anyhow::Result<Value> {
    let service = open_service(
        service_name,
        ServiceAccess::STOP | ServiceAccess::QUERY_STATUS,
    )?;
    let current = service
        .query_status()
        .context("query service before stop")?;
    if current.current_state != ServiceState::Stopped
        && current.current_state != ServiceState::StopPending
    {
        service.stop().context("stop Jarvis master service")?;
    }
    let status = wait_for_any_state(&service, &[ServiceState::Stopped], SERVICE_WAIT_TIMEOUT)?;
    Ok(status_receipt(
        "service_stopped",
        service_name,
        &status,
        None,
    ))
}

pub fn pause(service_name: &str) -> anyhow::Result<Value> {
    let service = open_service(
        service_name,
        ServiceAccess::PAUSE_CONTINUE | ServiceAccess::QUERY_STATUS,
    )?;
    let current = service
        .query_status()
        .context("query service before maintenance")?;
    if current.current_state != ServiceState::Paused {
        if current.current_state != ServiceState::Running {
            bail!("service must be running before entering maintenance mode");
        }
        service
            .pause()
            .context("enter Jarvis master maintenance mode")?;
    }
    let status = wait_for_any_state(&service, &[ServiceState::Paused], SERVICE_WAIT_TIMEOUT)?;
    Ok(status_receipt(
        "maintenance_entered",
        service_name,
        &status,
        Some(true),
    ))
}

pub fn resume(service_name: &str) -> anyhow::Result<Value> {
    let service = open_service(
        service_name,
        ServiceAccess::PAUSE_CONTINUE | ServiceAccess::QUERY_STATUS,
    )?;
    let current = service
        .query_status()
        .context("query service before resume")?;
    if current.current_state != ServiceState::Running {
        if current.current_state != ServiceState::Paused {
            bail!("service must be paused before exiting maintenance mode");
        }
        service
            .resume()
            .context("exit Jarvis master maintenance mode")?;
    }
    let status = wait_for_any_state(&service, &[ServiceState::Running], SERVICE_WAIT_TIMEOUT)?;
    Ok(status_receipt(
        "maintenance_exited",
        service_name,
        &status,
        Some(false),
    ))
}

pub fn status(service_name: &str) -> anyhow::Result<Value> {
    let service = open_service(
        service_name,
        ServiceAccess::QUERY_STATUS | ServiceAccess::QUERY_CONFIG,
    )?;
    let status = service
        .query_status()
        .context("query Jarvis master service status")?;
    let config = service
        .query_config()
        .context("query Jarvis master service config")?;
    Ok(
        status_receipt("service_status", service_name, &status, None)
            .as_object()
            .cloned()
            .map(|mut receipt| {
                receipt.insert(
                    "service_identity".to_string(),
                    json!(config
                        .account_name
                        .as_ref()
                        .map(|value| value.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "LocalSystem".to_string())),
                );
                receipt.insert(
                    "start_type".to_string(),
                    json!(format!("{:?}", config.start_type)),
                );
                Value::Object(receipt)
            })
            .expect("status receipt is an object"),
    )
}

pub fn recover(service_name: &str) -> anyhow::Result<Value> {
    let _ = stop(service_name)?;
    let started = start(service_name)?;
    Ok(json!({
        "status": "service_recovered",
        "service_name": service_name,
        "service": started,
        "reconciliation": "durable startup reconciliation completed before runtime health becomes ready"
    }))
}

pub fn uninstall(service_name: &str) -> anyhow::Result<Value> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .context("open the local Windows Service Control Manager")?;
    let service = manager
        .open_service(
            service_name,
            ServiceAccess::STOP | ServiceAccess::QUERY_STATUS | ServiceAccess::DELETE,
        )
        .context("open the installed Jarvis master service")?;
    let current = service
        .query_status()
        .context("query service before uninstall")?;
    if current.current_state != ServiceState::Stopped
        && current.current_state != ServiceState::StopPending
    {
        service.stop().context("stop service before uninstall")?;
    }
    if current.current_state != ServiceState::Stopped {
        let _ = wait_for_any_state(&service, &[ServiceState::Stopped], SERVICE_WAIT_TIMEOUT)?;
    }
    service
        .delete()
        .context("mark Jarvis master service for deletion")?;
    drop(service);

    let deadline = Instant::now() + SERVICE_WAIT_TIMEOUT;
    while Instant::now() < deadline {
        match manager.open_service(service_name, ServiceAccess::QUERY_STATUS) {
            Err(WindowsServiceError::Winapi(error))
                if error.raw_os_error()
                    == Some(
                        windows_sys::Win32::Foundation::ERROR_SERVICE_DOES_NOT_EXIST as i32,
                    ) =>
            {
                return Ok(json!({
                    "status": "service_uninstalled",
                    "service_name": service_name,
                    "master_data_preserved": true
                }));
            }
            _ => thread::sleep(Duration::from_millis(200)),
        }
    }
    bail!("service was marked for deletion but did not disappear before the timeout")
}

fn open_service(
    service_name: &str,
    access: ServiceAccess,
) -> anyhow::Result<windows_service::service::Service> {
    validate_service_name(service_name)?;
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .context("open the local Windows Service Control Manager")?;
    manager
        .open_service(service_name, access)
        .context("open the installed Jarvis master service")
}

fn wait_for_any_state(
    service: &windows_service::service::Service,
    expected: &[ServiceState],
    timeout: Duration,
) -> anyhow::Result<ServiceStatus> {
    let deadline = Instant::now() + timeout;
    loop {
        let status = service
            .query_status()
            .context("query service transition status")?;
        if expected.contains(&status.current_state) {
            return Ok(status);
        }
        if status.current_state == ServiceState::Stopped
            && !expected.contains(&ServiceState::Stopped)
        {
            bail!("service stopped before reaching the requested state: {status:?}");
        }
        if Instant::now() >= deadline {
            bail!("service did not reach the requested state before the timeout: {status:?}");
        }
        thread::sleep(Duration::from_millis(200));
    }
}

pub fn service_status(
    current_state: ServiceState,
    controls_accepted: ServiceControlAccept,
    exit_code: ServiceExitCode,
    checkpoint: u32,
    wait_hint: Duration,
) -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state,
        controls_accepted,
        exit_code,
        checkpoint,
        wait_hint,
        process_id: None,
    }
}

fn status_receipt(
    receipt_status: &str,
    service_name: &str,
    status: &ServiceStatus,
    maintenance_active: Option<bool>,
) -> Value {
    json!({
        "status": receipt_status,
        "service_name": service_name,
        "scm_state": format!("{:?}", status.current_state).to_ascii_lowercase(),
        "process_id": status.process_id,
        "maintenance_active": maintenance_active,
        "exit_code": format!("{:?}", status.exit_code),
    })
}

fn validate_service_name(service_name: &str) -> anyhow::Result<()> {
    if service_name.is_empty()
        || service_name.len() > 64
        || !service_name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        bail!("service name must be 1-64 ASCII letters, digits, hyphens, or underscores");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_name_is_bounded_and_shell_neutral() {
        assert!(validate_service_name("JarvisMaster-test_1").is_ok());
        assert!(validate_service_name("").is_err());
        assert!(validate_service_name("Jarvis Master").is_err());
        assert!(validate_service_name("JarvisMaster;Remove-Item").is_err());
        assert!(validate_service_name(&"x".repeat(65)).is_err());
    }
}
