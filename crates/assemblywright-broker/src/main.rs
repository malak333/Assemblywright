use assemblywright_broker::runtime::{load_config, run_stdio};
use assemblywright_broker::startup::{parse_startup_args, StartupMode};

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
                assemblywright_broker::windows_service_host::run(service_name, config, digest)
            }
            #[cfg(not(windows))]
            {
                let _ = (service_name, config, digest);
                Err(assemblywright_broker::runtime::RuntimeError::InvalidConfig)
            }
        }
    })();
    if result.is_err() {
        eprintln!("assemblywright-broker: runtime unavailable");
        std::process::exit(78);
    }
}
