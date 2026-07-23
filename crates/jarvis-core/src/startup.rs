use crate::{
    IpcAuth, JarvisError, JarvisResult, TrustedWakeKeyControlInstallDocument,
    TrustedWakeRuleEnrollment, WorkspaceRootConfig,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::os::unix::ffi::OsStrExt;
use std::path::Component;
use std::path::PathBuf;

pub const SERVE_STARTUP_CONFIG_VERSION: u16 = 1;
pub const MAX_SERVE_STARTUP_CONFIG_BYTES: usize = 64 * 1024;
/// macOS `sockaddr_un.sun_path` has 104 bytes including its trailing NUL.
pub const MAX_UNIX_SOCKET_PATH_BYTES: usize = 103;
pub const MAX_PEER_CODE_REQUIREMENT_BYTES: usize = 4 * 1024;
const MAX_TRUSTED_WAKE_STARTUP_DOCUMENT_BYTES: usize = 8 * 1024;

pub struct ServeStartupConfig {
    pub workspace_roots: Vec<WorkspaceRootConfig>,
    pub trusted_wake: Option<TrustedWakeStartupDocument>,
    pub ipc_auth: Option<IpcAuth>,
    pub ipc_transport: Option<ServeIpcTransport>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServeIpcTransport {
    UnixSocketV1 {
        socket_path: PathBuf,
    },
    UnixSocketPeerIdentityV1 {
        socket_path: PathBuf,
        peer_code_requirement: String,
        peer_identity_profile: PeerIdentityProfile,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerIdentityProfile {
    AdhocExact,
    DeveloperIdHardened,
}

const DEVELOPER_ID_APP_REQUIREMENT_PREFIX: &str = "anchor apple generic and identifier \"com.nobiletechnology.jarvis\" and certificate 1[field.1.2.840.113635.100.6.2.6] exists and certificate leaf[field.1.2.840.113635.100.6.1.13] exists and certificate leaf[subject.OU] = \"";

pub enum TrustedWakeStartupDocument {
    Bootstrap(TrustedWakeRuleEnrollment),
    KeyControl(TrustedWakeKeyControlInstallDocument),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ServeStartupConfigWire {
    version: u16,
    #[serde(default)]
    workspace_roots: Vec<WorkspaceRootWire>,
    #[serde(default)]
    trusted_wake: Option<TrustedWakeWire>,
    #[serde(default)]
    ipc_auth: Option<IpcAuthWire>,
    #[serde(default)]
    ipc_transport: Option<IpcTransportWire>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IpcAuthWire {
    scheme: IpcAuthScheme,
    token: String,
    generation: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum IpcAuthScheme {
    Bearer,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct IpcTransportWire {
    kind: IpcTransportKind,
    socket_path: String,
    #[serde(default)]
    peer_code_requirement: Option<String>,
    #[serde(default)]
    peer_identity_profile: Option<PeerIdentityProfile>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum IpcTransportKind {
    UnixSocketV1,
    UnixSocketPeerIdentityV1,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkspaceRootWire {
    id: String,
    path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustedWakeWire {
    kind: TrustedWakeKind,
    document: Value,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum TrustedWakeKind {
    Bootstrap,
    KeyControl,
}

impl ServeStartupConfig {
    pub fn parse(bytes: &[u8]) -> JarvisResult<Self> {
        if bytes.is_empty() || bytes.len() > MAX_SERVE_STARTUP_CONFIG_BYTES {
            return Err(JarvisError::Validation(format!(
                "startup configuration stdin must contain at most {MAX_SERVE_STARTUP_CONFIG_BYTES} bytes"
            )));
        }
        let wire: ServeStartupConfigWire = serde_json::from_slice(bytes).map_err(|_| {
            JarvisError::Validation("startup configuration stdin is invalid".to_string())
        })?;
        if wire.version != SERVE_STARTUP_CONFIG_VERSION {
            return Err(JarvisError::Validation(format!(
                "startup configuration version must be {SERVE_STARTUP_CONFIG_VERSION}"
            )));
        }
        if wire.workspace_roots.is_empty()
            && wire.trusted_wake.is_none()
            && wire.ipc_auth.is_none()
            && wire.ipc_transport.is_none()
        {
            return Err(JarvisError::Validation(
                "startup configuration must contain workspace roots, trusted wake input, or IPC authentication"
                    .to_string(),
            ));
        }

        if wire.workspace_roots.len() > crate::MAX_WORKSPACE_ROOTS {
            return Err(JarvisError::Validation(format!(
                "at most {} workspace roots may be configured",
                crate::MAX_WORKSPACE_ROOTS
            )));
        }
        let mut root_ids = HashSet::new();
        let mut root_paths = HashSet::new();
        let mut workspace_roots = Vec::with_capacity(wire.workspace_roots.len());
        for root in wire.workspace_roots {
            let config = WorkspaceRootConfig::new(root.id, PathBuf::from(root.path))?;
            if !root_ids.insert(config.id.clone()) {
                return Err(JarvisError::Validation(
                    "workspace root ids must be unique".to_string(),
                ));
            }
            if !root_paths.insert(config.path.clone()) {
                return Err(JarvisError::Validation(
                    "workspace root paths must be unique".to_string(),
                ));
            }
            workspace_roots.push(config);
        }
        let trusted_wake = wire.trusted_wake.map(parse_trusted_wake).transpose()?;
        let ipc_auth = wire
            .ipc_auth
            .map(|auth| match auth.scheme {
                IpcAuthScheme::Bearer => IpcAuth::new(&auth.token, auth.generation),
            })
            .transpose()?;
        let ipc_transport = wire.ipc_transport.map(parse_ipc_transport).transpose()?;
        if ipc_transport.is_some() && ipc_auth.is_none() {
            return Err(JarvisError::Validation(
                "Unix-socket IPC requires startup bearer authentication".to_string(),
            ));
        }
        Ok(Self {
            workspace_roots,
            trusted_wake,
            ipc_auth,
            ipc_transport,
        })
    }
}

fn parse_ipc_transport(wire: IpcTransportWire) -> JarvisResult<ServeIpcTransport> {
    let socket_path = PathBuf::from(wire.socket_path);
    validate_unix_socket_path(&socket_path)?;
    match wire.kind {
        IpcTransportKind::UnixSocketV1 => {
            if wire.peer_code_requirement.is_some() || wire.peer_identity_profile.is_some() {
                return Err(JarvisError::Validation(
                    "legacy Unix-socket IPC does not accept peer identity configuration"
                        .to_string(),
                ));
            }
            Ok(ServeIpcTransport::UnixSocketV1 { socket_path })
        }
        IpcTransportKind::UnixSocketPeerIdentityV1 => {
            let peer_code_requirement = wire.peer_code_requirement.ok_or_else(|| {
                JarvisError::Validation(
                    "peer-identity Unix-socket IPC requires a code requirement".to_string(),
                )
            })?;
            let peer_identity_profile = wire.peer_identity_profile.ok_or_else(|| {
                JarvisError::Validation(
                    "peer-identity Unix-socket IPC requires an identity profile".to_string(),
                )
            })?;
            if peer_code_requirement.is_empty()
                || peer_code_requirement.len() > MAX_PEER_CODE_REQUIREMENT_BYTES
                || peer_code_requirement.as_bytes().contains(&0)
            {
                return Err(JarvisError::Validation(format!(
                    "peer code requirement must contain 1 to {MAX_PEER_CODE_REQUIREMENT_BYTES} bytes without NUL"
                )));
            }
            validate_peer_code_requirement(&peer_code_requirement, peer_identity_profile)?;
            Ok(ServeIpcTransport::UnixSocketPeerIdentityV1 {
                socket_path,
                peer_code_requirement,
                peer_identity_profile,
            })
        }
    }
}

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

fn parse_trusted_wake(wire: TrustedWakeWire) -> JarvisResult<TrustedWakeStartupDocument> {
    let document_bytes = serde_json::to_vec(&wire.document).map_err(|_| {
        JarvisError::Validation("trusted wake startup document is invalid".to_string())
    })?;
    if document_bytes.is_empty() || document_bytes.len() > MAX_TRUSTED_WAKE_STARTUP_DOCUMENT_BYTES {
        return Err(JarvisError::Validation(
            "trusted wake startup document must contain at most 8192 bytes".to_string(),
        ));
    }
    match wire.kind {
        TrustedWakeKind::Bootstrap => serde_json::from_value(wire.document)
            .map(TrustedWakeStartupDocument::Bootstrap)
            .map_err(|_| {
                JarvisError::Validation("trusted wake bootstrap document is invalid".to_string())
            }),
        TrustedWakeKind::KeyControl => serde_json::from_value(wire.document)
            .map(TrustedWakeStartupDocument::KeyControl)
            .map_err(|_| {
                JarvisError::Validation("trusted wake key-control document is invalid".to_string())
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strict_workspace_startup_config_parses_without_argv_encoding() {
        let document = json!({
            "version": 1,
            "workspace_roots": [{"id":"project", "path":"/tmp/project"}]
        });
        let parsed = ServeStartupConfig::parse(document.to_string().as_bytes()).unwrap();
        assert_eq!(parsed.workspace_roots.len(), 1);
        assert!(parsed.trusted_wake.is_none());
        assert!(parsed.ipc_auth.is_none());
        assert!(parsed.ipc_transport.is_none());
    }

    #[test]
    fn auth_only_startup_config_accepts_strict_base64url_bearer() {
        let document = json!({
            "version": 1,
            "ipc_auth": {
                "scheme": "bearer",
                "token": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
                "generation": 7
            }
        });
        let parsed = ServeStartupConfig::parse(document.to_string().as_bytes()).unwrap();
        assert_eq!(parsed.ipc_auth.unwrap().generation(), 7);
        assert!(parsed.ipc_transport.is_none());
    }

    #[test]
    fn startup_config_accepts_strict_authenticated_unix_transport() {
        let document = json!({
            "version": 1,
            "ipc_auth": {
                "scheme": "bearer",
                "token": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
                "generation": 7
            },
            "ipc_transport": {
                "kind": "unix_socket_v1",
                "socket_path": "/tmp/jarvis-owned/core.sock"
            }
        });
        let parsed = ServeStartupConfig::parse(document.to_string().as_bytes()).unwrap();
        assert_eq!(
            parsed.ipc_transport,
            Some(ServeIpcTransport::UnixSocketV1 {
                socket_path: PathBuf::from("/tmp/jarvis-owned/core.sock")
            })
        );
    }

    #[test]
    fn startup_config_accepts_strict_peer_identity_unix_transport() {
        let developer_requirement = concat!(
            "anchor apple generic and identifier \"com.nobiletechnology.jarvis\" ",
            "and certificate 1[field.1.2.840.113635.100.6.2.6] exists ",
            "and certificate leaf[field.1.2.840.113635.100.6.1.13] exists ",
            "and certificate leaf[subject.OU] = \"AB12CD34EF\""
        );
        let document = json!({
            "version": 1,
            "ipc_auth": {
                "scheme": "bearer",
                "token": "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
                "generation": 7
            },
            "ipc_transport": {
                "kind": "unix_socket_peer_identity_v1",
                "socket_path": "/tmp/jarvis-owned/core.sock",
                "peer_code_requirement": developer_requirement,
                "peer_identity_profile": "developer_id_hardened"
            }
        });
        let parsed = ServeStartupConfig::parse(document.to_string().as_bytes()).unwrap();
        assert_eq!(
            parsed.ipc_transport,
            Some(ServeIpcTransport::UnixSocketPeerIdentityV1 {
                socket_path: PathBuf::from("/tmp/jarvis-owned/core.sock"),
                peer_code_requirement: developer_requirement.to_string(),
                peer_identity_profile: PeerIdentityProfile::DeveloperIdHardened,
            })
        );
    }

    #[test]
    fn startup_config_rejects_incomplete_or_unbounded_peer_identity_transport() {
        let auth = json!({
            "scheme":"bearer",
            "token":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            "generation":1
        });
        for transport in [
            json!({"kind":"unix_socket_peer_identity_v1","socket_path":"/tmp/core.sock","peer_identity_profile":"adhoc_exact"}),
            json!({"kind":"unix_socket_peer_identity_v1","socket_path":"/tmp/core.sock","peer_code_requirement":"true"}),
            json!({"kind":"unix_socket_peer_identity_v1","socket_path":"/tmp/core.sock","peer_code_requirement":"true","peer_identity_profile":"adhoc_exact"}),
            json!({"kind":"unix_socket_peer_identity_v1","socket_path":"/tmp/core.sock","peer_code_requirement":"anchor apple generic and identifier \"com.nobiletechnology.jarvis\" and certificate leaf[subject.OU] = \"AB12CD34EF\"","peer_identity_profile":"developer_id_hardened"}),
            json!({"kind":"unix_socket_peer_identity_v1","socket_path":"/tmp/core.sock","peer_code_requirement":"","peer_identity_profile":"adhoc_exact"}),
            json!({"kind":"unix_socket_peer_identity_v1","socket_path":"/tmp/core.sock","peer_code_requirement":"true","peer_identity_profile":"unknown"}),
            json!({"kind":"unix_socket_v1","socket_path":"/tmp/core.sock","peer_code_requirement":"true","peer_identity_profile":"adhoc_exact"}),
        ] {
            let invalid = json!({"version":1,"ipc_auth":auth.clone(),"ipc_transport":transport});
            assert!(ServeStartupConfig::parse(invalid.to_string().as_bytes()).is_err());
        }

        let oversized = "x".repeat(MAX_PEER_CODE_REQUIREMENT_BYTES + 1);
        let invalid = json!({
            "version":1,
            "ipc_auth":auth,
            "ipc_transport":{
                "kind":"unix_socket_peer_identity_v1",
                "socket_path":"/tmp/core.sock",
                "peer_code_requirement":oversized,
                "peer_identity_profile":"adhoc_exact"
            }
        });
        assert!(ServeStartupConfig::parse(invalid.to_string().as_bytes()).is_err());
    }

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

    #[test]
    fn startup_config_rejects_unsafe_or_unauthenticated_unix_transport() {
        for invalid in [
            json!({"version":1,"ipc_transport":{"kind":"unix_socket_v1","socket_path":"relative.sock"}}),
            json!({"version":1,"ipc_auth":{"scheme":"bearer","token":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","generation":1},"ipc_transport":{"kind":"unix_socket_v1","socket_path":"/tmp/../tmp/core.sock"}}),
            json!({"version":1,"ipc_auth":{"scheme":"bearer","token":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","generation":1},"ipc_transport":{"kind":"unknown","socket_path":"/tmp/core.sock"}}),
            json!({"version":1,"ipc_auth":{"scheme":"bearer","token":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","generation":1},"ipc_transport":{"kind":"unix_socket_v1","socket_path":"/tmp/core.sock","extra":true}}),
        ] {
            assert!(ServeStartupConfig::parse(invalid.to_string().as_bytes()).is_err());
        }

        let long_path = format!("/tmp/{}", "x".repeat(MAX_UNIX_SOCKET_PATH_BYTES));
        let invalid = json!({"version":1,"ipc_auth":{"scheme":"bearer","token":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","generation":1},"ipc_transport":{"kind":"unix_socket_v1","socket_path":long_path}});
        assert!(ServeStartupConfig::parse(invalid.to_string().as_bytes()).is_err());
    }

    #[test]
    fn startup_config_rejects_invalid_or_non_strict_ipc_auth() {
        for invalid in [
            json!({"version":1,"ipc_auth":{"scheme":"basic","token":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","generation":1}}),
            json!({"version":1,"ipc_auth":{"scheme":"bearer","token":"short","generation":1}}),
            json!({"version":1,"ipc_auth":{"scheme":"bearer","token":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","generation":0}}),
            json!({"version":1,"ipc_auth":{"scheme":"bearer","token":"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA","generation":1,"extra":true}}),
        ] {
            assert!(ServeStartupConfig::parse(invalid.to_string().as_bytes()).is_err());
        }
    }

    #[test]
    fn startup_config_rejects_unknown_version_fields_empty_and_oversized_input() {
        for invalid in [
            json!({"version":2,"workspace_roots":[{"id":"project","path":"/tmp"}]}),
            json!({"version":1,"unexpected":true,"workspace_roots":[{"id":"project","path":"/tmp"}]}),
            json!({"version":1}),
        ] {
            assert!(ServeStartupConfig::parse(invalid.to_string().as_bytes()).is_err());
        }
        assert!(
            ServeStartupConfig::parse(&vec![b'x'; MAX_SERVE_STARTUP_CONFIG_BYTES + 1]).is_err()
        );
    }

    #[test]
    fn startup_config_rejects_duplicate_and_excess_roots_before_opening_them() {
        let duplicate_id = json!({
            "version":1,
            "workspace_roots":[
                {"id":"project","path":"/tmp/one"},
                {"id":"project","path":"/tmp/two"}
            ]
        });
        let duplicate_path = json!({
            "version":1,
            "workspace_roots":[
                {"id":"one","path":"/tmp/project"},
                {"id":"two","path":"/tmp/project"}
            ]
        });
        let excess_roots = (0..=crate::MAX_WORKSPACE_ROOTS)
            .map(|index| json!({"id":format!("root{index}"),"path":format!("/tmp/{index}")}))
            .collect::<Vec<_>>();
        let excess = json!({"version":1,"workspace_roots":excess_roots});
        for invalid in [duplicate_id, duplicate_path, excess] {
            assert!(ServeStartupConfig::parse(invalid.to_string().as_bytes()).is_err());
        }
    }
}
