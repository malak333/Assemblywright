//! Developer-only GitHub publication. Credentialed commands run only in a runner-owned checkout.
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    env, fs,
    path::{Component, Path, PathBuf},
    process::Stdio,
    sync::atomic::{AtomicU8, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{io::AsyncReadExt, process::Command};

#[path = "developer_publication_process.rs"]
mod process_tree;

const OUTPUT_LIMIT: usize = 1024 * 1024;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(180);
const CHECKS_TIMEOUT: Duration = Duration::from_secs(900);
const EVENT_LIMIT: usize = 80;
const REPOSITORY_PAGE_SIZE: u32 = 50;
const SETUP_OUTPUT_LIMIT: usize = 64 * 1024;
const DEVICE_URL: &str = "https://github.com/login/device";

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct ProjectBinding {
    pub project: String,
    pub repository_url: String,
    pub repository_slug: String,
    pub base_branch: String,
    pub required_checks: Vec<RequiredCheck>,
    pub strict_required_checks: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(deny_unknown_fields)]
pub(super) struct RequiredCheck {
    pub context: String,
    pub integration_id: Option<u64>,
}

#[derive(Clone, Debug)]
pub(super) struct CandidateFile {
    pub path: String,
    pub before_sha256: Option<String>,
    pub content_sha256: String,
    pub content: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum GithubAccountObservation {
    SignedIn { login: String },
    SignedOut { message: &'static str },
    Unavailable { message: &'static str },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GithubRepositoryObservation {
    pub repository_id: u64,
    pub name_with_owner: String,
    pub url: String,
    pub visibility: String,
    pub default_branch: String,
    pub can_push: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum GithubRepositoryLookup {
    Present(GithubRepositoryObservation),
    Absent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GithubRepositoryPage {
    pub login: String,
    pub repositories: Vec<GithubRepositoryObservation>,
    pub has_more: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GithubDeviceChallenge {
    pub user_code: String,
    pub verification_url: &'static str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GithubSignInOutcome {
    pub account: GithubAccountObservation,
    pub command_succeeded: bool,
    pub cancelled: bool,
    pub challenge_seen: bool,
    pub credentials_consistent: bool,
}

#[derive(Clone, Debug)]
pub(super) struct PublicationInput {
    pub feature_id: String,
    pub title: String,
    pub body: String,
    pub binding: ProjectBinding,
    pub files: Vec<CandidateFile>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct PublicationEvent {
    pub sequence: u32,
    pub kind: String,
    pub stage: String,
    pub evidence_sha256: Option<String>,
    pub summary: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct PublicationRecord {
    pub schema_version: u8,
    pub status: String,
    pub stage: String,
    pub message: String,
    pub repository_url: String,
    pub repository_slug: String,
    pub base_branch: String,
    pub feature_branch: String,
    pub started_unix_seconds: u64,
    pub author_name: Option<String>,
    pub author_email: Option<String>,
    pub base_sha: Option<String>,
    pub candidate_tree_sha: Option<String>,
    pub commit_sha: Option<String>,
    pub pr_number: Option<u64>,
    pub pr_url: Option<String>,
    pub merged_sha: Option<String>,
    pub required_checks: Vec<RequiredCheck>,
    pub strict_required_checks: bool,
    #[serde(default)]
    pub events: Vec<PublicationEvent>,
}

impl PublicationRecord {
    pub(super) fn pending(input: &PublicationInput) -> Result<Self> {
        validate_uuid_text(&input.feature_id)?;
        validate_binding(&input.binding)?;
        validate_candidate_files(&input.files)?;
        validate_pull_request_text(&input.title, &input.body)?;
        Ok(Self {
            schema_version: 1,
            status: "pending".into(),
            stage: "prepare_candidate".into(),
            message: "Approved files are ready for GitHub publication".into(),
            repository_url: input.binding.repository_url.clone(),
            repository_slug: input.binding.repository_slug.clone(),
            base_branch: input.binding.base_branch.clone(),
            feature_branch: format!("codex/feature-{}", input.feature_id),
            started_unix_seconds: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .context("System clock is before the Unix epoch")?
                .as_secs(),
            author_name: None,
            author_email: None,
            base_sha: None,
            candidate_tree_sha: None,
            commit_sha: None,
            pr_number: None,
            pr_url: None,
            merged_sha: None,
            required_checks: input.binding.required_checks.clone(),
            strict_required_checks: input.binding.strict_required_checks,
            events: Vec::new(),
        })
    }

    pub(super) fn validate(&self) -> Result<()> {
        if self.schema_version != 1 {
            bail!("Persisted Developer publication version is unsupported");
        }
        if !matches!(
            self.status.as_str(),
            "pending" | "running" | "attention" | "succeeded"
        ) {
            bail!("Persisted Developer publication status is invalid");
        }
        validate_stage(&self.stage)?;
        let binding = ProjectBinding {
            project: "persisted".into(),
            repository_url: self.repository_url.clone(),
            repository_slug: self.repository_slug.clone(),
            base_branch: self.base_branch.clone(),
            required_checks: self.required_checks.clone(),
            strict_required_checks: self.strict_required_checks,
        };
        validate_binding(&binding)?;
        if !self.feature_branch.starts_with("codex/feature-")
            || self.feature_branch.len() > 96
            || !self
                .feature_branch
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '/' | '-'))
        {
            bail!("Persisted Developer publication branch is invalid");
        }
        for digest in [
            self.base_sha.as_deref(),
            self.candidate_tree_sha.as_deref(),
            self.commit_sha.as_deref(),
            self.merged_sha.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            validate_git_oid(digest)?;
        }
        if self.events.len() > EVENT_LIMIT {
            bail!("Persisted Developer publication evidence exceeds its bound");
        }
        validate_required_checks(&self.required_checks)?;
        if !self.strict_required_checks {
            bail!("Persisted Developer publication does not require the latest base before checks");
        }
        for (index, event) in self.events.iter().enumerate() {
            if event.sequence as usize != index + 1 {
                bail!("Persisted publication evidence sequence is invalid");
            }
            validate_stage(&event.stage)?;
            if !matches!(event.kind.as_str(), "intent" | "receipt") {
                bail!("Persisted publication evidence kind is invalid");
            }
            if event.summary.is_empty() || event.summary.len() > 1000 {
                bail!("Persisted publication evidence summary is invalid");
            }
        }
        for stage in ["push_branch", "open_pull_request", "merge_pull_request"] {
            for (index, _event) in self
                .events
                .iter()
                .enumerate()
                .filter(|(_, event)| event.stage == stage && event.kind == "receipt")
            {
                if !self.events[..index]
                    .iter()
                    .any(|prior| prior.stage == stage && prior.kind == "intent")
                {
                    bail!("Persisted publication receipt has no preceding intent");
                }
            }
        }
        if self.status == "succeeded" {
            if self.stage != "complete"
                || self.base_sha.is_none()
                || self.candidate_tree_sha.is_none()
                || self.commit_sha.is_none()
                || self.pr_number.is_none()
                || self.pr_url.is_none()
                || self.merged_sha.is_none()
                || !self.events.last().is_some_and(|event| {
                    event.kind == "receipt"
                        && event.stage == "complete"
                        && event.evidence_sha256.as_deref() == self.merged_sha.as_deref()
                })
            {
                bail!("Completed publication evidence is incomplete");
            }
        } else if self.stage == "complete" {
            bail!("Incomplete publication cannot claim the complete stage");
        }
        Ok(())
    }

    fn event(
        &mut self,
        kind: &str,
        stage: &str,
        evidence: Option<&str>,
        summary: &str,
    ) -> Result<()> {
        validate_stage(stage)?;
        if !matches!(kind, "intent" | "receipt") {
            bail!("Invalid publication evidence kind");
        }
        let sequence: u32 = self
            .events
            .len()
            .checked_add(1)
            .context("Publication evidence sequence overflow")?
            .try_into()?;
        if self.events.len() >= EVENT_LIMIT {
            bail!("Publication evidence limit reached");
        }
        self.stage = stage.into();
        self.status = "running".into();
        self.message = summary.chars().take(1000).collect();
        self.events.push(PublicationEvent {
            sequence,
            kind: kind.into(),
            stage: stage.into(),
            evidence_sha256: evidence.map(str::to_owned),
            summary: summary.chars().take(1000).collect(),
        });
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub(super) struct Runtime {
    git: PathBuf,
    gh: PathBuf,
    checkout_root: PathBuf,
    empty_hooks: PathBuf,
}

impl Runtime {
    pub(super) fn new(
        data_dir: &Path,
        git_override: Option<PathBuf>,
        gh_override: Option<PathBuf>,
    ) -> Result<Self> {
        let git = resolve_executable(git_override, "git")?;
        let gh = resolve_executable(gh_override, "gh")?;
        let checkout_root = data_dir.join("developer-publication-checkouts");
        let empty_hooks = data_dir.join("developer-publication-empty-hooks");
        fs::create_dir_all(&checkout_root)?;
        fs::create_dir_all(&empty_hooks)?;
        let checkout_root = git_process_path(&fs::canonicalize(checkout_root)?)?;
        let empty_hooks = git_process_path(&fs::canonicalize(empty_hooks)?)?;
        Ok(Self {
            git,
            gh,
            checkout_root,
            empty_hooks,
        })
    }

    pub(super) async fn inspect_account(
        &self,
        cancellation: &AtomicU8,
    ) -> Result<GithubAccountObservation> {
        self.inspect_account_with_environment(cancellation, false)
            .await
    }

    pub(super) async fn inspect_stored_account(
        &self,
        cancellation: &AtomicU8,
    ) -> Result<GithubAccountObservation> {
        self.inspect_account_with_environment(cancellation, true)
            .await
    }

    pub(super) async fn inspect_sign_in_credentials(
        &self,
        cancellation: &AtomicU8,
    ) -> Result<(GithubAccountObservation, bool)> {
        let stored = self.inspect_stored_account(cancellation).await?;
        let effective = self.inspect_account(cancellation).await?;
        let credentials_consistent = account_observations_match(&stored, &effective);
        Ok((effective, credentials_consistent))
    }

    pub(super) async fn list_accessible_repositories(
        &self,
        page: u32,
        cancellation: &AtomicU8,
    ) -> Result<GithubRepositoryPage> {
        if !(1..=100).contains(&page) {
            bail!("GitHub repository page must be between 1 and 100");
        }
        let login = match self.inspect_account(cancellation).await? {
            GithubAccountObservation::SignedIn { login } => login,
            GithubAccountObservation::SignedOut { .. } => {
                bail!("Sign in to GitHub before listing repositories")
            }
            GithubAccountObservation::Unavailable { .. } => {
                bail!("GitHub account verification is unavailable")
            }
        };
        let mut repositories = self.repository_page(page, cancellation).await?;
        self.enrich_repository_branches(&mut repositories, cancellation)
            .await?;
        let next_page = page
            .checked_add(1)
            .context("GitHub repository page overflow")?;
        let has_more = if page == 100 {
            false
        } else {
            !self
                .repository_page(next_page, cancellation)
                .await?
                .is_empty()
        };
        Ok(GithubRepositoryPage {
            login,
            repositories,
            has_more,
        })
    }

    pub(super) async fn observe_repository(
        &self,
        name_with_owner: &str,
        cancellation: &AtomicU8,
    ) -> Result<GithubRepositoryLookup> {
        let slug = canonical_repository(&format!("https://github.com/{name_with_owner}"))?;
        if slug != name_with_owner {
            bail!("GitHub repository identity must use canonical case");
        }
        let endpoint = format!("repos/{slug}");
        let output = self
            .setup_command(&["api", &endpoint], cancellation, &[0, 1], COMMAND_TIMEOUT)
            .await
            .context("GitHub repository observation is unavailable")?;
        if output.status == 1 {
            if github_api_error_status(&output.stdout) == Some(404) {
                return Ok(GithubRepositoryLookup::Absent);
            }
            bail!("GitHub repository observation could not be confirmed");
        }
        let mut repository = vec![parse_repository(&output.stdout)?];
        ensure_repository_identity(&repository[0], &slug)?;
        self.enrich_repository_branches(&mut repository, cancellation)
            .await?;
        Ok(GithubRepositoryLookup::Present(repository.remove(0)))
    }

    pub(super) async fn create_repository(
        &self,
        expected_login: &str,
        name: &str,
        visibility: &str,
        cancellation: &AtomicU8,
    ) -> Result<bool> {
        validate_account_login(expected_login)?;
        validate_new_repository_name(name)?;
        if !matches!(visibility, "public" | "private") {
            bail!("GitHub repository visibility must be public or private");
        }
        let observed_login = match self.inspect_account(cancellation).await? {
            GithubAccountObservation::SignedIn { login } => login,
            GithubAccountObservation::SignedOut { .. } => {
                bail!("GitHub login expired before repository creation")
            }
            GithubAccountObservation::Unavailable { .. } => {
                bail!("GitHub account could not be verified before repository creation")
            }
        };
        if !observed_login.eq_ignore_ascii_case(expected_login) {
            bail!("GitHub account changed after repository creation confirmation");
        }
        let name_with_owner = format!("{observed_login}/{name}");
        let visibility_flag = if visibility == "private" {
            "--private"
        } else {
            "--public"
        };
        let output = self
            .setup_command(
                &[
                    "repo",
                    "create",
                    &name_with_owner,
                    visibility_flag,
                    "--add-readme",
                ],
                cancellation,
                &[0, 1],
                COMMAND_TIMEOUT,
            )
            .await
            .context("GitHub repository creation result is unavailable")?;
        Ok(output.status == 0)
    }

    pub(super) async fn sign_in(
        &self,
        cancellation: &AtomicU8,
        mut challenge: impl FnMut(GithubDeviceChallenge) -> Result<()>,
    ) -> Result<GithubSignInOutcome> {
        if cancellation.load(Ordering::SeqCst) != 0 {
            return self.sign_in_postcheck(false, true, false).await;
        }
        let browser = match github_browser_sink_command() {
            Ok(browser) => browser,
            Err(_) => return self.sign_in_postcheck(false, false, false).await,
        };
        let mut command = Command::new(&self.gh);
        command
            .args([
                "auth",
                "login",
                "--hostname",
                "github.com",
                "--git-protocol",
                "https",
                "--web",
                "--skip-ssh-key",
            ])
            .current_dir(&self.checkout_root)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .env("GH_HOST", "github.com")
            .env("GH_BROWSER", browser)
            .env("GH_PROMPT_DISABLED", "1")
            .env("GH_PAGER", "cat")
            .env("PAGER", "cat")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env_remove("GH_REPO")
            .env_remove("GH_TOKEN")
            .env_remove("GITHUB_TOKEN");
        for key in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_SSH",
            "GIT_SSH_COMMAND",
            "GIT_PROXY_COMMAND",
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_PARAMETERS",
        ] {
            command.env_remove(key);
        }
        process_tree::prepare_process_tree(&mut command);
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(_) => return self.sign_in_postcheck(false, false, false).await,
        };
        let mut tree = match process_tree::attach_process_tree(&mut child).await {
            Ok(tree) => tree,
            Err(_) => return self.sign_in_postcheck(false, false, false).await,
        };
        let mut stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                let _ = tree
                    .terminate_and_wait(&mut child, Duration::from_secs(10))
                    .await;
                return self.sign_in_postcheck(false, false, false).await;
            }
        };
        let mut stderr = match child.stderr.take() {
            Some(stderr) => stderr,
            None => {
                let _ = tree
                    .terminate_and_wait(&mut child, Duration::from_secs(10))
                    .await;
                return self.sign_in_postcheck(false, false, false).await;
            }
        };
        let deadline = tokio::time::Instant::now() + COMMAND_TIMEOUT;
        let mut observed = Vec::new();
        let mut challenge_seen = false;
        let mut stdout_open = true;
        let mut stderr_open = true;
        let mut stdout_chunk = [0u8; 4096];
        let mut stderr_chunk = [0u8; 4096];
        let mut cancelled = false;
        let mut forced_failure = false;
        let status = loop {
            tokio::select! {
                result = child.wait() => match result {
                    Ok(status) => break Some(status),
                    Err(_) => {
                        forced_failure = true;
                        break None;
                    }
                },
                result = stdout.read(&mut stdout_chunk), if stdout_open => match result {
                    Ok(0) => stdout_open = false,
                    Ok(read) => {
                        if record_challenge(
                            &mut observed,
                            &stdout_chunk[..read],
                            &mut challenge_seen,
                            &mut challenge,
                        ).is_err() {
                            forced_failure = true;
                            break None;
                        }
                    }
                    Err(_) => {
                        forced_failure = true;
                        break None;
                    }
                },
                result = stderr.read(&mut stderr_chunk), if stderr_open => match result {
                    Ok(0) => stderr_open = false,
                    Ok(read) => {
                        if record_challenge(
                            &mut observed,
                            &stderr_chunk[..read],
                            &mut challenge_seen,
                            &mut challenge,
                        ).is_err() {
                            forced_failure = true;
                            break None;
                        }
                    }
                    Err(_) => {
                        forced_failure = true;
                        break None;
                    }
                },
                _ = tokio::time::sleep(Duration::from_millis(50)) => {
                    cancelled = cancellation.load(Ordering::SeqCst) != 0;
                    if cancelled || tokio::time::Instant::now() >= deadline {
                        forced_failure = true;
                        break None;
                    }
                }
            }
        };
        if forced_failure {
            if tree
                .terminate_and_wait(&mut child, Duration::from_secs(10))
                .await
                .is_err()
            {
                forced_failure = true;
            }
        } else if tree.confirm_stopped(Duration::from_secs(10)).await.is_err() {
            forced_failure = true;
            if tree
                .terminate_and_wait(&mut child, Duration::from_secs(10))
                .await
                .is_err()
            {
                forced_failure = true;
            }
        }
        let mut stdout_tail = Vec::new();
        let mut stderr_tail = Vec::new();
        let drain = tokio::time::timeout(Duration::from_secs(10), async {
            tokio::join!(
                stdout.read_to_end(&mut stdout_tail),
                stderr.read_to_end(&mut stderr_tail)
            )
        })
        .await;
        match drain {
            Ok((Ok(_), Ok(_))) => {
                if record_challenge(
                    &mut observed,
                    &stdout_tail,
                    &mut challenge_seen,
                    &mut challenge,
                )
                .and_then(|_| {
                    record_challenge(
                        &mut observed,
                        &stderr_tail,
                        &mut challenge_seen,
                        &mut challenge,
                    )
                })
                .is_err()
                {
                    forced_failure = true;
                }
            }
            _ => forced_failure = true,
        }
        let command_succeeded =
            status.as_ref().and_then(std::process::ExitStatus::code) == Some(0) && !forced_failure;
        if !forced_failure {
            tree.disarm();
        }
        self.sign_in_postcheck(command_succeeded, cancelled, challenge_seen)
            .await
    }

    async fn sign_in_postcheck(
        &self,
        command_succeeded: bool,
        cancelled: bool,
        challenge_seen: bool,
    ) -> Result<GithubSignInOutcome> {
        let postcheck = AtomicU8::new(0);
        let (effective, credentials_consistent) =
            self.inspect_sign_in_credentials(&postcheck).await?;
        Ok(GithubSignInOutcome {
            account: effective,
            command_succeeded,
            cancelled,
            challenge_seen,
            credentials_consistent,
        })
    }

    async fn inspect_account_with_environment(
        &self,
        cancellation: &AtomicU8,
        stored_credentials_only: bool,
    ) -> Result<GithubAccountObservation> {
        let auth = self
            .command_with_extra_env(
                &self.gh,
                &[
                    "auth",
                    "status",
                    "--active",
                    "--hostname",
                    "github.com",
                    "--json",
                    "hosts",
                ],
                None,
                cancellation,
                &[0, 1],
                COMMAND_TIMEOUT,
                false,
                &[],
                stored_credentials_only,
            )
            .await?;
        let auth_login = match parse_account_status(&auth.stdout) {
            Ok(Some(login)) => login,
            Ok(None) => {
                return Ok(GithubAccountObservation::SignedOut {
                    message: "GitHub CLI is signed out or its saved login expired",
                })
            }
            Err(_) => {
                return Ok(GithubAccountObservation::Unavailable {
                    message: "GitHub CLI account status could not be verified",
                })
            }
        };
        let live = self
            .command_with_extra_env(
                &self.gh,
                &["api", "user"],
                None,
                cancellation,
                &[0, 1],
                COMMAND_TIMEOUT,
                false,
                &[],
                stored_credentials_only,
            )
            .await?;
        if live.status != 0 {
            return Ok(if github_api_error_status(&live.stdout) == Some(401) {
                GithubAccountObservation::SignedOut {
                    message: "GitHub rejected the saved login; sign in again",
                }
            } else {
                GithubAccountObservation::Unavailable {
                    message: "GitHub account verification is unavailable",
                }
            });
        }
        let value: Value =
            serde_json::from_slice(&live.stdout).context("GitHub account response is invalid")?;
        let live_login = value
            .get("login")
            .and_then(Value::as_str)
            .context("GitHub account response has no login")?;
        validate_account_login(live_login)?;
        if !live_login.eq_ignore_ascii_case(&auth_login) {
            bail!("GitHub CLI account identity changed during verification");
        }
        Ok(GithubAccountObservation::SignedIn {
            login: live_login.into(),
        })
    }

    async fn repository_page(
        &self,
        page: u32,
        cancellation: &AtomicU8,
    ) -> Result<Vec<GithubRepositoryObservation>> {
        let endpoint = format!(
            "user/repos?affiliation=owner%2Ccollaborator%2Corganization_member&visibility=all&sort=full_name&direction=asc&per_page={REPOSITORY_PAGE_SIZE}&page={page}"
        );
        let output = self
            .setup_command(&["api", &endpoint], cancellation, &[0], COMMAND_TIMEOUT)
            .await
            .context("GitHub repository list is unavailable")?;
        parse_repository_page(&output.stdout)
    }

    async fn enrich_repository_branches(
        &self,
        repositories: &mut [GithubRepositoryObservation],
        cancellation: &AtomicU8,
    ) -> Result<()> {
        if repositories.is_empty() {
            return Ok(());
        }
        if repositories.len() > REPOSITORY_PAGE_SIZE as usize {
            bail!("GitHub repository branch query exceeds its bound");
        }
        let mut query = String::from("query {");
        for (index, repository) in repositories.iter().enumerate() {
            let (owner, name) = repository
                .name_with_owner
                .split_once('/')
                .context("GitHub repository identity is invalid")?;
            validate_account_login(owner)?;
            validate_new_repository_name(name)?;
            query.push_str(&format!(
                " r{index}:repository(owner:\"{owner}\",name:\"{name}\"){{nameWithOwner isEmpty defaultBranchRef{{name}}}}"
            ));
        }
        query.push_str(" }");
        if query.len() > 16 * 1024 {
            bail!("GitHub repository branch query exceeds its byte bound");
        }
        let query_argument = format!("query={query}");
        let output = self
            .setup_command(
                &["api", "graphql", "-f", &query_argument],
                cancellation,
                &[0],
                COMMAND_TIMEOUT,
            )
            .await
            .context("GitHub repository branch evidence is unavailable")?;
        let value: Value = serde_json::from_slice(&output.stdout)
            .context("GitHub repository branch evidence is invalid")?;
        apply_repository_branch_evidence(repositories, &value)
    }

    pub(super) async fn validate_connection(
        &self,
        project: &str,
        repository_url: &str,
        base_branch: &str,
        cancellation: &AtomicU8,
    ) -> Result<ProjectBinding> {
        let repository_slug = canonical_repository(repository_url)?;
        validate_project(project)?;
        validate_branch(base_branch)?;
        self.command(
            &self.gh,
            &["auth", "status", "--hostname", "github.com"],
            None,
            cancellation,
            &[0],
            COMMAND_TIMEOUT,
            false,
        )
        .await
        .context("GitHub CLI authentication is unavailable")?;
        let view = self
            .command(
                &self.gh,
                &["repo", "view", &repository_slug, "--json", "nameWithOwner"],
                None,
                cancellation,
                &[0],
                COMMAND_TIMEOUT,
                false,
            )
            .await
            .context("The existing GitHub repository is unavailable")?;
        let parsed: Value = serde_json::from_slice(&view.stdout)
            .context("GitHub repository response is invalid")?;
        let observed_slug = parsed
            .get("nameWithOwner")
            .and_then(Value::as_str)
            .context("GitHub repository identity is missing")?;
        if !observed_slug.eq_ignore_ascii_case(&repository_slug)
            || canonical_repository(&format!("https://github.com/{observed_slug}"))?
                != observed_slug
        {
            bail!("GitHub repository identity did not match the requested destination");
        }
        let repository_slug = observed_slug.to_owned();
        let canonical_url = format!("https://github.com/{repository_slug}.git");
        let reference = format!("refs/heads/{base_branch}");
        let remote = self
            .command(
                &self.git,
                &["ls-remote", "--exit-code", &canonical_url, &reference],
                None,
                cancellation,
                &[0],
                COMMAND_TIMEOUT,
                true,
            )
            .await
            .context("The selected GitHub base branch is unavailable")?;
        parse_ls_remote(&remote.stdout, &reference)?;
        let (required_checks, strict_required_checks) = self
            .current_required_checks(&repository_slug, base_branch, cancellation)
            .await?;
        Ok(ProjectBinding {
            project: project.into(),
            repository_url: canonical_url,
            repository_slug,
            base_branch: base_branch.into(),
            required_checks,
            strict_required_checks,
        })
    }

    pub(super) async fn publish(
        &self,
        input: &PublicationInput,
        record: &mut PublicationRecord,
        cancellation: &AtomicU8,
        mut persist: impl FnMut(&PublicationRecord) -> Result<()>,
    ) -> Result<()> {
        if record.status == "succeeded" {
            bail!("Completed publication cannot be replayed");
        }
        validate_candidate_files(&input.files)?;
        validate_pull_request_text(&input.title, &input.body)?;
        record.validate()?;
        if record.repository_url != input.binding.repository_url
            || record.repository_slug != input.binding.repository_slug
            || record.base_branch != input.binding.base_branch
            || record.feature_branch != format!("codex/feature-{}", input.feature_id)
            || record.required_checks != input.binding.required_checks
            || record.strict_required_checks != input.binding.strict_required_checks
        {
            bail!("Publication binding changed after approval");
        }
        if self
            .reconcile_if_merged(record, cancellation, &mut persist)
            .await?
        {
            return Ok(());
        }
        self.prepare_candidate(input, record, cancellation, &mut persist)
            .await?;
        self.push_branch(record, cancellation, &mut persist).await?;
        self.open_or_reconcile_pr(input, record, cancellation, &mut persist)
            .await?;
        self.wait_for_checks(record, cancellation, &mut persist)
            .await?;
        self.merge_and_verify(record, cancellation, &mut persist)
            .await?;
        Ok(())
    }

    async fn prepare_candidate(
        &self,
        input: &PublicationInput,
        record: &mut PublicationRecord,
        cancellation: &AtomicU8,
        persist: &mut impl FnMut(&PublicationRecord) -> Result<()>,
    ) -> Result<()> {
        check_cancelled(cancellation)?;
        record.event(
            "intent",
            "prepare_candidate",
            None,
            "Preparing an isolated checkout from the selected remote base",
        )?;
        persist(record)?;
        let checkout = self.checkout_root.join(&input.feature_id);
        if checkout.exists() {
            if fs::symlink_metadata(&checkout)?.file_type().is_symlink()
                || !checkout.is_dir()
                || !checkout.join(".git").is_dir()
            {
                bail!("Publication checkout path is occupied by an unrecognized entry");
            }
        } else {
            let checkout_text = checkout.to_string_lossy().into_owned();
            self.command(
                &self.git,
                &[
                    "clone",
                    "--no-checkout",
                    "--single-branch",
                    "--branch",
                    &record.base_branch,
                    "--",
                    &record.repository_url,
                    &checkout_text,
                ],
                None,
                cancellation,
                &[0],
                COMMAND_TIMEOUT,
                true,
            )
            .await
            .context("Could not create the private publication checkout")?;
        }
        self.command(
            &self.git,
            &[
                "fetch",
                "--no-tags",
                &record.repository_url,
                &format!(
                    "+refs/heads/{}:refs/remotes/origin/{}",
                    record.base_branch, record.base_branch
                ),
            ],
            Some(&checkout),
            cancellation,
            &[0],
            COMMAND_TIMEOUT,
            true,
        )
        .await
        .context("Could not refresh the selected remote base")?;
        self.command(
            &self.git,
            &[
                "checkout",
                "--detach",
                &format!("origin/{}", record.base_branch),
            ],
            Some(&checkout),
            cancellation,
            &[0],
            COMMAND_TIMEOUT,
            true,
        )
        .await?;
        let base = self
            .git_text(&["rev-parse", "HEAD"], &checkout, cancellation)
            .await?;
        validate_git_oid(&base)?;
        if let Some(bound) = record.base_sha.as_deref() {
            if bound != base {
                bail!("Remote base changed after publication preparation; reconciliation needs attention");
            }
        } else {
            record.base_sha = Some(base.clone());
        }
        for file in &input.files {
            let target = checked_candidate_path(&checkout, &file.path)?;
            let current = fs::read(&target).ok();
            match (&file.before_sha256, current.as_deref()) {
                (None, None) => {}
                (None, Some(_)) => bail!(
                    "Reviewed new file {} already exists in the remote base",
                    file.path
                ),
                (Some(expected), Some(bytes)) if sha256(bytes) == *expected => {}
                (Some(_), Some(_)) => bail!(
                    "Remote base content for {} differs from the reviewed original hash",
                    file.path
                ),
                (Some(_), None) => bail!(
                    "Reviewed original file {} is absent from the remote base",
                    file.path
                ),
            }
            if sha256(file.content.as_bytes()) != file.content_sha256 {
                bail!(
                    "Reviewed candidate bytes for {} changed before publication",
                    file.path
                );
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
                let canonical_parent = git_process_path(&fs::canonicalize(parent)?)?;
                if !canonical_parent.starts_with(&checkout) {
                    bail!("Publication path escaped its private checkout");
                }
            }
            let target = checked_candidate_path(&checkout, &file.path)?;
            fs::write(&target, file.content.as_bytes())?;
        }
        let status = self
            .command(
                &self.git,
                &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
                Some(&checkout),
                cancellation,
                &[0],
                COMMAND_TIMEOUT,
                true,
            )
            .await?;
        let changed = parse_status_paths(&status.stdout)?;
        let expected: BTreeSet<String> = input.files.iter().map(|file| file.path.clone()).collect();
        if changed != expected {
            bail!("Private checkout changed set does not exactly match the reviewed file set");
        }
        let mut add_args = vec!["add", "--"];
        add_args.extend(input.files.iter().map(|file| file.path.as_str()));
        self.command(
            &self.git,
            &add_args,
            Some(&checkout),
            cancellation,
            &[0],
            COMMAND_TIMEOUT,
            true,
        )
        .await?;
        for file in &input.files {
            let index_path = format!(":{}", file.path);
            let blob = self
                .command(
                    &self.git,
                    &["show", &index_path],
                    Some(&checkout),
                    cancellation,
                    &[0],
                    COMMAND_TIMEOUT,
                    true,
                )
                .await?;
            if sha256(&blob.stdout) != file.content_sha256 {
                bail!(
                    "Git transformed reviewed bytes for {} before commit",
                    file.path
                );
            }
        }
        let staged = self
            .command(
                &self.git,
                &[
                    "diff",
                    "--cached",
                    "--name-only",
                    "-z",
                    "--diff-filter=ACMRTUXB",
                ],
                Some(&checkout),
                cancellation,
                &[0],
                COMMAND_TIMEOUT,
                true,
            )
            .await?;
        let staged_paths = parse_name_only_paths(&staged.stdout)?;
        if staged_paths != expected {
            bail!("Committed tree changed set does not exactly match the reviewed file set");
        }
        let tree = self
            .git_text(&["write-tree"], &checkout, cancellation)
            .await?;
        validate_git_oid(&tree)?;
        if let Some(bound) = record.candidate_tree_sha.as_deref() {
            if bound != tree {
                bail!("Prepared candidate tree changed during publication reconciliation");
            }
        } else {
            record.candidate_tree_sha = Some(tree.clone());
        }
        if record.author_name.is_none() {
            record.author_name = self.owner_git_value("user.name", cancellation).await?;
            record.author_email = self.owner_git_value("user.email", cancellation).await?;
        }
        let name = record
            .author_name
            .as_deref()
            .context("Git user.name is required for automatic publication")?;
        let email = record
            .author_email
            .as_deref()
            .context("Git user.email is required for automatic publication")?;
        validate_identity(name, "Git user name")?;
        validate_identity(email, "Git user email")?;
        let epoch = format!("{} +0000", record.started_unix_seconds);
        let subject = format!("Feature {}", input.feature_id);
        self.command_with_extra_env(
            &self.git,
            &[
                "-c",
                &format!("user.name={name}"),
                "-c",
                &format!("user.email={email}"),
                "commit",
                "--no-gpg-sign",
                "-m",
                &subject,
            ],
            Some(&checkout),
            cancellation,
            &[0],
            COMMAND_TIMEOUT,
            true,
            &[
                ("GIT_AUTHOR_DATE", epoch.as_str()),
                ("GIT_COMMITTER_DATE", epoch.as_str()),
            ],
            false,
        )
        .await?;
        let commit = self
            .git_text(&["rev-parse", "HEAD"], &checkout, cancellation)
            .await?;
        validate_git_oid(&commit)?;
        let committed_tree = self
            .git_text(
                &["rev-parse", &format!("{commit}^{{tree}}")],
                &checkout,
                cancellation,
            )
            .await?;
        if committed_tree != tree {
            bail!("Publication commit tree differs from the exact reviewed candidate tree");
        }
        let committed_parent = self
            .git_text(
                &["rev-parse", &format!("{commit}^")],
                &checkout,
                cancellation,
            )
            .await?;
        if committed_parent != base {
            bail!("Publication commit parent differs from the frozen remote base");
        }
        if let Some(bound) = record.commit_sha.as_deref() {
            if bound != commit {
                bail!("Prepared publication commit changed during reconciliation");
            }
        } else {
            record.commit_sha = Some(commit.clone());
        }
        let evidence = sha256(format!("{}:{}:{}", base, tree, commit).as_bytes());
        record.event("receipt", "prepare_candidate", Some(&evidence), "Remote base hashes matched and the exact reviewed file set produced one candidate commit")?;
        persist(record)?;
        Ok(())
    }

    async fn push_branch(
        &self,
        record: &mut PublicationRecord,
        cancellation: &AtomicU8,
        persist: &mut impl FnMut(&PublicationRecord) -> Result<()>,
    ) -> Result<()> {
        let commit = record
            .commit_sha
            .clone()
            .context("Publication commit is missing")?;
        let checkout = self.checkout_for_branch(&record.feature_branch)?;
        let reference = format!("refs/heads/{}", record.feature_branch);
        let observed = self
            .ls_remote(&record.repository_url, &reference, cancellation)
            .await?;
        if let Some(observed) = observed {
            if observed != commit {
                bail!("The remote feature branch exists at a different commit");
            }
            record.event(
                "receipt",
                "push_branch",
                Some(&commit),
                "The remote feature branch already contains the exact reviewed commit",
            )?;
            persist(record)?;
            return Ok(());
        }
        record.event(
            "intent",
            "push_branch",
            Some(&commit),
            "Pushing the exact reviewed commit to its feature branch",
        )?;
        persist(record)?;
        self.command(
            &self.git,
            &[
                "push",
                "--porcelain",
                &record.repository_url,
                &format!("HEAD:{reference}"),
            ],
            Some(&checkout),
            cancellation,
            &[0],
            COMMAND_TIMEOUT,
            true,
        )
        .await
        .context("Feature branch push was not confirmed")?;
        let observed = self
            .ls_remote(&record.repository_url, &reference, cancellation)
            .await?
            .context("Pushed feature branch is absent")?;
        if observed != commit {
            bail!("Pushed feature branch does not match the reviewed commit");
        }
        record.event(
            "receipt",
            "push_branch",
            Some(&commit),
            "Feature branch push was verified at the exact reviewed commit",
        )?;
        persist(record)?;
        Ok(())
    }

    async fn open_or_reconcile_pr(
        &self,
        input: &PublicationInput,
        record: &mut PublicationRecord,
        cancellation: &AtomicU8,
        persist: &mut impl FnMut(&PublicationRecord) -> Result<()>,
    ) -> Result<()> {
        let commit = record
            .commit_sha
            .clone()
            .context("Publication commit is missing")?;
        if record.pr_number.is_none() {
            let list = self
                .command(
                    &self.gh,
                    &[
                        "pr",
                        "list",
                        "--repo",
                        &record.repository_slug,
                        "--head",
                        &record.feature_branch,
                        "--state",
                        "all",
                        "--json",
                        "number,url,headRefOid,baseRefName,baseRefOid,state",
                    ],
                    None,
                    cancellation,
                    &[0],
                    COMMAND_TIMEOUT,
                    false,
                )
                .await?;
            let values: Vec<Value> = serde_json::from_slice(&list.stdout)
                .context("GitHub pull request list is invalid")?;
            if values.len() > 1 {
                bail!("More than one pull request exists for the immutable feature branch");
            }
            if let Some(value) = values.first() {
                bind_pr(record, value, &commit)?;
            } else {
                record.event(
                    "intent",
                    "open_pull_request",
                    Some(&commit),
                    "Opening a pull request for the exact reviewed commit",
                )?;
                persist(record)?;
                let created = self
                    .command(
                        &self.gh,
                        &[
                            "pr",
                            "create",
                            "--repo",
                            &record.repository_slug,
                            "--base",
                            &record.base_branch,
                            "--head",
                            &record.feature_branch,
                            "--title",
                            &input.title,
                            "--body",
                            &input.body,
                        ],
                        None,
                        cancellation,
                        &[0],
                        COMMAND_TIMEOUT,
                        false,
                    )
                    .await
                    .context("Pull request creation was not confirmed")?;
                let url = String::from_utf8(created.stdout)?.trim().to_owned();
                if !url.starts_with(&format!(
                    "https://github.com/{}/pull/",
                    record.repository_slug
                )) {
                    bail!("GitHub returned an unexpected pull request URL");
                }
                let value = self.pr_view(record, cancellation).await?;
                bind_pr(record, &value, &commit)?;
            }
        }
        let value = self.pr_view(record, cancellation).await?;
        bind_pr(record, &value, &commit)?;
        let evidence = sha256(
            format!(
                "{}:{}",
                record.pr_number.unwrap(),
                record.pr_url.as_deref().unwrap_or_default()
            )
            .as_bytes(),
        );
        record.event(
            "receipt",
            "open_pull_request",
            Some(&evidence),
            "Pull request identity, base, and exact head commit were verified",
        )?;
        persist(record)?;
        Ok(())
    }

    async fn wait_for_checks(
        &self,
        record: &mut PublicationRecord,
        cancellation: &AtomicU8,
        persist: &mut impl FnMut(&PublicationRecord) -> Result<()>,
    ) -> Result<()> {
        let number = record
            .pr_number
            .context("Pull request number is missing")?
            .to_string();
        let commit = record
            .commit_sha
            .clone()
            .context("Publication commit is missing")?;
        record.event(
            "intent",
            "wait_required_checks",
            Some(&commit),
            "Waiting for every required GitHub check on the exact candidate commit",
        )?;
        persist(record)?;
        let deadline = tokio::time::Instant::now() + CHECKS_TIMEOUT;
        loop {
            check_cancelled(cancellation)?;
            let view = self.pr_view(record, cancellation).await?;
            bind_pr(record, &view, &commit)?;
            let checks = self
                .command(
                    &self.gh,
                    &[
                        "pr",
                        "checks",
                        &number,
                        "--repo",
                        &record.repository_slug,
                        "--required",
                        "--json",
                        "name,state,bucket",
                    ],
                    None,
                    cancellation,
                    &[0, 8],
                    COMMAND_TIMEOUT,
                    false,
                )
                .await?;
            let values: Vec<Value> = serde_json::from_slice(&checks.stdout)
                .context("Required-check response is invalid")?;
            if values.is_empty() {
                bail!("The pull request has no required GitHub checks");
            }
            let mut pending = false;
            let mut evidence_rows = Vec::new();
            let mut observed_names = BTreeSet::new();
            for value in values {
                let name = value
                    .get("name")
                    .and_then(Value::as_str)
                    .context("Required check name is missing")?;
                let state = value
                    .get("state")
                    .and_then(Value::as_str)
                    .context("Required check state is missing")?;
                let bucket = value
                    .get("bucket")
                    .and_then(Value::as_str)
                    .context("Required check bucket is missing")?;
                evidence_rows.push(format!("{name}:{state}:{bucket}"));
                if !observed_names.insert(name.to_owned()) {
                    bail!("GitHub returned duplicate required-check identities");
                }
                match bucket {
                    "pass" => {}
                    "pending" => pending = true,
                    _ => bail!("Required GitHub check {name} did not pass ({state})"),
                }
            }
            let required_names: BTreeSet<String> = record
                .required_checks
                .iter()
                .map(|check| check.context.clone())
                .collect();
            if observed_names != required_names {
                bail!("GitHub required-check set changed from the saved branch policy");
            }
            if !pending {
                self.verify_required_check_apps(record, cancellation, true)
                    .await?;
                evidence_rows.sort();
                evidence_rows.extend(record.required_checks.iter().map(|check| {
                    format!(
                        "policy:{}:{}",
                        check.context,
                        check
                            .integration_id
                            .map(|value| value.to_string())
                            .unwrap_or_else(|| "any".into())
                    )
                }));
                let evidence = sha256(format!("{commit}:{}", evidence_rows.join("|")).as_bytes());
                record.event(
                    "receipt",
                    "wait_required_checks",
                    Some(&evidence),
                    "All required GitHub checks passed for the exact candidate commit",
                )?;
                persist(record)?;
                return Ok(());
            }
            self.verify_required_check_apps(record, cancellation, false)
                .await?;
            if tokio::time::Instant::now() >= deadline {
                bail!("Timed out waiting for required GitHub checks");
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    async fn merge_and_verify(
        &self,
        record: &mut PublicationRecord,
        cancellation: &AtomicU8,
        persist: &mut impl FnMut(&PublicationRecord) -> Result<()>,
    ) -> Result<()> {
        let number = record
            .pr_number
            .context("Pull request number is missing")?
            .to_string();
        let commit = record
            .commit_sha
            .clone()
            .context("Publication commit is missing")?;
        let before = self.pr_view(record, cancellation).await?;
        bind_pr(record, &before, &commit)?;
        let (current_checks, current_strict) = self
            .current_required_checks(&record.repository_slug, &record.base_branch, cancellation)
            .await?;
        if current_checks != record.required_checks
            || current_strict != record.strict_required_checks
            || !current_strict
        {
            bail!("GitHub required-check policy changed before merge");
        }
        let base_reference = format!("refs/heads/{}", record.base_branch);
        let live_base = self
            .ls_remote(&record.repository_url, &base_reference, cancellation)
            .await?
            .context("Selected remote base is unavailable before merge")?;
        if Some(live_base.as_str()) != record.base_sha.as_deref() {
            bail!("Selected remote base changed after validation and review");
        }
        if before.get("state").and_then(Value::as_str) != Some("MERGED") {
            record.event(
                "intent",
                "merge_pull_request",
                Some(&commit),
                "Requesting a normal merge guarded by the exact pull request head commit",
            )?;
            persist(record)?;
            self.command(
                &self.gh,
                &[
                    "pr",
                    "merge",
                    &number,
                    "--repo",
                    &record.repository_slug,
                    "--merge",
                    "--match-head-commit",
                    &commit,
                ],
                None,
                cancellation,
                &[0],
                COMMAND_TIMEOUT,
                false,
            )
            .await
            .context("GitHub merge request was not confirmed")?;
        }
        record.event(
            "intent",
            "verify_remote_base",
            Some(&commit),
            "Verifying the pull request merge and the selected remote base",
        )?;
        persist(record)?;
        let deadline = tokio::time::Instant::now() + CHECKS_TIMEOUT;
        loop {
            check_cancelled(cancellation)?;
            let view = self.pr_view(record, cancellation).await?;
            bind_pr(record, &view, &commit)?;
            if view.get("state").and_then(Value::as_str) == Some("MERGED") {
                let merged = view
                    .get("mergeCommit")
                    .and_then(|value| value.get("oid"))
                    .and_then(Value::as_str)
                    .context("Merged pull request has no merge commit")?
                    .to_owned();
                validate_git_oid(&merged)?;
                let reference = format!("refs/heads/{}", record.base_branch);
                if self
                    .ls_remote(&record.repository_url, &reference, cancellation)
                    .await?
                    .as_deref()
                    == Some(merged.as_str())
                {
                    self.verify_merged_tree(record, &merged, cancellation)
                        .await?;
                    record.merged_sha = Some(merged.clone());
                    record.event("receipt", "complete", Some(&merged), "GitHub reported the pull request merged and the remote base resolved to that merge commit")?;
                    record.status = "succeeded".into();
                    record.stage = "complete".into();
                    record.message = "Pull request checks passed, merge completed, and the remote base was verified".into();
                    persist(record)?;
                    return Ok(());
                }
            }
            if tokio::time::Instant::now() >= deadline {
                bail!("GitHub merge or remote base verification timed out");
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }

    async fn reconcile_if_merged(
        &self,
        record: &mut PublicationRecord,
        cancellation: &AtomicU8,
        persist: &mut impl FnMut(&PublicationRecord) -> Result<()>,
    ) -> Result<bool> {
        if record.pr_number.is_none() {
            return Ok(false);
        }
        let commit = record
            .commit_sha
            .clone()
            .context("Publication with a pull request is missing its exact commit")?;
        let view = self.pr_view(record, cancellation).await?;
        bind_pr(record, &view, &commit)?;
        if view.get("state").and_then(Value::as_str) != Some("MERGED") {
            return Ok(false);
        }
        let merged = view
            .get("mergeCommit")
            .and_then(|value| value.get("oid"))
            .and_then(Value::as_str)
            .context("Merged pull request has no merge commit")?
            .to_owned();
        validate_git_oid(&merged)?;
        let reference = format!("refs/heads/{}", record.base_branch);
        if self
            .ls_remote(&record.repository_url, &reference, cancellation)
            .await?
            .as_deref()
            != Some(merged.as_str())
        {
            bail!("Merged pull request is not the selected remote base head");
        }
        self.verify_merged_tree(record, &merged, cancellation)
            .await?;
        record.merged_sha = Some(merged.clone());
        record.event(
            "receipt",
            "complete",
            Some(&merged),
            "Interrupted publication reconciled to the already merged pull request and remote base",
        )?;
        record.status = "succeeded".into();
        record.stage = "complete".into();
        record.message =
            "Previously completed merge and remote base were verified without another write".into();
        persist(record)?;
        Ok(true)
    }

    async fn pr_view(&self, record: &PublicationRecord, cancellation: &AtomicU8) -> Result<Value> {
        let selector = record
            .pr_number
            .map(|n| n.to_string())
            .unwrap_or_else(|| record.feature_branch.clone());
        let output = self
            .command(
                &self.gh,
                &[
                    "pr",
                    "view",
                    &selector,
                    "--repo",
                    &record.repository_slug,
                    "--json",
                    "number,url,headRefOid,baseRefName,baseRefOid,state,mergeCommit",
                ],
                None,
                cancellation,
                &[0],
                COMMAND_TIMEOUT,
                false,
            )
            .await?;
        serde_json::from_slice(&output.stdout).context("GitHub pull request response is invalid")
    }

    async fn current_required_checks(
        &self,
        slug: &str,
        base_branch: &str,
        cancellation: &AtomicU8,
    ) -> Result<(Vec<RequiredCheck>, bool)> {
        let rules_endpoint = format!(
            "repos/{slug}/rules/branches/{}?per_page=100",
            percent_encode_path_component(base_branch)
        );
        let rules = self
            .command(
                &self.gh,
                &["api", "--paginate", "--slurp", &rules_endpoint],
                None,
                cancellation,
                &[0],
                COMMAND_TIMEOUT,
                false,
            )
            .await
            .context("GitHub required-check policy is unavailable")?;
        let (mut checks, ruleset_ids) = parse_required_checks(&rules.stdout)?;
        for ruleset_id in ruleset_ids {
            let detail_endpoint = format!("repos/{slug}/rulesets/{ruleset_id}");
            let detail = self
                .command(
                    &self.gh,
                    &["api", &detail_endpoint],
                    None,
                    cancellation,
                    &[0],
                    COMMAND_TIMEOUT,
                    false,
                )
                .await
                .context("GitHub ruleset enforcement evidence is unavailable")?;
            validate_ruleset_detail(&detail.stdout, ruleset_id)?;
        }
        let classic_endpoint = format!(
            "repos/{slug}/branches/{}/protection",
            percent_encode_path_component(base_branch)
        );
        let classic = self
            .command(
                &self.gh,
                &["api", &classic_endpoint],
                None,
                cancellation,
                &[0, 1],
                COMMAND_TIMEOUT,
                false,
            )
            .await
            .context("Classic GitHub branch protection is unavailable")?;
        if classic.status == 0 {
            let classic_checks = parse_classic_branch_protection(&classic.stdout)?;
            checks.extend(classic_checks);
        } else {
            let body: Value = serde_json::from_slice(&classic.stdout)
                .context("Classic GitHub branch protection error is invalid")?;
            if body.get("status").and_then(Value::as_str) != Some("404")
                && body.get("status").and_then(Value::as_u64) != Some(404)
            {
                bail!("Classic GitHub branch protection could not be inspected");
            }
        }
        checks = normalize_required_checks(checks)?;
        validate_required_checks(&checks)?;
        Ok((checks, true))
    }

    async fn verify_required_check_apps(
        &self,
        record: &PublicationRecord,
        cancellation: &AtomicU8,
        require_success: bool,
    ) -> Result<()> {
        let commit = record
            .commit_sha
            .as_deref()
            .context("Publication commit is missing")?;
        let endpoint = format!(
            "repos/{}/commits/{commit}/check-runs?per_page=100",
            record.repository_slug
        );
        let output = self
            .command(
                &self.gh,
                &["api", &endpoint],
                None,
                cancellation,
                &[0],
                COMMAND_TIMEOUT,
                false,
            )
            .await
            .context("GitHub check-run identity evidence is unavailable")?;
        let value: Value = serde_json::from_slice(&output.stdout)
            .context("GitHub check-run identity response is invalid")?;
        let runs = value
            .get("check_runs")
            .and_then(Value::as_array)
            .context("GitHub check-run list is missing")?;
        let total: usize = value
            .get("total_count")
            .and_then(Value::as_u64)
            .context("GitHub check-run total is missing")?
            .try_into()?;
        if total != runs.len() {
            bail!("GitHub check-run identity list is incomplete");
        }
        for required in &record.required_checks {
            let expected_id = required
                .integration_id
                .filter(|value| *value != 0)
                .context("Saved required-check app identity is missing")?;
            let matching: Vec<&Value> = runs
                .iter()
                .filter(|run| {
                    run.get("name").and_then(Value::as_str) == Some(required.context.as_str())
                        && run.get("head_sha").and_then(Value::as_str) == Some(commit)
                        && run
                            .get("app")
                            .and_then(|app| app.get("id"))
                            .and_then(Value::as_u64)
                            == Some(expected_id)
                })
                .collect();
            if matching.is_empty()
                || (require_success
                    && matching.iter().any(|run| {
                        run.get("status").and_then(Value::as_str) != Some("completed")
                            || run.get("conclusion").and_then(Value::as_str) != Some("success")
                    }))
            {
                bail!("Required GitHub check app identity does not match the saved policy");
            }
        }
        Ok(())
    }

    async fn verify_merged_tree(
        &self,
        record: &PublicationRecord,
        merged: &str,
        cancellation: &AtomicU8,
    ) -> Result<()> {
        let checkout = self.checkout_for_branch(&record.feature_branch)?;
        self.command(
            &self.git,
            &["fetch", "--no-tags", &record.repository_url, merged],
            Some(&checkout),
            cancellation,
            &[0],
            COMMAND_TIMEOUT,
            true,
        )
        .await
        .context("Could not fetch the verified merge commit")?;
        let expression = format!("{merged}^{{tree}}");
        let tree = self
            .git_text(&["rev-parse", &expression], &checkout, cancellation)
            .await?;
        if Some(tree.as_str()) != record.candidate_tree_sha.as_deref() {
            bail!("Merged repository tree differs from the exact reviewed candidate tree");
        }
        Ok(())
    }

    async fn ls_remote(
        &self,
        url: &str,
        reference: &str,
        cancellation: &AtomicU8,
    ) -> Result<Option<String>> {
        let output = self
            .command(
                &self.git,
                &["ls-remote", url, reference],
                None,
                cancellation,
                &[0],
                COMMAND_TIMEOUT,
                true,
            )
            .await?;
        if output.stdout.is_empty() {
            return Ok(None);
        }
        Ok(Some(parse_ls_remote(&output.stdout, reference)?))
    }

    async fn git_text(&self, args: &[&str], cwd: &Path, cancellation: &AtomicU8) -> Result<String> {
        let output = self
            .command(
                &self.git,
                args,
                Some(cwd),
                cancellation,
                &[0],
                COMMAND_TIMEOUT,
                true,
            )
            .await?;
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    }

    async fn owner_git_value(&self, key: &str, cancellation: &AtomicU8) -> Result<Option<String>> {
        let output = self
            .command(
                &self.git,
                &["config", "--global", "--get", key],
                None,
                cancellation,
                &[0, 1],
                COMMAND_TIMEOUT,
                false,
            )
            .await?;
        if output.status == 1 {
            return Ok(None);
        }
        let value = String::from_utf8(output.stdout)?.trim().to_owned();
        validate_identity(&value, key)?;
        Ok(Some(value))
    }

    fn checkout_for_branch(&self, branch: &str) -> Result<PathBuf> {
        let id = branch
            .strip_prefix("codex/feature-")
            .context("Publication branch is invalid")?;
        validate_uuid_text(id)?;
        let path = self.checkout_root.join(id);
        if !path.join(".git").is_dir() {
            bail!("Private publication checkout is unavailable");
        }
        Ok(path)
    }

    // Keep every process-boundary control explicit at each call site; folding these
    // into defaults would make timeout, accepted exits, cwd, or Git isolation easier
    // to omit during security review.
    #[allow(clippy::too_many_arguments)]
    async fn command(
        &self,
        executable: &Path,
        args: &[&str],
        cwd: Option<&Path>,
        cancellation: &AtomicU8,
        accepted: &[i32],
        timeout: Duration,
        git_sanitized: bool,
    ) -> Result<CommandOutput> {
        self.command_with_extra_env(
            executable,
            args,
            cwd,
            cancellation,
            accepted,
            timeout,
            git_sanitized,
            &[],
            false,
        )
        .await
    }

    async fn setup_command(
        &self,
        args: &[&str],
        cancellation: &AtomicU8,
        accepted: &[i32],
        timeout: Duration,
    ) -> Result<CommandOutput> {
        self.command_with_extra_env(
            &self.gh,
            args,
            None,
            cancellation,
            accepted,
            timeout,
            false,
            &[],
            false,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn command_with_extra_env(
        &self,
        executable: &Path,
        args: &[&str],
        cwd: Option<&Path>,
        cancellation: &AtomicU8,
        accepted: &[i32],
        timeout: Duration,
        git_sanitized: bool,
        extra_env: &[(&str, &str)],
        remove_auth_environment: bool,
    ) -> Result<CommandOutput> {
        check_cancelled(cancellation)?;
        let mut command = Command::new(executable);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        command.current_dir(cwd.unwrap_or(&self.checkout_root));
        command
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GH_PROMPT_DISABLED", "1")
            .env("GH_PAGER", "cat")
            .env("PAGER", "cat");
        if executable == self.gh {
            command.env("GH_HOST", "github.com").env_remove("GH_REPO");
            if remove_auth_environment {
                for key in ["GH_TOKEN", "GITHUB_TOKEN"] {
                    command.env_remove(key);
                }
            }
        }
        for key in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_SSH",
            "GIT_SSH_COMMAND",
            "GIT_PROXY_COMMAND",
            "GIT_CONFIG_COUNT",
            "GIT_CONFIG_PARAMETERS",
        ] {
            command.env_remove(key);
        }
        if git_sanitized {
            let null_config = if cfg!(windows) { "NUL" } else { "/dev/null" };
            let helper_path = self.gh.to_string_lossy().replace('\\', "/");
            let helper = format!("!\"{helper_path}\" auth git-credential");
            command
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", null_config)
                .env("GIT_CONFIG_COUNT", "5")
                .env("GIT_CONFIG_KEY_0", "core.hooksPath")
                .env("GIT_CONFIG_VALUE_0", &self.empty_hooks)
                .env("GIT_CONFIG_KEY_1", "credential.helper")
                .env("GIT_CONFIG_VALUE_1", "")
                .env("GIT_CONFIG_KEY_2", "credential.https://github.com.helper")
                .env("GIT_CONFIG_VALUE_2", helper)
                .env("GIT_CONFIG_KEY_3", "protocol.file.allow")
                .env("GIT_CONFIG_VALUE_3", "never")
                .env("GIT_CONFIG_KEY_4", "submodule.recurse")
                .env("GIT_CONFIG_VALUE_4", "false");
        }
        for (key, value) in extra_env {
            command.env(key, value);
        }
        process_tree::prepare_process_tree(&mut command);
        let mut child = command
            .spawn()
            .with_context(|| format!("Could not start {}", executable.display()))?;
        let mut tree = match process_tree::attach_process_tree(&mut child).await {
            Ok(tree) => Some(tree),
            Err(error) => return Err(error),
        };
        let stdout = child
            .stdout
            .take()
            .context("Publication stdout is unavailable")?;
        let stderr = child
            .stderr
            .take()
            .context("Publication stderr is unavailable")?;
        let stdout_task = tokio::spawn(async move {
            let mut bytes = Vec::new();
            stdout
                .take((OUTPUT_LIMIT + 1) as u64)
                .read_to_end(&mut bytes)
                .await
                .map(|_| bytes)
        });
        let stderr_task = tokio::spawn(async move {
            let mut bytes = Vec::new();
            stderr
                .take((OUTPUT_LIMIT + 1) as u64)
                .read_to_end(&mut bytes)
                .await
                .map(|_| bytes)
        });
        let deadline = tokio::time::Instant::now() + timeout;
        let mut forced_error: Option<anyhow::Error> = None;
        let status = loop {
            tokio::select! {
                result = child.wait() => break Some(result?),
                _ = tokio::time::sleep(Duration::from_millis(50)) => {
                    let reason = if cancellation.load(Ordering::SeqCst) != 0 {
                        Some("GitHub publication was cancelled")
                    } else if tokio::time::Instant::now() >= deadline {
                        Some("GitHub publication command timed out")
                    } else {
                        None
                    };
                    if let Some(reason) = reason {
                        let termination = if let Some(tree) = tree.as_mut() {
                            tree.terminate_and_wait(&mut child, Duration::from_secs(10)).await
                        } else {
                            child.kill().await.map_err(Into::into)
                        };
                        forced_error = Some(if termination.is_ok() {
                            anyhow!(reason)
                        } else {
                            anyhow!("Publication process termination could not be confirmed")
                        });
                        if termination.is_err() {
                            drop(tree.take());
                        }
                        break None;
                    }
                }
            }
        };
        let process_confirmation = if forced_error.is_none() {
            if let Some(tree) = tree.as_mut() {
                tree.confirm_stopped(Duration::from_secs(10)).await
            } else {
                Ok(())
            }
        } else {
            Ok(())
        };
        if process_confirmation.is_err() {
            drop(tree.take());
        }
        let (stdout, stderr) = tokio::time::timeout(Duration::from_secs(10), async {
            Ok::<_, anyhow::Error>((stdout_task.await??, stderr_task.await??))
        })
        .await
        .context("Publication output pipes did not close after process completion")??;
        process_confirmation?;
        if let Some(error) = forced_error {
            return Err(error);
        }
        if stdout.len() > OUTPUT_LIMIT || stderr.len() > OUTPUT_LIMIT {
            bail!("GitHub publication command output exceeded its bound");
        }
        let code = status
            .context("Publication command completion status is missing")?
            .code()
            .context("GitHub publication command ended without an exit code")?;
        if !accepted.contains(&code) {
            let _ = stderr;
            bail!("GitHub publication command failed with exit code {code}");
        }
        if let Some(tree) = tree.as_mut() {
            tree.disarm();
        }
        Ok(CommandOutput {
            status: code,
            stdout,
        })
    }
}

struct CommandOutput {
    status: i32,
    stdout: Vec<u8>,
}

fn bind_pr(record: &mut PublicationRecord, value: &Value, commit: &str) -> Result<()> {
    let number = value
        .get("number")
        .and_then(Value::as_u64)
        .context("Pull request number is missing")?;
    let url = value
        .get("url")
        .and_then(Value::as_str)
        .context("Pull request URL is missing")?;
    let head = value
        .get("headRefOid")
        .and_then(Value::as_str)
        .context("Pull request head is missing")?;
    let base = value
        .get("baseRefName")
        .and_then(Value::as_str)
        .context("Pull request base is missing")?;
    let state = value
        .get("state")
        .and_then(Value::as_str)
        .context("Pull request state is missing")?;
    if !matches!(state, "OPEN" | "MERGED") {
        bail!("Pull request state is not publishable");
    }
    if state == "OPEN" {
        let base_oid = value
            .get("baseRefOid")
            .and_then(Value::as_str)
            .context("Open pull request base commit is missing")?;
        if Some(base_oid) != record.base_sha.as_deref() {
            bail!("Pull request base changed after candidate validation");
        }
    }
    if head != commit {
        bail!("Pull request head changed from the exact reviewed commit");
    }
    if base != record.base_branch {
        bail!("Pull request base changed from the selected branch");
    }
    let expected_url = format!(
        "https://github.com/{}/pull/{number}",
        record.repository_slug
    );
    if url != expected_url {
        bail!(
            "Pull request URL does not exactly identify the selected repository and pull request"
        );
    }
    if record.pr_number.is_some_and(|bound| bound != number)
        || record.pr_url.as_deref().is_some_and(|bound| bound != url)
    {
        bail!("Pull request identity changed during publication");
    }
    record.pr_number = Some(number);
    record.pr_url = Some(url.into());
    Ok(())
}

pub(super) fn validate_binding(binding: &ProjectBinding) -> Result<()> {
    validate_project(&binding.project)?;
    if canonical_repository(&binding.repository_url)? != binding.repository_slug {
        bail!("Persisted GitHub repository URL and identity disagree");
    }
    validate_branch(&binding.base_branch)?;
    validate_required_checks(&binding.required_checks)?;
    if !binding.strict_required_checks {
        bail!("GitHub required checks must require the latest base branch");
    }
    Ok(())
}

fn validate_required_checks(checks: &[RequiredCheck]) -> Result<()> {
    if checks.is_empty() || checks.len() > 40 {
        bail!("GitHub base branch must have 1 to 40 required checks");
    }
    let mut unique = BTreeSet::new();
    for check in checks {
        if check.context.is_empty()
            || check.context.len() > 200
            || check.context.chars().any(char::is_control)
            || check.integration_id.is_none_or(|value| value == 0)
            || !unique.insert(&check.context)
        {
            bail!("GitHub required-check identity is invalid");
        }
    }
    Ok(())
}

fn normalize_required_checks(checks: Vec<RequiredCheck>) -> Result<Vec<RequiredCheck>> {
    let mut normalized = std::collections::BTreeMap::<String, Option<u64>>::new();
    for check in checks {
        match normalized.get(&check.context).copied().flatten() {
            Some(existing) if check.integration_id.is_some_and(|value| value != existing) => {
                bail!("GitHub required-check policy has conflicting app identities")
            }
            Some(_) => {}
            None => {
                let entry = normalized.entry(check.context).or_insert(None);
                if check.integration_id.is_some() {
                    *entry = check.integration_id;
                }
            }
        }
    }
    Ok(normalized
        .into_iter()
        .map(|(context, integration_id)| RequiredCheck {
            context,
            integration_id,
        })
        .collect())
}

fn record_challenge(
    observed: &mut Vec<u8>,
    chunk: &[u8],
    challenge_seen: &mut bool,
    callback: &mut impl FnMut(GithubDeviceChallenge) -> Result<()>,
) -> Result<()> {
    append_setup_output(observed, chunk)?;
    if !*challenge_seen {
        if let Some(challenge) = parse_device_challenge(observed)? {
            callback(challenge)?;
            *challenge_seen = true;
        }
    }
    Ok(())
}

fn append_setup_output(observed: &mut Vec<u8>, chunk: &[u8]) -> Result<()> {
    if observed.len().saturating_add(chunk.len()) > SETUP_OUTPUT_LIMIT {
        bail!("GitHub setup output exceeded its bound");
    }
    observed.extend_from_slice(chunk);
    Ok(())
}

fn parse_device_challenge(bytes: &[u8]) -> Result<Option<GithubDeviceChallenge>> {
    if !bytes
        .windows(DEVICE_URL.len())
        .any(|window| window == DEVICE_URL.as_bytes())
    {
        return Ok(None);
    }
    let mut codes = BTreeSet::new();
    for (index, window) in bytes.windows(9).enumerate() {
        let valid = window[4] == b'-'
            && window.iter().enumerate().all(|(position, byte)| {
                position == 4 || byte.is_ascii_uppercase() || byte.is_ascii_digit()
            });
        let bounded_before = index == 0 || !bytes[index - 1].is_ascii_alphanumeric();
        let bounded_after = index + 9 == bytes.len() || !bytes[index + 9].is_ascii_alphanumeric();
        if valid && bounded_before && bounded_after {
            codes.insert(std::str::from_utf8(window)?.to_owned());
        }
    }
    match codes.len() {
        0 => Ok(None),
        1 => Ok(Some(GithubDeviceChallenge {
            user_code: codes.into_iter().next().unwrap(),
            verification_url: DEVICE_URL,
        })),
        _ => bail!("GitHub returned more than one device challenge"),
    }
}

#[cfg(windows)]
fn github_browser_sink_command() -> Result<String> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::SystemInformation::GetSystemDirectoryW;
    let mut buffer = vec![0u16; 32768];
    let length = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) };
    if length == 0 || length as usize >= buffer.len() {
        bail!("Windows system browser sink path is unavailable");
    }
    let directory = PathBuf::from(std::ffi::OsString::from_wide(&buffer[..length as usize]));
    let directory = fs::canonicalize(directory)?;
    let executable = fs::canonicalize(directory.join("cmd.exe"))?;
    if !executable.starts_with(&directory)
        || !executable.is_file()
        || executable
            .file_name()
            .and_then(|name| name.to_str())
            .is_none_or(|name| !name.eq_ignore_ascii_case("cmd.exe"))
    {
        bail!("Windows system browser sink identity is invalid");
    }
    let executable = git_process_path(&executable)?;
    Ok(format!("\"{}\" /d /c exit 0", executable.display()))
}

#[cfg(unix)]
fn github_browser_sink_command() -> Result<String> {
    let executable = fs::canonicalize("/usr/bin/true")?;
    if !executable.is_file() {
        bail!("System browser sink is unavailable");
    }
    Ok(executable.to_string_lossy().into_owned())
}

#[cfg(not(any(unix, windows)))]
fn github_browser_sink_command() -> Result<String> {
    bail!("GitHub device sign-in is unavailable on this platform")
}

fn parse_account_status(bytes: &[u8]) -> Result<Option<String>> {
    if bytes.is_empty() {
        return Ok(None);
    }
    let value: Value =
        serde_json::from_slice(bytes).context("GitHub CLI account status is invalid")?;
    let accounts = value
        .get("hosts")
        .and_then(|hosts| hosts.get("github.com"))
        .and_then(Value::as_array)
        .context("GitHub CLI account status has no github.com account list")?;
    let active = accounts
        .iter()
        .filter(|account| account.get("active").and_then(Value::as_bool) == Some(true))
        .collect::<Vec<_>>();
    if active.is_empty() {
        return Ok(None);
    }
    if active.len() != 1 {
        bail!("GitHub CLI reported more than one active account");
    }
    let account = active[0];
    if account.get("host").and_then(Value::as_str) != Some("github.com") {
        bail!("GitHub CLI active account host changed");
    }
    if account.get("state").and_then(Value::as_str) != Some("success") {
        return Ok(None);
    }
    let login = account
        .get("login")
        .and_then(Value::as_str)
        .context("GitHub CLI active account login is missing")?;
    validate_account_login(login)?;
    Ok(Some(login.into()))
}

fn account_observations_match(
    stored: &GithubAccountObservation,
    effective: &GithubAccountObservation,
) -> bool {
    match (stored, effective) {
        (
            GithubAccountObservation::SignedIn { login: stored },
            GithubAccountObservation::SignedIn { login: effective },
        ) => stored.eq_ignore_ascii_case(effective),
        (
            GithubAccountObservation::SignedOut { .. },
            GithubAccountObservation::SignedOut { .. },
        ) => true,
        _ => false,
    }
}

fn github_api_error_status(bytes: &[u8]) -> Option<u64> {
    let value: Value = serde_json::from_slice(bytes).ok()?;
    value
        .get("status")
        .and_then(|status| {
            status
                .as_u64()
                .or_else(|| status.as_str().and_then(|status| status.parse().ok()))
        })
        .or_else(|| match value.get("message").and_then(Value::as_str) {
            Some("Bad credentials") | Some("Requires authentication") => Some(401),
            Some("Not Found") => Some(404),
            _ => None,
        })
}

fn parse_repository_page(bytes: &[u8]) -> Result<Vec<GithubRepositoryObservation>> {
    let values: Vec<Value> =
        serde_json::from_slice(bytes).context("GitHub repository page is invalid")?;
    if values.len() > REPOSITORY_PAGE_SIZE as usize {
        bail!("GitHub repository page exceeds its bound");
    }
    values.iter().map(parse_repository_value).collect()
}

fn parse_repository(bytes: &[u8]) -> Result<GithubRepositoryObservation> {
    let value: Value =
        serde_json::from_slice(bytes).context("GitHub repository response is invalid")?;
    parse_repository_value(&value)
}

fn ensure_repository_identity(
    repository: &GithubRepositoryObservation,
    expected_slug: &str,
) -> Result<()> {
    if repository.name_with_owner != expected_slug
        || repository.url != format!("https://github.com/{}", repository.name_with_owner)
    {
        bail!("GitHub repository observation changed identity");
    }
    Ok(())
}

fn apply_repository_branch_evidence(
    repositories: &mut [GithubRepositoryObservation],
    value: &Value,
) -> Result<()> {
    let data = value
        .get("data")
        .and_then(Value::as_object)
        .context("GitHub repository branch evidence is missing")?;
    if data.len() != repositories.len() {
        bail!("GitHub repository branch evidence is incomplete");
    }
    for (index, repository) in repositories.iter_mut().enumerate() {
        let observed = data
            .get(&format!("r{index}"))
            .and_then(Value::as_object)
            .context("GitHub repository branch observation is missing")?;
        if observed.get("nameWithOwner").and_then(Value::as_str)
            != Some(repository.name_with_owner.as_str())
        {
            bail!("GitHub repository branch identity changed");
        }
        let is_empty = observed
            .get("isEmpty")
            .and_then(Value::as_bool)
            .context("GitHub repository empty-state evidence is missing")?;
        let branch = match observed.get("defaultBranchRef") {
            Some(Value::Null) => None,
            Some(Value::Object(reference)) => Some(
                reference
                    .get("name")
                    .and_then(Value::as_str)
                    .context("GitHub repository default branch reference is invalid")?,
            ),
            _ => bail!("GitHub repository default branch evidence is missing"),
        };
        match (is_empty, branch) {
            (true, None) => repository.default_branch.clear(),
            (false, Some(branch)) => {
                validate_branch(branch)?;
                repository.default_branch = branch.into();
            }
            _ => bail!("GitHub repository empty and branch evidence disagree"),
        }
    }
    Ok(())
}

fn parse_repository_value(value: &Value) -> Result<GithubRepositoryObservation> {
    let repository_id = value
        .get("id")
        .and_then(Value::as_u64)
        .filter(|value| *value != 0)
        .context("GitHub repository ID is missing")?;
    let name_with_owner = value
        .get("full_name")
        .or_else(|| value.get("nameWithOwner"))
        .and_then(Value::as_str)
        .context("GitHub repository identity is missing")?;
    let url = value
        .get("html_url")
        .or_else(|| value.get("url"))
        .and_then(Value::as_str)
        .context("GitHub repository URL is missing")?;
    let visibility = value
        .get("visibility")
        .and_then(Value::as_str)
        .context("GitHub repository visibility is missing")?;
    let default_branch = value
        .get("default_branch")
        .or_else(|| value.pointer("/defaultBranchRef/name"))
        .and_then(Value::as_str)
        .context("GitHub repository default branch is missing")?;
    let can_push = value
        .pointer("/permissions/push")
        .and_then(Value::as_bool)
        .or_else(|| {
            value
                .get("viewerPermission")
                .and_then(Value::as_str)
                .map(|permission| matches!(permission, "ADMIN" | "MAINTAIN" | "WRITE"))
        })
        .context("GitHub repository write permission is missing")?;
    let slug = canonical_repository(url)?;
    if slug != name_with_owner || url != format!("https://github.com/{name_with_owner}") {
        bail!("GitHub repository URL and identity disagree");
    }
    if !matches!(visibility, "public" | "private" | "internal") {
        bail!("GitHub repository visibility is invalid");
    }
    validate_branch(default_branch)?;
    Ok(GithubRepositoryObservation {
        repository_id,
        name_with_owner: name_with_owner.into(),
        url: url.into(),
        visibility: visibility.into(),
        default_branch: default_branch.into(),
        can_push,
    })
}

fn validate_account_login(login: &str) -> Result<()> {
    if login.is_empty()
        || login.len() > 39
        || login.starts_with('-')
        || login.ends_with('-')
        || !login
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        bail!("GitHub account login is invalid");
    }
    Ok(())
}

fn validate_new_repository_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 100
        || matches!(name, "." | "..")
        || name.to_ascii_lowercase().ends_with(".git")
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        bail!("Use a simple GitHub repository name");
    }
    Ok(())
}

fn validate_candidate_files(files: &[CandidateFile]) -> Result<()> {
    if files.is_empty() || files.len() > 40 {
        bail!("Publication requires 1 to 40 reviewed files");
    }
    let mut paths = BTreeSet::new();
    let mut portable_paths = BTreeSet::new();
    let mut total_bytes = 0usize;
    for file in files {
        validate_relative_path(&file.path)?;
        if !paths.insert(file.path.clone()) {
            bail!("Publication candidate repeats a file path");
        }
        if !portable_paths.insert(file.path.to_ascii_lowercase()) {
            bail!("Publication candidate paths collide on the Windows owner host");
        }
        validate_sha256(&file.content_sha256)?;
        if let Some(before) = &file.before_sha256 {
            validate_sha256(before)?;
        }
        if sha256(file.content.as_bytes()) != file.content_sha256 {
            bail!("Publication candidate content hash is invalid");
        }
        total_bytes = total_bytes
            .checked_add(file.content.len())
            .context("Publication candidate size overflow")?;
        if total_bytes > 2 * 1024 * 1024 {
            bail!("Publication candidate exceeds its byte bound");
        }
    }
    Ok(())
}

fn validate_pull_request_text(title: &str, body: &str) -> Result<()> {
    if title.trim().is_empty()
        || title.len() > 256
        || title.chars().any(char::is_control)
        || body.trim().is_empty()
        || body.len() > 8_000
        || body.chars().any(|value| value == '\0')
    {
        bail!("Pull request title or body is invalid");
    }
    Ok(())
}

fn validate_stage(stage: &str) -> Result<()> {
    if !matches!(
        stage,
        "prepare_candidate"
            | "push_branch"
            | "open_pull_request"
            | "wait_required_checks"
            | "merge_pull_request"
            | "verify_remote_base"
            | "complete"
    ) {
        bail!("Persisted Developer publication stage is invalid");
    }
    Ok(())
}

fn validate_project(project: &str) -> Result<()> {
    if project.is_empty() || project.len() > 160 {
        bail!("Project name is required");
    }
    validate_relative_path(project)
}

fn validate_branch(branch: &str) -> Result<()> {
    if branch.is_empty()
        || branch.len() > 120
        || branch.starts_with('-')
        || branch.starts_with('/')
        || branch.ends_with('/')
        || branch.contains("..")
        || branch.contains("@{")
        || branch.ends_with('.')
        || branch.chars().any(|c| {
            c.is_control()
                || c.is_whitespace()
                || matches!(c, '~' | '^' | ':' | '?' | '*' | '[' | '\\')
        })
    {
        bail!("GitHub base branch is invalid");
    }
    Ok(())
}

fn canonical_repository(url: &str) -> Result<String> {
    let rest = url
        .strip_prefix("https://github.com/")
        .context("Use a canonical https://github.com/owner/repository URL")?;
    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    let mut parts = rest.split('/');
    let owner = parts.next().unwrap_or_default();
    let repo = parts.next().unwrap_or_default();
    if parts.next().is_some() || !valid_repo_component(owner) || !valid_repo_component(repo) {
        bail!("GitHub repository URL must identify one owner and repository");
    }
    Ok(format!("{owner}/{repo}"))
}

fn valid_repo_component(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        && value != "."
        && value != ".."
}

fn validate_relative_path(value: &str) -> Result<()> {
    let path = Path::new(value);
    if value.contains('\\')
        || path.is_absolute()
        || value.len() > 500
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        bail!("Publication path is invalid");
    }
    if path.components().any(|part| {
        part.as_os_str()
            .to_string_lossy()
            .eq_ignore_ascii_case(".git")
    }) {
        bail!("Publication cannot modify Git metadata");
    }
    Ok(())
}

fn checked_candidate_path(root: &Path, relative: &str) -> Result<PathBuf> {
    validate_relative_path(relative)?;
    let mut cursor = root.to_path_buf();
    let components: Vec<_> = Path::new(relative).components().collect();
    for component in components.iter().take(components.len().saturating_sub(1)) {
        cursor.push(component.as_os_str());
        if let Ok(metadata) = fs::symlink_metadata(&cursor) {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                bail!("Publication path crosses a non-directory or symbolic link");
            }
        }
    }
    let target = root.join(relative);
    if fs::symlink_metadata(&target).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        bail!("Publication target is a symbolic link");
    }
    Ok(target)
}

fn parse_status_paths(bytes: &[u8]) -> Result<BTreeSet<String>> {
    let mut paths = BTreeSet::new();
    for entry in bytes
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        if entry.len() < 4 || entry[2] != b' ' {
            bail!("Git status response is invalid");
        }
        if matches!(entry[0], b'R' | b'C') || matches!(entry[1], b'R' | b'C') {
            bail!("Git status reported an unreviewed rename or copy");
        }
        let path = std::str::from_utf8(&entry[3..]).context("Git status path is not UTF-8")?;
        validate_relative_path(path)?;
        paths.insert(path.into());
    }
    Ok(paths)
}

fn parse_name_only_paths(bytes: &[u8]) -> Result<BTreeSet<String>> {
    let mut paths = BTreeSet::new();
    for entry in bytes
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
    {
        let path = std::str::from_utf8(entry).context("Git changed-file path is not UTF-8")?;
        validate_relative_path(path)?;
        if !paths.insert(path.into()) {
            bail!("Git changed-file response repeats a path");
        }
    }
    Ok(paths)
}

fn parse_required_checks(bytes: &[u8]) -> Result<(Vec<RequiredCheck>, BTreeSet<u64>)> {
    let pages: Vec<Value> =
        serde_json::from_slice(bytes).context("GitHub branch rules response is invalid")?;
    let rules: Vec<Value> = if pages.iter().all(Value::is_array) {
        pages
            .into_iter()
            .flat_map(|page| page.as_array().cloned().unwrap_or_default())
            .collect()
    } else if pages.iter().all(Value::is_object) {
        pages
    } else {
        bail!("GitHub branch rules response has an ambiguous page shape");
    };
    let mut checks = BTreeSet::new();
    let mut ruleset_ids = BTreeSet::new();
    for rule in rules {
        if rule.get("type").and_then(Value::as_str) == Some("merge_queue") {
            bail!("GitHub merge-queue policy is unsupported for exact automatic publication");
        }
        if rule.get("type").and_then(Value::as_str) != Some("required_status_checks") {
            continue;
        }
        let ruleset_id = rule
            .get("ruleset_id")
            .and_then(Value::as_u64)
            .filter(|value| *value != 0)
            .context("GitHub required-check rule has no ruleset identity")?;
        ruleset_ids.insert(ruleset_id);
        let parameters = rule
            .get("parameters")
            .context("GitHub required-check rule parameters are missing")?;
        if parameters
            .get("strict_required_status_checks_policy")
            .and_then(Value::as_bool)
            != Some(true)
        {
            bail!("GitHub ruleset checks must require the latest base branch");
        }
        let required = parameters
            .get("required_status_checks")
            .and_then(Value::as_array)
            .context("GitHub required-check rule is malformed")?;
        if required.is_empty() {
            bail!("GitHub required-check rule has no checks");
        }
        for check in required {
            let context = check
                .get("context")
                .and_then(Value::as_str)
                .context("GitHub required-check context is missing")?;
            let integration_id = check
                .get("integration_id")
                .and_then(Value::as_u64)
                .filter(|value| *value != 0)
                .context("GitHub required-check app identity is missing")?;
            checks.insert(RequiredCheck {
                context: context.to_owned(),
                integration_id: Some(integration_id),
            });
        }
    }
    Ok((checks.into_iter().collect(), ruleset_ids))
}

fn validate_ruleset_detail(bytes: &[u8], expected_id: u64) -> Result<()> {
    let value: Value =
        serde_json::from_slice(bytes).context("GitHub ruleset detail response is invalid")?;
    let bypass = value
        .get("bypass_actors")
        .and_then(Value::as_array)
        .context("GitHub ruleset bypass policy is missing")?;
    if value.get("id").and_then(Value::as_u64) != Some(expected_id)
        || value.get("target").and_then(Value::as_str) != Some("branch")
        || value.get("enforcement").and_then(Value::as_str) != Some("active")
        || !bypass.is_empty()
    {
        bail!("GitHub ruleset permits ambiguous or bypassed enforcement");
    }
    Ok(())
}

fn parse_classic_branch_protection(bytes: &[u8]) -> Result<Vec<RequiredCheck>> {
    let value: Value =
        serde_json::from_slice(bytes).context("Classic branch protection is invalid")?;
    if value
        .pointer("/required_status_checks/strict")
        .and_then(Value::as_bool)
        != Some(true)
        || value
            .pointer("/enforce_admins/enabled")
            .and_then(Value::as_bool)
            != Some(true)
        || !classic_pull_request_bypass_is_empty(&value)
    {
        bail!("Classic branch protection permits stale or bypassed checks");
    }
    let required = value
        .get("required_status_checks")
        .context("Classic required-check policy is missing")?;
    let checks = required
        .get("checks")
        .and_then(Value::as_array)
        .context("Classic required-check app identities are missing")?;
    let contexts: BTreeSet<&str> = required
        .get("contexts")
        .and_then(Value::as_array)
        .context("Classic required-check contexts are missing")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .context("Classic required-check context is invalid")
        })
        .collect::<Result<_>>()?;
    let mut result = BTreeSet::new();
    for check in checks {
        let context = check
            .get("context")
            .and_then(Value::as_str)
            .context("Classic required-check context is missing")?;
        let app_id = check
            .get("app_id")
            .and_then(Value::as_u64)
            .filter(|value| *value != 0)
            .context("Classic required-check app identity is missing")?;
        result.insert(RequiredCheck {
            context: context.into(),
            integration_id: Some(app_id),
        });
    }
    for context in contexts {
        if !result.iter().any(|check| check.context == context) {
            bail!("Classic required-check context has no bound app identity");
        }
    }
    Ok(result.into_iter().collect())
}

fn classic_pull_request_bypass_is_empty(value: &Value) -> bool {
    let allowances = match value.get("required_pull_request_reviews") {
        None | Some(Value::Null) => return true,
        Some(Value::Object(reviews)) => reviews.get("bypass_pull_request_allowances"),
        Some(_) => return false,
    };
    match allowances {
        None | Some(Value::Null) => true,
        Some(Value::Object(entries)) => {
            entries
                .keys()
                .all(|key| matches!(key.as_str(), "users" | "teams" | "apps"))
                && entries
                    .values()
                    .all(|entry| entry.as_array().is_some_and(Vec::is_empty))
        }
        Some(_) => false,
    }
}

fn percent_encode_path_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn parse_ls_remote(bytes: &[u8], reference: &str) -> Result<String> {
    let text = std::str::from_utf8(bytes)?.trim();
    let mut fields = text.split_whitespace();
    let sha = fields.next().context("Remote reference has no commit")?;
    let observed = fields.next().context("Remote reference name is missing")?;
    if fields.next().is_some() || observed != reference {
        bail!("Remote reference response is ambiguous");
    }
    validate_git_oid(sha)?;
    Ok(sha.into())
}

fn validate_git_oid(value: &str) -> Result<()> {
    if value.len() != 40 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("Git commit identity is invalid");
    }
    Ok(())
}
fn validate_sha256(value: &str) -> Result<()> {
    if value.len() != 64 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("SHA-256 evidence is invalid");
    }
    Ok(())
}
fn validate_uuid_text(value: &str) -> Result<()> {
    uuid::Uuid::parse_str(value).context("Publication feature ID is invalid")?;
    Ok(())
}
fn validate_identity(value: &str, label: &str) -> Result<()> {
    if value.is_empty() || value.len() > 320 || value.chars().any(char::is_control) {
        bail!("{label} is invalid");
    }
    Ok(())
}
fn check_cancelled(cancellation: &AtomicU8) -> Result<()> {
    if cancellation.load(Ordering::SeqCst) != 0 {
        bail!("GitHub publication was cancelled");
    }
    Ok(())
}
fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn resolve_executable(override_path: Option<PathBuf>, name: &str) -> Result<PathBuf> {
    if let Some(path) = override_path {
        if !path.is_absolute() {
            bail!("{name} executable override must be absolute");
        }
        let path =
            fs::canonicalize(path).with_context(|| format!("{name} executable is unavailable"))?;
        if !path.is_file() {
            bail!("{name} executable is not a file");
        }
        return git_process_path(&path);
    }
    let path = env::var_os("PATH").context("PATH is unavailable")?;
    let windows_name = format!("{name}.exe");
    let candidates: Vec<&str> = if cfg!(windows) {
        vec![name, windows_name.as_str()]
    } else {
        vec![name]
    };
    for directory in env::split_paths(&path) {
        for candidate in &candidates {
            let path = directory.join(candidate);
            if path.is_file() {
                return git_process_path(&fs::canonicalize(path)?);
            }
        }
    }
    Err(anyhow!(
        "{name} executable was not found on the startup PATH"
    ))
}

#[cfg(windows)]
fn git_process_path(path: &Path) -> Result<PathBuf> {
    use std::path::{Component, Prefix};
    let value = path.to_str().context("Publication path is not Unicode")?;
    let normalized = if let Some(rest) = value.strip_prefix(r"\\?\UNC\") {
        let mut components = rest.split('\\');
        if components.next().is_none_or(str::is_empty)
            || components.next().is_none_or(str::is_empty)
        {
            bail!("Publication UNC path is invalid");
        }
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = value.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    };
    let permitted = matches!(normalized.components().next(), Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_) | Prefix::UNC(_, _)));
    if !normalized.is_absolute() || !permitted {
        bail!("Publication process path is invalid");
    }
    Ok(normalized)
}

#[cfg(not(windows))]
fn git_process_path(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        bail!("Publication process path must be absolute");
    }
    Ok(path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repository_and_branch_validation_reject_command_and_path_confusion() {
        for bad in [
            "git@github.com:owner/repo.git",
            "https://evil.example/owner/repo",
            "https://github.com/owner/repo/extra",
            "https://github.com/../repo",
        ] {
            assert!(canonical_repository(bad).is_err(), "{bad}");
        }
        for bad in [
            "",
            "-main",
            "../main",
            "main lock",
            "main:evil",
            "main\\evil",
            "main@{1}",
        ] {
            assert!(validate_branch(bad).is_err(), "{bad}");
        }
        assert_eq!(
            canonical_repository("https://github.com/owner/repo.git").unwrap(),
            "owner/repo"
        );
        assert!(validate_branch("release/main-1").is_ok());
    }

    #[test]
    fn candidate_requires_exact_bounded_hash_bound_files() {
        let valid = CandidateFile {
            path: "src/app.rs".into(),
            before_sha256: None,
            content_sha256: sha256(b"new"),
            content: "new".into(),
        };
        assert!(validate_candidate_files(std::slice::from_ref(&valid)).is_ok());
        let mut drifted = valid.clone();
        drifted.content.push('!');
        assert!(validate_candidate_files(&[drifted]).is_err());
        let mut git = valid.clone();
        git.path = ".git/config".into();
        assert!(validate_candidate_files(&[git]).is_err());
        assert!(validate_candidate_files(&[]).is_err());
    }

    #[test]
    fn persisted_record_rejects_unknown_effect_states() {
        let input = PublicationInput {
            feature_id: "a8e78ac7-c9a9-47f0-92dc-b35777880967".into(),
            title: "Feature".into(),
            body: "Body".into(),
            binding: ProjectBinding {
                project: "demo".into(),
                repository_url: "https://github.com/owner/repo.git".into(),
                repository_slug: "owner/repo".into(),
                base_branch: "main".into(),
                required_checks: vec![RequiredCheck {
                    context: "release-local".into(),
                    integration_id: Some(42),
                }],
                strict_required_checks: true,
            },
            files: vec![CandidateFile {
                path: "app.txt".into(),
                before_sha256: None,
                content_sha256: sha256(b"ok"),
                content: "ok".into(),
            }],
        };
        let mut record = PublicationRecord::pending(&input).unwrap();
        record.status = "mystery".into();
        assert!(record.validate().is_err());
        record.status = "pending".into();
        record.stage = "unknown_effect".into();
        assert!(record.validate().is_err());
        let mut incomplete = PublicationRecord::pending(&input).unwrap();
        incomplete.status = "succeeded".into();
        incomplete.stage = "complete".into();
        assert!(incomplete.validate().is_err());
    }

    #[test]
    fn status_parser_preserves_spaces_and_rejects_renames() {
        let parsed = parse_status_paths(b" M src/file one.rs\0?? new.txt\0").unwrap();
        assert_eq!(
            parsed,
            BTreeSet::from(["new.txt".into(), "src/file one.rs".into()])
        );
        assert!(parse_status_paths(b"R  old.txt\0new.txt\0").is_err());
    }

    #[test]
    fn required_check_policy_is_frozen_from_effective_branch_rules() {
        let (checks, rulesets) = parse_required_checks(br#"[[{"ruleset_id":7,"type":"required_status_checks","parameters":{"strict_required_status_checks_policy":true,"required_status_checks":[{"context":"windows","integration_id":42},{"context":"release-local","integration_id":42}]}}]]"#).unwrap();
        assert_eq!(
            checks,
            vec![
                RequiredCheck {
                    context: "release-local".into(),
                    integration_id: Some(42)
                },
                RequiredCheck {
                    context: "windows".into(),
                    integration_id: Some(42)
                }
            ]
        );
        assert_eq!(rulesets, BTreeSet::from([7]));
        assert!(parse_required_checks(br#"[{"ruleset_id":7,"type":"required_status_checks","parameters":{"strict_required_status_checks_policy":false,"required_status_checks":[{"context":"release-local","integration_id":42}]}}]"#).is_err());
        assert!(parse_required_checks(br#"[{"ruleset_id":7,"type":"required_status_checks","parameters":{"strict_required_status_checks_policy":true,"required_status_checks":[{"context":"release-local"}]}}]"#).is_err());
        assert!(parse_required_checks(
            br#"[{"ruleset_id":8,"type":"merge_queue","parameters":{}}]"#
        )
        .is_err());
        assert!(validate_required_checks(&parse_required_checks(b"[]").unwrap().0).is_err());
    }

    #[test]
    fn required_check_policy_rejects_ruleset_and_classic_bypasses() {
        let safe_ruleset =
            br#"{"id":7,"target":"branch","enforcement":"active","bypass_actors":[]}"#;
        assert!(validate_ruleset_detail(safe_ruleset, 7).is_ok());
        assert!(validate_ruleset_detail(
            br#"{"id":7,"target":"branch","enforcement":"evaluate","bypass_actors":[]}"#,
            7
        )
        .is_err());
        assert!(validate_ruleset_detail(br#"{"id":7,"target":"branch","enforcement":"active","bypass_actors":[{"actor_id":1}]}"#, 7).is_err());

        let classic = parse_classic_branch_protection(
            br#"{"required_status_checks":{"strict":true,"contexts":["release-local"],"checks":[{"context":"release-local","app_id":42}]},"enforce_admins":{"enabled":true},"required_pull_request_reviews":{"bypass_pull_request_allowances":{"users":[],"teams":[],"apps":[]}}}"#,
        )
        .unwrap();
        assert_eq!(
            classic,
            vec![RequiredCheck {
                context: "release-local".into(),
                integration_id: Some(42)
            }]
        );
        assert!(parse_classic_branch_protection(br#"{"required_status_checks":{"strict":false,"contexts":["release-local"],"checks":[{"context":"release-local","app_id":42}]},"enforce_admins":{"enabled":true}}"#).is_err());
        assert!(parse_classic_branch_protection(br#"{"required_status_checks":{"strict":true,"contexts":["release-local"],"checks":[{"context":"release-local","app_id":42}]},"enforce_admins":{"enabled":false}}"#).is_err());
        assert!(parse_classic_branch_protection(br#"{"required_status_checks":{"strict":true,"contexts":["release-local"],"checks":[{"context":"release-local","app_id":42}]},"enforce_admins":{"enabled":true},"required_pull_request_reviews":{"bypass_pull_request_allowances":{"apps":[{"slug":"publisher"}]}}}"#).is_err());
        assert!(parse_classic_branch_protection(br#"{"required_status_checks":{"strict":true,"contexts":["release-local"],"checks":[{"context":"release-local"}]},"enforce_admins":{"enabled":true}}"#).is_err());
        assert!(parse_classic_branch_protection(br#"{"required_status_checks":{"strict":true,"contexts":["release-local"],"checks":[{"context":"release-local","app_id":42}]},"enforce_admins":{"enabled":true},"required_pull_request_reviews":"ambiguous"}"#).is_err());
    }

    #[test]
    fn pull_request_identity_is_exact_and_repository_bound() {
        let input = PublicationInput {
            feature_id: "a8e78ac7-c9a9-47f0-92dc-b35777880967".into(),
            title: "Feature".into(),
            body: "Body".into(),
            binding: ProjectBinding {
                project: "demo".into(),
                repository_url: "https://github.com/owner/repo.git".into(),
                repository_slug: "owner/repo".into(),
                base_branch: "main".into(),
                required_checks: vec![RequiredCheck {
                    context: "release-local".into(),
                    integration_id: Some(42),
                }],
                strict_required_checks: true,
            },
            files: vec![CandidateFile {
                path: "app.txt".into(),
                before_sha256: None,
                content_sha256: sha256(b"ok"),
                content: "ok".into(),
            }],
        };
        let mut record = PublicationRecord::pending(&input).unwrap();
        let commit = "a".repeat(40);
        record.base_sha = Some("b".repeat(40));
        let exact = serde_json::json!({"number":17,"url":"https://github.com/owner/repo/pull/17","headRefOid":commit,"baseRefName":"main","baseRefOid":"b".repeat(40),"state":"OPEN","mergeCommit":null});
        bind_pr(&mut record, &exact, &"a".repeat(40)).unwrap();
        let mut spoofed = exact.clone();
        spoofed["url"] = serde_json::json!("https://github.com/owner/repo/pull/17/files");
        assert!(bind_pr(&mut record, &spoofed, &"a".repeat(40)).is_err());
        let mut moved_base = exact;
        moved_base["baseRefOid"] = serde_json::json!("c".repeat(40));
        assert!(bind_pr(&mut record, &moved_base, &"a".repeat(40)).is_err());
    }

    #[test]
    fn setup_account_and_device_challenge_are_exact() {
        let status = br#"{"hosts":{"github.com":[{"state":"success","active":true,"host":"github.com","login":"owner"}]}}"#;
        assert_eq!(
            parse_account_status(status).unwrap().as_deref(),
            Some("owner")
        );
        assert!(parse_account_status(br#"{"hosts":{"github.com":[{"state":"success","active":true,"host":"github.com","login":"one"},{"state":"success","active":true,"host":"github.com","login":"two"}]}}"#).is_err());

        let challenge = parse_device_challenge(
            b"First copy ABCD-9XYZ, then open https://github.com/login/device",
        )
        .unwrap()
        .unwrap();
        assert_eq!(challenge.user_code, "ABCD-9XYZ");
        assert_eq!(challenge.verification_url, DEVICE_URL);
        assert!(
            parse_device_challenge(b"ABCD-9XYZ EFGH-1234 https://github.com/login/device").is_err()
        );
        assert!(
            parse_device_challenge(b"ABCD-9XYZ https://example.com/device")
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn repository_discovery_accepts_internal_and_uses_exact_branch_truth() {
        let bytes = br#"[
          {"id":1,"full_name":"owner/empty","html_url":"https://github.com/owner/empty","visibility":"private","default_branch":"main","permissions":{"push":true}},
          {"id":2,"full_name":"org/internal","html_url":"https://github.com/org/internal","visibility":"internal","default_branch":"trunk","permissions":{"push":false}}
        ]"#;
        let mut repositories = parse_repository_page(bytes).unwrap();
        assert_eq!(repositories[1].visibility, "internal");
        apply_repository_branch_evidence(
            &mut repositories,
            &serde_json::json!({"data":{
                "r0":{"nameWithOwner":"owner/empty","isEmpty":true,"defaultBranchRef":null},
                "r1":{"nameWithOwner":"org/internal","isEmpty":false,"defaultBranchRef":{"name":"trunk"}}
            }}),
        )
        .unwrap();
        assert_eq!(repositories[0].default_branch, "");
        assert_eq!(repositories[1].default_branch, "trunk");

        let mut contradictory = parse_repository_page(bytes).unwrap();
        assert!(apply_repository_branch_evidence(
            &mut contradictory,
            &serde_json::json!({"data":{
                "r0":{"nameWithOwner":"owner/empty","isEmpty":false,"defaultBranchRef":null},
                "r1":{"nameWithOwner":"org/internal","isEmpty":false,"defaultBranchRef":{"name":"trunk"}}
            }}),
        )
        .is_err());
    }

    #[test]
    fn redirected_repository_observation_is_rejected() {
        let repository = parse_repository(
            br#"{"id":42,"full_name":"other/transferred","html_url":"https://github.com/other/transferred","visibility":"private","default_branch":"main","permissions":{"push":true}}"#,
        )
        .unwrap();
        assert!(ensure_repository_identity(&repository, "owner/target").is_err());
        assert!(ensure_repository_identity(&repository, "OTHER/TRANSFERRED").is_err());
        assert!(ensure_repository_identity(&repository, "other/transferred").is_ok());
    }
}
