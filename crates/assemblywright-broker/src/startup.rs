use crate::runtime::{parse_sha256, RuntimeError};
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

#[derive(Debug, PartialEq, Eq)]
pub enum StartupMode {
    Stdio {
        config: PathBuf,
        digest: [u8; 32],
    },
    ServiceHost {
        service_name: String,
        config: PathBuf,
        digest: [u8; 32],
    },
}

pub fn parse_startup_args(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<StartupMode, RuntimeError> {
    let mut args = arguments.into_iter();
    let first = args.next().ok_or(RuntimeError::InvalidConfig)?;
    let service_name = if first == OsStr::new("--service-host") {
        if args.next().as_deref() != Some(OsStr::new("--service-name")) {
            return Err(RuntimeError::InvalidConfig);
        }
        let name = args
            .next()
            .and_then(|v| v.into_string().ok())
            .ok_or(RuntimeError::InvalidConfig)?;
        validate_service_name(&name)?;
        Some(name)
    } else if first == OsStr::new("--config") {
        None
    } else {
        return Err(RuntimeError::InvalidConfig);
    };
    if service_name.is_some() && args.next().as_deref() != Some(OsStr::new("--config")) {
        return Err(RuntimeError::InvalidConfig);
    }
    let config = args
        .next()
        .map(PathBuf::from)
        .ok_or(RuntimeError::InvalidConfig)?;
    if args.next().as_deref() != Some(OsStr::new("--config-sha256")) {
        return Err(RuntimeError::InvalidConfig);
    }
    let digest = args
        .next()
        .and_then(|v| v.into_string().ok())
        .ok_or(RuntimeError::InvalidConfig)?;
    if args.next().is_some() || !config.is_absolute() {
        return Err(RuntimeError::InvalidConfig);
    }
    let digest = parse_sha256(&digest)?;
    Ok(match service_name {
        Some(service_name) => StartupMode::ServiceHost {
            service_name,
            config,
            digest,
        },
        None => StartupMode::Stdio { config, digest },
    })
}

fn validate_service_name(name: &str) -> Result<(), RuntimeError> {
    let fixture = name.strip_prefix("AssemblywrightBrokerE2E");
    if name == "AssemblywrightBroker"
        || fixture.is_some_and(|tail| {
            !tail.is_empty() && tail.len() <= 32 && tail.bytes().all(|b| b.is_ascii_alphanumeric())
        })
    {
        Ok(())
    } else {
        Err(RuntimeError::InvalidConfig)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }
    #[test]
    fn parses_exact_stdio_and_service_modes() {
        let digest = "00".repeat(32);
        let path = if cfg!(windows) {
            "C:\\a.json"
        } else {
            "/a.json"
        };
        assert!(matches!(
            parse_startup_args(args(&["--config", path, "--config-sha256", &digest])),
            Ok(StartupMode::Stdio { .. })
        ));
        assert!(matches!(
            parse_startup_args(args(&[
                "--service-host",
                "--service-name",
                "AssemblywrightBrokerE2Eabc",
                "--config",
                path,
                "--config-sha256",
                &digest
            ])),
            Ok(StartupMode::ServiceHost { .. })
        ));
    }
    #[test]
    fn rejects_unknown_name_reordering_extra_and_bad_digest() {
        let digest = "00".repeat(32);
        let path = if cfg!(windows) { "C:\\a" } else { "/a" };
        for values in [
            vec![
                "--service-host",
                "--service-name",
                "AssemblywrightBrokerBad",
                "--config",
                path,
                "--config-sha256",
                &digest,
            ],
            vec![
                "--service-host",
                "--config",
                path,
                "--service-name",
                "AssemblywrightBroker",
                "--config-sha256",
                &digest,
            ],
            vec![
                "--service-host",
                "--service-name",
                "AssemblywrightBroker",
                "--config",
                path,
                "--config-sha256",
                &digest,
                "extra",
            ],
            vec![
                "--service-host",
                "--service-name",
                "AssemblywrightBroker",
                "--config",
                path,
                "--config-sha256",
                "x",
            ],
        ] {
            assert_eq!(
                parse_startup_args(args(&values)),
                Err(RuntimeError::InvalidConfig)
            );
        }
    }

    #[test]
    fn service_name_and_config_boundaries_are_exact() {
        let digest = "00".repeat(32);
        let path = if cfg!(windows) { "C:\\a" } else { "/a" };
        let maximum = format!("AssemblywrightBrokerE2E{}", "a".repeat(32));
        assert!(matches!(
            parse_startup_args(args(&[
                "--service-host",
                "--service-name",
                &maximum,
                "--config",
                path,
                "--config-sha256",
                &digest,
            ])),
            Ok(StartupMode::ServiceHost { .. })
        ));

        for name in [
            "",
            "AssemblywrightBrokerE2E",
            "AssemblywrightBrokerE2Ebad-name",
            &format!("AssemblywrightBrokerE2E{}", "a".repeat(33)),
        ] {
            assert_eq!(
                parse_startup_args(args(&[
                    "--service-host",
                    "--service-name",
                    name,
                    "--config",
                    path,
                    "--config-sha256",
                    &digest,
                ])),
                Err(RuntimeError::InvalidConfig)
            );
        }

        assert_eq!(
            parse_startup_args(args(&[
                "--config",
                "relative.json",
                "--config-sha256",
                &digest,
            ])),
            Err(RuntimeError::InvalidConfig)
        );
        assert_eq!(
            parse_startup_args(args(&[
                "--config",
                path,
                "--config-sha256",
                &digest,
                "extra",
            ])),
            Err(RuntimeError::InvalidConfig)
        );
    }
}
