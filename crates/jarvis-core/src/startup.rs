use crate::{
    IpcAuth, JarvisError, JarvisResult, TrustedWakeKeyControlInstallDocument,
    TrustedWakeRuleEnrollment, WorkspaceRootConfig,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::path::PathBuf;

pub const SERVE_STARTUP_CONFIG_VERSION: u16 = 1;
pub const MAX_SERVE_STARTUP_CONFIG_BYTES: usize = 64 * 1024;
const MAX_TRUSTED_WAKE_STARTUP_DOCUMENT_BYTES: usize = 8 * 1024;

pub struct ServeStartupConfig {
    pub workspace_roots: Vec<WorkspaceRootConfig>,
    pub trusted_wake: Option<TrustedWakeStartupDocument>,
    pub ipc_auth: Option<IpcAuth>,
}

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
        if wire.workspace_roots.is_empty() && wire.trusted_wake.is_none() && wire.ipc_auth.is_none()
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
        Ok(Self {
            workspace_roots,
            trusted_wake,
            ipc_auth,
        })
    }
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
