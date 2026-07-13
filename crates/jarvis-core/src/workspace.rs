use crate::{
    CancellationBehavior, CancellationSignal, InProcessPlugin, JarvisError, JarvisResult,
    JsonSchema, PluginAccess, PluginActionManifest, PluginManifest, PluginNetworkAccess,
    PluginPermission, PluginSource, PluginTimeout, RiskTier,
};
use rustix::fd::OwnedFd;
use rustix::fs::{fstat, open, openat, statat, AtFlags, Dir, FileType, Mode, OFlags};
use serde_json::{json, Map, Value};
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

pub const MAX_WORKSPACE_ROOTS: usize = 8;
pub const MAX_WORKSPACE_ROOT_ID_BYTES: usize = 32;
pub const MAX_WORKSPACE_RELATIVE_PATH_BYTES: usize = 512;
pub const MAX_WORKSPACE_LIST_ENTRIES: usize = 200;
pub const MAX_WORKSPACE_TEXT_BYTES: usize = 64 * 1024;
pub const MAX_WORKSPACE_TEXT_LINE_BYTES: usize = 16 * 1024;

pub(crate) fn audit_request_summary(input: &Value) -> Value {
    let root_id = input
        .get("root_id")
        .and_then(Value::as_str)
        .filter(|id| validate_root_id(id).is_ok());
    let requested_path = input.get("path").and_then(Value::as_str);
    let relative_path = requested_path.and_then(|path| {
        validate_relative_path(path, true).ok().map(|components| {
            if components.is_empty() {
                "@root".to_string()
            } else {
                components.join("/")
            }
        })
    });
    let path_redacted_or_invalid = requested_path.is_some() && relative_path.is_none();
    json!({
        "root_id": root_id,
        "relative_path": relative_path,
        "path_redacted_or_invalid": path_redacted_or_invalid,
        "limits": {
            "max_entries": MAX_WORKSPACE_LIST_ENTRIES,
            "max_text_bytes": MAX_WORKSPACE_TEXT_BYTES,
            "max_line_bytes": MAX_WORKSPACE_TEXT_LINE_BYTES,
        },
    })
}

pub(crate) fn finish_audit_summary(summary: &mut Value, outcome: &str, output: Option<&Value>) {
    let Some(summary) = summary.as_object_mut() else {
        return;
    };
    summary.insert("outcome".to_string(), Value::String(outcome.to_string()));
    if let Some(output) = output {
        for field in ["entry_count", "byte_count", "truncated"] {
            if let Some(value) = output.get(field) {
                summary.insert(field.to_string(), value.clone());
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorkspaceRootConfig {
    pub id: String,
    pub path: PathBuf,
}

impl WorkspaceRootConfig {
    pub fn parse(value: &str) -> JarvisResult<Self> {
        let (id, path) = value.split_once('=').ok_or_else(|| {
            JarvisError::Validation(
                "workspace root must use the form <id>=<absolute-path>".to_string(),
            )
        })?;
        validate_root_id(id)?;
        let path = PathBuf::from(path);
        if !path.is_absolute() {
            return Err(JarvisError::Validation(
                "workspace root path must be absolute".to_string(),
            ));
        }
        Ok(Self {
            id: id.to_string(),
            path,
        })
    }
}

#[derive(Debug)]
struct WorkspaceRoot {
    descriptor: OwnedFd,
}

#[derive(Debug, Clone)]
pub struct WorkspaceInspectPlugin {
    roots: Arc<HashMap<String, Arc<WorkspaceRoot>>>,
}

impl WorkspaceInspectPlugin {
    pub fn open(configs: Vec<WorkspaceRootConfig>) -> JarvisResult<Option<Self>> {
        if configs.is_empty() {
            return Ok(None);
        }
        if configs.len() > MAX_WORKSPACE_ROOTS {
            return Err(JarvisError::Validation(format!(
                "at most {MAX_WORKSPACE_ROOTS} workspace roots may be configured"
            )));
        }

        let mut roots = HashMap::new();
        let mut canonical_paths = HashSet::new();
        for config in configs {
            validate_root_id(&config.id)?;
            if roots.contains_key(&config.id) {
                return Err(JarvisError::Validation(format!(
                    "workspace root id {} is duplicated",
                    config.id
                )));
            }
            let canonical = std::fs::canonicalize(&config.path).map_err(|error| {
                JarvisError::Validation(format!(
                    "workspace root {} could not be canonicalized: {error}",
                    config.id
                ))
            })?;
            if !canonical.is_dir() {
                return Err(JarvisError::Validation(format!(
                    "workspace root {} must be an existing directory",
                    config.id
                )));
            }
            if !canonical_paths.insert(canonical.clone()) {
                return Err(JarvisError::Validation(
                    "workspace root paths must be unique".to_string(),
                ));
            }
            let descriptor = open(
                &canonical,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
                Mode::empty(),
            )
            .map_err(|error| {
                JarvisError::Validation(format!(
                    "workspace root {} could not be opened safely: {error}",
                    config.id
                ))
            })?;
            roots.insert(config.id, Arc::new(WorkspaceRoot { descriptor }));
        }
        Ok(Some(Self {
            roots: Arc::new(roots),
        }))
    }

    pub fn root_count(&self) -> usize {
        self.roots.len()
    }

    fn root(&self, id: &str) -> JarvisResult<&WorkspaceRoot> {
        validate_root_id(id)?;
        self.roots.get(id).map(Arc::as_ref).ok_or_else(|| {
            JarvisError::PolicyBlocked("workspace root is not configured".to_string())
        })
    }

    fn list(&self, input: &Value, cancellation: &CancellationSignal) -> JarvisResult<Value> {
        let root_id = input_string(input, "root_id")?;
        let relative = input_string(input, "path")?;
        let root = self.root(root_id)?;
        let components = validate_relative_path(relative, true)?;
        let directory = open_directory(root, &components, cancellation)?;
        let mut stream = Dir::read_from(&directory).map_err(|error| {
            JarvisError::Plugin(format!("workspace directory read failed: {error}"))
        })?;
        let mut entries = Vec::new();

        for entry in &mut stream {
            ensure_not_cancelled(cancellation)?;
            let entry = entry.map_err(|error| {
                JarvisError::Plugin(format!("workspace directory entry failed: {error}"))
            })?;
            let bytes = entry.file_name().to_bytes();
            if bytes == b"." || bytes == b".." {
                continue;
            }
            let name = std::str::from_utf8(bytes).map_err(|_| {
                JarvisError::PolicyBlocked("workspace contains a non-UTF-8 entry name".to_string())
            })?;
            if is_hidden_or_secret_name(name) {
                continue;
            }
            let stat = statat(&directory, entry.file_name(), AtFlags::SYMLINK_NOFOLLOW).map_err(
                |error| JarvisError::Plugin(format!("workspace entry inspection failed: {error}")),
            )?;
            let file_type = FileType::from_raw_mode(stat.st_mode);
            let kind = if file_type.is_dir() {
                "directory"
            } else if file_type.is_file() {
                "file"
            } else {
                continue;
            };
            if file_type.is_file() && stat.st_nlink != 1 {
                continue;
            }
            if entries.len() == MAX_WORKSPACE_LIST_ENTRIES {
                return Err(JarvisError::PolicyBlocked(format!(
                    "workspace directory exceeds the {MAX_WORKSPACE_LIST_ENTRIES}-entry list limit"
                )));
            }
            entries.push(json!({
                "name": name,
                "kind": kind,
                "size_bytes": if file_type.is_file() { Some(stat.st_size) } else { None },
            }));
        }
        entries.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
        Ok(json!({
            "entries": entries,
            "entry_count": entries.len(),
            "truncated": false,
        }))
    }

    fn read_text(&self, input: &Value, cancellation: &CancellationSignal) -> JarvisResult<Value> {
        let root_id = input_string(input, "root_id")?;
        let relative = input_string(input, "path")?;
        let root = self.root(root_id)?;
        let components = validate_relative_path(relative, false)?;
        let (parents, file_name) = components.split_at(components.len() - 1);
        let parent = open_directory(root, parents, cancellation)?;
        ensure_not_cancelled(cancellation)?;
        let descriptor = openat(
            &parent,
            OsStr::new(file_name[0]),
            OFlags::RDONLY | OFlags::CLOEXEC | OFlags::NOFOLLOW | OFlags::NONBLOCK,
            Mode::empty(),
        )
        .map_err(|_| {
            JarvisError::PolicyBlocked("workspace file could not be opened safely".to_string())
        })?;
        let stat = fstat(&descriptor).map_err(|error| {
            JarvisError::Plugin(format!("workspace file inspection failed: {error}"))
        })?;
        if !FileType::from_raw_mode(stat.st_mode).is_file() {
            return Err(JarvisError::PolicyBlocked(
                "workspace path is not a regular file".to_string(),
            ));
        }
        if stat.st_nlink != 1 {
            return Err(JarvisError::PolicyBlocked(
                "workspace hard-linked files are blocked".to_string(),
            ));
        }
        if stat.st_size < 0 || stat.st_size as usize > MAX_WORKSPACE_TEXT_BYTES {
            return Err(JarvisError::PolicyBlocked(format!(
                "workspace file exceeds the {MAX_WORKSPACE_TEXT_BYTES}-byte text limit"
            )));
        }
        let mut bytes = Vec::with_capacity(stat.st_size as usize);
        std::fs::File::from(descriptor)
            .take((MAX_WORKSPACE_TEXT_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|error| JarvisError::Plugin(format!("workspace file read failed: {error}")))?;
        ensure_not_cancelled(cancellation)?;
        if bytes.len() > MAX_WORKSPACE_TEXT_BYTES {
            return Err(JarvisError::PolicyBlocked(format!(
                "workspace file exceeds the {MAX_WORKSPACE_TEXT_BYTES}-byte text limit"
            )));
        }
        if bytes.contains(&0) {
            return Err(JarvisError::PolicyBlocked(
                "workspace file is binary".to_string(),
            ));
        }
        let text = String::from_utf8(bytes).map_err(|_| {
            JarvisError::PolicyBlocked("workspace file is not UTF-8 text".to_string())
        })?;
        if text
            .chars()
            .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        {
            return Err(JarvisError::PolicyBlocked(
                "workspace file contains disallowed control characters".to_string(),
            ));
        }
        if text
            .lines()
            .any(|line| line.len() > MAX_WORKSPACE_TEXT_LINE_BYTES)
        {
            return Err(JarvisError::PolicyBlocked(format!(
                "workspace text line exceeds the {MAX_WORKSPACE_TEXT_LINE_BYTES}-byte limit"
            )));
        }
        Ok(json!({
            "text": text,
            "byte_count": text.len(),
            "truncated": false,
        }))
    }
}

impl InProcessPlugin for WorkspaceInspectPlugin {
    fn manifest(&self) -> PluginManifest {
        let mut common = Map::new();
        common.insert("root_id".to_string(), json!({ "type": "string" }));
        common.insert("path".to_string(), json!({ "type": "string" }));
        let required = vec!["root_id".to_string(), "path".to_string()];

        PluginManifest {
            manifest_schema_version: 1,
            id: "workspace_inspect".to_string(),
            name: "Workspace Inspect".to_string(),
            version: "1.0.0".to_string(),
            source: PluginSource::FirstParty,
            author: "Jarvis".to_string(),
            source_path: None,
            subprocess: None,
            publisher_signature: None,
            actions: vec![
                PluginActionManifest {
                    name: "list".to_string(),
                    description: "List up to 200 non-sensitive regular files and directories at a relative path, or at the explicit @root sentinel, inside a configured workspace root.".to_string(),
                    permissions: vec![PluginPermission::ReadWorkspace],
                    risk_tier: RiskTier::Notify,
                    input_schema: JsonSchema::object(common.clone(), required.clone()),
                    output_schema: workspace_output_schema(&[
                        ("entries", "array"),
                        ("entry_count", "integer"),
                        ("truncated", "boolean"),
                    ]),
                    proactive: false,
                    memory_access: PluginAccess::None,
                    model_access: PluginAccess::None,
                    network_access: PluginNetworkAccess::default(),
                    audit_fields: vec![
                        "root_id".to_string(),
                        "relative_path".to_string(),
                        "limits".to_string(),
                        "entry_count".to_string(),
                        "truncated".to_string(),
                        "outcome".to_string(),
                    ],
                    timeout: PluginTimeout::default_for_action(),
                    cancellation: CancellationBehavior::Cooperative,
                },
                PluginActionManifest {
                    name: "read_text".to_string(),
                    description: "Read one bounded UTF-8 text file inside an explicitly configured workspace root.".to_string(),
                    permissions: vec![PluginPermission::ReadWorkspace],
                    risk_tier: RiskTier::Notify,
                    input_schema: JsonSchema::object(common, required),
                    output_schema: workspace_output_schema(&[
                        ("text", "string"),
                        ("byte_count", "integer"),
                        ("truncated", "boolean"),
                    ]),
                    proactive: false,
                    memory_access: PluginAccess::None,
                    model_access: PluginAccess::None,
                    network_access: PluginNetworkAccess::default(),
                    audit_fields: vec![
                        "root_id".to_string(),
                        "relative_path".to_string(),
                        "limits".to_string(),
                        "byte_count".to_string(),
                        "truncated".to_string(),
                        "outcome".to_string(),
                    ],
                    timeout: PluginTimeout::default_for_action(),
                    cancellation: CancellationBehavior::Cooperative,
                },
            ],
        }
    }

    fn execute(
        &self,
        action: &PluginActionManifest,
        input: Value,
        cancellation: CancellationSignal,
    ) -> JarvisResult<Value> {
        match action.name.as_str() {
            "list" => self.list(&input, &cancellation),
            "read_text" => self.read_text(&input, &cancellation),
            _ => Err(JarvisError::Plugin("unknown workspace action".to_string())),
        }
    }
}

fn validate_root_id(id: &str) -> JarvisResult<()> {
    if id.is_empty()
        || id.len() > MAX_WORKSPACE_ROOT_ID_BYTES
        || !id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
        || !id.as_bytes()[0].is_ascii_lowercase()
    {
        return Err(JarvisError::Validation(format!(
            "workspace root id must start with a lowercase letter and contain at most {MAX_WORKSPACE_ROOT_ID_BYTES} lowercase ASCII letters, digits, hyphens, or underscores"
        )));
    }
    Ok(())
}

fn validate_relative_path(path: &str, allow_root_sentinel: bool) -> JarvisResult<Vec<&str>> {
    if path.as_bytes().contains(&0) || path.len() > MAX_WORKSPACE_RELATIVE_PATH_BYTES {
        return Err(JarvisError::PolicyBlocked(
            "workspace path is invalid or oversized".to_string(),
        ));
    }
    if path.is_empty() {
        return Err(JarvisError::PolicyBlocked(
            "workspace path cannot be empty".to_string(),
        ));
    }
    if path == "@root" {
        return if allow_root_sentinel {
            Ok(Vec::new())
        } else {
            Err(JarvisError::PolicyBlocked(
                "workspace file path cannot use the root sentinel".to_string(),
            ))
        };
    }
    let candidate = Path::new(path);
    if candidate.is_absolute() {
        return Err(JarvisError::PolicyBlocked(
            "absolute workspace paths are blocked".to_string(),
        ));
    }
    let mut components = Vec::new();
    for component in candidate.components() {
        match component {
            Component::Normal(value) => {
                let value = value.to_str().ok_or_else(|| {
                    JarvisError::PolicyBlocked("non-UTF-8 workspace paths are blocked".to_string())
                })?;
                if value == "." || value == ".." || is_hidden_or_secret_name(value) {
                    return Err(JarvisError::PolicyBlocked(
                        "hidden, parent, dot, and secret-named workspace paths are blocked"
                            .to_string(),
                    ));
                }
                components.push(value);
            }
            _ => {
                return Err(JarvisError::PolicyBlocked(
                    "workspace path must contain only normal relative components".to_string(),
                ))
            }
        }
    }
    if components.is_empty() {
        return Err(JarvisError::PolicyBlocked(
            "workspace path cannot be empty".to_string(),
        ));
    }
    Ok(components)
}

fn open_directory(
    root: &WorkspaceRoot,
    components: &[&str],
    cancellation: &CancellationSignal,
) -> JarvisResult<OwnedFd> {
    let mut current = openat(
        &root.descriptor,
        ".",
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
        Mode::empty(),
    )
    .map_err(|_| {
        JarvisError::PolicyBlocked("workspace root descriptor is unavailable".to_string())
    })?;
    for component in components {
        ensure_not_cancelled(cancellation)?;
        current = openat(
            &current,
            OsStr::new(component),
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC | OFlags::NOFOLLOW,
            Mode::empty(),
        )
        .map_err(|_| {
            JarvisError::PolicyBlocked("workspace directory could not be opened safely".to_string())
        })?;
    }
    Ok(current)
}

fn ensure_not_cancelled(cancellation: &CancellationSignal) -> JarvisResult<()> {
    if cancellation.is_cancelled() {
        Err(JarvisError::Plugin(
            "workspace operation cancelled".to_string(),
        ))
    } else {
        Ok(())
    }
}

fn is_hidden_or_secret_name(name: &str) -> bool {
    if name.starts_with('.') {
        return true;
    }
    let normalized = name.to_ascii_lowercase();
    let tokens = normalized
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    let sensitive_tokens = [
        "secret",
        "secrets",
        "credential",
        "credentials",
        "password",
        "passwd",
        "token",
        "auth",
        "oauth",
        "pat",
        "apikey",
        "keychain",
        "netrc",
        "keystore",
        "truststore",
    ];
    let sensitive_pairs = [
        ("api", "key"),
        ("access", "key"),
        ("client", "secret"),
        ("private", "key"),
        ("ssh", "key"),
        ("auth", "token"),
        ("github", "pat"),
        ("service", "account"),
    ];
    sensitive_tokens
        .iter()
        .any(|sensitive| tokens.contains(sensitive))
        || tokens
            .windows(2)
            .any(|pair| sensitive_pairs.contains(&(pair[0], pair[1])))
        || normalized.starts_with("id_rsa")
        || normalized.starts_with("id_ed25519")
        || normalized == "kubeconfig"
        || normalized.starts_with("kubeconfig.")
        || normalized.ends_with(".env")
        || ["pem", "p12", "pfx", "key"]
            .iter()
            .any(|extension| normalized.ends_with(&format!(".{extension}")))
}

fn input_string<'a>(input: &'a Value, field: &str) -> JarvisResult<&'a str> {
    input.get(field).and_then(Value::as_str).ok_or_else(|| {
        JarvisError::Validation(format!("workspace input field {field} must be a string"))
    })
}

fn workspace_output_schema(fields: &[(&str, &str)]) -> JsonSchema {
    let properties = fields
        .iter()
        .map(|(name, kind)| ((*name).to_string(), json!({ "type": kind })))
        .collect();
    JsonSchema::object(
        properties,
        fields.iter().map(|(name, _)| (*name).to_string()).collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn fixture() -> (tempfile::TempDir, WorkspaceInspectPlugin) {
        let directory = tempfile::tempdir().expect("temp workspace");
        std::fs::write(directory.path().join("README.md"), "hello workspace\n")
            .expect("text fixture");
        std::fs::create_dir(directory.path().join("src")).expect("source dir");
        std::fs::write(directory.path().join("src/lib.rs"), "pub fn safe() {}\n")
            .expect("nested text fixture");
        std::fs::write(directory.path().join(".env"), "TOKEN=private\n").expect("hidden fixture");
        std::fs::write(directory.path().join("credentials.json"), "private")
            .expect("secret fixture");
        let plugin = WorkspaceInspectPlugin::open(vec![WorkspaceRootConfig {
            id: "project".to_string(),
            path: directory.path().to_path_buf(),
        }])
        .expect("open workspace")
        .expect("configured plugin");
        (directory, plugin)
    }

    #[test]
    fn lists_deterministically_without_sensitive_or_special_entries() {
        let (directory, plugin) = fixture();
        symlink("README.md", directory.path().join("linked.md")).expect("symlink fixture");
        let result = plugin
            .list(
                &json!({"root_id":"project", "path":"@root"}),
                &CancellationSignal::new(),
            )
            .expect("list workspace");
        assert_eq!(result["entry_count"], 2);
        assert_eq!(result["entries"][0]["name"], "README.md");
        assert_eq!(result["entries"][1]["name"], "src");
        let encoded = result.to_string();
        assert!(!encoded.contains(".env"));
        assert!(!encoded.contains("credential"));
        assert!(!encoded.contains("linked"));
        assert!(!encoded.contains(directory.path().to_string_lossy().as_ref()));
    }

    #[test]
    fn reads_text_and_rejects_escape_binary_secret_symlink_and_oversize() {
        let (directory, plugin) = fixture();
        let result = plugin
            .read_text(
                &json!({"root_id":"project", "path":"src/lib.rs"}),
                &CancellationSignal::new(),
            )
            .expect("read text");
        assert_eq!(result["text"], "pub fn safe() {}\n");

        std::fs::write(directory.path().join("binary.bin"), b"a\0b").expect("binary fixture");
        std::fs::write(directory.path().join("control.txt"), b"a\x07b").expect("control fixture");
        std::fs::write(
            directory.path().join("long-line.txt"),
            vec![b'x'; MAX_WORKSPACE_TEXT_LINE_BYTES + 1],
        )
        .expect("long-line fixture");
        std::fs::hard_link(
            directory.path().join("README.md"),
            directory.path().join("hardlink.md"),
        )
        .expect("hardlink fixture");
        symlink("README.md", directory.path().join("linked.md")).expect("symlink fixture");
        std::fs::write(
            directory.path().join("large.txt"),
            vec![b'x'; MAX_WORKSPACE_TEXT_BYTES + 1],
        )
        .expect("large fixture");
        for path in [
            "../README.md",
            "credentials.json",
            "binary.bin",
            "control.txt",
            "long-line.txt",
            "hardlink.md",
            "linked.md",
            "large.txt",
        ] {
            assert!(
                plugin
                    .read_text(
                        &json!({"root_id":"project", "path":path}),
                        &CancellationSignal::new()
                    )
                    .is_err(),
                "{path} must be rejected"
            );
        }
    }

    #[test]
    fn configuration_and_cancellation_fail_closed() {
        assert!(WorkspaceRootConfig::parse("Project=/tmp").is_err());
        assert!(WorkspaceRootConfig::parse("project=relative").is_err());
        let (_directory, plugin) = fixture();
        let cancellation = CancellationSignal::new();
        cancellation.cancel();
        assert!(plugin
            .list(&json!({"root_id":"project", "path":"@root"}), &cancellation)
            .is_err());
        assert!(plugin
            .read_text(
                &json!({"root_id":"missing", "path":"README.md"}),
                &CancellationSignal::new()
            )
            .is_err());
    }

    #[test]
    fn rejects_secret_name_variants_and_empty_paths() {
        for name in [
            "auth_token.json",
            "github_pat.txt",
            "api_key.yaml",
            "access_key",
            "client_secret.json",
            "ssh_key",
            "private-key.pem",
            "production.env",
            "id_rsa.bak",
            "id_ed25519.old",
            "service_account.json",
            "kubeconfig",
            "application.keystore",
        ] {
            assert!(
                is_hidden_or_secret_name(name),
                "{name} must be secret-named"
            );
        }
        for safe in [
            "README.md",
            "tokenizer.rs",
            "authentication.md",
            "keynote.txt",
        ] {
            assert!(
                !is_hidden_or_secret_name(safe),
                "{safe} should remain visible"
            );
        }
        assert!(validate_relative_path("", true).is_err());
        assert!(validate_relative_path("@root", true).is_ok());
        assert!(validate_relative_path("@root", false).is_err());
    }
}
