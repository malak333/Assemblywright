use crate::runtime::{load_config, validate_service_bootstrap, RuntimeError};
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{mpsc, OnceLock};
use std::time::Duration;
use windows_service::define_windows_service;
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::service_dispatcher;

#[derive(Clone)]
struct HostConfig {
    service_name: String,
    config: PathBuf,
    digest: [u8; 32],
}
static CONFIG: OnceLock<HostConfig> = OnceLock::new();
define_windows_service!(ffi_service_main, service_main);

pub fn run(service_name: String, config: PathBuf, digest: [u8; 32]) -> Result<(), RuntimeError> {
    CONFIG
        .set(HostConfig {
            service_name: service_name.clone(),
            config,
            digest,
        })
        .map_err(|_| RuntimeError::InvalidConfig)?;
    service_dispatcher::start(service_name, ffi_service_main).map_err(|_| RuntimeError::Io)
}

fn service_main(_: Vec<OsString>) {
    let Some(config) = CONFIG.get().cloned() else {
        return;
    };
    let (tx, rx) = mpsc::channel();
    let handler = move |control| match control {
        ServiceControl::Stop | ServiceControl::Shutdown => tx
            .send(())
            .map(|_| ServiceControlHandlerResult::NoError)
            .unwrap_or(ServiceControlHandlerResult::Other(1)),
        ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
        _ => ServiceControlHandlerResult::NotImplemented,
    };
    let Ok(status) = service_control_handler::register(&config.service_name, handler) else {
        return;
    };
    let _ = status.set_service_status(service_status(
        ServiceState::StartPending,
        ServiceControlAccept::empty(),
        ServiceExitCode::NO_ERROR,
        1,
        Duration::from_secs(30),
    ));
    if load_config(&config.config, config.digest)
        .and_then(validate_service_bootstrap)
        .is_err()
    {
        let _ = status.set_service_status(service_status(
            ServiceState::Stopped,
            ServiceControlAccept::empty(),
            ServiceExitCode::ServiceSpecific(1),
            0,
            Duration::ZERO,
        ));
        return;
    }
    if status
        .set_service_status(service_status(
            ServiceState::Running,
            ServiceControlAccept::STOP | ServiceControlAccept::SHUTDOWN,
            ServiceExitCode::NO_ERROR,
            0,
            Duration::ZERO,
        ))
        .is_err()
    {
        return;
    }
    let _ = rx.recv();
    let _ = status.set_service_status(service_status(
        ServiceState::StopPending,
        ServiceControlAccept::empty(),
        ServiceExitCode::NO_ERROR,
        1,
        Duration::from_secs(15),
    ));
    let _ = status.set_service_status(service_status(
        ServiceState::Stopped,
        ServiceControlAccept::empty(),
        ServiceExitCode::NO_ERROR,
        0,
        Duration::ZERO,
    ));
}

fn service_status(
    state: ServiceState,
    controls: ServiceControlAccept,
    exit_code: ServiceExitCode,
    checkpoint: u32,
    wait_hint: Duration,
) -> ServiceStatus {
    ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: controls,
        exit_code,
        checkpoint,
        wait_hint,
        process_id: None,
    }
}
