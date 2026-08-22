use crate::publication::{
    PublicationAdapter, PublicationAdapterError, PublicationExecutionControl,
};
use crate::{
    ArtifactIntegrationStore, CandidateEvidence, PublicationActionEvidence, PublicationActionKind,
    PublicationExecutionPlan,
};
use assemblywright_protocol::{
    feature_conveyor_publication_required_checks_sha256,
    FEATURE_CONVEYOR_PUBLICATION_COORDINATOR_SCHEMA_VERSION,
};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

const CONFIG_ROOT: &str = "github-publication";
const CONFIG_FILE: &str = "publication.json";
const GH_CONFIG_ROOT: &str = "gh-config";
const GH_HOSTS_FILE: &str = "hosts.yml";
const REPOSITORY: &str = "malak333/Assemblywright";
const OWNER: &str = "malak333";
const BASE_BRANCH: &str = "main";
const MERGE_STRATEGY: &str = "merge";
const POST_MERGE_GATE: &str = "release-local";
const RELEASE_LOCAL_CHECK_ID: &str = "release-local";
const WINDOWS_CHECK_ID: &str = "protocol-windows";
const RELEASE_LOCAL_CONTEXT: &str = "Release local gate";
const WINDOWS_CONTEXT: &str = "Protocol, master, identity, mTLS, and SCM";
const GITHUB_ACTIONS_APP_ID: u64 = 15_368;
const RELEASE_LOCAL_WORKFLOW_ID: u64 = 282_605_278;
const WINDOWS_WORKFLOW_ID: u64 = 314_849_303;
const RELEASE_LOCAL_WORKFLOW_PATH: &str = ".github/workflows/release-local.yml";
const WINDOWS_WORKFLOW_PATH: &str = ".github/workflows/windows-protocol.yml";
const RELEASE_LOCAL_WORKFLOW_SHA256: &str =
    "51e809a94f59193e213bdff6e49f3a86e612643f094e366055f42f8745026fd7";
const WINDOWS_WORKFLOW_SHA256: &str =
    "da1ebe295c34f3442ff2a3537ca617642c436b019cf5009843546fefb9f914a0";
const MAX_CONFIG_BYTES: usize = 16 * 1024;
const MAX_GH_OUTPUT_BYTES: usize = 512 * 1024;
const COMMAND_POLL_INTERVAL: Duration = Duration::from_millis(25);
const CHECK_POLL_INTERVAL: Duration = Duration::from_secs(5);
const RECONCILE_POLL_INTERVAL: Duration = Duration::from_secs(2);
/// Hosted checks may run substantially longer than one local process launch;
/// current authority and cancellation remain polled throughout this window.
pub const GITHUB_PUBLICATION_ACTION_DEADLINE: Duration = Duration::from_secs(45 * 60);
const FIXED_REMOTE: &str = "https://github.com/malak333/Assemblywright.git";
#[cfg(windows)]
const FIXED_GH_EXECUTABLE: &str = r"C:\Users\mike\AppData\Local\Microsoft\WinGet\Packages\GitHub.cli_Microsoft.Winget.Source_8wekyb3d8bbwe\bin\gh.exe";
#[cfg(windows)]
const FIXED_GIT_EXECUTABLE: &str = r"C:\Program Files\Git\cmd\git.exe";
#[cfg(windows)]
const PUBLICATION_LAUNCHER_MARKER: &str = "__assemblywright_github_publication_launcher_v1";
#[cfg(windows)]
const PUBLICATION_LAUNCH_GATE: u8 = 0xb9;

#[derive(Debug, thiserror::Error)]
pub enum GithubPublicationConfigError {
    #[error("GitHub publication configuration is incomplete or invalid")]
    Invalid,
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct GithubPublicationConfig {
    schema_version: u16,
    enabled: bool,
    repository: String,
    base_branch: String,
    merge_strategy: String,
    post_merge_gate: String,
    required_checks: Vec<ConfiguredCheck>,
    gh_executable_sha256: String,
    git_executable_sha256: String,
    master_executable_sha256: String,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct ConfiguredCheck {
    id: String,
    workflow: String,
    context: String,
    app_id: u64,
    workflow_id: u64,
    workflow_path: String,
    workflow_sha256: String,
}

#[derive(Debug, Clone)]
struct ExecutableIdentity {
    sha256: [u8; 32],
    length: u64,
    modified: std::time::SystemTime,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(windows)]
    volume_serial: u32,
    #[cfg(windows)]
    file_index: u64,
}

#[cfg(windows)]
#[derive(Clone, Copy)]
enum PublicationExecutableKind {
    Gh,
    Git,
}

struct PublicationCommand {
    #[cfg(windows)]
    kind: PublicationExecutableKind,
    executable: PathBuf,
    identity: ExecutableIdentity,
    #[cfg(windows)]
    master: PathBuf,
    #[cfg(windows)]
    master_identity: ExecutableIdentity,
    root: PathBuf,
    gh: PathBuf,
    gh_config: PathBuf,
    current_dir: PathBuf,
    arguments: Vec<OsString>,
}

impl PublicationCommand {
    fn arg(&mut self, value: impl AsRef<OsStr>) -> &mut Self {
        self.arguments.push(value.as_ref().to_os_string());
        self
    }

    fn args<I, S>(&mut self, values: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.arguments.extend(
            values
                .into_iter()
                .map(|value| value.as_ref().to_os_string()),
        );
        self
    }

    fn current_dir(&mut self, value: impl AsRef<Path>) -> &mut Self {
        self.current_dir = value.as_ref().to_path_buf();
        self
    }

    fn build(self) -> Result<PreparedPublicationCommand, PublicationAdapterError> {
        #[cfg(windows)]
        let (mut command, executable_guard, gated) = {
            let master_guard = open_verified_executable(&self.master, &self.master_identity)?;
            let mut command = Command::new(&self.master);
            command
                .arg(PUBLICATION_LAUNCHER_MARKER)
                .arg(match self.kind {
                    PublicationExecutableKind::Gh => "gh",
                    PublicationExecutableKind::Git => "git",
                })
                .arg(hex(&self.identity.sha256))
                .arg(self.identity.length.to_string())
                .arg(self.identity.volume_serial.to_string())
                .arg(self.identity.file_index.to_string())
                .arg("--")
                .args(&self.arguments);
            (command, master_guard, true)
        };
        #[cfg(not(windows))]
        let (mut command, executable_guard, gated) = {
            let guard = open_verified_executable(&self.executable, &self.identity)?;
            let mut command = Command::new(&self.executable);
            command.args(&self.arguments);
            (command, guard, false)
        };
        command
            .current_dir(&self.current_dir)
            .env_clear()
            .env("GH_CONFIG_DIR", &self.gh_config)
            .env("GH_PROMPT_DISABLED", "1")
            .env("NO_COLOR", "1")
            .env(
                "PATH",
                sanitized_publication_command_path(&self.executable, &self.gh, &self.root)?,
            )
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .stdin(if gated { Stdio::piped() } else { Stdio::null() })
            .stderr(Stdio::null());
        #[cfg(windows)]
        if let Some(system_root) = std::env::var_os("SystemRoot") {
            command.env("SystemRoot", system_root);
        }
        Ok(PreparedPublicationCommand {
            command,
            _executable_guard: executable_guard,
            gated,
        })
    }
}

struct PreparedPublicationCommand {
    command: Command,
    _executable_guard: File,
    gated: bool,
}

#[cfg(windows)]
pub fn github_publication_launcher_exit_code() -> Option<i32> {
    let mut arguments = std::env::args_os().skip(1);
    if arguments.next().as_deref() != Some(OsStr::new(PUBLICATION_LAUNCHER_MARKER)) {
        return None;
    }
    let target = match arguments.next().and_then(|value| value.into_string().ok()) {
        Some(value) => value,
        None => return Some(1),
    };
    let expected_sha256 = match arguments.next().and_then(|value| value.into_string().ok()) {
        Some(value) if is_lowercase_sha256(&value) => value,
        _ => return Some(1),
    };
    let expected_length = match arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .and_then(|value| value.parse::<u64>().ok())
    {
        Some(value) if value > 0 => value,
        _ => return Some(1),
    };
    let expected_volume = match arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .and_then(|value| value.parse::<u32>().ok())
    {
        Some(value) => value,
        None => return Some(1),
    };
    let expected_index = match arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .and_then(|value| value.parse::<u64>().ok())
    {
        Some(value) => value,
        None => return Some(1),
    };
    if arguments.next().as_deref() != Some(OsStr::new("--")) {
        return Some(1);
    }
    let child_arguments = arguments.collect::<Vec<_>>();
    let executable = match target.as_str() {
        "gh" => Path::new(FIXED_GH_EXECUTABLE),
        "git" => Path::new(FIXED_GIT_EXECUTABLE),
        _ => return Some(1),
    };
    let guard = match open_locked_windows_executable(
        executable,
        &expected_sha256,
        expected_length,
        expected_volume,
        expected_index,
    ) {
        Ok(file) => file,
        Err(_) => return Some(1),
    };
    let mut gate = [0_u8; 1];
    if std::io::stdin().read_exact(&mut gate).is_err() || gate[0] != PUBLICATION_LAUNCH_GATE {
        return Some(1);
    }
    let mut command = Command::new(executable);
    command
        .args(child_arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let result = command.spawn().and_then(|mut child| child.wait());
    drop(guard);
    Some(match result {
        Ok(status) if status.success() => 0,
        _ => 1,
    })
}

#[derive(Debug, Clone)]
pub struct ProcessGithubPublication {
    data_dir: PathBuf,
    root: PathBuf,
    gh_config: PathBuf,
    gh: PathBuf,
    git: PathBuf,
    gh_identity: ExecutableIdentity,
    git_identity: ExecutableIdentity,
    master: PathBuf,
    master_identity: ExecutableIdentity,
}

pub struct GithubPublicationSession {
    runtime: ProcessGithubPublication,
    store: ArtifactIntegrationStore,
    candidate: CandidateEvidence,
    pull_request_number: Option<u64>,
    merge_commit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GithubPublicationLiveProofReceipt {
    pub schema_version: u16,
    pub status: String,
    pub repository: String,
    pub base_branch: String,
    pub source_head: String,
    pub publication_commit: String,
    pub resulting_main_commit: String,
    pub pull_request_number: u64,
    pub pull_request_url_sha256: String,
    pub branch_name_sha256: String,
    pub required_checks_sha256: String,
    pub post_merge_checks_sha256: String,
    pub master_executable_sha256: String,
    pub observed_at_ms: u64,
}

impl GithubPublicationLiveProofReceipt {
    pub fn validate(&self) -> Result<(), PublicationAdapterError> {
        if self.schema_version != 1
            || self.status != "github_publication_live_proof_passed"
            || self.repository != REPOSITORY
            || self.base_branch != BASE_BRANCH
            || !is_git_commit(&self.source_head)
            || !is_git_commit(&self.publication_commit)
            || !is_git_commit(&self.resulting_main_commit)
            || self.source_head == self.publication_commit
            || self.source_head == self.resulting_main_commit
            || self.publication_commit == self.resulting_main_commit
            || self.pull_request_number == 0
            || !is_lowercase_sha256(&self.pull_request_url_sha256)
            || !is_lowercase_sha256(&self.branch_name_sha256)
            || !is_lowercase_sha256(&self.required_checks_sha256)
            || !is_lowercase_sha256(&self.post_merge_checks_sha256)
            || !is_lowercase_sha256(&self.master_executable_sha256)
            || self.observed_at_ms == 0
        {
            return Err(PublicationAdapterError::MissingEvidence);
        }
        Ok(())
    }
}

impl ProcessGithubPublication {
    pub fn load(data_dir: &Path) -> Result<Option<Self>, GithubPublicationConfigError> {
        let root = data_dir.join(CONFIG_ROOT);
        let config_path = root.join(CONFIG_FILE);
        #[cfg(windows)]
        let gh = PathBuf::from(FIXED_GH_EXECUTABLE);
        #[cfg(not(windows))]
        let gh = root.join("gh");
        #[cfg(windows)]
        let git = PathBuf::from(FIXED_GIT_EXECUTABLE);
        #[cfg(not(windows))]
        let git = root.join("git");
        let gh_config = root.join(GH_CONFIG_ROOT);
        let master = std::env::current_exe()?;
        if !root.exists() && !config_path.exists() && !gh_config.exists() {
            return Ok(None);
        }
        for path in [&root, &config_path, &gh_config, &gh, &git, &master] {
            reject_link(path)?;
        }
        let root = fs::canonicalize(root)?;
        let config_path = fs::canonicalize(config_path)?;
        let gh = fs::canonicalize(gh)?;
        let git = fs::canonicalize(git)?;
        let gh_config = fs::canonicalize(gh_config)?;
        if config_path.parent() != Some(root.as_path())
            || gh_config.parent() != Some(root.as_path())
            || !fs::metadata(&config_path)?.is_file()
            || !fs::metadata(&gh)?.is_file()
            || !fs::metadata(&git)?.is_file()
            || !fs::metadata(&gh_config)?.is_dir()
        {
            return Err(GithubPublicationConfigError::Invalid);
        }
        #[cfg(not(windows))]
        if gh.parent() != Some(root.as_path()) || git.parent() != Some(root.as_path()) {
            return Err(GithubPublicationConfigError::Invalid);
        }
        validate_private_path(&root, true)?;
        validate_private_path(&config_path, false)?;
        validate_private_path(&gh_config, true)?;
        let hosts = gh_config.join(GH_HOSTS_FILE);
        reject_link(&hosts)?;
        validate_private_path(&hosts, false)?;
        let hosts_bytes = fs::read(&hosts)?;
        if hosts_bytes.is_empty()
            || hosts_bytes.len() > MAX_CONFIG_BYTES
            || contains_plaintext_token(&hosts_bytes)
        {
            return Err(GithubPublicationConfigError::Invalid);
        }
        let config_bytes = fs::read(&config_path)?;
        if config_bytes.is_empty() || config_bytes.len() > MAX_CONFIG_BYTES {
            return Err(GithubPublicationConfigError::Invalid);
        }
        let config: GithubPublicationConfig = serde_json::from_slice(&config_bytes)?;
        validate_config(&config)?;
        let gh_identity = load_executable_identity(&gh, &config.gh_executable_sha256)?;
        let git_identity = load_executable_identity(&git, &config.git_executable_sha256)?;
        let master = fs::canonicalize(master)?;
        let master_identity = load_executable_identity(&master, &config.master_executable_sha256)?;
        let canonical_data_dir = root
            .parent()
            .ok_or(GithubPublicationConfigError::Invalid)?
            .to_path_buf();
        Ok(Some(Self {
            data_dir: canonical_data_dir,
            root,
            gh_config,
            gh,
            git,
            gh_identity,
            git_identity,
            master,
            master_identity,
        }))
    }

    pub fn bind_candidate(
        &self,
        plan: &PublicationExecutionPlan,
        store: ArtifactIntegrationStore,
        candidate: CandidateEvidence,
    ) -> Result<GithubPublicationSession, PublicationAdapterError> {
        if plan.repository_id.is_nil()
            || plan.base_branch != BASE_BRANCH
            || plan.merge_strategy != MERGE_STRATEGY
            || plan.post_merge_gate != POST_MERGE_GATE
            || plan.required_checks != fixed_check_ids()
            || candidate.integration_id != plan.request.integration_id
            || candidate.candidate_commit != plan.request.candidate_commit
            || candidate.candidate_tree != plan.request.candidate_tree
            || candidate.base_commit != plan.request.remote_base_commit
        {
            return Err(PublicationAdapterError::Unavailable);
        }
        store
            .open_verified_candidate(&candidate)
            .map_err(|_| PublicationAdapterError::Unavailable)?;
        Ok(GithubPublicationSession {
            runtime: self.clone(),
            store,
            candidate,
            pull_request_number: None,
            merge_commit: None,
        })
    }

    pub fn required_check_ids(&self) -> Vec<String> {
        fixed_check_ids()
    }

    #[doc(hidden)]
    pub fn verify_provisioned_assets(&self) -> Result<(), PublicationAdapterError> {
        self.verify_assets()
    }

    /// Verifies the credential store, pinned repository, current main, and
    /// no-bypass protection before the coordinator is allowed to persist an
    /// external-effect intent. This performs only authenticated reads.
    pub fn preflight(&self) -> Result<(), PublicationAdapterError> {
        let control = PublicationExecutionControl::new(
            Arc::new(AtomicBool::new(false)),
            Instant::now() + Duration::from_secs(30),
            Arc::new(|| true),
        );
        let user = self.api("user", None, &[], &control)?;
        if user.get("login").and_then(Value::as_str) != Some(OWNER) {
            return Err(PublicationAdapterError::Unavailable);
        }
        let repository = self.api(&format!("repos/{REPOSITORY}"), None, &[], &control)?;
        if repository.get("full_name").and_then(Value::as_str) != Some(REPOSITORY)
            || repository.get("private").and_then(Value::as_bool) != Some(false)
        {
            return Err(PublicationAdapterError::Unavailable);
        }
        self.branch_head(BASE_BRANCH, &control)?
            .ok_or(PublicationAdapterError::Unavailable)?;
        self.observe_branch_protection(&control)
    }

    pub fn preflight_for_plan(
        &self,
        plan: &PublicationExecutionPlan,
    ) -> Result<(), PublicationAdapterError> {
        self.preflight()?;
        let control = PublicationExecutionControl::new(
            Arc::new(AtomicBool::new(false)),
            Instant::now() + Duration::from_secs(30),
            Arc::new(|| true),
        );
        let observed = self
            .branch_head(BASE_BRANCH, &control)?
            .ok_or(PublicationAdapterError::MissingEvidence)?;
        validate_remote_base_observation(&observed, &plan.request.remote_base_commit)
    }

    fn verify_assets(&self) -> Result<(), PublicationAdapterError> {
        open_verified_executable(&self.gh, &self.gh_identity)?;
        open_verified_executable(&self.git, &self.git_identity)?;
        open_verified_executable(&self.master, &self.master_identity)?;
        validate_private_path(&self.root, true)
            .map_err(|_| PublicationAdapterError::Unavailable)?;
        validate_private_path(&self.gh_config, true)
            .map_err(|_| PublicationAdapterError::Unavailable)?;
        let hosts = self.gh_config.join(GH_HOSTS_FILE);
        let bytes = fs::read(&hosts).map_err(|_| PublicationAdapterError::Unavailable)?;
        if contains_plaintext_token(&bytes) {
            return Err(PublicationAdapterError::Unavailable);
        }
        Ok(())
    }

    fn api(
        &self,
        endpoint: &str,
        method: Option<&str>,
        fields: &[(&str, &str)],
        control: &PublicationExecutionControl,
    ) -> Result<Value, PublicationAdapterError> {
        self.verify_assets()?;
        let mut command = self.command(&self.gh);
        command
            .arg("api")
            .arg("--hostname")
            .arg("github.com")
            .arg("-H")
            .arg("Accept: application/vnd.github+json")
            .arg("-H")
            .arg("X-GitHub-Api-Version: 2022-11-28")
            .arg(endpoint);
        if let Some(method) = method {
            command.arg("--method").arg(method);
        }
        for (name, value) in fields {
            command.arg("-f").arg(format!("{name}={value}"));
        }
        let bytes = run_bounded_command(command, true, control)?;
        serde_json::from_slice(&bytes).map_err(|_| PublicationAdapterError::MissingEvidence)
    }

    fn command(&self, executable: &Path) -> PublicationCommand {
        #[cfg(windows)]
        let (kind, identity) = if executable == self.gh {
            (PublicationExecutableKind::Gh, self.gh_identity.clone())
        } else {
            (PublicationExecutableKind::Git, self.git_identity.clone())
        };
        #[cfg(not(windows))]
        let identity = if executable == self.gh {
            self.gh_identity.clone()
        } else {
            self.git_identity.clone()
        };
        PublicationCommand {
            #[cfg(windows)]
            kind,
            executable: executable.to_path_buf(),
            identity,
            #[cfg(windows)]
            master: self.master.clone(),
            #[cfg(windows)]
            master_identity: self.master_identity.clone(),
            root: self.root.clone(),
            gh: self.gh.clone(),
            gh_config: self.gh_config.clone(),
            current_dir: self.root.clone(),
            arguments: Vec::new(),
        }
    }

    fn observe_branch_protection(
        &self,
        control: &PublicationExecutionControl,
    ) -> Result<(), PublicationAdapterError> {
        let protection = self.api(
            "repos/malak333/Assemblywright/branches/main/protection",
            None,
            &[],
            control,
        )?;
        parse_branch_protection(&protection)?;
        let rulesets = self.api(
            "repos/malak333/Assemblywright/rulesets?includes_parents=true&per_page=100",
            None,
            &[],
            control,
        )?;
        let summaries = rulesets
            .as_array()
            .ok_or(PublicationAdapterError::MissingEvidence)?;
        for summary in summaries {
            if summary.get("enforcement").and_then(Value::as_str) == Some("disabled") {
                continue;
            }
            let id = summary
                .get("id")
                .and_then(Value::as_u64)
                .ok_or(PublicationAdapterError::MissingEvidence)?;
            let detail = self.api(
                &format!("repos/malak333/Assemblywright/rulesets/{id}"),
                None,
                &[],
                control,
            )?;
            let bypass = detail
                .get("bypass_actors")
                .and_then(Value::as_array)
                .ok_or(PublicationAdapterError::MissingEvidence)?;
            if !bypass.is_empty() {
                return Err(PublicationAdapterError::MissingEvidence);
            }
        }
        Ok(())
    }

    fn branch_head(
        &self,
        branch: &str,
        control: &PublicationExecutionControl,
    ) -> Result<Option<String>, PublicationAdapterError> {
        let endpoint = format!("repos/{REPOSITORY}/git/ref/heads/{branch}");
        match self.api(&endpoint, None, &[], control) {
            Ok(value) => value
                .pointer("/object/sha")
                .and_then(Value::as_str)
                .filter(|value| is_git_commit(value))
                .map(str::to_string)
                .map(Some)
                .ok_or(PublicationAdapterError::MissingEvidence),
            Err(PublicationAdapterError::MissingEvidence) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn required_checks_passed(
        &self,
        commit: &str,
        control: &PublicationExecutionControl,
    ) -> Result<[u8; 32], PublicationAdapterError> {
        loop {
            control.poll()?;
            let endpoint = format!(
                "repos/{REPOSITORY}/commits/{commit}/check-runs?filter=latest&per_page=100"
            );
            let value = self.api(&endpoint, None, &[], control)?;
            match parse_required_checks(&value)? {
                CheckState::Passed(observations) => {
                    let mut evidence = Vec::with_capacity(observations.len());
                    for observation in observations {
                        let workflow = self.api(
                            &format!(
                                "repos/{REPOSITORY}/actions/runs/{}",
                                observation.workflow_run_id
                            ),
                            None,
                            &[],
                            control,
                        )?;
                        validate_workflow_run(&observation, &workflow, commit)?;
                        self.validate_workflow_content(
                            observation.workflow_path,
                            observation.workflow_sha256,
                            commit,
                            control,
                        )?;
                        evidence.push(json!({
                            "context": observation.context,
                            "check_run_id": observation.check_run_id,
                            "workflow_run_id": observation.workflow_run_id,
                            "workflow_id": observation.workflow_id,
                            "workflow_path": observation.workflow_path,
                            "head_sha": commit,
                            "conclusion": "success"
                        }));
                    }
                    let canonical = serde_json::to_vec(&evidence)
                        .map_err(|_| PublicationAdapterError::MissingEvidence)?;
                    return Ok(Sha256::digest(canonical).into());
                }
                CheckState::Pending => thread::sleep(CHECK_POLL_INTERVAL),
            }
        }
    }

    fn validate_workflow_content(
        &self,
        path: &str,
        expected_sha256: &str,
        commit: &str,
        control: &PublicationExecutionControl,
    ) -> Result<(), PublicationAdapterError> {
        let value = self.api(
            &format!("repos/{REPOSITORY}/contents/{path}?ref={commit}"),
            None,
            &[],
            control,
        )?;
        if value.get("type").and_then(Value::as_str) != Some("file")
            || value.get("encoding").and_then(Value::as_str) != Some("base64")
            || value.get("path").and_then(Value::as_str) != Some(path)
        {
            return Err(PublicationAdapterError::MissingEvidence);
        }
        let encoded = value
            .get("content")
            .and_then(Value::as_str)
            .ok_or(PublicationAdapterError::MissingEvidence)?
            .bytes()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect::<Vec<_>>();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .map_err(|_| PublicationAdapterError::MissingEvidence)?;
        validate_github_workflow_content(path, expected_sha256, &bytes)
    }

    fn git_push(
        &self,
        repository: &Path,
        commit: &str,
        branch: &str,
        control: &PublicationExecutionControl,
    ) -> Result<(), PublicationAdapterError> {
        self.verify_assets()?;
        let mut command = self.command(&self.git);
        let (credential_cwd, git_dir, _work_tree) =
            credential_git_process_boundary(&self.root, repository)?;
        command
            .current_dir(credential_cwd)
            .arg("--no-optional-locks")
            .arg("--git-dir")
            .arg(git_dir)
            .arg("-c")
            .arg("core.hooksPath=NUL")
            .arg("-c")
            .arg("credential.helper=")
            .arg("-c")
            .arg("credential.helper=!gh auth git-credential")
            .arg("-c")
            .arg("credential.useHttpPath=true")
            .arg("push")
            .arg(FIXED_REMOTE)
            .arg(format!("{commit}:refs/heads/{branch}"));
        run_bounded_command(command, false, control).map(|_| ())
    }

    fn delete_branch(
        &self,
        branch: &str,
        control: &PublicationExecutionControl,
    ) -> Result<(), PublicationAdapterError> {
        if self.branch_is_absent(branch, control)? {
            return Ok(());
        }
        self.verify_assets()?;
        let mut command = self.command(&self.gh);
        command
            .arg("api")
            .arg("--hostname")
            .arg("github.com")
            .arg("-H")
            .arg("Accept: application/vnd.github+json")
            .arg("-H")
            .arg("X-GitHub-Api-Version: 2022-11-28")
            .arg(format!("repos/{REPOSITORY}/git/refs/heads/{branch}"))
            .arg("--method")
            .arg("DELETE");
        run_bounded_command(command, false, control)?;
        if !self.branch_is_absent(branch, control)? {
            return Err(PublicationAdapterError::AmbiguousEffect);
        }
        Ok(())
    }

    fn branch_is_absent(
        &self,
        branch: &str,
        control: &PublicationExecutionControl,
    ) -> Result<bool, PublicationAdapterError> {
        let value = self.api(
            &format!("repos/{REPOSITORY}/git/matching-refs/heads/{branch}"),
            None,
            &[],
            control,
        )?;
        let refs = value
            .as_array()
            .ok_or(PublicationAdapterError::MissingEvidence)?;
        if refs.iter().any(|reference| {
            reference.get("ref").and_then(Value::as_str)
                != Some(format!("refs/heads/{branch}").as_str())
        }) {
            return Err(PublicationAdapterError::MissingEvidence);
        }
        Ok(refs.is_empty())
    }

    fn close_branch_pull_requests_best_effort(
        &self,
        branch: &str,
        control: &PublicationExecutionControl,
    ) {
        let endpoint = format!(
            "repos/{REPOSITORY}/pulls?state=open&head={OWNER}:{branch}&base={BASE_BRANCH}&per_page=100"
        );
        let Ok(value) = self.api(&endpoint, None, &[], control) else {
            return;
        };
        let Some(pulls) = value.as_array() else {
            return;
        };
        for number in pulls
            .iter()
            .filter_map(|pull| pull.get("number").and_then(Value::as_u64))
        {
            let _ = self.api(
                &format!("repos/{REPOSITORY}/pulls/{number}"),
                Some("PATCH"),
                &[("state", "closed")],
                control,
            );
        }
    }

    fn git_command(
        &self,
        directory: &Path,
        arguments: &[&str],
        credential: bool,
        capture: bool,
        control: &PublicationExecutionControl,
    ) -> Result<Vec<u8>, PublicationAdapterError> {
        self.verify_assets()?;
        let mut command = self.command(&self.git);
        if credential {
            let (credential_cwd, git_dir, work_tree) =
                credential_git_process_boundary(&self.root, directory)?;
            command
                .current_dir(credential_cwd)
                .arg("--git-dir")
                .arg(git_dir)
                .arg("--work-tree")
                .arg(work_tree);
        } else {
            command.current_dir(directory);
        }
        command.arg("--no-optional-locks");
        if credential {
            command
                .arg("-c")
                .arg("credential.helper=")
                .arg("-c")
                .arg("credential.helper=!gh auth git-credential")
                .arg("-c")
                .arg("credential.useHttpPath=true");
        }
        command.args(arguments);
        run_bounded_command(command, capture, control)
    }
}

/// Runs the same fixed credential/executable/API boundary without touching the
/// master database. It creates one uniquely named proof commit and PR, merges
/// through normal protected GitHub merge, observes the exact post-merge checks,
/// deletes the disposable branch, and returns path-free digest-only metadata.
pub fn execute_github_publication_live_proof(
    runtime: &ProcessGithubPublication,
    expected_source_head: &str,
    observed_at_ms: u64,
) -> Result<GithubPublicationLiveProofReceipt, PublicationAdapterError> {
    if !is_git_commit(expected_source_head) {
        return Err(PublicationAdapterError::MissingEvidence);
    }
    runtime.preflight()?;
    let control = PublicationExecutionControl::new(
        Arc::new(AtomicBool::new(false)),
        Instant::now() + GITHUB_PUBLICATION_ACTION_DEADLINE,
        Arc::new(|| true),
    );
    let source_head = runtime
        .branch_head(BASE_BRANCH, &control)?
        .ok_or(PublicationAdapterError::MissingEvidence)?;
    // This check precedes even local proof-work creation. The proof controller
    // therefore cannot accidentally prove a newer GitHub main than the exact
    // clean checkout it admitted.
    validate_remote_base_observation(&source_head, expected_source_head)?;
    let proof_id = Uuid::new_v4();
    let branch = format!("assemblywright-publication-proof-{proof_id}");
    let proof_root = runtime.root.join("proof-work");
    create_private_directory(&proof_root)?;
    let work = proof_root.join(proof_id.to_string());
    create_private_directory(&work)?;
    let marker_relative = format!(".assemblywright-publication-proofs/{proof_id}.json");
    let marker = work.join(&marker_relative);
    let mut created_pr = None;
    let mut merged = false;
    let result = (|| {
        runtime.git_command(
            &work,
            &["init", "--initial-branch=proof"],
            false,
            false,
            &control,
        )?;
        runtime.git_command(
            &work,
            &[
                "-c",
                "core.hooksPath=NUL",
                "fetch",
                "--depth=1",
                FIXED_REMOTE,
                BASE_BRANCH,
            ],
            true,
            false,
            &control,
        )?;
        let fetched =
            runtime.git_command(&work, &["rev-parse", "FETCH_HEAD"], false, true, &control)?;
        let fetched = String::from_utf8(fetched)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| is_git_commit(value))
            .ok_or(PublicationAdapterError::MissingEvidence)?;
        validate_proof_source_binding(&source_head, &fetched, &fetched, &source_head)?;
        runtime.git_command(
            &work,
            &[
                "-c",
                "core.hooksPath=NUL",
                "checkout",
                "--detach",
                "FETCH_HEAD",
            ],
            false,
            false,
            &control,
        )?;
        let parent = marker
            .parent()
            .ok_or(PublicationAdapterError::Unavailable)?;
        create_private_directory(parent)?;
        let marker_value = json!({
            "schema_version": 1,
            "proof_id": proof_id,
            "source_head": source_head,
            "created_at_ms": observed_at_ms,
            "boundary": "github_publication_live_proof"
        });
        let bytes =
            serde_json::to_vec(&marker_value).map_err(|_| PublicationAdapterError::Unavailable)?;
        let mut file = create_private_file(&marker)?;
        file.write_all(&bytes)
            .map_err(|_| PublicationAdapterError::Unavailable)?;
        file.sync_all()
            .map_err(|_| PublicationAdapterError::Unavailable)?;
        runtime.git_command(
            &work,
            &["-c", "core.hooksPath=NUL", "add", "--", &marker_relative],
            false,
            false,
            &control,
        )?;
        runtime.git_command(
            &work,
            &[
                "-c",
                "core.hooksPath=NUL",
                "-c",
                "user.name=Assemblywright publication proof",
                "-c",
                "user.email=assemblywright-proof@users.noreply.github.com",
                "commit",
                "-m",
                "Prove protected GitHub publication",
            ],
            false,
            false,
            &control,
        )?;
        let commit_output =
            runtime.git_command(&work, &["rev-parse", "HEAD"], false, true, &control)?;
        let publication_commit = String::from_utf8(commit_output)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| is_git_commit(value))
            .ok_or(PublicationAdapterError::MissingEvidence)?;
        let parent_output =
            runtime.git_command(&work, &["rev-parse", "HEAD^"], false, true, &control)?;
        let parent = String::from_utf8(parent_output)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| is_git_commit(value))
            .ok_or(PublicationAdapterError::MissingEvidence)?;
        let current_main = runtime
            .branch_head(BASE_BRANCH, &control)?
            .ok_or(PublicationAdapterError::MissingEvidence)?;
        validate_proof_source_binding(&source_head, &fetched, &parent, &current_main)?;
        runtime.git_push(&work, &publication_commit, &branch, &control)?;
        if runtime.branch_head(&branch, &control)?.as_deref() != Some(publication_commit.as_str()) {
            return Err(PublicationAdapterError::AmbiguousEffect);
        }
        let pr = runtime.api(
            &format!("repos/{REPOSITORY}/pulls"),
            Some("POST"),
            &[
                (
                    "title",
                    &format!("Assemblywright GitHub publication proof {proof_id}"),
                ),
                ("head", &branch),
                ("base", BASE_BRANCH),
                (
                    "body",
                    "Digest-bound live publication proof; merged marker is intentional.",
                ),
            ],
            &control,
        )?;
        let pr_number = pr
            .get("number")
            .and_then(Value::as_u64)
            .filter(|value| *value > 0)
            .ok_or(PublicationAdapterError::MissingEvidence)?;
        created_pr = Some(pr_number);
        let pr_url = pr
            .get("html_url")
            .and_then(Value::as_str)
            .filter(|value| value.starts_with("https://github.com/malak333/Assemblywright/pull/"))
            .ok_or(PublicationAdapterError::MissingEvidence)?;
        let required_checks = runtime.required_checks_passed(&publication_commit, &control)?;
        runtime.observe_branch_protection(&control)?;
        if runtime.branch_head(BASE_BRANCH, &control)?.as_deref() != Some(source_head.as_str()) {
            return Err(PublicationAdapterError::AmbiguousEffect);
        }
        let exact_pr = runtime.api(
            &format!("repos/{REPOSITORY}/pulls/{pr_number}"),
            None,
            &[],
            &control,
        )?;
        if exact_pr.pointer("/head/sha").and_then(Value::as_str)
            != Some(publication_commit.as_str())
            || exact_pr.pointer("/head/ref").and_then(Value::as_str) != Some(branch.as_str())
            || exact_pr.pointer("/base/ref").and_then(Value::as_str) != Some(BASE_BRANCH)
            || exact_pr.get("state").and_then(Value::as_str) != Some("open")
        {
            return Err(PublicationAdapterError::AmbiguousEffect);
        }
        let merge = runtime.api(
            &format!("repos/{REPOSITORY}/pulls/{pr_number}/merge"),
            Some("PUT"),
            &[
                ("merge_method", MERGE_STRATEGY),
                ("sha", &publication_commit),
            ],
            &control,
        )?;
        if merge.get("merged").and_then(Value::as_bool) != Some(true) {
            return Err(PublicationAdapterError::AmbiguousEffect);
        }
        let resulting_main_commit = merge
            .get("sha")
            .and_then(Value::as_str)
            .filter(|value| is_git_commit(value))
            .ok_or(PublicationAdapterError::AmbiguousEffect)?
            .to_string();
        merged = true;
        runtime.observe_branch_protection(&control)?;
        loop {
            control.poll()?;
            if runtime.branch_head(BASE_BRANCH, &control)?.as_deref()
                == Some(resulting_main_commit.as_str())
            {
                break;
            }
            thread::sleep(RECONCILE_POLL_INTERVAL);
        }
        let post_merge_checks = runtime.required_checks_passed(&resulting_main_commit, &control)?;
        runtime.observe_branch_protection(&control)?;
        if runtime.branch_head(BASE_BRANCH, &control)?.as_deref()
            != Some(resulting_main_commit.as_str())
        {
            return Err(PublicationAdapterError::AmbiguousEffect);
        }
        let receipt = GithubPublicationLiveProofReceipt {
            schema_version: 1,
            status: "github_publication_live_proof_passed".to_string(),
            repository: REPOSITORY.to_string(),
            base_branch: BASE_BRANCH.to_string(),
            source_head,
            publication_commit,
            resulting_main_commit,
            pull_request_number: pr_number,
            pull_request_url_sha256: hex(&Sha256::digest(pr_url.as_bytes())),
            branch_name_sha256: hex(&Sha256::digest(branch.as_bytes())),
            required_checks_sha256: hex(&required_checks),
            post_merge_checks_sha256: hex(&post_merge_checks),
            master_executable_sha256: hex(&runtime.master_identity.sha256),
            observed_at_ms,
        };
        receipt.validate()?;
        Ok(receipt)
    })();
    let cleanup_control = PublicationExecutionControl::new(
        Arc::new(AtomicBool::new(false)),
        Instant::now() + Duration::from_secs(30),
        Arc::new(|| true),
    );
    if !merged {
        if let Some(pr) = created_pr {
            let _ = runtime.api(
                &format!("repos/{REPOSITORY}/pulls/{pr}"),
                Some("PATCH"),
                &[("state", "closed")],
                &cleanup_control,
            );
        }
        runtime.close_branch_pull_requests_best_effort(&branch, &cleanup_control);
    }
    let remote_cleanup = runtime.delete_branch(&branch, &cleanup_control);
    let local_cleanup = remove_proof_work(&proof_root, proof_id);
    validate_proof_cleanup_status(remote_cleanup.is_ok(), local_cleanup.is_ok())?;
    result
}

impl PublicationAdapter for GithubPublicationSession {
    fn is_available(&self) -> bool {
        self.runtime.verify_assets().is_ok()
    }

    fn execute(
        &mut self,
        plan: &PublicationExecutionPlan,
        action: PublicationActionKind,
        control: &PublicationExecutionControl,
    ) -> Result<PublicationActionEvidence, PublicationAdapterError> {
        control.poll()?;
        self.runtime.observe_branch_protection(control)?;
        let candidate_path = self
            .runtime
            .data_dir
            .join("feature-conveyor-candidates")
            .join("candidates")
            .join(self.candidate.integration_id.to_string());
        let mut verified = self
            .store
            .open_verified_candidate(&self.candidate)
            .map_err(|_| PublicationAdapterError::AmbiguousEffect)?;
        let checks_digest =
            feature_conveyor_publication_required_checks_sha256(&plan.required_checks)
                .map_err(|_| PublicationAdapterError::MissingEvidence)?;
        let (observed, pr, merge, gate) = match action {
            PublicationActionKind::PushBranch => {
                let remote_main = self
                    .runtime
                    .branch_head(BASE_BRANCH, control)?
                    .ok_or(PublicationAdapterError::MissingEvidence)?;
                validate_remote_base_observation(&remote_main, &plan.request.remote_base_commit)?;
                match self.runtime.branch_head(&plan.feature_branch, control)? {
                    Some(head) if head == plan.request.candidate_commit => {}
                    Some(_) => return Err(PublicationAdapterError::AmbiguousEffect),
                    None => self.runtime.git_push(
                        &candidate_path,
                        &plan.request.candidate_commit,
                        &plan.feature_branch,
                        control,
                    )?,
                }
                let head = self
                    .runtime
                    .branch_head(&plan.feature_branch, control)?
                    .filter(|head| head == &plan.request.candidate_commit)
                    .ok_or(PublicationAdapterError::AmbiguousEffect)?;
                (head, None, None, false)
            }
            PublicationActionKind::UpsertPullRequest => {
                let pr = self.upsert_pull_request(plan, control)?;
                self.pull_request_number = Some(pr);
                (plan.request.candidate_commit.clone(), Some(pr), None, false)
            }
            PublicationActionKind::ObserveRequiredChecks => {
                self.runtime
                    .required_checks_passed(&plan.request.candidate_commit, control)?;
                let pr = self.require_pr()?;
                (plan.request.candidate_commit.clone(), Some(pr), None, false)
            }
            PublicationActionKind::VerifyPullRequestHead => {
                let pr = self.require_pr()?;
                self.verify_pull_request(plan, pr, control)?;
                (plan.request.candidate_commit.clone(), Some(pr), None, false)
            }
            PublicationActionKind::MergePullRequest => {
                let pr = self.require_pr()?;
                self.verify_pull_request(plan, pr, control)?;
                self.runtime
                    .required_checks_passed(&plan.request.candidate_commit, control)?;
                self.runtime.observe_branch_protection(control)?;
                self.verify_pull_request(plan, pr, control)?;
                let remote_main = self
                    .runtime
                    .branch_head(BASE_BRANCH, control)?
                    .ok_or(PublicationAdapterError::MissingEvidence)?;
                validate_remote_base_observation(&remote_main, &plan.request.remote_base_commit)?;
                let endpoint = format!("repos/{REPOSITORY}/pulls/{pr}/merge");
                let response = self.runtime.api(
                    &endpoint,
                    Some("PUT"),
                    &[
                        ("merge_method", MERGE_STRATEGY),
                        ("sha", &plan.request.candidate_commit),
                    ],
                    control,
                )?;
                if response.get("merged").and_then(Value::as_bool) != Some(true) {
                    return Err(PublicationAdapterError::AmbiguousEffect);
                }
                let commit = response
                    .get("sha")
                    .and_then(Value::as_str)
                    .filter(|value| is_git_commit(value))
                    .ok_or(PublicationAdapterError::AmbiguousEffect)?
                    .to_string();
                self.runtime.observe_branch_protection(control)?;
                self.merge_commit = Some(commit.clone());
                (
                    plan.request.candidate_commit.clone(),
                    Some(pr),
                    Some(commit),
                    false,
                )
            }
            PublicationActionKind::ReconcileRemoteMain => {
                let merge = self.require_merge()?.to_string();
                loop {
                    control.poll()?;
                    if self.runtime.branch_head(BASE_BRANCH, control)?.as_deref()
                        == Some(merge.as_str())
                    {
                        break;
                    }
                    thread::sleep(RECONCILE_POLL_INTERVAL);
                }
                (merge.clone(), None, Some(merge), false)
            }
            PublicationActionKind::RunPostMergeGate => {
                let merge = self.require_merge()?.to_string();
                self.runtime.required_checks_passed(&merge, control)?;
                self.runtime.observe_branch_protection(control)?;
                if self.runtime.branch_head(BASE_BRANCH, control)?.as_deref()
                    != Some(merge.as_str())
                {
                    return Err(PublicationAdapterError::AmbiguousEffect);
                }
                (merge.clone(), None, Some(merge), true)
            }
        };
        control.poll()?;
        verified
            .revalidate(&self.store)
            .map_err(|_| PublicationAdapterError::AmbiguousEffect)?;
        evidence(plan, action, observed, pr, checks_digest, merge, gate)
    }
}

impl GithubPublicationSession {
    fn require_pr(&self) -> Result<u64, PublicationAdapterError> {
        self.pull_request_number
            .ok_or(PublicationAdapterError::MissingEvidence)
    }

    fn require_merge(&self) -> Result<&str, PublicationAdapterError> {
        self.merge_commit
            .as_deref()
            .ok_or(PublicationAdapterError::MissingEvidence)
    }

    fn upsert_pull_request(
        &self,
        plan: &PublicationExecutionPlan,
        control: &PublicationExecutionControl,
    ) -> Result<u64, PublicationAdapterError> {
        let endpoint = format!(
            "repos/{REPOSITORY}/pulls?state=open&head={OWNER}:{}&base={BASE_BRANCH}&per_page=100",
            plan.feature_branch
        );
        let existing = self.runtime.api(&endpoint, None, &[], control)?;
        let pulls = existing
            .as_array()
            .ok_or(PublicationAdapterError::MissingEvidence)?;
        let value = match pulls.as_slice() {
            [] => self.runtime.api(
                &format!("repos/{REPOSITORY}/pulls"),
                Some("POST"),
                &[
                    (
                        "title",
                        &format!("Assemblywright feature {}", plan.request.feature_id),
                    ),
                    ("head", &plan.feature_branch),
                    ("base", BASE_BRANCH),
                    ("body", "Assemblywright digest-bound publication."),
                ],
                control,
            )?,
            [value] => (*value).clone(),
            _ => return Err(PublicationAdapterError::AmbiguousEffect),
        };
        let number = value
            .get("number")
            .and_then(Value::as_u64)
            .filter(|number| *number > 0)
            .ok_or(PublicationAdapterError::MissingEvidence)?;
        self.verify_pull_request(plan, number, control)?;
        Ok(number)
    }

    fn verify_pull_request(
        &self,
        plan: &PublicationExecutionPlan,
        number: u64,
        control: &PublicationExecutionControl,
    ) -> Result<Value, PublicationAdapterError> {
        let value = self.runtime.api(
            &format!("repos/{REPOSITORY}/pulls/{number}"),
            None,
            &[],
            control,
        )?;
        if value.pointer("/head/sha").and_then(Value::as_str)
            != Some(plan.request.candidate_commit.as_str())
            || value.pointer("/head/ref").and_then(Value::as_str)
                != Some(plan.feature_branch.as_str())
            || value.pointer("/base/ref").and_then(Value::as_str) != Some(BASE_BRANCH)
            || value.get("state").and_then(Value::as_str) != Some("open")
        {
            return Err(PublicationAdapterError::AmbiguousEffect);
        }
        Ok(value)
    }
}

fn evidence(
    plan: &PublicationExecutionPlan,
    action: PublicationActionKind,
    observed_head_commit: String,
    pull_request_number: Option<u64>,
    checks_digest: [u8; 32],
    resulting_main_commit: Option<String>,
    post_merge_gate_passed: bool,
) -> Result<PublicationActionEvidence, PublicationAdapterError> {
    let checks = !matches!(
        action,
        PublicationActionKind::PushBranch | PublicationActionKind::UpsertPullRequest
    );
    let merge = matches!(
        action,
        PublicationActionKind::MergePullRequest
            | PublicationActionKind::ReconcileRemoteMain
            | PublicationActionKind::RunPostMergeGate
    );
    PublicationActionEvidence {
        schema_version: FEATURE_CONVEYOR_PUBLICATION_COORDINATOR_SCHEMA_VERSION,
        publication_id: plan.request.publication_id,
        action,
        remote_base_commit: plan.request.remote_base_commit.clone(),
        candidate_commit: plan.request.candidate_commit.clone(),
        feature_branch: plan.feature_branch.clone(),
        base_branch: plan.base_branch.clone(),
        pull_request_number,
        observed_head_commit,
        required_checks_sha256: checks.then_some(checks_digest),
        required_check_count: if checks {
            u16::try_from(plan.required_checks.len())
                .map_err(|_| PublicationAdapterError::MissingEvidence)?
        } else {
            0
        },
        required_checks_passed: checks,
        branch_protection_enforced: true,
        bypass_used: false,
        merge_strategy: merge.then(|| plan.merge_strategy.clone()),
        resulting_main_commit,
        post_merge_gate_id: post_merge_gate_passed.then(|| POST_MERGE_GATE.to_string()),
        post_merge_gate_passed,
        evidence_sha256: [0; 32],
    }
    .seal()
    .map_err(|_| PublicationAdapterError::MissingEvidence)
}

fn validate_config(config: &GithubPublicationConfig) -> Result<(), GithubPublicationConfigError> {
    let expected_checks = vec![
        ConfiguredCheck {
            id: RELEASE_LOCAL_CHECK_ID.to_string(),
            workflow: "Assemblywright Release Local Gate".to_string(),
            context: RELEASE_LOCAL_CONTEXT.to_string(),
            app_id: GITHUB_ACTIONS_APP_ID,
            workflow_id: RELEASE_LOCAL_WORKFLOW_ID,
            workflow_path: RELEASE_LOCAL_WORKFLOW_PATH.to_string(),
            workflow_sha256: RELEASE_LOCAL_WORKFLOW_SHA256.to_string(),
        },
        ConfiguredCheck {
            id: WINDOWS_CHECK_ID.to_string(),
            workflow: "Assemblywright Windows Distributed Gate".to_string(),
            context: WINDOWS_CONTEXT.to_string(),
            app_id: GITHUB_ACTIONS_APP_ID,
            workflow_id: WINDOWS_WORKFLOW_ID,
            workflow_path: WINDOWS_WORKFLOW_PATH.to_string(),
            workflow_sha256: WINDOWS_WORKFLOW_SHA256.to_string(),
        },
    ];
    if config.schema_version != 1
        || !config.enabled
        || config.repository != REPOSITORY
        || config.base_branch != BASE_BRANCH
        || config.merge_strategy != MERGE_STRATEGY
        || config.post_merge_gate != POST_MERGE_GATE
        || config.required_checks != expected_checks
        || !is_lowercase_sha256(&config.gh_executable_sha256)
        || !is_lowercase_sha256(&config.git_executable_sha256)
        || !is_lowercase_sha256(&config.master_executable_sha256)
    {
        return Err(GithubPublicationConfigError::Invalid);
    }
    Ok(())
}

fn fixed_check_ids() -> Vec<String> {
    vec![
        WINDOWS_CHECK_ID.to_string(),
        RELEASE_LOCAL_CHECK_ID.to_string(),
    ]
}

fn parse_branch_protection(value: &Value) -> Result<(), PublicationAdapterError> {
    let checks = value
        .pointer("/required_status_checks/checks")
        .and_then(Value::as_array)
        .ok_or(PublicationAdapterError::MissingEvidence)?;
    let identities = checks
        .iter()
        .map(|check| {
            Some((
                check.get("context")?.as_str()?,
                check.get("app_id")?.as_u64()?,
            ))
        })
        .collect::<Option<Vec<_>>>()
        .ok_or(PublicationAdapterError::MissingEvidence)?;
    let pull_request_reviews = value
        .get("required_pull_request_reviews")
        .and_then(Value::as_object)
        .ok_or(PublicationAdapterError::MissingEvidence)?;
    let intended_pull_request_policy = pull_request_reviews
        .get("dismiss_stale_reviews")
        .and_then(Value::as_bool)
        == Some(false)
        && pull_request_reviews
            .get("require_code_owner_reviews")
            .and_then(Value::as_bool)
            == Some(false)
        && pull_request_reviews
            .get("require_last_push_approval")
            .and_then(Value::as_bool)
            == Some(false)
        && pull_request_reviews
            .get("required_approving_review_count")
            .and_then(Value::as_u64)
            == Some(0);
    let bypass_empty = match pull_request_reviews.get("bypass_pull_request_allowances") {
        None => true,
        Some(Value::Object(bypass))
            if bypass
                .keys()
                .all(|key| matches!(key.as_str(), "users" | "teams" | "apps"))
                && bypass
                    .values()
                    .all(|entry| entry.as_array().is_some_and(Vec::is_empty)) =>
        {
            true
        }
        Some(_) => false,
    };
    if value
        .pointer("/required_status_checks/strict")
        .and_then(Value::as_bool)
        != Some(true)
        || identities.len() != 2
        || !identities.contains(&(RELEASE_LOCAL_CONTEXT, GITHUB_ACTIONS_APP_ID))
        || !identities.contains(&(WINDOWS_CONTEXT, GITHUB_ACTIONS_APP_ID))
        || !intended_pull_request_policy
        || value
            .pointer("/enforce_admins/enabled")
            .and_then(Value::as_bool)
            != Some(true)
        || value
            .pointer("/required_conversation_resolution/enabled")
            .and_then(Value::as_bool)
            != Some(true)
        || !bypass_empty
        || value
            .pointer("/allow_force_pushes/enabled")
            .and_then(Value::as_bool)
            != Some(false)
        || value
            .pointer("/allow_deletions/enabled")
            .and_then(Value::as_bool)
            != Some(false)
    {
        return Err(PublicationAdapterError::MissingEvidence);
    }
    Ok(())
}

#[doc(hidden)]
pub fn validate_github_branch_protection_observation(
    value: &Value,
) -> Result<(), PublicationAdapterError> {
    parse_branch_protection(value)
}

#[doc(hidden)]
pub fn validate_github_required_checks_observation(
    value: &Value,
    workflow_runs: &Value,
    commit: &str,
) -> Result<[u8; 32], PublicationAdapterError> {
    match parse_required_checks(value)? {
        CheckState::Passed(observations) => {
            let runs = workflow_runs
                .as_array()
                .ok_or(PublicationAdapterError::MissingEvidence)?;
            let mut evidence = Vec::new();
            for observation in observations {
                let run = runs
                    .iter()
                    .find(|run| {
                        run.get("id").and_then(Value::as_u64) == Some(observation.workflow_run_id)
                    })
                    .ok_or(PublicationAdapterError::MissingEvidence)?;
                validate_workflow_run(&observation, run, commit)?;
                evidence.push(json!({
                    "context": observation.context,
                    "check_run_id": observation.check_run_id,
                    "workflow_run_id": observation.workflow_run_id,
                    "workflow_id": observation.workflow_id,
                    "workflow_path": observation.workflow_path,
                    "head_sha": commit,
                    "conclusion": "success"
                }));
            }
            let canonical = serde_json::to_vec(&evidence)
                .map_err(|_| PublicationAdapterError::MissingEvidence)?;
            Ok(Sha256::digest(canonical).into())
        }
        CheckState::Pending => Err(PublicationAdapterError::MissingEvidence),
    }
}

enum CheckState {
    Pending,
    Passed(Vec<CheckObservation>),
}

struct CheckObservation {
    context: &'static str,
    check_run_id: u64,
    workflow_run_id: u64,
    workflow_id: u64,
    workflow_path: &'static str,
    workflow_sha256: &'static str,
}

fn parse_required_checks(value: &Value) -> Result<CheckState, PublicationAdapterError> {
    let runs = value
        .get("check_runs")
        .and_then(Value::as_array)
        .ok_or(PublicationAdapterError::MissingEvidence)?;
    let mut observed = Vec::new();
    for (context, workflow_id, workflow_path, workflow_sha256) in [
        (
            RELEASE_LOCAL_CONTEXT,
            RELEASE_LOCAL_WORKFLOW_ID,
            RELEASE_LOCAL_WORKFLOW_PATH,
            RELEASE_LOCAL_WORKFLOW_SHA256,
        ),
        (
            WINDOWS_CONTEXT,
            WINDOWS_WORKFLOW_ID,
            WINDOWS_WORKFLOW_PATH,
            WINDOWS_WORKFLOW_SHA256,
        ),
    ] {
        let latest = runs
            .iter()
            .filter(|run| {
                run.get("name").and_then(Value::as_str) == Some(context)
                    && run.pointer("/app/id").and_then(Value::as_u64) == Some(GITHUB_ACTIONS_APP_ID)
            })
            .max_by_key(|run| run.get("id").and_then(Value::as_u64).unwrap_or(0));
        let Some(run) = latest else {
            return Ok(CheckState::Pending);
        };
        if run.get("status").and_then(Value::as_str) != Some("completed") {
            return Ok(CheckState::Pending);
        }
        if run.get("conclusion").and_then(Value::as_str) != Some("success") {
            return Err(PublicationAdapterError::MissingEvidence);
        }
        let id = run
            .get("id")
            .and_then(Value::as_u64)
            .ok_or(PublicationAdapterError::MissingEvidence)?;
        let workflow_run_id = run
            .get("details_url")
            .and_then(Value::as_str)
            .and_then(parse_workflow_run_id)
            .ok_or(PublicationAdapterError::MissingEvidence)?;
        observed.push(CheckObservation {
            context,
            check_run_id: id,
            workflow_run_id,
            workflow_id,
            workflow_path,
            workflow_sha256,
        });
    }
    Ok(CheckState::Passed(observed))
}

#[doc(hidden)]
pub fn validate_github_workflow_content(
    path: &str,
    expected_sha256: &str,
    bytes: &[u8],
) -> Result<(), PublicationAdapterError> {
    let trusted = match path {
        RELEASE_LOCAL_WORKFLOW_PATH => RELEASE_LOCAL_WORKFLOW_SHA256,
        WINDOWS_WORKFLOW_PATH => WINDOWS_WORKFLOW_SHA256,
        _ => return Err(PublicationAdapterError::MissingEvidence),
    };
    if expected_sha256 != trusted || hex(&Sha256::digest(bytes)) != trusted {
        return Err(PublicationAdapterError::MissingEvidence);
    }
    Ok(())
}

#[doc(hidden)]
pub fn validate_remote_base_observation(
    observed: &str,
    expected: &str,
) -> Result<(), PublicationAdapterError> {
    if !is_git_commit(observed) || !is_git_commit(expected) || observed != expected {
        return Err(PublicationAdapterError::MissingEvidence);
    }
    Ok(())
}

#[doc(hidden)]
pub fn validate_proof_source_binding(
    source_head: &str,
    fetched_head: &str,
    proof_parent: &str,
    current_main: &str,
) -> Result<(), PublicationAdapterError> {
    if [source_head, fetched_head, proof_parent, current_main]
        .iter()
        .any(|commit| !is_git_commit(commit))
        || fetched_head != source_head
        || proof_parent != source_head
        || current_main != source_head
    {
        return Err(PublicationAdapterError::AmbiguousEffect);
    }
    Ok(())
}

#[doc(hidden)]
pub fn validate_proof_cleanup_status(
    remote_branch_absent: bool,
    local_work_absent: bool,
) -> Result<(), PublicationAdapterError> {
    if !remote_branch_absent || !local_work_absent {
        return Err(PublicationAdapterError::AmbiguousEffect);
    }
    Ok(())
}

#[doc(hidden)]
pub fn sanitized_publication_command_path(
    executable: &Path,
    gh: &Path,
    root: &Path,
) -> Result<OsString, PublicationAdapterError> {
    let executable_parent = executable
        .parent()
        .ok_or(PublicationAdapterError::Unavailable)?;
    let gh_parent = gh.parent().ok_or(PublicationAdapterError::Unavailable)?;
    std::env::join_paths([executable_parent, gh_parent, root])
        .map_err(|_| PublicationAdapterError::Unavailable)
}

#[doc(hidden)]
pub fn credential_git_process_boundary(
    publication_root: &Path,
    repository: &Path,
) -> Result<(PathBuf, PathBuf, PathBuf), PublicationAdapterError> {
    let data_dir = publication_root
        .parent()
        .ok_or(PublicationAdapterError::Unavailable)?;
    if !publication_root.is_absolute()
        || !repository.is_absolute()
        || repository == publication_root
        || repository == data_dir
        || !repository.starts_with(data_dir)
    {
        return Err(PublicationAdapterError::Unavailable);
    }
    let credential_cwd = publication_root.to_path_buf();
    let git_dir = repository.join(".git");
    let work_tree = repository.to_path_buf();
    #[cfg(windows)]
    {
        Ok((
            windows_git_process_path(&credential_cwd)?,
            windows_git_process_path(&git_dir)?,
            windows_git_process_path(&work_tree)?,
        ))
    }
    #[cfg(not(windows))]
    Ok((credential_cwd, git_dir, work_tree))
}

#[cfg(windows)]
fn windows_git_process_path(path: &Path) -> Result<PathBuf, PublicationAdapterError> {
    use std::path::{Component, Prefix};

    let value = path.to_str().ok_or(PublicationAdapterError::Unavailable)?;
    let normalized = if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        let mut components = rest.split('\\');
        if components.next().is_none_or(str::is_empty)
            || components.next().is_none_or(str::is_empty)
        {
            return Err(PublicationAdapterError::Unavailable);
        }
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = value.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    };
    let permitted_prefix = matches!(
        normalized.components().next(),
        Some(Component::Prefix(prefix))
            if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::UNC(_, _))
    );
    if !normalized.is_absolute() || !permitted_prefix {
        return Err(PublicationAdapterError::Unavailable);
    }
    Ok(normalized)
}

fn parse_workflow_run_id(value: &str) -> Option<u64> {
    let suffix = value.strip_prefix("https://github.com/malak333/Assemblywright/actions/runs/")?;
    let (run, tail) = suffix.split_once('/')?;
    if !tail.starts_with("job/") {
        return None;
    }
    run.parse().ok().filter(|id| *id > 0)
}

fn validate_workflow_run(
    observation: &CheckObservation,
    run: &Value,
    commit: &str,
) -> Result<(), PublicationAdapterError> {
    if run.get("id").and_then(Value::as_u64) != Some(observation.workflow_run_id)
        || run.get("workflow_id").and_then(Value::as_u64) != Some(observation.workflow_id)
        || run.get("path").and_then(Value::as_str) != Some(observation.workflow_path)
        || run.get("head_sha").and_then(Value::as_str) != Some(commit)
        || run.pointer("/repository/full_name").and_then(Value::as_str) != Some(REPOSITORY)
        || !matches!(
            run.get("event").and_then(Value::as_str),
            Some("push" | "pull_request")
        )
        || run.get("status").and_then(Value::as_str) != Some("completed")
        || run.get("conclusion").and_then(Value::as_str) != Some("success")
    {
        return Err(PublicationAdapterError::MissingEvidence);
    }
    Ok(())
}

fn run_bounded_command(
    command: PublicationCommand,
    capture_stdout: bool,
    control: &PublicationExecutionControl,
) -> Result<Vec<u8>, PublicationAdapterError> {
    control.poll()?;
    let mut prepared = command.build()?;
    prepared.command.stdout(if capture_stdout {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    let mut child = prepared
        .command
        .spawn()
        .map_err(|_| PublicationAdapterError::Unavailable)?;
    #[cfg(windows)]
    let containment = match WindowsPublicationContainment::assign(&child) {
        Ok(containment) => containment,
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
    };
    #[cfg(windows)]
    if prepared.gated
        && child
            .stdin
            .take()
            .is_none_or(|mut stdin| stdin.write_all(&[PUBLICATION_LAUNCH_GATE]).is_err())
    {
        containment.terminate();
        let _ = child.kill();
        let _ = child.wait();
        return Err(PublicationAdapterError::Unavailable);
    }
    #[cfg(not(windows))]
    debug_assert!(!prepared.gated);
    let reader = child.stdout.take().map(|mut stdout| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            Read::by_ref(&mut stdout)
                .take((MAX_GH_OUTPUT_BYTES + 1) as u64)
                .read_to_end(&mut bytes)
                .map(|_| bytes)
        })
    });
    loop {
        if let Err(error) = control.poll() {
            #[cfg(windows)]
            containment.terminate();
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                let bytes = match reader {
                    Some(reader) => reader
                        .join()
                        .map_err(|_| PublicationAdapterError::AmbiguousEffect)?
                        .map_err(|_| PublicationAdapterError::AmbiguousEffect)?,
                    None => Vec::new(),
                };
                if !status.success() || bytes.len() > MAX_GH_OUTPUT_BYTES {
                    return Err(PublicationAdapterError::MissingEvidence);
                }
                return Ok(bytes);
            }
            Ok(None) => thread::sleep(COMMAND_POLL_INTERVAL),
            Err(_) => {
                #[cfg(windows)]
                containment.terminate();
                let _ = child.kill();
                let _ = child.wait();
                return Err(PublicationAdapterError::AmbiguousEffect);
            }
        }
    }
}

#[cfg(windows)]
struct WindowsPublicationContainment {
    job: windows_sys::Win32::Foundation::HANDLE,
}

#[cfg(windows)]
impl WindowsPublicationContainment {
    fn assign(child: &std::process::Child) -> Result<Self, PublicationAdapterError> {
        use std::mem::{size_of, zeroed};
        use std::os::windows::io::AsRawHandle;
        use std::ptr::null;
        use windows_sys::Win32::System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        };
        let job = unsafe { CreateJobObjectW(null(), null()) };
        if job.is_null() {
            return Err(PublicationAdapterError::Unavailable);
        }
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if unsafe {
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        } == 0
            || unsafe {
                AssignProcessToJobObject(
                    job,
                    child.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE,
                )
            } == 0
        {
            unsafe { windows_sys::Win32::Foundation::CloseHandle(job) };
            return Err(PublicationAdapterError::Unavailable);
        }
        Ok(Self { job })
    }

    fn terminate(&self) {
        unsafe {
            windows_sys::Win32::System::JobObjects::TerminateJobObject(self.job, 1);
        }
    }
}

#[cfg(windows)]
impl Drop for WindowsPublicationContainment {
    fn drop(&mut self) {
        unsafe { windows_sys::Win32::Foundation::CloseHandle(self.job) };
    }
}

fn create_private_directory(path: &Path) -> Result<(), PublicationAdapterError> {
    if !path.exists() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = fs::DirBuilder::new();
            builder.mode(0o700);
            builder
                .create(path)
                .map_err(|_| PublicationAdapterError::Unavailable)?;
        }
        #[cfg(not(unix))]
        fs::create_dir(path).map_err(|_| PublicationAdapterError::Unavailable)?;
    }
    reject_link(path).map_err(|_| PublicationAdapterError::Unavailable)?;
    #[cfg(unix)]
    validate_private_path(path, true).map_err(|_| PublicationAdapterError::Unavailable)?;
    #[cfg(not(unix))]
    if !fs::metadata(path)
        .map_err(|_| PublicationAdapterError::Unavailable)?
        .is_dir()
    {
        return Err(PublicationAdapterError::Unavailable);
    }
    Ok(())
}

fn create_private_file(path: &Path) -> Result<File, PublicationAdapterError> {
    #[cfg(unix)]
    let file = {
        use std::os::unix::fs::OpenOptionsExt;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(path)
    };
    #[cfg(not(unix))]
    let file = OpenOptions::new().write(true).create_new(true).open(path);
    let file = file.map_err(|_| PublicationAdapterError::Unavailable)?;
    #[cfg(unix)]
    validate_private_path(path, false).map_err(|_| PublicationAdapterError::Unavailable)?;
    #[cfg(not(unix))]
    if !file
        .metadata()
        .map_err(|_| PublicationAdapterError::Unavailable)?
        .is_file()
    {
        return Err(PublicationAdapterError::Unavailable);
    }
    Ok(file)
}

fn remove_proof_work(root: &Path, proof_id: Uuid) -> Result<(), PublicationAdapterError> {
    let path = root.join(proof_id.to_string());
    if path.parent() == Some(root)
        && path.file_name().and_then(|value| value.to_str()) == Some(proof_id.to_string().as_str())
        && fs::symlink_metadata(&path).ok().is_some_and(|metadata| {
            metadata.file_type().is_dir() && !metadata.file_type().is_symlink()
        })
    {
        fs::remove_dir_all(&path).map_err(|_| PublicationAdapterError::AmbiguousEffect)?;
        if fs::symlink_metadata(&path).is_ok() {
            return Err(PublicationAdapterError::AmbiguousEffect);
        }
        Ok(())
    } else {
        Err(PublicationAdapterError::AmbiguousEffect)
    }
}

fn load_executable_identity(
    path: &Path,
    expected: &str,
) -> Result<ExecutableIdentity, GithubPublicationConfigError> {
    if !is_lowercase_sha256(expected) {
        return Err(GithubPublicationConfigError::Invalid);
    }
    reject_link(path)?;
    #[cfg(windows)]
    validate_windows_executable_acl(path)?;
    let metadata = fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > 512 * 1024 * 1024 {
        return Err(GithubPublicationConfigError::Invalid);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.uid() != unsafe { libc::geteuid() }
            || metadata.permissions().mode() & 0o022 != 0
            || metadata.permissions().mode() & 0o111 == 0
            || metadata.nlink() != 1
        {
            return Err(GithubPublicationConfigError::Invalid);
        }
    }
    let mut file = OpenOptions::new().read(true).open(path)?;
    let mut bytes = Vec::with_capacity(
        usize::try_from(metadata.len()).map_err(|_| GithubPublicationConfigError::Invalid)?,
    );
    file.read_to_end(&mut bytes)?;
    let digest: [u8; 32] = Sha256::digest(&bytes).into();
    if hex(&digest) != expected {
        return Err(GithubPublicationConfigError::Invalid);
    }
    Ok(ExecutableIdentity {
        sha256: digest,
        length: metadata.len(),
        modified: metadata.modified()?,
        #[cfg(unix)]
        device: {
            use std::os::unix::fs::MetadataExt;
            metadata.dev()
        },
        #[cfg(unix)]
        inode: {
            use std::os::unix::fs::MetadataExt;
            metadata.ino()
        },
        #[cfg(windows)]
        volume_serial: windows_file_identity(&file)
            .map_err(|_| GithubPublicationConfigError::Invalid)?
            .0,
        #[cfg(windows)]
        file_index: windows_file_identity(&file)
            .map_err(|_| GithubPublicationConfigError::Invalid)?
            .1,
    })
}

fn open_verified_executable(
    path: &Path,
    identity: &ExecutableIdentity,
) -> Result<File, PublicationAdapterError> {
    reject_link(path).map_err(|_| PublicationAdapterError::Unavailable)?;
    #[cfg(windows)]
    validate_windows_executable_acl(path).map_err(|_| PublicationAdapterError::Unavailable)?;
    #[cfg(windows)]
    let mut file = {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
        };
        OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .map_err(|_| PublicationAdapterError::Unavailable)?
    };
    #[cfg(not(windows))]
    let mut file = OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(|_| PublicationAdapterError::Unavailable)?;
    let metadata = file
        .metadata()
        .map_err(|_| PublicationAdapterError::Unavailable)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.dev() != identity.device || metadata.ino() != identity.inode {
            return Err(PublicationAdapterError::Unavailable);
        }
    }
    #[cfg(windows)]
    if windows_file_identity(&file).map_err(|_| PublicationAdapterError::Unavailable)?
        != (identity.volume_serial, identity.file_index)
    {
        return Err(PublicationAdapterError::Unavailable);
    }
    if metadata.len() != identity.length || metadata.modified().ok() != Some(identity.modified) {
        return Err(PublicationAdapterError::Unavailable);
    }
    let mut bytes = Vec::with_capacity(
        usize::try_from(identity.length).map_err(|_| PublicationAdapterError::Unavailable)?,
    );
    file.read_to_end(&mut bytes)
        .map_err(|_| PublicationAdapterError::Unavailable)?;
    if <[u8; 32]>::from(Sha256::digest(&bytes)) != identity.sha256 {
        return Err(PublicationAdapterError::Unavailable);
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| PublicationAdapterError::Unavailable)?;
    Ok(file)
}

#[cfg(windows)]
fn windows_file_identity(file: &File) -> Result<(u32, u64), std::io::Error> {
    use std::mem::MaybeUninit;
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };
    let mut information = MaybeUninit::<BY_HANDLE_FILE_INFORMATION>::zeroed();
    if unsafe {
        GetFileInformationByHandle(
            file.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE,
            information.as_mut_ptr(),
        )
    } == 0
    {
        return Err(std::io::Error::last_os_error());
    }
    let information = unsafe { information.assume_init() };
    Ok((
        information.dwVolumeSerialNumber,
        (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow),
    ))
}

#[cfg(windows)]
fn open_locked_windows_executable(
    path: &Path,
    expected_sha256: &str,
    expected_length: u64,
    expected_volume: u32,
    expected_index: u64,
) -> Result<File, PublicationAdapterError> {
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_REPARSE_POINT, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
    };
    validate_windows_executable_acl(path).map_err(|_| PublicationAdapterError::Unavailable)?;
    let mut file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|_| PublicationAdapterError::Unavailable)?;
    let metadata = file
        .metadata()
        .map_err(|_| PublicationAdapterError::Unavailable)?;
    if !metadata.is_file()
        || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || metadata.len() != expected_length
        || windows_file_identity(&file).map_err(|_| PublicationAdapterError::Unavailable)?
            != (expected_volume, expected_index)
    {
        return Err(PublicationAdapterError::Unavailable);
    }
    let mut bytes = Vec::with_capacity(
        usize::try_from(expected_length).map_err(|_| PublicationAdapterError::Unavailable)?,
    );
    file.read_to_end(&mut bytes)
        .map_err(|_| PublicationAdapterError::Unavailable)?;
    if hex(&Sha256::digest(&bytes)) != expected_sha256 {
        return Err(PublicationAdapterError::Unavailable);
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|_| PublicationAdapterError::Unavailable)?;
    Ok(file)
}

#[cfg(windows)]
fn validate_windows_executable_acl(path: &Path) -> Result<(), GithubPublicationConfigError> {
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::{addr_of, null_mut};
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree, GENERIC_ALL, GENERIC_WRITE};
    use windows_sys::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
    use windows_sys::Win32::Security::{
        AclSizeInformation, CreateWellKnownSid, EqualSid, GetAce, GetAclInformation,
        GetTokenInformation, TokenUser, WinBuiltinAdministratorsSid, WinLocalSystemSid,
        ACCESS_ALLOWED_ACE, ACL_SIZE_INFORMATION, DACL_SECURITY_INFORMATION,
        OWNER_SECURITY_INFORMATION, PSID, SECURITY_MAX_SID_SIZE, TOKEN_QUERY, TOKEN_USER,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        DELETE, FILE_APPEND_DATA, FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, FILE_WRITE_EA, WRITE_DAC,
        WRITE_OWNER,
    };
    use windows_sys::Win32::System::SystemServices::ACCESS_ALLOWED_ACE_TYPE;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
    wide.push(0);
    let mut owner: PSID = null_mut();
    let mut dacl = null_mut();
    let mut descriptor = null_mut();
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut dacl,
            null_mut(),
            &mut descriptor,
        )
    };
    if status != 0 || owner.is_null() || dacl.is_null() || descriptor.is_null() {
        if !descriptor.is_null() {
            unsafe { LocalFree(descriptor) };
        }
        return Err(GithubPublicationConfigError::Invalid);
    }
    let result = (|| {
        let mut token = null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err(GithubPublicationConfigError::Invalid);
        }
        let token_result = (|| {
            let mut token_bytes = 0_u32;
            unsafe { GetTokenInformation(token, TokenUser, null_mut(), 0, &mut token_bytes) };
            let mut token_buffer =
                vec![0_usize; (token_bytes as usize).div_ceil(size_of::<usize>())];
            if token_buffer.is_empty()
                || unsafe {
                    GetTokenInformation(
                        token,
                        TokenUser,
                        token_buffer.as_mut_ptr().cast(),
                        token_bytes,
                        &mut token_bytes,
                    )
                } == 0
            {
                return Err(GithubPublicationConfigError::Invalid);
            }
            let user = unsafe { &*(token_buffer.as_ptr().cast::<TOKEN_USER>()) };
            let mut system_sid = [0_u8; SECURITY_MAX_SID_SIZE as usize];
            let mut system_bytes = SECURITY_MAX_SID_SIZE;
            let mut administrators_sid = [0_u8; SECURITY_MAX_SID_SIZE as usize];
            let mut administrators_bytes = SECURITY_MAX_SID_SIZE;
            if unsafe {
                CreateWellKnownSid(
                    WinLocalSystemSid,
                    null_mut(),
                    system_sid.as_mut_ptr().cast(),
                    &mut system_bytes,
                )
            } == 0
                || unsafe {
                    CreateWellKnownSid(
                        WinBuiltinAdministratorsSid,
                        null_mut(),
                        administrators_sid.as_mut_ptr().cast(),
                        &mut administrators_bytes,
                    )
                } == 0
            {
                return Err(GithubPublicationConfigError::Invalid);
            }
            let mut acl_info: ACL_SIZE_INFORMATION = unsafe { zeroed() };
            if unsafe {
                GetAclInformation(
                    dacl,
                    (&mut acl_info as *mut ACL_SIZE_INFORMATION).cast(),
                    size_of::<ACL_SIZE_INFORMATION>() as u32,
                    AclSizeInformation,
                )
            } == 0
            {
                return Err(GithubPublicationConfigError::Invalid);
            }
            let write_mask = FILE_WRITE_DATA
                | FILE_APPEND_DATA
                | FILE_WRITE_EA
                | FILE_WRITE_ATTRIBUTES
                | DELETE
                | WRITE_DAC
                | WRITE_OWNER
                | GENERIC_WRITE
                | GENERIC_ALL;
            for index in 0..acl_info.AceCount {
                let mut raw: *mut c_void = null_mut();
                if unsafe { GetAce(dacl, index, &mut raw) } == 0 || raw.is_null() {
                    return Err(GithubPublicationConfigError::Invalid);
                }
                let ace = unsafe { &*(raw.cast::<ACCESS_ALLOWED_ACE>()) };
                if u32::from(ace.Header.AceType) != ACCESS_ALLOWED_ACE_TYPE
                    || ace.Mask & write_mask == 0
                {
                    continue;
                }
                let sid = addr_of!(ace.SidStart) as PSID;
                if unsafe { EqualSid(sid, owner) } == 0
                    && unsafe { EqualSid(sid, user.User.Sid) } == 0
                    && unsafe { EqualSid(sid, system_sid.as_mut_ptr().cast()) } == 0
                    && unsafe { EqualSid(sid, administrators_sid.as_mut_ptr().cast()) } == 0
                {
                    return Err(GithubPublicationConfigError::Invalid);
                }
            }
            Ok(())
        })();
        unsafe { CloseHandle(token) };
        token_result
    })();
    unsafe { LocalFree(descriptor) };
    result
}

fn reject_link(path: &Path) -> Result<(), GithubPublicationConfigError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(GithubPublicationConfigError::Invalid);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes()
            & windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT
            != 0
        {
            return Err(GithubPublicationConfigError::Invalid);
        }
    }
    Ok(())
}

fn validate_private_path(path: &Path, directory: bool) -> Result<(), GithubPublicationConfigError> {
    let metadata = fs::metadata(path)?;
    if metadata.is_dir() != directory || metadata.is_file() == directory {
        return Err(GithubPublicationConfigError::Invalid);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.uid() != unsafe { libc::geteuid() }
            || metadata.permissions().mode() & 0o077 != 0
            || (!directory && metadata.nlink() != 1)
        {
            return Err(GithubPublicationConfigError::Invalid);
        }
        if !directory
            && path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    matches!(name, "gh" | "git") && metadata.permissions().mode() & 0o111 == 0
                })
        {
            return Err(GithubPublicationConfigError::Invalid);
        }
    }
    #[cfg(windows)]
    validate_private_windows_acl(path)?;
    Ok(())
}

#[cfg(windows)]
fn validate_private_windows_acl(path: &Path) -> Result<(), GithubPublicationConfigError> {
    use std::ffi::c_void;
    use std::mem::{size_of, zeroed};
    use std::os::windows::ffi::OsStrExt;
    use std::ptr::{addr_of, null_mut};
    use windows_sys::Win32::Foundation::{CloseHandle, LocalFree};
    use windows_sys::Win32::Security::Authorization::{GetNamedSecurityInfoW, SE_FILE_OBJECT};
    use windows_sys::Win32::Security::{
        AclSizeInformation, CreateWellKnownSid, EqualSid, GetAce, GetAclInformation,
        GetSecurityDescriptorControl, GetTokenInformation, TokenUser, WinLocalSystemSid,
        ACCESS_ALLOWED_ACE, ACL_SIZE_INFORMATION, DACL_SECURITY_INFORMATION, INHERITED_ACE,
        OWNER_SECURITY_INFORMATION, PSID, SECURITY_MAX_SID_SIZE, SE_DACL_PROTECTED, TOKEN_QUERY,
        TOKEN_USER,
    };
    use windows_sys::Win32::Storage::FileSystem::FILE_ALL_ACCESS;
    use windows_sys::Win32::System::SystemServices::ACCESS_ALLOWED_ACE_TYPE;
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    let mut wide = path.as_os_str().encode_wide().collect::<Vec<_>>();
    wide.push(0);
    let mut owner: PSID = null_mut();
    let mut dacl = null_mut();
    let mut descriptor = null_mut();
    let status = unsafe {
        GetNamedSecurityInfoW(
            wide.as_ptr(),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut dacl,
            null_mut(),
            &mut descriptor,
        )
    };
    if status != 0 || owner.is_null() || dacl.is_null() || descriptor.is_null() {
        if !descriptor.is_null() {
            unsafe { LocalFree(descriptor) };
        }
        return Err(GithubPublicationConfigError::Invalid);
    }
    let result = (|| {
        let mut token = null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } == 0 {
            return Err(GithubPublicationConfigError::Invalid);
        }
        let token_result = (|| {
            let mut token_bytes = 0;
            unsafe { GetTokenInformation(token, TokenUser, null_mut(), 0, &mut token_bytes) };
            let mut buffer = vec![0_usize; (token_bytes as usize).div_ceil(size_of::<usize>())];
            if buffer.is_empty()
                || unsafe {
                    GetTokenInformation(
                        token,
                        TokenUser,
                        buffer.as_mut_ptr().cast(),
                        token_bytes,
                        &mut token_bytes,
                    )
                } == 0
            {
                return Err(GithubPublicationConfigError::Invalid);
            }
            let user = unsafe { &*(buffer.as_ptr().cast::<TOKEN_USER>()) };
            if unsafe { EqualSid(owner, user.User.Sid) } == 0 {
                return Err(GithubPublicationConfigError::Invalid);
            }
            let mut control = 0_u16;
            let mut revision = 0_u32;
            if unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } == 0
                || control & SE_DACL_PROTECTED == 0
            {
                return Err(GithubPublicationConfigError::Invalid);
            }
            let mut acl_info: ACL_SIZE_INFORMATION = unsafe { zeroed() };
            if unsafe {
                GetAclInformation(
                    dacl,
                    (&mut acl_info as *mut ACL_SIZE_INFORMATION).cast(),
                    size_of::<ACL_SIZE_INFORMATION>() as u32,
                    AclSizeInformation,
                )
            } == 0
                || acl_info.AceCount != 2
            {
                return Err(GithubPublicationConfigError::Invalid);
            }
            let mut system_sid = [0_u8; SECURITY_MAX_SID_SIZE as usize];
            let mut system_sid_bytes = SECURITY_MAX_SID_SIZE;
            if unsafe {
                CreateWellKnownSid(
                    WinLocalSystemSid,
                    null_mut(),
                    system_sid.as_mut_ptr().cast(),
                    &mut system_sid_bytes,
                )
            } == 0
            {
                return Err(GithubPublicationConfigError::Invalid);
            }
            let mut saw_user = false;
            let mut saw_system = false;
            for index in 0..acl_info.AceCount {
                let mut raw: *mut c_void = null_mut();
                if unsafe { GetAce(dacl, index, &mut raw) } == 0 || raw.is_null() {
                    return Err(GithubPublicationConfigError::Invalid);
                }
                let ace = unsafe { &*(raw.cast::<ACCESS_ALLOWED_ACE>()) };
                if u32::from(ace.Header.AceType) != ACCESS_ALLOWED_ACE_TYPE
                    || u32::from(ace.Header.AceFlags) & INHERITED_ACE != 0
                    || ace.Mask & FILE_ALL_ACCESS != FILE_ALL_ACCESS
                {
                    return Err(GithubPublicationConfigError::Invalid);
                }
                let sid = addr_of!(ace.SidStart) as PSID;
                if unsafe { EqualSid(sid, user.User.Sid) } != 0 && !saw_user {
                    saw_user = true;
                } else if unsafe { EqualSid(sid, system_sid.as_mut_ptr().cast()) } != 0
                    && !saw_system
                {
                    saw_system = true;
                } else {
                    return Err(GithubPublicationConfigError::Invalid);
                }
            }
            if !saw_user || !saw_system {
                return Err(GithubPublicationConfigError::Invalid);
            }
            Ok(())
        })();
        unsafe { CloseHandle(token) };
        token_result
    })();
    unsafe { LocalFree(descriptor) };
    result
}

fn contains_plaintext_token(bytes: &[u8]) -> bool {
    let lowercase = String::from_utf8_lossy(bytes).to_ascii_lowercase();
    lowercase.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("oauth_token:") || trimmed.starts_with("token:")
    })
}

fn is_lowercase_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn is_git_commit(value: &str) -> bool {
    value.len() == 40
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut result, "{byte:02x}").expect("writing to String cannot fail");
    }
    result
}
