use assemblywright_executor::runtime::{load_config, run_stdio};
use assemblywright_executor::startup::{parse_startup_args, StartupMode};

fn main() {
    let result = (|| match parse_startup_args(std::env::args_os().skip(1))? {
        StartupMode::Stdio { config, digest } => run_stdio(
            load_config(&config, digest)?,
            std::io::stdin().lock(),
            std::io::stdout().lock(),
        ),
        StartupMode::ServiceHost {
            service_name,
            config,
            digest,
        } => {
            #[cfg(windows)]
            {
                assemblywright_executor::windows_service_host::run(service_name, config, digest)
            }
            #[cfg(not(windows))]
            {
                let _ = (service_name, config, digest);
                Err(assemblywright_executor::runtime::RuntimeError::InvalidConfig)
            }
        }
    })();
    if result.is_err() {
        eprintln!("assemblywright-executor: runtime unavailable");
        std::process::exit(78);
    }
}
