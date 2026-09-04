use crate::ipc::{load_ack_seed, InertExecutorIpc};
use crate::runtime::{load_config, validate_service_bootstrap, RuntimeError};
use assemblywright_protocol::windows_execution_pipe::WindowsExecutionPipeError;
use ed25519_dalek::VerifyingKey;
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
    let Ok(loaded) = load_config(&config.config, config.digest) else {
        let _ = status.set_service_status(service_status(
            ServiceState::Stopped,
            ServiceControlAccept::empty(),
            ServiceExitCode::ServiceSpecific(1),
            0,
            Duration::ZERO,
        ));
        return;
    };
    if validate_service_bootstrap(loaded.clone()).is_err() {
        let _ = status.set_service_status(service_status(
            ServiceState::Stopped,
            ServiceControlAccept::empty(),
            ServiceExitCode::ServiceSpecific(1),
            0,
            Duration::ZERO,
        ));
        return;
    }
    let expects_ipc = loaded.ipc.is_some();
    let ipc_bootstrap = loaded.ipc.clone().and_then(|ipc| {
        let seed = load_ack_seed(&ipc.ack_seed_path).ok()?;
        let authority_key = VerifyingKey::from_bytes(&loaded.authority_verifying_key).ok()?;
        let runtime = InertExecutorIpc::open(
            &ipc.durable_state_path,
            loaded.executor_id,
            loaded.bound_authority_revision,
            loaded.next_request_sequence,
            loaded.authority_key_id,
            authority_key,
            ipc.ack_key_id.clone(),
            &*seed,
        )
        .ok()?;
        Some((ipc, runtime))
    });
    if expects_ipc && ipc_bootstrap.is_none() {
        let _ = status.set_service_status(service_status(
            ServiceState::Stopped,
            ServiceControlAccept::empty(),
            ServiceExitCode::ServiceSpecific(2),
            0,
            Duration::ZERO,
        ));
        return;
    }
    let (ipc_done_tx, ipc_done_rx) = mpsc::channel();
    if let Some((ipc, mut runtime)) = ipc_bootstrap {
        std::thread::spawn(move || {
            let result: Result<(), ()> = loop {
                let handled = crate::windows_execution_ipc::serve_broker_once(
                    &ipc.pipe_name,
                    &ipc.executor_service_sid,
                    &ipc.expected_broker_service_sid,
                    |bytes| {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_err(|_| WindowsExecutionPipeError::InvalidFrame)?
                            .as_millis() as u64;
                        let ack = runtime
                            .handle(bytes, now)
                            .map_err(|_| WindowsExecutionPipeError::InvalidFrame)?;
                        ack.encode_frame()
                            .map_err(|_| WindowsExecutionPipeError::InvalidFrame)
                    },
                );
                if handled.is_err() {
                    break Err(());
                }
            };
            let _ = ipc_done_tx.send(result);
        });
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
    loop {
        if rx.recv_timeout(Duration::from_millis(100)).is_ok() {
            break;
        }
        if ipc_done_rx.try_recv().is_ok() {
            let _ = status.set_service_status(service_status(
                ServiceState::Stopped,
                ServiceControlAccept::empty(),
                ServiceExitCode::ServiceSpecific(3),
                0,
                Duration::ZERO,
            ));
            return;
        }
    }
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
