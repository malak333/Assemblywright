use assemblywright_broker::runtime::{config_path_from_args, load_config, run_stdio};

fn main() {
    let result = (|| {
        let (path, digest) = config_path_from_args()?;
        let config = load_config(&path, digest)?;
        run_stdio(config, std::io::stdin().lock(), std::io::stdout().lock())
    })();
    if result.is_err() {
        eprintln!("assemblywright-broker: runtime unavailable");
        std::process::exit(78);
    }
}
