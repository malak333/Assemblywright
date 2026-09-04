#[cfg(not(windows))]
fn main() {}

#[cfg(windows)]
mod windows_fixture {
    use assemblywright_master::execution_ipc::{
        InertWindowsExecutionIpcFoundation, WindowsExecutionIpcBinding,
    };
    use assemblywright_protocol::{WindowsBrokerForwardedAcks, WindowsExecutionAck};
    use ed25519_dalek::SigningKey;
    use serde::Serialize;
    use sha2::Digest;
    use std::ffi::OsString;
    use std::path::PathBuf;
    use std::sync::OnceLock;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};
    use uuid::Uuid;
    use windows_service::define_windows_service;
    use windows_service::service::{
        ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus, ServiceType,
    };
    use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
    use windows_service::service_dispatcher;

    #[derive(Clone)]
    struct Config {
        service_name: String,
        pipe_name: String,
        broker_sid: String,
        receipt: PathBuf,
        scenario: String,
    }

    #[derive(Serialize)]
    struct Receipt {
        schema_version: u16,
        status: &'static str,
        scenario: String,
        broker_ack_id: Option<Uuid>,
        executor_ack_id: Option<Uuid>,
        broker_frame_sha256: Option<[u8; 32]>,
        executor_frame_sha256: Option<[u8; 32]>,
        issued_at_ms: u64,
        effects_applied: u32,
        rejected_before_ack: bool,
    }

    static CONFIG: OnceLock<Config> = OnceLock::new();
    define_windows_service!(fixture_service_main, service_main);

    pub fn main() {
        let config = parse().unwrap_or_else(|| std::process::exit(78));
        let name = config.service_name.clone();
        CONFIG
            .set(config)
            .unwrap_or_else(|_| std::process::exit(78));
        service_dispatcher::start(name, fixture_service_main)
            .unwrap_or_else(|_| std::process::exit(78));
    }

    fn parse() -> Option<Config> {
        let mut args = std::env::args_os().skip(1);
        if args.next()?.to_str()? != "--service-name" {
            return None;
        }
        let service_name = args.next()?.into_string().ok()?;
        if !service_name
            .strip_prefix("AssemblywrightMasterE2E")
            .is_some_and(|tail| {
                !tail.is_empty()
                    && tail.len() <= 32
                    && tail.bytes().all(|byte| byte.is_ascii_alphanumeric())
            })
            || args.next()?.to_str()? != "--pipe"
        {
            return None;
        }
        let pipe_name = args.next()?.into_string().ok()?;
        if args.next()?.to_str()? != "--broker-sid" {
            return None;
        }
        let broker_sid = args.next()?.into_string().ok()?;
        if args.next()?.to_str()? != "--receipt" {
            return None;
        }
        let receipt = PathBuf::from(args.next()?);
        if args.next()?.to_str()? != "--scenario" {
            return None;
        }
        let scenario = args.next()?.into_string().ok()?;
        if args.next().is_some()
            || !receipt.is_absolute()
            || !matches!(
                scenario.as_str(),
                "valid"
                    | "delayed_write"
                    | "replay"
                    | "unsigned"
                    | "tampered"
                    | "gap"
                    | "stale"
                    | "stale_authority"
                    | "wrong_sid"
                    | "localservice_dacl_denied"
            )
        {
            return None;
        }
        Some(Config {
            service_name,
            pipe_name,
            broker_sid,
            receipt,
            scenario,
        })
    }

    fn service_main(_: Vec<OsString>) {
        let Some(config) = CONFIG.get().cloned() else {
            return;
        };
        let Ok(status) = service_control_handler::register(&config.service_name, |_| {
            ServiceControlHandlerResult::NotImplemented
        }) else {
            return;
        };
        let _ = status.set_service_status(service_status(ServiceState::Running, 0));
        let result = run(&config);
        let exit = if result.is_ok() {
            ServiceExitCode::NO_ERROR
        } else {
            ServiceExitCode::ServiceSpecific(1)
        };
        let _ = status.set_service_status(ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: ServiceState::Stopped,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: exit,
            checkpoint: 0,
            wait_hint: Duration::ZERO,
            process_id: None,
        });
    }

    fn run(config: &Config) -> Result<(), ()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ())?
            .as_millis() as u64;
        let authority = SigningKey::from_bytes(&[81; 32]);
        let broker_key = SigningKey::from_bytes(&[82; 32]);
        let executor_key = SigningKey::from_bytes(&[83; 32]);
        let authority_revision = if config.scenario == "stale_authority" {
            2
        } else {
            1
        };
        let binding = WindowsExecutionIpcBinding {
            master_id: Uuid::from_u128(106),
            broker_id: Uuid::from_u128(102),
            executor_id: Uuid::from_u128(103),
            session_id: Uuid::from_u128(104),
            session_revision: 1,
            child_epoch_id: Uuid::from_u128(105),
            child_epoch_revision: 1,
            feature_lifecycle_revision: 1,
            authority_revision,
            authority_key_id: "fixture-master-ipc-v1".into(),
            broker_ack_key_id: "fixture-broker-ack-v1".into(),
            broker_ack_key: broker_key.verifying_key(),
            executor_ack_key_id: "fixture-executor-ack-v1".into(),
            executor_ack_key: executor_key.verifying_key(),
        };
        let foundation =
            InertWindowsExecutionIpcFoundation::new(binding, authority).map_err(|_| ())?;
        let broker_sequence = if config.scenario == "gap" { 2 } else { 1 };
        let prior = if config.scenario == "replay" {
            Some(
                serde_json::from_slice::<serde_json::Value>(
                    &std::fs::read(&config.receipt).map_err(|_| ())?,
                )
                .map_err(|_| ())?,
            )
        } else {
            None
        };
        let replay_issued = prior
            .as_ref()
            .and_then(|value| value.get("issued_at_ms"))
            .and_then(|value| value.as_u64());
        let (issued, expires) = if config.scenario == "stale" {
            (now.saturating_sub(60_000), now.saturating_sub(30_000))
        } else {
            let issued = replay_issued.unwrap_or(now);
            (issued, issued + 30_000)
        };
        let (mut broker, mut executor) = foundation
            .sign_dispatch_validation(broker_sequence, 1, issued, expires)
            .map_err(|_| ())?;
        // Deterministic identities make the exact frame replayable across one
        // deliberate service restart in the native fixture.
        executor.frame_id = Uuid::from_u128(109);
        executor.nonce = Uuid::from_u128(110);
        executor.signature.clear();
        executor
            .sign(&SigningKey::from_bytes(&[81; 32]))
            .map_err(|_| ())?;
        broker.forwarded_executor_frame = executor.encode_frame().map_err(|_| ())?;
        broker.forwarded_executor_frame_sha256 =
            sha2::Sha256::digest(&broker.forwarded_executor_frame).into();
        broker.frame_id = Uuid::from_u128(107);
        broker.nonce = Uuid::from_u128(108);
        broker.signature.clear();
        broker
            .sign(&SigningKey::from_bytes(&[81; 32]))
            .map_err(|_| ())?;
        let mut request = broker.encode_frame().map_err(|_| ())?;
        match config.scenario.as_str() {
            "unsigned" => {
                broker.signature.clear();
                request = serde_json::to_vec(&broker).map_err(|_| ())?;
            }
            "tampered" => {
                broker.authority_revision += 1;
                request = serde_json::to_vec(&broker).map_err(|_| ())?;
            }
            _ => {}
        }
        if config.scenario == "localservice_dacl_denied" {
            use std::os::windows::fs::OpenOptionsExt;
            const WRITE_DAC: u32 = 0x0004_0000;
            if std::fs::OpenOptions::new()
                .access_mode(WRITE_DAC)
                .open(std::path::Path::new(&config.pipe_name))
                .is_ok()
            {
                return Err(());
            }
        }
        let mut response = None;
        for _ in 0..100 {
            let exchange = if config.scenario == "delayed_write" {
                assemblywright_master::windows_execution_ipc::transact_service_with_write_delay_for_native_test(
                    &config.pipe_name,
                    &config.broker_sid,
                    &request,
                    Duration::from_millis(500),
                )
            } else {
                assemblywright_master::windows_execution_ipc::transact_service(
                    &config.pipe_name,
                    &config.broker_sid,
                    &request,
                )
            };
            match exchange {
                Ok(bytes) => {
                    response = Some(bytes);
                    break;
                }
                Err(_) => std::thread::sleep(Duration::from_millis(50)),
            }
        }
        let expects_rejection = matches!(
            config.scenario.as_str(),
            "unsigned"
                | "tampered"
                | "gap"
                | "stale"
                | "stale_authority"
                | "wrong_sid"
                | "localservice_dacl_denied"
        );
        let receipt = if expects_rejection {
            if response.is_some() {
                return Err(());
            }
            Receipt {
                schema_version: 1,
                status: "windows_execution_ipc_rejected",
                scenario: config.scenario.clone(),
                broker_ack_id: None,
                executor_ack_id: None,
                broker_frame_sha256: None,
                executor_frame_sha256: None,
                issued_at_ms: issued,
                effects_applied: 0,
                rejected_before_ack: true,
            }
        } else {
            let response =
                WindowsBrokerForwardedAcks::decode_frame(&response.ok_or(())?).map_err(|_| ())?;
            foundation
                .verify_ack(&broker, &response.broker_ack)
                .map_err(|_| ())?;
            let executor_ack =
                WindowsExecutionAck::decode_frame(&response.executor_ack_frame).map_err(|_| ())?;
            foundation
                .verify_ack(&executor, &executor_ack)
                .map_err(|_| ())?;
            if config.scenario == "replay" {
                let prior = prior.as_ref().ok_or(())?;
                let broker_ack_id = response.broker_ack.ack_id.to_string();
                let executor_ack_id = executor_ack.ack_id.to_string();
                if prior.get("broker_ack_id").and_then(|v| v.as_str())
                    != Some(broker_ack_id.as_str())
                    || prior.get("executor_ack_id").and_then(|v| v.as_str())
                        != Some(executor_ack_id.as_str())
                {
                    return Err(());
                }
            }
            Receipt {
                schema_version: 1,
                status: "windows_execution_ipc_inert_roundtrip_passed",
                scenario: config.scenario.clone(),
                broker_ack_id: Some(response.broker_ack.ack_id),
                executor_ack_id: Some(executor_ack.ack_id),
                broker_frame_sha256: Some(broker.canonical_sha256().map_err(|_| ())?),
                executor_frame_sha256: Some(executor.canonical_sha256().map_err(|_| ())?),
                issued_at_ms: issued,
                effects_applied: response.broker_ack.effects_applied + executor_ack.effects_applied,
                rejected_before_ack: false,
            }
        };
        let bytes = serde_json::to_vec(&receipt).map_err(|_| ())?;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&config.receipt)
            .map_err(|_| ())?;
        use std::io::Write;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| ())
    }

    fn service_status(state: ServiceState, checkpoint: u32) -> ServiceStatus {
        ServiceStatus {
            service_type: ServiceType::OWN_PROCESS,
            current_state: state,
            controls_accepted: ServiceControlAccept::empty(),
            exit_code: ServiceExitCode::NO_ERROR,
            checkpoint,
            wait_hint: Duration::ZERO,
            process_id: None,
        }
    }
}

#[cfg(windows)]
fn main() {
    windows_fixture::main();
}
