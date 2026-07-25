use crate::{JarvisError, JarvisResult};
use serde::Deserialize;
use std::os::unix::ffi::OsStrExt;
use std::path::Component;

/// macOS `sockaddr_un.sun_path` has 104 bytes including its trailing NUL.
pub const MAX_UNIX_SOCKET_PATH_BYTES: usize = 103;
pub const MAX_PEER_CODE_REQUIREMENT_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerIdentityProfile {
    AdhocExact,
    DeveloperIdHardened,
}

const DEVELOPER_ID_APP_REQUIREMENT_PREFIX: &str = "anchor apple generic and identifier \"com.nobiletechnology.jarvis\" and certificate 1[field.1.2.840.113635.100.6.2.6] exists and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and certificate leaf[subject.OU] = \"";

pub fn validate_peer_code_requirement(
    requirement: &str,
    profile: PeerIdentityProfile,
) -> JarvisResult<()> {
    let valid = match profile {
        PeerIdentityProfile::AdhocExact => is_exact_adhoc_requirement(requirement),
        PeerIdentityProfile::DeveloperIdHardened => requirement
            .strip_prefix(DEVELOPER_ID_APP_REQUIREMENT_PREFIX)
            .and_then(|value| value.strip_suffix('"'))
            .is_some_and(is_valid_team_identifier),
    };
    if !valid {
        return Err(JarvisError::Validation(
            "peer code requirement does not match its identity profile".to_string(),
        ));
    }
    Ok(())
}

fn is_exact_adhoc_requirement(requirement: &str) -> bool {
    if let Some(hash) = requirement
        .strip_prefix("cdhash H\"")
        .and_then(|value| value.strip_suffix('"'))
    {
        return is_valid_cdhash(hash);
    }
    let Some(rest) = requirement.strip_prefix("identifier \"") else {
        return false;
    };
    let Some((identifier, hash)) = rest.split_once("\" and cdhash H\"") else {
        return false;
    };
    !identifier.is_empty()
        && identifier.len() <= 256
        && identifier
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        && hash.strip_suffix('"').is_some_and(is_valid_cdhash)
}

fn is_valid_cdhash(hash: &str) -> bool {
    matches!(hash.len(), 40 | 64) && hash.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_valid_team_identifier(team: &str) -> bool {
    team.len() == 10
        && team
            .bytes()
            .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
}

pub fn validate_unix_socket_path(socket_path: &std::path::Path) -> JarvisResult<()> {
    let bytes = socket_path.as_os_str().as_bytes();
    if !socket_path.is_absolute()
        || bytes.is_empty()
        || bytes.len() > MAX_UNIX_SOCKET_PATH_BYTES
        || bytes.contains(&0)
        || socket_path.file_name().is_none()
        || socket_path
            .components()
            .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
    {
        return Err(JarvisError::Validation(format!(
            "Unix socket path must be an absolute, normalized leaf of at most {MAX_UNIX_SOCKET_PATH_BYTES} bytes"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peer_identity_profiles_require_exact_canonical_policy_shapes() {
        for valid in [
            "cdhash H\"0123456789abcdef0123456789abcdef01234567\"",
            "identifier \"com.nobiletechnology.jarvis\" and cdhash H\"0123456789abcdef0123456789abcdef01234567\"",
        ] {
            validate_peer_code_requirement(valid, PeerIdentityProfile::AdhocExact).unwrap();
        }
        for invalid in [
            "true",
            "identifier \"com.nobiletechnology.jarvis\"",
            "cdhash H\"short\"",
            "cdhash H\"0123456789abcdef0123456789abcdef01234567\" or true",
        ] {
            assert!(
                validate_peer_code_requirement(invalid, PeerIdentityProfile::AdhocExact).is_err()
            );
        }

        let developer = concat!(
            "anchor apple generic and identifier \"com.nobiletechnology.jarvis\" ",
            "and certificate 1[field.1.2.840.113635.100.6.2.6] exists ",
            "and certificate leaf[field.1.2.840.113635.100.6.1.13] exists ",
            "and certificate leaf[subject.OU] = \"AB12CD34EF\""
        );
        validate_peer_code_requirement(developer, PeerIdentityProfile::DeveloperIdHardened)
            .unwrap();
        assert!(validate_peer_code_requirement(
            "anchor apple generic and identifier \"com.nobiletechnology.jarvis\" and certificate leaf[subject.OU] = \"AB12CD34EF\"",
            PeerIdentityProfile::DeveloperIdHardened,
        )
        .is_err());
    }
}
